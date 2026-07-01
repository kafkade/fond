import SwiftUI

/// Root wrist UI: an idle prompt when nothing is cooking, otherwise a scrolling
/// list with the imminent step, live active timers, startable steps, and session
/// controls. Countdowns refresh from `WatchSessionModel`'s 1-second ticker.
struct WatchRootView: View {
    @EnvironmentObject private var model: WatchSessionModel

    var body: some View {
        NavigationStack {
            Group {
                if model.session.isActive {
                    activeList
                } else {
                    IdleView()
                }
            }
            .navigationTitle("Fond")
        }
    }

    private var activeList: some View {
        List {
            Section {
                Text(model.session.recipeTitle)
                    .font(.headline)
                    .lineLimit(2)
                Label("Serve \(FondTime.clockLabel(model.session.serveAt))", systemImage: "fork.knife")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            if let next = model.session.nextUpStep() {
                Section("Next up") {
                    NextUpRow(step: next)
                }
            }

            Section("Timers") {
                if model.session.activeTimers.isEmpty {
                    Text("No running timers. Start one from a step.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(model.session.activeTimers) { timer in
                        WatchTimerRow(timer: timer)
                    }
                }
            }

            if !model.session.startableSteps.isEmpty {
                Section("Steps") {
                    ForEach(model.session.startableSteps) { step in
                        StartStepRow(step: step)
                    }
                }
            }

            Section {
                Button {
                    model.advance()
                } label: {
                    Label("Start next timer", systemImage: "forward.fill")
                }
                Button(role: .destructive) {
                    model.endSession()
                } label: {
                    Label("End cook", systemImage: "stop.fill")
                }
            }
        }
    }
}

/// Shown when the phone has no active cook session.
struct IdleView: View {
    @EnvironmentObject private var model: WatchSessionModel

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: "timer")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            Text("Nothing cooking")
                .font(.headline)
            Text("Start cook mode on your iPhone to see timers here.")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Button {
                model.requestSync()
            } label: {
                Label("Refresh", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
        }
        .padding()
    }
}

/// The imminent scheduled step, with its start time and a one-tap start.
struct NextUpRow: View {
    let step: CookStepPayload
    @EnvironmentObject private var model: WatchSessionModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(step.label)
                .font(.subheadline)
                .lineLimit(2)
            HStack(spacing: 8) {
                Label(FondTime.clockLabel(step.scheduledStart), systemImage: "clock")
                if let seconds = step.durationSeconds {
                    Label(FondTime.clock(Int(seconds)), systemImage: "timer")
                }
            }
            .font(.caption2)
            .foregroundStyle(.secondary)

            if model.session.hasActiveTimer(forStep: step.id) {
                Label("Timer running", systemImage: "checkmark.circle")
                    .font(.caption2)
                    .foregroundStyle(.green)
            } else if step.isTimed {
                Button {
                    model.startTimer(for: step)
                } label: {
                    Label("Start timer", systemImage: "play.fill")
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            }
        }
        .padding(.vertical, 2)
    }
}

/// A live countdown row with wrist controls (swipe: pause/resume, +1, cancel).
struct WatchTimerRow: View {
    let timer: CookTimerPayload
    @EnvironmentObject private var model: WatchSessionModel

    private var isDone: Bool { timer.displaySeconds <= 0 }

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle().stroke(.quaternary, lineWidth: 3)
                Circle()
                    .trim(from: 0, to: timer.progress)
                    .stroke(color, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Image(systemName: isDone ? "bell.fill" : (timer.state == .paused ? "pause.fill" : "timer"))
                    .font(.caption2)
                    .foregroundStyle(color)
            }
            .frame(width: 30, height: 30)

            VStack(alignment: .leading, spacing: 1) {
                Text(timer.label)
                    .font(.caption2)
                    .lineLimit(1)
                Text(isDone ? "Done" : FondTime.clock(timer.displaySeconds))
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(isDone ? .green : .primary)
            }
            Spacer()
        }
        .swipeActions(edge: .leading) {
            if timer.state == .running {
                Button {
                    model.pause(timer)
                } label: {
                    Label("Pause", systemImage: "pause.fill")
                }
            } else if timer.state == .paused {
                Button {
                    model.resume(timer)
                } label: {
                    Label("Resume", systemImage: "play.fill")
                }
                .tint(.green)
            }
            Button {
                model.addMinute(timer)
            } label: {
                Label("+1 min", systemImage: "goforward.60")
            }
            .tint(.orange)
        }
        .swipeActions(edge: .trailing) {
            Button(role: .destructive) {
                model.cancel(timer)
            } label: {
                Label("Cancel", systemImage: "xmark")
            }
        }
    }

    private var color: Color {
        if isDone { return .green }
        return timer.state == .paused ? .secondary : .orange
    }
}

/// A timed step with a one-tap "start timer" button.
struct StartStepRow: View {
    let step: CookStepPayload
    @EnvironmentObject private var model: WatchSessionModel

    var body: some View {
        Button {
            model.startTimer(for: step)
        } label: {
            HStack {
                VStack(alignment: .leading, spacing: 1) {
                    Text(step.label)
                        .font(.caption2)
                        .lineLimit(1)
                        .foregroundStyle(.primary)
                    if let seconds = step.durationSeconds {
                        Text(FondTime.clock(Int(seconds)))
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
                Image(systemName: "play.circle")
                    .foregroundStyle(.orange)
            }
        }
    }
}
