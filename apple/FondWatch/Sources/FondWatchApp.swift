import SwiftUI

/// Fond's watchOS companion — a glanceable surface for the cooking timeline that
/// runs on the phone/Mac. It shows live active timers, alerts on the wrist when a
/// step timer fires, and can start/pause/advance the cook session from the
/// wrist. All state is relayed from the phone (ADR-011 / issue #75).
@main
struct FondWatchApp: App {
    @StateObject private var model = WatchSessionModel()

    var body: some Scene {
        WindowGroup {
            WatchRootView()
                .environmentObject(model)
        }
    }
}
