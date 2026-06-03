//! SQLite persistence, migrations, and FTS5 search for fond.
//!
//! The database is a derived index — `.cook` recipe files on disk
//! are the source of truth, and `fond reindex` rebuilds the DB
//! from those files. The database is disposable; the files are sacred.

mod cook_log;
mod db;
mod error;
mod grocery;
mod meal_plan;
mod note;
mod pantry;
mod paths;
mod rating;
pub mod reindex;
mod repo;
mod scoreboard;
mod user;

pub use cook_log::*;
pub use db::*;
pub use error::*;
pub use grocery::*;
pub use meal_plan::*;
pub use note::*;
pub use pantry::*;
pub use paths::*;
pub use rating::*;
pub use reindex::{ReindexReport, reindex};
pub use repo::*;
pub use scoreboard::*;
pub use user::*;
