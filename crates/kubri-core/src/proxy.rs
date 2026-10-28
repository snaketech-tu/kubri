//! Which cluster connections skip the proxy. kube-rs takes the proxy from `HTTPS_PROXY` but
//! ignores `NO_PROXY`, so a cluster on localhost or the company network would be reached
//! through a proxy that kubectl leaves out. This follows kubectl (Go's `httpproxy`).

use std::net::IpAddr;

use kube::Config;
use kube::config::Kubeconfig;

/// Drops the proxy kube-rs took from `HTTPS_PROXY` when kubectl would not use it for this
/// cluster: localhost, loopback addresses and hosts in `NO_PROXY`. A `proxy-url` in the
/// kubeconfig always counts, as in kubectl.
pub(crate) fn skip_proxy_like_kubectl(config: &mut Config, kubeconfig: &Kubeconfig, context: &str) {
    if config.proxy_url.is_none() || has_proxy_url(kubeconfig, context) {
        return;
    }
    let no_proxy = ["NO_PROXY", "no_proxy"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
    if bypasses_proxy(&config.cluster_url, no_proxy.as_deref()) {
        config.proxy_url = None;
    }
}

fn has_proxy_url(kubeconfig: &Kubeconfig, context: &str) -> bool {
    let cluster = kubeconfig
        .contexts
        .iter()
        .find(|c| c.name == context)
        .and_then(|c| c.context.as_ref())
        .map(|c| c.cluster.as_str());
    kubeconfig
        .clusters
        .iter()
        .find(|c| Some(c.name.as_str()) == cluster)
        .and_then(|c| c.cluster.as_ref())
        .and_then(|c| c.proxy_url.as_deref())
        .is_some_and(|url| !url.is_empty())
}

fn bypasses_proxy(uri: &http::Uri, no_proxy: Option<&str>) -> bool {
    let Some(host) = uri.host() else {
        return false;
    };
    let host = unbracket(host).to_ascii_lowercase();
    let ip = host.parse::<IpAddr>().ok();
    if host == "localhost" || ip.is_some_and(|ip| ip.is_loopback()) {
        return true;
    }
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("http") { 80 } else { 443 });
    no_proxy
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .any(|entry| matches(entry, &host, ip, port))
}

/// One `NO_PROXY` entry: `*`, a network like `10.0.0.0/8`, an address, or a domain that
/// also matches its subdomains (only its subdomains with a leading `.` or `*.`), each with
/// an optional port.
fn matches(entry: &str, host: &str, ip: Option<IpAddr>, port: u16) -> bool {
    if entry == "*" {
        return true;
    }
    if let Some((network, bits)) = entry.split_once('/') {
        return match (ip, network.parse(), bits.parse()) {
            (Some(ip), Ok(network), Ok(bits)) => in_network(ip, network, bits),
            _ => false,
        };
    }
    let (name, entry_port) = split_port(entry);
    if entry_port.is_some_and(|p| p != port) {
        return false;
    }
    let name = unbracket(name).to_ascii_lowercase();
    if let Ok(address) = name.parse::<IpAddr>() {
        return ip == Some(address);
    }
    match name.strip_prefix('*').unwrap_or(&name).strip_prefix('.') {
        Some(domain) => !domain.is_empty() && host.ends_with(&format!(".{domain}")),
        None => host == name || host.ends_with(&format!(".{name}")),
    }
}

fn in_network(ip: IpAddr, network: IpAddr, bits: u32) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(network)) if bits <= 32 => {
            let mask = u32::MAX.checked_shl(32 - bits).unwrap_or(0);
            u32::from(ip) & mask == u32::from(network) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(network)) if bits <= 128 => {
            let mask = u128::MAX.checked_shl(128 - bits).unwrap_or(0);
            u128::from(ip) & mask == u128::from(network) & mask
        }
        _ => false,
    }
}

