//! Paprika recipe importer.
//!
//! Parses `.paprikarecipe` (single gzipped JSON) and `.paprikarecipes`
//! (ZIP archive of gzipped JSONs) export files from the Paprika app.
//!
//! # Format
//!
//! - `.paprikarecipe`:  single gzip-compressed JSON
//! - `.paprikarecipes`: ZIP archive where each entry is a gzipped JSON
//!
//! No encryption, no DRM — standard gzip + ZIP.

mod convert;
mod parse;
mod types;

pub use convert::*;
pub use parse::*;
pub use types::*;
