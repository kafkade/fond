/// Import-level errors for fond.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// I/O error reading the source file.
    #[error("failed to read import file: {0}")]
    Io(#[from] std::io::Error),

    /// The source file is not a valid ZIP archive.
    #[error("invalid archive: {0}")]
    InvalidArchive(String),

    /// Gzip decompression failed for an entry.
    #[error("gzip decompression failed for {entry}: {message}")]
    GzipError { entry: String, message: String },

    /// JSON deserialization failed for an entry.
    #[error("JSON parse failed for {entry}: {message}")]
    JsonError { entry: String, message: String },

    /// HTTP fetch failed.
    #[error("HTTP fetch failed for {url}: {message}")]
    HttpError { url: String, message: String },
}
