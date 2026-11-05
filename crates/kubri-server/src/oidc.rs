//! Signing in with OpenID Connect, for a Kubri shared by a team: the authorization code
//! flow with PKCE, a nonce and a state bound to the browser. The ID token's signature,
//! issuer, audience, expiry and nonce are all checked before anyone is let in.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use kubri_core::Person;
use serde::Deserialize;
use serde_json::Value;

/// How long the provider may take to send someone back.
const LOGIN_FOR: Duration = Duration::from_secs(10 * 60);
/// Logins started and not finished that Kubri keeps track of, at most.
const MAX_PENDING: usize = 10_000;
/// Clock differences Kubri accepts between itself and the provider.
const LEEWAY: u64 = 60;
/// The keys are fetched again for an unknown key id at most this often.
const KEYS_AGAIN_AFTER: Duration = Duration::from_secs(60);

/// What the server was started with.
#[derive(Debug, Clone)]
pub struct Config {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    /// Shown on the sign-in button.
    pub name: String,
    /// Empty for `openid email profile`, plus `groups` and `offline_access` where the
    /// provider has them.
    pub scopes: Vec<String>,
    pub username_claim: String,
    pub username_prefix: String,
    pub groups_claim: String,
    pub groups_prefix: String,
    /// Only members of these groups, as the provider names them, may sign in. Empty for all.
    pub allowed_groups: Vec<String>,
    /// Keep refresh tokens, to pass the ID token on to the clusters.
    pub keep_tokens: bool,
    /// A certificate authority for the provider, besides the system's.
    pub ca_file: Option<PathBuf>,
}

/// The provider's endpoints, from its discovery document.
#[derive(Debug, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    #[serde(default)]
    scopes_supported: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_methods_supported: Vec<String>,
}

struct Provider {
    discovery: Discovery,
    keys: Mutex<(JwkSet, Instant)>,
}

/// A login on its way through the provider.
struct Pending {
    nonce: String,
    verifier: String,
    redirect_uri: String,
    started: Instant,
}

/// The tokens of a signed-in person, when Kubri passes the ID token on.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub id_token: String,
    pub refresh_token: Option<String>,
    /// When the ID token expires.
    pub expires: SystemTime,
    subject: String,
}

/// Who signed in, and their tokens when Kubri keeps them.
pub struct SignedIn {
    pub person: Person,
    pub tokens: Option<Tokens>,
}

pub struct Oidc {
    config: Config,
    http: reqwest::Client,
    provider: tokio::sync::Mutex<Option<std::sync::Arc<Provider>>>,
    pending: Mutex<HashMap<String, Pending>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    error_description: Option<String>,
}

impl Oidc {
    pub fn new(config: Config) -> Result<Self, String> {
        // kube brings ring; reqwest uses whatever provider the process has.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none());
        if let Some(file) = &config.ca_file {
            let pem = std::fs::read(file)
                .map_err(|e| format!("cannot read the provider's certificate authority {}: {e}", file.display()))?;
            for certificate in reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|e| format!("cannot read the certificates in {}: {e}", file.display()))?
            {
                http = http.add_root_certificate(certificate);
            }
        }
        let http = http.build().map_err(|e| e.to_string())?;
        Ok(Self {
            config,
            http,
            provider: Default::default(),
            pending: Default::default(),
        })
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// The provider's discovery document and keys, fetched on first use and kept.
    async fn provider(&self) -> Result<std::sync::Arc<Provider>, String> {
        let mut provider = self.provider.lock().await;
        if let Some(provider) = &*provider {
            return Ok(provider.clone());
        }
        let issuer = self.config.issuer.trim_end_matches('/');
        let url = format!("{issuer}/.well-known/openid-configuration");
        let discovery: Discovery = self.get_json(&url).await?;
        // The issuer must be the one configured, exactly, or tokens would not match it.
        if discovery.issuer != self.config.issuer {
            return Err(format!(
                "the provider calls itself {}, not {}: set KUBRI_OIDC_ISSUER to exactly that",
                discovery.issuer, self.config.issuer
            ));
        }
        let keys: JwkSet = self.get_json(&discovery.jwks_uri).await?;
        let found = std::sync::Arc::new(Provider {
            discovery,
            keys: Mutex::new((keys, Instant::now())),
        });
        *provider = Some(found.clone());
        Ok(found)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, String> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| format!("cannot reach the sign-in provider at {url}: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("the sign-in provider answered {} for {url}", response.status()));
        }
        response
            .json()
            .await
            .map_err(|e| format!("cannot read what the sign-in provider sent from {url}: {e}"))
    }

