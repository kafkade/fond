import Foundation
import FondKit

/// Owns the `FondClient` and the *location* it is bound to.
///
/// The app can run against one of two homes (principle #2 — the `.cook` files
/// are the source of truth, the SQLite index is derived and rebuildable):
///
/// - **Sample library** (default, first launch): bundled sample `.cook` files
///   are seeded into the app's private Application Support directory and served
///   from there — self-contained, no external permissions.
/// - **Synced folder**: a user-chosen fond home (e.g. an iCloud Drive /
///   Syncthing-managed `~/fond`) reached through a security-scoped bookmark, so
///   edits land as `.cook` files there and file-sync propagates them to the CLI
///   and other devices (issue #104).
///
/// Switching to a synced folder does **not** seed samples into it — an existing
/// collection is honored as-is; only a missing `recipes/` subdirectory is created.
@MainActor
final class AppModel: ObservableObject {
    enum LoadState: Equatable {
        case loading
        case ready
        case failed(String)
    }

    /// Where recipes currently live.
    enum Location: Equatable {
        case sampleLibrary
        case syncedFolder(URL)

        var isSynced: Bool {
            if case .syncedFolder = self { return true }
            return false
        }

        /// The folder shown to the user, if any.
        var displayURL: URL? {
            switch self {
            case .sampleLibrary: return nil
            case .syncedFolder(let url): return url
            }
        }
    }

    @Published private(set) var state: LoadState = .loading
    @Published private(set) var recipes: [RecipeSummaryDto] = []
    @Published private(set) var tags: [TagCountDto] = []
    @Published private(set) var location: Location = .sampleLibrary

    private(set) var client: FondClient?

    /// UserDefaults key holding the security-scoped bookmark for the chosen folder.
    static let bookmarkDefaultsKey = "recipesFolderBookmark"

    /// The security-scoped folder whose scope is held open for the client's
    /// lifetime. `nil` when running the sample library.
    private var scopedFolder: SecurityScopedFolder?

    /// Watches the bound folder for external (sync) changes and triggers a reindex.
    private var watcher: RecipesFolderWatcher?

    init() {
        Task { await bootstrap() }
    }

    // MARK: - Bootstrap

    /// Resolve the persisted folder bookmark if present, otherwise fall back to
    /// the seeded sample library, and open the client there.
    func bootstrap() async {
        state = .loading
        do {
            if let folder = try SecurityScopedFolder.resolve(forKey: Self.bookmarkDefaultsKey) {
                try openSyncedFolder(folder)
            } else {
                try openSampleLibrary()
            }
        } catch {
            teardownBinding()
            self.state = .failed(Self.describe(error))
        }
    }

    // MARK: - Location switching

    /// Point the app at a user-chosen synced fond home. The URL comes from the
    /// document picker / `fileImporter`; its bookmark is persisted so the choice
    /// survives relaunch.
    func chooseSyncedFolder(_ url: URL) {
        state = .loading
        do {
            let folder = try SecurityScopedFolder.persist(
                pickedURL: url,
                forKey: Self.bookmarkDefaultsKey
            )
            try openSyncedFolder(folder)
        } catch {
            // On failure, leave the previous binding cleared and surface the error;
            // the user can retry or fall back to the sample library.
            teardownBinding()
            SecurityScopedFolder.clear(forKey: Self.bookmarkDefaultsKey)
            self.state = .failed(Self.describe(error))
        }
    }

    /// Return to the built-in sample library, forgetting any chosen folder.
    func useSampleLibrary() {
        state = .loading
        SecurityScopedFolder.clear(forKey: Self.bookmarkDefaultsKey)
        do {
            try openSampleLibrary()
        } catch {
            teardownBinding()
            self.state = .failed(Self.describe(error))
        }
    }

    // MARK: - Binding a client to a data dir

    private func openSampleLibrary() throws {
        teardownBinding()
        let dataDir = try Self.prepareSampleDataDir()
        let client = try FondClient(dataDir: dataDir.path)
        _ = try client.reindex()
        self.client = client
        self.location = .sampleLibrary
        finishOpen(recipesDir: dataDir.appendingPathComponent("recipes", isDirectory: true))
    }

    private func openSyncedFolder(_ folder: SecurityScopedFolder) throws {
        teardownBinding()
        // Best-effort: open the security scope and hold it for the whole time the
        // client is bound — the Rust core does filesystem I/O against this folder
        // on every operation. `startAccessing()` returns false for URLs that
        // aren't security-scoped (e.g. a non-sandboxed macOS build), where access
        // works anyway, so a real failure surfaces from the file operations below.
        folder.startAccessing()
        self.scopedFolder = folder

        // Treat the chosen folder as the fond home; ensure `recipes/` exists but
        // never seed samples into a user's own collection.
        let recipesDir = folder.url.appendingPathComponent("recipes", isDirectory: true)
        try FileManager.default.createDirectory(at: recipesDir, withIntermediateDirectories: true)

        let client = try FondClient(dataDir: folder.url.path)
        _ = try client.reindex()
        self.client = client
        self.location = .syncedFolder(folder.url)
        finishOpen(recipesDir: recipesDir)
    }

    /// Common tail for both homes: load the list/tags, mark ready, and start
    /// watching for external changes.
    private func finishOpen(recipesDir: URL) {
        self.recipes = (try? client?.listRecipes(filter: nil)) ?? []
        self.tags = (try? client?.listTags()) ?? []
        self.state = .ready
        startWatching(recipesDir: recipesDir)
    }

    /// Release the current client, watcher, and security scope.
    private func teardownBinding() {
        watcher?.stop()
        watcher = nil
        client = nil
        recipes = []
        tags = []
        scopedFolder?.stopAccessing()
        scopedFolder = nil
    }

    // MARK: - External-change reindex

    private func startWatching(recipesDir: URL) {
        watcher = RecipesFolderWatcher(directory: recipesDir) { [weak self] in
            self?.reindexFromDisk()
        }
        watcher?.start()
    }

    /// Rebuild the derived index from the bound folder and refresh the UI. Runs
    /// on external file changes (sync landing edits) and on returning to the app.
    func reindexFromDisk() {
        guard let client, state == .ready else { return }
        _ = try? client.reindex()
        reload()
    }

    // MARK: - Queries

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

    // MARK: - Seeding (sample library only)

    /// Create `~/Library/Application Support/fond/recipes`, copying any bundled
    /// `.cook` files that aren't there yet (idempotent).
    static func prepareSampleDataDir() throws -> URL {
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
