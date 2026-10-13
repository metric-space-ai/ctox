//! Remote MCP connectors (Cursor/Claude-style "Add connector").
//!
//! DE: CTOX ist intern MCP-Client der offiziellen Remote-MCP-Server der Anbieter.
//! Anmeldung per MCP-Autorisierung (OAuth 2.1 + PKCE, Discovery über
//! `.well-known/oauth-protected-resource` und `oauth-authorization-server`,
//! Dynamic Client Registration nach RFC 7591). Keine Client-Registrierung durch
//! den Betreiber, wo der Anbieter DCR anbietet. Tokens liegen im Secret-Store.
//! Agenten nutzen die Tools weiterhin über `ctox toolhub exec <server>.<tool>`.
//!
//! EN: CTOX acts as MCP client of the providers' official remote MCP servers.
//! Auth follows the MCP authorization spec (OAuth 2.1 + PKCE, RFC 9728 / RFC 8414
//! discovery, RFC 7591 DCR, RFC 8707 resource indicators). Tokens live in the
//! encrypted secret store; agents call tools via the chainable CLI.
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::http::{HttpRequest, HttpResponse, HttpTransport};
use crate::model::LocalizedText;
use crate::pkce;
use crate::store::SecretBackend;

pub const SCOPE_PREFIX: &str = "toolhub-mcp:";
const PROTOCOL_VERSION: &str = "2025-06-18";
const CLIENT_NAME: &str = "CTOX Toolhub";
pub const PENDING_TTL_SECS: u64 = 15 * 60;

/// How a server's OAuth client is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientRegistration {
    /// RFC 7591 DCR: zero configuration, like Cursor/Claude connectors.
    Dynamic,
    /// Provider has no DCR: an operator-registered client must be configured once
    /// (`ctox toolhub mcp-client-set`).
    Preregistered,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteServer {
    pub slug: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub description: LocalizedText,
    pub registration: ClientRegistration,
    /// Scopes requested when the server metadata does not announce any.
    pub default_scopes: &'static [&'static str],
    /// Extra authorize parameters (Google needs offline access for refresh tokens).
    pub extra_authorize_params: &'static [(&'static str, &'static str)],
}

/// Built-in official remote MCP servers (verified 2026-10-13).
pub fn registry() -> Vec<RemoteServer> {
    vec![
        RemoteServer {
            slug: "cloudflare",
            name: "Cloudflare",
            url: "https://mcp.cloudflare.com/mcp",
            description: LocalizedText::new(
                "Cloudflare-API (Zonen, DNS, Workers, R2 …) über den offiziellen MCP-Server.",
                "Cloudflare API (zones, DNS, Workers, R2 …) via the official MCP server.",
            ),
            registration: ClientRegistration::Dynamic,
            default_scopes: &["offline_access", "user:read", "account:read"],
            extra_authorize_params: &[],
        },
        RemoteServer {
            slug: "cloudflare-bindings",
            name: "Cloudflare Workers Bindings",
            url: "https://bindings.mcp.cloudflare.com/mcp",
            description: LocalizedText::new(
                "Workers, KV, R2, D1 und Hyperdrive verwalten.",
                "Manage Workers, KV, R2, D1 and Hyperdrive.",
            ),
            registration: ClientRegistration::Dynamic,
            default_scopes: &[],
            extra_authorize_params: &[],
        },
        RemoteServer {
            slug: "vercel",
            name: "Vercel",
            url: "https://mcp.vercel.com",
            description: LocalizedText::new(
                "Projekte, Deployments, Logs und Doku auf Vercel.",
                "Projects, deployments, logs and docs on Vercel.",
            ),
            registration: ClientRegistration::Dynamic,
            default_scopes: &[],
            extra_authorize_params: &[],
        },
        RemoteServer {
            slug: "slack",
            name: "Slack",
            url: "https://mcp.slack.com/mcp",
            description: LocalizedText::new(
                "Nachrichten, Kanäle und Suche in Slack.",
                "Messages, channels and search in Slack.",
            ),
            registration: ClientRegistration::Preregistered,
            default_scopes: &[
                "search:read.public",
                "channels:history",
                "groups:history",
                "im:history",
                "mpim:history",
                "chat:write",
                "users:read",
                "users:read.email",
                "canvases:read",
                "canvases:write",
            ],
            extra_authorize_params: &[],
        },
        RemoteServer {
            slug: "gmail",
            name: "Gmail",
            url: "https://gmailmcp.googleapis.com/mcp/v1",
            description: LocalizedText::new(
                "E-Mails in Gmail suchen, lesen und entwerfen.",
                "Search, read and draft Gmail messages.",
            ),
            registration: ClientRegistration::Preregistered,
            default_scopes: &[
                "https://www.googleapis.com/auth/gmail.readonly",
                "https://www.googleapis.com/auth/gmail.compose",
            ],
            extra_authorize_params: &[("access_type", "offline"), ("prompt", "consent")],
        },
    ]
}

