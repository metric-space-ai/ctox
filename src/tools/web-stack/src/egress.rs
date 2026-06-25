//! SSRF egress guard for the CTOX web stack.
//!
//! Several web-stack paths fetch URLs that originate from an untrusted source:
//! the model-facing `ctox_web_read` tool (the model chooses the URL), evidence
//! pages discovered in a SERP, open-access PDF URLs resolved from a third-party
//! API, and deep-research source snapshots. Without a guard, a model that has
//! been steered by a prompt-injected page (or simply given a hostile task) can
//! make CTOX issue requests to `http://127.0.0.1:<port>/`, RFC1918 hosts, or the
//! `169.254.169.254` cloud-metadata endpoint, and the response body is handed
//! back to the model.
//!
//! The guard has two layers:
//!
//! 1. [`assert_fetchable_url`] — a cheap, early scheme check so non-`http(s)`
//!    URLs (`file://`, `ftp://`, …) fail with a clear message before any I/O.
//! 2. [`SsrfResolver`] — a `ureq` resolver that filters DNS results down to
//!    public addresses at connect time. Because `ureq` re-resolves every
//!    redirect hop through the agent's resolver, this also closes the
//!    DNS-rebinding / redirect-to-internal TOCTOU gap that a pre-flight host
//!    check alone would leave open.
//!
//! Operator-configured, deliberately-internal endpoints (a self-hosted SearXNG
//! instance, or hosts listed in `CTOX_WEB_EGRESS_ALLOW`) are exempted via the
//! resolver's allow-list so legitimate local services keep working.

use std::io;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::path::Path;

use anyhow::anyhow;
use anyhow::Result;
use url::Url;

/// Reject any URL whose scheme is not `http`/`https` before we touch the
/// network. The IP-level guard lives in [`SsrfResolver`]; this is the
/// fast-failing front door for the model-facing read tool.
pub fn assert_fetchable_url(raw: &str) -> Result<()> {
    let parsed =
        Url::parse(raw).map_err(|err| anyhow!("refusing to fetch invalid URL '{raw}': {err}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        other => Err(anyhow!(
            "refusing to fetch URL with non-http(s) scheme '{other}': {raw}"
        )),
    }
}

/// Extract the lower-cased host of a URL (no port, no brackets) for allow-list
/// comparison. Returns `None` if the input does not parse or has no host.
pub fn host_of(raw: &str) -> Option<String> {
    let parsed = Url::parse(raw).ok()?;
    parsed.host_str().map(|host| {
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase()
    })
}

/// Read the operator allow-list from the SQLite runtime config
/// (`CTOX_WEB_EGRESS_ALLOW`, comma-separated host names). Empty by default.
pub fn allow_hosts_from_config(root: &Path) -> Vec<String> {
    crate::runtime_config::get(root, "CTOX_WEB_EGRESS_ALLOW")
        .map(|raw| {
            raw.split(',')
                .map(|part| part.trim().to_ascii_lowercase())
                .filter(|part| !part.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// `ureq` resolver that only returns publicly-routable addresses, except for
/// hosts the operator explicitly allow-listed.
#[derive(Clone)]
pub struct SsrfResolver {
    allow_hosts: Vec<String>,
}

impl SsrfResolver {
    /// Build a resolver. `allow_hosts` are lower-cased host names that bypass
    /// the public-IP filter (e.g. a self-hosted SearXNG, or test mocks).
    pub fn new(allow_hosts: Vec<String>) -> Self {
        Self { allow_hosts }
    }

    fn is_allowed_host(&self, netloc: &str) -> bool {
        if self.allow_hosts.is_empty() {
            return false;
        }
        let host = host_from_netloc(netloc);
        self.allow_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&host))
    }
}

impl ureq::Resolver for SsrfResolver {
    fn resolve(&self, netloc: &str) -> io::Result<Vec<SocketAddr>> {
        let addrs: Vec<SocketAddr> = netloc.to_socket_addrs()?.collect();
        if addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no addresses resolved for '{netloc}'"),
            ));
        }
        if self.is_allowed_host(netloc) {
            return Ok(addrs);
        }
        let public: Vec<SocketAddr> = addrs
            .into_iter()
            .filter(|addr| is_public_ip(addr.ip()))
            .collect();
        if public.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "egress blocked: '{netloc}' resolves only to non-public (loopback/private/link-local/metadata) addresses"
                ),
            ));
        }
        Ok(public)
    }
}

