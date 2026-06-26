# ADR-011: Native Apple Bridge — UniFFI over `fond-core`

**Status**: Accepted
**Date**: 2026-06-26
**Decision**: Expose fond's read + cook-mode functionality to Apple platforms through a single thin `fond-ffi` crate that wraps the existing Rust crates with UniFFI-generated Swift bindings, rather than reimplementing logic in Swift or shipping a separate service.

## Context

fond's roadmap (§9.4, §12, Phase 5) calls for native iOS/iPad/macOS/Watch apps that reuse the proven Rust core instead of forking business logic into Swift. The roadmap flags the native bridge as the project's highest-risk item (🔴) precisely because a bad bridge design would either leak business logic into the UI or impose an ABI that churns with every internal refactor.

The core is already structured for this: `fond-core`/`fond-domain`/`fond-timeline` are pure logic, and `fond-store` owns the SQLite index. The web UI (Phase 4) demonstrated that a thin skin over these crates works — the native bridge is the same idea on a different surface.

Two constraints shaped the design:

1. `rusqlite::Connection` (inside `FondDb`) is `!Send`, but foreign-language objects are shared across threads and must be `Send + Sync`.
2. The first issue is scoped to **read + cook mode** (browse, search, view, scale, timeline); editing/write-back is deferred.

## Decision

Add a dedicated **`fond-ffi`** crate that:

- Exposes a single UniFFI interface object, `FondClient`, constructed from a data-directory path. It holds `Mutex<FondDb>` plus the data dir — the same `!Send` mitigation already used by `fond-web::AppState`.
- Presents **plain `#[uniffi::Record]`/`#[uniffi::Enum]` DTOs** owned by `fond-ffi`, mapped from internal types at the boundary. The foreign ABI is therefore decoupled from internal struct churn.
- Flattens `StoreError`/`DomainError`/`ScaleError` into one `#[uniffi::Error] FondError`.
- Covers read + cook mode: `list_recipes`, `search`, `list_tags`, `get_recipe`, `scale_recipe`, `build_timeline`, `schedule_timeline`, plus `reindex` so apps can rebuild the derived index from seeded `.cook` files.

UniFFI uses the proc-macro approach (no UDL). `apple/build-xcframework.sh` builds static libraries for the Apple targets, generates Swift bindings via an in-crate `uniffi-bindgen` binary, and assembles `Fond.xcframework`. A `FondKit` Swift package wraps the framework; a multiplatform SwiftUI app (`FondApp`) consumes it as a proof of concept.

```text
SwiftUI (FondApp, iOS+macOS) ─► FondKit (SwiftPM) ─► Fond.xcframework
                                                          │  UniFFI scaffolding
                                                          ▼
   fond-ffi ─► fond-store / fond-domain / fond-core / fond-timeline
```

## Rationale

- **No logic in the UI**: Swift calls into the same Rust that powers the CLI and web. Scaling, parsing, and timeline scheduling have one implementation.
- **Stable ABI**: boundary DTOs mean internal refactors don't ripple into generated Swift.
- **Cross-platform safe**: `fond-ffi` is pure Rust and part of `cargo test --workspace`, so it is verified on Linux/macOS/Windows in CI. The Swift/Xcode steps are macOS-toolchain-only and run locally, so they introduce no new CI required check (and thus no `kafkade/github-infra` change).
- **Honours data ownership**: the bridge exposes `reindex`, reinforcing that the SQLite database is a disposable index rebuilt from `.cook` files.
- **De-risks incrementally**: a working macOS + iOS app over the bridge proves the 🔴 item before the iPad and Watch surfaces are built.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|------------------|
| Reimplement logic in Swift | Forks business rules, guarantees drift, abandons the "one core" principle. |
| Expose internal structs directly over FFI | Couples the foreign ABI to every internal refactor. |
| C ABI by hand / `cbindgen` | More boilerplate and error-prone marshalling than UniFFI; no idiomatic Swift types. |
| Local HTTP server consumed by Swift | Heavier runtime, weaker offline story, awkward lifecycle on iOS, no type safety. |
| Make `FondDb` `Send + Sync` internally | Large change to the store for a problem a boundary `Mutex` already solves. |

## Consequences

- New `fond-ffi` workspace crate (`crate-type = ["lib", "staticlib", "cdylib"]`) plus a `uniffi` workspace dependency; the crate is CI-gated like the others.
- New top-level `apple/` directory: `build-xcframework.sh`, `FondKit` package, `FondApp` (XcodeGen-generated), and `SampleData`. Generated artifacts (xcframework, `fond_ffi.swift`, `.xcodeproj`) are git-ignored.
- Building the framework/app requires the Apple toolchain (Xcode, Rust apple targets); the Rust crate alone does not.
- Serialized DB access via `Mutex` is acceptable for single-household, low-concurrency use, matching the web crate's existing trade-off.
- Editing/write-back, iPad-specific layouts, the Watch app, sync, and App Store distribution remain follow-up work.