/// `host:port`, `[v6]:port`, or no port (a bare IPv6 address has colons of its own).
fn split_port(entry: &str) -> (&str, Option<u16>) {
    if entry.parse::<IpAddr>().is_ok() {
        return (entry, None);
    }
    match entry.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(':') => match port.parse() {
            Ok(port) => (name, Some(port)),
            Err(_) => (entry, None),
        },
        _ => (entry, None),
    }
}

fn unbracket(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bypasses(url: &str, no_proxy: &str) -> bool {
        bypasses_proxy(&url.parse().unwrap(), Some(no_proxy))
    }

    #[test]
    fn localhost_never_goes_through_the_proxy() {
        assert!(bypasses("https://127.0.0.1:6443", ""));
        assert!(bypasses("https://127.0.0.53", ""));
        assert!(bypasses("https://[::1]:6443", ""));
        assert!(bypasses("https://localhost:6443", ""));
        assert!(!bypasses("https://10.0.0.1:6443", ""));
        assert!(!bypasses_proxy(&"https://k8s.example.com".parse().unwrap(), None));
    }

    #[test]
    fn domains_match_themselves_and_subdomains() {
        let list = "example.com, .corp.local,*.internal";
        assert!(bypasses("https://example.com", list));
        assert!(bypasses("https://api.k8s.example.com:6443", list));
        assert!(!bypasses("https://notexample.com", list));
        // A leading dot or `*.` matches only subdomains.
        assert!(bypasses("https://k8s.corp.local", list));
        assert!(!bypasses("https://corp.local", list));
        assert!(bypasses("https://a.internal", list));
        assert!(!bypasses("https://internal", list));
        assert!(bypasses("https://API.Example.COM", list));
    }

    #[test]
    fn addresses_and_networks() {
        let list = "192.0.2.10,10.0.0.0/8,fd00::/8,2001:db8::1";
        assert!(bypasses("https://192.0.2.10:6443", list));
        assert!(!bypasses("https://192.0.2.11:6443", list));
        assert!(bypasses("https://10.43.0.1", list));
        assert!(!bypasses("https://11.0.0.1", list));
        assert!(bypasses("https://[fd00::1]:6443", list));
        assert!(bypasses("https://[2001:db8::1]", list));
        assert!(!bypasses("https://[2001:db8::2]", list));
        // A network does not match host names.
        assert!(!bypasses("https://ten.example", "10.0.0.0/8"));
        assert!(bypasses("https://203.0.113.5", "0.0.0.0/0"));
    }

    #[test]
    fn ports_have_to_match_when_given() {
        assert!(bypasses("https://k8s.example.com:6443", "k8s.example.com:6443"));
        assert!(!bypasses("https://k8s.example.com:8443", "k8s.example.com:6443"));
        assert!(bypasses("https://k8s.example.com", "k8s.example.com:443"));
        assert!(bypasses("https://[2001:db8::1]:6443", "[2001:db8::1]:6443"));
        assert!(!bypasses("https://[2001:db8::1]:443", "[2001:db8::1]:6443"));
    }

    #[test]
    fn a_star_skips_the_proxy_for_everything() {
        assert!(bypasses("https://k8s.example.com", "*"));
        assert!(bypasses("https://k8s.example.com", "foo, *"));
        assert!(!bypasses("https://k8s.example.com", " , "));
    }

    #[test]
    fn a_proxy_url_in_the_kubeconfig_wins() {
        let kubeconfig = Kubeconfig::from_yaml(
            "clusters:
- {name: a, cluster: {server: 'https://127.0.0.1:6443', proxy-url: 'http://proxy:3128'}}
- {name: b, cluster: {server: 'https://127.0.0.1:6443'}}
contexts:
- {name: a, context: {cluster: a, user: u}}
- {name: b, context: {cluster: b, user: u}}",
        )
        .unwrap();
        assert!(has_proxy_url(&kubeconfig, "a"));
        assert!(!has_proxy_url(&kubeconfig, "b"));
        assert!(!has_proxy_url(&kubeconfig, "missing"));
    }
}
