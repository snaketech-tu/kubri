//! Runs Kubri in the browser: serves the UI and the API it talks to.

mod agents;
mod api;
mod audit;
mod auth;
mod guard;
mod oidc;
mod team;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use clap::Parser;
use kubri_core::Kubri;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use audit::AuditLog;
use auth::Auth;
use team::{Credentials, Team};

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
    /// Never change anything in a cluster: no edits, deletes, scaling, shells or dangerous
    /// plugins. Lists, logs and port-forwards still work.
    #[arg(long, env = "KUBRI_READ_ONLY", value_parser = clap::builder::BoolishValueParser::new())]
    read_only: bool,
    /// Allow local terminals: a shell on this server, with kubectl pointed at the connected
    /// context, for everyone who can sign in. Also lets them add contexts that run a command
    /// to sign in or read certificate and token files on this server. Off by default.
    #[arg(long, env = "KUBRI_LOCAL_TERMINAL", value_parser = clap::builder::BoolishValueParser::new())]
    local_terminal: bool,
    /// How long signing in lasts, in hours (up to ten years).
    #[arg(
        long,
        env = "KUBRI_SESSION_HOURS",
        default_value_t = 168,
        value_parser = clap::value_parser!(u64).range(1..=87_600)
    )]
    session_hours: u64,
    /// Open a snapshot file at start, for looking at a cluster saved by someone else.
    #[arg(long, value_name = "FILE", conflicts_with = "oidc_issuer")]
    snapshot: Option<PathBuf>,
    /// Sign people in with this OpenID Connect provider, for a Kubri shared by a team: each
    /// person then acts with their own Kubernetes permissions. See docs/TEAMS.md.
    #[arg(
        long,
        env = "KUBRI_OIDC_ISSUER",
        requires = "oidc_client_id",
        conflicts_with_all = ["password", "password_file", "no_auth", "local_terminal"]
    )]
    oidc_issuer: Option<String>,
    /// The client ID Kubri has at the provider.
    #[arg(long, env = "KUBRI_OIDC_CLIENT_ID", requires = "oidc_issuer")]
    oidc_client_id: Option<String>,
    /// The client secret Kubri has at the provider.
    #[arg(
        long,
        env = "KUBRI_OIDC_CLIENT_SECRET",
        hide_env_values = true,
        conflicts_with = "oidc_client_secret_file"
    )]
    oidc_client_secret: Option<String>,
    /// A file holding the client secret, such as a Kubernetes secret.
    #[arg(long, env = "KUBRI_OIDC_CLIENT_SECRET_FILE")]
    oidc_client_secret_file: Option<PathBuf>,
    /// The provider's name on the sign-in button. The issuer's host name by default.
    #[arg(long, env = "KUBRI_OIDC_NAME")]
    oidc_name: Option<String>,
    /// The scopes to ask for, separated by commas or spaces. By default `openid email
    /// profile`, with `groups` and `offline_access` where the provider has them.
    #[arg(long, env = "KUBRI_OIDC_SCOPES")]
    oidc_scopes: Option<String>,
    /// The claim that names a person in Kubernetes.
    #[arg(long, env = "KUBRI_OIDC_USERNAME_CLAIM", default_value = "email")]
    oidc_username_claim: String,
    /// Put before every user name, such as `oidc:`, to match the cluster's RoleBindings.
    #[arg(long, env = "KUBRI_OIDC_USERNAME_PREFIX", default_value = "")]
    oidc_username_prefix: String,
    /// The claim with a person's groups.
    #[arg(long, env = "KUBRI_OIDC_GROUPS_CLAIM", default_value = "groups")]
    oidc_groups_claim: String,
    /// Put before every group name, such as `oidc:`.
    #[arg(long, env = "KUBRI_OIDC_GROUPS_PREFIX", default_value = "")]
    oidc_groups_prefix: String,
    /// Only members of these groups may sign in, separated by commas. Everyone the
    /// provider knows may when empty.
    #[arg(long, env = "KUBRI_OIDC_ALLOWED_GROUPS", value_delimiter = ',')]
    oidc_allowed_groups: Vec<String>,
    /// A PEM file with the certificate authority of the provider, if the system does not
    /// trust it.
    #[arg(long, env = "KUBRI_OIDC_CA_FILE")]
    oidc_ca_file: Option<PathBuf>,
    /// How people's requests reach the clusters: `impersonate`, with the server's own
    /// credentials impersonating each person, or `token`, passing on each person's ID token.
    #[arg(
        long,
        env = "KUBRI_TEAM_CREDENTIALS",
        value_enum,
        default_value = "impersonate"
    )]
    team_credentials: Credentials,
    /// The address people reach Kubri at, such as `https://kubri.example.com`, for the
    /// provider to send them back to. Taken from each request when not set.
    #[arg(long, env = "KUBRI_PUBLIC_URL")]
    public_url: Option<String>,
    /// Write every change made through Kubri to this file as JSON lines, or to stdout
    /// with `-`.
    #[arg(long, env = "KUBRI_AUDIT_LOG", value_name = "FILE")]
    audit_log: Option<String>,
    /// Groups whose members see everyone's changes on the Activity panel, separated by
    /// commas. Everyone else sees their own.
    #[arg(long, env = "KUBRI_ACTIVITY_GROUPS", value_delimiter = ',')]
    activity_groups: Vec<String>,
    /// Serve MCP over stdio for an AI agent instead of the web UI. Agents only read this way.
    #[arg(long)]
    mcp: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    // Over stdio, stdout belongs to the protocol.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "kubri_server=info,kubri_core=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

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
        Ok(runtime) if args.mcp => runtime.block_on(serve_mcp()),
        Ok(runtime) => runtime.block_on(serve(args)),
        Err(err) => {
            tracing::error!("{err}");
            ExitCode::FAILURE
        }
    }
}

