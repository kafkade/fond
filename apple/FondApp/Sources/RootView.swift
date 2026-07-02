import SwiftUI
import FondKit

/// Root three-column layout: sidebar (collections/tags) → recipe list → detail.
///
/// `NavigationSplitView` adapts automatically: on a regular-width canvas (iPad
/// landscape, macOS, wide Stage Manager) all three columns are visible; in
/// compact width (iPhone, Slide Over, narrow multitasking) it collapses to a
/// navigation stack, so browse → view → cook works everywhere from one codebase.
struct RootView: View {
    @EnvironmentObject private var model: AppModel

    @State private var sidebar: SidebarSelection? = .all
    @State private var selectedSlug: String?
    @State private var columnVisibility: NavigationSplitViewVisibility = .all

    var body: some View {
        NavigationSplitView(columnVisibility: $columnVisibility) {
            SidebarView(selection: $sidebar)
                .navigationSplitViewColumnWidth(min: 220, ideal: 260, max: 320)
        } content: {
            RecipeListView(selection: sidebar ?? .all, selectedSlug: $selectedSlug)
                .navigationSplitViewColumnWidth(min: 300, ideal: 360, max: 460)
        } detail: {
            NavigationStack {
                if let slug = selectedSlug {
                    RecipeDetailView(slug: slug, selectedSlug: $selectedSlug)
                } else {
                    ContentUnavailableView(
                        "Select a recipe",
                        systemImage: "fork.knife",
                        description: Text("Pick a recipe to see its ingredients, steps, and cook mode.")
                    )
                }
            }
            // Reset any pushed cook-mode view when the selected recipe changes.
            .id(selectedSlug)
        }
        .navigationSplitViewStyle(.balanced)
        // When the sidebar filter changes, clear a detail selection that is no
        // longer in the visible list.
        .onChange(of: sidebar) { _, newValue in
            let visible = model.recipes(for: newValue ?? .all).map(\.slug)
            if let slug = selectedSlug, !visible.contains(slug) {
                selectedSlug = nil
            }
        }
    }
}
