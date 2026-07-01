import Combine
import Foundation
import UserNotifications
import WatchConnectivity
import WatchKit
#if canImport(WidgetKit)
import WidgetKit
#endif

/// Watch side of the phone ⇄ watch relay.
///
/// Receives the authoritative `CookSessionPayload` from the phone over the
/// WCSession application context, keeps live countdowns ticking locally from the
/// relayed absolute deadlines, fires an in-app haptic the instant a timer
/// reaches zero (with a pre-scheduled local notification covering the
/// backgrounded case), mirrors the snapshot into the App Group for the "Next up"
/// widget, and forwards wrist controls back to the phone.
@MainActor
final class WatchSessionModel: NSObject, ObservableObject {
    @Published private(set) var session: CookSessionPayload = .idle()

    private var ticker: AnyCancellable?
    private var firedTimerIDs: Set<String> = []
    private let notifications = WatchNotifications()

    override init() {
        super.init()
        notifications.requestAuthorization()
        restoreSnapshot()
        activate()
    }

    // MARK: - Activation

    private func activate() {
        guard WCSession.isSupported() else { return }
        let wc = WCSession.default
        wc.delegate = self
        wc.activate()
    }

    /// Cold-launch: adopt whatever snapshot the App Group already holds so the
    /// UI isn't empty before the first relay arrives.
    private func restoreSnapshot() {
        guard let defaults = UserDefaults(suiteName: FondWatchLink.appGroup),
              let data = defaults.data(forKey: FondWatchLink.snapshotDefaultsKey),
              let restored = FondCodec.decode(CookSessionPayload.self, from: data)
        else { return }
        session = restored
        manageTicker()
    }

    // MARK: - Applying a relayed session

    func apply(_ incoming: CookSessionPayload) {
        // Drop stale relays that arrive out of order for the same session.
        if incoming.id == session.id, incoming.revision < session.revision { return }
        session = incoming
        firedTimerIDs.formIntersection(Set(incoming.timers.map(\.id)))
        persistSnapshot(incoming)
        notifications.reschedule(for: incoming)
        reloadWidget()
        manageTicker()
    }

    private func persistSnapshot(_ payload: CookSessionPayload) {
        guard let defaults = UserDefaults(suiteName: FondWatchLink.appGroup),
              let data = FondCodec.encode(payload) else { return }
        defaults.set(data, forKey: FondWatchLink.snapshotDefaultsKey)
    }

    private func reloadWidget() {
        #if canImport(WidgetKit)
        WidgetCenter.shared.reloadAllTimelines()
        #endif
    }

    // MARK: - Controls → phone

    func send(_ message: CookControlMessage) {
        guard WCSession.isSupported() else { return }
        guard let data = FondCodec.encode(message) else { return }
        let wc = WCSession.default
        let payload = [FondWatchLink.controlMessageKey: data]
        if wc.isReachable {
            wc.sendMessage(payload, replyHandler: nil) { _ in
                // Fall back to a guaranteed transfer if the live message fails.
                wc.transferUserInfo(payload)
            }
        } else {
            wc.transferUserInfo(payload)
        }
    }

    func startTimer(for step: CookStepPayload) { send(.init(action: .startStepTimer, stepId: step.id)) }
    func pause(_ timer: CookTimerPayload) { send(.init(action: .pauseTimer, timerId: timer.id)) }
    func resume(_ timer: CookTimerPayload) { send(.init(action: .resumeTimer, timerId: timer.id)) }
    func addMinute(_ timer: CookTimerPayload) { send(.init(action: .addMinute, timerId: timer.id)) }
    func cancel(_ timer: CookTimerPayload) { send(.init(action: .cancelTimer, timerId: timer.id)) }
    func advance() { send(.init(action: .advanceStep)) }
    func endSession() { send(.init(action: .endSession)) }
    func requestSync() { send(.init(action: .requestSync)) }

    // MARK: - Ticker + haptics

    private var needsTicking: Bool {
        session.timers.contains { $0.state == .running && $0.displaySeconds > 0 }
    }

    private func manageTicker() {
        if needsTicking, ticker == nil {
            ticker = Timer.publish(every: 1, on: .main, in: .common)
                .autoconnect()
                .sink { [weak self] _ in self?.onTick() }
        } else if !needsTicking {
            ticker?.cancel()
            ticker = nil
        }
    }

    private func onTick() {
        for timer in session.timers where timer.state == .running {
            if timer.displaySeconds <= 0, !firedTimerIDs.contains(timer.id) {
                firedTimerIDs.insert(timer.id)
                WKInterfaceDevice.current().play(.notification)
            }
        }
        objectWillChange.send()
        manageTicker()
    }
}

// MARK: - WCSessionDelegate

extension WatchSessionModel: WCSessionDelegate {
    nonisolated func session(
        _ session: WCSession,
        activationDidCompleteWith activationState: WCSessionActivationState,
        error: Error?
    ) {
        // Ask the phone to re-broadcast so a freshly launched Watch is current.
        Task { @MainActor in self.requestSync() }
    }

    nonisolated func session(_ session: WCSession, didReceiveApplicationContext context: [String: Any]) {
        guard let data = context[FondWatchLink.sessionContextKey] as? Data,
              let payload = FondCodec.decode(CookSessionPayload.self, from: data) else { return }
        Task { @MainActor in self.apply(payload) }
    }

    nonisolated func session(_ session: WCSession, didReceiveMessage message: [String: Any]) {
        guard let data = message[FondWatchLink.sessionContextKey] as? Data,
              let payload = FondCodec.decode(CookSessionPayload.self, from: data) else { return }
        Task { @MainActor in self.apply(payload) }
    }
}