/// MCP over stdio, for an agent that starts Kubri itself.
async fn serve_mcp() -> ExitCode {
    let kubri = Arc::new(Kubri::load());
    // No Kubri window can confirm a change made this way.
    kubri.set_read_only();
    let hub = Arc::new(kubri_mcp::Hub::for_this_computer());
    let mcp = Arc::new(kubri_mcp::Mcp::new(kubri, kubri_mcp::Transport::Stdio, hub));
    match kubri_mcp::stdio::serve(mcp).await {
        Ok(()) => ExitCode::SUCCESS,
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
    kubri.record_changes();
    if args.read_only {
        kubri.set_read_only();
        tracing::info!("read-only: Kubri does not change clusters");
    }
    if args.local_terminal {
        kubri.allow_local_terminal();
        tracing::warn!(
            "local terminals are on: everyone who can sign in can run a shell on this server, and add contexts that run commands here"
        );
    }
    if let Some(path) = &args.snapshot {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let opened = match std::fs::read(path) {
            Ok(bytes) => kubri.open_snapshot(&bytes, name).await.map_err(|e| e.to_string()),
            Err(err) => Err(err.to_string()),
        };
        if let Err(err) = opened {
            tracing::error!("cannot open {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    }
    if !args.ui_dir.join("index.html").exists() {
        tracing::warn!(dir = %args.ui_dir.display(), "UI not found, only the API is served");
    }
    let audit = match AuditLog::open(args.audit_log.as_deref(), args.activity_groups.clone()) {
        Ok(audit) => Arc::new(audit),
        Err(err) => {
            tracing::error!("{err}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(path) = &args.audit_log {
        tracing::info!(to = path, "writing every change to the audit log");
    }
    let activity = Router::new()
        .route("/activity", axum::routing::get(audit::activity))
        .with_state(audit.clone());
    let (api, endpoint) = if args.oidc_issuer.is_some() {
        let team = match Team::new(kubri, args.team_credentials, auth.clone(), audit) {
            Ok(team) => team,
            Err(err) => {
                tracing::error!("{err}");
                return ExitCode::FAILURE;
            }
        };
        let api = Router::new().fallback(team::dispatch).with_state(team);
        (api.merge(agents::off()), Router::new())
    } else {
        let agents = agents::Agents::new(kubri.clone(), args.addr, args.read_only);
        let record = audit::Audit {
            log: audit,
            kubri: kubri.clone(),
        };
        let api = api::router(kubri)
            .layer(axum::middleware::from_fn_with_state(record, audit::record))
            .merge(agents.router());
        (api, agents.endpoint())
    };
    let api = api
        .merge(activity)
        .layer(axum::middleware::from_fn_with_state(auth.clone(), auth::require))
        .merge(auth::router(auth));
    let app = Router::new()
        .nest("/api", api)
        .merge(endpoint)
        .fallback_service(ui(&args.ui_dir))
        .layer(axum::middleware::from_fn_with_state(guard, guard::check))
        .layer(axum::middleware::from_fn(guard::headers))
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

/// The built UI, with every unknown path answered by `index.html` for the UI's own routes.
/// A missing file under `_app` is a 404, so a page from an older version does not get
/// `index.html` as a script, cached for a year.
fn ui(dir: &Path) -> Router {
    let files = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
    Router::new()
        .nest_service("/_app", ServeDir::new(dir.join("_app")))
        .fallback_service(files)
        .layer(axum::middleware::from_fn(cache_control))
}

/// Files under `_app/immutable` have a hash in their names and never change. The rest,
/// `index.html` first, is checked again on every load, so a new version shows at once.
async fn cache_control(request: Request, next: Next) -> Response {
    let immutable = request.uri().path().starts_with("/_app/immutable/");
    let mut response = next.run(request).await;
    let status = response.status();
    if status.is_success() || status == StatusCode::NOT_MODIFIED {
        let value = if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    }
    response
}

/// Decides whether people sign in, and with which password or provider.
fn auth(args: &Args) -> Result<Auth, String> {
    let ttl = Duration::from_secs(args.session_hours.saturating_mul(3600));
    if let Some(issuer) = &args.oidc_issuer {
        return oidc(args, issuer, ttl);
    }
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

/// Signing in with OpenID Connect, for a Kubri shared by a team.
fn oidc(args: &Args, issuer: &str, ttl: Duration) -> Result<Auth, String> {
    let client_secret = match (&args.oidc_client_secret, &args.oidc_client_secret_file) {
        (Some(secret), _) => Some(secret.clone()),
        (None, Some(file)) => Some(
            std::fs::read_to_string(file)
                .map_err(|err| format!("cannot read the client secret file {}: {err}", file.display()))?
                .trim()
                .to_owned(),
        ),
        (None, None) => None,
    };
    let url = reqwest::Url::parse(issuer).map_err(|e| format!("KUBRI_OIDC_ISSUER is not a URL: {e}"))?;
    if url.scheme() != "https" {
        tracing::warn!("the sign-in provider is not reached over HTTPS: use that for anything but a test");
    }
    if let Some(public_url) = &args.public_url {
        reqwest::Url::parse(public_url).map_err(|e| format!("KUBRI_PUBLIC_URL is not a URL: {e}"))?;
    }
    let scopes = args
        .oidc_scopes
        .as_deref()
        .unwrap_or_default()
        .split([',', ' '])
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let config = oidc::Config {
        issuer: issuer.to_owned(),
        client_id: args.oidc_client_id.clone().unwrap_or_default(),
        client_secret,
        name: args
            .oidc_name
            .clone()
            .unwrap_or_else(|| url.host_str().unwrap_or("your provider").to_owned()),
        scopes,
        username_claim: args.oidc_username_claim.clone(),
        username_prefix: args.oidc_username_prefix.clone(),
        groups_claim: args.oidc_groups_claim.clone(),
        groups_prefix: args.oidc_groups_prefix.clone(),
        allowed_groups: args.oidc_allowed_groups.clone(),
        keep_tokens: args.team_credentials == Credentials::Token,
        ca_file: args.oidc_ca_file.clone(),
    };
    let oidc = oidc::Oidc::new(config)?;
    tracing::info!(
        issuer,
        credentials = ?args.team_credentials,
        "shared by a team: people sign in with OpenID Connect and act with their own permissions"
    );
    Ok(Auth::with_oidc(oidc, ttl, args.public_url.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    #[test]
    fn session_hours_stay_in_range() {
        let hours = |value: &str| {
            Args::try_parse_from(["kubri-server", "--session-hours", value]).map(|a| a.session_hours)
        };
        assert_eq!(hours("1").unwrap(), 1);
        assert_eq!(hours("87600").unwrap(), 87_600);
        assert!(hours("0").is_err());
        assert!(hours("18446744073709551615").is_err());
    }

    #[tokio::test]
    async fn only_hashed_files_are_cached_for_long() {
        let dir = std::env::temp_dir().join(format!("kubri-ui-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("_app/immutable")).unwrap();
        std::fs::write(dir.join("index.html"), "<html></html>").unwrap();
        std::fs::write(dir.join("_app/immutable/app.1234.js"), "").unwrap();
        let cache = |path: &'static str| {
            let ui = ui(&dir);
            async move {
                let request = Request::get(path).body(Body::empty()).unwrap();
                let response = ui.oneshot(request).await.unwrap();
                let value = response.headers().get(header::CACHE_CONTROL).cloned();
                (response.status(), value.map(|v| v.to_str().unwrap().to_owned()))
            }
        };
        let no_cache = (StatusCode::OK, Some("no-cache".to_owned()));
        assert_eq!(cache("/").await, no_cache);
        assert_eq!(cache("/index.html").await, no_cache);
        // A route of the UI, answered with index.html.
        assert_eq!(cache("/pods").await, no_cache);
        assert_eq!(
            cache("/_app/immutable/app.1234.js").await,
            (
                StatusCode::OK,
                Some("public, max-age=31536000, immutable".to_owned())
            )
        );
        // A file of an older version.
        assert_eq!(
            cache("/_app/immutable/app.0000.js").await,
            (StatusCode::NOT_FOUND, None)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
