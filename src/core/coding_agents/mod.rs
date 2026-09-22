//! Native owner of CTOX's built-in coding agent.
//!
//! Coding work on a Business OS app runs through the embedded pi sidecar
//! ([`pi_sidecar`]): the module's app source is projected into a bounded turn,
//! and the resulting snapshot is recorded as versioned source commits through
//! `business_os::store`.
//!
//! The former vendor-CLI wrapper surface (installing and driving external
//! `codex` / `claude` / `agy` binaries, with its own auth flows, workspace
//! grants, session store and SQLite projection) was removed in
//! SYNC-A-LEGACY-CODING-AGENT-REMOVAL: nothing dispatched to it any more, and
//! provider authentication now belongs to the CLIProxyAPI gateway
//! (`crate::execution::cliproxyapi_host`), not to per-vendor installers here.
use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::path::Path;

/// P2: native owner of the pi-code coding sidecar (LocalTransport client). It
/// drives the embedded, bounded pi engine over a Unix socket — one fresh daemon
/// per turn, killed on drop.
pub(crate) mod pi_sidecar;

pub(crate) fn handle_cli(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let outcome = execute_cli(root, args)?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    if outcome.get("ok").and_then(Value::as_bool) == Some(false) {
        let message = outcome
            .get("stderr")
            .and_then(Value::as_str)
            .or_else(|| outcome.get("error").and_then(Value::as_str))
            .unwrap_or("coding agent command failed");
        bail!("{message}");
    }
    Ok(())
}

fn execute_cli(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(help_outcome()),
        Some("turn") => run_coding_turn_cli(root, &args[1..]),
        Some("smoke") => run_coding_smoke_cli(root, &args[1..]),
        Some("route") => {
            anyhow::ensure!(args.len() == 1, "usage: ctox coding-agent route");
            pi_sidecar::inherited_coding_route_status(root)
        }
        Some(other) => bail!(
            "unknown coding-agent subcommand '{other}' (usage: ctox coding-agent turn \
--module <id> --prompt <text> [--faux] [--preset <id> | --model <json>])"
        ),
    }
}

fn run_coding_smoke_cli(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    let mut preset_id = String::new();
    let mut prompt: Option<String> = None;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--preset" => {
                preset_id = args
                    .get(idx + 1)
                    .context("--preset value is required")?
                    .clone();
                idx += 2;
            }
            "--prompt" | "-p" => {
                prompt = Some(
                    args.get(idx + 1)
                        .context("--prompt value is required")?
                        .clone(),
                );
                idx += 2;
            }
            other => bail!(
                "unexpected argument '{other}' (usage: ctox coding-agent smoke --preset <id> [--prompt <text>])"
            ),
        }
    }
    anyhow::ensure!(!preset_id.trim().is_empty(), "--preset is required");
    let dist = pi_sidecar::resolve_sidecar_dist(root)?;
    pi_sidecar::run_coding_preset_smoke(root, &dist, &preset_id, prompt.as_deref())
}

/// One bounded coding turn on a Business OS module — the CLI twin of the
/// `ctox.coding.turn` business command, but with local operator authority.
fn run_coding_turn_cli(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    let mut module = String::new();
    let mut prompt = String::new();
    let mut faux = false;
    let mut model: Option<Value> = None;
    let mut preset_id: Option<String> = None;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--module" | "-m" => {
                module = args
                    .get(idx + 1)
                    .context("--module value is required")?
                    .clone();
                idx += 2;
            }
            "--prompt" | "-p" => {
                prompt = args
                    .get(idx + 1)
                    .context("--prompt value is required")?
                    .clone();
                idx += 2;
            }
            "--faux" => {
                faux = true;
                idx += 1;
            }
            "--model" => {
                let raw = args.get(idx + 1).context("--model value is required")?;
                model = Some(serde_json::from_str(raw).context("--model must be JSON")?);
                idx += 2;
            }
            "--preset" => {
                preset_id = Some(
                    args.get(idx + 1)
                        .context("--preset value is required")?
                        .clone(),
                );
                idx += 2;
            }
            other => bail!(
                "unexpected argument '{other}' (usage: ctox coding-agent turn \
--module <id> --prompt <text> [--faux] [--preset <id> | --model <json>])"
            ),
        }
    }
    anyhow::ensure!(!module.is_empty(), "--module is required");
    anyhow::ensure!(!prompt.is_empty(), "--prompt is required");
    anyhow::ensure!(
        preset_id.is_none() || model.is_none(),
        "--preset and --model are mutually exclusive"
    );
    if let Some(preset_id) = preset_id {
        // Resolve at execution time from the native capability topology. The
        // operator passes the same opaque identifier as Business OS; URLs,
        // headers, account handles and credentials remain server-authored.
        model = pi_sidecar::resolve_coding_model_preset(root, &preset_id)?;
    }
    let dist = pi_sidecar::resolve_sidecar_dist(root)?;
    pi_sidecar::run_module_coding_turn(root, &dist, &module, &prompt, faux, model)
}

fn help_outcome() -> Value {
    json!({
        "ok": true,
        "operation": "help",
        "stdout": "ctox coding-agent turn --module <id> --prompt <text> [--faux] [--preset <id> | --model <json>]\nctox coding-agent smoke --preset <id> [--prompt <text>]\nctox coding-agent route  (nonsecret inherited provider, origin and wire API)\n",
        "stderr": "",
        "exit_code": 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_turn_rejects_ambiguous_preset_and_raw_model() {
        let root = tempfile::tempdir().unwrap();
        let args = [
            "turn",
            "--module",
            "widget",
            "--prompt",
            "test",
            "--preset",
            "ctox",
            "--model",
            r#"{"id":"raw"}"#,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let error = execute_cli(root.path(), &args).unwrap_err().to_string();
        assert!(error.contains("mutually exclusive"));
    }

    #[test]
    fn operator_turn_re_resolves_unknown_preset_before_starting_sidecar() {
        let root = tempfile::tempdir().unwrap();
        let args = [
            "turn",
            "--module",
            "widget",
            "--prompt",
            "test",
            "--preset",
            "browser-forged",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let error = execute_cli(root.path(), &args).unwrap_err().to_string();
        assert!(error.contains("preset is unavailable"));
        assert!(!root.path().join("coding-agents").exists());
    }
}
