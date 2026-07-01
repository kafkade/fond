import SwiftUI
import Combine
#if os(iOS)
import UIKit
#endif

/// A single running/paused/finished countdown, tied to a cook-mode step.
struct KitchenTimer: Identifiable {
    enum State { case running, paused, finished }

    let id = UUID()
    let label: String
    let total: Int
    var state: State
    /// Wall-clock fire time; authoritative while `running`.
    var deadline: Date
    /// Seconds remaining; authoritative while `paused`/`finished`.
    var remaining: Int

    /// Seconds to show right now, derived from wall-clock time while running so
    /// countdowns stay accurate even if ticks coalesce.
    var displaySeconds: Int {
        switch state {
        case .running: return max(0, Int(deadline.timeIntervalSinceNow.rounded(.up)))
        case .paused, .finished: return remaining
        }
    }

    var progress: Double {
        guard total > 0 else { return 0 }
        return 1 - Double(displaySeconds) / Double(total)
    }
}

/// Drives every active kitchen timer from one shared 1-second ticker. Owns
/// start/pause/resume/cancel and fires a haptic + flips to `.finished` when a
/// countdown reaches zero.
@MainActor
final class KitchenTimerModel: ObservableObject {
    @Published private(set) var timers: [KitchenTimer] = []
    private var cancellable: AnyCancellable?

    var hasTimers: Bool { !timers.isEmpty }

    func start(label: String, seconds: Int) {
        guard seconds > 0 else { return }
        timers.append(
            KitchenTimer(
                label: label,
                total: seconds,
                state: .running,
                deadline: Date().addingTimeInterval(Double(seconds)),
                remaining: seconds
            )
        )
        ensureTicking()
    }

    func pause(_ id: UUID) {
        guard let i = index(id), timers[i].state == .running else { return }
        timers[i].remaining = timers[i].displaySeconds
        timers[i].state = .paused
        stopIfIdle()
    }

    func resume(_ id: UUID) {
        guard let i = index(id), timers[i].state == .paused else { return }
        timers[i].deadline = Date().addingTimeInterval(Double(timers[i].remaining))
        timers[i].state = .running
        ensureTicking()
    }

    func addMinute(_ id: UUID) {
        guard let i = index(id) else { return }
        switch timers[i].state {
        case .running:
            timers[i].deadline.addTimeInterval(60)
        case .paused:
            timers[i].remaining += 60
        case .finished:
            timers[i].remaining = 60
            timers[i].deadline = Date().addingTimeInterval(60)
            timers[i].state = .running
            ensureTicking()
        }
    }

    func cancel(_ id: UUID) {
        timers.removeAll { $0.id == id }
        stopIfIdle()
    }

    func cancelAll() {
        timers.removeAll()
        stopIfIdle()
    }

    // MARK: - Ticking

    private func index(_ id: UUID) -> Int? { timers.firstIndex { $0.id == id } }

    private var hasRunning: Bool { timers.contains { $0.state == .running } }

    private func ensureTicking() {
        guard cancellable == nil else { return }
        cancellable = Timer.publish(every: 1, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in self?.tick() }
    }

    private func stopIfIdle() {
        if !hasRunning {
            cancellable?.cancel()
            cancellable = nil
        }
    }

    private func tick() {
        var justFinished = false
        for i in timers.indices where timers[i].state == .running {
            if timers[i].displaySeconds <= 0 {
                timers[i].state = .finished
                timers[i].remaining = 0
                justFinished = true
            }
        }
        if justFinished { KitchenHaptics.done() }
        stopIfIdle()
        // Running countdowns are derived from `deadline`, so nudge observers to
        // re-read even on ticks where the stored array is otherwise unchanged.
        objectWillChange.send()
    }

    static func clock(_ seconds: Int) -> String {
        let s = max(0, seconds)
        let h = s / 3600, m = (s % 3600) / 60, sec = s % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, sec)
            : String(format: "%d:%02d", m, sec)
    }
}

enum KitchenHaptics {
    static func done() {
        #if os(iOS)
        UINotificationFeedbackGenerator().notificationOccurred(.success)
        #endif
    }
}

/// A single timer card: label, live remaining time, progress ring, and controls.
struct KitchenTimerView: View {
    let timer: KitchenTimer
    let onPause: () -> Void
    let onResume: () -> Void
    let onAddMinute: () -> Void
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                Circle().stroke(.quaternary, lineWidth: 4)
                Circle()
                    .trim(from: 0, to: timer.progress)
                    .stroke(ringColor, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Image(systemName: timer.state == .finished ? "bell.fill" : "timer")
                    .font(.caption)
                    .foregroundStyle(ringColor)
            }
            .frame(width: 40, height: 40)

            VStack(alignment: .leading, spacing: 2) {
                Text(timer.label)
                    .font(.subheadline)
                    .lineLimit(1)
                Text(timer.state == .finished ? "Done" : KitchenTimerModel.clock(timer.displaySeconds))
                    .font(.system(.title3, design: .monospaced))
                    .foregroundStyle(timer.state == .finished ? Color.green : .primary)
            }

            Spacer()

            controls
        }
        .padding(10)
        .background(timer.state == .finished ? Color.green.opacity(0.12) : Color.secondarySystemBackgroundCompat,
                    in: RoundedRectangle(cornerRadius: 12))
    }

    @ViewBuilder
    private var controls: some View {
        switch timer.state {
        case .running:
            iconButton("pause.fill", action: onPause)
            iconButton("goforward.60", action: onAddMinute)
            iconButton("xmark", role: .destructive, action: onCancel)
        case .paused:
            iconButton("play.fill", action: onResume)
            iconButton("goforward.60", action: onAddMinute)
            iconButton("xmark", role: .destructive, action: onCancel)
        case .finished:
            iconButton("arrow.counterclockwise", action: onAddMinute)
            iconButton("checkmark", role: .destructive, action: onCancel)
        }
    }

    private func iconButton(_ symbol: String, role: ButtonRole? = nil, action: @escaping () -> Void) -> some View {
        Button(role: role, action: action) {
            Image(systemName: symbol)
                .frame(width: 28, height: 28)
        }
        .buttonStyle(.bordered)
        .buttonBorderShape(.circle)
    }

    private var ringColor: Color {
        switch timer.state {
        case .running: return .orange
        case .paused: return .secondary
        case .finished: return .green
        }
    }
}

// `secondarySystemBackground` is iOS-only; provide a cross-platform stand-in so
// the shared multiplatform target keeps compiling on macOS.
extension ShapeStyle where Self == Color {
    static var secondarySystemBackgroundCompat: Color {
        #if os(iOS)
        Color(uiColor: .secondarySystemBackground)
        #else
        Color(nsColor: .underPageBackgroundColor)
        #endif
    }
}
