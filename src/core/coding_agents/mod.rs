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

/// main has already resolved the global root, but retains its pair in argv.
pub(crate) fn coding_models_cli_args_are_valid(args: &[String]) -> bool {
    matches!(args, [command] if command == "models")
        || matches!(args, [command, flag, value] if command == "models"
            && flag == "--root" && !value.is_empty() && !value.starts_with('-'))
}

/// The native root is already selected by main. This read-only command takes
/// one optional probe and one validated global root pair, in either order.
pub(crate) fn coding_route_cli_options(args: &[String]) -> Option<bool> {
    if args.first().map(String::as_str) != Some("route") {
        return None;
    }
    let mut probe = false;
    let mut rooted = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--probe" if !probe => {
                probe = true;
                index += 1;
            }
            "--root" if !rooted => {
                let root = args.get(index + 1)?;
                if root.is_empty() || root.starts_with('-') {
                    return None;
                }
                rooted = true;
                index += 2;
            }
            _ => return None,
        }
    }
    Some(probe)
}

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
        Some("models") => {
            anyhow::ensure!(
                coding_models_cli_args_are_valid(args),
                "usage: ctox coding-agent models [--root <root>]"
            );
            pi_sidecar::coding_model_capabilities_for_cli(root)
        }
        Some("route") => {
            let probe = coding_route_cli_options(args)
                .context("usage: ctox coding-agent route [--probe] [--root <root>]")?;
            if probe {
                pi_sidecar::inherited_coding_route_models_probe(root)
            } else {
                pi_sidecar::inherited_coding_route_status(root)
            }
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
    let mut has_global_root = false;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--root" => {
                anyhow::ensure!(!has_global_root, "--root may only be supplied once");
                let value = args.get(idx + 1).context("--root value is required")?;
                anyhow::ensure!(
                    !value.is_empty() && !value.starts_with('-'),
                    "--root value is required"
                );
                // Root selection belongs to main; never select a different root here.
                has_global_root = true;
                idx += 2;
            }
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
        model = pi_sidecar::resolve_coding_model_preset_for_cli(root, &preset_id)?;
    }
    let dist = pi_sidecar::resolve_sidecar_dist(root)?;
    pi_sidecar::run_module_coding_turn(root, &dist, &module, &prompt, faux, model)
}

fn help_outcome() -> Value {
    json!({
        "ok": true,
        "operation": "help",
        "stdout": "ctox coding-agent turn --module <id> --prompt <text> [--faux] [--preset <id> | --model <json>]\nctox coding-agent smoke --preset <id> [--prompt <text>]\nctox coding-agent models  (daemon-published opaque presets and readiness)\nctox coding-agent route [--probe] [--root <root>]  (public route; optional authenticated live model list)\n",
        "stderr": "",
        "exit_code": 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_route_options_are_exact_and_keep_main_root_authority() {
        for (suffix, expected) in [
            (vec![], Some(false)),
            (vec!["--probe"], Some(true)),
            (vec!["--root", "/native-root"], Some(false)),
            (vec!["--probe", "--root", "/native-root"], Some(true)),
            (vec!["--root", "/native-root", "--probe"], Some(true)),
            (vec!["--root"], None),
            (vec!["--root", ""], None),
            (vec!["--root", "--probe"], None),
            (vec!["--probe", "--probe"], None),
            (vec!["--root", "/one", "--root", "/two"], None),
            (vec!["--endpoint", "https://example.com"], None),
            (vec!["--model", "MiniMax-M3"], None),
            (vec!["--token", "fixture-secret"], None),
        ] {
            let args = std::iter::once("route")
                .chain(suffix)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert_eq!(coding_route_cli_options(&args), expected);
        }
        assert_eq!(coding_route_cli_options(&["turn".to_owned()]), None);
    }

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

    #[test]
    fn coding_models_global_root_pair_rejects_malformed_options() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().display().to_string();
        for suffix in [
            vec!["--root"],
            vec!["--root", ""],
            vec!["--root", "--json"],
            vec!["--root", &path, "--root", &path],
            vec!["--root", &path, "--unknown"],
            vec!["--unknown"],
        ] {
            let args = std::iter::once("models")
                .chain(suffix)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(!coding_models_cli_args_are_valid(&args));
            assert!(execute_cli(root.path(), &args)
                .unwrap_err()
                .to_string()
                .contains("usage:"));
            assert!(!root.path().join("runtime").exists());
        }
    }

    #[test]
    fn operator_turn_accepts_one_global_root_before_preset_validation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().display().to_string();
        let base = [
            "turn",
            "--module",
            "widget",
            "--prompt",
            "test",
            "--preset",
            "browser-forged",
        ];
        let mut args = base.into_iter().map(str::to_owned).collect::<Vec<_>>();
        args.extend(["--root".to_owned(), path.clone()]);
        assert!(execute_cli(root.path(), &args)
            .unwrap_err()
            .to_string()
            .contains("preset is unavailable"));
        for suffix in [
            vec!["--root"],
            vec!["--root", ""],
            vec!["--root", "--unknown"],
            vec!["--root", &path, "--root", &path],
        ] {
            let args = base
                .into_iter()
                .chain(suffix)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(execute_cli(root.path(), &args)
                .unwrap_err()
                .to_string()
                .contains("--root"));
        }
        assert!(!root.path().join("coding-agents").exists());
    }
}
