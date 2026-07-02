import Foundation
import FondKit

/// Owns the `FondClient`, seeds bundled sample recipes into app storage on
/// first launch, rebuilds the index, and exposes recipe data to the views.
@MainActor
final class AppModel: ObservableObject {
    enum LoadState: Equatable {
        case loading
        case ready
        case failed(String)
    }

    @Published private(set) var state: LoadState = .loading
    @Published private(set) var recipes: [RecipeSummaryDto] = []
    @Published private(set) var tags: [TagCountDto] = []

    private(set) var client: FondClient?

    init() {
        Task { await bootstrap() }
    }

    func bootstrap() async {
        state = .loading
        do {
            let dataDir = try Self.prepareDataDir()
            let client = try FondClient(dataDir: dataDir.path)
            // The SQLite index is derived; rebuild it from the seeded files.
            _ = try client.reindex()
            self.client = client
            self.recipes = try client.listRecipes(filter: nil)
            self.tags = (try? client.listTags()) ?? []
            self.state = .ready
        } catch {
            self.state = .failed(Self.describe(error))
        }
    }

    /// Recipes scoped to a sidebar selection. `.all` returns everything; a tag
    /// selection filters server-side via the Rust core's `RecipeFilter`.
    func recipes(for selection: SidebarSelection) -> [RecipeSummaryDto] {
        switch selection {
        case .all:
            return recipes
        case .tag(let name):
            guard let client else { return [] }
            let filter = RecipeFilterDto(tags: [name], maxTimeMinutes: nil, source: nil)
            return (try? client.listRecipes(filter: filter)) ?? []
        }
    }

    /// Full-text search, optionally scoped to a sidebar selection's tag.
    /// Returns [] on empty/whitespace queries.
    func search(_ query: String, in selection: SidebarSelection = .all) -> [SearchResultDto] {
        guard let client,
              !query.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
        let filter: RecipeFilterDto?
        switch selection {
        case .all:
            filter = nil
        case .tag(let name):
            filter = RecipeFilterDto(tags: [name], maxTimeMinutes: nil, source: nil)
        }
        return (try? client.search(query: query, filter: filter)) ?? []
    }

    // MARK: - Mutations

    /// Re-read the recipe list and tag counts from the index after a write.
    /// Cheap for a single-household library, so it runs on the main actor.
    func reload() {
        guard let client else { return }
        recipes = (try? client.listRecipes(filter: nil)) ?? recipes
        tags = (try? client.listTags()) ?? tags
    }

    // MARK: - Seeding

    /// Create `~/Library/Application Support/fond/recipes`, copying any bundled
    /// `.cook` files that aren't there yet (idempotent).
    static func prepareDataDir() throws -> URL {
        let fm = FileManager.default
        let support = try fm.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dataDir = support.appendingPathComponent("fond", isDirectory: true)
        let recipesDir = dataDir.appendingPathComponent("recipes", isDirectory: true)
        try fm.createDirectory(at: recipesDir, withIntermediateDirectories: true)

        if let bundled = Bundle.main.url(forResource: "recipes", withExtension: nil) {
            let files = try fm.contentsOfDirectory(at: bundled, includingPropertiesForKeys: nil)
            for file in files where file.pathExtension == "cook" {
                let dest = recipesDir.appendingPathComponent(file.lastPathComponent)
                if !fm.fileExists(atPath: dest.path) {
                    try fm.copyItem(at: file, to: dest)
                }
            }
        }
        return dataDir
    }

    private static func describe(_ error: Error) -> String {
        if let fondError = error as? FondError {
            return String(describing: fondError)
        }
        return error.localizedDescription
    }
}
