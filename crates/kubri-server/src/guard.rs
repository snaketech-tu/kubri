//! Keeps other websites out. A page on any site could otherwise send requests to Kubri on
//! localhost, or reach it under its own domain name pointed at 127.0.0.1 (DNS rebinding),
//! and read the cluster or run plugins.

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Refuses requests that come from another site, or under a name other than localhost while
/// Kubri only listens on localhost.
pub async fn check(State(loopback): State<bool>, request: Request, next: Next) -> Response {
    let verdict = {
        let header = |name| request.headers().get(name).and_then(|v| v.to_str().ok());
        allowed(header(header::HOST), header(header::ORIGIN), loopback)
    };
    match verdict {
        Ok(()) => next.run(request).await,
        Err(reason) => {
            tracing::warn!(uri = %request.uri(), reason, "refused request");
            (StatusCode::FORBIDDEN, reason).into_response()
        }
    }
}

pub fn allowed(host: Option<&str>, origin: Option<&str>, loopback: bool) -> Result<(), &'static str> {
    let host = host.map(str::to_ascii_lowercase);
    if loopback && !host.as_deref().is_some_and(|h| is_loopback(hostname(h))) {
        return Err("Kubri only answers to localhost");
    }
    // Browsers send Origin with every cross-site request and every WebSocket.
    if let Some(origin) = origin {
        let authority = origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            .map(str::to_ascii_lowercase);
        if authority.is_none() || authority != host {
            return Err("requests from other sites are not allowed");
        }
    }
    Ok(())
}

/// `localhost:7878` → `localhost`, `[::1]:7878` → `[::1]`.
fn hostname(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority.split_inclusive(']').next().unwrap_or(authority);
    }
    authority.split(':').next().unwrap_or(authority)
}

fn is_loopback(name: &str) -> bool {
    name == "localhost"
        || name.ends_with(".localhost")
        || name == "[::1]"
        || name
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lets_in_only_localhost_and_the_same_origin() {
        let ok = |host, origin, loopback| allowed(Some(host), origin, loopback).is_ok();
        assert!(ok("127.0.0.1:7878", None, true));
        assert!(ok("localhost:5173", Some("http://localhost:5173"), true));
        assert!(ok("[::1]:7878", Some("http://[::1]:7878"), true));
        assert!(ok("127.0.0.2:7878", None, true));
        // DNS rebinding: an attacker's name that resolves to 127.0.0.1.
        assert!(!ok("evil.example:7878", None, true));
        assert!(!ok("localhost.evil.example:7878", None, true));
        // Another site posting to localhost.
        assert!(!ok("127.0.0.1:7878", Some("https://evil.example"), true));
        assert!(!ok("127.0.0.1:7878", Some("null"), true));
        assert!(allowed(None, None, true).is_err());
        // Listening on all addresses: any name, but still no other sites.
        assert!(ok("kubri.internal:7878", None, false));
        assert!(ok(
            "kubri.internal:7878",
            Some("http://kubri.internal:7878"),
            false
        ));
        assert!(!ok("kubri.internal:7878", Some("http://evil.example"), false));
    }
}
