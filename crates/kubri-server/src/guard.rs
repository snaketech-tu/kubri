//! Keeps other websites out. A page on any site could otherwise send requests to Kubri, or
//! reach it under its own domain name pointed at Kubri's address (DNS rebinding), and read
//! the cluster or run plugins.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub struct Policy {
    /// Answer only to names a website cannot point at Kubri: localhost, IP addresses and
    /// `allowed_hosts`. Needed without a password; with one, a rebound site has no session.
    pub check_host: bool,
    pub allowed_hosts: Vec<String>,
}

/// Refuses requests that come from another site, and requests under a name another site
/// could control.
pub async fn check(State(policy): State<Arc<Policy>>, request: Request, next: Next) -> Response {
    let verdict = {
        let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok());
        policy.allows(header("host"), header("x-forwarded-host"), header("origin"))
    };
    match verdict {
        Ok(()) => next.run(request).await,
        Err(reason) => {
            tracing::warn!(uri = %request.uri(), reason, "refused request");
            (StatusCode::FORBIDDEN, reason).into_response()
        }
    }
}

impl Policy {
    pub fn allows(
        &self,
        host: Option<&str>,
        forwarded_host: Option<&str>,
        origin: Option<&str>,
    ) -> Result<(), &'static str> {
        let host = host.map(str::to_ascii_lowercase);
        if self.check_host && !host.as_deref().is_some_and(|h| self.trusted(hostname(h))) {
            return Err(
                "Without a password, Kubri only answers to localhost, IP addresses and the names in KUBRI_ALLOWED_HOSTS",
            );
        }
        // Browsers send Origin with every cross-site request and every WebSocket. Behind a
        // proxy, the name the browser used may come as X-Forwarded-Host, which no web page
        // can set.
        if let Some(origin) = origin {
            let authority = origin
                .strip_prefix("http://")
                .or_else(|| origin.strip_prefix("https://"))
                .map(str::to_ascii_lowercase);
            let forwarded = forwarded_host.map(|h| {
                h.split(',')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase()
            });
            if authority.is_none() || (authority != host && authority != forwarded) {
                return Err("requests from other sites are not allowed");
            }
        }
        Ok(())
    }

    fn trusted(&self, name: &str) -> bool {
        name == "localhost"
            || name.ends_with(".localhost")
            || name.parse::<Ipv4Addr>().is_ok()
            || name
                .strip_prefix('[')
                .and_then(|n| n.strip_suffix(']'))
                .is_some_and(|n| n.parse::<Ipv6Addr>().is_ok())
            || self.allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(name))
    }
}

/// `localhost:7878` → `localhost`, `[::1]:7878` → `[::1]`.
fn hostname(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority.split_inclusive(']').next().unwrap_or(authority);
    }
    authority.split(':').next().unwrap_or(authority)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(check_host: bool) -> Policy {
        Policy {
            check_host,
            allowed_hosts: vec!["Kubri.Internal".into()],
        }
    }

    #[test]
    fn without_a_password_only_names_no_site_can_take_over() {
        let ok = |host, origin| policy(true).allows(Some(host), None, origin).is_ok();
        assert!(ok("127.0.0.1:7878", None));
        assert!(ok("localhost:5173", Some("http://localhost:5173")));
        assert!(ok("app.localhost:7878", None));
        assert!(ok("[::1]:7878", Some("http://[::1]:7878")));
        assert!(ok("192.168.1.20:7878", None));
        assert!(ok("kubri.internal:7878", None));
        // DNS rebinding: an attacker's name that resolves to Kubri's address.
        assert!(!ok("evil.example:7878", None));
        assert!(!ok("localhost.evil.example:7878", None));
        assert!(!ok("[evil.example]:7878", None));
        assert!(policy(true).allows(None, None, None).is_err());
    }

    #[test]
    fn never_from_other_sites() {
        for check_host in [true, false] {
            let ok = |host, origin| policy(check_host).allows(Some(host), None, Some(origin)).is_ok();
            assert!(!ok("127.0.0.1:7878", "https://evil.example"));
            assert!(!ok("127.0.0.1:7878", "null"));
            assert!(!ok("kubri.internal:7878", "http://evil.example"));
            assert!(ok("kubri.internal:7878", "http://kubri.internal:7878"));
        }
    }

    #[test]
    fn with_a_password_any_name_from_the_same_site() {
        let ok = |host, forwarded, origin| policy(false).allows(Some(host), forwarded, origin).is_ok();
        assert!(ok("kubri.example.com", None, None));
        assert!(ok("kubri.example.com", None, Some("https://kubri.example.com")));
        // A proxy that sends its own address as Host, and the browser's in X-Forwarded-Host.
        assert!(ok(
            "10.0.0.5:7878",
            Some("kubri.example.com"),
            Some("https://kubri.example.com")
        ));
        assert!(ok(
            "10.0.0.5:7878",
            Some("kubri.example.com, proxy.internal"),
            Some("https://kubri.example.com")
        ));
        assert!(!ok(
            "10.0.0.5:7878",
            Some("kubri.example.com"),
            Some("https://evil.example")
        ));
    }
}
