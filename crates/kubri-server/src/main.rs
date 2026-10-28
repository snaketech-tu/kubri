//! Runs Kubri in the browser: serves the UI and the API it talks to.

mod api;
mod auth;
mod guard;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use clap::Parser;
use kubri_core::Kubri;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use auth::Auth;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Address to listen on.
    #[arg(long, env = "KUBRI_ADDR", default_value = "127.0.0.1:7878")]
    addr: SocketAddr,
    /// Directory with the built UI (`pnpm build` in `ui/`).
    #[arg(long, env = "KUBRI_UI_DIR", default_value = "ui/build")]
    ui_dir: PathBuf,
    /// The password to sign in with. Without one, Kubri makes one up when it listens on
    /// other addresses than localhost, and prints it.
    #[arg(
        long,
        env = "KUBRI_PASSWORD",
        hide_env_values = true,
        conflicts_with = "password_file"
    )]
    password: Option<String>,
    /// A file holding the password, such as a Docker or Kubernetes secret.
    #[arg(long, env = "KUBRI_PASSWORD_FILE")]
    password_file: Option<PathBuf>,
    /// Let in everyone who can reach Kubri, without a password. Only for use behind a proxy
    /// that signs people in itself.
    #[arg(long, env = "KUBRI_NO_AUTH", value_parser = clap::builder::BoolishValueParser::new())]
    no_auth: bool,
    /// Host names to answer to without a password, besides localhost and IP addresses,
    /// separated by commas.
    #[arg(long, env = "KUBRI_ALLOWED_HOSTS", value_delimiter = ',')]
    allowed_hosts: Vec<String>,
    /// How long signing in lasts, in hours.
    #[arg(long, env = "KUBRI_SESSION_HOURS", default_value_t = 168)]
    session_hours: u64,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "kubri_server=info,kubri_core=info".into()),
        )
        .init();
    let args = Args::parse();

    match kubri_core::write_in_cluster_kubeconfig() {
        Some(Ok(path)) => {
            tracing::info!(kubeconfig = %path.display(), "running in a pod: using its service account");
            // SAFETY: no other thread exists yet that could read the environment.
            unsafe { std::env::set_var("KUBECONFIG", &path) };
        }
        Some(Err(err)) => tracing::warn!("cannot write a kubeconfig for the pod's service account: {err}"),
        None => {}
    }

    match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime.block_on(serve(args)),
        Err(err) => {
            tracing::error!("{err}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(args: Args) -> ExitCode {
    let auth = match auth(&args) {
        Ok(auth) => Arc::new(auth),
        Err(err) => {
            tracing::error!("{err}");
            return ExitCode::FAILURE;
        }
    };
    if !auth.enabled() && !args.addr.ip().is_loopback() {
        tracing::warn!(
            addr = %args.addr,
            "anyone who can reach this address can use Kubri without a password"
        );
    }
    let guard = Arc::new(guard::Policy {
        check_host: !auth.enabled(),
        allowed_hosts: args.allowed_hosts.clone(),
    });

    let kubri = Arc::new(Kubri::load());
    if !args.ui_dir.join("index.html").exists() {
        tracing::warn!(dir = %args.ui_dir.display(), "UI not found, only the API is served");
    }
    let spa = ServeDir::new(&args.ui_dir).fallback(ServeFile::new(args.ui_dir.join("index.html")));
    let api = api::router(kubri)
        .layer(axum::middleware::from_fn_with_state(auth.clone(), auth::require))
        .merge(auth::router(auth));
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(spa)
        .layer(axum::middleware::from_fn_with_state(guard, guard::check))
        .layer(TraceLayer::new_for_http());

    let listener = match tokio::net::TcpListener::bind(args.addr).await {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!(addr = %args.addr, "cannot listen: {err}");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!("Kubri is running on http://{}", args.addr);
    // Open sockets would keep a graceful shutdown waiting, so stop right away.
    tokio::select! {
        served = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()) => {
            if let Err(err) = served {
                tracing::error!("{err}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        () = shutdown() => ExitCode::SUCCESS,
    }
}

/// Decides whether people sign in, and with which password.
fn auth(args: &Args) -> Result<Auth, String> {
    let ttl = Duration::from_secs(args.session_hours.saturating_mul(3600));
    let password = match (&args.password, &args.password_file) {
        (Some(password), _) => Some(password.clone()),
        (None, Some(file)) => {
            let password = std::fs::read_to_string(file)
                .map_err(|err| format!("cannot read the password file {}: {err}", file.display()))?;
            Some(password.trim_end_matches(['\r', '\n']).to_string())
        }
        (None, None) => None,
    };
    match password {
        Some(_) if args.no_auth => Err("a password and KUBRI_NO_AUTH exclude each other".into()),
        Some(password) if password.is_empty() => Err("the password is empty".into()),
        Some(password) => {
            if password.chars().count() < 12 {
                tracing::warn!("the password is short; use at least 12 characters");
            }
            Ok(Auth::with_password(&password, ttl))
        }
        None if args.no_auth => Ok(Auth::disabled()),
        None if args.addr.ip().is_loopback() => {
            tracing::info!(
                "no password: only this computer can reach Kubri (set KUBRI_PASSWORD to require one)"
            );
            Ok(Auth::disabled())
        }
        None => {
            let password = auth::generate_password();
            tracing::warn!(
                "no password set, so Kubri made one up: {password}  \
                 (set KUBRI_PASSWORD or KUBRI_PASSWORD_FILE to choose your own)"
            );
            Ok(Auth::with_password(&password, ttl))
        }
    }
}

/// Ctrl+C, or `docker stop` and Kubernetes, which send SIGTERM.
async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