/// Split a `host:port` netloc (possibly bracketed IPv6) into its lower-cased
/// host portion.
fn host_from_netloc(netloc: &str) -> String {
    let trimmed = netloc.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        // Bracketed IPv6, e.g. "[::1]:443".
        if let Some(end) = rest.find(']') {
            return rest[..end].to_ascii_lowercase();
        }
    }
    match trimmed.rsplit_once(':') {
        Some((host, _port)) => host.to_ascii_lowercase(),
        None => trimmed.to_ascii_lowercase(),
    }
}

/// True only for addresses that are safe to dial from a server fetching
/// untrusted URLs. Blocks loopback, private, link-local (incl. the
/// `169.254.169.254` metadata address), shared/CGNAT, unspecified, multicast,
/// and the IPv6 equivalents (ULA `fc00::/7`, link-local `fe80::/10`, plus any
/// IPv4-mapped form of the above).
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_v4(mapped);
            }
            // `to_ipv4()` also covers deprecated IPv4-compatible addresses.
            if let Some(compat) = v6.to_ipv4() {
                return is_public_v4(compat);
            }
            is_public_v6(v6)
        }
    }
}

fn is_public_v4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    // 100.64.0.0/10 — shared address space / carrier-grade NAT (incl. Tailscale).
    let is_shared = octets[0] == 100 && (octets[1] & 0xc0) == 0x40;
    // 0.0.0.0/8 — "this network".
    let is_this_network = octets[0] == 0;
    !(v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_unspecified()
        || v4.is_multicast()
        || is_shared
        || is_this_network)
}

fn is_public_v6(v6: Ipv6Addr) -> bool {
    let segments = v6.segments();
    // fc00::/7 — unique local addresses.
    let is_unique_local = (segments[0] & 0xfe00) == 0xfc00;
    // fe80::/10 — unicast link-local.
    let is_link_local = (segments[0] & 0xffc0) == 0xfe80;
    !(v6.is_loopback()
        || v6.is_unspecified()
        || v6.is_multicast()
        || is_unique_local
        || is_link_local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::net::Ipv6Addr;
    use ureq::Resolver;

    #[test]
    fn blocks_loopback_private_and_metadata_v4() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT / Tailscale
            "0.0.0.0",
        ] {
            let parsed: Ipv4Addr = ip.parse().unwrap();
            assert!(!is_public_ip(IpAddr::V4(parsed)), "{ip} must be blocked");
        }
    }

    #[test]
    fn allows_ordinary_public_v4() {
        for ip in ["1.1.1.1", "8.8.8.8", "93.184.216.34"] {
            let parsed: Ipv4Addr = ip.parse().unwrap();
            assert!(is_public_ip(IpAddr::V4(parsed)), "{ip} must be allowed");
        }
    }

    #[test]
    fn blocks_v6_loopback_ula_linklocal_and_mapped() {
        for ip in [
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ] {
            let parsed: Ipv6Addr = ip.parse().unwrap();
            assert!(!is_public_ip(IpAddr::V6(parsed)), "{ip} must be blocked");
        }
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(assert_fetchable_url("file:///etc/passwd").is_err());
        assert!(assert_fetchable_url("ftp://example.com/x").is_err());
        assert!(assert_fetchable_url("gopher://example.com").is_err());
        assert!(assert_fetchable_url("https://example.com/ok").is_ok());
        assert!(assert_fetchable_url("http://example.com/ok").is_ok());
    }

    #[test]
    fn resolver_blocks_loopback_but_honors_allowlist() {
        let guarded = SsrfResolver::new(Vec::new());
        // Loopback literal resolves only to a blocked address.
        assert!(guarded.resolve("127.0.0.1:80").is_err());

        // Same host is permitted when explicitly allow-listed.
        let allowed = SsrfResolver::new(vec!["127.0.0.1".to_string()]);
        assert!(allowed.resolve("127.0.0.1:80").is_ok());
    }

    #[test]
    fn host_helpers_normalize() {
        assert_eq!(
            host_of("https://Example.COM:8443/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_from_netloc("[::1]:443"), "::1");
        assert_eq!(host_from_netloc("Localhost:7000"), "localhost");
    }
}
