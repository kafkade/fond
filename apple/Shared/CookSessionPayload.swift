import Foundation

// Plain, dependency-free value types shared verbatim by the iOS app, the
// watchOS app, and the WidgetKit extension. They are the *relayed timeline
// payload* referenced by ADR-011/issue #75: the phone owns `FondClient` and the
// timeline, lowers a `ScheduledTimelineDto` into these Codable structs, and
// ships them to the Watch over WatchConnectivity. Nothing here depends on
// FondKit / the Rust core, so the Watch never links the xcframework.

// MARK: - Link constants

/// Shared identifiers for the phone ⇄ watch ⇄ widget plumbing.
public enum FondWatchLink {
    /// App Group container shared by the app targets and the widget extension.
    /// Declared in each target's entitlements; used to hand the widget the
    /// latest session snapshot.
    public static let appGroup = "group.dev.kafkade.fond"

    /// Key under which the encoded `CookSessionPayload` travels in a WCSession
    /// application context.
    public static let sessionContextKey = "cookSession"

    /// Key under which an encoded `CookControlMessage` travels in a WCSession
    /// message / user-info transfer (wrist → phone).
    public static let controlMessageKey = "cookControl"

    /// `UserDefaults`(suiteName:) key holding the latest snapshot for the widget.
    public static let snapshotDefaultsKey = "cookSessionSnapshot"

    /// Sentinel meaning "no active cook session" when written to the App Group.
    public static let clearedSnapshot = "cleared"
}

// MARK: - Timer

/// Lifecycle of a single relayed countdown, mirroring the phone's `KitchenTimer`.
public enum CookTimerState: String, Codable, Hashable, Sendable {
    case running, paused, finished
}

/// One running/paused/finished countdown, relayed by absolute `deadline` (while
/// running) so both wrist and phone can derive the remaining time locally and
/// stay in sync even if ticks coalesce.
public struct CookTimerPayload: Codable, Hashable, Identifiable, Sendable {
    public var id: String
    public var stepId: UInt64?
    public var label: String
    public var total: Int
    public var state: CookTimerState
    /// Wall-clock fire time; authoritative while `running`.
    public var deadline: Date
    /// Seconds remaining; authoritative while `paused`/`finished`.
    public var remaining: Int

    public init(
        id: String,
        stepId: UInt64?,
        label: String,
        total: Int,
        state: CookTimerState,
        deadline: Date,
        remaining: Int
    ) {
        self.id = id
        self.stepId = stepId
        self.label = label
        self.total = total
        self.state = state
        self.deadline = deadline
        self.remaining = remaining
    }

    /// Seconds to show right now, derived from wall-clock time while running.
    public var displaySeconds: Int {
        switch state {
        case .running: return max(0, Int(deadline.timeIntervalSinceNow.rounded(.up)))
        case .paused, .finished: return remaining
        }
    }

    public var progress: Double {
        guard total > 0 else { return 0 }
        return 1 - Double(displaySeconds) / Double(total)
    }
}

// MARK: - Steps

/// Active/passive prep/cook classification, kept as a `String` raw value so the
/// payload has no dependency on the FFI enum.
public enum CookTaskType: String, Codable, Hashable, Sendable {
    case activePrep, passivePrep, activeCook, passiveCook, rest

    public var isActive: Bool {
        switch self {
        case .activePrep, .activeCook: return true
        case .passivePrep, .passiveCook, .rest: return false
        }
    }

    public var label: String {
        switch self {
        case .activePrep: return "Active prep"
        case .passivePrep: return "Passive prep"
        case .activeCook: return "Active cook"
        case .passiveCook: return "Passive cook"
        case .rest: return "Rest"
        }
    }
}

/// A scheduled timeline node, flattened for relay. `durationSeconds == nil`
/// marks an untimed step (never fabricated — ADR-008).
public struct CookStepPayload: Codable, Hashable, Identifiable, Sendable {
    public var id: UInt64
    public var stepIndex: UInt32
    public var label: String
    public var taskType: CookTaskType
    public var scheduledStart: String
    public var scheduledEnd: String
    public var durationSeconds: UInt64?

    public init(
        id: UInt64,
        stepIndex: UInt32,
        label: String,
        taskType: CookTaskType,
        scheduledStart: String,
        scheduledEnd: String,
        durationSeconds: UInt64?
    ) {
        self.id = id
        self.stepIndex = stepIndex
        self.label = label
        self.taskType = taskType
        self.scheduledStart = scheduledStart
        self.scheduledEnd = scheduledEnd
        self.durationSeconds = durationSeconds
    }

    public var isTimed: Bool { (durationSeconds ?? 0) > 0 }
}

// MARK: - Session

