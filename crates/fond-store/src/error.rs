/// Store-level errors for fond persistence.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Database connection or query error.
    #[error("database error: {message}")]
    Database { message: String },

    /// Migration error.
    #[error("migration error: {message}")]
    Migration { message: String },

    /// I/O error (file reading during reindex).
    #[error("io error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },

    /// Domain-level parse error during reindex.
    #[error("parse error for {file}: {message}")]
    Parse { file: String, message: String },

    /// Encryption/decryption error for the sealed overlay bundle (issue #103).
    #[error("crypto error: {message}")]
    Crypto { message: String },
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database {
            message: e.to_string(),
        }
    }
}
