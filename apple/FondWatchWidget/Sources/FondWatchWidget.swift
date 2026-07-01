import SwiftUI
import WidgetKit

// "Next up" complication / Smart Stack widget for watchOS. It reads the latest
// cook-session snapshot the watch app mirrors into the shared App Group and
// shows the imminent step with a live, OS-driven countdown — no polling.

// MARK: - Snapshot access

enum NextUpSnapshot {
    static func load() -> CookSessionPayload {
        guard let defaults = UserDefaults(suiteName: FondWatchLink.appGroup),
              let data = defaults.data(forKey: FondWatchLink.snapshotDefaultsKey),
              let session = FondCodec.decode(CookSessionPayload.self, from: data)
        else { return .idle() }
        return session
    }
}

// MARK: - Timeline

struct NextUpEntry: TimelineEntry {
    let date: Date
    let session: CookSessionPayload

    /// The soonest running timer, if any — drives a live countdown.
    var runningTimer: CookTimerPayload? {
        session.timers
            .filter { $0.state == .running && $0.deadline > date }
            .min { $0.deadline < $1.deadline }
    }

    /// The imminent scheduled step for the "next up" label.
    var nextStep: CookStepPayload? { session.nextUpStep(now: date) }
}

struct NextUpProvider: TimelineProvider {
    func placeholder(in context: Context) -> NextUpEntry {
        NextUpEntry(date: Date(), session: .idle())
    }

    func getSnapshot(in context: Context, completion: @escaping (NextUpEntry) -> Void) {
        completion(NextUpEntry(date: Date(), session: NextUpSnapshot.load()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<NextUpEntry>) -> Void) {
        let now = Date()
        let session = NextUpSnapshot.load()
        let entry = NextUpEntry(date: now, session: session)
        // The visible countdown ticks via `Text(timerInterval:)`; the watch app
        // reloads us on every session change. As a backstop, refresh at the
        // soonest timer deadline (or in 15 min when idle).
        let refresh = entry.runningTimer?.deadline ?? now.addingTimeInterval(15 * 60)
        completion(Timeline(entries: [entry], policy: .after(refresh)))
    }
}

// MARK: - Views

struct NextUpWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: NextUpEntry

    var body: some View {
        switch family {
        case .accessoryInline:
            inline
        case .accessoryCircular:
            circular
        case .accessoryCorner:
            circular
        default:
            rectangular
        }
    }

    // One-liner: "🔥 Sear chicken 4:59" or a fallback.
    private var inline: some View {
        if let timer = entry.runningTimer {
            return Text("\(Image(systemName: "timer")) \(shortLabel(timer.label)) \(Text(timerInterval: entry.date...timer.deadline, countsDown: true))")
        } else if let step = entry.nextStep {
            return Text("\(Image(systemName: "fork.knife")) \(shortLabel(step.label))")
        } else {
            return Text("\(Image(systemName: "timer")) No timers")
        }
    }

    // Ring + compact countdown for circular / corner families.
    private var circular: some View {
        ZStack {
            AccessoryWidgetBackground()
            if let timer = entry.runningTimer {
                VStack(spacing: 0) {
                    Image(systemName: "timer").font(.caption2)
                    Text(timerInterval: entry.date...timer.deadline, countsDown: true)
                        .font(.system(.caption2, design: .monospaced))
                        .multilineTextAlignment(.center)
                }
            } else {
                Image(systemName: entry.session.isActive ? "fork.knife" : "timer")
                    .font(.title3)
            }
        }
    }

    // Two-line "Next up" card for the rectangular family / Smart Stack.
    private var rectangular: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let timer = entry.runningTimer {
                Label("Timer", systemImage: "timer")
                    .font(.caption2)
                    .foregroundStyle(.orange)
                Text(shortLabel(timer.label))
                    .font(.headline)
                    .lineLimit(1)
                Text(timerInterval: entry.date...timer.deadline, countsDown: true)
                    .font(.system(.body, design: .monospaced))
            } else if let step = entry.nextStep {
                Label("Next up", systemImage: "fork.knife")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Text(shortLabel(step.label))
                    .font(.headline)
                    .lineLimit(2)
                Text("at \(FondTime.clockLabel(step.scheduledStart))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else {
                Label("Fond", systemImage: "timer")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Text("Nothing cooking")
                    .font(.headline)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
    }

    private func shortLabel(_ label: String) -> String {
        label.count > 40 ? String(label.prefix(39)) + "…" : label
    }
}

// MARK: - Widget + bundle

struct NextUpWidget: Widget {
    let kind = "FondNextUp"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: NextUpProvider()) { entry in
            NextUpWidgetView(entry: entry)
                .containerBackground(.clear, for: .widget)
        }
        .configurationDisplayName("Next Up")
        .description("The imminent cooking step or running timer.")
        .supportedFamilies([
            .accessoryInline,
            .accessoryCircular,
            .accessoryCorner,
            .accessoryRectangular,
        ])
    }
}

@main
struct FondWatchWidgetBundle: WidgetBundle {
    var body: some Widget {
        NextUpWidget()
    }
}
