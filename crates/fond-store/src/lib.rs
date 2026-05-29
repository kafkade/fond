//! SQLite persistence, migrations, and FTS5 search for fond.
//!
//! The database is a derived index — `.cook` recipe files on disk
//! are the source of truth, and `fond reindex` rebuilds the DB
//! from those files. The database is disposable; the files are sacred.

mod db;
mod error;
mod paths;
pub mod reindex;
mod repo;

pub use db::*;
pub use error::*;
pub use paths::*;
pub use reindex::{ReindexReport, reindex};
pub use repo::*;
