# Native Apple apps (UniFFI + SwiftUI)

This directory holds fond's Apple-platform front-ends. They are **skins over the
Rust core** — all recipe logic (parsing, search, scaling, cooking timelines)
lives in `fond-core`/`fond-domain`/`fond-store`/`fond-timeline` and is exposed
to Swift through the [`fond-ffi`](../crates/fond-ffi) crate via
[UniFFI](https://mozilla.github.io/uniffi-rs/). See
[ADR-011](../docs/adr/011-native-apple-bridge.md) for the design.

Scope today: **read + cook mode** (browse, search, view, scale, cooking
timeline) with an **iPad-optimized adaptive layout** (three-column split view +
side-by-side cook mode with live kitchen timers) and a **watchOS companion** for
active cook timers, wrist alerts, and a "Next up" complication. Editing/write-back
and sync are follow-up work.

## watchOS companion (active timers & alerts)

The Watch app is a thin **timer/alert/control surface** — it does *not* embed the
Rust core. The phone stays the single owner of `FondClient` and the timeline;
when you start cook mode there, the app lowers the backward-scheduled
`ScheduledTimelineDto` + its live timers into a plain `Codable`
`CookSessionPayload` (in [`Shared/`](Shared/)) and relays it to the Watch over
**WatchConnectivity** (`updateApplicationContext`). See
[ADR-014](../docs/adr/014-watch-companion-relay.md).

On the wrist you get:

- A live **active-timer list** with countdowns derived from relayed absolute
  deadlines (so both sides stay in sync even if ticks coalesce).
- **Local notifications + haptics** when a step timer fires — pre-scheduled per
  running timer so the alert lands even when the app is backgrounded.
- A **"Next up" complication / Smart Stack widget** (`FondWatchWidget`) showing
  the imminent step or running timer with an OS-driven live countdown.
- **Start / pause / +1 min / cancel / advance / end** controls that send messages
  back to the phone, which mutates the authoritative session and re-broadcasts.

The App Group (`group.dev.kafkade.fond`) shares the latest snapshot from the
Watch app to its widget; on unsigned local PoC builds the group container isn't
provisioned, so the widget reads whatever the app last wrote within its own
sandbox — fully wired for a signed build.

## iPad / adaptive layout

`RootView` uses a three-column `NavigationSplitView` — sidebar
(collections/tags) → recipe list → recipe detail. It adapts automatically:

- **Regular width** (iPad landscape, macOS, wide Stage Manager): all three
  columns visible; cook mode splits into steps beside a live timers + plan panel.
- **Compact width** (iPhone, Slide Over, narrow multitasking): collapses to a
  navigation stack; cook mode falls back to a single scrolling column.

Cook mode timers are real countdowns (start/pause/resume/+1 min/cancel) with a
haptic + visual alert on completion. Selection-driven lists give Magic Keyboard
arrow-key navigation and trackpad hover; ⌘R starts cook mode from a recipe.

## Layout

```text
apple/
  build-xcframework.sh   Build Swift bindings + Fond.xcframework from fond-ffi
  FondKit/               Swift package wrapping the bindings + framework
  FondApp/               Multiplatform SwiftUI app (iOS + macOS) — project.yml
  FondWatch/             watchOS companion app (active timers & alerts)
  FondWatchWidget/       watchOS "Next up" complication / Smart Stack widget
  Shared/                Plain Codable relay payload shared by app + watch + widget
  SampleData/recipes/    Sample .cook files bundled into the app
```

Generated artifacts (`FondKit/xcframework/`, `FondKit/Sources/FondKit/fond_ffi.swift`,
`FondApp/FondApp.xcodeproj`, `SampleData/fond.db`, and the `FondWatch*/Info.plist`
and `*.entitlements` files XcodeGen generates from `project.yml`) are git-ignored —
recreate them with the steps below.

## Prerequisites

- Rust toolchain (`rustup`)
- Xcode 15+ (`xcodebuild`, `lipo`)
- [XcodeGen](https://github.com/yonaskolb/XcodeGen) (`brew install xcodegen`)

## Build & run

```bash
# 1. Build the Rust core into a Swift-ready xcframework + bindings.
#    (Adds the Apple Rust targets and builds release libs for each.)
./apple/build-xcframework.sh

# 2. Generate the Xcode project for the app.
cd apple/FondApp
xcodegen generate

# 3. Build / run.
open FondApp.xcodeproj          # then ⌘R, choosing the My Mac or a Simulator
# …or headless:
xcodebuild -project FondApp.xcodeproj -scheme Fond \
  -destination 'platform=macOS' build
```

To run the **watchOS companion**, pick the `FondWatch` scheme with a paired
iPhone + Apple Watch simulator (the watch app is embedded in the iOS `Fond`
app for the iOS destination). Start cook mode on the phone and the same timers,
"Next up" step, and wrist controls appear on the Watch; a firing timer produces
a haptic alert. The `FondWatch`/`FondWatchWidget` targets don't link the
xcframework, but the shared project still resolves the `FondKit` package, so run
`build-xcframework.sh` first regardless.

On first launch the app copies the bundled `SampleData/recipes/*.cook` into its
Application Support directory and calls `FondClient.reindex()` to (re)build the
SQLite index — reinforcing that the database is a disposable, rebuildable
derivative of the `.cook` source files.

## How the bridge works

`FondClient` is the single entry point. It opens `<dataDir>/fond.db`, runs
migrations, and serves read + cook-mode queries:

```swift
import FondKit

let client = try FondClient(dataDir: dataDir.path)
_ = try client.reindex()                              // rebuild index from files
let recipes = try client.listRecipes(filter: nil)     // [RecipeSummaryDto]
let hits    = try client.search(query: "chicken", filter: nil)
let recipe  = try client.getRecipe(slug: "chicken-adobo")
let scaled  = try client.scaleRecipe(slug: "chicken-adobo",
                                     factor: .multiplier(value: 2))
let plan    = try client.scheduleTimeline(slug: "chicken-adobo",
                                          serveAt: "2026-01-31T18:30:00")
```

Because `rusqlite::Connection` is `!Send`, `FondClient` serializes access behind
a `Mutex` (the same trade-off as the web server) — fine for single-household,
low-concurrency use.

## Regenerating after Rust changes

Any change to the `fond-ffi` public surface (new methods or DTO fields) requires
re-running `./apple/build-xcframework.sh` to refresh the generated Swift before
the app will see it.
