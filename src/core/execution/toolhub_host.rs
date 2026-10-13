//! CTOX host glue for `ctox-toolhub`: encrypted secret backend, the chainable
//! `ctox toolhub ...` CLI and the background refresh tick. CLI only; CTOX's
//! single MCP stays the external control surface and does not expose toolkits.
//!
//! DE: Tokens bleiben lokal im Host (`secrets.rs`). Ausgaben enthalten nie
//! Zugangsdaten. OAuth läuft wie im cliproxyapi-Port über einen lokalen
//! Callback-Server auf der Instanz (PKCE); kein zentraler OAuth-Dienst.
//! EN: Tokens stay local in the host (`secrets.rs`). Output never contains
//! credentials. OAuth uses a local callback server on the instance (PKCE),
//! like the cliproxyapi port; there is no central OAuth service.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ctox_toolhub::http::UreqTransport;
use ctox_toolhub::oauth2::{CallbackResult, RedirectTarget};
use ctox_toolhub::{
    AuthConfig, AuthSchemeKind, ClientCredentialSource, ConnectionStatus, SecretBackend,
    SecretEntry, ToolHub, ToolOutcome,
};
use serde_json::{json, Value};

use crate::secrets;

/// Loopback port of the local OAuth callback server.
pub const LOOPBACK_PORT_ENV: &str = "CTOX_TOOLHUB_LOOPBACK_PORT";
const DEFAULT_LOOPBACK_PORT: u16 = 8765;
/// Fallback project id (a `workjet_projects` record id) when `--project` is omitted.
pub const PROJECT_ENV: &str = "CTOX_PROJECT";
/// Seconds between background refresh ticks.
pub const REFRESH_TICK_SECS: u64 = 60;

/// Stable process exit codes of `ctox toolhub exec`.
pub mod exit {
    pub const OK: i32 = 0;
    pub const INTERNAL: i32 = 1;
    pub const VALIDATION: i32 = 2;
    pub const CREDENTIAL_REQUIRED: i32 = 3;
    pub const UPSTREAM: i32 = 4;
    pub const APPROVAL_REQUIRED: i32 = 5;
}

/// `ctox-toolhub` secret backend over the encrypted CTOX secret store.
pub struct CtoxSecretBackend {
    root: PathBuf,
}

impl CtoxSecretBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl SecretBackend for CtoxSecretBackend {
    fn put(&self, scope: &str, name: &str, value: &str, metadata: Value) -> Result<()> {
        let _guard = secrets::credential_lifecycle_guard();
        secrets::write_secret_record(
            &self.root,
            scope,
            name,
            value,
            Some(format!("toolhub {scope}/{name}")),
            metadata,
        )?;
        Ok(())
    }

    fn get(&self, scope: &str, name: &str) -> Result<Option<String>> {
        secrets::read_secret_value_optional(&self.root, scope, name).map_err(anyhow::Error::from)
    }

    fn delete(&self, scope: &str, name: &str) -> Result<()> {
        let _guard = secrets::credential_lifecycle_guard();
        secrets::delete_secret_record(&self.root, scope, name)
    }

    fn list(&self) -> Result<Vec<SecretEntry>> {
        Ok(secrets::list_secret_records(&self.root, None)?
            .into_iter()
            .filter(|r| r.scope.starts_with("toolhub"))
            .map(|r| SecretEntry {
                scope: r.scope,
                name: r.secret_name,
                metadata: r.metadata,
            })
            .collect())
    }
}

fn loopback_port() -> Result<u16> {
    match std::env::var(LOOPBACK_PORT_ENV) {
        Ok(p) => p.parse().context("invalid CTOX_TOOLHUB_LOOPBACK_PORT"),
        Err(_) => Ok(DEFAULT_LOOPBACK_PORT),
    }
}

pub fn hub(root: &Path) -> Result<ToolHub<CtoxSecretBackend>> {
    Ok(ToolHub::new(
        CtoxSecretBackend::new(root),
        Arc::new(UreqTransport::default()),
        RedirectTarget::Loopback {
            port: loopback_port()?,
        },
    ))
}

/// One background refresh pass (called from the long-running instance server).
pub fn refresh_tick(root: &Path) {
    match hub(root).and_then(|h| h.refresh_due()) {
        Ok(accounts) => {
            for a in accounts
                .iter()
                .filter(|a| a.status != ConnectionStatus::Active)
            {
                eprintln!(
                    "[toolhub] refresh failed for {} ({}): {}",
                    a.id,
                    a.toolkit,
                    a.status_reason.as_deref().unwrap_or("unknown")
                );
            }
        }
        Err(err) => eprintln!("[toolhub] refresh tick failed: {err:#}"),
    }
}

/// Spawn the periodic refresh thread. Errors never stop the loop.
pub fn start_refresh_loop(root: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("toolhub-refresh".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(REFRESH_TICK_SECS));
            let _ = std::panic::catch_unwind(|| refresh_tick(&root));
        });
    if let Err(err) = spawned {
        eprintln!("[toolhub] could not start refresh loop: {err}");
    }
}

