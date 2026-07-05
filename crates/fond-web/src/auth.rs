//! Optional HTTP Basic Auth gate for the web server.
//!
//! Household-scale protection: a single shared secret (the *token*) supplied via
//! `FOND_AUTH_TOKEN` / `--auth-token`. There are no accounts or roles — any
//! username is accepted and the token is compared, in constant time, against the
//! Basic Auth password. This is designed to ride over TLS (a reverse proxy or
//! fond's native rustls); see the "Self-hosting fond securely" docs.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use subtle::ConstantTimeEq;

/// How the web server authenticates requests.
#[derive(Clone, Debug)]
pub enum AuthConfig {
    /// No authentication — only safe for loopback binds.
    Disabled,
    /// Require HTTP Basic Auth whose password equals this shared token.
    BasicToken(String),
}

impl AuthConfig {
    /// Whether a non-empty token gate is configured.
    pub fn is_enabled(&self) -> bool {
        matches!(self, AuthConfig::BasicToken(t) if !t.is_empty())
    }
}

/// Middleware that enforces Basic Auth against a shared token.
///
/// Returns `401 Unauthorized` with a `WWW-Authenticate` challenge when the
/// `Authorization` header is missing or the credential does not match.
pub async fn require_basic_auth(
    State(token): State<Arc<String>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if token_matches(req.headers().get(header::AUTHORIZATION), token.as_bytes()) {
        next.run(req).await
    } else {
        unauthorized()
    }
}

fn unauthorized() -> Response {
    // Generic by design: the response is byte-for-byte identical whether the
    // Authorization header was missing or the token was wrong, and the challenge
    // never reveals whether auth is configured or hints at the token. It is just
    // the standard Basic challenge.
    let mut resp = (StatusCode::UNAUTHORIZED, "401 Unauthorized").into_response();
    resp.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"fond\", charset=\"UTF-8\""),
    );
    resp
}

/// Constant-time check that the `Authorization` header carries a Basic credential
/// whose password equals `token`. An empty token never matches.
fn token_matches(header_value: Option<&HeaderValue>, token: &[u8]) -> bool {
    if token.is_empty() {
        return false;
    }
    let Some(value) = header_value.and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(encoded) = value
        .strip_prefix("Basic ")
        .or_else(|| value.strip_prefix("basic "))
    else {
        return false;
    };
    let Ok(decoded) = BASE64.decode(encoded.trim()) else {
        return false;
    };
    // Credential is "username:password"; the token is the password. The username
    // is ignored (documented), so split on the first ':'.
    let password = match decoded.iter().position(|&b| b == b':') {
        Some(idx) => &decoded[idx + 1..],
        None => &decoded[..],
    };
    password.ct_eq(token).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_header(user: &str, pass: &str) -> HeaderValue {
        let raw = format!("{user}:{pass}");
        let enc = BASE64.encode(raw.as_bytes());
        HeaderValue::from_str(&format!("Basic {enc}")).unwrap()
    }

    #[test]
    fn matches_correct_password_any_username() {
        let h = basic_header("anyone", "s3cret");
        assert!(token_matches(Some(&h), b"s3cret"));
    }

    #[test]
    fn rejects_wrong_password() {
        let h = basic_header("anyone", "nope");
        assert!(!token_matches(Some(&h), b"s3cret"));
    }

    #[test]
    fn rejects_missing_header() {
        assert!(!token_matches(None, b"s3cret"));
    }

    #[test]
    fn rejects_empty_token() {
        let h = basic_header("anyone", "");
        assert!(!token_matches(Some(&h), b""));
    }

    #[test]
    fn rejects_non_basic_scheme() {
        let h = HeaderValue::from_static("Bearer s3cret");
        assert!(!token_matches(Some(&h), b"s3cret"));
    }

    #[test]
    fn accepts_password_without_colon() {
        // Some clients send just the token with no username/colon.
        let enc = BASE64.encode(b"s3cret");
        let h = HeaderValue::from_str(&format!("Basic {enc}")).unwrap();
        assert!(token_matches(Some(&h), b"s3cret"));
    }
}
