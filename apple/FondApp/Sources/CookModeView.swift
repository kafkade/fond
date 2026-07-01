import SwiftUI
import FondKit

/// Cook mode: builds the recipe's timeline, schedules it backward from a chosen
/// serve time, and — on a wide canvas (iPad landscape / macOS) — lays the steps
/// out beside a live panel of kitchen timers and the plan summary. In compact
/// width it falls back to a single scrolling column. Timers are real countdowns
/// started from a step's known duration.
struct CookModeView: View {
    let slug: String
    let title: String
    @EnvironmentObject private var model: AppModel
    @EnvironmentObject private var session: CookSessionModel
    private var timers: KitchenTimerModel { session.timers }

    @State private var serveAt = Date().addingTimeInterval(2 * 3600)
    @State private var schedule: ScheduledTimelineDto?
    @State private var error: String?

    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var hSize
    private var isWide: Bool { hSize == .regular }
    #else
    private var isWide: Bool { true }
    #endif

    var body: some View {
        Group {
            if isWide {
                wideLayout
            } else {
                compactLayout
            }
        }
        .navigationTitle("Cook · \(title)")
        .task(id: slug) { reschedule() }
    }

    // MARK: - Wide (iPad landscape / macOS): steps beside live timers

    private var wideLayout: some View {
        HStack(spacing: 0) {
            List {
                if let schedule {
                    Section("Steps") {
                        ForEach(schedule.nodes) { stepRow($0) }
                    }
                } else {
                    placeholderSection
                }
            }
            .frame(minWidth: 320)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    serveCard
                    if let schedule { planCard(schedule) }
                    timersCard
                }
                .padding()
            }
            .frame(width: 360)
            #if os(iOS)
            .background(Color(uiColor: .systemGroupedBackground))
            #endif
        }
    }

    // MARK: - Compact (iPhone / Slide Over): single column

    private var compactLayout: some View {
        List {
            Section {
                DatePicker("Serve at", selection: $serveAt)
                    .onChange(of: serveAt) { _, _ in reschedule() }
            } footer: {
                Text("Steps are scheduled backward from this time. Untimed steps stay untimed.")
            }

            if timers.hasTimers {
                Section("Timers") { timerCards }
            }

            if let schedule {
                Section("Plan") { planRows(schedule) }
                Section("Steps") {
                    ForEach(schedule.nodes) { stepRow($0) }
                }
            } else if let error {
                Section { Text(error).foregroundStyle(.red) }
            } else {
                Section { ProgressView() }
            }
        }
    }

    // MARK: - Shared pieces

    @ViewBuilder
    private var placeholderSection: some View {
        if let error {
            Section { Text(error).foregroundStyle(.red) }
        } else {
            Section { ProgressView() }
        }
    }

    private func stepRow(_ scheduled: ScheduledNodeDto) -> some View {
        let node = scheduled.node
        return HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .trailing) {
                Text(Self.timeLabel(scheduled.scheduledStart))
                    .font(.system(.body, design: .monospaced))
                Image(systemName: node.taskType.isActive ? "hand.raised.fill" : "hourglass")
                    .font(.caption)
                    .foregroundStyle(node.taskType.isActive ? .orange : .secondary)
            }
            .frame(width: 64, alignment: .trailing)

            VStack(alignment: .leading, spacing: 4) {
                Text(node.label)
                HStack(spacing: 8) {
                    Text(node.taskType.label)
                        .font(.caption2)
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                    if let duration = node.duration {
                        Label(duration.pretty, systemImage: "timer")
                            .font(.caption2).foregroundStyle(.secondary)
                    }
                }
                if let duration = node.duration, duration.seconds > 0 {
                    Button {
                        session.startTimer(stepId: node.id, label: node.label, seconds: Int(duration.seconds))
                    } label: {
                        Label("Start timer", systemImage: "timer")
                            .font(.caption)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .padding(.top, 2)
                }
            }
            Spacer()
        }
        .padding(.vertical, 2)
    }

    private var serveCard: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Serve time").font(.headline)
            DatePicker("Serve at", selection: $serveAt)
                .labelsHidden()
                .onChange(of: serveAt) { _, _ in reschedule() }
            Text("Steps are scheduled backward from this time. Untimed steps stay untimed.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func planCard(_ schedule: ScheduledTimelineDto) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Plan").font(.headline)
            planRows(schedule)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func planRows(_ schedule: ScheduledTimelineDto) -> some View {
        summaryRow("Start cooking", Self.timeLabel(schedule.startAt))
        summaryRow("Hands-on time", Self.durationLabel(schedule.totalActiveSeconds))
        summaryRow("Hands-off time", Self.durationLabel(schedule.totalPassiveSeconds))
        if schedule.hasUntimedSteps {
            Label("Some steps have no known duration", systemImage: "questionmark.circle")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private var timersCard: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Timers").font(.headline)
                Spacer()
                if timers.hasTimers {
                    Button("Clear all") { timers.cancelAll() }
                        .font(.caption)
                }
            }
            if timers.hasTimers {
                timerCards
            } else {
                Text("Start a timer from any timed step to count it down here.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var timerCards: some View {
        ForEach(timers.timers) { timer in
            KitchenTimerView(
                timer: timer,
                onPause: { timers.pause(timer.id) },
                onResume: { timers.resume(timer.id) },
                onAddMinute: { timers.addMinute(timer.id) },
                onCancel: { timers.cancel(timer.id) }
            )
        }
    }

    private func summaryRow(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).foregroundStyle(.secondary)
            Spacer()
            Text(value).font(.system(.body, design: .monospaced))
        }
    }

    // MARK: - Scheduling

    private func reschedule() {
        guard let client = model.client else { return }
        do {
            let scheduled = try client.scheduleTimeline(slug: slug, serveAt: Self.serveFormatter.string(from: serveAt))
            schedule = scheduled
            error = nil
            // Promote to the app-wide authoritative session so it relays to the
            // Watch and drives the "Next up" complication.
            session.activate(schedule: scheduled)
        } catch {
            self.error = String(describing: error)
        }
    }

    // MARK: - Formatting

    /// Matches the ISO 8601 local format the FFI expects/returns (no timezone).
    private static let serveFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd'T'HH:mm:ss"
        return f
    }()

    private static let displayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "HH:mm"
        return f
    }()

    /// Convert an ISO string from the FFI into a short "HH:mm" label.
    static func timeLabel(_ iso: String) -> String {
        guard let date = serveFormatter.date(from: iso) else { return iso }
        return displayFormatter.string(from: date)
    }

    static func durationLabel(_ seconds: UInt64) -> String {
        let total = Int(seconds)
        let h = total / 3600
        let m = (total % 3600) / 60
        if h > 0 && m > 0 { return "\(h)h \(m)m" }
        if h > 0 { return "\(h)h" }
        return "\(m)m"
    }
}
