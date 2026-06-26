import SwiftUI
import FondKit

/// Cook mode: builds the recipe's timeline and schedules it backward from a
/// chosen serve time, surfacing per-step start times, active/passive work, and
/// timer durations.
struct CookModeView: View {
    let slug: String
    let title: String
    @EnvironmentObject private var model: AppModel

    @State private var serveAt = Date().addingTimeInterval(2 * 3600)
    @State private var schedule: ScheduledTimelineDto?
    @State private var error: String?

    var body: some View {
        List {
            Section {
                DatePicker("Serve at", selection: $serveAt)
                    .onChange(of: serveAt) { _, _ in reschedule() }
            } footer: {
                Text("Steps are scheduled backward from this time. Untimed steps stay untimed.")
            }

            if let schedule {
                Section("Plan") {
                    summaryRow("Start cooking", Self.timeLabel(schedule.startAt))
                    summaryRow("Hands-on time", Self.durationLabel(schedule.totalActiveSeconds))
                    summaryRow("Hands-off time", Self.durationLabel(schedule.totalPassiveSeconds))
                    if schedule.hasUntimedSteps {
                        Label("Some steps have no known duration", systemImage: "questionmark.circle")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }

                Section("Steps") {
                    ForEach(schedule.nodes) { scheduled in
                        timelineRow(scheduled)
                    }
                }
            } else if let error {
                Section { Text(error).foregroundStyle(.red) }
            } else {
                Section { ProgressView() }
            }
        }
        .navigationTitle("Cook · \(title)")
        .task(id: slug) { reschedule() }
    }

    private func timelineRow(_ scheduled: ScheduledNodeDto) -> some View {
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

            VStack(alignment: .leading, spacing: 2) {
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
            }
            Spacer()
        }
        .padding(.vertical, 2)
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
            schedule = try client.scheduleTimeline(slug: slug, serveAt: Self.serveFormatter.string(from: serveAt))
            error = nil
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
