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

use super::toolhub_role;
use super::toolhub_ui;
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
        // Loopback redirect (RFC 8252), exactly like "LLM einbinden": the
        // client that opens the authorize URL (Workjet in-app browser on the
        // user's PC) owns the listener and hands state + code to THIS instance
        // as the sync command `ctox.toolhub.oauth.complete`. The instance
        // never serves an HTTP callback.
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
  connect --auth-config <id> [--project <project_id>] [--actor <a>] [--credential-stdin [--param key=value]...] [--no-callback-server]
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

/// Repeated `--param key=value` flags (per-connection parameters).
fn params(args: &[String]) -> Result<std::collections::BTreeMap<String, String>> {
    let mut out = std::collections::BTreeMap::new();
    for (i, a) in args.iter().enumerate() {
        if a == "--param" {
            let kv = args.get(i + 1).context("--param needs key=value")?;
            let (k, v) = kv.split_once('=').context("--param needs key=value")?;
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Ok(out)
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
                return print(&hub.connect_with_credential_params(
                    config,
                    &project,
                    &actor,
                    &read_stdin()?,
                    &params(rest)?,
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
            let (code, mut value) = exec(&hub, rest, &resolver);
            if code == exit::CREDENTIAL_REQUIRED
                && value.is_object()
                && !has(rest, "--no-ui-request")
            {
                // Surface a card in the session instead of a dead end — but only
                // the supervisor raises cards; workers hand a needs_user back.
                let role = toolhub_role::resolve(root, session_from(rest).as_deref());
                if let Some(toolkit) = rest
                    .first()
                    .and_then(|n| n.split('.').next())
                    .filter(|_| !role.may_raise_cards())
                {
                    let kind = card_kind_for(root, toolkit);
                    let hand_off = toolhub_role::needs_user(
                        role,
                        &serde_json::to_value(kind)?.as_str().unwrap_or("credential").to_string(),
                        toolkit,
                        "credential_required",
                        value.get("fields").cloned().unwrap_or_else(|| json!([])),
                    );
                    value["needs_user"] = hand_off["needs_user"].clone();
                    value["next"] = hand_off["next"].clone();
                } else if let Some(toolkit) = rest.first().and_then(|n| n.split('.').next()) {
                    let kind = card_kind_for(root, toolkit);
                    if let Ok(mut r) = toolhub_ui::create(
                        root,
                        kind,
                        toolkit,
                        vec![],
                        None,
                        session_from(rest),
                        900,
                    ) {
                        attach_connection_fields(root, &mut r);
                        value["ui_request"] = toolhub_ui::summary(&r);
                        value["next"] = json!(format!(
                            "ctox ui status {} (or rerun after the user connects)",
                            r.id
                        ));
                    }
                }
            }
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

/// Redirect URI handed to providers: loopback on the client's machine.
/// State, PKCE verifier, DCR registration and tokens stay on this instance;
/// the code reaches it only through ctox sync (`ctox.toolhub.oauth.complete`).
pub fn mcp_redirect_uri(_root: &Path) -> Result<String> {
    Ok(RedirectTarget::Loopback {
        port: loopback_port()?,
    }
    .redirect_uri())
}

fn mcp(root: &Path) -> Result<McpRef> {
    Ok(McpRef(McpHost {
        backend: CtoxSecretBackend::new(root),
        transport: UreqTransport::default(),
        redirect: mcp_redirect_uri(root).unwrap_or_default(),
    }))
}

/// Finish an OAuth flow of THIS instance from the provider's callback URL:
/// remote MCP connector first, then toolkit OAuth. Unknown state = error,
/// nothing is consumed.
pub fn complete_oauth_callback(root: &Path, url: &str) -> Result<Value> {
    match mcp(root).and_then(|h| h.complete(url, INSTANCE_SCOPE)) {
        Ok(info) => Ok(json!({"kind": "mcp", "connection": info})),
        Err(mcp_err) => match hub(root).and_then(|h| h.complete_oauth(url)) {
            Ok(account) => Ok(json!({"kind": "toolkit", "toolkit": account.toolkit, "account_id": account.id})),
            Err(e) => Err(anyhow::anyhow!("{mcp_err:#}; {e:#}")),
        },
    }
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

/// Transport-neutral toolhub request handler of the instance. Reached only
/// locally (CLI, Business OS MCP) and through ctox sync commands
/// (`ctox.toolhub.*`, see [`handle_toolhub_sync_command`]); there is no HTTP
/// route. Returns (status code in HTTP semantics, JSON).
pub fn handle_toolhub_request(root: &Path, route: &str, admin: bool, body: &Value) -> (u16, Value) {
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
                    mcp_redirect_uri(root)
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
                let fields: serde_json::Map<String, Value> = h
                    .catalog()
                    .iter()
                    .filter_map(|t| {
                        let f = h.connection_fields(&t.slug).ok()?;
                        (!f.is_empty()).then(|| (t.slug.clone(), json!(f)))
                    })
                    .collect();
                (
                    200,
                    json!({"ok": true, "toolkits": h.catalog(), "accounts": accounts, "connection_fields": fields}),
                )
            }
            Err(e) => err(500, e.to_string()),
        },
        "credential" if !admin => err(403, "chef or admin role required".into()),
        "credential" => {
            let (Some(toolkit), Some(credential)) = (s("toolkit"), s("credential")) else {
                return err(400, "toolkit and credential required".into());
            };
            let given = params_from(&body);
            match store_token(root, &toolkit, &credential, &given) {
                Ok(account) => (200, json!({"ok": true, "account": account})),
                Err(e) => err(400, e.to_string()),
            }
        }
        // In-session cards (see toolhub_ui). Secrets go to the secret store,
        // never into the request file or the response.
        "ui/requests" => {
            let session = s("session");
            match toolhub_ui::list(root) {
                Ok(all) => {
                    let horizon = now_secs().saturating_sub(24 * 3600);
                    let items: Vec<_> = all
                        .into_iter()
                        .filter(|r| r.created_at >= horizon)
                        .filter(|r| {
                            session.is_none() || r.session.is_none() || r.session == session
                        })
                        .collect();
                    (200, json!({"ok": true, "requests": items}))
                }
                Err(e) => err(500, e.to_string()),
            }
        }
        "ui/resolve" if !admin => err(403, "chef or admin role required".into()),
        "ui/resolve" => {
            let id = s("id").unwrap_or_default();
            let action = s("action").unwrap_or_default();
            let result = match action.as_str() {
                "submit" => toolhub_ui::resolve_credential(
                    root,
                    &id,
                    body.get("values").unwrap_or(&Value::Null),
                    |toolkit, credential, params| {
                        store_token(root, toolkit, credential, params).map(|_| ())
                    },
                ),
                "decline" => toolhub_ui::finish(root, &id, toolhub_ui::UiStatus::Declined),
                "approve" => toolhub_ui::get(root, &id).and_then(|r| {
                    if r.kind != toolhub_ui::UiKind::Confirm {
                        bail!("not a confirm request");
                    }
                    toolhub_ui::finish(root, &id, toolhub_ui::UiStatus::Done)
                }),
                "connected" => toolhub_ui::get(root, &id).and_then(|r| {
                    if toolkit_connected(root, &r.toolkit) {
                        toolhub_ui::finish(root, &id, toolhub_ui::UiStatus::Done)
                    } else {
                        Ok(r)
                    }
                }),
                other => Err(anyhow::anyhow!("unknown action `{other}`")),
            };
            match result {
                Ok(r) => (200, json!({"ok": true, "request": r})),
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
        "oauth/complete" if !admin => err(403, "chef or admin role required".into()),
        "oauth/complete" => {
            let Some(callback) = s("callback_url") else {
                return err(400, "callback_url required".into());
            };
            match complete_oauth_callback(root, &callback) {
                Ok(v) => (200, json!({"ok": true, "result": v})),
                Err(e) => err(400, e.to_string()),
            }
        }
        other => err(404, format!("unknown toolhub route `{other}`")),
    }
}

/// Business OS MCP tools `business_os.toolhub_{list,catalog,exec,needs_user}`:
/// the same instance handler as the sync commands `ctox.toolhub.*`, so
/// harnesses reach the Toolhub through their MCP connection.
/// Role-aware: only supervisor/operator calls raise a user card; workers get
/// a structured `needs_user` for their supervisor.
pub fn handle_toolhub_mcp_tool(root: &Path, tool: &str, args: &Value) -> Result<Value> {
    let thread = args.get("thread_id").and_then(Value::as_str);
    let claim = args.get("team_role").and_then(Value::as_str);
    let role = {
        let conn = crate::business_os::store::open_store(root).ok();
        toolhub_role::resolve_claimed(conn.as_ref(), thread, claim)
    };
    let routed = |route: &str, body: &Value| -> Result<Value> {
        let (status, value) = handle_toolhub_request(root, route, false, body);
        if status != 200 {
            bail!(
                "{}",
                value.get("message").and_then(Value::as_str).unwrap_or("toolhub request failed")
            );
        }
        Ok(value)
    };
    match tool {
        "business_os.toolhub_list" => {
            let catalog = routed("catalog", &Value::Null)?;
            let mcp = routed("connections", &Value::Null)?;
            Ok(json!({"ok": true, "accounts": catalog["accounts"], "mcp_connections": mcp["connections"], "role": role.as_str()}))
        }
        "business_os.toolhub_catalog" => {
            let mut catalog = routed("catalog", &Value::Null)?;
            if let Some(filter) = args.get("toolkit").and_then(Value::as_str) {
                if let Some(list) = catalog["toolkits"].as_array_mut() {
                    list.retain(|t| t["slug"] == filter);
                }
            }
            Ok(catalog)
        }
        "business_os.toolhub_exec" => {
            let name = args.get("tool").and_then(Value::as_str).unwrap_or_default();
            let body = json!({
                "tool": name,
                "arguments": args.get("arguments").cloned().unwrap_or_else(|| json!({})),
                "confirm": args.get("confirm").and_then(Value::as_bool).unwrap_or(false),
            });
            let mut value = routed("call", &body)?;
            if value["exit_code"] == exit::CREDENTIAL_REQUIRED {
                let toolkit = name.split('.').next().unwrap_or_default();
                let fields = value["outcome"].get("fields").cloned().unwrap_or_else(|| json!([]));
                value["needs_user"] = raise_or_hand_off(root, role, toolkit, "credential_required", thread, fields)?;
            }
            value["role"] = json!(role.as_str());
            Ok(value)
        }
        "business_os.toolhub_needs_user" => {
            let toolkit = args.get("toolkit").and_then(Value::as_str).unwrap_or_default();
            let reason = args.get("reason").and_then(Value::as_str).unwrap_or("user input required");
            let fields = args.get("fields").cloned().unwrap_or_else(|| json!([]));
            Ok(json!({"ok": true, "role": role.as_str(),
                "needs_user": raise_or_hand_off(root, role, toolkit, reason, thread, fields)?}))
        }
        other => bail!("unknown toolhub tool `{other}`"),
    }
}

/// Supervisor/operator: create (deduped) the card in the thread's session.
/// Workers: never a card, only the hand-off object.
fn raise_or_hand_off(
    root: &Path,
    role: toolhub_role::TeamRole,
    toolkit: &str,
    reason: &str,
    thread: Option<&str>,
    fields: Value,
) -> Result<Value> {
    let kind = card_kind_for(root, toolkit);
    let kind_str = serde_json::to_value(kind)?.as_str().unwrap_or("credential").to_string();
    let mut out = toolhub_role::needs_user(role, &kind_str, toolkit, reason, fields)["needs_user"].clone();
    if role.may_raise_cards() {
        let mut r = toolhub_ui::create(
            root,
            kind,
            toolkit,
            vec![],
            Some(reason.to_string()),
            thread.map(str::to_string),
            900,
        )?;
        attach_connection_fields(root, &mut r);
        out["ui_request"] = toolhub_ui::summary(&r);
        out["next"] = json!("A card is pinned in the chat. End the turn as waiting; retry after it is resolved.");
    } else {
        out["next"] = json!(if role == toolhub_role::TeamRole::PersistentWorker {
            "Hand this needs_user to your supervisor (workjet_escalate) and park."
        } else {
            "Stop and end your run with this needs_user; the supervisor re-dispatches you."
        });
    }
    Ok(out)
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

/// Sync command types of the toolhub. Clients (Workjet, Business OS shell)
/// write them into the replicated `business_commands` collection; the instance
/// executes them and writes the outcome back into the same document.
pub const SYNC_COMMAND_TYPES: [&str; 3] = [
    "ctox.toolhub.request",
    "ctox.toolhub.credential.submit",
    "ctox.toolhub.oauth.complete",
];

/// Routes allowed through `ctox.toolhub.request`. Secret-carrying routes have
/// their own command type whose secret field is detached before persistence.
const SYNC_REQUEST_ROUTES: [&str; 8] = [
    "connections", "catalog", "connect", "disconnect", "mcp/tools", "mcp/call", "call", "ui/requests",
];

/// Execute one toolhub sync command on this instance. `secret` is the value the
/// command bus detached from the replicated document (never persisted):
/// `credential.submit` -> the credential (or a JSON object of card values),
/// `oauth.complete` -> the full loopback callback URL (state + code).
pub fn handle_toolhub_sync_command(
    root: &Path,
    command_type: &str,
    payload: &Value,
    secret: Option<String>,
    admin: bool,
) -> (u16, Value) {
    let err = |status: u16, m: &str| (status, json!({"ok": false, "message": m}));
    let body = payload.get("body").cloned().unwrap_or_else(|| json!({}));
    match command_type {
        "ctox.toolhub.request" => {
            let route = payload.get("route").and_then(Value::as_str).unwrap_or_default();
            if !SYNC_REQUEST_ROUTES.contains(&route) {
                return err(400, "route not allowed over ctox.toolhub.request");
            }
            handle_toolhub_request(root, route, admin, &body)
        }
        "ctox.toolhub.credential.submit" => {
            let Some(secret) = secret.filter(|v| !v.trim().is_empty()) else {
                return err(400, "Zugangsdaten fehlen, bitte erneut speichern / credential missing, please save again");
            };
            let mut body = body;
            if let Some(id) = body.get("ui_request_id").and_then(Value::as_str).map(str::to_string) {
                let values = serde_json::from_str::<Value>(&secret)
                    .ok()
                    .filter(Value::is_object)
                    .unwrap_or_else(|| json!({"credential": secret}));
                return handle_toolhub_request(
                    root,
                    "ui/resolve",
                    admin,
                    &json!({"id": id, "action": "submit", "values": values}),
                );
            }
            body["credential"] = json!(secret);
            handle_toolhub_request(root, "credential", admin, &body)
        }
        "ctox.toolhub.oauth.complete" => {
            let Some(callback) = secret.filter(|v| !v.trim().is_empty()) else {
                return err(400, "Anmeldung unvollständig, bitte erneut verbinden / sign-in incomplete, please connect again");
            };
            handle_toolhub_request(root, "oauth/complete", admin, &json!({"callback_url": callback}))
        }
        _ => err(404, "unknown toolhub sync command"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_toolhub::MemorySecretBackend;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let r = std::env::temp_dir().join(format!("ctox-toolhub-mcp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(&r).unwrap();
        r
    }

    /// Two instances on one machine: public origin, callback URI, credentials
    /// and OAuth states are per instance; nothing leaks across.
    #[test]
    fn two_instances_share_no_toolhub_state() {
        let a = temp_root("iso-a");
        let b = temp_root("iso-b");
        let redirect = mcp_redirect_uri(&a).unwrap();
        assert!(redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/toolhub/oauth/callback"), "{redirect}");

        // Credentials: stored in A only.
        store_token(&a, "linear", "lin_api_isolation_test", &Default::default()).unwrap();
        assert_eq!(hub(&a).unwrap().store().list().unwrap().len(), 1);
        assert!(hub(&b).unwrap().store().list().unwrap().is_empty(), "B must not see A's connection");

        // OAuth: A's state, verifier and redirect URI live in A only.
        let ha = hub(&a).unwrap();
        ha.upsert_auth_config(
            &AuthConfig {
                id: "gh".into(),
                toolkit: "github".into(),
                scheme: AuthSchemeKind::OAuth2,
                source: ClientCredentialSource::None,
                client_id: Some("client-a".into()),
                scopes: vec![],
                enabled: true,
            },
            None,
        )
        .unwrap();
        let req = ha.connect_oauth("gh", INSTANCE_SCOPE, "test").unwrap();
        assert!(req.redirect_url.contains("redirect_uri=http%3A%2F%2F127.0.0.1"), "{}", req.redirect_url);
        // The loopback capture reaches B as a sync command: B must reject it.
        let callback = format!("{redirect}?code=c&state={}", req.state);
        let (status, v) = handle_toolhub_sync_command(&b, "ctox.toolhub.oauth.complete", &json!({}), Some(callback), true);
        assert_eq!(status, 400, "{v}");
        assert!(v["message"].as_str().unwrap().contains("unknown or already used OAuth state"), "{v}");
        // B's attempt consumed nothing in A: A still holds its pending state.
        let still = hub(&a).unwrap();
        let acct = still.store().list().unwrap().into_iter().find(|x| x.id == req.account_id).unwrap();
        let pending = still.store().take_pending(&acct).unwrap().expect("A's pending state intact");
        assert_eq!(pending.state, req.state);
        assert_eq!(pending.redirect_uri, redirect);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn sync_commands_gate_secrets_and_roles() {
        let root = temp_root("sync-gate");
        // Secret routes are not reachable through the generic request command.
        for route in ["credential", "ui/resolve", "oauth/complete"] {
            let (code, _) = handle_toolhub_sync_command(&root, "ctox.toolhub.request",
                &json!({"route": route, "body": {}}), None, true);
            assert_eq!(code, 400, "{route}");
        }
        // Credential submit without the detached secret fails closed.
        let (code, _) = handle_toolhub_sync_command(&root, "ctox.toolhub.credential.submit",
            &json!({"body": {"toolkit": "linear"}}), None, true);
        assert_eq!(code, 400);
        // Non-admin sessions cannot store credentials.
        let (code, _) = handle_toolhub_sync_command(&root, "ctox.toolhub.credential.submit",
            &json!({"body": {"toolkit": "linear"}}), Some("lin_api_sync_test".into()), false);
        assert_eq!(code, 403);
        // Admin: stored on the instance; the reply carries no secret.
        let (code, v) = handle_toolhub_sync_command(&root, "ctox.toolhub.credential.submit",
            &json!({"body": {"toolkit": "linear"}}), Some("lin_api_sync_test".into()), true);
        assert_eq!(code, 200, "{v}");
        assert!(!v.to_string().contains("lin_api_sync_test"), "secret echoed: {v}");
        let (code, cat) = handle_toolhub_sync_command(&root, "ctox.toolhub.request",
            &json!({"route": "catalog"}), None, false);
        assert_eq!(code, 200);
        assert_eq!(cat["accounts"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mcp_needs_user_raises_card_only_for_supervisor_or_operator() {
        // Workers: hand-off only, nothing pinned in the chat.
        for (claim, tag) in [("one_shot_worker", "os"), ("persistent_worker", "pw"), ("supervisor", "unbound-sup")] {
            let root = temp_root(tag);
            let out = handle_toolhub_mcp_tool(&root, "business_os.toolhub_needs_user",
                &json!({"toolkit": "jira", "thread_id": "t1", "team_role": claim})).unwrap();
            assert!(out["needs_user"].get("ui_request").is_none(), "{claim} must not raise a card");
            assert_eq!(out["needs_user"]["toolkit"], "jira");
            assert!(toolhub_ui::list(&root).unwrap_or_default().is_empty(), "{claim}");
        }
        // Owner-facing thread (operator claim, unbound): card pinned to the thread.
        let root = temp_root("op");
        let out = handle_toolhub_mcp_tool(&root, "business_os.toolhub_needs_user",
            &json!({"toolkit": "jira", "thread_id": "t1", "team_role": "operator"})).unwrap();
        assert_eq!(out["role"], "operator");
        assert!(out["needs_user"]["ui_request"].is_object());
        let open = toolhub_ui::list(&root).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].session.as_deref(), Some("t1"));
        // Deduped on repeat.
        handle_toolhub_mcp_tool(&root, "business_os.toolhub_needs_user",
            &json!({"toolkit": "jira", "thread_id": "t1", "team_role": "operator"})).unwrap();
        assert_eq!(toolhub_ui::list(&root).unwrap().len(), 1);
    }

    #[test]
    fn mcp_unknown_toolhub_tool_is_rejected() {
        assert!(handle_toolhub_mcp_tool(&temp_root("unk"), "business_os.toolhub_nope", &json!({})).is_err());
    }

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
    fn ui_card_credential_lands_in_secret_store_and_unblocks_cli() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        assert_eq!(
            card_kind_for(&root, "slack"),
            toolhub_ui::UiKind::Credential
        );
        assert_eq!(card_kind_for(&root, "vercel"), toolhub_ui::UiKind::Connect);
        let r = toolhub_ui::create(
            &root,
            toolhub_ui::UiKind::Credential,
            "slack",
            vec![],
            None,
            Some("t1".into()),
            600,
        )
        .unwrap();
        let (_, listed) =
            handle_toolhub_request(&root, "ui/requests", false, &json!({"session": "t1"}));
        assert_eq!(listed["requests"][0]["id"], r.id);
        let (_, other) =
            handle_toolhub_request(&root, "ui/requests", false, &json!({"session": "t2"}));
        assert!(other["requests"].as_array().unwrap().is_empty());
        let waiter = {
            let root = root.clone();
            let id = r.id.clone();
            std::thread::spawn(move || toolhub_ui::wait(&root, &id, |_| false).unwrap())
        };
        let body = json!({"id": r.id, "action": "submit", "values": {"token": "xoxb-card-999"}});
        assert_eq!(
            handle_toolhub_request(&root, "ui/resolve", false, &body).0,
            403
        );
        let (code, v) = handle_toolhub_request(&root, "ui/resolve", true, &body);
        assert_eq!(code, 200, "{v}");
        assert!(!v.to_string().contains("xoxb-card-999"));
        let done = waiter.join().unwrap();
        assert_eq!(toolhub_ui::exit_code(done.status), toolhub_ui::exit::DONE);
        assert!(toolkit_connected(&root, "slack"));
        let (_, cat) = handle_toolhub_request(&root, "catalog", false, &Value::Null);
        assert!(!cat.to_string().contains("xoxb-card-999"));
        // Connect cards only finish once the connection is really live.
        let c = toolhub_ui::create(
            &root,
            toolhub_ui::UiKind::Connect,
            "vercel",
            vec![],
            None,
            None,
            600,
        )
        .unwrap();
        let (_, v) = handle_toolhub_request(
            &root,
            "ui/resolve",
            true,
            &json!({"id": c.id, "action": "connected"}),
        );
        assert_eq!(v["request"]["status"], "open");
        let (_, v) = handle_toolhub_request(
            &root,
            "ui/resolve",
            true,
            &json!({"id": c.id, "action": "decline"}),
        );
        assert_eq!(v["request"]["status"], "declined");
    }

    #[test]
    fn central_proxy_token_connect_catalog_and_approval_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Non-admins cannot store credentials.
        let (code, _) = handle_toolhub_request(
            root,
            "credential",
            false,
            &json!({"toolkit": "slack", "credential": "xoxb-test-123"}),
        );
        assert_eq!(code, 403);
        let (code, v) = handle_toolhub_request(
            root,
            "credential",
            true,
            &json!({"toolkit": "slack", "credential": "xoxb-test-123"}),
        );
        assert_eq!(code, 200, "{v}");
        let (_, cat) = handle_toolhub_request(root, "catalog", false, &Value::Null);
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
        let (_, call) = handle_toolhub_request(
            root,
            "call",
            false,
            &json!({"tool": "slack.send_message", "arguments": {"channel": "C1", "text": "hi"}}),
        );
        assert_eq!(call["exit_code"], exit::APPROVAL_REQUIRED);
        // Validation errors come back as exit 2.
        let (_, bad) = handle_toolhub_request(
            root,
            "call",
            false,
            &json!({"tool": "slack.get_history", "arguments": {"channel": "C1&x"}}),
        );
        assert_eq!(bad["exit_code"], exit::VALIDATION);
        let (_, gone) = handle_toolhub_request(root, "disconnect", true, &json!({"toolkit": "slack"}));
        assert_eq!(gone["removed"], 1);
        assert_eq!(handle_toolhub_request(root, "nope", true, &Value::Null).0, 404);
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

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Store a token credential for a native toolkit, instance-wide.
/// `params` object of a proxy body (per-connection parameters, non-secret).
fn params_from(body: &Value) -> std::collections::BTreeMap<String, String> {
    body.get("params")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn store_token(
    root: &Path,
    toolkit: &str,
    credential: &str,
    params: &std::collections::BTreeMap<String, String>,
) -> Result<Value> {
    let h = hub_for_proxy(root)?;
    let config = AuthConfig {
        id: format!("{toolkit}-token"),
        toolkit: toolkit.to_string(),
        scheme: h.credential_scheme(toolkit)?,
        source: ClientCredentialSource::None,
        client_id: None,
        scopes: vec![],
        enabled: true,
    };
    h.upsert_auth_config(&config, None)?;
    let account = h.connect_with_credential_params(
        &config.id,
        INSTANCE_SCOPE,
        "workjet",
        credential.trim(),
        params,
    )?;
    Ok(serde_json::to_value(account)?)
}

fn is_native_toolkit(root: &Path, toolkit: &str) -> bool {
    hub_for_proxy(root)
        .map(|h| {
            serde_json::to_value(h.catalog())
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .any(|t| t.get("slug").and_then(Value::as_str) == Some(toolkit))
        })
        .unwrap_or(false)
}

/// True once the instance holds a live connection for this toolkit.
fn toolkit_connected(root: &Path, toolkit: &str) -> bool {
    let mcp_live = mcp(root)
        .and_then(|h| h.list())
        .map(|l| {
            l.iter()
                .any(|c| c.server == toolkit && c.project == INSTANCE_SCOPE && c.status == "ACTIVE")
        })
        .unwrap_or(false);
    mcp_live
        || hub_for_proxy(root)
            .and_then(|h| h.store().list())
            .map(|l| {
                l.iter().any(|a| {
                    a.toolkit == toolkit
                        && a.project == INSTANCE_SCOPE
                        && serde_json::to_value(&a.status).ok() == Some(json!("ACTIVE"))
                })
            })
            .unwrap_or(false)
}

/// Kind of card that resolves a missing credential for this toolkit.
fn card_kind_for(root: &Path, toolkit: &str) -> toolhub_ui::UiKind {
    let remote = ctox_toolhub::remote_mcp::registry()
        .iter()
        .any(|s| s.slug == toolkit);
    if remote && !is_native_toolkit(root, toolkit) {
        toolhub_ui::UiKind::Connect
    } else {
        toolhub_ui::UiKind::Credential
    }
}

/// Credential cards also ask for the toolkit's per-connection params
/// (subdomain, instance URL, …) with DE+EN labels from the adapter.
fn attach_connection_fields(root: &Path, r: &mut toolhub_ui::UiRequest) {
    if r.kind != toolhub_ui::UiKind::Credential || !r.params.is_empty() {
        return;
    }
    let Ok(h) = hub_for_proxy(root) else { return };
    if let Ok(fields) = h.connection_fields(&r.toolkit) {
        if !fields.is_empty() {
            r.params = fields.iter().filter_map(|f| serde_json::to_value(f).ok()).collect();
            let _ = toolhub_ui::save_request(root, r);
        }
    }
}

fn session_from(args: &[String]) -> Option<String> {
    flag(args, "--session")
        .map(String::from)
        .or_else(|| std::env::var("CTOX_SESSION_ID").ok())
        .filter(|s| !s.trim().is_empty())
}

const UI_USAGE: &str = "usage: ctox ui request credential|connect|confirm --toolkit <slug> \\
  [--fields name[,name:plain]] [--message <text>] [--session <id>] [--timeout <secs>] [--no-wait]
  ctox ui status <id> | ctox ui list
exit codes (request): 0 done, 4 declined, 5 expired, 2 usage, 6 worker context (needs_user printed; only the supervisor raises cards)";

/// `ctox ui …`: create and wait for in-session cards.
pub fn handle_ui_command(root: &Path, args: &[String]) -> Result<()> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("request") => {
            let Some(kind) = rest.first().and_then(|k| toolhub_ui::parse_kind(k)) else {
                eprintln!("{UI_USAGE}");
                std::process::exit(toolhub_ui::exit::USAGE);
            };
            let Some(toolkit) = flag(rest, "--toolkit") else {
                eprintln!("{UI_USAGE}");
                std::process::exit(toolhub_ui::exit::USAGE);
            };
            let role = toolhub_role::resolve(root, session_from(rest).as_deref());
            if !role.may_raise_cards() {
                eprintln!(
                    "ctox ui request: only the CTOX supervisor may ask the user (this session is a {}). \
Return the needs_user below to your supervisor instead.",
                    role.as_str()
                );
                print(&toolhub_role::needs_user(
                    role,
                    rest.first().map(String::as_str).unwrap_or("credential"),
                    toolkit,
                    flag(rest, "--message").unwrap_or("user input required"),
                    json!(toolhub_ui::parse_fields(flag(rest, "--fields"))),
                ))?;
                std::process::exit(toolhub_ui::exit::NEEDS_SUPERVISOR);
            }
            let ttl = flag(rest, "--timeout")
                .and_then(|t| t.parse().ok())
                .unwrap_or(900);
            let mut r = toolhub_ui::create(
                root,
                kind,
                toolkit,
                toolhub_ui::parse_fields(flag(rest, "--fields")),
                flag(rest, "--message").map(String::from),
                session_from(rest),
                ttl,
            )?;
            attach_connection_fields(root, &mut r);
            if has(rest, "--no-wait") {
                return print(&toolhub_ui::summary(&r));
            }
            let done = toolhub_ui::wait(root, &r.id, |r| {
                r.kind == toolhub_ui::UiKind::Connect && toolkit_connected(root, &r.toolkit)
            })?;
            print(&toolhub_ui::summary(&done))?;
            let code = toolhub_ui::exit_code(done.status);
            if code != toolhub_ui::exit::DONE {
                std::process::exit(code);
            }
            Ok(())
        }
        Some("status") => print(&toolhub_ui::get(root, required(args, "status")?)?),
        Some("list") => print(&toolhub_ui::list(root)?),
        _ => bail!("{UI_USAGE}"),
    }
}