const USAGE: &str = "usage: ctox toolhub <command> [options]   (JSON on stdout, logs on stderr)
  remote MCP connectors: mcp-servers | mcp-list | mcp-connect --server S [--project P] | mcp-complete --callback URL | mcp-client-set --server S --client-id ID [--client-secret-stdin] | mcp-tools --server S | mcp-disconnect --server S | exec <server>.<tool> [--json ..] [--confirm]
  catalog [--json]                      toolkits, tools, input schemas
  list [--json]                         connected accounts (no secrets)
  configs                               auth configs (no secrets)
  config-set --toolkit <slug> --id <id> --scheme <oauth2|bearer|api_key|basic> [--client-id <id>] [--client-secret-stdin] [--scope <s>]...
  connect --auth-config <id> [--project <project_id>] [--actor <a>] [--credential-stdin] [--no-callback-server]
  complete --callback <redirect-url-or-query>
  status|refresh|disconnect --account <id>
  exec <toolkit>.<action> [--json <args>|stdin] [--project <project_id>] [--account <id>] [--confirm]
exit codes (exec): 0 ok, 2 validation, 3 credential_required, 4 upstream, 5 approval_required, 1 internal";

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn required<'a>(args: &'a [String], name: &str) -> Result<&'a str> {
    flag(args, name).with_context(|| format!("missing {name}\n{USAGE}"))
}

fn read_stdin() -> Result<String> {
    let mut s = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
    Ok(s.trim_end_matches(['\r', '\n']).to_string())
}

fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Checks whether a project id names an active CTOX (Workjet) project.
pub type ProjectResolver<'a> = &'a dyn Fn(&str) -> Result<bool>;

/// Real CTOX project identity: an active record in `workjet_projects`
/// (Business OS store). Missing/archived/deleted projects are rejected.
fn store_project_resolver(root: &Path) -> impl Fn(&str) -> Result<bool> + '_ {
    move |id| Ok(crate::business_os::active_workjet_project_owner(root, id)?.is_some())
}

/// `--project`, else `$CTOX_PROJECT`; must be an existing active project.
fn resolve_project(args: &[String], resolver: ProjectResolver<'_>) -> Result<String> {
    let id = flag(args, "--project")
        .map(String::from)
        .or_else(|| std::env::var(PROJECT_ENV).ok().filter(|p| !p.trim().is_empty()))
        .context("Projekt fehlt: --project <id> oder CTOX_PROJECT setzen / missing project: pass --project <id> or set CTOX_PROJECT")?;
    if !ctox_toolhub::store::valid_segment(&id) {
        bail!("ungültige Projekt-ID / invalid project id `{id}`");
    }
    if !resolver(&id)? {
        bail!("unbekanntes oder inaktives CTOX-Projekt / unknown or inactive CTOX project `{id}`");
    }
    Ok(id)
}

fn actor(args: &[String]) -> String {
    flag(args, "--actor")
        .map(String::from)
        .or_else(|| std::env::var("CTOX_ACTOR").ok())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "unknown".into())
}

fn parse_scheme(s: &str) -> Result<AuthSchemeKind> {
    Ok(match s {
        "oauth2" => AuthSchemeKind::OAuth2,
        "bearer" => AuthSchemeKind::Bearer,
        "api_key" => AuthSchemeKind::ApiKey,
        "basic" => AuthSchemeKind::Basic,
        other => bail!("unknown scheme `{other}`"),
    })
}

fn callback_query(callback: CallbackResult) -> String {
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    match callback {
        CallbackResult::Code { code, state } => {
            format!("code={}&state={}", enc(&code), enc(&state))
        }
        CallbackResult::Denied { error, state } => {
            format!(
                "error={}&state={}",
                enc(&error),
                enc(&state.unwrap_or_default())
            )
        }
    }
}

