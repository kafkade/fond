//! UniFFI bindings exposing fond's read + cook-mode functionality to Swift.
//!
//! This crate is a thin boundary layer. It wraps the pure-Rust core
//! (`fond-core`, `fond-domain`, `fond-timeline`) and the SQLite index
//! (`fond-store`) behind a single [`FondClient`] interface object and a set of
//! plain data-transfer records. Foreign-language callers (the SwiftUI apps)
//! never see internal types directly, so the FFI ABI stays decoupled from
//! internal refactors.
//!
//! Scope is intentionally **read + cook mode**. Editing / write-back is
//! deferred to a later iteration.

mod client;
mod dto;
mod error;

pub use client::FondClient;
pub use dto::*;
pub use error::FondError;

uniffi::setup_scaffolding!();
