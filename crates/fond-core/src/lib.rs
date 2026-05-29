//! Shared business logic and services for fond.
//!
//! This crate contains pure application logic — orchestration,
//! validation, and transformations — with no I/O. Storage and
//! network access are provided via trait implementations in
//! downstream crates.

pub use fond_domain::*;
