//! Server-rendered web UI for fond (Axum + HTMX).
//!
//! `fond serve` launches a local Axum HTTP server that provides a
//! browser-based UI for household members who prefer not to use the CLI.
//! All pages are server-rendered with HTMX for dynamic interactions.
//!
//! For anything beyond a loopback bind, protect it: enable HTTP Basic Auth
//! ([`AuthConfig::BasicToken`]) and terminate TLS — either natively via
//! [`TlsConfig`] or at a reverse proxy. See the "Self-hosting fond securely"
//! guide in the mdBook docs.

mod auth;
mod error;
mod filters;
mod routes;
mod state;

pub use auth::AuthConfig;
pub use state::AppState;

#[doc(hidden)]
pub use auth::require_basic_auth;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;

/// TLS material for native HTTPS termination.
#[derive(Clone, Debug)]
pub struct TlsConfig {
    /// PEM-encoded certificate chain.
    pub cert: PathBuf,
    /// PEM-encoded private key.
    pub key: PathBuf,
}

/// Configuration for the web server.
pub struct ServeConfig {
    pub bind: String,
    pub port: u16,
    pub data_dir: PathBuf,
    /// How incoming requests are authenticated.
    pub auth: AuthConfig,
    /// When set, serve over HTTPS with the given certificate/key.
    pub tls: Option<TlsConfig>,
}

/// Start the fond web server.
pub async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    let state = AppState::new(config.data_dir)?;
    let app = build_router(state, &config.auth);

    let addr: SocketAddr = format!("{}:{}", config.bind, config.port).parse()?;
    let scheme = if config.tls.is_some() {
        "https"
    } else {
        "http"
    };
    let auth_state = if config.auth.is_enabled() {
        "enabled (Basic Auth)"
    } else {
        "disabled"
    };
    tracing::info!("fond web UI listening on {scheme}://{addr} (auth: {auth_state})");

    match config.tls {
        Some(tls) => {
            let rustls_config =
                axum_server::tls_rustls::RustlsConfig::from_pem_file(&tls.cert, &tls.key)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "failed to load TLS certificate/key ({} / {}): {e}",
                            tls.cert.display(),
                            tls.key.display()
                        )
                    })?;
            axum_server::bind_rustls(addr, rustls_config)
                .serve(app.into_make_service())
                .await?;
        }
        None => {
            axum_server::bind(addr)
                .serve(app.into_make_service())
                .await?;
        }
    }
    Ok(())
}

/// Build the Axum router with all routes, applying auth middleware if configured.
fn build_router(state: AppState, auth: &AuthConfig) -> Router {
    let app = routes::build(state);
    match auth {
        AuthConfig::BasicToken(token) if !token.is_empty() => {
            let token = Arc::new(token.clone());
            app.layer(axum::middleware::from_fn_with_state(
                token,
                auth::require_basic_auth,
            ))
        }
        _ => app,
    }
}
