//! Signing in, with a password or with OpenID Connect. Signing in gives the browser a
//! session cookie, which every other API request and socket then needs.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use axum::extract::{ConnectInfo, FromRequest, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use kubri_core::Person;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::oidc::{Oidc, Tokens};

const COOKIE: &str = "kubri_session";
/// Binds a sign-in with OpenID Connect to the browser that started it.
const LOGIN_COOKIE: &str = "kubri_oidc";
/// Wrong passwords from one address before Kubri makes it wait.
const FREE_ATTEMPTS: u32 = 5;
const FIRST_WAIT: Duration = Duration::from_secs(30);
/// Wrong passwords are forgotten after a day without new ones.
const FORGET_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// A passed-on ID token is refreshed this long before it expires.
const REFRESH_BEFORE: Duration = Duration::from_secs(2 * 60);
/// After a failed refresh, Kubri waits this long before trying again.
const REFRESH_AGAIN_AFTER: Duration = Duration::from_secs(30);

pub struct Auth {
    method: Method,
    ttl: Duration,
    /// Session id → the session.
    sessions: Mutex<HashMap<String, Session>>,
    failures: Mutex<HashMap<IpAddr, Failures>>,
    /// One token refresh at a time.
    refreshing: tokio::sync::Mutex<()>,
    /// Where the browser reaches Kubri, for the provider to send it back to.
    public_url: Option<String>,
}

enum Method {
    /// Everyone who can reach the server may use it.
    Open,
    Password(Password),
    Oidc(Oidc),
}

struct Password {
    key: hmac::Key,
    tag: hmac::Tag,
}

#[derive(Clone)]
struct Session {
    ends: Instant,
    /// Who signed in, with OpenID Connect.
    person: Option<Person>,
    /// Their tokens, when Kubri passes the ID token on.
    tokens: Option<Tokens>,
    /// When a refresh last failed.
    refresh_failed: Option<Instant>,
}

/// Who sent a request: the session and the person signed in with it. `require` puts it on
/// every request it lets through.
#[derive(Clone, Default)]
pub struct Caller {
    pub session: Option<String>,
    pub person: Option<Person>,
    /// The ID token to pass on, when Kubri does.
    pub token: Option<Arc<str>>,
}

#[derive(Clone, Copy)]
struct Failures {
    count: u32,
    last: Instant,
}

impl Auth {
    /// Everyone who can reach the server may use it.
    pub fn disabled() -> Self {
        Self::with(Method::Open, Duration::ZERO)
    }

    /// Signing in with `password` lasts for `ttl`, or until the server restarts.
    pub fn with_password(password: &str, ttl: Duration) -> Self {
        // Comparing HMACs takes the same time however much of a guess is right.
        let key = hmac::Key::generate(hmac::HMAC_SHA256, &SystemRandom::new()).expect("system random");
        let tag = hmac::sign(&key, password.as_bytes());
        Self::with(Method::Password(Password { key, tag }), ttl)
    }

    /// Signing in with an OpenID Connect provider lasts for `ttl`, or, when Kubri passes the
    /// ID token on, until it can no longer be refreshed.
    pub fn with_oidc(oidc: Oidc, ttl: Duration, public_url: Option<String>) -> Self {
        let mut auth = Self::with(Method::Oidc(oidc), ttl);
        auth.public_url = public_url.map(|u| u.trim_end_matches('/').to_owned());
        auth
    }

    fn with(method: Method, ttl: Duration) -> Self {
        Self {
            method,
            ttl,
            sessions: Default::default(),
            failures: Default::default(),
            refreshing: Default::default(),
            public_url: None,
        }
    }

    pub fn enabled(&self) -> bool {
        !matches!(self.method, Method::Open)
    }

