import SwiftUI
import FondKit

/// Middle column: the recipes for the current sidebar selection, with full-text
/// search. Selection is bound to a slug so the detail column (third pane) updates
/// in place — no push/pop needed on iPad/macOS.
struct RecipeListView: View {
    @EnvironmentObject private var model: AppModel
    let selection: SidebarSelection
    @Binding var selectedSlug: String?
    @State private var query = ""
    @State private var showingNew = false

    var body: some View {
        Group {
            switch model.state {
            case .loading:
                ProgressView("Loading recipes…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .failed(let message):
                ContentUnavailableView {
                    Label("Couldn’t load recipes", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(message)
                }
            case .ready:
                recipeList
            }
        }
        .navigationTitle(title)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .searchable(text: $query, placement: .automatic, prompt: "Search recipes")
        .toolbar {
            ToolbarItem {
                Button { showingNew = true } label: {
                    Label("New Recipe", systemImage: "plus")
                }
                .disabled(model.state != .ready)
            }
        }
        .sheet(isPresented: $showingNew) {
            RecipeEditView(mode: .create, onSaved: { slug in selectedSlug = slug })
                .environmentObject(model)
        }
    }

    private var title: String {
        switch selection {
        case .all: return "All Recipes"
        case .tag(let name): return name
        }
    }

    @ViewBuilder
    private var recipeList: some View {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty {
            let recipes = model.recipes(for: selection)
            List(selection: $selectedSlug) {
                ForEach(recipes) { recipe in
                    RecipeRow(title: recipe.title, subtitle: recipe.source,
                              tags: recipe.tags, time: recipe.totalTime)
                        .tag(recipe.slug)
                }
            }
            .overlay {
                if recipes.isEmpty {
                    ContentUnavailableView("No recipes", systemImage: "fork.knife")
                }
            }
        } else {
            let results = model.search(trimmed, in: selection)
            List(selection: $selectedSlug) {
                ForEach(results) { hit in
                    RecipeRow(title: hit.title, subtitle: hit.source,
                              tags: hit.tags, time: nil)
                        .tag(hit.slug)
                }
            }
            .overlay {
                if results.isEmpty {
                    ContentUnavailableView.search(text: trimmed)
                }
            }
        }
    }
}

struct RecipeRow: View {
    let title: String
    let subtitle: String
    let tags: [String]
    let time: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.headline)
            if !subtitle.isEmpty {
                Text(subtitle).font(.subheadline).foregroundStyle(.secondary)
            }
            HStack(spacing: 6) {
                if let time, !time.isEmpty {
                    Label(time, systemImage: "clock")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                ForEach(tags.prefix(3), id: \.self) { tag in
                    Text(tag)
                        .font(.caption2)
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                }
            }
        }
        .padding(.vertical, 2)
    }
}
