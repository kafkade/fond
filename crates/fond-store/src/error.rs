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
    Io { source: std::io::Error },

    /// Domain-level parse error during reindex.
    #[error("parse error for {file}: {message}")]
    Parse { file: String, message: String },

    /// Encryption/decryption error for the sealed overlay bundle (issue #103).
    #[error("crypto error: {message}")]
    Crypto { message: String },
}

impl From<std::io::Error> for StoreError {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database {
            message: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StoreError;
    use std::error::Error;

    #[test]
    fn io_conversion_preserves_display_and_source() {
        let error = StoreError::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ));

        assert_eq!(error.to_string(), "io error: permission denied");
        let source = error
            .source()
            .and_then(|source| source.downcast_ref::<std::io::Error>())
            .expect("I/O error remains available in the source chain");
        assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(source.to_string(), "permission denied");
    }
}
