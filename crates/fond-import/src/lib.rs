//! Import pipeline for fond — source-specific adapters that parse external
//! recipe formats and produce fond domain [`Recipe`]s with `.cook` file text.
//!
//! Each adapter (Paprika, schema.org, etc.) implements parsing and conversion
//! independently, but all converge on the same output: a domain `Recipe` plus
//! generated Cooklang text ready to be written to disk.
//!
//! This crate is intentionally I/O-free for persistence — it reads source
//! files but never writes `.cook` files or touches SQLite. The CLI or
//! calling code handles file writing and indexing.

mod error;
pub mod ocr;
pub mod paprika;
mod pipeline;
pub mod schema_org;

pub use error::*;
pub use pipeline::*;
