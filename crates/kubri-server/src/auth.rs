//! Signing in with a password. Signing in gives the browser a session cookie, which every
//! other API request and socket then needs.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequest, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

const COOKIE: &str = "kubri_session";
/// Wrong passwords from one address before Kubri makes it wait.
const FREE_ATTEMPTS: u32 = 5;
const FIRST_WAIT: Duration = Duration::from_secs(30);
/// Wrong passwords are forgotten after a day without new ones.
const FORGET_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

pub struct Auth {
    password: Option<Password>,
}

struct Password {
    key: hmac::Key,
    tag: hmac::Tag,
    ttl: Duration,
    /// Session id → when it ends.
    sessions: Mutex<HashMap<String, Instant>>,
    failures: Mutex<HashMap<IpAddr, Failures>>,
}

#[derive(Clone, Copy)]
struct Failures {
    count: u32,
    last: Instant,
}

impl Auth {
    /// Everyone who can reach the server may use it.
    pub fn disabled() -> Self {
        Self { password: None }
    }

    /// Signing in with `password` lasts for `ttl`, or until the server restarts.
    pub fn with_password(password: &str, ttl: Duration) -> Self {
        // Comparing HMACs takes the same time however much of a guess is right.
        let key = hmac::Key::generate(hmac::HMAC_SHA256, &SystemRandom::new()).expect("system random");
        let tag = hmac::sign(&key, password.as_bytes());
        Self {
            password: Some(Password {
                key,
                tag,
                ttl,
                sessions: Default::default(),
                failures: Default::default(),
            }),
        }
    }

    pub fn enabled(&self) -> bool {
        self.password.is_some()
    }

    fn signed_in(&self, headers: &HeaderMap) -> bool {
        let Some(password) = &self.password else {
            return true;
        };
        let Some(id) = session_cookie(headers) else {
            return false;
        };
        let now = Instant::now();
        password
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|&ends| ends > now)
    }
}

impl Password {
    fn matches(&self, attempt: &str) -> bool {
        hmac::verify(&self.key, attempt.as_bytes(), self.tag.as_ref()).is_ok()
    }

    fn start_session(&self, now: Instant) -> String {
        let id = random_token();
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, ends| *ends > now);
        sessions.insert(id.clone(), now + self.ttl);
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

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).expect("system random");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == COOKIE).then_some(value)
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

/// Lets only signed-in browsers through.
pub async fn require(State(auth): State<Arc<Auth>>, request: Request, next: Next) -> Response {
    if auth.signed_in(request.headers()) {
        next.run(request).await
    } else {
        error(StatusCode::UNAUTHORIZED, "Sign in to Kubri first")
    }
}

/// `/session`, `/login` and `/logout`, which work without signing in.
pub fn router(auth: Arc<Auth>) -> Router {
    Router::new()
        .route("/session", get(session))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .with_state(auth)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionState {
    /// Whether the server asks for a password at all.
    required: bool,
    signed_in: bool,
}

async fn session(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Json<SessionState> {
    Json(SessionState {
        required: auth.enabled(),
        signed_in: auth.signed_in(&headers),
    })
}

#[derive(Deserialize)]
struct Credentials {
    password: String,
}

async fn login(State(auth): State<Arc<Auth>>, request: Request) -> Response {
    let Some(password) = &auth.password else {
        return error(StatusCode::NOT_FOUND, "This Kubri has no password");
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
    if let Some(wait) = client.and_then(|ip| password.wait(ip, now)) {
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
            password.failed(ip, now);
        }
        tracing::warn!(client = %client.map_or("unknown".into(), |ip| ip.to_string()), "wrong password");
        return error(StatusCode::UNAUTHORIZED, "Wrong password");
    }
    if let Some(ip) = client {
        password.failures.lock().unwrap().remove(&ip);
    }
    let id = password.start_session(now);
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie(&id, password.ttl, secure))],
    )
        .into_response()
}

async fn logout(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Response {
    if let (Some(password), Some(id)) = (&auth.password, session_cookie(&headers)) {
        password.sessions.lock().unwrap().remove(id);
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie("", Duration::ZERO, https(&headers)))],
    )
        .into_response()
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
        let Auth {
            password: Some(password),
        } = Auth::with_password("x", Duration::ZERO)
        else {
            unreachable!()
        };
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
