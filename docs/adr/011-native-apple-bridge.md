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

**Editing (later iteration).** The write-back surface deferred in the first issue has since landed, extending `fond-ffi` without changing the bridge's shape:

- A pure `CookDocument` edit layer in `fond-domain` splits a `.cook` file into ordered frontmatter + body blocks and re-emits it **byte-for-byte when unedited**, so structured edits round-trip losslessly (principle #4). Metadata edits preserve the body and unknown frontmatter keys; body edits re-serialize only the changed blocks.
- A single-recipe write helper in `fond-store` writes the `.cook` file first (source of truth), then upserts just that recipe's rows in the derived index.
- New `FondClient` methods — `get_recipe_for_edit`, `create_recipe`, `save_recipe`, `save_recipe_source`, `attach_photo`, `delete_recipe`, and `preview_ingredients` — plus `FondError::Conflict`/`AlreadyExists`. Saves carry the base `content_hash` as a lightweight optimistic-concurrency guard; ingredients are edited inline in step text (no separate list, matching Cooklang); photos are content-addressed under `photos/` (ADR-002) and linked via an `image:` frontmatter key. No DB migration — additive-only, honouring the ADR-013 1.0 freeze.

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
- Native recipe editing (create/edit/delete + photo attach) has since landed on the same bridge, writing back to `.cook` files with a lossless Cooklang round-trip (see the Decision "Editing" note above). iPad-specific layouts and the Watch app also landed (ADR-014). The app can also **bind to a user-chosen synced folder** — via a security-scoped bookmark it treats an iCloud Drive / Syncthing-managed `~/fond` as the data dir, reading and writing `.cook` files there and reindexing on external change, so phone edits reach the CLI and other devices (issue #104). Broader multi-device overlay write-back and App Store distribution remain follow-up work.