pub fn handle_toolhub_command(root: &Path, args: &[String]) -> Result<()> {
    let hub = hub(root)?;
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("catalog") => print(&hub.catalog()),
        Some("list") => print(&hub.store().list()?),
        Some("configs") => print(&hub.store().auth_configs()?),
        Some("config-set") => {
            let scheme = parse_scheme(required(rest, "--scheme")?)?;
            let client_id = flag(rest, "--client-id").map(String::from);
            let scopes = rest
                .windows(2)
                .filter(|w| w[0] == "--scope")
                .map(|w| w[1].clone())
                .collect();
            let secret = has(rest, "--client-secret-stdin")
                .then(read_stdin)
                .transpose()?;
            let config = AuthConfig {
                id: required(rest, "--id")?.into(),
                toolkit: required(rest, "--toolkit")?.into(),
                scheme,
                source: if client_id.is_some() {
                    ClientCredentialSource::Custom
                } else {
                    ClientCredentialSource::None
                },
                client_id,
                scopes,
                enabled: true,
            };
            hub.upsert_auth_config(&config, secret.as_deref())?;
            print(&config)
        }
        Some("connect") => {
            let config = required(rest, "--auth-config")?;
            let resolver = store_project_resolver(root);
            let (project, actor) = (resolve_project(rest, &resolver)?, actor(rest));
            if has(rest, "--credential-stdin") {
                return print(&hub.connect_with_credential(
                    config,
                    &project,
                    &actor,
                    &read_stdin()?,
                )?);
            }
            let request = hub.connect_oauth(config, &project, &actor)?;
            let server = if has(rest, "--no-callback-server") {
                false
            } else {
                spawn_callback_server(&request.account_id)?
            };
            print(&json!({
                "account_id": request.account_id,
                "toolkit": request.toolkit,
                "status": request.status,
                "redirect_url": request.redirect_url,
                "callback_server": server,
                "complete_hint": "ctox toolhub complete --callback '<final redirect URL>'",
            }))
        }
        // Internal: one-shot local callback server started by `connect`.
        Some("_callback-server") => {
            let port = loopback_port()?;
            let timeout = ctox_toolhub::hub::PENDING_TTL_SECS;
            let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
            let callback = ctox_toolhub::oauth2::wait_for_loopback_callback(
                listener,
                std::time::Duration::from_secs(timeout),
            )?;
            print(&hub.complete_oauth(&callback_query(callback))?)
        }
        Some("complete") => print(&hub.complete_oauth(required(rest, "--callback")?)?),
        Some("status") => print(&hub.status(required(rest, "--account")?)?),
        Some("disconnect") => {
            print(&json!({"removed": hub.disconnect(required(rest, "--account")?)?}))
        }
        Some("refresh") => print(&hub.refresh(required(rest, "--account")?)?),
        // Remote MCP connectors (official provider MCP servers, MCP OAuth + DCR).
        Some("mcp-servers") => print(&ctox_toolhub::remote_mcp::registry()),
        Some("mcp-list") => print(&mcp(root)?.list()?),
        Some("mcp-connect") => {
            let resolver = store_project_resolver(root);
            let project = mcp_project(rest, &resolver)?;
            let scopes: Vec<String> = rest
                .windows(2)
                .filter(|w| w[0] == "--scope")
                .map(|w| w[1].clone())
                .collect();
            let req = mcp(root)?.connect(
                required(rest, "--server")?,
                &project,
                (!scopes.is_empty()).then_some(scopes),
            )?;
            print(&req)
        }
        Some("mcp-complete") => {
            let resolver = store_project_resolver(root);
            let project = mcp_project(rest, &resolver)?;
            print(&mcp(root)?.complete(required(rest, "--callback")?, &project)?)
        }
        Some("mcp-client-set") => {
            let secret = has(rest, "--client-secret-stdin")
                .then(read_stdin)
                .transpose()?;
            mcp(root)?.set_client(
                required(rest, "--server")?,
                required(rest, "--client-id")?,
                secret.as_deref().map(str::trim),
            )?;
            print(&json!({"configured": true}))
        }
        Some("mcp-tools") => {
            let resolver = store_project_resolver(root);
            let project = mcp_project(rest, &resolver)?;
            print(&mcp(root)?.list_tools(required(rest, "--server")?, &project)?)
        }
        Some("mcp-disconnect") => {
            let resolver = store_project_resolver(root);
            let project = mcp_project(rest, &resolver)?;
            mcp(root)?.disconnect(required(rest, "--server")?, &project)?;
            print(&json!({"removed": true}))
        }
        Some("exec")
            if rest
                .first()
                .and_then(|n| n.split_once('.'))
                .is_some_and(|(s, _)| ctox_toolhub::remote_mcp::server(s).is_ok()) =>
        {
            let resolver = store_project_resolver(root);
            let (code, value) = exec_mcp(root, rest, &resolver);
            println!("{}", serde_json::to_string_pretty(&value)?);
            if code != exit::OK {
                std::process::exit(code);
            }
            Ok(())
        }
        Some("exec") => {
            let resolver = store_project_resolver(root);
            let (code, value) = exec(&hub, rest, &resolver);
            println!("{}", serde_json::to_string_pretty(&value)?);
            if code != exit::OK {
                std::process::exit(code);
            }
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

struct McpHost {
    backend: CtoxSecretBackend,
    transport: UreqTransport,
    redirect: String,
}

impl McpHost {
    fn client(&self) -> ctox_toolhub::remote_mcp::RemoteMcp<'_, CtoxSecretBackend> {
        ctox_toolhub::remote_mcp::RemoteMcp {
            backend: &self.backend,
            transport: &self.transport,
            redirect_uri: self.redirect.clone(),
        }
    }
}

struct McpRef(McpHost);
impl McpRef {
    fn list(&self) -> Result<Vec<ctox_toolhub::remote_mcp::ConnectionInfo>> {
        self.0.client().list()
    }
    fn connect(
        &self,
        s: &str,
        p: &str,
        sc: Option<Vec<String>>,
    ) -> Result<ctox_toolhub::remote_mcp::McpConnectRequest> {
        self.0.client().connect(s, p, sc)
    }
    fn complete(&self, cb: &str, p: &str) -> Result<ctox_toolhub::remote_mcp::ConnectionInfo> {
        self.0.client().complete(cb, p)
    }
    fn set_client(&self, s: &str, id: &str, sec: Option<&str>) -> Result<()> {
        self.0.client().set_client(s, id, sec)
    }
    fn list_tools(&self, s: &str, p: &str) -> Result<Vec<ctox_toolhub::remote_mcp::RemoteTool>> {
        self.0.client().list_tools(s, p)
    }
    fn disconnect(&self, s: &str, p: &str) -> Result<()> {
        self.0.client().disconnect(s, p)
    }
}

/// Public HTTPS base URL of this instance (e.g. `https://welsch.ctox.dev`).
/// Configured, never derived from request headers (Host is caller-controlled).
pub const PUBLIC_BASE_URL_ENV: &str = "CTOX_PUBLIC_BASE_URL";
/// Instance-hosted OAuth callback route for remote MCP connectors.
pub const MCP_CALLBACK_PATH: &str = "/toolhub/oauth/callback";

/// Redirect URI owned by the instance: DCR registration (one per instance and
/// provider, reused), state, PKCE verifier and tokens all belong to the
/// instance; any client only opens the authorize URL the instance hands out.
/// Loopback is a local-dev fallback only (`CTOX_TOOLHUB_DEV_LOOPBACK=1`).
pub fn mcp_redirect_uri() -> Result<String> {
    if let Ok(base) = std::env::var(PUBLIC_BASE_URL_ENV) {
        let base = base.trim().trim_end_matches('/');
        let parsed = url::Url::parse(base).context("invalid CTOX_PUBLIC_BASE_URL")?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            bail!("CTOX_PUBLIC_BASE_URL must be an https origin");
        }
        return Ok(format!("{base}{MCP_CALLBACK_PATH}"));
    }
    if std::env::var("CTOX_TOOLHUB_DEV_LOOPBACK").is_ok_and(|v| v == "1") {
        return Ok(RedirectTarget::Loopback {
            port: loopback_port()?,
        }
        .redirect_uri());
    }
    bail!("Öffentliche Instanz-URL fehlt (CTOX_PUBLIC_BASE_URL) / instance public base URL not configured")
}

