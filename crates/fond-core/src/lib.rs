//! Shared business logic and services for fond.
//!
//! This crate contains pure application logic — orchestration,
//! validation, and transformations — with no I/O. Storage and
//! network access are provided via trait implementations in
//! downstream crates.

pub mod ingredient_class;
pub mod quantity;
pub mod scale;
pub mod substitution;

pub use fond_domain::*;
