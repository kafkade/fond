//! Schema.org/JSON-LD recipe extraction and conversion.
//!
//! Extracts [`SchemaRecipe`] objects from HTML pages using JSON-LD
//! structured data, with a fallback HTML scraper for pages that lack it.
//! Converts extracted recipes to fond domain [`Recipe`]s with generated
//! `.cook` file text.

mod convert;
mod extract;
mod types;

pub use convert::*;
pub use extract::*;
pub use types::*;
