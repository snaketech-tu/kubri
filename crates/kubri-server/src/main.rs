//! Runs Kubri in the browser: serves the UI and the API it talks to.

mod api;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use kubri_core::Kubri;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Address to listen on. There is no login yet, so keep it on localhost.
    #[arg(long, env = "KUBRI_ADDR", default_value = "127.0.0.1:7878")]
    addr: SocketAddr,
    /// Directory with the built UI (`pnpm build` in `ui/`).
    #[arg(long, env = "KUBRI_UI_DIR", default_value = "ui/build")]
    ui_dir: PathBuf,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "kubri_server=info,kubri_core=info".into()),
        )
        .init();
    let args = Args::parse();

    let kubri = Arc::new(Kubri::load());

    if !args.addr.ip().is_loopback() {
        tracing::warn!(addr = %args.addr, "listening on a non-loopback address without authentication");
    }
    if !args.ui_dir.join("index.html").exists() {
        tracing::warn!(dir = %args.ui_dir.display(), "UI not found, only the API is served");
    }
    let spa = ServeDir::new(&args.ui_dir).fallback(ServeFile::new(args.ui_dir.join("index.html")));
    let app = Router::new()
        .nest("/api", api::router(kubri))
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(args.addr).await?;
    tracing::info!("Kubri is running on http://{}", args.addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
