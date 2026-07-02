//! Error type surfaced across the FFI boundary.
//!
//! Internal crate errors (`StoreError`, `DomainError`, `ScaleError`) are
//! flattened into a single [`FondError`] enum so foreign callers get a
//! stable, exhaustive set of cases.

use fond_core::scale::ScaleError;
use fond_domain::DomainError;
use fond_store::StoreError;

/// Errors returned by [`crate::FondClient`] methods.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FondError {
    /// The SQLite index could not be opened, migrated, or queried.
    #[error("database error: {message}")]
    Database { message: String },

    /// A schema migration failed.
    #[error("migration error: {message}")]
    Migration { message: String },

    /// A filesystem error occurred (reading `.cook` files, copying data).
    #[error("io error: {message}")]
    Io { message: String },

    /// A `.cook` file could not be parsed.
    #[error("parse error: {message}")]
    Parse { message: String },

    /// A requested recipe slug does not exist in the index.
    #[error("recipe not found: {slug}")]
    NotFound { slug: String },

    /// A caller-supplied argument was invalid (bad scale factor, unparseable
    /// timestamp, etc).
    #[error("invalid argument: {message}")]
    InvalidArgument { message: String },

    /// The recipe changed on disk since it was loaded (optimistic-concurrency
    /// guard). The caller should reload and retry.
    #[error("conflict: {message}")]
    Conflict { message: String },

    /// A recipe already exists at the target slug/file (e.g. a rename would
    /// clobber another recipe).
    #[error("a recipe already exists with slug: {slug}")]
    AlreadyExists { slug: String },
}

impl From<StoreError> for FondError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Database { message } => FondError::Database { message },
            StoreError::Migration { message } => FondError::Migration { message },
            StoreError::Io { source } => FondError::Io {
                message: source.to_string(),
            },
            StoreError::Parse { file, message } => FondError::Parse {
                message: format!("{file}: {message}"),
            },
        }
    }
}

impl From<DomainError> for FondError {
    fn from(e: DomainError) -> Self {
        FondError::Parse {
            message: e.to_string(),
        }
    }
}

impl From<ScaleError> for FondError {
    fn from(e: ScaleError) -> Self {
        FondError::InvalidArgument {
            message: e.to_string(),
        }
    }
}

impl From<std::io::Error> for FondError {
    fn from(e: std::io::Error) -> Self {
        FondError::Io {
            message: e.to_string(),
        }
    }
}
