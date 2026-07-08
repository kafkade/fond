import Foundation

/// Watches a `recipes/` directory for changes that arrive from *outside* the app
/// — a file-sync daemon (Syncthing), iCloud Drive, or the CLI writing into the
/// same shared fond home — and coalesces them into a single reindex callback.
///
/// Uses a `DispatchSourceFileSystemObject` vnode watch on the directory itself,
/// which fires on writes, renames, and deletes of its entries. Sync tools stage
/// atomically (write-temp-then-rename), producing a burst of events, so the
/// callback is debounced to run once the dust settles rather than per event.
///
/// This is intentionally lightweight and cross-platform (iOS + macOS). It is a
/// best-effort freshness signal; the authoritative rebuild is still `reindex`,
/// which is cheap for a single-household library and idempotent.
final class RecipesFolderWatcher {
    private let directory: URL
    private let onChange: () -> Void
    private let debounce: DispatchTimeInterval

    private var fileDescriptor: CInt = -1
    private var source: DispatchSourceFileSystemObject?
    private var pending: DispatchWorkItem?
    private let queue = DispatchQueue(label: "dev.kafkade.fond.recipes-watcher")

    /// - Parameters:
    ///   - directory: the `recipes/` directory to observe.
    ///   - debounce: how long to wait after the last event before firing.
    ///   - onChange: invoked on the main queue after a settled burst of changes.
    init(
        directory: URL,
        debounce: DispatchTimeInterval = .milliseconds(400),
        onChange: @escaping () -> Void
    ) {
        self.directory = directory
        self.debounce = debounce
        self.onChange = onChange
    }

    /// Begin watching. No-op if the directory can't be opened (the app still
    /// works; it just won't auto-refresh on external change until relaunch).
    func start() {
        guard source == nil else { return }
        let fd = open(directory.path, O_EVTONLY)
        guard fd >= 0 else { return }
        fileDescriptor = fd

        let src = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .rename, .delete, .extend, .attrib, .link, .revoke],
            queue: queue
        )
        src.setEventHandler { [weak self] in self?.scheduleCallback() }
        src.setCancelHandler { [weak self] in
            guard let self else { return }
            if self.fileDescriptor >= 0 {
                close(self.fileDescriptor)
                self.fileDescriptor = -1
            }
        }
        source = src
        src.resume()
    }

    /// Stop watching and release the file descriptor.
    func stop() {
        pending?.cancel()
        pending = nil
        source?.cancel()
        source = nil
    }

    private func scheduleCallback() {
        pending?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            DispatchQueue.main.async { self.onChange() }
        }
        pending = work
        queue.asyncAfter(deadline: .now() + debounce, execute: work)
    }

    deinit {
        stop()
    }
}
