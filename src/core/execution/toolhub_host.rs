//! CTOX host glue for `ctox-toolhub`: encrypted secret backend, management CLI
//! (`ctox toolhub ...`) and the native MCP dispatch adapter.
//!
//! DE: Tokens bleiben im Host. Workjet und Modelle erhalten nur Tool-Schemas,
//! Ergebnisse und Status (nie Zugangsdaten).
//! EN: Tokens stay in the host. Workjet and models receive tool schemas,
//! results and status only (never credentials).
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ctox_core::native_mcp_dispatch::{NativeMcpDispatch, NativeMcpInvocation};
use ctox_protocol::mcp::CallToolResult;
use ctox_toolhub::http::UreqTransport;
use ctox_toolhub::oauth2::RedirectTarget;
use ctox_toolhub::{
    AuthConfig, AuthSchemeKind, ClientCredentialSource, SecretBackend, SecretEntry, ToolHub,
    ToolOutcome,
};
use serde_json::{json, Value};

use crate::secrets;

/// MCP server name under which toolkit tools are exposed to sessions.
// Wired into session registration in a follow-up (tool advertisement, see PR).
#[allow(dead_code)]
pub const TOOLHUB_MCP_SERVER_NAME: &str = "ctox_toolhub";
/// Runtime setting: public instance name (`<name>.ctox.dev`) for OAuth callbacks.
pub const INSTANCE_ENV: &str = "CTOX_TOOLHUB_INSTANCE";
/// Runtime setting: loopback port for OAuth callbacks when no instance name is set.
pub const LOOPBACK_PORT_ENV: &str = "CTOX_TOOLHUB_LOOPBACK_PORT";
const DEFAULT_LOOPBACK_PORT: u16 = 8765;

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

pub fn redirect_target() -> Result<RedirectTarget> {
    if let Ok(instance) = std::env::var(INSTANCE_ENV) {
        if !instance.trim().is_empty() {
            return RedirectTarget::for_instance_name(instance.trim());
        }
    }
    let port = match std::env::var(LOOPBACK_PORT_ENV) {
        Ok(p) => p.parse().context("invalid CTOX_TOOLHUB_LOOPBACK_PORT")?,
        Err(_) => DEFAULT_LOOPBACK_PORT,
    };
    Ok(RedirectTarget::Loopback { port })
}

pub fn hub(root: &Path) -> Result<ToolHub<CtoxSecretBackend>> {
    Ok(ToolHub::new(
        CtoxSecretBackend::new(root),
        Arc::new(UreqTransport::default()),
        redirect_target()?,
    ))
}

const USAGE: &str = "usage: ctox toolhub <catalog|mcp-tools|configs|config-set|connect|complete|status|list|disconnect|refresh|refresh-due|exec> [options]
  config-set --toolkit <slug> --id <id> --scheme <oauth2|bearer|api_key|basic> [--client-id <id>] [--client-secret-stdin] [--scope <s>]...
  connect --auth-config <id> --owner <actor> [--credential-stdin] [--wait-loopback <secs>]
  complete --callback <url-or-query>
  status|disconnect|refresh --account <id>
  exec --owner <actor> --tool <SLUG> --args <json> [--account <id>]";

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn required<'a>(args: &'a [String], name: &str) -> Result<&'a str> {
    flag(args, name).with_context(|| format!("missing {name}\n{USAGE}"))
}

fn read_stdin_secret() -> Result<String> {
    let mut s = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
    Ok(s.trim_end_matches(['\r', '\n']).to_string())
}

fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
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