pub fn server(slug: &str) -> Result<RemoteServer> {
    registry()
        .into_iter()
        .find(|s| s.slug == slug)
        .ok_or_else(|| anyhow!("unknown MCP server `{slug}`"))
}

/// OAuth authorization-server metadata subset (RFC 8414).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthServerMeta {
    pub issuer: Option<String>,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Discovery {
    /// RFC 8707 resource indicator.
    pub resource: String,
    pub auth: AuthServerMeta,
    /// Scopes announced by the protected resource / WWW-Authenticate.
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Registered OAuth client (DCR result or operator-provided). Stored as secret.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientInfo {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub redirect_uri: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Pending {
    state: String,
    verifier: String,
    created_at: u64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_at: Option<u64>,
}

/// Non-secret connection status (shown in Workjet "Schnittstellen").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub server: String,
    pub project: String,
    pub status: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    pub updated_at: u64,
    #[serde(default)]
    pub tool_count: Option<usize>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// One remote tool, as announced by `tools/list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteTool {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(default)]
    pub annotations: Value,
}

impl RemoteTool {
    /// MCP `readOnlyHint` absent or false => treated as mutating (approval needed).
    pub fn mutating(&self) -> bool {
        self.annotations
            .get("readOnlyHint")
            .and_then(Value::as_bool)
            != Some(true)
    }
}

/// Connect request returned to Workjet: open `authorize_url` in an in-app window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpConnectRequest {
    pub server: String,
    pub project: String,
    pub authorize_url: String,
    pub redirect_uri: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum McpOutcome {
    Success {
        server: String,
        tool: String,
        result: Value,
    },
    ToolError {
        server: String,
        tool: String,
        result: Value,
    },
    CredentialRequired {
        server: String,
        project: String,
        reason: LocalizedText,
    },
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn scope_of(server: &str, project: &str) -> String {
    format!("{SCOPE_PREFIX}{server}:{project}")
}
fn client_scope(server: &str) -> String {
    format!("{SCOPE_PREFIX}{server}")
}

/// Parse `resource_metadata="…"` and `scope="…"` from a WWW-Authenticate header.
pub fn parse_www_authenticate(h: &str) -> (Option<String>, Vec<String>) {
    let param = |name: &str| {
        let key = format!("{name}=\"");
        h.find(&key).and_then(|i| {
            let rest = &h[i + key.len()..];
            rest.find('"').map(|j| rest[..j].to_string())
        })
    };
    let scopes = param("scope")
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();
    (param("resource_metadata"), scopes)
}

/// Parse a JSON-RPC response that may arrive as JSON or as an SSE stream.
pub fn parse_rpc_body(body: &[u8], id: u64) -> Result<Value> {
    let text = String::from_utf8_lossy(body);
    let mut candidates = Vec::new();
    if text.trim_start().starts_with('{') {
        candidates.push(serde_json::from_str::<Value>(text.trim())?);
    } else {
        let mut data = String::new();
        for line in text.lines().chain(std::iter::once("")) {
            if let Some(d) = line.strip_prefix("data:") {
                data.push_str(d.trim_start());
            } else if line.trim().is_empty() && !data.is_empty() {
                if let Ok(v) = serde_json::from_str::<Value>(&data) {
                    candidates.push(v);
                }
                data.clear();
            }
        }
    }
    let msg = candidates
        .into_iter()
        .find(|v| v.get("id").and_then(Value::as_u64) == Some(id))
        .ok_or_else(|| anyhow!("MCP server returned no response for request {id}"))?;
    if let Some(err) = msg.get("error") {
        bail!(
            "MCP error: {}",
            err.get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
    }
    Ok(msg.get("result").cloned().unwrap_or(Value::Null))
}

pub struct RemoteMcp<'a, B: SecretBackend> {
    pub backend: &'a B,
    pub transport: &'a dyn HttpTransport,
    pub redirect_uri: String,
}

enum Rpc {
    Ok(Value),
    Unauthorized,
}

impl<'a, B: SecretBackend> RemoteMcp<'a, B> {
    fn get_json(&self, url: &str) -> Result<Option<Value>> {
        let r = self
            .transport
            .send(HttpRequest::new("GET", url).header("accept", "application/json"))?;
        Ok(if (200..300).contains(&r.status) {
            r.json().ok()
        } else {
            None
        })
    }

    /// RFC 9728 + RFC 8414 discovery, starting from an unauthenticated probe.
    pub fn discover(&self, srv: &RemoteServer) -> Result<Discovery> {
        let mut scopes: Vec<String> = Vec::new();
        let u = url::Url::parse(srv.url)?;
        let origin = format!("{}://{}", u.scheme(), u.host_str().unwrap_or_default());
        let path = u.path().trim_end_matches('/');
        let mut candidates = vec![];
        if !path.is_empty() {
            candidates.push(format!(
                "{origin}/.well-known/oauth-protected-resource{path}"
            ));
        }
        candidates.push(format!("{origin}/.well-known/oauth-protected-resource"));
        let mut resource = srv.url.to_string();
        let mut as_base = origin.clone();
        for c in candidates {
            if let Some(prm) = self.get_json(&c)? {
                if let Some(r) = prm["resource"].as_str() {
                    resource = r.to_string();
                }
                if let Some(a) = prm["authorization_servers"].get(0).and_then(Value::as_str) {
                    as_base = a.trim_end_matches('/').to_string();
                }
                if scopes.is_empty() {
                    scopes = prm["scopes_supported"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                }
                break;
            }
        }
        let asu = url::Url::parse(&as_base)?;
        let as_origin = format!("{}://{}", asu.scheme(), asu.host_str().unwrap_or_default());
        let as_path = asu.path().trim_end_matches('/');
        let meta_urls = [
            format!("{as_origin}/.well-known/oauth-authorization-server{as_path}"),
            format!("{as_origin}/.well-known/openid-configuration{as_path}"),
            format!("{as_base}/.well-known/openid-configuration"),
        ];
        for m in meta_urls {
            if let Some(v) = self.get_json(&m)? {
                if let Ok(auth) = serde_json::from_value::<AuthServerMeta>(v) {
                    return Ok(Discovery {
                        resource,
                        auth,
                        scopes,
                    });
                }
            }
        }
        bail!(
            "no OAuth authorization server metadata found for `{}`",
            srv.slug
        )
    }

    fn client(&self, srv: &RemoteServer, d: &Discovery) -> Result<ClientInfo> {
        let scope = client_scope(srv.slug);
        if let Some(raw) = self.backend.get(&scope, "client")? {
            let c: ClientInfo = serde_json::from_str(&raw)?;
            if c.redirect_uri == self.redirect_uri {
                return Ok(c);
            }
        }
        if srv.registration == ClientRegistration::Preregistered {
            bail!(
                "`{}` has no dynamic client registration; configure the client once with `ctox toolhub mcp-client-set --server {}`",
                srv.slug, srv.slug
            );
        }
        let endpoint = d
            .auth
            .registration_endpoint
            .as_deref()
            .ok_or_else(|| anyhow!("`{}` announces no registration_endpoint", srv.slug))?;
        let resp = self
            .transport
            .send(HttpRequest::new("POST", endpoint).json(&json!({
                "client_name": CLIENT_NAME,
                "redirect_uris": [self.redirect_uri],
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "none",
            })))?;
        let v = resp.json().unwrap_or(Value::Null);
        let client_id = v["client_id"]
            .as_str()
            .ok_or_else(|| anyhow!("dynamic client registration failed (HTTP {})", resp.status))?;
        let c = ClientInfo {
            client_id: client_id.into(),
            client_secret: v["client_secret"].as_str().map(String::from),
            redirect_uri: self.redirect_uri.clone(),
        };
        self.backend.put(
            &scope,
            "client",
            &serde_json::to_string(&c)?,
            json!({"kind": "mcp-client", "server": srv.slug, "registration": "dynamic"}),
        )?;
        Ok(c)
    }

    /// Operator-provided client for servers without DCR (Slack, Google).
    pub fn set_client(
        &self,
        slug: &str,
        client_id: &str,
        client_secret: Option<&str>,
    ) -> Result<()> {
        let srv = server(slug)?;
        let c = ClientInfo {
            client_id: client_id.into(),
            client_secret: client_secret.map(String::from),
            redirect_uri: self.redirect_uri.clone(),
        };
        self.backend.put(&client_scope(srv.slug), "client", &serde_json::to_string(&c)?,
            json!({"kind": "mcp-client", "server": srv.slug, "registration": "preregistered", "client_id": client_id}))
    }

    /// Step 1 of "Add": discovery, DCR and PKCE; returns the provider-hosted consent URL.
    pub fn connect(
        &self,
        slug: &str,
        project: &str,
        scopes: Option<Vec<String>>,
    ) -> Result<McpConnectRequest> {
        let srv = server(slug)?;
        let d = self.discover(&srv)?;
        let client = self.client(&srv, &d)?;
        let p = pkce::generate();
        let state = format!("mcp.{slug}.{}", pkce::random_token(24));
        let scopes = scopes.unwrap_or_else(|| {
            if !srv.default_scopes.is_empty() {
                srv.default_scopes.iter().map(|s| s.to_string()).collect()
            } else {
                d.scopes.clone()
            }
        });
        let mut url = url::Url::parse(&d.auth.authorization_endpoint)?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("response_type", "code")
                .append_pair("client_id", &client.client_id)
                .append_pair("redirect_uri", &client.redirect_uri)
                .append_pair("state", &state)
                .append_pair("code_challenge", &p.challenge)
                .append_pair("code_challenge_method", "S256")
                .append_pair("resource", &d.resource);
            if !scopes.is_empty() {
                q.append_pair("scope", &scopes.join(" "));
            }
            for (k, v) in srv.extra_authorize_params {
                q.append_pair(k, v);
            }
        }
        let scope = scope_of(slug, project);
        self.backend.put(
            &scope,
            "pending",
            &serde_json::to_string(&Pending {
                state: state.clone(),
                verifier: p.verifier.clone(),
                created_at: now(),
            })?,
            json!({"kind": "mcp-pending", "server": slug, "project": project}),
        )?;
        self.backend.put(
            &scope,
            "discovery",
            &serde_json::to_string(&d)?,
            json!({"kind": "mcp-discovery"}),
        )?;
        self.save_info(&ConnectionInfo {
            server: slug.into(),
            project: project.into(),
            status: "INITIALIZING".into(),
            scopes,
            expires_at: None,
            updated_at: now(),
            tool_count: None,
            reason: None,
        })?;
        Ok(McpConnectRequest {
            server: slug.into(),
            project: project.into(),
            authorize_url: url.into(),
            redirect_uri: client.redirect_uri,
            state,
        })
    }

    fn save_info(&self, info: &ConnectionInfo) -> Result<()> {
        // Metadata (non-secret) is kept on a dedicated row so `list` never decrypts tokens.
        self.backend.put(
            &scope_of(&info.server, &info.project),
            "info",
            "",
            serde_json::to_value(info)?,
        )
    }

    pub fn list(&self) -> Result<Vec<ConnectionInfo>> {
        Ok(self
            .backend
            .list()?
            .into_iter()
            .filter(|e| e.scope.starts_with(SCOPE_PREFIX) && e.name == "info")
            .filter_map(|e| serde_json::from_value(e.metadata).ok())
            .collect())
    }

    fn info(&self, slug: &str, project: &str) -> Option<ConnectionInfo> {
        self.list()
            .ok()?
            .into_iter()
            .find(|i| i.server == slug && i.project == project)
    }

    /// Step 2: the in-app window hands over the final redirect URL.
    pub fn complete(&self, callback: &str, project: &str) -> Result<ConnectionInfo> {
        let parsed = crate::oauth2::parse_callback(callback)?;
        let (code, state) = match parsed {
            crate::oauth2::CallbackResult::Code { code, state } => (code, state),
            crate::oauth2::CallbackResult::Denied { error, .. } => {
                bail!("authorization denied: {error}")
            }
        };
        let slug = state
            .strip_prefix("mcp.")
            .and_then(|s| s.split('.').next())
            .ok_or_else(|| anyhow!("callback state does not belong to an MCP connection"))?
            .to_string();
        let srv = server(&slug)?;
        let scope = scope_of(&slug, project);
        let pending: Pending = serde_json::from_str(
            &self
                .backend
                .get(&scope, "pending")?
                .ok_or_else(|| anyhow!("no pending authorization"))?,
        )?;
        if !constant_eq(pending.state.as_bytes(), state.as_bytes()) {
            bail!("state mismatch");
        }
        if now().saturating_sub(pending.created_at) > PENDING_TTL_SECS {
            bail!("authorization expired");
        }
        let d: Discovery = serde_json::from_str(
            &self
                .backend
                .get(&scope, "discovery")?
                .context("missing discovery")?,
        )?;
        let client = self.client(&srv, &d)?;
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code),
            ("redirect_uri", client.redirect_uri.clone()),
            ("client_id", client.client_id.clone()),
            ("code_verifier", pending.verifier),
            ("resource", d.resource.clone()),
        ];
        if let Some(s) = &client.client_secret {
            form.push(("client_secret", s.clone()));
        }
        let tokens = self.token_request(&d, &form)?;
        self.backend.delete(&scope, "pending")?;
        let tools = self.store_tokens_and_probe(&srv, project, &tokens)?;
        let mut info = self.info(&slug, project).unwrap_or(ConnectionInfo {
            server: slug.clone(),
            project: project.into(),
            status: String::new(),
            scopes: vec![],
            expires_at: None,
            updated_at: 0,
            tool_count: None,
            reason: None,
        });
        info.status = "ACTIVE".into();
        info.expires_at = tokens.expires_at;
        info.updated_at = now();
        info.tool_count = tools;
        info.reason = None;
        self.save_info(&info)?;
        Ok(info)
    }

    fn store_tokens_and_probe(
        &self,
        srv: &RemoteServer,
        project: &str,
        t: &Tokens,
    ) -> Result<Option<usize>> {
        self.backend.put(
            &scope_of(srv.slug, project),
            "token",
            &serde_json::to_string(t)?,
            json!({"kind": "mcp-token"}),
        )?;
        Ok(self.list_tools(srv.slug, project).ok().map(|t| t.len()))
    }

    fn token_request(&self, _d: &Discovery, form: &[(&str, String)]) -> Result<Tokens> {
        let pairs: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let r = self.transport.send(
            HttpRequest::new("POST", &_d.auth.token_endpoint)
                .header("accept", "application/json")
                .form(&pairs),
        )?;
        let v = r.json().unwrap_or(Value::Null);
        // Slack (oauth.v2.user.access) nests the user token under `authed_user`.
        let src = if v.get("access_token").is_none() && v.get("authed_user").is_some() {
            &v["authed_user"]
        } else {
            &v
        };
        if let Some(e) = v.get("error").and_then(Value::as_str) {
            bail!("token endpoint error: {e}");
        }
        let access = src["access_token"].as_str().ok_or_else(|| {
            anyhow!(
                "token endpoint returned HTTP {} without access_token",
                r.status
            )
        })?;
        Ok(Tokens {
            access_token: access.into(),
            refresh_token: src["refresh_token"].as_str().map(String::from),
            expires_at: src["expires_in"].as_u64().map(|s| now() + s),
        })
    }

    fn tokens(&self, srv: &RemoteServer, project: &str) -> Result<Option<Tokens>> {
        let scope = scope_of(srv.slug, project);
        let Some(raw) = self.backend.get(&scope, "token")? else {
            return Ok(None);
        };
        let t: Tokens = serde_json::from_str(&raw)?;
        if t.expires_at.is_some_and(|e| e <= now() + 60) {
            return self.refresh(srv, project, &t).map(Some);
        }
        Ok(Some(t))
    }

    fn refresh(&self, srv: &RemoteServer, project: &str, t: &Tokens) -> Result<Tokens> {
        let rt = t
            .refresh_token
            .clone()
            .ok_or_else(|| anyhow!("token expired and no refresh token"))?;
        let scope = scope_of(srv.slug, project);
        let d: Discovery = serde_json::from_str(
            &self
                .backend
                .get(&scope, "discovery")?
                .context("missing discovery")?,
        )?;
        let client = self.client(srv, &d)?;
        let mut form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", rt.clone()),
            ("client_id", client.client_id),
            ("resource", d.resource.clone()),
        ];
        if let Some(s) = client.client_secret {
            form.push(("client_secret", s));
        }
        let mut n = self.token_request(&d, &form)?;
        if n.refresh_token.is_none() {
            n.refresh_token = Some(rt);
        }
        self.backend.put(
            &scope,
            "token",
            &serde_json::to_string(&n)?,
            json!({"kind": "mcp-token"}),
        )?;
        if let Some(mut info) = self.info(srv.slug, project) {
            info.expires_at = n.expires_at;
            info.updated_at = now();
            info.status = "ACTIVE".into();
            self.save_info(&info)?;
        }
        Ok(n)
    }

    fn post(
        &self,
        srv: &RemoteServer,
        token: &str,
        session: Option<&str>,
        body: &Value,
    ) -> Result<HttpResponse> {
        let mut r = HttpRequest::new("POST", srv.url)
            .header("accept", "application/json, text/event-stream")
            .header("authorization", format!("Bearer {token}"))
            .header("mcp-protocol-version", PROTOCOL_VERSION);
        if let Some(s) = session {
            r = r.header("mcp-session-id", s);
        }
        self.transport.send(r.json(body))
    }

    /// Stateless-friendly call: initialize + request in one go.
    fn rpc(&self, slug: &str, project: &str, method: &str, params: Value) -> Result<Rpc> {
        let srv = server(slug)?;
        let Some(t) = self.tokens(&srv, project)? else {
            return Ok(Rpc::Unauthorized);
        };
        let init = self.post(&srv, &t.access_token, None, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": PROTOCOL_VERSION, "capabilities": {},
                       "clientInfo": {"name": "ctox-toolhub", "version": env!("CARGO_PKG_VERSION")}}}))?;
        if init.status == 401 || init.status == 403 {
            self.mark(&srv, project, "EXPIRED", &format!("HTTP {}", init.status));
            return Ok(Rpc::Unauthorized);
        }
        if !(200..300).contains(&init.status) {
            bail!("MCP initialize failed: HTTP {}", init.status);
        }
        parse_rpc_body(&init.body, 1)?;
        let session = init.session_id.clone();
        let _ = self.post(
            &srv,
            &t.access_token,
            session.as_deref(),
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        );
        let r = self.post(
            &srv,
            &t.access_token,
            session.as_deref(),
            &json!({"jsonrpc": "2.0", "id": 2, "method": method, "params": params}),
        )?;
        if r.status == 401 {
            self.mark(&srv, project, "EXPIRED", "HTTP 401");
            return Ok(Rpc::Unauthorized);
        }
        if !(200..300).contains(&r.status) {
            bail!("MCP {method} failed: HTTP {}", r.status);
        }
        Ok(Rpc::Ok(parse_rpc_body(&r.body, 2)?))
    }

    fn mark(&self, srv: &RemoteServer, project: &str, status: &str, reason: &str) {
        if let Some(mut info) = self.info(srv.slug, project) {
            info.status = status.into();
            info.reason = Some(reason.into());
            info.updated_at = now();
            let _ = self.save_info(&info);
        }
    }

    pub fn list_tools(&self, slug: &str, project: &str) -> Result<Vec<RemoteTool>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = cursor
                .as_ref()
                .map(|c| json!({"cursor": c}))
                .unwrap_or(json!({}));
            match self.rpc(slug, project, "tools/list", params)? {
                Rpc::Unauthorized => bail!("credential_required"),
                Rpc::Ok(v) => {
                    out.extend(
                        serde_json::from_value::<Vec<RemoteTool>>(v["tools"].clone())
                            .unwrap_or_default(),
                    );
                    cursor = v["nextCursor"].as_str().map(String::from);
                    if cursor.is_none() {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn call_tool(
        &self,
        slug: &str,
        project: &str,
        tool: &str,
        args: &Value,
    ) -> Result<McpOutcome> {
        match self.rpc(
            slug,
            project,
            "tools/call",
            json!({"name": tool, "arguments": args}),
        )? {
            Rpc::Unauthorized => Ok(self.credential_required(slug, project)),
            Rpc::Ok(v) => Ok(if v["isError"].as_bool() == Some(true) {
                McpOutcome::ToolError {
                    server: slug.into(),
                    tool: tool.into(),
                    result: v,
                }
            } else {
                McpOutcome::Success {
                    server: slug.into(),
                    tool: tool.into(),
                    result: v,
                }
            }),
        }
    }

    pub fn credential_required(&self, slug: &str, project: &str) -> McpOutcome {
        McpOutcome::CredentialRequired {
            server: slug.into(),
            project: project.into(),
            reason: LocalizedText::new(
                "Bitte die Schnittstelle in Workjet verbinden (Einstellungen → Schnittstellen oder Karte in der Sitzung).",
                "Please connect this integration in Workjet (Settings → Integrations or the card in the session).",
            ),
        }
    }

    pub fn disconnect(&self, slug: &str, project: &str) -> Result<()> {
        let scope = scope_of(slug, project);
        for n in ["token", "pending", "discovery", "info"] {
            let _ = self.backend.delete(&scope, n);
        }
        Ok(())
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemorySecretBackend;
    use std::sync::Mutex;

    struct Fake(Mutex<Vec<HttpRequest>>);
    impl HttpTransport for Fake {
        fn send(&self, r: HttpRequest) -> Result<HttpResponse> {
            self.0.lock().unwrap().push(r.clone());
            let ok = |v: Value| {
                Ok(HttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&v).unwrap(),
                    session_id: None,
                })
            };
            let u = r.url.as_str();
            if u.ends_with("/.well-known/oauth-protected-resource/mcp") {
                return ok(
                    json!({"resource": "https://mcp.test/mcp", "authorization_servers": ["https://auth.test"], "scopes_supported": ["a"]}),
                );
            }
            if u == "https://auth.test/.well-known/oauth-authorization-server" {
                return ok(
                    json!({"authorization_endpoint": "https://auth.test/authorize", "token_endpoint": "https://auth.test/token", "registration_endpoint": "https://auth.test/register"}),
                );
            }
            if u.ends_with("/register") {
                return ok(json!({"client_id": "dyn-1"}));
            }
            if u.ends_with("/token") {
                return ok(
                    json!({"access_token": "AT", "refresh_token": "RT", "expires_in": 3600}),
                );
            }
            if u == "https://mcp.cloudflare.com/mcp" {
                let body: Value =
                    serde_json::from_slice(r.body.as_deref().unwrap_or(b"{}")).unwrap_or_default();
                let id = body["id"].as_u64().unwrap_or(0);
                if r.headers.get("authorization").is_none() {
                    return Ok(HttpResponse {
                        status: 401,
                        body: vec![],
                        session_id: None,
                    });
                }
                let result = match body["method"].as_str() {
                    Some("tools/list") => {
                        json!({"tools": [{"name": "zones_list", "inputSchema": {}, "annotations": {"readOnlyHint": true}}, {"name": "dns_create", "inputSchema": {}}]})
                    }
                    Some("tools/call") => json!({"content": [{"type": "text", "text": "ok"}]}),
                    _ => json!({}),
                };
                let sse = format!(
                    "event: message\ndata: {}\n\n",
                    json!({"jsonrpc": "2.0", "id": id, "result": result})
                );
                return Ok(HttpResponse {
                    status: 200,
                    body: sse.into_bytes(),
                    session_id: Some("S1".into()),
                });
            }
            Ok(HttpResponse {
                status: 404,
                body: vec![],
                session_id: None,
            })
        }
    }

    #[test]
    fn full_dcr_pkce_flow_and_tool_call() {
        // Point discovery for the cloudflare entry at the fake auth server.
        let backend = MemorySecretBackend::default();
        let fake = Fake(Mutex::new(vec![]));
        let hub = RemoteMcp {
            backend: &backend,
            transport: &fake,
            redirect_uri: "http://127.0.0.1:8765/toolhub/oauth/callback".into(),
        };
        // Fake answers the cloudflare PRM path with auth.test.
        let fake_prm = fake
            .send(HttpRequest::new(
                "GET",
                "https://mcp.cloudflare.com/.well-known/oauth-protected-resource/mcp",
            ))
            .unwrap();
        assert_eq!(fake_prm.status, 200);
        assert!(matches!(
            hub.call_tool("cloudflare", "p1", "zones_list", &json!({}))
                .unwrap(),
            McpOutcome::CredentialRequired { .. }
        ));
        let req = hub.connect("cloudflare", "p1", None).unwrap();
        assert!(req
            .authorize_url
            .starts_with("https://auth.test/authorize?"));
        assert!(
            req.authorize_url.contains("client_id=dyn-1")
                && req.authorize_url.contains("code_challenge_method=S256")
                && req
                    .authorize_url
                    .contains("resource=https%3A%2F%2Fmcp.test%2Fmcp")
        );
        assert!(hub
            .complete(
                "http://127.0.0.1:8765/toolhub/oauth/callback?code=C&state=wrong",
                "p1"
            )
            .is_err());
        let info = hub
            .complete(
                &format!(
                    "http://127.0.0.1:8765/toolhub/oauth/callback?code=C&state={}",
                    req.state
                ),
                "p1",
            )
            .unwrap();
        assert_eq!(info.status, "ACTIVE");
        assert_eq!(info.tool_count, Some(2));
        let tools = hub.list_tools("cloudflare", "p1").unwrap();
        assert!(!tools[0].mutating() && tools[1].mutating());
        assert!(matches!(
            hub.call_tool("cloudflare", "p1", "zones_list", &json!({}))
                .unwrap(),
            McpOutcome::Success { .. }
        ));
        // Secrets never appear in the listed metadata.
        let listed = serde_json::to_string(&hub.list().unwrap()).unwrap();
        assert!(!listed.contains("AT") || !listed.contains("RT"));
        assert!(
            hub.connect("slack", "p1", None).is_err(),
            "slack needs a preregistered client"
        );
    }

    #[test]
    fn parses_sse_and_www_authenticate() {
        let (m, s) = parse_www_authenticate(
            r#"Bearer realm="OAuth", resource_metadata="https://x/.well-known/oauth-protected-resource/mcp", scope="user:read account:read""#,
        );
        assert_eq!(
            m.as_deref(),
            Some("https://x/.well-known/oauth-protected-resource/mcp")
        );
        assert_eq!(s, vec!["user:read", "account:read"]);
        let v = parse_rpc_body(
            b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"x\":1}}\n\n",
            2,
        )
        .unwrap();
        assert_eq!(v["x"], 1);
    }
}
