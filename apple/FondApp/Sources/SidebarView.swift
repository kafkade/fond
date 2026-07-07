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
    @State private var showingSettings = false

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
        .toolbar {
            ToolbarItem {
                Button {
                    showingSettings = true
                } label: {
                    Label("Recipe Storage", systemImage: "gearshape")
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            if model.location.isSynced {
                SyncedFolderBadge()
            }
        }
        .sheet(isPresented: $showingSettings) {
            SettingsView()
                .environmentObject(model)
        }
    }
}

/// A small footer showing that the app is bound to a synced folder rather than
/// the sample library, so it's obvious edits are writing back to a shared home.
private struct SyncedFolderBadge: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        if let url = model.location.displayURL {
            HStack(spacing: 6) {
                Image(systemName: "folder.badge.gearshape")
                Text(url.lastPathComponent)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(.bar)
        }
    }
}