fn mcp(root: &Path) -> Result<McpRef> {
    Ok(McpRef(McpHost {
        backend: CtoxSecretBackend::new(root),
        transport: UreqTransport::default(),
        redirect: mcp_redirect_uri().unwrap_or_default(),
    }))
}

/// Instance-hosted OAuth callback: validates state, exchanges code + verifier
/// on the instance and returns a small result page for the in-app window.
pub fn handle_mcp_oauth_callback(root: &Path, url: &str) -> (u16, String) {
    let page = |ok: bool, de: &str, en: &str| {
        format!(
            "<!doctype html><meta charset=utf-8><title>{}</title><body data-toolhub-result=\"{}\" style=\"font:15px system-ui;padding:2em\"><h1>{de}</h1><p>{en}</p></body>",
            if ok { "CTOX Toolhub: verbunden" } else { "CTOX Toolhub: Fehler" },
            if ok { "connected" } else { "failed" }
        )
    };
    match mcp(root).and_then(|h| h.complete(url, INSTANCE_SCOPE)) {
        Ok(_) => (
            200,
            page(
                true,
                "Verbunden. Dieses Fenster schließt sich.",
                "Connected. This window will close.",
            ),
        ),
        Err(e) => {
            eprintln!("[toolhub] oauth callback failed: {e:#}");
            (
                400,
                page(false, "Verbindung fehlgeschlagen.", "Connection failed."),
            )
        }
    }
}

fn exec_via_proxy(
    base: &str,
    server: &str,
    tool: &str,
    args: &Value,
    confirm: bool,
) -> (i32, Value) {
    let token = std::env::var("CTOX_TOOLHUB_PROXY_TOKEN").unwrap_or_default();
    let url = format!(
        "{}/api/business-os/toolhub/mcp/call",
        base.trim_end_matches('/')
    );
    let resp = ureq::post(&url)
        .set("authorization", &format!("Bearer {token}"))
        .set("content-type", "application/json")
        .send_string(
            &json!({"server": server, "tool": tool, "arguments": args, "confirm": confirm})
                .to_string(),
        );
    let v: Value = match resp {
        Ok(r) => serde_json::from_reader(r.into_reader()).unwrap_or(Value::Null),
        Err(ureq::Error::Status(code, _)) => {
            return (
                exit::UPSTREAM,
                json!({"outcome": "provider_error", "message": format!("proxy HTTP {code}")}),
            )
        }
        Err(e) => {
            return (
                exit::INTERNAL,
                json!({"outcome": "error", "message": e.to_string()}),
            )
        }
    };
    let outcome = v.get("outcome").cloned().unwrap_or(Value::Null);
    let code = match outcome.get("outcome").and_then(Value::as_str) {
        Some("success") => exit::OK,
        Some("credential_required") => exit::CREDENTIAL_REQUIRED,
        Some("approval_required") => exit::APPROVAL_REQUIRED,
        _ => exit::UPSTREAM,
    };
    (code, outcome)
}

