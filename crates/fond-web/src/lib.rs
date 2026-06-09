//! Server-rendered web UI for fond (Axum + HTMX).
//!
//! `fond serve` launches a local Axum HTTP server that provides a
//! browser-based UI for household members who prefer not to use the CLI.
//! All pages are server-rendered with HTMX for dynamic interactions.
//! No authentication — designed for trusted LAN / self-host use.

mod error;
mod filters;
mod routes;
mod state;

pub use state::AppState;

use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Configuration for the web server.
pub struct ServeConfig {
    pub bind: String,
    pub port: u16,
    pub data_dir: PathBuf,
}

/// Start the fond web server.
pub async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    let state = AppState::new(config.data_dir)?;
    let app = build_router(state);

    let addr: SocketAddr = format!("{}:{}", config.bind, config.port).parse()?;
    tracing::info!("fond web UI listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the Axum router with all routes.
fn build_router(state: AppState) -> Router {
    routes::build(state)
}
