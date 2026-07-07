import SwiftUI
import UniformTypeIdentifiers

/// Lets the user choose where recipes live: the built-in **sample library**
/// (self-contained default) or a **synced folder** — a fond home (e.g. an
/// iCloud Drive / Syncthing-managed `~/fond`) the app reads and writes so edits
/// propagate to the CLI and other devices (issue #104).
///
/// Picking a folder goes through the system document picker (`fileImporter`),
/// which grants the app a security-scoped URL; `AppModel` persists it as a
/// bookmark so the choice survives relaunch.
struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    @State private var showingImporter = false
    @State private var confirmReset = false

    var body: some View {
        NavigationStack {
            Form {
                locationSection
                syncGuidanceSection
                if case .failed(let message) = model.state {
                    Section {
                        Label(message, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.orange)
                    }
                }
            }
            .navigationTitle("Recipe Storage")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .fileImporter(
                isPresented: $showingImporter,
                allowedContentTypes: [.folder],
                allowsMultipleSelection: false
            ) { result in
                handleImport(result)
            }
            .confirmationDialog(
                "Switch to the sample library?",
                isPresented: $confirmReset,
                titleVisibility: .visible
            ) {
                Button("Use Sample Library", role: .destructive) {
                    model.useSampleLibrary()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("The app will stop reading your synced folder and run from the bundled samples. Your folder’s files are left untouched.")
            }
        }
    }

    // MARK: - Sections

    @ViewBuilder
    private var locationSection: some View {
        Section("Current Location") {
            switch model.location {
            case .sampleLibrary:
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Sample Library")
                        Text("Bundled recipes in the app’s private storage. Not synced.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                } icon: {
                    Image(systemName: "shippingbox")
                }
            case .syncedFolder(let url):
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(url.lastPathComponent)
                        Text(url.path)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                            .truncationMode(.middle)
                    }
                } icon: {
                    Image(systemName: "folder.badge.gearshape")
                }
            }
        }

        Section {
            Button {
                showingImporter = true
            } label: {
                Label(
                    model.location.isSynced ? "Choose a Different Folder…" : "Choose Synced Folder…",
                    systemImage: "folder.badge.plus"
                )
            }

            if model.location.isSynced {
                Button(role: .destructive) {
                    confirmReset = true
                } label: {
                    Label("Use Sample Library", systemImage: "arrow.uturn.backward")
                }
            }
        } footer: {
            Text("Point Fond at a folder your devices sync (iCloud Drive or Syncthing). Fond treats it as your recipe home: it reads and writes `.cook` files under a `recipes/` subfolder there, so edits reach the CLI and your other devices.")
        }
    }

    private var syncGuidanceSection: some View {
        Section("Keeping devices in sync") {
            GuidanceRow(
                systemImage: "arrow.triangle.2.circlepath",
                title: "Use one shared home",
                detail: "Point every device — and the CLI — at the same folder (e.g. ~/fond). Fond writes `.cook` files atomically so a sync daemon never sees a half-written file."
            )
            GuidanceRow(
                systemImage: "externaldrive.badge.xmark",
                title: "Don’t sync fond.db",
                detail: "The SQLite index is derived and rebuilt automatically. Add `fond.db` to your sync-ignore list to avoid conflicts on the binary file."
            )
            GuidanceRow(
                systemImage: "exclamationmark.arrow.triangle.2.circlepath",
                title: "Concurrent edits",
                detail: "Editing the same recipe on two offline devices relies on your file-sync tool’s conflict handling. Fond flags a conflict if a file changed on disk since you opened it."
            )
        }
    }

    // MARK: - Actions

    private func handleImport(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            guard let url = urls.first else { return }
            model.chooseSyncedFolder(url)
        case .failure:
            // User cancelled or the picker failed; nothing to do.
            break
        }
    }
}

private struct GuidanceRow: View {
    let systemImage: String
    let title: String
    let detail: String

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } icon: {
            Image(systemName: systemImage)
        }
    }
}
