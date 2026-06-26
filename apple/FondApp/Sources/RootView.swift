import SwiftUI
import FondKit

/// Root list of recipes with full-text search. Tapping a recipe pushes its
/// detail view.
struct RootView: View {
    @EnvironmentObject private var model: AppModel
    @State private var query = ""

    var body: some View {
        NavigationStack {
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
            .navigationTitle("Fond")
            .searchable(text: $query, prompt: "Search recipes")
        }
    }

    @ViewBuilder
    private var recipeList: some View {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty {
            List(model.recipes) { recipe in
                NavigationLink(value: recipe.slug) {
                    RecipeRow(title: recipe.title, subtitle: recipe.source,
                              tags: recipe.tags, time: recipe.totalTime)
                }
            }
            .navigationDestination(for: String.self) { slug in
                RecipeDetailView(slug: slug)
            }
        } else {
            let results = model.search(trimmed)
            List(results) { hit in
                NavigationLink(value: hit.slug) {
                    RecipeRow(title: hit.title, subtitle: hit.source,
                              tags: hit.tags, time: nil)
                }
            }
            .overlay {
                if results.isEmpty {
                    ContentUnavailableView.search(text: trimmed)
                }
            }
            .navigationDestination(for: String.self) { slug in
                RecipeDetailView(slug: slug)
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