fn hub_for_proxy(root: &Path) -> Result<ToolHub<CtoxSecretBackend>> {
    hub(root)
}

/// Instance-wide connection scope used by the Workjet "Schnittstellen" store and
/// the central proxy (shared by every harness/PC of the instance network).
pub const INSTANCE_SCOPE: &str = "_instance";

/// Project for MCP commands: explicit/env project (must exist) or the instance scope.
fn mcp_project(args: &[String], resolver: ProjectResolver<'_>) -> Result<String> {
    let explicit = flag(args, "--project").is_some()
        || std::env::var(PROJECT_ENV).is_ok_and(|p| !p.trim().is_empty());
    if explicit {
        resolve_project(args, resolver)
    } else {
        Ok(INSTANCE_SCOPE.into())
    }
}

/// Central toolhub proxy on the instance (`/api/business-os/toolhub/<route>`).
/// Auth is done by the Business OS server (session cookie or capability bearer),
/// so Workjet apps, harnesses and PCs in the instance network share connections
/// without ever receiving provider tokens. Returns (HTTP status, JSON).
pub fn handle_toolhub_http(root: &Path, route: &str, admin: bool, body: &Value) -> (u16, Value) {
    use ctox_toolhub::remote_mcp::McpOutcome;
    let err = |status: u16, m: String| (status, json!({"ok": false, "message": m}));
    let hub = match mcp(root) {
        Ok(h) => h,
        Err(e) => return err(500, e.to_string()),
    };
    let s = |k: &str| body.get(k).and_then(Value::as_str).map(str::to_string);
    let server = s("server").unwrap_or_default();
    match route {
        "connections" => {
            let connections: Vec<_> = hub
                .list()
                .unwrap_or_default()
                .into_iter()
                .filter(|c| c.project == INSTANCE_SCOPE)
                .collect();
            (
                200,
                json!({"ok": true, "servers": ctox_toolhub::remote_mcp::registry(), "connections": connections}),
            )
        }
        "connect" | "disconnect" if !admin => err(403, "chef or admin role required".into()),
        "connect" => {
            if hub.0.redirect.is_empty() {
                return err(
                    409,
                    mcp_redirect_uri()
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                );
            }
            match hub.connect(&server, INSTANCE_SCOPE, None) {
                Ok(req) => (
                    200,
                    json!({"ok": true, "server": req.server, "authorize_url": req.authorize_url, "redirect_uri": req.redirect_uri}),
                ),
                Err(e) => err(502, e.to_string()),
            }
        }
        "disconnect" => {
            if let Some(toolkit) = s("toolkit") {
                let removed = hub_for_proxy(root).and_then(|h| {
                    let mut n = 0;
                    for a in h
                        .store()
                        .list()?
                        .into_iter()
                        .filter(|a| a.project == INSTANCE_SCOPE && a.toolkit == toolkit)
                    {
                        if h.disconnect(&a.id)? {
                            n += 1;
                        }
                    }
                    Ok(n)
                });
                return match removed {
                    Ok(n) => (200, json!({"ok": true, "removed": n})),
                    Err(e) => err(500, e.to_string()),
                };
            }
            match hub.disconnect(&server, INSTANCE_SCOPE) {
                Ok(()) => (200, json!({"ok": true})),
                Err(e) => err(500, e.to_string()),
            }
        }
        "mcp/tools" => match hub.list_tools(&server, INSTANCE_SCOPE) {
            Ok(tools) => (200, json!({"ok": true, "server": server, "tools": tools})),
            Err(_) => (
                200,
                json!({"ok": false, "outcome": serde_json::to_value(hub.0.client().credential_required(&server, INSTANCE_SCOPE)).unwrap_or(Value::Null)}),
            ),
        },
        "mcp/call" => {
            let tool = s("tool").unwrap_or_default();
            let args = body.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let confirmed = body.get("confirm").and_then(Value::as_bool) == Some(true);
            if !confirmed {
                match hub.list_tools(&server, INSTANCE_SCOPE) {
                    Ok(tools) => match tools.iter().find(|t| t.name == tool) {
                        None => return err(404, format!("unknown tool `{tool}` on `{server}`")),
                        Some(t) if t.mutating() => {
                            return (
                                200,
                                json!({"ok": false, "outcome": {"outcome": "approval_required", "tool": format!("{server}.{tool}")}}),
                            )
                        }
                        Some(_) => {}
                    },
                    Err(_) => {
                        return (
                            200,
                            json!({"ok": false, "outcome": serde_json::to_value(hub.0.client().credential_required(&server, INSTANCE_SCOPE)).unwrap_or(Value::Null)}),
                        )
                    }
                }
            }
            match hub
                .0
                .client()
                .call_tool(&server, INSTANCE_SCOPE, &tool, &args)
            {
                Ok(o) => (
                    200,
                    json!({"ok": matches!(o, McpOutcome::Success { .. }), "outcome": o}),
                ),
                Err(e) => err(502, e.to_string()),
            }
        }
        // Native adapters (Slack, Gmail, GitHub): catalog, token connect, call.
        "catalog" => match hub_for_proxy(root) {
            Ok(h) => {
                let accounts: Vec<_> = h
                    .store()
                    .list()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|a| a.project == INSTANCE_SCOPE)
                    .collect();
                (
                    200,
                    json!({"ok": true, "toolkits": h.catalog(), "accounts": accounts}),
                )
            }
            Err(e) => err(500, e.to_string()),
        },
        "credential" if !admin => err(403, "chef or admin role required".into()),
        "credential" => {
            let (Some(toolkit), Some(credential)) = (s("toolkit"), s("credential")) else {
                return err(400, "toolkit and credential required".into());
            };
            let result = hub_for_proxy(root).and_then(|h| {
                let config = AuthConfig {
                    id: format!("{toolkit}-token"),
                    toolkit: toolkit.clone(),
                    scheme: AuthSchemeKind::Bearer,
                    source: ClientCredentialSource::None,
                    client_id: None,
                    scopes: vec![],
                    enabled: true,
                };
                h.upsert_auth_config(&config, None)?;
                h.connect_with_credential(&config.id, INSTANCE_SCOPE, "workjet", credential.trim())
            });
            match result {
                Ok(account) => (200, json!({"ok": true, "account": account})),
                Err(e) => err(400, e.to_string()),
            }
        }
        "call" => {
            let name = s("tool").unwrap_or_default();
            let mut argv = vec![
                name,
                "--json".into(),
                body.get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}))
                    .to_string(),
            ];
            if body.get("confirm").and_then(Value::as_bool) == Some(true) {
                argv.push("--confirm".into());
            }
            match hub_for_proxy(root) {
                Ok(h) => {
                    let (code, value) = exec(&h, &argv, &|_| Ok(false));
                    (
                        200,
                        json!({"ok": code == exit::OK, "exit_code": code, "outcome": value}),
                    )
                }
                Err(e) => err(500, e.to_string()),
            }
        }
        other => err(404, format!("unknown toolhub route `{other}`")),
    }
}