    /// Where to send the browser to sign in, and the state to bind to it.
    pub async fn start(&self, redirect_uri: String) -> Result<(String, String), String> {
        let provider = self.provider().await?;
        let state = random();
        let nonce = random();
        let verifier = random();
        let challenge =
            URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()));
        let scopes = self.scopes(&provider.discovery);
        let mut url = reqwest::Url::parse(&provider.discovery.authorization_endpoint)
            .map_err(|e| format!("the provider's authorization endpoint is not a URL: {e}"))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", &scopes.join(" "))
            .append_pair("state", &state)
            .append_pair("nonce", &nonce)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        let now = Instant::now();
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|_, p| now.duration_since(p.started) < LOGIN_FOR);
        if pending.len() >= MAX_PENDING {
            return Err("too many sign-ins at once, try again in a few minutes".into());
        }
        pending.insert(
            state.clone(),
            Pending {
                nonce,
                verifier,
                redirect_uri,
                started: now,
            },
        );
        Ok((url.into(), state))
    }

    fn scopes(&self, discovery: &Discovery) -> Vec<String> {
        if !self.config.scopes.is_empty() {
            let mut scopes = self.config.scopes.clone();
            if !scopes.iter().any(|s| s == "openid") {
                scopes.insert(0, "openid".into());
            }
            return scopes;
        }
        let mut scopes: Vec<String> = ["openid", "email", "profile"].map(String::from).into();
        let supported = |scope: &str| discovery.scopes_supported.iter().any(|s| s == scope);
        if supported("groups") {
            scopes.push("groups".into());
        }
        if self.config.keep_tokens && supported("offline_access") {
            scopes.push("offline_access".into());
        }
        scopes
    }

    /// Finishes a login the provider sent back with `code`, for the `state` the browser
    /// was bound to.
    pub async fn finish(&self, code: &str, state: &str) -> Result<SignedIn, String> {
        let pending = self
            .pending
            .lock()
            .unwrap()
            .remove(state)
            .filter(|p| p.started.elapsed() < LOGIN_FOR)
            .ok_or("this sign-in took too long or was already used, sign in again")?;
        let provider = self.provider().await?;
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", pending.redirect_uri.as_str()),
            ("code_verifier", pending.verifier.as_str()),
        ];
        let tokens = self.token_request(&provider, &form).await?;
        let id_token = tokens
            .id_token
            .ok_or("the sign-in provider sent no ID token; is the openid scope allowed?")?;
        let claims = self.verify(&provider, &id_token).await?;
        if claims.get("nonce").and_then(Value::as_str) != Some(pending.nonce.as_str()) {
            return Err("the ID token is not for this sign-in (its nonce differs)".into());
        }
        let person = self.person(&claims)?;
        let tokens = self.config.keep_tokens.then(|| Tokens {
            expires: expiry(&claims),
            subject: subject(&claims),
            id_token,
            refresh_token: tokens.refresh_token,
        });
        Ok(SignedIn { person, tokens })
    }

    /// New tokens for `tokens`, before its ID token expires.
    pub async fn refresh(&self, tokens: &Tokens) -> Result<Tokens, String> {
        let refresh_token = tokens
            .refresh_token
            .as_deref()
            .ok_or("the sign-in provider gave no refresh token")?;
        let provider = self.provider().await?;
        let form = [("grant_type", "refresh_token"), ("refresh_token", refresh_token)];
        let fresh = self.token_request(&provider, &form).await?;
        let id_token = fresh
            .id_token
            .ok_or("the sign-in provider sent no new ID token")?;
        let claims = self.verify(&provider, &id_token).await?;
        if subject(&claims) != tokens.subject {
            return Err("the refreshed ID token is for someone else".into());
        }
        Ok(Tokens {
            expires: expiry(&claims),
            subject: tokens.subject.clone(),
            id_token,
            // Providers that rotate refresh tokens send a new one.
            refresh_token: fresh.refresh_token.or_else(|| tokens.refresh_token.clone()),
        })
    }

    async fn token_request(&self, provider: &Provider, form: &[(&str, &str)]) -> Result<TokenResponse, String> {
        let mut form: Vec<(&str, &str)> = form.to_vec();
        form.push(("client_id", &self.config.client_id));
        let mut request = self
            .http
            .post(&provider.discovery.token_endpoint)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(secret) = &self.config.client_secret {
            let methods = &provider.discovery.token_endpoint_auth_methods_supported;
            let post_only = methods.iter().any(|m| m == "client_secret_post")
                && !methods.iter().any(|m| m == "client_secret_basic");
            if post_only {
                form.push(("client_secret", secret));
            } else {
                // RFC 6749 2.3.1: both are form-encoded before they go into the header.
                let user = form_encode(&self.config.client_id);
                let password = form_encode(secret);
                let basic = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
                request = request.header(reqwest::header::AUTHORIZATION, format!("Basic {basic}"));
            }
        }
        let body: String = reqwest::Url::parse_with_params("http://x", &form)
            .map_err(|e| e.to_string())?
            .query()
            .unwrap_or_default()
            .to_owned();
        let response = request
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("cannot reach the sign-in provider: {e}"))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(match serde_json::from_str::<TokenError>(&text) {
                Ok(error) => format!(
                    "the sign-in provider refused: {}",
                    error.error_description.unwrap_or(error.error)
                ),
                Err(_) => format!("the sign-in provider answered {status}"),
            });
        }
        serde_json::from_str(&text).map_err(|e| format!("cannot read the sign-in provider's tokens: {e}"))
    }

    /// The ID token's claims, after checking its signature, issuer, audience and expiry.
    async fn verify(&self, provider: &Provider, token: &str) -> Result<serde_json::Map<String, Value>, String> {
        let header = jsonwebtoken::decode_header(token).map_err(|e| format!("the ID token is not valid: {e}"))?;
        if !ASYMMETRIC.contains(&header.alg) {
            return Err(format!("the ID token is signed with {:?}, which Kubri does not accept", header.alg));
        }
        let jwk = match self.key(provider, header.kid.as_deref(), header.alg) {
            Some(jwk) => jwk,
            None => {
                // The provider may have rotated its keys.
                self.fetch_keys(provider).await?;
                self.key(provider, header.kid.as_deref(), header.alg)
                    .ok_or("the ID token is signed with a key the sign-in provider does not list")?
            }
        };
        let key = DecodingKey::from_jwk(&jwk).map_err(|e| format!("cannot use the provider's key: {e}"))?;
        let mut validation = Validation::new(header.alg);
        validation.leeway = LEEWAY;
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.client_id]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let data = jsonwebtoken::decode::<serde_json::Map<String, Value>>(token, &key, &validation)
            .map_err(|e| format!("the ID token is not valid: {e}"))?;
        let claims = data.claims;
        // With more than one audience, the token must have been issued to Kubri.
        if let Some(Value::Array(audiences)) = claims.get("aud")
            && audiences.len() > 1
            && claims.get("azp").and_then(Value::as_str) != Some(self.config.client_id.as_str())
        {
            return Err("the ID token was issued to another client".into());
        }
        Ok(claims)
    }

    /// The key `kid` names; without a key id, the only key for `alg`.
    fn key(&self, provider: &Provider, kid: Option<&str>, alg: Algorithm) -> Option<Jwk> {
        let keys = provider.keys.lock().unwrap();
        let fits = |jwk: &&Jwk| {
            jwk.common
                .key_algorithm
                .is_none_or(|a| format!("{a:?}") == format!("{alg:?}"))
                && jwk.common.public_key_use.as_ref().is_none_or(|u| {
                    matches!(u, jsonwebtoken::jwk::PublicKeyUse::Signature)
                })
        };
        match kid {
            Some(kid) => keys.0.find(kid).filter(fits).cloned(),
            None => {
                let mut candidates = keys.0.keys.iter().filter(fits);
                let only = candidates.next()?;
                candidates.next().is_none().then(|| only.clone())
            }
        }
    }

    async fn fetch_keys(&self, provider: &Provider) -> Result<(), String> {
        if provider.keys.lock().unwrap().1.elapsed() < KEYS_AGAIN_AFTER {
            return Ok(());
        }
        let keys: JwkSet = self.get_json(&provider.discovery.jwks_uri).await?;
        *provider.keys.lock().unwrap() = (keys, Instant::now());
        Ok(())
    }

    /// The person the claims name, if they may sign in.
    fn person(&self, claims: &serde_json::Map<String, Value>) -> Result<Person, String> {
        let config = &self.config;
        let username = claims
            .get(&config.username_claim)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                format!(
                    "the ID token has no {} claim to name you by; set KUBRI_OIDC_USERNAME_CLAIM to one it has",
                    config.username_claim
                )
            })?;
        if config.username_claim == "email" {
            let verified = claims.get("email_verified");
            if verified.is_some_and(|v| v == &Value::Bool(false) || v == "false") {
                return Err("the sign-in provider has not verified your email address".into());
            }
        }
        let name = format!("{}{username}", config.username_prefix);
        // Kubernetes keeps names starting with `system:` for itself.
        if name.starts_with("system:") {
            return Err(format!("{name} is a name Kubernetes keeps for itself"));
        }
        let raw: Vec<String> = match claims.get(&config.groups_claim) {
            Some(Value::Array(groups)) => groups.iter().filter_map(Value::as_str).map(String::from).collect(),
            Some(Value::String(group)) => vec![group.clone()],
            _ => Vec::new(),
        };
        if !config.allowed_groups.is_empty() && !raw.iter().any(|g| config.allowed_groups.contains(g)) {
            return Err(format!(
                "{username} is in none of the groups allowed to use this Kubri"
            ));
        }
        let groups = raw
            .iter()
            .map(|g| format!("{}{g}", config.groups_prefix))
            .filter(|g| !g.starts_with("system:"))
            .collect();
        Ok(Person { name, groups })
    }
}

const ASYMMETRIC: [Algorithm; 9] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::EdDSA,
];

fn expiry(claims: &serde_json::Map<String, Value>) -> SystemTime {
    let seconds = claims.get("exp").and_then(Value::as_u64).unwrap_or(0);
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn subject(claims: &serde_json::Map<String, Value>) -> String {
    claims.get("sub").and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn form_encode(value: &str) -> String {
    reqwest::Url::parse_with_params("http://x", &[("v", value)])
        .ok()
        .and_then(|u| u.query().map(|q| q.trim_start_matches("v=").to_owned()))
        .unwrap_or_default()
}

/// 32 random bytes, URL-safe: states, nonces and PKCE verifiers.
fn random() -> String {
    crate::auth::random_token()
}
