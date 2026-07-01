import SwiftUI
import FondKit

/// Which collection the recipe list is scoped to. Backed by tags today (fond has
/// no separate "collection" entity yet), with `.all` as the everything bucket.
enum SidebarSelection: Hashable {
    case all
    case tag(String)
}

/// First column of the split view: an "All Recipes" bucket followed by every tag
/// with its recipe count. Selection drives the middle recipe-list column.
///
/// Uses a selection-bound `List`, so on iPad/macOS arrow keys move the highlight
/// and a pointer hover shows the standard selection affordance.
struct SidebarView: View {
    @EnvironmentObject private var model: AppModel
    @Binding var selection: SidebarSelection?

    var body: some View {
        List(selection: $selection) {
            Section {
                Label("All Recipes", systemImage: "square.grid.2x2")
                    .tag(SidebarSelection.all)
            }

            if !model.tags.isEmpty {
                Section("Tags") {
                    ForEach(model.tags) { tag in
                        HStack {
                            Label(tag.name, systemImage: "tag")
                            Spacer()
                            Text("\(tag.count)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .monospacedDigit()
                        }
                        .tag(SidebarSelection.tag(tag.name))
                    }
                }
            }
        }
        .navigationTitle("Fond")
        #if os(iOS)
        .listStyle(.sidebar)
        #endif
    }
}
