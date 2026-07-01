import Foundation
import UserNotifications

/// Schedules a local notification per running timer so a firing timer alerts the
/// wrist (haptic) even when the app is backgrounded. When the app is in the
/// foreground the banner is suppressed — `WatchSessionModel`'s ticker plays the
/// haptic and shows the "Done" state, so the alert never doubles up.
@MainActor
final class WatchNotifications: NSObject, UNUserNotificationCenterDelegate {
    private let center = UNUserNotificationCenter.current()
    private let idPrefix = "fond.timer."
    private var scheduled: Set<String> = []

    func requestAuthorization() {
        center.delegate = self
        center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    /// Clear previously scheduled timer notifications and schedule fresh ones for
    /// every running timer at its remaining interval.
    func reschedule(for session: CookSessionPayload) {
        if !scheduled.isEmpty {
            center.removePendingNotificationRequests(withIdentifiers: Array(scheduled))
            scheduled.removeAll()
        }

        for timer in session.timers where timer.state == .running {
            let seconds = timer.deadline.timeIntervalSinceNow
            guard seconds > 0.5 else { continue }

            let content = UNMutableNotificationContent()
            content.title = "Timer done"
            content.body = timer.label
            content.sound = .default

            let trigger = UNTimeIntervalNotificationTrigger(timeInterval: seconds, repeats: false)
            let id = idPrefix + timer.id
            center.add(UNNotificationRequest(identifier: id, content: content, trigger: trigger))
            scheduled.insert(id)
        }
    }

    // Foreground delivery: suppress the system banner; the in-app ticker owns the
    // haptic + visual alert.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([])
    }
}
