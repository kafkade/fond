import Foundation

// Small ergonomic additions to the generated UniFFI types. These keep the
// SwiftUI layer tidy (stable identities for `List`/`ForEach`) without changing
// the generated bindings.

// `RecipeSummaryDto` and `TimelineNodeDto` already carry an `id` field, so they
// only need the conformance declared.
extension RecipeSummaryDto: Identifiable {}

extension SearchResultDto: Identifiable {
    public var id: String { slug }
}

extension TagCountDto: Identifiable {
    public var id: String { name }
}

extension TimelineNodeDto: Identifiable {}

extension ScheduledNodeDto: Identifiable {
    public var id: UInt64 { node.id }
}

public extension TaskTypeDto {
    /// Whether this step needs hands-on attention.
    var isActive: Bool {
        switch self {
        case .activePrep, .activeCook: return true
        case .passivePrep, .passiveCook, .rest: return false
        }
    }

    /// Short human label for display.
    var label: String {
        switch self {
        case .activePrep: return "Active prep"
        case .passivePrep: return "Passive prep"
        case .activeCook: return "Active cook"
        case .passiveCook: return "Passive cook"
        case .rest: return "Rest"
        }
    }
}

public extension StepDurationDto {
    /// Friendly duration like "1h 30m", "45m", or "30s".
    var pretty: String {
        let total = Int(seconds)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        var parts: [String] = []
        if h > 0 { parts.append("\(h)h") }
        if m > 0 { parts.append("\(m)m") }
        if h == 0, m == 0, s > 0 { parts.append("\(s)s") }
        return parts.isEmpty ? "—" : parts.joined(separator: " ")
    }
}
