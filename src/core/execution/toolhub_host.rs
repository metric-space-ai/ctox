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
/// Default CTOX project for connections when `--project` is omitted.
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
  catalog [--json]                      toolkits, tools, input schemas
  list [--json]                         connected accounts (no secrets)
  configs                               auth configs (no secrets)
  config-set --toolkit <slug> --id <id> --scheme <oauth2|bearer|api_key|basic> [--client-id <id>] [--client-secret-stdin] [--scope <s>]...
  connect --auth-config <id> [--project <p>] [--actor <a>] [--credential-stdin] [--no-callback-server]
  complete --callback <redirect-url-or-query>
  status|refresh|disconnect --account <id>
  exec <toolkit>.<action> [--json <args>|stdin] [--project <p>] [--account <id>] [--confirm]
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

fn project(args: &[String]) -> String {
    flag(args, "--project")
        .map(String::from)
        .or_else(|| std::env::var(PROJECT_ENV).ok().filter(|p| !p.is_empty()))
        .unwrap_or_else(|| "default".into())
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
            let (project, actor) = (project(rest), actor(rest));
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
        Some("exec") => {
            let (code, value) = exec(&hub, rest);
            println!("{}", serde_json::to_string_pretty(&value)?);
            if code != exit::OK {
                std::process::exit(code);
            }
            Ok(())
        }
        _ => bail!("{USAGE}"),
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
pub fn exec<B: SecretBackend>(hub: &ToolHub<B>, args: &[String]) -> (i32, Value) {
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
    match hub.execute(
        &project(args),
        &spec.slug,
        &tool_args,
        flag(args, "--account"),
    ) {
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

    #[test]
    fn exec_exit_codes_are_stable() {
        let hub = mem_hub();
        let (c, v) = exec(&hub, &args(&["github.nope", "--json", "{}"]));
        assert_eq!(
            (c, v["outcome"].as_str()),
            (exit::VALIDATION, Some("validation_error"))
        );
        let (c, _) = exec(&hub, &args(&["github.get_repo", "--json", "[]"]));
        assert_eq!(c, exit::VALIDATION);
        let (c, v) = exec(
            &hub,
            &args(&[
                "github.create_issue",
                "--json",
                r#"{"owner":"a","repo":"b","title":"t"}"#,
            ]),
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
        );
        assert_eq!(c, exit::CREDENTIAL_REQUIRED);
        assert_eq!(v["auth_schemes"], json!(["oauth2", "bearer"]));
        assert!(v["connect_hint"].as_str().unwrap().contains("config-set --toolkit github"));
        let (c, _) = exec(
            &hub,
            &args(&[
                "github.get_repo",
                "--json",
                r#"{"owner":"a/..","repo":"b"}"#,
            ]),
        );
        assert_eq!(
            c,
            exit::VALIDATION,
            "arguments are validated before credentials"
        );
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
