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
}

#[cfg(feature = "ureq-transport")]
pub struct UreqTransport {
    agent: ureq::Agent,
    restricted: ureq::Agent,
}

/// Resolver for spec adapters: drops every non-public address, fails if none remain.
#[cfg(feature = "ureq-transport")]
fn public_only_resolve(netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
    use std::net::ToSocketAddrs;
    let all: Vec<std::net::SocketAddr> = netloc.to_socket_addrs()?.collect();
    let public: Vec<std::net::SocketAddr> = all
        .into_iter()
        .filter(|a| !crate::spec::egress::is_forbidden_ip(a.ip()))
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
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent(concat!("ctox-toolhub/", env!("CARGO_PKG_VERSION")))
                .build(),
            restricted: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .redirects(0)
                .resolver(public_only_resolve)
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
