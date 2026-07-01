# ADR-014: Apple Watch Companion — Phone-Relayed Cook-Session Payload

**Status**: Accepted
**Date**: 2026-06-30
**Decision**: Ship the watchOS companion as a thin timer/alert/control surface
that renders a plain `Codable` cook-session payload relayed from the phone over
WatchConnectivity, rather than embedding `fond-ffi` (the Rust core) on the Watch.

## Context

Roadmap Phase 5 and issue #75 call for a watchOS companion — "the highest-value
kitchen surface for the timeline engine: glanceable active timers and alerts
while your hands are busy." The acceptance criterion is concrete: *starting a
cook session on the phone/Mac surfaces the same timers on the Watch, and a
firing timer produces a haptic alert.*

The existing native app (ADR-011) already skins the Rust core through `fond-ffi`
and, in cook mode, builds a backward-scheduled timeline (ADR-008) and runs live
kitchen timers on the phone. The open question was how the Watch obtains that
data. Issue #75 explicitly offers two options: *"watchOS target backed by
`fond-ffi` (or a phone-relayed timeline payload)."*

Two forces shaped the choice:

1. The acceptance criterion is inherently a **sync** problem — the phone's
   *running* session must appear on the wrist — which requires a phone↔watch
   channel regardless of whether the Watch can also build timelines itself.
2. The Watch doesn't need search, scaling, or parsing. It needs the *active
   timers* and the *imminent step*.

## Decision

Adopt the **phone-relayed timeline payload**:

- The phone remains the single owner of `FondClient` and the timeline. A new
  `CookSessionModel` couples the `ScheduledTimelineDto` with the existing
  `KitchenTimerModel` and lowers both into a dependency-free `CookSessionPayload`
  (in `apple/Shared/`, shared verbatim by the iOS app, the Watch app, and the
  widget).
- `PhoneSessionRelay` pushes the payload as the WCSession **application context**
  (latest-state, coalescing, delivered when the Watch next wakes) on every
  session change. Wrist controls travel back as `sendMessage`/`transferUserInfo`
  and are applied onto the authoritative phone session, which re-broadcasts.
- Timers are relayed by **absolute deadline** (not remaining seconds), so both
  sides derive countdowns locally and stay correct even if ticks coalesce —
  reusing the phone's existing deadline-based `KitchenTimer` pattern.
- The Watch app (`FondWatch`) schedules a **local notification per running
  timer** at its deadline (so a firing timer alerts even when backgrounded),
  suppresses the foreground banner, and plays a `WKInterfaceDevice` haptic the
  instant a countdown hits zero.
- A WidgetKit extension (`FondWatchWidget`) renders a **"Next up" complication /
  Smart Stack** widget from the latest snapshot the Watch mirrors into a shared
  App Group, using OS-driven `Text(timerInterval:)` for a live countdown with no
  polling.
- The Watch app is embedded in the iOS `Fond` app for the iOS destination only
  (`destinationFilters: [iOS]`); macOS carries no Watch. The `FondWatch` /
  `FondWatchWidget` targets do **not** link the xcframework.

```text
iOS Fond app (owns FondClient)
  ScheduledTimelineDto ──► CookSession (authoritative, running timers)
        │  WCSession.updateApplicationContext        ▲ control messages
        ▼                                            │ (start/pause/advance)
  FondWatch (thin) ─ live countdowns ─ UNNotification ─ haptic ─┘
        │  App Group snapshot
        ▼
  FondWatchWidget ("Next up" complication / Smart Stack)
```

## Rationale

- **Meets the acceptance criterion directly**: the relay *is* the phone→watch
  sync the criterion demands; the haptic path is a pre-scheduled local
  notification plus an in-app zero-crossing haptic.
- **No logic on the Watch**: it renders a payload, matching the "one core"
  principle. Scheduling still has exactly one implementation (the Rust core).
- **Lower risk than embedding the core on watchOS**: avoids adding watchOS Rust
  slices (`arm64_32`/`arm64`/sim) to the xcframework and build script for
  functionality the wrist doesn't need.
- **Battery/notification budget honoured** (issue risk): deadline-based timers +
  pre-scheduled notifications fire without foreground execution; the widget uses
  OS-driven timer text, not wakeups.
- **No CI impact**: like ADR-011, all watchOS work is Xcode-toolchain-only and
  runs locally. The Rust workspace's required `CI` check is untouched, so no
  `kafkade/github-infra` change is required.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Embed `fond-ffi` on the Watch (standalone timelines) | Still needs a phone↔watch channel to satisfy "surfaces the same timers"; adds watchOS Rust slices + build complexity for logic the wrist doesn't need. |
| Relay remaining-seconds instead of deadlines | Drifts as ticks coalesce; deadlines keep both sides exact. |
| `sendMessage` for state broadcast | Requires reachability; `updateApplicationContext` coalesces to latest-state and delivers on wake. |
| Manual foreground haptic only (no notifications) | Misses the backgrounded case — the common one when hands are busy. |
| Multi-entry widget timeline for countdowns | `Text(timerInterval:)` ticks natively; event-driven `WidgetCenter` reloads suffice. |

## Consequences

- New `apple/Shared/` payload module and `apple/FondWatch/` +
  `apple/FondWatchWidget/` targets, wired in `FondApp/project.yml` with an App
  Group and companion Info.plist keys. Generated Info.plist/entitlements are
  git-ignored.
- The iOS app gains an authoritative `CookSessionModel` + `PhoneSessionRelay`;
  cook mode now promotes its schedule into that app-wide session so it keeps
  relaying even when the cook-mode screen isn't visible.
- On unsigned local PoC builds the App Group container isn't provisioned, so the
  widget reads whatever the app last wrote within its own sandbox; the wiring is
  correct for a signed build. WatchConnectivity itself needs no entitlement.
- Editing/write-back, standalone Watch operation, and sync remain follow-up work.
