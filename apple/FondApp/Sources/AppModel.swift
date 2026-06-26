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
            self.state = .ready
        } catch {
            self.state = .failed(Self.describe(error))
        }
    }

    /// Full-text search; returns [] on empty/whitespace queries.
    func search(_ query: String) -> [SearchResultDto] {
        guard let client,
              !query.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
        return (try? client.search(query: query, filter: nil)) ?? []
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