    /// The sessions that have not ended.
    pub fn live_sessions(&self) -> HashSet<String> {
        let now = Instant::now();
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, s| s.ends > now);
        sessions.keys().cloned().collect()
    }

    /// Who sent a request with these headers, if they signed in.
    async fn caller(&self, headers: &HeaderMap) -> Option<Caller> {
        if !self.enabled() {
            return Some(Caller::default());
        }
        let id = session_cookie(headers)?;
        let session = self.session(id)?;
        let session = match (&self.method, &session.tokens) {
            (Method::Oidc(oidc), Some(tokens)) if due(tokens) => self.refresh(oidc, id).await?,
            _ => session,
        };
        Some(Caller {
            session: Some(id.to_owned()),
            person: session.person,
            token: session.tokens.map(|t| Arc::from(t.id_token)),
        })
    }

    fn session(&self, id: &str) -> Option<Session> {
        let now = Instant::now();
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .filter(|s| s.ends > now)
            .cloned()
    }

    /// The session with a fresh ID token, or `None` when it ended because the token
    /// expired and could not be refreshed.
    async fn refresh(&self, oidc: &Oidc, id: &str) -> Option<Session> {
        let _one = self.refreshing.lock().await;
        // Another request may have refreshed it meanwhile.
        let session = self.session(id)?;
        let tokens = session.tokens.as_ref()?;
        if !due(tokens)
            || tokens.refresh_token.is_none()
            || session.refresh_failed.is_some_and(|t| t.elapsed() < REFRESH_AGAIN_AFTER)
        {
            return self.unless_expired(id, session);
        }
        match oidc.refresh(tokens).await {
            Ok(fresh) => {
                let mut sessions = self.sessions.lock().unwrap();
                let session = sessions.get_mut(id)?;
                session.tokens = Some(fresh);
                session.refresh_failed = None;
                Some(session.clone())
            }
            Err(err) => {
                tracing::warn!("cannot refresh the ID token of {}: {err}", person_name(&session));
                if let Some(session) = self.sessions.lock().unwrap().get_mut(id) {
                    session.refresh_failed = Some(Instant::now());
                }
                self.unless_expired(id, session)
            }
        }
    }

    /// Ends a session whose ID token expired.
    fn unless_expired(&self, id: &str, session: Session) -> Option<Session> {
        let expired = session
            .tokens
            .as_ref()
            .is_some_and(|t| t.expires <= SystemTime::now());
        if expired {
            tracing::info!("the sign-in of {} ended with its ID token", person_name(&session));
            self.sessions.lock().unwrap().remove(id);
            return None;
        }
        Some(session)
    }

    fn start_session(&self, now: Instant, person: Option<Person>, tokens: Option<Tokens>) -> String {
        let id = random_token();
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, s| s.ends > now);
        sessions.insert(
            id.clone(),
            Session {
                ends: now + self.ttl,
                person,
                tokens,
                refresh_failed: None,
            },
        );
        id
    }

    /// How long `client` must wait before trying again, after too many wrong passwords.
    fn wait(&self, client: IpAddr, now: Instant) -> Option<Duration> {
        let failures = *self.failures.lock().unwrap().get(&client)?;
        if failures.count < FREE_ATTEMPTS || now.duration_since(failures.last) > FORGET_AFTER {
            return None;
        }
        // 30 seconds, doubling with every further wrong password up to 16 minutes.
        let wait = FIRST_WAIT * 2u32.pow((failures.count - FREE_ATTEMPTS).min(5));
        (failures.last + wait)
            .checked_duration_since(now)
            .filter(|w| !w.is_zero())
    }

    fn failed(&self, client: IpAddr, now: Instant) {
        let mut failures = self.failures.lock().unwrap();
        if failures.len() > 10_000 {
            failures.retain(|_, f| now.duration_since(f.last) <= FORGET_AFTER);
        }
        let entry = failures.entry(client).or_insert(Failures { count: 0, last: now });
        if now.duration_since(entry.last) > FORGET_AFTER {
            entry.count = 0;
        }
        entry.count += 1;
        entry.last = now;
    }

    /// Where the provider sends the browser back to.
    fn redirect_uri(&self, headers: &HeaderMap) -> Option<String> {
        let base = match &self.public_url {
            Some(url) => url.clone(),
            None => {
                let host = headers.get(header::HOST)?.to_str().ok()?;
                let scheme = if https(headers) { "https" } else { "http" };
                format!("{scheme}://{host}")
            }
        };
        Some(format!("{base}/api/auth/callback"))
    }
}

impl Password {
    fn matches(&self, attempt: &str) -> bool {
        hmac::verify(&self.key, attempt.as_bytes(), self.tag.as_ref()).is_ok()
    }
}

/// Whether a passed-on ID token should be refreshed now.
fn due(tokens: &Tokens) -> bool {
    match tokens.expires.duration_since(SystemTime::now()) {
        Ok(left) => left <= REFRESH_BEFORE,
        Err(_) => true,
    }
}

fn person_name(session: &Session) -> String {
    session.person.as_ref().map_or_else(|| "someone".into(), |p| p.name.clone())
}

/// The address limits apply to. One IPv6 user usually has a whole /64 network to pick from.
fn client_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !(u64::MAX as u128))),
        },
        v4 => v4,
    }
}

