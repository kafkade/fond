/// Domain-level errors for fond.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// A required field was missing or empty.
    #[error("missing required field: {field}")]
    MissingField { field: &'static str },

    /// A value failed validation.
    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: &'static str, reason: String },

    /// Cooklang parsing failed.
    #[error("failed to parse Cooklang: {message}")]
    ParseCooklang { message: String },
}
