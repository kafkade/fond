import Foundation

/// A user-chosen folder the app is allowed to read and write, backed by a
/// **security-scoped bookmark** so access survives relaunches.
///
/// The Rust `FondClient` performs filesystem I/O against this folder for the
/// whole time it is bound to it (every read, write, and reindex), so the
/// security scope must stay *open* for that entire lifetime — not just for a
/// single access. Callers therefore `startAccessing()` once when the folder is
/// resolved and `stopAccessing()` only when switching away or on teardown.
///
/// The bookmark is persisted under a UserDefaults key; platform-conditional
/// creation/resolution options bridge the difference between macOS
/// (`.withSecurityScope`) and iOS (plain minimal bookmarks whose scope is
/// unlocked via `startAccessingSecurityScopedResource`).
final class SecurityScopedFolder {
    /// The resolved on-disk location (the fond home the user picked).
    let url: URL

    private var isAccessing = false

    private init(url: URL) {
        self.url = url
    }

    // MARK: - Bookmark options (platform-conditional)

    private static var creationOptions: URL.BookmarkCreationOptions {
        #if os(macOS)
        return [.withSecurityScope]
        #else
        // iOS has no `.withSecurityScope`; a minimal bookmark plus
        // start/stopAccessing is the supported path for document-picker URLs.
        return []
        #endif
    }

    private static var resolutionOptions: URL.BookmarkResolutionOptions {
        #if os(macOS)
        return [.withSecurityScope]
        #else
        return []
        #endif
    }

    // MARK: - Persistence

    /// Create a security-scoped bookmark for a freshly-picked folder and persist
    /// it under `defaultsKey`. The picked URL is already scoped by the document
    /// picker, so this briefly opens the scope to mint the bookmark.
    static func persist(
        pickedURL url: URL,
        forKey defaultsKey: String,
        defaults: UserDefaults = .standard
    ) throws -> SecurityScopedFolder {
        let needsScope = url.startAccessingSecurityScopedResource()
        defer { if needsScope { url.stopAccessingSecurityScopedResource() } }

        let data = try url.bookmarkData(
            options: creationOptions,
            includingResourceValuesForKeys: nil,
            relativeTo: nil
        )
        defaults.set(data, forKey: defaultsKey)
        return SecurityScopedFolder(url: url)
    }

    /// Resolve a previously-persisted bookmark, refreshing it transparently if
    /// the system reports it stale. Returns `nil` when no bookmark is stored.
    static func resolve(
        forKey defaultsKey: String,
        defaults: UserDefaults = .standard
    ) throws -> SecurityScopedFolder? {
        guard let data = defaults.data(forKey: defaultsKey) else { return nil }

        var isStale = false
        let url = try URL(
            resolvingBookmarkData: data,
            options: resolutionOptions,
            relativeTo: nil,
            bookmarkDataIsStale: &isStale
        )
        let folder = SecurityScopedFolder(url: url)

        if isStale {
            // Re-mint from the resolved URL so the next launch stays valid.
            let refreshed = folder.startAccessing()
            defer { if refreshed { folder.stopAccessing() } }
            if let fresh = try? url.bookmarkData(
                options: creationOptions,
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            ) {
                defaults.set(fresh, forKey: defaultsKey)
            }
        }
        return folder
    }

    /// Remove any persisted bookmark for `defaultsKey`.
    static func clear(forKey defaultsKey: String, defaults: UserDefaults = .standard) {
        defaults.removeObject(forKey: defaultsKey)
    }

    // MARK: - Scope lifetime

    /// Open the security scope. Idempotent. Returns whether the scope is (now)
    /// held by this object.
    @discardableResult
    func startAccessing() -> Bool {
        guard !isAccessing else { return true }
        isAccessing = url.startAccessingSecurityScopedResource()
        return isAccessing
    }

    /// Close the security scope if this object opened it.
    func stopAccessing() {
        guard isAccessing else { return }
        url.stopAccessingSecurityScopedResource()
        isAccessing = false
    }

    deinit {
        stopAccessing()
    }
}
