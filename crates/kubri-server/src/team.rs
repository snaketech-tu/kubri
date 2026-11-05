//! A Kubri shared by a team: every browser that signs in with OpenID Connect gets a Kubri of
//! its own, which acts for the person who signed in. Nothing of one person's connection,
//! caches or watches reaches another.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use kubri_core::{ActAs, Kubri, Person};
use tower::ServiceExt;

use crate::audit::{Audit, AuditLog};
use crate::auth::{Auth, Caller};

/// How often Kubri lets go of the Kubris of sessions that ended.
const PRUNE_EVERY: Duration = Duration::from_secs(60);

/// How people's requests reach the clusters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Credentials {
    /// The server's own credentials, impersonating each person.
    Impersonate,
    /// Each person's own ID token, for clusters that trust the same issuer.
    Token,
}

pub struct Team {
    base: Arc<Kubri>,
    credentials: Credentials,
    auth: Arc<Auth>,
    audit: Arc<AuditLog>,
    /// Where passed-on tokens are kept: a folder only this process's user can read.
    dir: Option<PathBuf>,
    members: Mutex<HashMap<String, Member>>,
    pruned: Mutex<Instant>,
    next: std::sync::atomic::AtomicU64,
}

/// The Kubri of one signed-in browser.
struct Member {
    router: Router,
    /// The folder with the person's token, when Kubri passes it on.
    dir: Option<PathBuf>,
    /// The token last written there.
    token: Option<Arc<str>>,
}

impl Team {
    pub fn new(
        base: Arc<Kubri>,
        credentials: Credentials,
        auth: Arc<Auth>,
        audit: Arc<AuditLog>,
    ) -> Result<Arc<Self>, String> {
        let dir = match credentials {
            Credentials::Impersonate => None,
            Credentials::Token => Some(private_dir(
                std::env::temp_dir().join(format!("kubri-team-{}", crate::auth::random_token())),
            )?),
        };
        Ok(Arc::new(Self {
            base,
            credentials,
            auth,
            audit,
            dir,
            members: Default::default(),
            pruned: Mutex::new(Instant::now()),
            next: Default::default(),
        }))
    }

    /// The router of `session`'s Kubri, made on its first request.
    fn router(&self, session: &str, person: &Person, token: Option<&Arc<str>>) -> Result<Router, String> {
        self.prune();
        let mut members = self.members.lock().unwrap_or_else(|e| e.into_inner());
        if !members.contains_key(session) {
            let member = self.join(person)?;
            members.insert(session.to_owned(), member);
        }
        let member = members.get_mut(session).expect("just added");
        if let (Some(dir), Some(token)) = (&member.dir, token)
            && member.token.as_ref() != Some(token)
        {
            kubri_core::write_private(&dir.join("token"), token).map_err(|e| e.to_string())?;
            member.token = Some(token.clone());
        }
        Ok(member.router.clone())
    }

    fn join(&self, person: &Person) -> Result<Member, String> {
        let (act_as, dir) = match (self.credentials, &self.dir) {
            (Credentials::Token, Some(base)) => {
                let n = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let dir = private_dir(base.join(n.to_string()))?;
                let act_as = ActAs::Token {
                    person: person.clone(),
                    dir: dir.clone(),
                };
                (act_as, Some(dir))
            }
            _ => (ActAs::Impersonate(person.clone()), None),
        };
        let kubri = Arc::new(self.base.for_person(act_as));
        tracing::info!(person = person.name, "a Kubri of their own for {}", person.name);
        let router = crate::api::router(kubri.clone()).layer(axum::middleware::from_fn_with_state(
            Audit {
                log: self.audit.clone(),
                kubri,
            },
            crate::audit::record,
        ));
        Ok(Member {
            router,
            dir,
            token: None,
        })
    }

    /// Lets go of the Kubris of sessions that ended, and of their tokens.
    fn prune(&self) {
        {
            let mut pruned = self.pruned.lock().unwrap_or_else(|e| e.into_inner());
            if pruned.elapsed() < PRUNE_EVERY {
                return;
            }
            *pruned = Instant::now();
        }
        let live = self.auth.live_sessions();
        let mut members = self.members.lock().unwrap_or_else(|e| e.into_inner());
        members.retain(|session, member| {
            let keep = live.contains(session);
            if !keep && let Some(dir) = &member.dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            keep
        });
    }
}