/// A password for when none is set: 24 letters and digits, about 143 bits.
pub fn generate_password() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let random = SystemRandom::new();
    let mut password = String::with_capacity(24);
    while password.len() < 24 {
        let mut byte = [0u8];
        random.fill(&mut byte).expect("system random");
        // 248 is the largest multiple of 62 below 256, so every character is equally likely.
        if byte[0] < 248 {
            password.push(ALPHABET[byte[0] as usize % ALPHABET.len()] as char);
        }
    }
    password
}

pub(crate) fn random_token() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).expect("system random");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    read_cookie(headers, COOKIE)
}

fn read_cookie<'a>(headers: &'a HeaderMap, cookie: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == cookie).then_some(value)
        })
}

/// Whether the browser reached Kubri over HTTPS, directly or through a proxy.
fn https(headers: &HeaderMap) -> bool {
    let header = |name| headers.get(name).and_then(|v| v.to_str().ok());
    header("x-forwarded-proto").is_some_and(|p| p.split(',').next().unwrap_or("").trim() == "https")
        || header(header::ORIGIN.as_str()).is_some_and(|o| o.starts_with("https://"))
}

fn cookie(value: &str, max_age: Duration, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{COOKIE}={value}; Path=/; Max-Age={}; HttpOnly; SameSite=Strict{secure}",
        max_age.as_secs()
    )
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

/// Lets only signed-in browsers through, and tells the handlers who sent the request.
pub async fn require(State(auth): State<Arc<Auth>>, mut request: Request, next: Next) -> Response {
    match auth.caller(request.headers()).await {
        Some(caller) => {
            request.extensions_mut().insert(caller);
            next.run(request).await
        }
        None => error(StatusCode::UNAUTHORIZED, "Sign in to Kubri first"),
    }
}

/// `/session`, `/login`, `/logout` and the OpenID Connect sign-in, which work without
/// signing in.
pub fn router(auth: Arc<Auth>) -> Router {
    Router::new()
        .route("/session", get(session))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/auth/login", get(oidc_login))
        .route("/auth/callback", get(oidc_callback))
        .with_state(auth)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionState {
    /// Whether the server asks people to sign in at all.
    required: bool,
    signed_in: bool,
    /// `password` or `oidc`, when people sign in.
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'static str>,
    /// The name of the OpenID Connect provider, for the sign-in button.
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    /// Who signed in, on a Kubri shared by a team.
    #[serde(skip_serializing_if = "Option::is_none")]
    person: Option<Person>,
}

async fn session(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Json<SessionState> {
    let caller = auth.caller(&headers).await;
    let (method, provider) = match &auth.method {
        Method::Open => (None, None),
        Method::Password(_) => (Some("password"), None),
        Method::Oidc(oidc) => (Some("oidc"), Some(oidc.name().to_owned())),
    };
    Json(SessionState {
        required: auth.enabled(),
        signed_in: caller.is_some(),
        method,
        provider,
        person: caller.and_then(|c| c.person),
    })
}

#[derive(Deserialize)]
struct Credentials {
    password: String,
}

async fn login(State(auth): State<Arc<Auth>>, request: Request) -> Response {
    let password = match &auth.method {
        Method::Password(password) => password,
        Method::Oidc(oidc) => {
            return error(
                StatusCode::NOT_FOUND,
                format!("This Kubri signs in with {}", oidc.name()),
            );
        }
        Method::Open => return error(StatusCode::NOT_FOUND, "This Kubri has no password"),
    };
    let client = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| client_key(c.0.ip()));
    let secure = https(request.headers());
    let credentials = match Json::<Credentials>::from_request(request, &()).await {
        Ok(Json(credentials)) => credentials,
        Err(rejection) => return rejection.into_response(),
    };

    let now = Instant::now();
    if let Some(wait) = client.and_then(|ip| auth.wait(ip, now)) {
        let seconds = wait.as_secs_f64().ceil() as u64;
        let mut response = error(
            StatusCode::TOO_MANY_REQUESTS,
            format!("Too many wrong passwords. Try again in {}.", in_words(seconds)),
        );
        response.headers_mut().insert(header::RETRY_AFTER, seconds.into());
        return response;
    }
    if !password.matches(&credentials.password) {
        if let Some(ip) = client {
            auth.failed(ip, now);
        }
        tracing::warn!(client = %client.map_or("unknown".into(), |ip| ip.to_string()), "wrong password");
        return error(StatusCode::UNAUTHORIZED, "Wrong password");
    }
    if let Some(ip) = client {
        auth.failures.lock().unwrap().remove(&ip);
    }
    let id = auth.start_session(now, None, None);
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie(&id, auth.ttl, secure))],
    )
        .into_response()
}

