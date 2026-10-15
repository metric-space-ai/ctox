//! Outbound restrictions for spec adapters: HTTPS only, declared hosts only,
//! no private/loopback targets, no redirects (enforced by the transport).
use anyhow::{bail, Result};
use std::net::IpAddr;

/// Host rule from `allowed_hosts`: exact DNS name or `*.suffix` (one or more labels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRule {
    Exact(String),
    Suffix(String),
}

impl HostRule {
    pub fn parse(raw: &str) -> Result<Self> {
        let r = raw.trim().to_ascii_lowercase();
        let (wild, name) = match r.strip_prefix("*.") {
            Some(rest) => (true, rest.to_string()),
            None => (false, r.clone()),
        };
        if !is_public_dns_name(&name) {
            bail!("allowed host `{raw}` is not a public DNS name");
        }
        if wild && name.split('.').count() < 2 {
            bail!("wildcard host `{raw}` is too broad (needs at least a registrable domain)");
        }
        Ok(if wild { Self::Suffix(name) } else { Self::Exact(name) })
    }

    pub fn matches(&self, host: &str) -> bool {
        let h = host.to_ascii_lowercase();
        match self {
            Self::Exact(n) => h == *n,
            Self::Suffix(s) => h.len() > s.len() + 1 && h.ends_with(&format!(".{s}")),
        }
    }
}

/// A DNS name (no IP literal, no single label, no local/internal suffixes).
pub fn is_public_dns_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 || name.parse::<IpAddr>().is_ok() {
        return false;
    }
    let labels: Vec<&str> = name.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    let ok_labels = labels.iter().all(|l| {
        !l.is_empty()
            && l.len() <= 63
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    let tld = labels.last().copied().unwrap_or("");
    let blocked_tld = matches!(
        tld,
        "local" | "localhost" | "internal" | "lan" | "home" | "corp" | "intranet" | "arpa" | "test" | "invalid" | "example"
    );
    ok_labels && !blocked_tld && !tld.bytes().all(|b| b.is_ascii_digit())
}

/// Non-public address ranges a spec adapter must never reach.
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_broadcast()
                || v.is_documentation()
                || v.is_unspecified()
                || v.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (64..128).contains(&o[1])) // CGNAT
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..20).contains(&o[1]))
                || o[0] >= 240
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80 // link local
                || (s[0] == 0x2001 && s[1] == 0x0db8)
                || v.to_ipv4_mapped().is_some_and(|m| is_forbidden_ip(IpAddr::V4(m)))
        }
    }
}

/// Egress policy of one spec adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressPolicy {
    pub rules: Vec<HostRule>,
}

impl EgressPolicy {
    pub fn allows_host(&self, host: &str) -> bool {
        self.rules.iter().any(|r| r.matches(host))
    }

    /// Validate a fully built request URL before it leaves the hub.
    pub fn check_url(&self, raw: &str) -> Result<()> {
        let u = url::Url::parse(raw).map_err(|e| anyhow::anyhow!("invalid request URL: {e}"))?;
        if u.scheme() != "https" {
            bail!("spec adapters may only call https URLs");
        }
        if !u.username().is_empty() || u.password().is_some() {
            bail!("credentials in the request URL are not allowed");
        }
        if u.port().is_some_and(|p| p != 443) {
            bail!("spec adapters may only use the default https port");
        }
        let host = match u.host() {
            Some(url::Host::Domain(d)) => d.to_ascii_lowercase(),
            _ => bail!("spec adapters may only call DNS hosts, not IP addresses"),
        };
        if !is_public_dns_name(&host) || !self.allows_host(&host) {
            bail!("host `{host}` is not in the adapter's allowed hosts");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_rules() {
        assert!(HostRule::parse("api.todoist.com").unwrap().matches("API.todoist.com"));
        let w = HostRule::parse("*.zendesk.com").unwrap();
        assert!(w.matches("acme.zendesk.com"));
        assert!(!w.matches("zendesk.com"));
        assert!(!w.matches("acme.zendesk.com.evil.io"));
        for bad in ["127.0.0.1", "localhost", "*.com", "intranet.local", "metadata.google.internal", "a..b", "http://x.com"] {
            assert!(HostRule::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn url_checks() {
        let p = EgressPolicy { rules: vec![HostRule::parse("api.todoist.com").unwrap()] };
        p.check_url("https://api.todoist.com/api/v1/user").unwrap();
        for bad in [
            "http://api.todoist.com/x",
            "https://api.todoist.com:8443/x",
            "https://user:pw@api.todoist.com/x",
            "https://evil.com/x",
            "https://127.0.0.1/x",
            "https://[::1]/x",
        ] {
            assert!(p.check_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn forbidden_ips() {
        for ip in ["10.0.0.1", "172.16.3.4", "192.168.1.1", "127.0.0.1", "169.254.169.254", "100.64.0.1", "::1", "fd00::1", "fe80::1", "::ffff:10.0.0.1"] {
            assert!(is_forbidden_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "104.18.1.1", "2606:4700::1111"] {
            assert!(!is_forbidden_ip(ip.parse().unwrap()), "{ip}");
        }
    }
}
