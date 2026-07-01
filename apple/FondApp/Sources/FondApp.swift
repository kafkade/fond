import SwiftUI

/// Fond — a multiplatform (iOS + macOS) SwiftUI proof-of-concept over the
/// `fond-core` Rust library via UniFFI. Read + cook-mode only.
@main
struct FondApp: App {
    @StateObject private var model = AppModel()
    @StateObject private var session = CookSessionModel()
    @StateObject private var relay = PhoneSessionRelay()

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
    }
}
