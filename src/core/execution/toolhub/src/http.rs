//! Minimal synchronous HTTP abstraction so providers and tests share one seam.
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
}

impl HttpRequest {
    pub fn new(method: &str, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
        }
    }
    pub fn header(mut self, k: &str, v: impl Into<String>) -> Self {
        self.headers.insert(k.to_ascii_lowercase(), v.into());
        self
    }
    pub fn form(mut self, pairs: &[(&str, &str)]) -> Self {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        self.body = Some(body.into_bytes());
        self.header("content-type", "application/x-www-form-urlencoded")
    }
    pub fn json(mut self, value: &serde_json::Value) -> Self {
        self.body = Some(serde_json::to_vec(value).unwrap_or_default());
        self.header("content-type", "application/json")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    /// `Mcp-Session-Id` response header (streamable HTTP MCP sessions).
    pub session_id: Option<String>,
}

impl HttpResponse {
    pub fn json(&self) -> anyhow::Result<serde_json::Value> {
        if self.body.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        Ok(serde_json::from_slice(&self.body)?)
    }
}

pub trait HttpTransport: Send + Sync {
    fn send(&self, request: HttpRequest) -> anyhow::Result<HttpResponse>;
    /// Send for runtime (spec) adapters: no redirects are followed and the host
    /// must resolve to public addresses only (the connection is pinned to the
    /// checked addresses). Test transports may keep the default.
    fn send_restricted(&self, request: HttpRequest) -> anyhow::Result<HttpResponse> {
        self.send(request)
    }
    /// Where `url` redirects to (status, absolute `Location`), without following
    /// it; same egress rules as `send_restricted`. Default: unknown (`None`).
    fn redirect_of(&self, url: &str) -> anyhow::Result<Option<(u16, String)>> {
        let _ = url;
        Ok(None)
    }
}

#[cfg(feature = "ureq-transport")]
pub struct UreqTransport {
    agent: ureq::Agent,
    restricted: ureq::Agent,
}

/// Resolver for spec adapters: drops every non-public address, fails if none remain.
#[cfg(feature = "ureq-transport")]
fn public_only_resolve(netloc: &str, fake_ip_egress: bool) -> std::io::Result<Vec<std::net::SocketAddr>> {
    use std::net::ToSocketAddrs;
    let all: Vec<std::net::SocketAddr> = netloc.to_socket_addrs()?.collect();
    let public: Vec<std::net::SocketAddr> = all
        .into_iter()
        .filter(|a| {
            !crate::spec::egress::is_forbidden_ip(a.ip())
                || (fake_ip_egress && crate::spec::egress::is_benchmark_range(a.ip()))
        })
        .collect();
    if public.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "host resolves only to private or reserved addresses",
        ));
    }
    Ok(public)
}

#[cfg(feature = "ureq-transport")]
impl Default for UreqTransport {
    fn default() -> Self {
        Self::with_options(TransportOptions::default())
    }
}

/// Typed transport options (set by the caller from instance config; no env switches).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportOptions {
    /// The host network answers DNS with 198.18.0.0/15 "fake IPs" and routes
    /// them through an egress proxy (sandboxes, some corporate networks). Only
    /// then may spec adapters connect to that range; all other private ranges
    /// stay blocked.
    pub fake_ip_egress: bool,
}

#[cfg(feature = "ureq-transport")]
impl UreqTransport {
    pub fn with_options(options: TransportOptions) -> Self {
        let fake = options.fake_ip_egress;
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent(concat!("ctox-toolhub/", env!("CARGO_PKG_VERSION")))
                .build(),
            restricted: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .redirects(0)
                .resolver(move |netloc: &str| public_only_resolve(netloc, fake))
                .user_agent(concat!("ctox-toolhub/", env!("CARGO_PKG_VERSION")))
                .build(),
        }
    }
}

#[cfg(feature = "ureq-transport")]
impl HttpTransport for UreqTransport {
    fn send(&self, request: HttpRequest) -> anyhow::Result<HttpResponse> {
        send_with(&self.agent, request)
    }
    fn send_restricted(&self, request: HttpRequest) -> anyhow::Result<HttpResponse> {
        send_with(&self.restricted, request)
    }
    fn redirect_of(&self, url: &str) -> anyhow::Result<Option<(u16, String)>> {
        let response = match self.restricted.get(url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => return Err(anyhow::anyhow!("http transport error: {e}")),
        };
        let status = response.status();
        if !(300..400).contains(&status) {
            return Ok(None);
        }
        let Some(location) = response.header("location") else { return Ok(None) };
        let absolute = url::Url::parse(url).and_then(|base| base.join(location)).map(|u| u.to_string()).unwrap_or_else(|_| location.to_string());
        Ok(Some((status, absolute)))
    }
}

#[cfg(feature = "ureq-transport")]
fn send_with(agent: &ureq::Agent, request: HttpRequest) -> anyhow::Result<HttpResponse> {
    {
        let mut req = agent.request(&request.method, &request.url);
        for (k, v) in &request.headers {
            req = req.set(k, v);
        }
        let result = match &request.body {
            Some(body) => req.send_bytes(body),
            None => req.call(),
        };
        let response = match result {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => return Err(anyhow::anyhow!("http transport error: {e}")),
        };
        let status = response.status();
        let session_id = response.header("mcp-session-id").map(String::from);
        let mut body = Vec::new();
        std::io::Read::read_to_end(
            &mut response.into_reader().take(16 * 1024 * 1024),
            &mut body,
        )?;
        Ok(HttpResponse {
            status,
            body,
            session_id,
        })
    }
}

#[cfg(feature = "ureq-transport")]
use std::io::Read as _;
