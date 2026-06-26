import SwiftUI

/// Fond — a multiplatform (iOS + macOS) SwiftUI proof-of-concept over the
/// `fond-core` Rust library via UniFFI. Read + cook-mode only.
@main
struct FondApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
        }
        #if os(macOS)
        .defaultSize(width: 900, height: 640)
        #endif
    }
}
