import Combine
import Foundation
import FondKit

/// Anything that can relay a cook-session snapshot to the Watch (and widget).
/// Implemented by `PhoneSessionRelay`; kept as a protocol so the session model
/// has no WatchConnectivity dependency and stays unit-reasonable.
@MainActor
protocol CookSessionBroadcaster: AnyObject {
    func broadcast(_ payload: CookSessionPayload)
}

/// The phone's single source of truth for an in-progress cook session.
///
/// It couples the backward-scheduled timeline (`ScheduledTimelineDto` from the
/// Rust core) with the live `KitchenTimerModel`, lowers both into a plain
/// `CookSessionPayload`, and pushes that to the Watch via a `broadcaster` on
/// every meaningful change. Wrist controls arrive back through `handleControl`
/// and mutate this same authoritative state, which then re-broadcasts.
@MainActor
final class CookSessionModel: ObservableObject {
    @Published private(set) var schedule: ScheduledTimelineDto?
    @Published private(set) var isActive = false

    /// The live countdown engine, shared with the on-phone cook-mode UI.
    let timers = KitchenTimerModel()

    weak var broadcaster: CookSessionBroadcaster?

    private var revision = 0
    private var sessionId = UUID().uuidString
    private var cancellables = Set<AnyCancellable>()

    init() {
        // Any discrete timer change (start/pause/resume/+1/cancel/finish) relays.
        timers.onChange = { [weak self] in self?.broadcast() }
        // Re-emit the timers' per-second UI ticks through this model so views
        // observing the session (env object) still refresh live countdowns.
        timers.objectWillChange
            .sink { [weak self] in self?.objectWillChange.send() }
            .store(in: &cancellables)
    }

    // MARK: - Lifecycle

    /// Adopt a freshly (re)scheduled timeline. Switching recipes starts a new
    /// session identity and clears stale timers; rescheduling the same recipe
    /// (e.g. a new serve time) keeps running timers intact.
    func activate(schedule: ScheduledTimelineDto) {
        if self.schedule?.recipeSlug != schedule.recipeSlug {
            sessionId = UUID().uuidString
            timers.cancelAll()
        }
        self.schedule = schedule
        isActive = true
        broadcast()
    }

    /// End the session: stop timers and clear the relayed state.
    func end() {
        timers.cancelAll()
        isActive = false
        schedule = nil
        broadcast()
    }

    // MARK: - Timer operations (phone UI + relay share these)

    func startTimer(stepId: UInt64, label: String, seconds: Int) {
        guard seconds > 0, !timers.hasActiveTimer(forStep: stepId) else { return }
        timers.start(label: label, seconds: seconds, stepId: stepId)
    }

    /// Start the next timed step that doesn't yet have a running timer.
    func advance() {
        guard let schedule else { return }
        for scheduled in schedule.nodes {
            let node = scheduled.node
            guard let seconds = node.duration?.seconds, seconds > 0 else { continue }
            if !timers.hasActiveTimer(forStep: node.id) {
                startTimer(stepId: node.id, label: node.label, seconds: Int(seconds))
                return
            }
        }
    }

    // MARK: - Control from the wrist

    func handleControl(_ message: CookControlMessage) {
        switch message.action {
        case .startStepTimer:
            if let stepId = message.stepId,
               let scheduled = step(stepId),
               let seconds = scheduled.node.duration?.seconds {
                startTimer(stepId: stepId, label: scheduled.node.label, seconds: Int(seconds))
            }
        case .pauseTimer:
            if let id = uuid(message.timerId) { timers.pause(id) }
        case .resumeTimer:
            if let id = uuid(message.timerId) { timers.resume(id) }
        case .addMinute:
            if let id = uuid(message.timerId) { timers.addMinute(id) }
        case .cancelTimer:
            if let id = uuid(message.timerId) { timers.cancel(id) }
        case .advanceStep:
            advance()
        case .endSession:
            end()
        case .requestSync:
            broadcast()
        }
    }

    // MARK: - Broadcast

    func broadcast() {
        broadcaster?.broadcast(makePayload())
    }

    /// Lower the current schedule + live timers into the relayed payload.
    func makePayload() -> CookSessionPayload {
        revision += 1
        guard let schedule, isActive else { return .idle() }

        let steps = schedule.nodes.map { scheduled -> CookStepPayload in
            let node = scheduled.node
            return CookStepPayload(
                id: node.id,
                stepIndex: node.stepIndex,
                label: node.label,
                taskType: node.taskType.shared,
                scheduledStart: scheduled.scheduledStart,
                scheduledEnd: scheduled.scheduledEnd,
                durationSeconds: node.duration?.seconds
            )
        }

        let relayTimers = timers.timers.map { timer -> CookTimerPayload in
            CookTimerPayload(
                id: timer.id.uuidString,
                stepId: timer.stepId,
                label: timer.label,
                total: timer.total,
                state: timer.state.shared,
                deadline: timer.deadline,
                remaining: timer.displaySeconds
            )
        }

        return CookSessionPayload(
            id: sessionId,
            recipeSlug: schedule.recipeSlug,
            recipeTitle: schedule.recipeTitle,
            serveAt: schedule.serveAt,
            startAt: schedule.startAt,
            isActive: true,
            steps: steps,
            timers: relayTimers,
            revision: revision,
            updatedAt: Date()
        )
    }

    // MARK: - Helpers

    private func step(_ id: UInt64) -> ScheduledNodeDto? {
        schedule?.nodes.first { $0.node.id == id }
    }

    private func uuid(_ string: String?) -> UUID? {
        string.flatMap(UUID.init(uuidString:))
    }
}

// MARK: - FFI → shared payload mappings

extension TaskTypeDto {
    var shared: CookTaskType {
        switch self {
        case .activePrep: return .activePrep
        case .passivePrep: return .passivePrep
        case .activeCook: return .activeCook
        case .passiveCook: return .passiveCook
        case .rest: return .rest
        }
    }
}

extension KitchenTimer.State {
    var shared: CookTimerState {
        switch self {
        case .running: return .running
        case .paused: return .paused
        case .finished: return .finished
        }
    }
}
