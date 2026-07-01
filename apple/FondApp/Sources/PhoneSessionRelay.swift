import Foundation

#if os(iOS)
import WatchConnectivity
#endif

/// Phone side of the phone ⇄ watch relay.
///
/// It is the session model's `broadcaster`: every time the authoritative
/// `CookSessionModel` changes it encodes a `CookSessionPayload` and pushes it as
/// the WCSession *application context* (latest-state, coalescing, delivered when
/// the Watch next wakes). Control actions from the wrist arrive as messages /
/// user-info transfers and are applied back onto the same session, which then
/// re-broadcasts. On macOS (no Watch) this degrades to a no-op broadcaster.
@MainActor
final class PhoneSessionRelay: NSObject, ObservableObject, CookSessionBroadcaster {
    private weak var session: CookSessionModel?

    /// Wire this relay to the app's session model and activate WatchConnectivity.
    func bind(to session: CookSessionModel) {
        guard self.session !== session else { return }
        self.session = session
        session.broadcaster = self
        activate()
        // Emit the current state immediately (idle if nothing is cooking) so a
        // freshly launched Watch gets a snapshot.
        session.broadcast()
    }

    // MARK: - CookSessionBroadcaster

    func broadcast(_ payload: CookSessionPayload) {
        guard let data = FondCodec.encode(payload) else { return }
        #if os(iOS)
        guard WCSession.isSupported() else { return }
        let wc = WCSession.default
        guard wc.activationState == .activated else { return }
        do {
            try wc.updateApplicationContext([FondWatchLink.sessionContextKey: data])
        } catch {
            // Non-fatal: the next change will retry with fresher state.
        }
        #endif
    }

    // MARK: - Activation

    private func activate() {
        #if os(iOS)
        guard WCSession.isSupported() else { return }
        let wc = WCSession.default
        wc.delegate = self
        if wc.activationState != .activated {
            wc.activate()
        }
        #endif
    }

    // MARK: - Applying wrist control

    fileprivate func apply(controlData: Data) {
        guard let message = FondCodec.decode(CookControlMessage.self, from: controlData) else { return }
        session?.handleControl(message)
    }
}

#if os(iOS)
extension PhoneSessionRelay: WCSessionDelegate {
    nonisolated func session(
        _ session: WCSession,
        activationDidCompleteWith activationState: WCSessionActivationState,
        error: Error?
    ) {
        // On activation, push the latest snapshot so the Watch is current.
        Task { @MainActor in self.session?.broadcast() }
    }

    nonisolated func sessionDidBecomeInactive(_ session: WCSession) {}

    nonisolated func sessionDidDeactivate(_ session: WCSession) {
        // Reactivate so a re-paired / switched Watch keeps working.
        session.activate()
    }

    nonisolated func session(_ session: WCSession, didReceiveMessage message: [String: Any]) {
        guard let data = message[FondWatchLink.controlMessageKey] as? Data else { return }
        Task { @MainActor in self.apply(controlData: data) }
    }

    nonisolated func session(
        _ session: WCSession,
        didReceiveMessage message: [String: Any],
        replyHandler: @escaping ([String: Any]) -> Void
    ) {
        if let data = message[FondWatchLink.controlMessageKey] as? Data {
            Task { @MainActor in self.apply(controlData: data) }
        }
        replyHandler([:])
    }

    nonisolated func session(_ session: WCSession, didReceiveUserInfo userInfo: [String: Any]) {
        guard let data = userInfo[FondWatchLink.controlMessageKey] as? Data else { return }
        Task { @MainActor in self.apply(controlData: data) }
    }
}
#endif