/// `exec <server>.<tool>` against a connected remote MCP server.
fn exec_mcp(root: &Path, args: &[String], projects: ProjectResolver<'_>) -> (i32, Value) {
    use ctox_toolhub::remote_mcp::McpOutcome;
    let Some((server, tool)) = args.first().and_then(|n| n.split_once('.')) else {
        return validation("missing <server>.<tool>");
    };
    let raw = match flag(args, "--json") {
        Some(j) => j.to_string(),
        None if std::io::IsTerminal::is_terminal(&std::io::stdin()) => "{}".into(),
        None => match read_stdin() {
            Ok(s) if s.trim().is_empty() => "{}".into(),
            Ok(s) => s,
            Err(e) => return validation(e),
        },
    };
    let tool_args: Value = match serde_json::from_str(&raw) {
        Ok(v @ Value::Object(_)) => v,
        Ok(_) => return validation("tool arguments must be a JSON object"),
        Err(e) => return validation(format!("invalid JSON arguments: {e}")),
    };
    let project = match mcp_project(args, projects) {
        Ok(p) => p,
        Err(e) => return validation(e),
    };
    // Network client mode: harnesses/PCs outside the instance call the central
    // proxy instead of holding a local store (CTOX_TOOLHUB_PROXY_URL + capability token).
    if let Ok(base) = std::env::var("CTOX_TOOLHUB_PROXY_URL") {
        return exec_via_proxy(&base, server, tool, &tool_args, has(args, "--confirm"));
    }
    let hub = match mcp(root) {
        Ok(h) => h,
        Err(e) => {
            return (
                exit::INTERNAL,
                json!({"outcome": "error", "message": e.to_string()}),
            )
        }
    };
    // Approval gate: tools without MCP readOnlyHint need --confirm.
    if !has(args, "--confirm") {
        match hub.list_tools(server, &project) {
            Ok(tools) => match tools.iter().find(|t| t.name == tool) {
                None => return validation(format!("unknown tool `{tool}` on `{server}`")),
                Some(t) if t.mutating() => {
                    return (
                        exit::APPROVAL_REQUIRED,
                        json!({"outcome": "approval_required", "tool": format!("{server}.{tool}"),
                        "reason": {"de": "Dieses Tool kann Daten beim Anbieter ändern und braucht eine Freigabe.",
                                   "en": "This tool may change data at the provider and needs approval."},
                        "confirm_hint": "re-run the same command with --confirm after approval"}),
                    );
                }
                Some(_) => {}
            },
            Err(_) => {
                let v = hub.0.client().credential_required(server, &project);
                return (
                    exit::CREDENTIAL_REQUIRED,
                    serde_json::to_value(v).unwrap_or(Value::Null),
                );
            }
        }
    }
    match hub.0.client().call_tool(server, &project, tool, &tool_args) {
        Ok(o) => {
            let code = match &o {
                McpOutcome::Success { .. } => exit::OK,
                McpOutcome::ToolError { .. } => exit::UPSTREAM,
                McpOutcome::CredentialRequired { .. } => exit::CREDENTIAL_REQUIRED,
            };
            (code, serde_json::to_value(&o).unwrap_or(Value::Null))
        }
        Err(e) => (
            exit::UPSTREAM,
            json!({"outcome": "provider_error", "message": e.to_string()}),
        ),
    }
}

