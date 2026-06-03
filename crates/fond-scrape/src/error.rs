/// Errors from the scraping layer.
#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    /// HTTP request failed.
    #[error("HTTP request failed for {url}: {message}")]
    HttpError { url: String, message: String },

    /// Non-success HTTP status code.
    #[error("HTTP {status} for {url}")]
    HttpStatus { url: String, status: u16 },

    /// Response body is not valid UTF-8.
    #[error("response is not valid UTF-8: {0}")]
    InvalidEncoding(String),

    /// Credential storage error.
    #[error("credential store error: {0}")]
    CredentialError(String),

    /// Request timed out.
    #[error("request timed out for {0}")]
    Timeout(String),
}
