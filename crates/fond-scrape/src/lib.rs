//! Web scraping and HTTP fetching for fond.
//!
//! This crate owns all HTTP I/O, session management, and credential storage
//! for fond's web import pipeline. It is deliberately isolated from the core
//! domain and storage crates so that brittle, site-dependent scraping code
//! never destabilises recipe storage, search, or cooking features.
//!
//! # Architecture (ADR-006)
//!
//! - **Schema.org-first**: structured JSON-LD extraction covers the long tail
//!   of recipe websites with minimal per-site maintenance.
//! - **Local-first ethics**: credentials stay on the user's machine via the OS
//!   keychain; imported content is never redistributed.
//! - **Best-effort**: web importers may break when sites change; breakage is
//!   sandboxed here and never affects the core.
//!
//! # Legal boundaries
//!
//! Some recipe services (NYT Cooking, Cook's Illustrated/ATK) explicitly
//! prohibit automated access in their Terms of Service. fond respects these
//! restrictions and does **not** provide authenticated scrapers for those
//! services. See `docs/due-diligence/nyt-atk-scraping-review.md`.

mod client;
mod credentials;
mod error;

pub use client::*;
pub use credentials::*;
pub use error::*;
