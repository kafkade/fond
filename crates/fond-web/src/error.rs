use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

/// Web-layer error type that converts to HTTP responses.
pub struct WebError {
    status: StatusCode,
    message: String,
}

impl WebError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let body = format!(
            r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Error — fond</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>body{{font-family:system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#1a1a1a}}
h1{{color:#b91c1c}}a{{color:#2563eb}}</style></head>
<body><h1>{} {}</h1><p>{}</p><p><a href="/">← Back to recipes</a></p></body></html>"#,
            self.status.as_u16(),
            self.status.canonical_reason().unwrap_or("Error"),
            self.message
        );
        (self.status, Html(body)).into_response()
    }
}

impl From<fond_store::StoreError> for WebError {
    fn from(e: fond_store::StoreError) -> Self {
        Self::internal(format!("Database error: {e}"))
    }
}

impl From<anyhow::Error> for WebError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(format!("{e}"))
    }
}