async fn logout(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Response {
    if let Some(id) = session_cookie(&headers)
        && let Some(session) = auth.sessions.lock().unwrap().remove(id)
        && session.person.is_some()
    {
        tracing::info!("{} signed out", person_name(&session));
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie("", Duration::ZERO, https(&headers)))],
    )
        .into_response()
}

/// Sends the browser to the OpenID Connect provider.
async fn oidc_login(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Response {
    let Method::Oidc(oidc) = &auth.method else {
        return error(StatusCode::NOT_FOUND, "This Kubri does not sign in with OpenID Connect");
    };
    let Some(redirect_uri) = auth.redirect_uri(&headers) else {
        return error(StatusCode::BAD_REQUEST, "No Host header to send you back to");
    };
    match oidc.start(redirect_uri).await {
        Ok((url, state)) => {
            let binding = format!(
                "{LOGIN_COOKIE}={state}; Path=/api/auth; Max-Age=600; HttpOnly; SameSite=Lax{}",
                if https(&headers) { "; Secure" } else { "" }
            );
            redirect(&url, [binding])
        }
        Err(err) => {
            tracing::warn!("cannot start signing in: {err}");
            back_with_error(&err, &headers)
        }
    }
}

#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Where the provider sends the browser back to, with a code to sign in with.
async fn oidc_callback(
    State(auth): State<Arc<Auth>>,
    headers: HeaderMap,
    Query(callback): Query<Callback>,
) -> Response {
    let Method::Oidc(oidc) = &auth.method else {
        return error(StatusCode::NOT_FOUND, "This Kubri does not sign in with OpenID Connect");
    };
    let signed_in = async {
        if let Some(error) = callback.error {
            return Err(format!(
                "the sign-in provider said: {}",
                callback.error_description.unwrap_or(error)
            ));
        }
        let (Some(code), Some(state)) = (callback.code, callback.state) else {
            return Err("the sign-in provider sent no code".to_owned());
        };
        // The state must be the one this browser started with, so nobody can sign someone
        // else in with their own account.
        if read_cookie(&headers, LOGIN_COOKIE) != Some(state.as_str()) {
            return Err("this sign-in was started in another browser or took too long, sign in again".into());
        }
        oidc.finish(&code, &state).await
    }
    .await;
    match signed_in {
        Ok(signed_in) => {
            tracing::info!(
                person = signed_in.person.name,
                groups = ?signed_in.person.groups,
                "signed in"
            );
            let id = auth.start_session(Instant::now(), Some(signed_in.person), signed_in.tokens);
            let secure = https(&headers);
            redirect(
                &home(&auth),
                [cookie(&id, auth.ttl, secure), forget_login(secure)],
            )
        }
        Err(err) => {
            tracing::warn!("signing in failed: {err}");
            back_with_error(&err, &headers)
        }
    }
}

/// The page to go to after signing in.
fn home(auth: &Auth) -> String {
    match &auth.public_url {
        Some(url) => format!("{url}/"),
        None => "/".into(),
    }
}

fn forget_login(secure: bool) -> String {
    format!(
        "{LOGIN_COOKIE}=; Path=/api/auth; Max-Age=0; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

/// Back to the sign-in page, which shows `message`.
fn back_with_error(message: &str, headers: &HeaderMap) -> Response {
    let query = reqwest::Url::parse_with_params("http://x/", &[("sign-in-error", message)])
        .ok()
        .and_then(|u| u.query().map(str::to_owned))
        .unwrap_or_default();
    redirect(&format!("/?{query}"), [forget_login(https(headers))])
}

fn redirect<const N: usize>(location: &str, cookies: [String; N]) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    if let Ok(location) = HeaderValue::from_str(location) {
        headers.insert(header::LOCATION, location);
    }
    for cookie in cookies {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            headers.append(header::SET_COOKIE, value);
        }
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn in_words(seconds: u64) -> String {
    match seconds {
        0..=1 => "a second".into(),
        2..=59 => format!("{seconds} seconds"),
        60 => "a minute".into(),
        _ => format!("{} minutes", seconds.div_ceil(60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn app(auth: Auth) -> Router {
        let auth = Arc::new(auth);
        Router::new()
            .route("/secret", get(|| async { "cluster" }))
            .layer(axum::middleware::from_fn_with_state(auth.clone(), require))
            .merge(router(auth))
    }

    fn login_request(password: &str, client: &str) -> Request<Body> {
        let mut request = Request::post("/login")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "password": password }).to_string(),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo::<SocketAddr>(client.parse().unwrap()));
        request
    }

    fn with_cookie(uri: &str, cookie: &str) -> Request<Body> {
        Request::get(uri)
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap()
    }

    async fn status(app: &Router, request: Request<Body>) -> StatusCode {
        app.clone().oneshot(request).await.unwrap().status()
    }

    /// Signs in and returns the cookie to send back.
    async fn sign_in(app: &Router, password: &str) -> String {
        let response = app
            .clone()
            .oneshot(login_request(password, "10.0.0.1:5000"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let set_cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(set_cookie.contains("HttpOnly; SameSite=Strict"));
        assert!(!set_cookie.contains("Secure"));
        set_cookie.split(';').next().unwrap().to_string()
    }

    #[tokio::test]
    async fn without_a_password_everything_is_open() {
        let app = app(Auth::disabled());
        assert_eq!(
            status(&app, Request::get("/secret").body(Body::empty()).unwrap()).await,
            StatusCode::OK
        );
        let response = app
            .clone()
            .oneshot(Request::get("/session").body(Body::empty()).unwrap())
            .await;
        let bytes = axum::body::to_bytes(response.unwrap().into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(bytes, r#"{"required":false,"signedIn":true}"#);
        assert_eq!(
            status(&app, login_request("x", "10.0.0.1:5000")).await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn signing_in_opens_the_api_until_signing_out() {
        let app = app(Auth::with_password("hunter2", Duration::from_secs(3600)));
        assert_eq!(
            status(&app, Request::get("/secret").body(Body::empty()).unwrap()).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(&app, login_request("hunter3", "10.0.0.1:5000")).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(&app, with_cookie("/secret", "kubri_session=made-up")).await,
            StatusCode::UNAUTHORIZED
        );

        let cookie = sign_in(&app, "hunter2").await;
        assert_eq!(
            status(&app, with_cookie("/secret", &cookie)).await,
            StatusCode::OK
        );
        assert_eq!(
            status(&app, with_cookie("/secret", &format!("theme=dark; {cookie}"))).await,
            StatusCode::OK
        );

        let logout = Request::post("/logout")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(logout).await.unwrap();
        assert!(
            response.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert_eq!(
            status(&app, with_cookie("/secret", &cookie)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn sessions_end() {
        let app = app(Auth::with_password("hunter2", Duration::ZERO));
        let cookie = sign_in(&app, "hunter2").await;
        assert_eq!(
            status(&app, with_cookie("/secret", &cookie)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn guessing_gets_slower() {
        let app = app(Auth::with_password("hunter2", Duration::from_secs(3600)));
        for _ in 0..FREE_ATTEMPTS {
            assert_eq!(
                status(&app, login_request("guess", "10.0.0.9:1")).await,
                StatusCode::UNAUTHORIZED
            );
        }
        // Even the right password has to wait now, from that address.
        let response = app
            .clone()
            .oneshot(login_request("hunter2", "10.0.0.9:2"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "30");
        // The same /64 network counts as one address.
        assert_eq!(
            status(&app, login_request("guess", "[2001:db8::1]:1")).await,
            StatusCode::UNAUTHORIZED
        );
        for _ in 1..FREE_ATTEMPTS {
            status(&app, login_request("guess", "[2001:db8::1]:1")).await;
        }
        assert_eq!(
            status(&app, login_request("hunter2", "[2001:db8::ffff]:1")).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        // Others are not held up.
        sign_in(&app, "hunter2").await;
    }

    #[test]
    fn waits_double() {
        let password = Auth::with_password("x", Duration::ZERO);
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let start = Instant::now();
        for _ in 0..FREE_ATTEMPTS + 2 {
            password.failed(ip, start);
        }
        assert_eq!(password.wait(ip, start), Some(Duration::from_secs(120)));
        assert_eq!(
            password.wait(ip, start + Duration::from_secs(100)),
            Some(Duration::from_secs(20))
        );
        assert_eq!(password.wait(ip, start + Duration::from_secs(120)), None);
        assert_eq!(
            password.wait(ip, start + FORGET_AFTER + Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn secure_cookies_over_https() {
        let mut headers = HeaderMap::new();
        assert!(!https(&headers));
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        assert!(https(&headers));
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "https://kubri.example.com".parse().unwrap());
        assert!(https(&headers));
        assert!(cookie("abc", Duration::from_secs(60), true).ends_with("; Secure"));
    }

    #[test]
    fn made_up_passwords_are_long_and_plain() {
        let password = generate_password();
        assert_eq!(password.len(), 24);
        assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(password, generate_password());
    }

    #[test]
    fn waits_in_words() {
        assert_eq!(in_words(1), "a second");
        assert_eq!(in_words(30), "30 seconds");
        assert_eq!(in_words(60), "a minute");
        assert_eq!(in_words(61), "2 minutes");
    }
}
