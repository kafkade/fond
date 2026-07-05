//! Integration tests for the Basic Auth middleware used by `fond serve`.
//!
//! These exercise the exact middleware wired into the protected router, without
//! standing up the full app/database: a protected instance must reject
//! unauthenticated requests, while an unprotected (loopback) instance is
//! reachable with no credentials.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use tower::ServiceExt; // for `oneshot`

const TOKEN: &str = "s3cret-token";

fn protected_app() -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(TOKEN.to_string()),
            fond_web::require_basic_auth,
        ))
}

fn open_app() -> Router {
    Router::new().route("/", get(|| async { "ok" }))
}

fn basic_header(user: &str, pass: &str) -> String {
    let enc = BASE64.encode(format!("{user}:{pass}").as_bytes());
    format!("Basic {enc}")
}

#[tokio::test]
async fn rejects_request_without_credentials() {
    let resp = protected_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let challenge = resp
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .expect("WWW-Authenticate header present");
    assert!(challenge.to_str().unwrap().starts_with("Basic"));
}

#[tokio::test]
async fn rejects_wrong_token() {
    let resp = protected_app()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::AUTHORIZATION, basic_header("household", "nope"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn accepts_correct_token_any_username() {
    let resp = protected_app()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::AUTHORIZATION, basic_header("anyone", TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn loopback_path_unaffected_without_auth() {
    // With auth disabled (loopback default), the route is reachable with no header.
    let resp = open_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

/// The 401 must not leak whether credentials were missing vs. wrong — the
/// response (status, challenge header, body) is identical either way, so it
/// never hints at the token or how auth is configured.
#[tokio::test]
async fn unauthorized_response_is_identical_for_missing_and_wrong() {
    use http_body_util::BodyExt;

    async fn parts(req: Request<Body>) -> (StatusCode, String, Vec<u8>) {
        let resp = protected_app().oneshot(req).await.unwrap();
        let status = resp.status();
        let challenge = resp
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let body = resp
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, challenge, body)
    }

    let missing = parts(Request::builder().uri("/").body(Body::empty()).unwrap()).await;
    let wrong = parts(
        Request::builder()
            .uri("/")
            .header(header::AUTHORIZATION, basic_header("user", "wrong"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert_eq!(missing, wrong, "401 responses must be byte-identical");
}