impl Drop for Team {
    fn drop(&mut self) {
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Makes `path`, readable only by this user.
fn private_dir(path: PathBuf) -> Result<PathBuf, String> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    // Fails when the path exists, so nothing else can have put a link there.
    builder
        .create(&path)
        .map_err(|e| format!("cannot make {}: {e}", path.display()))?;
    Ok(path)
}

/// Hands every API request to the Kubri of the browser that sent it.
pub async fn dispatch(State(team): State<Arc<Team>>, request: Request) -> Response {
    let caller = request.extensions().get::<Caller>().cloned().unwrap_or_default();
    let (Some(session), Some(person)) = (&caller.session, &caller.person) else {
        return (StatusCode::UNAUTHORIZED, "Sign in to Kubri first").into_response();
    };
    match team.router(session, person, caller.token.as_ref()) {
        Ok(router) => router.oneshot(request).await.into_response(),
        Err(err) => {
            tracing::error!("{err}");
            (StatusCode::INTERNAL_SERVER_ERROR, err).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;

    fn person(name: &str) -> Person {
        Person {
            name: name.into(),
            groups: vec!["devs".into()],
        }
    }

    fn team(credentials: Credentials) -> Arc<Team> {
        let base = Arc::new(Kubri::from_kubeconfig(Default::default()));
        let auth = Arc::new(Auth::disabled());
        let audit = Arc::new(AuditLog::open(None, vec![]).unwrap());
        Team::new(base, credentials, auth, audit).unwrap()
    }

    fn app(team: Arc<Team>) -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .fallback(dispatch)
            .with_state(team)
    }

    async fn get_as(app: &Router, path: &str, caller: Option<Caller>) -> (StatusCode, String) {
        let mut request = Request::get(path).body(Body::empty()).unwrap();
        if let Some(caller) = caller {
            request.extensions_mut().insert(caller);
        }
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test]
    async fn every_session_gets_a_kubri_acting_for_its_person() {
        let team = team(Credentials::Impersonate);
        let app = app(team.clone());
        assert_eq!(get_as(&app, "/contexts", None).await.0, StatusCode::UNAUTHORIZED);
        let alice = Caller {
            session: Some("s1".into()),
            person: Some(person("alice")),
            token: None,
        };
        // The person's own Kubri answers, with the server's kubeconfig.
        let (status, body) = get_as(&app, "/contexts", Some(alice.clone())).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, body) = get_as(&app, "/terminal/local", Some(alice)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("shared by a team"), "{body}");
        let bob = Caller {
            session: Some("s2".into()),
            person: Some(person("bob")),
            token: None,
        };
        get_as(&app, "/contexts", Some(bob)).await;
        assert_eq!(team.members.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn passed_on_tokens_are_kept_private_and_follow_refreshes() {
        let team = team(Credentials::Token);
        let app = app(team.clone());
        let mut alice = Caller {
            session: Some("s1".into()),
            person: Some(person("alice")),
            token: Some(Arc::from("token-1")),
        };
        get_as(&app, "/contexts", Some(alice.clone())).await;
        let dir = team.members.lock().unwrap()["s1"].dir.clone().unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("token")).unwrap(), "token-1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir), 0o700);
            assert_eq!(mode(&dir.join("token")), 0o600);
            assert_eq!(mode(team.dir.as_ref().unwrap()), 0o700);
        }
        alice.token = Some(Arc::from("token-2"));
        get_as(&app, "/contexts", Some(alice)).await;
        assert_eq!(std::fs::read_to_string(dir.join("token")).unwrap(), "token-2");

        // Ended sessions let go of their Kubri and token.
        *team.pruned.lock().unwrap() = Instant::now() - PRUNE_EVERY;
        team.prune();
        assert!(team.members.lock().unwrap().is_empty());
        assert!(!dir.exists());
        let base = team.dir.clone().unwrap();
        drop(app);
        drop(team);
        assert!(!base.exists());
    }
}