/// The full relayed cook session: recipe meta, the backward-scheduled steps, and
/// the live timers. `isActive == false` means no cook is in progress (the Watch
/// shows an idle state and the widget clears).
public struct CookSessionPayload: Codable, Hashable, Identifiable, Sendable {
    public var id: String
    public var recipeSlug: String
    public var recipeTitle: String
    public var serveAt: String
    public var startAt: String
    public var isActive: Bool
    public var steps: [CookStepPayload]
    public var timers: [CookTimerPayload]
    /// Monotonic counter so stale relays can be ignored if they arrive out of
    /// order.
    public var revision: Int
    public var updatedAt: Date

    public init(
        id: String,
        recipeSlug: String,
        recipeTitle: String,
        serveAt: String,
        startAt: String,
        isActive: Bool,
        steps: [CookStepPayload],
        timers: [CookTimerPayload],
        revision: Int,
        updatedAt: Date
    ) {
        self.id = id
        self.recipeSlug = recipeSlug
        self.recipeTitle = recipeTitle
        self.serveAt = serveAt
        self.startAt = startAt
        self.isActive = isActive
        self.steps = steps
        self.timers = timers
        self.revision = revision
        self.updatedAt = updatedAt
    }

    /// An empty, inactive session — the "nothing cooking" state.
    public static func idle() -> CookSessionPayload {
        CookSessionPayload(
            id: "idle",
            recipeSlug: "",
            recipeTitle: "",
            serveAt: "",
            startAt: "",
            isActive: false,
            steps: [],
            timers: [],
            revision: 0,
            updatedAt: .distantPast
        )
    }

    /// Running timers only, soonest deadline first — the wrist's active list.
    public var activeTimers: [CookTimerPayload] {
        timers
            .filter { $0.state != .finished }
            .sorted { $0.deadline < $1.deadline }
    }

    /// Whether a live (non-finished) timer already exists for a step.
    public func hasActiveTimer(forStep stepId: UInt64) -> Bool {
        timers.contains { $0.stepId == stepId && $0.state != .finished }
    }

    /// Timed steps that don't yet have a running timer — startable from the wrist.
    public var startableSteps: [CookStepPayload] {
        steps.filter { $0.isTimed && !hasActiveTimer(forStep: $0.id) }
    }

    /// The imminent step for the "Next up" complication: the earliest timed step
    /// whose scheduled start is now or in the future, falling back to the first
    /// timed step. Untimed steps are skipped.
    public func nextUpStep(now: Date = Date()) -> CookStepPayload? {
        let timed = steps.filter { $0.isTimed }
        let upcoming = timed
            .compactMap { step -> (CookStepPayload, Date)? in
                guard let date = FondTime.parse(step.scheduledStart) else { return nil }
                return (step, date)
            }
            .sorted { $0.1 < $1.1 }
        if let next = upcoming.first(where: { $0.1 >= now }) {
            return next.0
        }
        return upcoming.last?.0 ?? timed.first
    }
}

// MARK: - Control messages (wrist → phone)

/// A control action initiated from the wrist and applied by the phone (the
/// authoritative session owner), which then re-broadcasts the updated session.
public struct CookControlMessage: Codable, Hashable, Sendable {
    public enum Action: String, Codable, Sendable {
        case startStepTimer
        case pauseTimer
        case resumeTimer
        case addMinute
        case cancelTimer
        case advanceStep
        case endSession
        case requestSync
    }

    public var action: Action
    public var timerId: String?
    public var stepId: UInt64?

    public init(action: Action, timerId: String? = nil, stepId: UInt64? = nil) {
        self.action = action
        self.timerId = timerId
        self.stepId = stepId
    }
}

// MARK: - Codec helpers

/// Shared JSON codec so both ends agree on `Date` encoding. Defaults (seconds
/// since reference date) are fine because both sides use this same coder.
public enum FondCodec {
    public static let encoder = JSONEncoder()
    public static let decoder = JSONDecoder()

    public static func encode<T: Encodable>(_ value: T) -> Data? {
        try? encoder.encode(value)
    }

    public static func decode<T: Decodable>(_ type: T.Type, from data: Data) -> T? {
        try? decoder.decode(type, from: data)
    }
}

/// ISO 8601 *local* (timezone-free) datetime parsing/formatting matching the
/// format the FFI emits (`yyyy-MM-dd'T'HH:mm:ss`).
public enum FondTime {
    public static let iso: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd'T'HH:mm:ss"
        return f
    }()

    public static func parse(_ s: String) -> Date? {
        iso.date(from: s)
    }

    public static func string(_ date: Date) -> String {
        iso.string(from: date)
    }

    /// Short "HH:mm" label for a relayed ISO string.
    public static func clockLabel(_ iso: String) -> String {
        guard let date = parse(iso) else { return iso }
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "HH:mm"
        return f.string(from: date)
    }

    /// A running-clock label ("1:05", "12:30", "1:02:00") for a seconds value.
    public static func clock(_ seconds: Int) -> String {
        let s = max(0, seconds)
        let h = s / 3600, m = (s % 3600) / 60, sec = s % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, sec)
            : String(format: "%d:%02d", m, sec)
    }
}