/// Start the local callback server as a detached `ctox` child so `connect`
/// returns immediately (chainable). Returns false if the port is busy.
fn spawn_callback_server(account_id: &str) -> Result<bool> {
    let port = loopback_port()?;
    if std::net::TcpListener::bind(("127.0.0.1", port)).is_err() {
        eprintln!(
            "[toolhub] callback port {port} busy; use `ctox toolhub complete` for {account_id}"
        );
        return Ok(false);
    }
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .args(["toolhub", "_callback-server"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("failed to start local OAuth callback server")?;
    Ok(true)
}

fn validation(message: impl std::fmt::Display) -> (i32, Value) {
    (
        exit::VALIDATION,
        json!({"outcome": "validation_error", "message": message.to_string()}),
    )
}

/// `exec` core: returns (exit code, stdout JSON). Never includes credentials.
pub fn exec<B: SecretBackend>(
    hub: &ToolHub<B>,
    args: &[String],
    projects: ProjectResolver<'_>,
) -> (i32, Value) {
    let Some(name) = args.first().filter(|a| !a.starts_with("--")) else {
        return validation("missing <toolkit>.<action>");
    };
    let spec = match hub.tool(name) {
        Ok((_, spec)) => spec.clone(),
        Err(e) => return validation(e),
    };
    let raw = match flag(args, "--json") {
        Some(j) => j.to_string(),
        None if std::io::IsTerminal::is_terminal(&std::io::stdin()) => "{}".into(),
        None => match read_stdin() {
            Ok(s) if s.trim().is_empty() => "{}".into(),
            Ok(s) => s,
            Err(e) => return validation(e),
        },
    };
    let tool_args: Value = match serde_json::from_str(&raw) {
        Ok(v @ Value::Object(_)) => v,
        Ok(_) => return validation("tool arguments must be a JSON object"),
        Err(e) => return validation(format!("invalid JSON arguments: {e}")),
    };
    // Mutating tools need an explicit, separately approvable invocation: the
    // `--confirm` command line is what the harness approval policy gates.
    if spec.mutating && !has(args, "--confirm") {
        return (
            exit::APPROVAL_REQUIRED,
            json!({
                "outcome": "approval_required",
                "tool": spec.slug,
                "reason": {
                    "de": "Dieses Tool ändert Daten beim Anbieter und braucht eine Freigabe.",
                    "en": "This tool changes data at the provider and needs approval."
                },
                "confirm_hint": "re-run the same command with --confirm after approval"
            }),
        );
    }
    let project = match mcp_project(args, projects) {
        Ok(p) => p,
        Err(e) => return validation(e),
    };
    match hub.execute(&project, &spec.slug, &tool_args, flag(args, "--account")) {
        Ok(outcome) => {
            let code = match &outcome {
                ToolOutcome::Success { .. } => exit::OK,
                ToolOutcome::ProviderError { .. } => exit::UPSTREAM,
                ToolOutcome::CredentialRequired { .. } => exit::CREDENTIAL_REQUIRED,
            };
            (code, serde_json::to_value(&outcome).unwrap_or(Value::Null))
        }
        Err(e) => {
            let message = e.to_string();
            if message.starts_with("missing argument") || message.starts_with("invalid argument") {
                validation(message)
            } else {
                (
                    exit::INTERNAL,
                    json!({"outcome": "error", "message": message}),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_toolhub::MemorySecretBackend;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn mem_hub() -> ToolHub<MemorySecretBackend> {
        ToolHub::new(
            MemorySecretBackend::default(),
            Arc::new(UreqTransport::default()),
            RedirectTarget::Loopback { port: 1 },
        )
    }

    fn known(id: &str) -> Result<bool> {
        Ok(id == "p")
    }

    #[test]
    fn exec_exit_codes_are_stable() {
        let hub = mem_hub();
        let (c, v) = exec(&hub, &args(&["github.nope", "--json", "{}"]), &known);
        assert_eq!(
            (c, v["outcome"].as_str()),
            (exit::VALIDATION, Some("validation_error"))
        );
        let (c, _) = exec(&hub, &args(&["github.get_repo", "--json", "[]"]), &known);
        assert_eq!(c, exit::VALIDATION);
        let (c, v) = exec(
            &hub,
            &args(&[
                "github.create_issue",
                "--json",
                r#"{"owner":"a","repo":"b","title":"t"}"#,
            ]),
            &known,
        );
        assert_eq!(
            (c, v["outcome"].as_str()),
            (exit::APPROVAL_REQUIRED, Some("approval_required"))
        );
        let (c, v) = exec(
            &hub,
            &args(&[
                "github.get_repo",
                "--json",
                r#"{"owner":"a","repo":"b"}"#,
                "--project",
                "p",
            ]),
            &known,
        );
        assert_eq!(c, exit::CREDENTIAL_REQUIRED);
        assert_eq!(v["auth_schemes"], json!(["oauth2", "bearer"]));
        assert!(v["connect_hint"]
            .as_str()
            .unwrap()
            .contains("config-set --toolkit github"));
        let (c, _) = exec(
            &hub,
            &args(&[
                "github.get_repo",
                "--json",
                r#"{"owner":"a/..","repo":"b"}"#,
                "--project",
                "p",
            ]),
            &known,
        );
        assert_eq!(
            c,
            exit::VALIDATION,
            "arguments are validated before credentials"
        );
    }

    #[test]
    fn project_must_exist_and_be_valid() {
        let hub = mem_hub();
        for bad in ["unknown", "../x"] {
            let (c, v) = exec(
                &hub,
                &args(&[
                    "github.get_repo",
                    "--json",
                    r#"{"owner":"a","repo":"b"}"#,
                    "--project",
                    bad,
                ]),
                &known,
            );
            assert_eq!(c, exit::VALIDATION, "{v}");
        }
        assert!(resolve_project(&args(&["--project", "p"]), &known).is_ok());
    }

    #[test]
    fn central_proxy_token_connect_catalog_and_approval_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Non-admins cannot store credentials.
        let (code, _) = handle_toolhub_http(
            root,
            "credential",
            false,
            &json!({"toolkit": "slack", "credential": "xoxb-test-123"}),
        );
        assert_eq!(code, 403);
        let (code, v) = handle_toolhub_http(
            root,
            "credential",
            true,
            &json!({"toolkit": "slack", "credential": "xoxb-test-123"}),
        );
        assert_eq!(code, 200, "{v}");
        let (_, cat) = handle_toolhub_http(root, "catalog", false, &Value::Null);
        assert!(cat["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["toolkit"] == "slack" && a["status"] == "ACTIVE"));
        assert!(
            !cat.to_string().contains("xoxb-test-123"),
            "token must never leave the instance"
        );
        assert!(cat["toolkits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["slug"] == "gmail"));
        // Mutating tool through the proxy without confirm -> approval_required, no provider call.
        let (_, call) = handle_toolhub_http(
            root,
            "call",
            false,
            &json!({"tool": "slack.send_message", "arguments": {"channel": "C1", "text": "hi"}}),
        );
        assert_eq!(call["exit_code"], exit::APPROVAL_REQUIRED);
        // Validation errors come back as exit 2.
        let (_, bad) = handle_toolhub_http(
            root,
            "call",
            false,
            &json!({"tool": "slack.get_history", "arguments": {"channel": "C1&x"}}),
        );
        assert_eq!(bad["exit_code"], exit::VALIDATION);
        let (_, gone) = handle_toolhub_http(root, "disconnect", true, &json!({"toolkit": "slack"}));
        assert_eq!(gone["removed"], 1);
        assert_eq!(handle_toolhub_http(root, "nope", true, &Value::Null).0, 404);
    }

    #[test]
    fn ctox_secret_backend_roundtrip_in_encrypted_store() {
        let dir = tempfile::tempdir().unwrap();
        let b = CtoxSecretBackend::new(dir.path());
        b.put(
            "toolhub:github:ca_1",
            "token",
            "{\"access_token\":\"x\"}",
            json!({"status": "ACTIVE"}),
        )
        .unwrap();
        assert_eq!(
            b.get("toolhub:github:ca_1", "token").unwrap().as_deref(),
            Some("{\"access_token\":\"x\"}")
        );
        let listed = b.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].metadata["status"], "ACTIVE");
        b.delete("toolhub:github:ca_1", "token").unwrap();
        assert!(b.get("toolhub:github:ca_1", "token").unwrap().is_none());
    }
}
