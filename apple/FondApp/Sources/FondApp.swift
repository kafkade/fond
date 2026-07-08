import SwiftUI

/// Fond — a multiplatform (iOS + macOS) SwiftUI proof-of-concept over the
/// `fond-core` Rust library via UniFFI. Read, cook mode, editing, and — via a
/// user-chosen synced folder — write-back into a shared fond home (issue #104).
@main
struct FondApp: App {
    @StateObject private var model = AppModel()
    @StateObject private var session = CookSessionModel()
    @StateObject private var relay = PhoneSessionRelay()

    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
                .environmentObject(session)
                .onAppear { relay.bind(to: session) }
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 720)
        #endif
        // Returning to the app is a good moment to pick up edits a sync daemon
        // landed while it was backgrounded, complementing the live folder watcher.
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { model.reindexFromDisk() }
        }

        #if os(macOS)
        Settings {
            SettingsView()
                .environmentObject(model)
                .frame(minWidth: 460, minHeight: 520)
        }
        #endif
    }
}