pub fn handle_toolhub_command(root: &Path, args: &[String]) -> Result<()> {
    let hub = hub(root)?;
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("catalog") => print(&hub.catalog()),
        Some("mcp-tools") => print(&hub.mcp_tools()),
        Some("configs") => print(&hub.store().auth_configs()?),
        Some("config-set") => {
            let scheme = parse_scheme(required(rest, "--scheme")?)?;
            let client_id = flag(rest, "--client-id").map(String::from);
            let scopes = rest
                .windows(2)
                .filter(|w| w[0] == "--scope")
                .map(|w| w[1].clone())
                .collect();
            let secret = if rest.iter().any(|a| a == "--client-secret-stdin") {
                Some(read_stdin_secret()?)
            } else {
                None
            };
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
            let owner = required(rest, "--owner")?;
            if rest.iter().any(|a| a == "--credential-stdin") {
                return print(&hub.connect_with_credential(
                    config,
                    owner,
                    &read_stdin_secret()?,
                )?);
            }
            let request = hub.connect_oauth(config, owner)?;
            match flag(rest, "--wait-loopback") {
                None => print(&json!({
                    "account_id": request.account_id,
                    "toolkit": request.toolkit,
                    "redirect_url": request.redirect_url,
                    "status": request.status,
                })),
                Some(secs) => {
                    let RedirectTarget::Loopback { port } = redirect_target()? else {
                        bail!("--wait-loopback requires a loopback redirect target");
                    };
                    eprintln!(
                        "Bitte im Browser öffnen / Please open in your browser:\n{}",
                        request.redirect_url
                    );
                    let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
                    let callback = ctox_toolhub::oauth2::wait_for_loopback_callback(
                        listener,
                        std::time::Duration::from_secs(secs.parse()?),
                    )?;
                    let query = match callback {
                        ctox_toolhub::oauth2::CallbackResult::Code { code, state } => {
                            format!("code={}&state={}", urlencode(&code), urlencode(&state))
                        }
                        ctox_toolhub::oauth2::CallbackResult::Denied { error, state } => format!(
                            "error={}&state={}",
                            urlencode(&error),
                            urlencode(&state.unwrap_or_default())
                        ),
                    };
                    print(&hub.complete_oauth(&query)?)
                }
            }
        }
        Some("complete") => print(&hub.complete_oauth(required(rest, "--callback")?)?),
        Some("status") => print(&hub.status(required(rest, "--account")?)?),
        Some("list") => print(&hub.store().list()?),
        Some("disconnect") => {
            print(&json!({"removed": hub.disconnect(required(rest, "--account")?)?}))
        }
        Some("refresh") => print(&hub.refresh(required(rest, "--account")?)?),
        Some("refresh-due") => print(&hub.refresh_due()?),
        Some("exec") => {
            let tool_args: Value = serde_json::from_str(required(rest, "--args")?)?;
            print(&hub.execute(
                required(rest, "--owner")?,
                required(rest, "--tool")?,
                &tool_args,
                flag(rest, "--account"),
            )?)
        }
        _ => bail!("{USAGE}"),
    }
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Converts a hub outcome into an MCP tool result. Credential problems are
/// returned as structured, non-secret `credential_required` content so that
/// Workjet can render a connect element (phase 3).
#[allow(dead_code)]
pub fn outcome_to_call_result(outcome: &ToolOutcome) -> CallToolResult {
    let structured = serde_json::to_value(outcome).unwrap_or(Value::Null);
    let is_error = !matches!(outcome, ToolOutcome::Success { .. });
    CallToolResult {
        content: vec![json!({"type": "text", "text": structured.to_string()})],
        structured_content: Some(structured),
        is_error: Some(is_error),
        meta: None,
    }
}

/// Native MCP dispatch for `ctox_toolhub` calls of one verified actor.
/// The actor comes from verified command provenance, never from arguments.
#[allow(dead_code)]
pub struct ToolhubDispatch<B: SecretBackend> {
    hub: ToolHub<B>,
    actor: String,
}

#[allow(dead_code)]
impl<B: SecretBackend> ToolhubDispatch<B> {
    pub fn new(hub: ToolHub<B>, actor: String) -> Self {
        Self { hub, actor }
    }
}

impl<B: SecretBackend + 'static> NativeMcpDispatch for ToolhubDispatch<B> {
    fn dispatch(
        &self,
        invocation: NativeMcpInvocation<'_>,
    ) -> Option<std::result::Result<CallToolResult, String>> {
        if invocation.server() != TOOLHUB_MCP_SERVER_NAME {
            return None;
        }
        let args = invocation.arguments().cloned().unwrap_or_else(|| json!({}));
        if !args.is_object() {
            return Some(Err("toolhub arguments must be an object".into()));
        }
        Some(
            self.hub
                .execute(&self.actor, invocation.tool(), &args, None)
                .map(|o| outcome_to_call_result(&o))
                .map_err(|e| e.to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_required_maps_to_structured_error_without_secrets() {
        let outcome = ToolOutcome::CredentialRequired {
            toolkit: "github".into(),
            account_id: None,
            reason: ctox_toolhub::LocalizedText::new("de", "en"),
        };
        let r = outcome_to_call_result(&outcome);
        assert_eq!(r.is_error, Some(true));
        assert_eq!(
            r.structured_content.unwrap()["outcome"],
            "credential_required"
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
