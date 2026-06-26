# fond-ffi

UniFFI binding layer that exposes fond's **read + cook-mode** functionality to
Swift (and, in principle, other UniFFI-supported languages).

It is a thin boundary over the pure-Rust core:

```text
fond-ffi ──▶ fond-store (SQLite index)
         ──▶ fond-domain (Recipe parsing/types)
         ──▶ fond-core   (scaling)
         ──▶ fond-timeline (cook-mode DAG + scheduling)
```

The crate exposes a single interface object, [`FondClient`], plus a set of
plain data-transfer records. Callers never touch internal types, so the FFI
ABI stays stable across internal refactors.

## Why a `Mutex`

`FondClient` wraps `Mutex<FondDb>` because `rusqlite::Connection` is `!Send`,
while UniFFI interface objects are `Arc`-wrapped and must be `Send + Sync`.
This mirrors the trade-off already made in `fond-web`'s `AppState` and is fine
for single-household, low-concurrency use.

## Generating Swift bindings

See [`apple/build-xcframework.sh`](../../apple/build-xcframework.sh), which:

1. Builds the static libraries for the Apple targets.
2. Runs the in-crate `uniffi-bindgen` binary (`--features bindgen`) to emit the
   Swift bindings + C headers + module map.
3. Assembles `Fond.xcframework` consumed by the `FondKit` Swift package.

## Scope

Read + cook mode only. Editing / write-back is deferred.
