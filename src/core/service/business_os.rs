// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use base64::Engine;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use url::Url;
use uuid::Uuid;

use crate::mission::channels;
use crate::persistence;
use crate::skill_store;
// Von ausserhalb wird dieser Pfad weiter erwartet; der Umzug darf die
// oeffentliche Flaeche des Moduls nicht veraendern.
pub(crate) use super::business_os_app_testing::resolve_business_os_validator_node;
use super::business_os_app_testing::{
    app_browser_evidence_arg, app_validator_args_from_finalize_args,
    business_os_app_reference_candidates, collect_business_os_app_bench_status,
    handle_business_os_app_bench, run_business_os_app_audit, run_business_os_app_bench,
    run_business_os_app_e2e, run_business_os_app_smoke, run_business_os_app_validator,
};

pub(super) const BUSINESS_OS_APP_CANDIDATES: &[&str] = &["src/apps/business-os", "business-os"];
const ACTIVATION_PAYLOAD_KEY: &str = "business_os.skill_activation.v1";
const MCP_POLICY_KEYS: &[&str] = &[
    "CTOX_BUSINESS_OS_MCP_ENABLED",
    "CTOX_BUSINESS_OS_MCP_ALLOW_READS",
    "CTOX_BUSINESS_OS_MCP_ALLOW_WRITES",
    "CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS",
    "CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS",
    "CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE",
    "CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS",
    "CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS",
    "CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES",
    "CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES",
    "CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS",
    "CTOX_BUSINESS_OS_MCP_DENY_TOOLS",
];

const CORE_MODULES: &[(&str, &str)] = &[
    ("sales", "Sales"),
    ("marketing", "Marketing"),
    ("operations", "Operations"),
    ("business", "Business"),
    ("ctox", "CTOX"),
];

#[derive(Debug, Clone, Copy, Serialize)]
struct SkillAppBinding {
    skill_id: &'static str,
    pack: &'static str,
    title: &'static str,
    module_id: &'static str,
    submodule_id: &'static str,
}

const SKILL_APP_BINDINGS: &[SkillAppBinding] = &[
    binding(
        "business-os-import-parser",
        "business",
        "Business OS Import Parser",
        "business",
        "automation",
    ),
    binding(
        "business-os-requirement-matching",
        "business",
        "Business OS Matching",
        "business",
        "automation",
    ),
    binding(
        "ctox-cv-print-parser",
        "business",
        "CTOX CV Print Parser",
        "business",
        "automation",
    ),
    binding("doc", "content", "Documents", "documents", "library"),
    binding("pdf", "content", "PDF", "documents", "library"),
    binding(
        "spreadsheet",
        "content",
        "Spreadsheets",
        "documents",
        "spreadsheets",
    ),
    binding("slides", "content", "Slides", "documents", "slides"),
    binding(
        "technical-drawing-review",
        "content",
        "Technical Drawing Review",
        "documents",
        "drawings",
    ),
    binding(
        "transcribe",
        "content",
        "Transcribe",
        "documents",
        "transcripts",
    ),
    binding("screenshot", "content", "Screenshot", "content", "assets"),
    binding(
        "imagegen",
        "content",
        "Image Generation",
        "content",
        "images",
    ),
    binding("sora", "content", "Sora", "content", "video"),
    binding("speech", "content", "Speech", "content", "voice"),
    binding("figma", "design", "Figma", "content", "design"),
    binding(
        "figma-implement-design",
        "design",
        "Figma Implementation",
        "content",
        "design",
    ),
    binding(
        "frontend-skill",
        "development",
        "Frontend Skill",
        "content",
        "web",
    ),
    binding(
        "aspnet-core",
        "development",
        "ASP.NET Core",
        "developer",
        "frameworks",
    ),
    binding(
        "chatgpt-apps",
        "development",
        "ChatGPT Apps",
        "developer",
        "apps",
    ),
    binding(
        "develop-web-game",
        "development",
        "Web Game Development",
        "developer",
        "apps",
    ),
    binding(
        "jupyter-notebook",
        "development",
        "Jupyter Notebook",
        "developer",
        "notebooks",
    ),
    binding(
        "nextjs-postgres-port",
        "development",
        "Next.js Postgres Port",
        "developer",
        "frameworks",
    ),
    binding("winui-app", "development", "WinUI App", "developer", "apps"),
    binding(
        "gh-address-comments",
        "git",
        "Address PR Comments",
        "developer",
        "source-control",
    ),
    binding("gh-fix-ci", "git", "Fix CI", "developer", "quality"),
    binding("yeet", "git", "Publish PR", "developer", "source-control"),
    binding(
        "playwright",
        "testing",
        "Playwright",
        "developer",
        "quality",
    ),
    binding(
        "playwright-interactive",
        "testing",
        "Playwright Interactive",
        "developer",
        "quality",
    ),
    binding(
        "cloudflare-deploy",
        "deploy",
        "Cloudflare Deploy",
        "deployment",
        "cloudflare",
    ),
    binding(
        "netlify-deploy",
        "deploy",
        "Netlify Deploy",
        "deployment",
        "netlify",
    ),
    binding(
        "render-deploy",
        "deploy",
        "Render Deploy",
        "deployment",
        "render",
    ),
    binding(
        "vercel-deploy",
        "deploy",
        "Vercel Deploy",
        "deployment",
        "vercel",
    ),
    binding(
        "security-best-practices",
        "security",
        "Security Best Practices",
        "security",
        "best-practices",
    ),
    binding(
        "security-ownership-map",
        "security",
        "Security Ownership Map",
        "security",
        "ownership",
    ),
    binding(
        "security-threat-model",
        "security",
        "Security Threat Model",
        "security",
        "threat-models",
    ),
    binding("linear", "integration", "Linear", "integrations", "linear"),
    binding(
        "notion-knowledge-capture",
        "integration",
        "Notion Knowledge Capture",
        "integrations",
        "notion",
    ),
    binding(
        "notion-meeting-intelligence",
        "integration",
        "Notion Meeting Intelligence",
        "integrations",
        "notion",
    ),
    binding(
        "notion-spec-to-implementation",
        "integration",
        "Notion Spec to Implementation",
        "integrations",
        "notion",
    ),
    binding(
        "openai-docs",
        "reference",
        "OpenAI Docs",
        "research",
        "openai-docs",
    ),
    binding(
        "notion-research-documentation",
        "integration",
        "Notion Research Documentation",
        "research",
        "notion-research",
    ),
    binding("sentry", "integration", "Sentry", "support", "monitoring"),
    binding("zammad-rest", "vendor", "Zammad REST", "support", "zammad"),
    binding(
        "zammad-printengine-monitoring-sim",
        "vendor",
        "Zammad Print Engine Monitoring",
        "support",
        "monitoring",
    ),
];

const fn binding(
    skill_id: &'static str,
    pack: &'static str,
    title: &'static str,
    module_id: &'static str,
    submodule_id: &'static str,
) -> SkillAppBinding {
    SkillAppBinding {
        skill_id,
        pack,
        title,
        module_id,
        submodule_id,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BusinessOsActivation {
    schema_version: u8,
    enabled_modules: Vec<String>,
    enabled_skills: Vec<String>,
}

pub fn handle_business_os_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        None | Some("status") => {
            println!("{}", business_os_status_text(root));
            Ok(())
        }
        Some("serve") => serve_native_business_os(root, &args[1..]),
        Some("peer") => handle_business_os_peer(root, &args[1..]),
        Some("rxdb") => handle_business_os_rxdb(root, &args[1..]),
        Some("turn") => handle_business_os_turn(root, &args[1..]),
        Some("app") => handle_business_os_app(root, &args[1..]),
        Some("repair") => handle_business_os_repair(root, &args[1..]),
        Some("backup") => handle_business_os_backup(root, &args[1..]),
        Some("commands") => handle_business_os_commands(root, &args[1..]),
        Some("harness-bench") => {
            let result = super::business_os_harness_bench::handle(root, &args[1..])?;
            print_json(&result)?;
            anyhow::ensure!(
                result.get("ok").and_then(serde_json::Value::as_bool) != Some(false),
                "Business OS harness bench reported failures"
            );
            Ok(())
        }
        Some("auth") => handle_business_os_auth(root, &args[1..]),
        Some("desktop") => handle_business_os_desktop(root, &args[1..]),
        Some("mobile-invite") => handle_business_os_mobile_invite(root, &args[1..]),
        Some("shell-update") => handle_business_os_shell_update(root, &args[1..]),
        Some("customer-apps") => match args.get(1).map(String::as_str) {
            None | Some("audit") => print_json(&crate::business_os::audit_customer_apps(root)?),
            Some(other) => anyhow::bail!("unknown business-os customer-apps command `{other}`"),
        },
        Some("web-stack") => handle_business_os_web_stack(root, &args[1..]),
        Some("files") => handle_business_os_files(root, &args[1..]),
        Some("mcp") => handle_business_os_mcp(root, &args[1..]),
        Some("modules") => handle_business_os_modules(root, &args[1..]),
        Some("skills") => handle_business_os_skills(root, &args[1..]),
        Some("help") | Some("--help") | Some("-h") => {
            print_business_os_help();
            Ok(())
        }
        Some(other) => anyhow::bail!(
            "unknown business-os command `{other}`\n\n{}",
            business_os_usage()
        ),
    }
}

fn handle_business_os_shell_update(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let action = args.first().map(String::as_str).unwrap_or("status");
    let version = match args.get(1..).unwrap_or_default() {
        [] => None,
        [flag, version] if action == "stage" && flag == "--version" => Some(version.as_str()),
        _ => anyhow::bail!("shell-update accepts only stage --version <signed-release-version> as additional arguments"),
    };
    let result = match action {
        "status" => crate::business_os::shell_update::status(root, true),
        "check" => crate::business_os::shell_update::check(root),
        "stage" => match version {
            Some(version) => crate::business_os::shell_update::stage_version(root, version),
            None => crate::business_os::shell_update::stage(root),
        },
        "activate" => crate::business_os::shell_update::activate(root),
        "rollback" => crate::business_os::shell_update::rollback(root),
        "--help" | "-h" => {
            println!("usage: ctox business-os shell-update [status|check|stage [--version <signed-release-version>]|activate|rollback]");
            return Ok(());
        }
        other => anyhow::bail!("unknown business-os shell-update command `{other}`"),
    };
    match result {
        Ok(value) => {
            if action != "status" {
                crate::business_os::shell_update::record_audit(
                    root,
                    action,
                    "succeeded",
                    "local-cli",
                )?;
            }
            print_json(&value)
        }
        Err(error) => {
            if action != "status" {
                crate::business_os::shell_update::record_audit(
                    root,
                    action,
                    "failed",
                    "local-cli",
                )?;
            }
            Err(error)
        }
    }
}

fn handle_business_os_mobile_invite(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("create") => {
            validate_mobile_invite_flags(
                &args[1..],
                &[
                    "--ttl-seconds",
                    "--display-name",
                    "--device-pairing-id",
                    "--device-id",
                    "--proof-key-thumbprint",
                ],
            )?;
            let ttl_seconds = flag_value(args, "--ttl-seconds")
                .map(str::parse::<i64>)
                .transpose()
                .context("mobile invite --ttl-seconds must be an integer")?
                .unwrap_or(crate::business_os::mobile_invites::DEFAULT_TTL_SECONDS);
            let binding = crate::business_os::mobile_invites::device_binding(
                flag_value(args, "--device-pairing-id"),
                flag_value(args, "--device-id"),
                flag_value(args, "--proof-key-thumbprint"),
            )?;
            print_json(&crate::business_os::mobile_invites::create(
                root,
                ttl_seconds,
                flag_value(args, "--display-name"),
                binding.as_ref(),
            )?)
        }
        Some("revoke") => {
            validate_mobile_invite_flags(&args[1..], &["--invite-id", "--device-pairing-id"])?;
            match (
                flag_value(args, "--invite-id"),
                flag_value(args, "--device-pairing-id"),
            ) {
                (Some(invite_id), None) => print_json(
                    &crate::business_os::mobile_invites::revoke(root, invite_id)?,
                ),
                (None, Some(device_pairing_id)) => print_json(
                    &crate::business_os::mobile_invites::revoke_by_device_pairing_id(
                        root,
                        device_pairing_id,
                    )?,
                ),
                _ => anyhow::bail!(
                    "mobile invite revoke requires exactly one of --invite-id or --device-pairing-id"
                ),
            }
        }
        Some("--help") | Some("-h") | None => {
            println!(
                "usage:\n  ctox business-os mobile-invite create [--ttl-seconds 300] [--display-name <name>] [--device-pairing-id <id> --device-id <id> --proof-key-thumbprint <base64url-sha256>]\n  ctox business-os mobile-invite revoke (--invite-id <opaque-id> | --device-pairing-id <id>)"
            );
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os mobile-invite command `{other}`"),
    }
}

fn validate_mobile_invite_flags(args: &[String], allowed: &[&str]) -> anyhow::Result<()> {
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        anyhow::ensure!(
            allowed.contains(&flag),
            "unknown business-os mobile-invite flag `{flag}`"
        );
        let value = args
            .get(index + 1)
            .map(String::as_str)
            .filter(|value| !value.starts_with("--"))
            .with_context(|| format!("mobile invite {flag} requires a value"))?;
        anyhow::ensure!(!value.is_empty(), "mobile invite {flag} requires a value");
        index += 2;
    }
    Ok(())
}

fn handle_business_os_backup(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("restore-drill") => {
            let result = crate::business_os::store::run_business_os_backup_restore_drill(
                root,
                flag_value(args, "--module").or_else(|| flag_value(args, "--module-id")),
            )?;
            print_json(&result)
        }
        Some("prune-drills") => {
            let result = crate::business_os::store::prune_business_os_backup_restore_drills(
                root,
                args.iter().any(|arg| arg == "--dry-run"),
            )?;
            print_json(&result)
        }
        Some("inspect-manifest") => {
            let manifest = flag_value(args, "--manifest")
                .or_else(|| {
                    args.get(1)
                        .map(String::as_str)
                        .filter(|arg| !arg.starts_with("--"))
                })
                .context("usage: ctox business-os backup inspect-manifest --manifest <path>")?;
            let result = crate::business_os::store::inspect_business_os_backup_manifest(
                root,
                std::path::Path::new(manifest),
            )?;
            print_json(&result)
        }
        Some("key-escrow-status") => {
            let result = crate::business_os::store::inspect_business_os_backup_key_escrow(root)?;
            print_json(&result)
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os backup command `{other}`"),
    }
}

fn handle_business_os_desktop(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("invite") if desktop_invite_help_requested(args) => {
            println!(
                "usage: ctox business-os desktop invite [--display-name <name>] \
[--ttl-hours <n> | --expires-at <rfc3339>] [--format json|link] [--output <path>]"
            );
            Ok(())
        }
        Some("invite") => {
            let invite = build_desktop_invite(root, args)?;
            match flag_value(args, "--format").unwrap_or("json") {
                "json" => {
                    if let Some(output) =
                        flag_value(args, "--output").or_else(|| flag_value(args, "-o"))
                    {
                        fs::write(output, serde_json::to_string_pretty(&invite)?).with_context(
                            || format!("failed to write desktop invite to `{output}`"),
                        )?;
                        print_json(&serde_json::json!({
                            "ok": true,
                            "path": output,
                            "type": "ctox-business-os-invite",
                            "version": 1,
                            "secret_value_in_payload": true,
                        }))
                    } else {
                        print_json(&invite)
                    }
                }
                "link" | "deep-link" => {
                    let link = invite
                        .get("desktop_link")
                        .and_then(serde_json::Value::as_str)
                        .context("desktop invite link is missing")?;
                    if let Some(output) =
                        flag_value(args, "--output").or_else(|| flag_value(args, "-o"))
                    {
                        fs::write(output, link).with_context(|| {
                            format!("failed to write desktop invite link to `{output}`")
                        })?;
                        print_json(&serde_json::json!({
                            "ok": true,
                            "path": output,
                            "format": "link",
                            "secret_value_in_payload": true,
                        }))
                    } else {
                        println!("{link}");
                        Ok(())
                    }
                }
                other => anyhow::bail!("unsupported desktop invite format `{other}`"),
            }
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os desktop command `{other}`"),
    }
}

fn desktop_invite_help_requested(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
}

fn build_desktop_invite(root: &Path, args: &[String]) -> anyhow::Result<serde_json::Value> {
    let config = crate::business_os::store::sync_config(root)?;
    let display_name = flag_value(args, "--display-name")
        .or_else(|| flag_value(args, "--name"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(config.instance_id.as_str());
    let requested_expires_at = flag_value(args, "--expires-at")
        .map(str::to_string)
        .unwrap_or_else(|| {
            let ttl_hours = flag_value(args, "--ttl-hours")
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(168);
            (chrono::Utc::now() + chrono::Duration::hours(ttl_hours))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        });
    let requested_expires_at = chrono::DateTime::parse_from_rfc3339(&requested_expires_at)
        .context("desktop invite --expires-at must be RFC3339")?
        .with_timezone(&chrono::Utc);
    let user_id = flag_value(args, "--user")
        .or_else(|| flag_value(args, "--user-id"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("desktop-owner");
    let user_display_name = flag_value(args, "--user-display-name")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Desktop Owner");
    let role = flag_value(args, "--role")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("chef");
    anyhow::ensure!(
        matches!(role, "chef" | "admin" | "founder" | "user"),
        "desktop invite --role must be chef, admin, founder, or user"
    );
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let (capability_token, capability_expires_at_ms) =
        crate::business_os::store::issue_business_os_capability_token_for_managed_user(
            root,
            user_id,
            user_display_name,
            role,
            now_ms,
        )?;
    let capability_expires_at = chrono::DateTime::from_timestamp_millis(capability_expires_at_ms)
        .context("desktop invite capability expiry is invalid")?;
    let expires_at = std::cmp::min(requested_expires_at, capability_expires_at)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut invite = serde_json::json!({
        "type": "ctox-business-os-invite",
        "version": 1,
        "display_name": display_name,
        "instance_id": config.instance_id,
        "sync_room": config.sync_room,
        "native_peer_id": config.native_peer_id,
        "signaling_urls": config.signaling_urls,
        "signaling_auth_version": config.signaling_auth_version,
        "signaling_browser_token": config.signaling_browser_token,
        "signaling_browser_token_hash": config.signaling_browser_token_hash,
        "signaling_native_token_hash": config.signaling_native_token_hash,
        "transport": "webrtc",
        "expires_at": expires_at,
        "data_plane": "rxdb-webrtc",
        "http_bridge_available": false,
        "secret_value_in_payload": true,
        "session": {
            "authenticated": true,
            "source": "desktop_invite",
            "capability_token": capability_token,
            "capability_expires_at_ms": capability_expires_at_ms,
            "user": {
                "id": user_id,
                "display_name": user_display_name,
                "role": role,
                "is_admin": matches!(role, "chef" | "admin" | "founder"),
            }
        }
    });
    let encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&invite)?);
    invite["desktop_link"] =
        serde_json::Value::String(format!("ctox-business-os-desktop://pair?payload={encoded}"));
    Ok(invite)
}

fn handle_business_os_repair(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("queue-projections") => {
            let apply = args.iter().any(|arg| arg == "--apply");
            let dry_run = args.iter().any(|arg| arg == "--dry-run");
            if apply == dry_run {
                anyhow::bail!(
                    "usage: ctox business-os repair queue-projections (--dry-run | --apply)"
                );
            }
            let result = crate::business_os::store::repair_queue_projections(
                root,
                crate::business_os::store::QueueProjectionRepairOptions { apply },
            )?;
            print_json(&result)
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os repair command `{other}`"),
    }
}

pub fn business_os_status_text(root: &Path) -> String {
    let native_app = existing_dir_path(root, BUSINESS_OS_APP_CANDIDATES);
    let installed_modules = root.join("runtime/business-os/installed-modules");

    format!(
        "CTOX Business OS\n\
         Native app:   {native_app_status}  {native_app}\n\
         Runtime apps: {installed_modules_status}  {installed_modules}\n\
         Native store: {native_store}\n\n\
         Serve the native no-build Business OS:\n\
           ctox business-os serve --addr 127.0.0.1:8765\n\n\
         Create or modify runtime-installed Business OS apps:\n\
           ctox business-os app create --instruction <text> [--module-id <id>]\n\
           ctox business-os app modify <module-id> --instruction <text>\n\n\
         Runtime contract:\n\
           - Business OS apps are runtime-installed vanilla HTML/CSS/JS modules.\n\
           - Dynamic apps live under runtime/business-os/installed-modules/<module-id>.\n\
           - CTOX core runs as the outbound CTOX Sync Engine/WebRTC peer.\n\
           - SQLite state, commands, module manifests, and files sync over RxDB.\n\
           - Only system Business OS apps are installed by default.\n\
           - Non-system apps are installed through the app store only.\n",
        native_app_status = exists_label(native_app.join("index.html").is_file()),
        installed_modules_status = exists_label(installed_modules.is_dir()),
        native_app = native_app.display(),
        installed_modules = installed_modules.display(),
        native_store = root.join("runtime/business-os.sqlite3").display(),
    )
}

fn serve_native_business_os(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let addr = flag_value(args, "--addr").unwrap_or("127.0.0.1:8765");
    crate::business_os::serve_business_os(
        root,
        crate::business_os::BusinessOsServeOptions {
            addr: addr.to_string(),
        },
    )
}

fn handle_business_os_peer(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        None | Some("status") => print_json(&serde_json::to_value(
            crate::business_os::store::sync_config(root)?,
        )?),
        Some("rotate") => print_json(&serde_json::to_value(
            crate::business_os::store::rotate_sync_credentials(root)?,
        )?),
        Some("rotate-room") => print_json(&serde_json::to_value(
            crate::business_os::store::rotate_sync_room_password(root)?,
        )?),
        Some("rotate-native") => print_json(&serde_json::to_value(
            crate::business_os::store::rotate_sync_native_signaling_token(root)?,
        )?),
        Some("start") => crate::business_os::run_native_peer_foreground(root),
        Some("ensure") => {
            crate::business_os::ensure_native_peer(root)?;
            print_json(&serde_json::json!({
                "ok": true,
                "running": crate::business_os::store::sync_config(root)?.native_rxdb_peer_available,
            }))
        }
        // Per-device revocation control surface (server-authoritative): a revoked
        // signaling peer id is denied at connect time by the native peer's
        // is_peer_valid gate. Operator/harness surface; the app equivalent would
        // ride the policy-gated command channel.
        Some("revoke") => {
            let peer_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .map(String::as_str)
                .context("usage: ctox business-os peer revoke <peer-id> [--reason <text>]")?;
            let reason = flag_value(args, "--reason").unwrap_or("");
            crate::business_os::store::revoke_business_peer(root, peer_id, "cli", reason)?;
            print_json(&serde_json::json!({ "ok": true, "revoked": peer_id }))
        }
        Some("unrevoke") => {
            let peer_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .map(String::as_str)
                .context("usage: ctox business-os peer unrevoke <peer-id>")?;
            crate::business_os::store::clear_business_peer_revocation(root, peer_id)?;
            print_json(&serde_json::json!({ "ok": true, "cleared": peer_id }))
        }
        Some("revocations") | Some("list-revocations") => print_json(&serde_json::json!({
            "ok": true,
            "revocations": crate::business_os::store::list_revoked_business_peers(root)?,
        })),
        // Audit surface for the per-collection sync read-authz matrix (#12c).
        // `--role <r>` shows which collections that role is denied; `--token <t>`
        // resolves a capability token to its role first. `enforced` reflects the
        // default-off runtime flag; enforcement at the WebRTC handshake is the
        // tracked integration step (needs a live two-peer mesh test).
        Some("collection-access") => {
            let role_str = flag_value(args, "--token")
                .and_then(|token| crate::business_os::store::verify_capability_role(root, token))
                .or_else(|| flag_value(args, "--role").map(str::to_owned))
                .unwrap_or_else(|| "user".to_owned());
            let role = crate::business_os::policy::parse_role(&role_str);
            let denied: Vec<&str> = crate::business_os::policy::ADMIN_ONLY_COLLECTIONS
                .iter()
                .filter(|collection| {
                    !crate::business_os::policy::role_may_read_collection(role, collection)
                })
                .copied()
                .collect();
            print_json(&serde_json::json!({
                "ok": true,
                "role": role.as_str(),
                "enforced": crate::business_os::store::collection_authz_enabled(root),
                "denied_collections": denied,
                "note": "deny-by-exception: every collection not listed is readable by all roles",
            }))
        }
        Some("--help") | Some("-h") => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os peer command `{other}`"),
    }
}

// Backlog OS-B1: operator surface for the external TURN relay (see
// docs/ctox-turn.md — the relay lives next to the signaling plane, never
// inside the daemon; CTOX only mints ephemeral credentials for it).
fn handle_business_os_turn(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        None | Some("status") => print_json(&crate::business_os::store::turn_config_status(root)?),
        Some("set") => {
            let url = flag_value(args, "--url");
            let secret = flag_value(args, "--secret");
            print_json(&crate::business_os::store::set_turn_config(
                root, url, secret,
            )?)
        }
        Some("--help") | Some("-h") => {
            println!("usage:\n  ctox business-os turn status\n  ctox business-os turn set [--url turns:host:5349] [--secret <coturn use-auth-secret>]");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os turn command `{other}`"),
    }
}

fn handle_business_os_rxdb(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        // Backlog OS-A4: sync/peer diagnosis for operators AND the harness.
        // Reads the native peer's heartbeat status file, so it works from a
        // separate CLI process while the daemon runs (performance metrics —
        // loop idle/active/error ticks, SQLite runtime counters incl. the
        // per-database external-poll wakeups — ride the heartbeat).
        Some("status") => {
            let status = enrich_rxdb_peer_status_with_production_readiness(
                crate::business_os::native_peer_status(root),
            );
            if args.iter().any(|arg| arg == "--json") {
                print_json(&status)
            } else {
                println!("{}", render_rxdb_peer_status_text(&status));
                Ok(())
            }
        }
        Some("repair-optional-drift") => {
            let collection = flag_value(args, "--collection")
                .or_else(|| {
                    args.get(1)
                        .filter(|value| !value.starts_with("--"))
                        .map(String::as_str)
                })
                .context(
                    "usage: ctox business-os rxdb repair-optional-drift --collection <name> [--dry-run] [--force]",
                )?;
            let dry_run = args.iter().any(|arg| arg == "--dry-run");
            let force = args.iter().any(|arg| arg == "--force");
            let result = crate::business_os::repair_optional_rxdb_collection_schema_drift(
                root, collection, dry_run, force,
            )?;
            print_json(&result)
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os rxdb command `{other}`"),
    }
}

fn enrich_rxdb_peer_status_with_production_readiness(
    mut status: serde_json::Value,
) -> serde_json::Value {
    let running = status
        .get("running")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let replication_up = status
        .get("replicationUp")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let heartbeat_fresh = status
        .pointer("/heartbeat/fresh")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let health_error_total = status
        .pointer("/health/errorTotal")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let command_consumer_alive = status
        .pointer("/health_stages/command_consumer_alive")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let turn_credential_ready = status
        .pointer("/health_stages/turn_credential_ready")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let circuit_state = status
        .get("circuitBreaker")
        .and_then(|value| value.get("state"))
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let critical_tasks_alive = status
        .get("criticalTasks")
        .and_then(|value| value.as_array())
        .map(|tasks| {
            !tasks.is_empty()
                && tasks.iter().all(|task| {
                    task.get("alive")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false);
    let pending_sync_count = status
        .pointer("/command_plane/pending_sync_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let oldest_pending_age_ms = status
        .pointer("/command_plane/oldest_pending_age_ms")
        .and_then(|value| value.as_u64());
    let projection_outbox_age_ms = status.pointer("/health_stages/projection_outbox").cloned();

    let missing_evidence = [
        "release_soak_3x31_no_retry",
        "nightly_soak_9x31_no_retry",
        "full_matrix_min_40_no_retry",
        "canary_72h",
        "native_restore_drill",
        "wan_turn_matrix",
        "browser_recovery_matrix",
        "app_runtime_package_gate",
        "security_privacy_signoff",
        "record_workbench_30_day_pilot",
        "workflow_30_day_pilot",
        "runbook_exercises",
        "slo_samples",
        "browser_journal",
        "browser_recovery_export",
    ];
    let mut blockers = Vec::<String>::new();
    if !running {
        blockers.push("native_peer_not_running".to_owned());
    }
    if !replication_up {
        blockers.push("replication_not_up".to_owned());
    }
    if !heartbeat_fresh {
        blockers.push("heartbeat_not_fresh".to_owned());
    }
    if health_error_total > 0 {
        blockers.push("native_peer_health_errors".to_owned());
    }
    if !command_consumer_alive {
        blockers.push("command_consumer_not_alive".to_owned());
    }
    if !critical_tasks_alive {
        blockers.push("critical_task_liveness_unproven".to_owned());
    }
    if !turn_credential_ready {
        blockers.push("credentialed_turn_not_ready".to_owned());
    }
    if circuit_state != "closed" && circuit_state != "unknown" {
        blockers.push(format!("signaling_circuit_{circuit_state}"));
    }
    if pending_sync_count > 0 {
        blockers.push("pending_command_sync".to_owned());
    }
    blockers.extend(
        missing_evidence
            .iter()
            .map(|evidence| format!("missing_evidence:{evidence}")),
    );

    let readiness = serde_json::json!({
        "schema": "ctox.sync.production_readiness_95.status.v1",
        "ready": blockers.is_empty(),
        "ratingTarget": "9.5/10",
        "sloTargets": {
            "localSubmitP95Ms": 100,
            "lanReplicationP95Ms": 2_000,
            "wanReplicationP95Ms": 5_000,
            "reconnectP95Ms": 60_000,
            "convergencePercentWithinSlo": 99.9,
            "nativeBackupRpoMs": 15 * 60 * 1_000,
            "nativeBackupRtoMs": 60 * 60 * 1_000,
        },
        "liveness": {
            "running": running,
            "replicationUp": replication_up,
            "heartbeatFresh": heartbeat_fresh,
            "healthErrorTotal": health_error_total,
            "commandConsumerAlive": command_consumer_alive,
            "criticalTasksAlive": critical_tasks_alive,
        },
        "transport": {
            "circuitBreakerState": circuit_state,
            "turnCredentialReady": turn_credential_ready,
        },
        "commandPlane": {
            "pendingSyncCount": pending_sync_count,
            "oldestPendingAgeMs": oldest_pending_age_ms,
            "projectionOutboxAgeMs": projection_outbox_age_ms.unwrap_or(serde_json::Value::Null),
        },
        "releaseGates": {
            "releaseSoakModes": 31,
            "releaseSoakCycles": 3,
            "nightlySoakCycles": 9,
            "nightlyTimeoutMinutes": 360,
            "fullMatrixMinimumModes": 40,
            "canaryHours": 72,
            "pilotDays": 30,
        },
        "evidenceArtifacts": {
            "templateCatalog": "node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js",
            "artifactBuilder": "node src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js --kind <gate> --input <measurements.json> --output <artifact.json>",
            "fullMatrixRunner": "node src/core/rxdb/tools/run_sync_production_readiness_95_full_matrix.js",
            "operationalGateRunner": "node src/core/rxdb/tools/run_sync_production_readiness_95_operational_gate.js --gate <gate>",
            "releaseSoak": "rxdb-soak-summary.json",
            "nightlySoak": "runtime/build/ctox-sync-production-readiness-95-nightly-soak.json",
            "defaultMatrix": "runtime/build/ctox-sync-production-readiness-95-default-matrix.json",
            "businessOsMatrix": "runtime/build/ctox-sync-production-readiness-95-business-os-matrix.json",
            "canary": "runtime/build/ctox-sync-production-readiness-95-canary.json",
            "nativeRestoreDrill": "runtime/build/ctox-sync-production-readiness-95-restore-drill.json",
            "wanTurnMatrix": "runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json",
            "wanTurnRunner": "node src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js",
            "browserRecoveryMatrix": "runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json",
            "browserRecoveryRunner": "node src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js",
            "appRuntimePackageGate": "runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json",
            "appRuntimePackageRunner": "node src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js",
            "recordWorkbenchPilot": "runtime/build/ctox-sync-production-readiness-95-record-workbench-pilot.json",
            "workflowPilot": "runtime/build/ctox-sync-production-readiness-95-workflow-pilot.json",
            "runbookExercises": "runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json",
            "runbookExercisesRunner": "node src/core/rxdb/tools/run_sync_production_readiness_95_runbook_exercises.js",
            "evidenceAudit": "runtime/build/ctox-sync-production-readiness-95-evidence-audit.json",
            "operatorReport": "runtime/build/ctox-sync-production-readiness-95-operator-report.json"
        },
        "missingEvidence": missing_evidence,
        "blockers": blockers,
    });

    if let Some(object) = status.as_object_mut() {
        object.insert("productionReadiness".to_owned(), readiness);
    }
    status
}

// Compact human summary of `native_peer_status` for `rxdb status`. The full
// JSON (loops, SQLite runtime counters, file-fetch stats) stays behind
// `--json`; this view answers "is the data plane healthy and quiet?".
fn render_rxdb_peer_status_text(status: &serde_json::Value) -> String {
    let running = status
        .get("running")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let replication_up = status
        .get("replicationUp")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let heartbeat_age_ms = status.get("heartbeatAgeMs").and_then(|v| v.as_u64());
    let heartbeat = heartbeat_age_ms
        .map(|age| format!("{:.1}s ago", age as f64 / 1000.0))
        .unwrap_or_else(|| "none".to_string());
    let mut out = format!(
        "CTOX Sync Engine native peer\n  running:        {running}\n  replicationUp:  {replication_up}\n  heartbeat:      {heartbeat}\n"
    );
    if let Some(errors) = status.get("healthErrors").and_then(|v| v.as_array()) {
        for error in errors {
            let message = error
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown health error");
            out.push_str(&format!("  HEALTH:         {message}\n"));
        }
    }
    let loops = status
        .get("performance")
        .and_then(|v| v.get("loops"))
        .and_then(|v| v.as_object());
    if let Some(loops) = loops {
        out.push_str("Projection loops (ticks idle/active/error, last ms):\n");
        let mut names: Vec<&String> = loops.keys().collect();
        names.sort();
        for name in names {
            let snapshot = &loops[name];
            let idle = snapshot
                .get("idle_ticks")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let active = snapshot
                .get("active_ticks")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let errors = snapshot
                .get("error_ticks")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let last_ms = snapshot
                .get("last_duration_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            out.push_str(&format!(
                "  {name:<20} {idle}/{active}/{errors}  last {last_ms}ms\n"
            ));
        }
    }
    let wakeups = status
        .get("performance")
        .and_then(|v| v.get("rxdb_sqlite"))
        .and_then(|v| v.get("external_poll_wakeups_by_database"))
        .and_then(|v| v.as_object());
    if let Some(wakeups) = wakeups {
        out.push_str("External-poll wakeups by database:\n");
        for (database, count) in wakeups {
            out.push_str(&format!("  {database}: {count}\n"));
        }
    }
    if let Some(readiness) = status.get("productionReadiness") {
        let ready = readiness
            .get("ready")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let blocker_count = readiness
            .get("blockers")
            .and_then(|value| value.as_array())
            .map(|values| values.len())
            .unwrap_or(0);
        out.push_str(&format!(
            "Production readiness 9.5: ready={ready} blockers={blocker_count}\n"
        ));
    }
    out.push_str("Full detail: ctox business-os rxdb status --json\n");
    out
}

fn handle_business_os_app(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("create") => handle_business_os_app_create(root, &args[1..]),
        Some("modify") => handle_business_os_app_modify(root, &args[1..]),
        Some("validate") => {
            let module_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .context(
                    "usage: ctox business-os app validate <module-id> [--installed|--source] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]",
                )?;
            let output = run_business_os_app_validator(root, module_id, &args[2..])?;
            if !output.status.success() {
                print_process_output(&output);
                anyhow::bail!("Business OS app validation failed for `{module_id}`");
            }
            print_process_output(&output);
            Ok(())
        }
        Some("smoke") => {
            let module_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .context(
                    "usage: ctox business-os app smoke <module-id> [--url <business-os-url>] [--json] [--create-action <action>] [--timeout-ms <n>] [--output <path>] [--screenshot <path>]",
                )?;
            let output = run_business_os_app_smoke(root, module_id, &args[2..])?;
            if !output.status.success() {
                print_process_output(&output);
                anyhow::bail!("Business OS app browser smoke failed for `{module_id}`");
            }
            print_process_output(&output);
            Ok(())
        }
        Some("e2e") => {
            let module_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .context(
                    "usage: ctox business-os app e2e <module-id> [--url <business-os-url>] [--json] [--timeout-ms <n>] [--output <path>] [--screenshot <path>] [--marker <value>]",
                )?;
            let output = run_business_os_app_e2e(root, module_id, &args[2..])?;
            if !output.status.success() {
                print_process_output(&output);
                anyhow::bail!("Business OS app browser E2E failed for `{module_id}`");
            }
            print_process_output(&output);
            Ok(())
        }
        Some("audit") => {
            let module_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .context(
                    "usage: ctox business-os app audit <module-id> [--installed|--source] [--profile quick|release|full] [--url <business-os-url>] [--deployed-url <url> [--active --approval-id <id>]]",
                )?;
            let result = run_business_os_app_audit(root, module_id, &args[2..])?;
            print_json(&result)?;
            if result.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
                anyhow::bail!(
                    "Business OS app audit is not closable for `{module_id}`; inspect completion_review.blockers"
                );
            }
            Ok(())
        }
        Some("references") => {
            let output = business_os_app_reference_candidates(root, &args[1..])?;
            if args.iter().any(|arg| arg == "--json") {
                print_json(&output)
            } else {
                let modules = output
                    .get("modules")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                println!("Business OS app reference candidates:");
                for module in modules {
                    let id = module
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    let title = module
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(id);
                    let path = module
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    let collections = module
                        .get("collections")
                        .and_then(serde_json::Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    println!("- {id}: {title}");
                    if !collections.is_empty() {
                        println!("  collections: {collections}");
                    }
                    println!("  path: {path}");
                }
                Ok(())
            }
        }
        Some("refresh-catalog") => {
            crate::business_os::store::write_module_catalog_projection_to_rxdb(root)?;
            print_json(&serde_json::json!({
                "ok": true,
                "command": "business-os app refresh-catalog",
                "data_plane": "rxdb-webrtc",
            }))
        }
        Some("finalize") => {
            let module_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .context(
                    "usage: ctox business-os app finalize <module-id> --task-id <queue-task-id> [--installed|--source] [--reason <text>]",
                )?;
            let task_id = flag_value(args, "--task-id").context(
                "usage: ctox business-os app finalize <module-id> --task-id <queue-task-id> [--installed|--source] [--reason <text>]",
            )?;
            let validator_args = app_validator_args_from_finalize_args(args);
            let output = run_business_os_app_validator(root, module_id, &validator_args)?;
            print_process_output(&output);
            if !output.status.success() {
                anyhow::bail!(
                    "Business OS app finalize refused to complete `{module_id}` because validation is red"
                );
            }
            let reason = flag_value(args, "--reason")
                .unwrap_or("Business OS app artifacts validated by ctox business-os app finalize");
            let result =
                crate::business_os::store::complete_business_command_from_app_validation_success(
                    root,
                    task_id,
                    Some(module_id),
                    reason,
                )?
                .with_context(|| {
                    format!("queue task `{task_id}` is not linked to a Business OS app command")
                })?;
            print_json(&serde_json::json!({
                "ok": true,
                "module_id": module_id,
                "task_id": task_id,
                "result": result,
            }))
        }
        Some("bench") => {
            let result = handle_business_os_app_bench(root, &args[1..])?;
            print_json(&result)?;
            if result.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
                anyhow::bail!("Business OS app bench did not submit all tasks");
            }
            Ok(())
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os app command `{other}`"),
    }
}

fn handle_business_os_app_create(root: &Path, args: &[String]) -> anyhow::Result<()> {
    if args_have_help(args) {
        println!(
            "usage: ctox business-os app create --instruction <text> [--module-id <id>] [--title <title>] [--description <text>] [--category <category>] [--version 0.1.0] [--actor <user-id>]"
        );
        return Ok(());
    }
    let instruction = app_instruction_arg(args, false)
        .context("usage: ctox business-os app create --instruction <text> [--module-id <id>]")?;
    let module_id = flag_value(args, "--module-id")
        .or_else(|| flag_value(args, "--app-id"))
        .map(sanitize_business_os_app_module_id)
        .transpose()?
        .unwrap_or_else(|| {
            sanitize_business_os_app_module_id(
                flag_value(args, "--title").unwrap_or(instruction.as_str()),
            )
            .unwrap_or_else(|_| format!("business-app-{}", now_ms()))
        });
    let title = flag_value(args, "--title")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| title_from_module_id(&module_id));
    let description = flag_value(args, "--description")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| instruction.chars().take(220).collect::<String>());
    let category = flag_value(args, "--category")
        .unwrap_or("")
        .trim()
        .to_owned();
    let version =
        normalize_business_os_app_version(flag_value(args, "--version").unwrap_or("0.1.0"))?;
    let actor = business_os_app_cli_actor(args);
    let now = now_ms();
    let document = serde_json::json!({
        "id": flag_value(args, "--command-id")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("cmd_app_create_{}_{}", module_id, now)),
        "command_id": flag_value(args, "--command-id")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("cmd_app_create_{}_{}", module_id, now)),
        "module": "creator",
        "type": "ctox.business_os.app.create",
        "command_type": "ctox.business_os.app.create",
        "record_id": module_id.as_str(),
        "status": "pending_sync",
        "payload": {
            "title": format!("Create {title}"),
            "instruction": instruction.as_str(),
            "module_id": module_id.as_str(),
            "app_id": module_id.as_str(),
            "app_title": title.as_str(),
            "description": description.as_str(),
            "category": category.as_str(),
            "desired_version": version.as_str(),
            "install_target": "runtime-installed-module",
            "target": "app",
            "mode": "app",
            "required_skills": [BUSINESS_OS_APP_BENCH_SKILL]
        },
        "client_context": {
            "source": "ctox-cli.business-os-app-create",
            "target": "app",
            "mode": "app",
            "module_id": module_id.as_str(),
            "app_id": module_id.as_str(),
            "install_target": "runtime-installed-module",
            "actor": actor
        },
        "created_at_ms": now,
        "updated_at_ms": now
    });
    submit_business_os_app_command_document(root, document)
}

fn handle_business_os_app_modify(root: &Path, args: &[String]) -> anyhow::Result<()> {
    if args_have_help(args) {
        println!(
            "usage: ctox business-os app modify <module-id> --instruction <text> [--actor <user-id>]"
        );
        return Ok(());
    }
    let raw_module_id = flag_value(args, "--module-id")
        .or_else(|| flag_value(args, "--app-id"))
        .or_else(|| {
            args.first()
                .filter(|value| !value.starts_with("--"))
                .map(String::as_str)
        })
        .context("usage: ctox business-os app modify <module-id> --instruction <text>")?;
    let module_id = sanitize_business_os_app_module_id(raw_module_id)?;
    let instruction = app_instruction_arg(args, true)
        .context("usage: ctox business-os app modify <module-id> --instruction <text>")?;
    let title = flag_value(args, "--title")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Modify {}", title_from_module_id(&module_id)));
    let actor = business_os_app_cli_actor(args);
    let now = now_ms();
    let document = serde_json::json!({
        "id": flag_value(args, "--command-id")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("cmd_app_modify_{}_{}", module_id, now)),
        "command_id": flag_value(args, "--command-id")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("cmd_app_modify_{}_{}", module_id, now)),
        "module": "creator",
        "type": "ctox.business_os.app.modify",
        "command_type": "ctox.business_os.app.modify",
        "record_id": module_id.as_str(),
        "status": "pending_sync",
        "payload": {
            "title": title.as_str(),
            "instruction": instruction.as_str(),
            "module_id": module_id.as_str(),
            "app_id": module_id.as_str(),
            "install_target": "runtime-installed-module",
            "target": "app",
            "mode": "app",
            "required_skills": [BUSINESS_OS_APP_BENCH_SKILL]
        },
        "client_context": {
            "source": "ctox-cli.business-os-app-modify",
            "target": "app",
            "mode": "app",
            "module_id": module_id.as_str(),
            "app_id": module_id.as_str(),
            "install_target": "runtime-installed-module",
            "actor": actor
        },
        "created_at_ms": now,
        "updated_at_ms": now
    });
    submit_business_os_app_command_document(root, document)
}

fn submit_business_os_app_command_document(
    root: &Path,
    document: serde_json::Value,
) -> anyhow::Result<()> {
    let accepted = crate::business_os::store::accept_rxdb_business_command(root, document)?;
    print_json(&accepted)?;
    let status = accepted
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !matches!(status, "accepted" | "completed") {
        anyhow::bail!("Business OS app command was not accepted");
    }
    Ok(())
}

pub(super) const BUSINESS_OS_APP_BENCH_EVIDENCE_DIR: &str =
    "runtime/business-os/app-creation-bench";
pub(super) const BUSINESS_OS_APP_BENCH_SOURCE: &str = "ctox-cli.business-os-app-bench";
pub(super) const BUSINESS_OS_APP_BENCH_SKILL: &str = "business-os-app-module-development";
pub(super) const BUSINESS_OS_APP_BENCH_USAGE: &str = "ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]\nctox business-os app bench status --run-id <id> [--validate]";
pub(super) const BUSINESS_OS_APP_REFERENCE_DEFAULT_LIMIT: usize = 8;
pub(super) const BUSINESS_OS_APP_REFERENCE_MAX_LIMIT: usize = 24;

fn print_process_output(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        print!("{stdout}");
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }
}

fn handle_business_os_auth(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("issue-capability") => {
            let mut user_id: Option<String> = None;
            let mut email: Option<String> = None;
            let mut display_name: Option<String> = None;
            let mut role: Option<String> = None;
            let mut ensure_user = false;
            let mut idx = 1;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--user" | "--user-id" => {
                        user_id = args.get(idx + 1).cloned();
                        idx += 2;
                    }
                    "--display-name" => {
                        display_name = args.get(idx + 1).cloned();
                        idx += 2;
                    }
                    "--email" => {
                        email = args.get(idx + 1).cloned();
                        idx += 2;
                    }
                    "--role" => {
                        role = args.get(idx + 1).cloned();
                        idx += 2;
                    }
                    "--ensure-user" => {
                        ensure_user = true;
                        idx += 1;
                    }
                    other => {
                        if user_id.is_none() {
                            user_id = Some(other.to_string());
                        }
                        idx += 1;
                    }
                }
            }
            let user_id = user_id
                .context("usage: ctox business-os auth issue-capability --user <user-id>")?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let (token, expires_at_ms) = if ensure_user {
                crate::business_os::store::issue_business_os_capability_token_for_managed_user_with_email(
                    root,
                    &user_id,
                    email.as_deref(),
                    display_name.as_deref().unwrap_or(&user_id),
                    role.as_deref().unwrap_or("user"),
                    now,
                )?
            } else {
                crate::business_os::store::issue_business_os_capability_token(root, &user_id, now)?
            };
            print_json(&serde_json::json!({
                "ok": true,
                "user_id": user_id,
                "capability_token": token,
                "expires_at_ms": expires_at_ms
            }))
        }
        other => anyhow::bail!(
            "usage: ctox business-os auth issue-capability --user <user-id> (got {other:?})"
        ),
    }
}

fn handle_business_os_commands(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("diagnostics") => {
            let mut diagnostics = crate::business_os::store::business_command_diagnostics(root)?;
            if let Some(object) = diagnostics.as_object_mut() {
                object.insert(
                    "native_peer".to_string(),
                    crate::business_os::native_peer_status(root),
                );
            }
            print_json(&diagnostics)
        }
        Some("inspect") => {
            let command_id = args
                .get(1)
                .context("usage: ctox business-os commands inspect <command-id>")?;
            let inspected =
                crate::mission::channels::inspect_business_command(root, command_id)?
                    .with_context(|| format!("business command `{command_id}` was not found"))?;
            print_json(&inspected)
        }
        Some("gc") => {
            let apply = args.iter().any(|arg| arg == "--apply");
            anyhow::ensure!(
                apply || args.iter().any(|arg| arg == "--dry-run"),
                "usage: ctox business-os commands gc (--dry-run | --apply)"
            );
            print_json(
                &crate::mission::channels::business_command_retention_maintenance(root, apply)?,
            )
        }
        Some("reconcile") => {
            let apply = args.iter().any(|arg| arg == "--apply");
            anyhow::ensure!(
                apply || args.iter().any(|arg| arg == "--dry-run"),
                "usage: ctox business-os commands reconcile (--dry-run | --apply)"
            );
            print_json(
                &crate::mission::channels::reconcile_business_command_invariants(root, apply)?,
            )
        }
        Some("process") | Some("process-source-parse") => {
            let command_id = args
                .get(1)
                .context("usage: ctox business-os commands process <command-id>")?;
            let accepted =
                crate::business_os::store::process_source_parse_command(root, command_id)?;
            print_json(&serde_json::to_value(accepted)?)
        }
        Some("retry") => {
            let command_id = args
                .get(1)
                .context("usage: ctox business-os commands retry <command-id>")?;
            print_json(&crate::business_os::store::retry_failed_app_create_command(
                root, command_id,
            )?)
        }
        Some("dispatch") => {
            // Agent-facing entry point for writeback commands (e.g. the
            // `outbound.pipeline.write_outreach_draft` draft writeback). The
            // document is fed through the daemon-owned RxDB command-bus path
            // the native peer uses — no HTTP, external gateway, or direct
            // protected-store write from the sandboxed worker.
            let document = read_command_document(args)?;
            let accepted = crate::service::dispatch_business_command(root, document)?;
            print_json(&accepted)
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os commands command `{other}`"),
    }
}

fn handle_business_os_mcp(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("serve") => {
            let addr = flag_value(args, "--addr").unwrap_or("127.0.0.1:8788");
            crate::business_os::mcp_channel::serve_mcp_channel(
                root,
                crate::business_os::mcp_channel::BusinessOsMcpServeOptions {
                    addr: addr.to_string(),
                },
            )
        }
        Some("connect") => {
            let url = flag_value(args, "--url")
                .or_else(|| {
                    args.get(1)
                        .filter(|value| !value.starts_with("--"))
                        .map(String::as_str)
                })
                .context("usage: ctox business-os mcp connect --url wss://mcp.ctox.dev/connect/<instance-id> [--token <token>]")?;
            let token = flag_value(args, "--token")
                .map(str::to_string)
                .or_else(|| std::env::var("CTOX_BUSINESS_OS_MCP_CONNECT_TOKEN").ok())
                .filter(|value| !value.trim().is_empty());
            let max_reconnect_delay_ms = flag_value(args, "--max-reconnect-delay-ms")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(30_000);
            let heartbeat_interval_ms = flag_value(args, "--heartbeat-interval-ms")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(30_000);
            let max_connection_age_ms = flag_value(args, "--max-connection-age-ms")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(15 * 60 * 1000);
            crate::business_os::mcp_channel::connect_managed_gateway(
                root,
                crate::business_os::mcp_channel::BusinessOsMcpGatewayConnectOptions {
                    url: url.to_string(),
                    token,
                    reconnect: !args.iter().any(|arg| arg == "--once"),
                    max_reconnect_delay_ms,
                    heartbeat_interval_ms,
                    max_connection_age_ms,
                },
            )
        }
        Some("gateway-status") => {
            let url = flag_value(args, "--url")
                .map(str::to_string)
                .or_else(|| {
                    let instance_id = flag_value(args, "--instance-id")
                        .or_else(|| args.get(1).filter(|value| !value.starts_with("--")).map(String::as_str))?;
                    let base = flag_value(args, "--base").unwrap_or("https://mcp.ctox.dev");
                    Some(format!("{}/status/{}", base.trim_end_matches('/'), instance_id))
                })
                .context("usage: ctox business-os mcp gateway-status --url https://mcp.ctox.dev/status/<instance-id> [--token <token>]")?;
            let token = flag_value(args, "--token")
                .map(str::to_string)
                .or_else(|| std::env::var("CTOX_BUSINESS_OS_MCP_GATEWAY_TOKEN").ok())
                .filter(|value| !value.trim().is_empty());
            print_json(&crate::business_os::mcp_channel::managed_gateway_status(
                crate::business_os::mcp_channel::BusinessOsMcpGatewayStatusOptions { url, token },
            )?)
        }
        Some("tools") => print_json(&serde_json::json!({
            "ok": true,
            "tools": crate::business_os::mcp_channel::tool_descriptors()
        })),
        Some("policy") => handle_business_os_mcp_policy(root, &args[1..]),
        Some("call") => {
            let tool = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .context("usage: ctox business-os mcp call <tool-name> [--args <json>]")?;
            let arguments = flag_value(args, "--args")
                .or_else(|| flag_value(args, "--arguments"))
                .map(serde_json::from_str)
                .transpose()
                .context("invalid --args JSON")?
                .unwrap_or_else(|| serde_json::json!({}));
            print_json(&crate::business_os::mcp_channel::call_tool_audited(
                root, tool, arguments,
            )?)
        }
        Some("audit") => {
            if args.iter().any(|arg| arg == "--prune") {
                let deleted = crate::business_os::mcp_channel::prune_mcp_activity(root)?;
                print_json(&serde_json::json!({
                    "ok": true,
                    "deleted": deleted,
                    "policy": crate::business_os::mcp_channel::mcp_policy(root)
                }))?;
                return Ok(());
            }
            let limit = flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .or_else(|| args.get(1).and_then(|value| value.parse::<usize>().ok()));
            let format = match flag_value(args, "--format").unwrap_or("json") {
                "json" => crate::business_os::mcp_channel::BusinessOsMcpAuditExportFormat::Json,
                "jsonl" | "ndjson" => {
                    crate::business_os::mcp_channel::BusinessOsMcpAuditExportFormat::Jsonl
                }
                other => anyhow::bail!("unsupported MCP audit export format `{other}`"),
            };
            let context = crate::business_os::mcp_channel::McpChannelRequestContext {
                channel: "chatgpt_mcp".to_string(),
                surface: "business_os_mcp".to_string(),
                actor: "ctox-cli:mcp-audit".to_string(),
                workspace: "local".to_string(),
                tool: "business_os.list_mcp_activity".to_string(),
                request_id: format!("cli-{}", Uuid::new_v4()),
                confirmation_state:
                    crate::business_os::mcp_channel::McpConfirmationState::NotRequired,
                trusted_role: None,
                trusted_role_source: None,
            };
            let export = crate::business_os::mcp_channel::export_mcp_activity(
                root, &context, limit, format,
            )?;
            if let Some(output) = flag_value(args, "--output").or_else(|| flag_value(args, "-o")) {
                fs::write(output, export)
                    .with_context(|| format!("failed to write MCP audit export to `{output}`"))?;
                print_json(&serde_json::json!({
                    "ok": true,
                    "path": output
                }))
            } else {
                print!("{export}");
                Ok(())
            }
        }
        Some("status") | None => {
            let context = crate::business_os::mcp_channel::McpChannelRequestContext {
                channel: "chatgpt_mcp".to_string(),
                surface: "business_os_mcp".to_string(),
                actor: "ctox-cli:mcp-status".to_string(),
                workspace: "local".to_string(),
                tool: "business_os.status".to_string(),
                request_id: format!("cli-{}", Uuid::new_v4()),
                confirmation_state:
                    crate::business_os::mcp_channel::McpConfirmationState::NotRequired,
                trusted_role: None,
                trusted_role_source: None,
            };
            print_json(&crate::business_os::mcp_channel::mcp_status(
                root, &context,
            )?)
        }
        Some("--help") | Some("-h") => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os mcp command `{other}`"),
    }
}

fn handle_business_os_mcp_policy(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("set") => {
            let mut policy = crate::business_os::mcp_channel::mcp_policy(root);
            apply_mcp_policy_bool_arg(args, "--enabled", &mut policy.enabled)?;
            apply_mcp_policy_bool_arg(args, "--allow-reads", &mut policy.allow_reads)?;
            apply_mcp_policy_bool_arg(args, "--allow-writes", &mut policy.allow_writes)?;
            apply_mcp_policy_bool_arg(args, "--allow-approvals", &mut policy.allow_approvals)?;
            apply_mcp_policy_bool_arg(
                args,
                "--allow-external-effects",
                &mut policy.allow_external_effects,
            )?;
            apply_mcp_policy_usize_arg(
                args,
                "--rate-limit-per-minute",
                &mut policy.rate_limit_per_minute,
            )?;
            apply_mcp_policy_usize_arg(
                args,
                "--audit-retention-days",
                &mut policy.audit_retention_days,
            )?;
            if args.iter().any(|arg| arg == "--clear-deny-tools") {
                policy.denied_tools.clear();
            }
            if args.iter().any(|arg| arg == "--clear-allowed-actors") {
                policy.allowed_actors.clear();
            }
            if args.iter().any(|arg| arg == "--clear-allowed-workspaces") {
                policy.allowed_workspaces.clear();
            }
            if args.iter().any(|arg| arg == "--clear-allowed-modules") {
                policy.allowed_modules.clear();
            }
            if args.iter().any(|arg| arg == "--clear-allowed-collections") {
                policy.allowed_collections.clear();
            }
            apply_mcp_policy_values_arg(args, "--allow-actor", &mut policy.allowed_actors);
            apply_mcp_policy_values_arg(args, "--allow-workspace", &mut policy.allowed_workspaces);
            apply_mcp_policy_values_arg(args, "--allow-module", &mut policy.allowed_modules);
            apply_mcp_policy_values_arg(
                args,
                "--allow-collection",
                &mut policy.allowed_collections,
            );
            let deny_tools = mcp_policy_deny_tools_from_args(args)?;
            if !deny_tools.is_empty() {
                policy.denied_tools = deny_tools;
            }
            crate::business_os::mcp_channel::save_mcp_policy(root, &policy)?;
            print_json(&serde_json::json!({
                "ok": true,
                "policy": crate::business_os::mcp_channel::mcp_policy(root),
                "storage": "business_os.mcp_policy.v1",
                "keys": mcp_policy_env_projection(&mcp_policy_to_env_map(&policy))
            }))
        }
        Some("keys") => {
            let policy = crate::business_os::mcp_channel::mcp_policy(root);
            print_json(&serde_json::json!({
                "ok": true,
                "storage": "business_os.mcp_policy.v1",
                "keys": mcp_policy_env_projection(&mcp_policy_to_env_map(&policy))
            }))
        }
        Some("--help") | Some("-h") => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os mcp policy command `{other}`"),
        None => {
            let policy = crate::business_os::mcp_channel::mcp_policy(root);
            print_json(&serde_json::json!({
                "ok": true,
                "policy": policy,
                "storage": "business_os.mcp_policy.v1",
                "keys": mcp_policy_env_projection(&mcp_policy_to_env_map(&policy))
            }))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalCtoxSecretRef {
    scope: String,
    name: String,
}

pub(crate) fn run_business_os_web_stack_auth_assist_request(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    let source_id = flag_value(args, "--source-id")
        .context("usage: ctox business-os web-stack auth-assist-request --source-id <id> [--target-url <url>] [--credential-ref <ctox-secret://scope/name>] [--login-hint <hint>] [--task-id <id>]")?;
    let target_url_override = flag_value(args, "--target-url");
    let credential_ref = optional_web_stack_credential_ref(flag_value(args, "--credential-ref"))?;
    let login_hint = optional_web_stack_login_hint(flag_value(args, "--login-hint"));
    let requesting_task_id = flag_value(args, "--task-id").unwrap_or_default();
    let owner_user_id =
        resolve_web_stack_auth_owner_user_id(root, args, requesting_task_id, false)?;
    enqueue_web_stack_auth_assist_request(
        root,
        source_id,
        target_url_override,
        credential_ref.as_deref(),
        login_hint.as_deref(),
        None,
        requesting_task_id,
        "ctox_web_auth_assist_request",
        "ctox_web_auth_assist_request",
        owner_user_id.as_deref(),
        false,
        true,
    )
}

fn run_business_os_web_stack_auth_assist_login(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    run_business_os_web_stack_auth_assist_login_with_continuation(root, args, None, false)
}

pub(crate) fn run_business_os_web_stack_authenticated_automation(
    root: &Path,
    args: &[String],
    continuation_source: &str,
) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        !continuation_source.trim().is_empty(),
        "authenticated-automation requires a non-empty browser automation source"
    );
    let combined = run_business_os_web_stack_auth_assist_login_with_continuation(
        root,
        args,
        Some(continuation_source),
        false,
    )?;
    anyhow::ensure!(
        combined.get("ok").and_then(serde_json::Value::as_bool) == Some(true),
        "authenticated-automation login gate did not complete successfully"
    );
    let mut result = combined
        .pointer("/automation/result/post_auth_result")
        .cloned()
        .context("authenticated-automation did not produce post_auth_result")?;
    let result_ok = result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    if let Some(object) = result.as_object_mut() {
        object.insert("ok".to_string(), serde_json::Value::Bool(result_ok));
        object.entry("status".to_string()).or_insert_with(|| {
            serde_json::Value::String(if result_ok { "completed" } else { "failed" }.to_string())
        });
        object.insert(
            "session_id".to_string(),
            combined
                .get("session_id")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        );
        object.insert(
            "authenticated_login".to_string(),
            serde_json::json!({
                "status": combined.get("status").cloned().unwrap_or(serde_json::Value::Null),
                "login_state": combined.get("login_state").cloned().unwrap_or(serde_json::Value::Null),
                "verify_selector_found": combined
                    .pointer("/login_result/verify_selector_found")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                "redacted": true,
            }),
        );
    }
    Ok(result)
}

fn run_business_os_web_stack_auth_assist_login_with_continuation(
    root: &Path,
    args: &[String],
    continuation_source: Option<&str>,
    accept_owner_as_trusted_local: bool,
) -> anyhow::Result<serde_json::Value> {
    let source_id = flag_value(args, "--source-id")
        .context("usage: ctox business-os web-stack auth-assist-login --source-id <id> --credential-ref <ctox-secret://scope/name> [--target-url <url>] [--login-hint <hint>] [--task-id <id>] [--timeout-ms <n>] [--dir <path>] [--credential-selector <selector>] [--verify-selector <selector>]")?;
    let target_url_override = flag_value(args, "--target-url");
    let credential_ref =
        optional_web_stack_credential_ref(flag_value(args, "--credential-ref"))?
            .context("auth-assist-login requires --credential-ref <ctox-secret://scope/name>")?;
    let local_secret_ref = parse_local_ctox_secret_ref(&credential_ref)?;
    let explicit_login_hint = optional_web_stack_login_hint(flag_value(args, "--login-hint"));
    let requesting_task_id = flag_value(args, "--task-id").unwrap_or_default();
    let owner_user_id = resolve_web_stack_auth_owner_user_id(
        root,
        args,
        requesting_task_id,
        accept_owner_as_trusted_local,
    )?;
    let timeout_ms = flag_value(args, "--timeout-ms")
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("failed to parse --timeout-ms `{value}`"))
        })
        .transpose()?
        .unwrap_or(45_000)
        .clamp(1_000, 300_000);
    let browser_dir = flag_value(args, "--dir").map(PathBuf::from);
    let secret_value =
        crate::secrets::read_secret_value(root, &local_secret_ref.scope, &local_secret_ref.name)
            .with_context(|| {
                format!(
            "failed to resolve credential_ref {credential_ref}; expected ctox secret scope/name"
        )
            })?;
    anyhow::ensure!(
        !secret_value.trim().is_empty(),
        "credential_ref {credential_ref} resolved to an empty secret"
    );
    let resolved_credential =
        resolve_web_stack_credential(&secret_value, explicit_login_hint.clone());
    anyhow::ensure!(
        !resolved_credential.credential_value.trim().is_empty(),
        "credential_ref {credential_ref} resolved to an empty credential value"
    );
    let auth_assist = enqueue_web_stack_auth_assist_request(
        root,
        source_id,
        target_url_override,
        Some(&credential_ref),
        explicit_login_hint.as_deref(),
        None,
        requesting_task_id,
        "ctox_harness",
        "ctox_web_auth_assist_login",
        owner_user_id.as_deref(),
        false,
        // Native CLI intake is trusted-local; enqueueing the request as a
        // plain replicated document made the later intake demand a capability
        // token the daemon does not have and reject its own auth request.
        true,
    )?;
    let session_id = auth_assist
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .context("auth-assist login did not produce a session_id")?
        .to_string();
    let target_url = auth_assist
        .get("target_url")
        .and_then(serde_json::Value::as_str)
        .context("auth-assist login did not produce a target_url")?
        .to_string();
    let source_id = auth_assist
        .get("source_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(source_id)
        .to_string();
    let credential_selector = flag_value(args, "--credential-selector")
        .map(str::to_string)
        .or_else(|| {
            auth_assist
                .get("credential_selector")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let verify_selector = flag_value(args, "--verify-selector")
        .map(str::to_string)
        .or_else(|| {
            auth_assist
                .get("verify_selector")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let automation_source = build_web_stack_auth_assist_login_source_with_continuation(
        &target_url,
        &source_id,
        resolved_credential.login_hint.as_deref(),
        &credential_ref,
        &resolved_credential.credential_value,
        credential_selector.trim(),
        verify_selector.trim(),
        continuation_source,
    )?;
    let mut automation = crate::business_os::run_browser_session_automation(
        root,
        crate::business_os::BrowserSessionAutomationRequest {
            session_id: session_id.clone(),
            dir: browser_dir,
            timeout_ms: Some(timeout_ms),
            source: automation_source,
            profile_owner: auth_assist
                .get("owner_user_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        },
    )?;
    redact_secret_value_from_json(&mut automation, &secret_value);
    redact_secret_value_from_json(&mut automation, &resolved_credential.credential_value);
    if resolved_credential.login_hint_from_secret {
        if let Some(login_hint) = resolved_credential.login_hint.as_deref() {
            redact_secret_value_from_json(&mut automation, login_hint);
        }
    }
    let login_result = automation
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let automation_ok = automation
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let login_ok = login_result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(automation_ok);
    let login_state = login_result
        .get("login_state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if automation_ok && login_ok {
            "authenticated"
        } else {
            "login_failed"
        });
    let status = if automation_ok && login_ok {
        "completed"
    } else {
        match login_state {
            "mfa_required" => "mfa_required",
            "verify_selector_missing" => "verify_selector_missing",
            _ => "login_failed",
        }
    };
    let mfa_required = login_result
        .get("mfa_required")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let login_error_detected = login_result
        .get("login_error_detected")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Ok(serde_json::json!({
        "ok": automation_ok && login_ok,
        "status": status,
        "command": "business-os web-stack auth-assist-login",
        "session_id": session_id,
        "source_id": source_id,
        "target_url": target_url,
        "credential_ref": credential_ref,
        "login_hint": explicit_login_hint,
        "login_hint_from_credential_bundle": resolved_credential.login_hint_from_secret,
        "login_state": login_state,
        "mfa_required": mfa_required,
        "login_error_detected": login_error_detected,
        "verify_selector": verify_selector,
        "credential_selector": credential_selector,
        "secret_value_in_payload": false,
        "frame_data_in_payload": false,
        "browser_stream": "rxdb",
        "timeout_ms": timeout_ms,
        "auth_assist_request": auth_assist,
        "login_result": login_result,
        "automation": automation,
    }))
}

pub(crate) fn run_business_os_web_stack_source_capture(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    run_business_os_web_stack_source_capture_with_trust(root, args, false)
}

pub(crate) fn run_business_os_web_stack_source_capture_trusted(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    run_business_os_web_stack_source_capture_with_trust(root, args, true)
}

fn run_business_os_web_stack_source_capture_with_trust(
    root: &Path,
    args: &[String],
    accept_as_trusted_local: bool,
) -> anyhow::Result<serde_json::Value> {
    let source_id = flag_value(args, "--source-id")
        .context("source-capture requires --source-id <id>")?
        .trim()
        .to_ascii_lowercase();
    anyhow::ensure!(
        matches!(
            source_id.as_str(),
            "dnbhoovers.com" | "leadfeeder.com" | "rocketreach.com" | "xing.com"
        ),
        "source-capture supports only authenticated D&B, Leadfeeder, RocketReach, and XING sessions"
    );
    let company = flag_value(args, "--company")
        .context("source-capture requires --company <name>")?
        .trim();
    anyhow::ensure!(!company.is_empty(), "source-capture company is empty");
    let country = flag_value(args, "--country").unwrap_or("DE").trim();
    let source = build_web_stack_authenticated_source_capture(&source_id, company, country)?;
    let credential_ref = optional_web_stack_credential_ref(flag_value(args, "--credential-ref"))?
        .or_else(|| {
            ctox_web_stack::sources::find(&source_id)
                .and_then(|module| module.browser_recipe())
                .and_then(|recipe| recipe.required_secret_name)
                .map(|name| format!("ctox-secret://credentials/{name}"))
        })
        .or_else(|| {
            web_stack_auth_source_defaults(&source_id)
                .map(|defaults| format!("ctox-secret://credentials/{}", defaults.secret_name))
        });
    let credential_available = credential_ref
        .as_deref()
        .and_then(|reference| parse_local_ctox_secret_ref(reference).ok())
        .and_then(|reference| {
            crate::secrets::read_secret_value(root, &reference.scope, &reference.name).ok()
        })
        .is_some_and(|value| !value.trim().is_empty());
    if !credential_available {
        return run_business_os_web_stack_source_capture_with_browser_authorization(
            root,
            args,
            &source_id,
            company,
            country,
            credential_ref.as_deref(),
            &source,
            accept_as_trusted_local,
        );
    }
    let credential_ref =
        credential_ref.context("source-capture requires a browser credential_ref")?;
    let mut login_args = args.to_vec();
    if flag_value(&login_args, "--credential-ref").is_none() {
        login_args.push("--credential-ref".to_string());
        login_args.push(credential_ref.clone());
    }
    if flag_value(&login_args, "--target-url").is_none() {
        if let Some(defaults) = web_stack_auth_source_defaults(&source_id) {
            login_args.push("--target-url".to_string());
            login_args.push(defaults.login_url.to_string());
        }
    }
    let combined = run_business_os_web_stack_auth_assist_login_with_continuation(
        root,
        &login_args,
        Some(&source),
        accept_as_trusted_local,
    )?;
    let result = combined
        .pointer("/login_result/post_auth_result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let records = result
        .get("records")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(serde_json::json!({
        "ok": combined.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false)
            && matches!(result.get("status").and_then(serde_json::Value::as_str), Some("completed") | Some("succeeded") | Some("no_match") | Some("no_extractable_fields")),
        "source_id": source_id,
        "session_id": combined.get("session_id").cloned().unwrap_or(serde_json::Value::Null),
        "company": company,
        "country": country,
        "records": records,
        "record_count": records.len(),
        "source_status": result.get("status").and_then(serde_json::Value::as_str).unwrap_or("failed"),
        "source_url": result.get("source_url").and_then(serde_json::Value::as_str),
        "credential_ref": credential_ref,
        "secret_value_in_payload": false,
        "browser_stream": "rxdb",
        "authenticated_capture": combined,
    }))
}

fn enqueue_web_stack_source_capture_auth_assist(
    root: &Path,
    args: &[String],
    source_id: &str,
    credential_ref: Option<&str>,
    accept_owner_as_trusted_local: bool,
) -> anyhow::Result<serde_json::Value> {
    let requesting_task_id = flag_value(args, "--task-id").unwrap_or_default();
    let owner_user_id = resolve_web_stack_auth_owner_user_id(
        root,
        args,
        requesting_task_id,
        accept_owner_as_trusted_local,
    )?;
    enqueue_web_stack_auth_assist_request(
        root,
        source_id,
        flag_value(args, "--target-url"),
        credential_ref,
        optional_web_stack_login_hint(flag_value(args, "--login-hint")).as_deref(),
        None,
        requesting_task_id,
        "ctox_web_source_capture",
        "ctox_web_source_capture",
        owner_user_id.as_deref(),
        true,
        true,
    )
}

fn run_business_os_web_stack_source_capture_with_browser_authorization(
    root: &Path,
    args: &[String],
    source_id: &str,
    company: &str,
    country: &str,
    credential_ref: Option<&str>,
    continuation_source: &str,
    accept_owner_as_trusted_local: bool,
) -> anyhow::Result<serde_json::Value> {
    let timeout_ms = flag_value(args, "--timeout-ms")
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("failed to parse --timeout-ms `{value}`"))
        })
        .transpose()?
        .unwrap_or(45_000)
        .clamp(1_000, 300_000);
    let browser_dir = flag_value(args, "--dir").map(PathBuf::from);
    let browser_assist = enqueue_web_stack_source_capture_auth_assist(
        root,
        args,
        source_id,
        credential_ref,
        accept_owner_as_trusted_local,
    )?;
    let session_id = browser_assist
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .context("source-capture browser authorization did not produce a session_id")?
        .to_string();
    let target_url = browser_assist
        .get("target_url")
        .and_then(serde_json::Value::as_str)
        .context("source-capture browser authorization did not produce a target_url")?;
    let source = format!(
        "await ctoxBrowser.goto({}, {{ timeoutMs: {} }});\n{}",
        serde_json::to_string(target_url)?,
        timeout_ms.min(60_000),
        continuation_source
    );
    let automation = crate::business_os::run_browser_session_automation(
        root,
        crate::business_os::BrowserSessionAutomationRequest {
            session_id: session_id.clone(),
            dir: browser_dir,
            timeout_ms: Some(timeout_ms),
            source,
            profile_owner: browser_assist
                .get("owner_user_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        },
    )?;
    let result = automation
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let records = result
        .get("records")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let source_status = result
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("auth_required");
    let automation_ok = automation
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let capture_complete = matches!(
        source_status,
        "completed" | "succeeded" | "no_match" | "no_extractable_fields"
    );
    let login_state = if capture_complete {
        "authenticated"
    } else {
        "user_authorization_required"
    };
    Ok(serde_json::json!({
        "ok": automation_ok && capture_complete,
        "source_id": source_id,
        "session_id": session_id,
        "company": company,
        "country": country,
        "records": records,
        "record_count": records.len(),
        "source_status": source_status,
        "source_url": result.get("source_url").and_then(serde_json::Value::as_str),
        "credential_ref": credential_ref,
        "secret_value_in_payload": false,
        "browser_stream": "rxdb",
        "browser_assist": browser_assist,
        "authenticated_capture": {
            "ok": automation_ok && capture_complete,
            "status": if capture_complete { "completed" } else { "user_authorization_required" },
            "login_state": login_state,
            "mfa_required": false,
            "login_error_detected": false,
            "session_id": session_id,
            "secret_value_in_payload": false,
            "browser_stream": "rxdb",
            "login_result": {
                "ok": automation_ok && capture_complete,
                "login_state": login_state,
                "post_auth_result": result,
            },
            "automation": automation,
        },
    }))
}

fn build_web_stack_authenticated_source_capture(
    source_id: &str,
    company: &str,
    country: &str,
) -> anyhow::Result<String> {
    if source_id == "xing.com" {
        return ctox_web_stack::sources::xing::build_authenticated_browser_capture(
            company, country,
        );
    }
    if source_id == "rocketreach.com" {
        return build_web_stack_rocketreach_source_capture(company, country);
    }
    Ok(format!(
        r#"const sourceId = {};
const company = {};
const country = {};
const allowedHosts = {{
  "dnbhoovers.com": ["dnbhoovers.com", "app.dnbhoovers.com"],
  "leadfeeder.com": ["leadfeeder.com", "app.leadfeeder.com"],
}}[sourceId];
const hostAllowed = (raw) => {{
  try {{
    const host = new URL(raw).hostname.toLowerCase();
    return allowedHosts.some((allowed) => host === allowed || host.endsWith(`.${{allowed}}`));
  }} catch {{ return false; }}
}};
const currentUrl = page.url();
if (/login|signin|auth/i.test(currentUrl)) {{
  return {{ status: "auth_required", source_url: currentUrl, records: [] }};
}}
{{
  const candidates = [
    'input[placeholder*="search" i]',
    'input[placeholder*="suche" i]',
    'input[placeholder*="firma" i]',
    'input[aria-label*="search" i]',
    'input[type="search"]',
  ];
  for (const selector of candidates) {{
    const field = page.locator(selector).first();
    if ((await field.count()) < 1
        || !(await field.isVisible().catch(() => false))
        || !(await field.isEditable().catch(() => false))) continue;
    try {{
      await field.fill(company, {{ timeout: 3000 }});
      await field.press("Enter", {{ timeout: 3000 }});
      break;
    }} catch {{}}
  }}
}}
await page.waitForLoadState("networkidle", {{ timeout: 12000 }}).catch(() => null);
if (sourceId === "dnbhoovers.com") {{
  await page.locator('a[href*="/company/"]')
    .filter({{ hasText: company }})
    .first()
    .waitFor({{ state: "visible", timeout: 10000 }})
    .catch(() => null);
}}
await page.waitForTimeout(1200);
if (!hostAllowed(page.url())) {{
  return {{ status: "wrong_origin", source_url: page.url(), records: [] }};
}}
const snapshot = await page.evaluate((companyName) => {{
  const clean = (value, max = 1500) => String(value || "").replace(/\s+/g, " ").trim().slice(0, max);
  const contextFor = (link) => {{
    let node = link.parentElement;
    let context = clean(link.innerText || link.textContent || "");
    for (let depth = 0; node && depth < 8; depth += 1, node = node.parentElement) {{
      const candidate = clean(node.innerText || node.textContent || "");
      if (candidate.length >= context.length && candidate.length <= 1500) context = candidate;
      if (candidate.length > 1500) break;
    }}
    return context;
  }};
  const text = String(document.body?.innerText || "").replace(/\s+/g, " ").trim().slice(0, 100000);
  const links = Array.from(document.querySelectorAll("a[href]"))
    .map((link) => ({{ url: link.href, text: clean(link.innerText || link.textContent || ""), context: contextFor(link) }}))
    .filter((link) => link.text)
    .slice(0, 500);
  return {{ text, links, title: document.title, companyName }};
}}, company);
const normalized = (value) => String(value || "").toLocaleLowerCase("de-DE").replace(/[^a-z0-9äöüß]+/g, " ").trim();
const companyTokens = normalized(company).split(/\s+/).filter((token) => token.length >= 4 && !["gmbh", "gesellschaft", "aktiengesellschaft"].includes(token));
const corpus = normalized(`${{snapshot.title}} ${{snapshot.text}}`);
const matched = companyTokens.length > 0 && companyTokens.slice(0, 2).every((token) => corpus.includes(token));
const records = [];
const push = (field, value, confidence, note, url = page.url()) => {{
  if (!value || !hostAllowed(url)) return;
  const normalizedValue = String(value).trim();
  if (records.some((record) => record.field === field && record.value === normalizedValue)) return;
  records.push({{ field, value: normalizedValue, confidence, source_url: url, note }});
}};
const sourceLinks = snapshot.links.filter((link) => hostAllowed(link.url));
if (sourceId === "dnbhoovers.com") {{
  const companyName = normalized(company);
  const hit = sourceLinks.find((link) => normalized(link.text) === companyName && /\/company\//i.test(link.url));
  if (hit) {{
    const context = hit.context || snapshot.text;
    push("firma_name", hit.text, "high", "D&B Hoovers exact company result", hit.url);
    const phone = context.match(/(?:\+|00)\d[\d\s()\/-]{{7,}}\d/)?.[0];
    const city = context.match(/\bFolgen\s+([^,]{{2,80}}),/)?.[1];
    const revenue = context.match(/Umsatz\s+EUR:\s*([0-9.,]+\s*[BMK]?)/i)?.[1];
    const employees = context.match(/Beschäftigte\s*\(Gesamt\):\s*([0-9.,]+\s*[KMB]?)/i)?.[1];
    const duns = context.match(/D-U-N-S:\s*([0-9-]+)/i)?.[1];
    const industry = phone
      ? context.match(new RegExp(`${{phone.replace(/[.*+?^${{}}()|[\]\\]/g, "\\$&")}}\\s+(.+?)\\s+(?:Private|Public|Nonprofit)`, "i"))?.[1]
      : null;
    const structure = context.match(/\b((?:Private|Public|Nonprofit)(?:Parent|Subsidiary|Branch|Independent)(?:Firmenzentrale)?)\b/i)?.[1];
    push("firma_telefon", phone, "high", "D&B Hoovers company result", hit.url);
    push("firma_ort", city, "high", "D&B Hoovers company result", hit.url);
    push("branche", industry, "high", "D&B Hoovers industry", hit.url);
    push("umsatz", revenue, "high", "D&B Hoovers revenue EUR", hit.url);
    push("mitarbeiter", employees, "high", "D&B Hoovers employee total", hit.url);
    push("firma_register", duns, "high", "D&B D-U-N-S", hit.url);
    push("konzernstruktur", structure, "medium", "D&B Hoovers organization role", hit.url);
  }}
}} else if (sourceId === "leadfeeder.com") {{
  const companyName = normalized(company);
  const hit = sourceLinks.find((link) => normalized(link.text) === companyName && /\/h\/company\//i.test(link.url));
  if (hit) {{
    const context = hit.context || snapshot.text;
    const escapedName = hit.text.replace(/[.*+?^${{}}()|[\]\\]/g, "\\$&");
    const employeePattern = "[0-9]{{1,3}}(?:[.,][0-9]{{3}})*[-–][0-9]{{1,3}}(?:[.,][0-9]{{3}})*";
    const location = context.match(new RegExp(`${{escapedName}}\\s+(.{{2,80}}?)\\s+${{employeePattern}}`, "i"))?.[1];
    const employees = context.match(new RegExp(`\\b(${{employeePattern}})\\b`))?.[1];
    const website = context.match(/\b(?:https?:\/\/|www\.)[^\s]+/i)?.[0]?.replace(/\.\.\.$/, "");
    push("firma_name", hit.text, "high", "Leadfeeder exact company result", hit.url);
    push("firma_ort", location, "medium", "Leadfeeder company location", hit.url);
    push("mitarbeiter", employees, "medium", "Leadfeeder employee range", hit.url);
    push("firma_domain", website, "medium", "Leadfeeder company website", hit.url);
  }}
}}
return {{
  status: records.length > 0 ? "succeeded" : (matched ? "no_extractable_fields" : "no_match"),
  source_url: page.url(),
  country,
  records,
}};"#,
        serde_json::to_string(source_id)?,
        serde_json::to_string(company)?,
        serde_json::to_string(country)?,
    ))
}

#[derive(Clone, Copy)]
struct WebStackAuthSourceDefaults {
    login_url: &'static str,
    allowed_domains: &'static [&'static str],
    secret_name: &'static str,
}

fn web_stack_auth_source_defaults(source_id: &str) -> Option<WebStackAuthSourceDefaults> {
    match source_id.trim().to_ascii_lowercase().as_str() {
        "rocketreach.com" => Some(WebStackAuthSourceDefaults {
            login_url: "https://rocketreach.co/login",
            allowed_domains: &["rocketreach.com", "rocketreach.co"],
            secret_name: "ROCKETREACH_BROWSER_LOGIN",
        }),
        _ => None,
    }
}

fn build_web_stack_rocketreach_source_capture(
    company: &str,
    country: &str,
) -> anyhow::Result<String> {
    let company = serde_json::to_string(company)?;
    let country = serde_json::to_string(country)?;
    Ok(ROCKETREACH_BROWSER_CAPTURE_TEMPLATE
        .replace("__COMPANY_JSON__", &company)
        .replace("__COUNTRY_JSON__", &country)
        .replace("__RECORD_PARSER__", ROCKETREACH_BROWSER_RECORD_PARSER))
}

const ROCKETREACH_BROWSER_RECORD_PARSER: &str = r#"const parseRocketReachRecords = (companyName, snapshots) => {
  const clean = (value, max = 500) => String(value || "").replace(/\s+/g, " ").trim().slice(0, max);
  const normalize = (value) => clean(value, 3000)
    .toLocaleLowerCase("de-DE")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/ß/g, "ss")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
  const ignoredCompanyTokens = new Set([
    "ag", "se", "gmbh", "kg", "ohg", "gbr", "mbh", "inc", "ltd", "llc",
    "gesellschaft", "aktiengesellschaft", "holding", "group", "gruppe", "company",
  ]);
  const companyTokens = normalize(companyName).split(/\s+/).filter((token) =>
    token.length >= 3 && !ignoredCompanyTokens.has(token)
  );
  const relevantCompanyText = (value) => {
    const required = companyTokens.slice(0, Math.min(2, companyTokens.length));
    const haystack = normalize(value);
    return required.length > 0 && required.every((token) => haystack.includes(token));
  };
  const providerUrl = (raw) => {
    try {
      const url = new URL(raw);
      const host = url.hostname.toLowerCase();
      return host === "rocketreach.com" || host.endsWith(".rocketreach.com")
        || host === "rocketreach.co" || host.endsWith(".rocketreach.co") ? url : null;
    } catch { return null; }
  };
  const companyProfileUrl = (raw) => {
    const url = providerUrl(raw);
    if (!url) return null;
    return /(?:\/company\/|\/companies\/|-profile_[a-z0-9])/i.test(url.pathname) ? url : null;
  };
  const personProfileUrl = (raw) => {
    const url = providerUrl(raw);
    if (!url) return null;
    return /(?:\/person\/|\/people\/|-email_[a-z0-9])/i.test(url.pathname) ? url : null;
  };
  const validNamePart = (value) => /\p{L}/u.test(value)
    && !/\d/u.test(value)
    && /^[\p{L}][\p{L}.'’\-]*$/u.test(value);
  const personName = (candidate) => {
    const values = [candidate.name, candidate.heading, candidate.linkText, candidate.title];
    for (const raw of values) {
      const value = clean(raw)
        .replace(/\s*[|·-]\s*RocketReach.*$/i, "")
        .replace(/\s+(?:email|phone|contact)(?:\s+information)?\s*$/i, "")
        .replace(/^(?:(?:dr|prof)\.?\s+)+/i, "");
      const parts = value.split(/\s+/).filter(Boolean);
      if (parts.length >= 2 && parts.length <= 6 && parts.every(validNamePart)
          && !relevantCompanyText(value)) return parts;
    }
    return [];
  };
  const records = [];
  const push = (field, value, confidence, note, rawUrl) => {
    const url = providerUrl(rawUrl);
    const cleanValue = clean(value);
    if (!url || !cleanValue) return;
    if (records.some((record) => record.field === field
        && record.value === cleanValue && record.source_url === url.href)) return;
    records.push({ field, value: cleanValue, confidence, source_url: url.href, note });
  };
  const safeSnapshots = (Array.isArray(snapshots) ? snapshots : []).filter((snapshot) =>
    providerUrl(snapshot?.sourceUrl)
  );
  let companyEvidenceUrl = null;
  let companyEvidenceName = "";
  for (const snapshot of safeSnapshots) {
    const candidates = [
      ...(snapshot.links || []).map((link) => ({
        url: link.url,
        text: `${link.text || ""} ${(link.contextLines || []).join(" ")}`,
        name: link.text,
      })),
      { url: snapshot.sourceUrl, text: `${snapshot.title || ""} ${(snapshot.headings || []).join(" ")}`, name: (snapshot.headings || [])[0] },
    ];
    const hit = candidates.find((candidate) => companyProfileUrl(candidate.url)
      && relevantCompanyText(candidate.text));
    if (!hit) continue;
    companyEvidenceUrl = companyProfileUrl(hit.url).href;
    companyEvidenceName = clean(hit.name) || companyName;
    break;
  }
  if (!companyEvidenceUrl) return { records: [], companyMatched: false, protectedFieldCount: 0 };
  push("firma_name", relevantCompanyText(companyEvidenceName) ? companyEvidenceName : companyName,
    "high", "RocketReach company identity verified", companyEvidenceUrl);

  const people = [];
  for (const snapshot of safeSnapshots) {
    for (const embedded of snapshot.embeddedPeople || []) {
      people.push({ ...embedded, sourceUrl: embedded.sourceUrl || snapshot.sourceUrl });
    }
    for (const link of snapshot.links || []) {
      const url = personProfileUrl(link.url);
      const contextLines = (link.contextLines || []).map((line) => clean(line)).filter(Boolean);
      if (!url || !relevantCompanyText(contextLines.join(" "))) continue;
      people.push({ sourceUrl: url.href, linkText: link.text, contextLines });
    }
    if (personProfileUrl(snapshot.sourceUrl)) {
      people.push({
        sourceUrl: snapshot.sourceUrl,
        heading: (snapshot.headings || [])[0],
        title: snapshot.title,
        contextLines: snapshot.bodyLines || [],
      });
    }
  }
  const seenPeople = new Set();
  for (const candidate of people) {
    const sourceUrl = personProfileUrl(candidate.sourceUrl);
    if (!sourceUrl) continue;
    const lines = (candidate.contextLines || []).map((line) => clean(line)).filter(Boolean);
    const companyContext = clean(candidate.company || lines.join(" "), 4000);
    if (!relevantCompanyText(companyContext)) continue;
    const names = personName(candidate);
    if (names.length < 2) continue;
    const personKey = `${normalize(names.join(" "))}|${sourceUrl.href}`;
    if (seenPeople.has(personKey)) continue;
    seenPeople.add(personKey);
    const context = lines.join(" ");
    const email = clean(candidate.email || context.match(/\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/i)?.[0]);
    const phone = clean(candidate.phone || context.match(/(?:\+|00)\d[\d\s().\/-]{6,}\d/)?.[0]);
    const ignoredPosition = /^(?:contact|contacts?|email|phone|telefon|mobile|mobil|location|standort|rocketreach|view profile|profil ansehen)$/i;
    const position = clean(candidate.position || lines.find((line) =>
      line.length >= 2 && line.length <= 160
      && /\p{L}/u.test(line)
      && !relevantCompanyText(line)
      && normalize(line) !== normalize(names.join(" "))
      && !ignoredPosition.test(line)
      && !/@/.test(line)
      && !/(?:\+|00)\d/.test(line)
    ));
    push("person_vorname", names[0], "high", "RocketReach authenticated person profile", sourceUrl.href);
    push("person_nachname", names.slice(1).join(" "), "high", "RocketReach authenticated person profile", sourceUrl.href);
    push("person_position", position, "medium", "RocketReach current company context", sourceUrl.href);
    push("person_email", email, "high", "RocketReach authenticated contact field", sourceUrl.href);
    push("person_telefon", phone, "high", "RocketReach authenticated contact field", sourceUrl.href);
  }
  const protectedFieldCount = records.filter((record) => record.field.startsWith("person_")).length;
  return { records, companyMatched: true, protectedFieldCount };
};"#;

const ROCKETREACH_BROWSER_CAPTURE_TEMPLATE: &str = r#"const company = __COMPANY_JSON__;
const country = __COUNTRY_JSON__;
__RECORD_PARSER__
const hostAllowed = (raw) => {
  try {
    const host = new URL(raw).hostname.toLowerCase();
    return host === "rocketreach.com" || host.endsWith(".rocketreach.com")
      || host === "rocketreach.co" || host.endsWith(".rocketreach.co");
  } catch { return false; }
};
const currentUrl = page.url();
if (!hostAllowed(currentUrl)) return { status: "wrong_origin", source_url: currentUrl, country, records: [] };
if (/\/(?:login|signin|auth)(?:\/|$|\?)/i.test(new URL(currentUrl).pathname)) {
  return { status: "auth_required", source_url: currentUrl, country, records: [] };
}
const snapshotPage = async () => page.evaluate(() => {
  const clean = (value, max = 500) => String(value || "").replace(/\s+/g, " ").trim().slice(0, max);
  const contextLines = (node) => {
    const container = node.closest("article, li, tr, [role='row'], [data-testid*='result'], [class*='result'], [class*='card']") || node.parentElement;
    return String(container?.innerText || "").split(/\n+/).map((line) => clean(line)).filter(Boolean).slice(0, 24);
  };
  const links = Array.from(document.querySelectorAll("a[href]"))
    .map((link) => ({ url: link.href, text: clean(link.innerText || link.textContent), contextLines: contextLines(link) }))
    .filter((link) => link.text || link.contextLines.length > 0)
    .slice(0, 800);
  const headings = Array.from(document.querySelectorAll("h1, h2, [role='heading']"))
    .map((node) => clean(node.innerText || node.textContent)).filter(Boolean).slice(0, 40);
  const bodyLines = String(document.body?.innerText || "").split(/\n+/)
    .map((line) => clean(line)).filter(Boolean).slice(0, 500);
  const embeddedPeople = [];
  const seen = new Set();
  const pick = (object, keys) => {
    for (const key of keys) {
      const value = object?.[key];
      if (typeof value === "string" && clean(value)) return clean(value);
    }
    return "";
  };
  const visit = (value, depth = 0) => {
    if (!value || depth > 9 || embeddedPeople.length >= 100) return;
    if (Array.isArray(value)) {
      for (const entry of value.slice(0, 500)) visit(entry, depth + 1);
      return;
    }
    if (typeof value !== "object") return;
    const first = pick(value, ["firstName", "first_name", "first"]);
    const last = pick(value, ["lastName", "last_name", "last"]);
    const name = pick(value, ["fullName", "full_name", "displayName", "name"]);
    const companyValue = value.company && typeof value.company === "object"
      ? pick(value.company, ["name", "companyName", "company_name"])
      : pick(value, ["companyName", "company_name", "currentCompany", "current_company"]);
    const sourceUrl = pick(value, ["profileUrl", "profile_url", "url", "canonicalUrl", "canonical_url"]);
    const email = pick(value, ["email", "workEmail", "work_email", "professionalEmail", "professional_email"]);
    const phone = pick(value, ["phone", "phoneNumber", "phone_number", "mobilePhone", "mobile_phone"]);
    const position = pick(value, ["title", "jobTitle", "job_title", "position", "currentTitle", "current_title"]);
    if ((first && last) || name) {
      const key = `${first}|${last}|${name}|${sourceUrl}|${companyValue}`;
      if (!seen.has(key)) {
        seen.add(key);
        embeddedPeople.push({ first, last, name: name || `${first} ${last}`, company: companyValue, sourceUrl, email, phone, position });
      }
    }
    for (const entry of Object.values(value).slice(0, 200)) visit(entry, depth + 1);
  };
  for (const script of Array.from(document.querySelectorAll("script[type='application/json'], script#__NEXT_DATA__")).slice(0, 30)) {
    const text = script.textContent || "";
    if (!text || text.length > 3000000) continue;
    try { visit(JSON.parse(text)); } catch {}
  }
  return { sourceUrl: location.href, title: document.title, links, headings, bodyLines, embeddedPeople };
});
const waitForResults = async () => {
  await Promise.race([
    page.waitForLoadState("networkidle", { timeout: 12000 }).catch(() => null),
    page.locator('a[href*="-profile_"], a[href*="/company/"], a[href*="-email_"]').first()
      .waitFor({ state: "visible", timeout: 12000 }).catch(() => null),
  ]).catch(() => null);
};
const snapshots = [];
const searchSelectors = [
  'input[placeholder*="search" i]', 'input[aria-label*="search" i]',
  'input[placeholder*="company" i]', 'input[type="search"]',
];
for (const selector of searchSelectors) {
  const field = page.locator(selector).first();
  if ((await field.count()) < 1 || !(await field.isVisible().catch(() => false))
      || !(await field.isEditable().catch(() => false))) continue;
  try {
    await field.fill(company, { timeout: 3000 });
    await field.press("Enter", { timeout: 3000 });
    await waitForResults();
    break;
  } catch {}
}
snapshots.push(await snapshotPage());
let parsed = parseRocketReachRecords(company, snapshots);
if (!parsed.companyMatched) {
  const queryUrl = `https://rocketreach.co/search?query=${encodeURIComponent(company)}`;
  await ctoxBrowser.goto(queryUrl, { timeoutMs: 30000 });
  await waitForResults();
  if (!hostAllowed(page.url())) return { status: "wrong_origin", source_url: page.url(), country, records: [] };
  snapshots.push(await snapshotPage());
  parsed = parseRocketReachRecords(company, snapshots);
}
const companyLink = snapshots.flatMap((snapshot) => snapshot.links || []).find((link) =>
  hostAllowed(link.url) && /(?:\/company\/|\/companies\/|-profile_[a-z0-9])/i.test(new URL(link.url).pathname)
  && parseRocketReachRecords(company, [{ ...snapshot, links: [link] }]).companyMatched
);
if (companyLink && page.url() !== companyLink.url) {
  await ctoxBrowser.goto(companyLink.url, { timeoutMs: 30000 });
  await waitForResults();
  if (!hostAllowed(page.url())) return { status: "wrong_origin", source_url: page.url(), country, records: [] };
  snapshots.push(await snapshotPage());
}
const personUrls = [...new Set(snapshots.flatMap((snapshot) => snapshot.links || [])
  .map((link) => link.url)
  .filter((url) => hostAllowed(url) && /(?:\/person\/|\/people\/|-email_[a-z0-9])/i.test(new URL(url).pathname))
)].slice(0, 5);
for (const personUrl of personUrls) {
  await ctoxBrowser.goto(personUrl, { timeoutMs: 30000 });
  await waitForResults();
  if (!hostAllowed(page.url())) continue;
  snapshots.push(await snapshotPage());
}
parsed = parseRocketReachRecords(company, snapshots);
return {
  status: parsed.protectedFieldCount > 0 ? "succeeded" : (parsed.companyMatched ? "no_extractable_fields" : "no_match"),
  source_url: snapshots.at(-1)?.sourceUrl || page.url(),
  country,
  records: parsed.records,
};"#;

fn run_business_os_web_stack_auth_assist_signup(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        args.iter().any(|arg| arg == "--confirm-provisioning"),
        "auth-assist-signup requires --confirm-provisioning because it may create an account"
    );
    let source_id = flag_value(args, "--source-id")
        .context("usage: ctox business-os web-stack auth-assist-signup --source-id <id> --target-url <url> --credential-ref <ctox-secret://scope/name> --login-hint <hint> --confirm-provisioning [--task-id <id>] [--timeout-ms <n>] [--dir <path>] [--email-selector <selector>] [--credential-selector <selector>] [--confirm-credential-selector <selector>] [--submit-selector <selector>] [--verify-selector <selector>] [--accept-terms] [--terms-selector <selector>]")?;
    let target_url_override = flag_value(args, "--target-url")
        .context("auth-assist-signup requires --target-url <signup-url>")?;
    let credential_ref =
        optional_web_stack_credential_ref(flag_value(args, "--credential-ref"))?
            .context("auth-assist-signup requires --credential-ref <ctox-secret://scope/name>")?;
    let local_secret_ref = parse_local_ctox_secret_ref(&credential_ref)?;
    let login_hint = optional_web_stack_login_hint(flag_value(args, "--login-hint"))
        .context("auth-assist-signup requires --login-hint <account-email-or-username>")?;
    let requesting_task_id = flag_value(args, "--task-id").unwrap_or_default();
    let owner_user_id =
        resolve_web_stack_auth_owner_user_id(root, args, requesting_task_id, false)?;
    let timeout_ms = flag_value(args, "--timeout-ms")
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("failed to parse --timeout-ms `{value}`"))
        })
        .transpose()?
        .unwrap_or(60_000)
        .clamp(1_000, 300_000);
    let browser_dir = flag_value(args, "--dir").map(PathBuf::from);
    let auth_assist = enqueue_web_stack_auth_assist_request(
        root,
        source_id,
        Some(target_url_override),
        Some(&credential_ref),
        Some(login_hint.as_str()),
        None,
        requesting_task_id,
        "ctox_web_auth_assist_signup",
        "ctox_web_auth_assist_signup",
        owner_user_id.as_deref(),
        false,
        // Trusted-local for the same reason as auth-assist-login: the daemon
        // itself files this request and owns the intake decision.
        true,
    )?;
    let session_id = auth_assist
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .context("auth-assist signup did not produce a session_id")?
        .to_string();
    let target_url = auth_assist
        .get("target_url")
        .and_then(serde_json::Value::as_str)
        .context("auth-assist signup did not produce a target_url")?
        .to_string();
    let source_id = auth_assist
        .get("source_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(source_id)
        .to_string();
    let secret_value =
        crate::secrets::read_secret_value(root, &local_secret_ref.scope, &local_secret_ref.name)
            .with_context(|| {
                format!(
            "failed to resolve credential_ref {credential_ref}; expected ctox secret scope/name"
        )
            })?;
    anyhow::ensure!(
        !secret_value.trim().is_empty(),
        "credential_ref {credential_ref} resolved to an empty secret"
    );
    let email_selector = flag_value(args, "--email-selector").unwrap_or_default();
    let credential_selector = flag_value(args, "--credential-selector").unwrap_or_default();
    let confirm_credential_selector =
        flag_value(args, "--confirm-credential-selector").unwrap_or_default();
    let submit_selector = flag_value(args, "--submit-selector").unwrap_or_default();
    let verify_selector = flag_value(args, "--verify-selector").unwrap_or_default();
    let terms_selector = flag_value(args, "--terms-selector").unwrap_or_default();
    let display_name = flag_value(args, "--display-name").unwrap_or_default();
    let display_name_selector = flag_value(args, "--display-name-selector").unwrap_or_default();
    let tenant_name = flag_value(args, "--tenant-name").unwrap_or_default();
    let tenant_name_selector = flag_value(args, "--tenant-name-selector").unwrap_or_default();
    let accept_terms = args.iter().any(|arg| arg == "--accept-terms");
    let automation_source = build_web_stack_auth_assist_signup_source(
        &target_url,
        &source_id,
        &login_hint,
        &credential_ref,
        &secret_value,
        email_selector.trim(),
        credential_selector.trim(),
        confirm_credential_selector.trim(),
        submit_selector.trim(),
        verify_selector.trim(),
        accept_terms,
        terms_selector.trim(),
        display_name.trim(),
        display_name_selector.trim(),
        tenant_name.trim(),
        tenant_name_selector.trim(),
    )?;
    let mut automation = crate::business_os::run_browser_session_automation(
        root,
        crate::business_os::BrowserSessionAutomationRequest {
            session_id: session_id.clone(),
            dir: browser_dir,
            timeout_ms: Some(timeout_ms),
            source: automation_source,
            profile_owner: auth_assist
                .get("owner_user_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        },
    )?;
    redact_secret_value_from_json(&mut automation, &secret_value);
    let signup_result = automation
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let automation_ok = automation
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let signup_ok = signup_result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(automation_ok);
    let signup_state = signup_result
        .get("signup_state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if automation_ok && signup_ok {
            "provisioned"
        } else {
            "signup_failed"
        });
    let status = if automation_ok && signup_ok {
        "completed"
    } else {
        match signup_state {
            "already_registered" => "already_registered",
            "verification_required" => "verification_required",
            "signup_error" => "signup_failed",
            _ => "signup_failed",
        }
    };
    Ok(serde_json::json!({
        "ok": automation_ok && signup_ok,
        "status": status,
        "command": "business-os web-stack auth-assist-signup",
        "session_id": session_id,
        "source_id": source_id,
        "target_url": target_url,
        "credential_ref": credential_ref,
        "login_hint": login_hint,
        "signup_state": signup_state,
        "verification_required": signup_result.get("verification_required").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "signup_error_detected": signup_result.get("signup_error_detected").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "already_registered": signup_result.get("already_registered").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "verify_selector": verify_selector,
        "credential_selector": credential_selector,
        "secret_value_in_payload": false,
        "frame_data_in_payload": false,
        "browser_stream": "rxdb",
        "timeout_ms": timeout_ms,
        "provisioning_confirmed": true,
        "auth_assist_request": auth_assist,
        "signup_result": signup_result,
        "automation": automation,
    }))
}

pub(crate) fn run_business_os_web_stack_context_capture(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    let session_id = flag_value(args, "--session-id").context(
        "usage: ctox business-os web-stack context-capture --session-id <id> [--source-id <id>] [--task-id <id>] [--no-handoff]",
    )?;
    crate::business_os::browser_context_capture(
        root,
        crate::business_os::BrowserContextCaptureRequest {
            session_id: session_id.to_string(),
            source_id: flag_value(args, "--source-id").map(str::to_string),
            requesting_task_id: flag_value(args, "--task-id").map(str::to_string),
            enqueue_handoff: !args.iter().any(|arg| arg == "--no-handoff"),
        },
    )
}

pub(crate) fn run_business_os_web_stack_context_extract(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    let session_id = flag_value(args, "--session-id").context(
        "usage: ctox business-os web-stack context-extract --session-id <id> [--source-id <id>] [--capture-script <id>] [--task-id <id>]",
    )?;
    let snapshot = crate::business_os::browser_context_capture(
        root,
        crate::business_os::BrowserContextCaptureRequest {
            session_id: session_id.to_string(),
            source_id: flag_value(args, "--source-id").map(str::to_string),
            requesting_task_id: flag_value(args, "--task-id").map(str::to_string),
            enqueue_handoff: false,
        },
    )?;
    let context = snapshot
        .get("browser_context")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let source_id = flag_value(args, "--source-id")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            context
                .get("source_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .context("context-extract requires --source-id or a session payload source_id")?;
    let module = ctox_web_stack::sources::find(&source_id)
        .with_context(|| format!("unknown web-stack source: {source_id}"))?;
    let recipe = module.browser_recipe();
    let capture_script = flag_value(args, "--capture-script")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            context
                .get("capture_script")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            recipe
                .as_ref()
                .and_then(|recipe| recipe.capture_script)
                .map(str::to_string)
        })
        .with_context(|| {
            format!(
                "web-stack source `{}` has no browser capture script",
                module.id()
            )
        })?;
    let now = now_ms();
    let command_id = format!("browser_extract_harness_{}_{}", now, Uuid::new_v4());
    let frame_id = context
        .get("frame_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let requesting_task_id = flag_value(args, "--task-id").unwrap_or_default();
    let document = serde_json::json!({
        "id": command_id.clone(),
        "command_id": command_id.clone(),
        "module": "browser",
        "command_type": "browser.capture.extract",
        "type": "browser.capture.extract",
        "record_id": session_id,
        "inbound_channel": "ctox_harness",
        "status": "pending_sync",
        "payload": {
            "session_id": session_id,
            "source_id": source_id,
            "capture_script": capture_script,
            "frame_id": frame_id,
            "browser_context_artifact": {
                "kind": "browser_context",
                "schema_version": 1,
                "stream": "rxdb",
                "source_module": "web_stack",
                "source_id": source_id,
                "capture_script": capture_script,
                "browser_context": context,
                "secret_value_in_payload": false,
                "frame_data_in_payload": false
            },
            "requesting_task_id": requesting_task_id,
            "secret_value_in_payload": false,
            "frame_data_in_payload": false
        },
        "client_context": {
            "source": "ctox-harness.browser-context-extract",
            "source_module": "ctox_harness",
            "command_path": "ctox_browser_context_extract",
            "actor": {
                "user_id": "ctox-harness",
                "display_name": "CTOX Harness",
                "role": "admin",
                "is_admin": true
            }
        },
        "created_at_ms": now,
        "updated_at_ms": now
    });
    let stored = crate::business_os::enqueue_business_command_document(root, document)?;
    Ok(serde_json::json!({
        "ok": true,
        "command_id": stored.get("command_id").and_then(serde_json::Value::as_str).unwrap_or(command_id.as_str()),
        "session_id": session_id,
        "source_id": source_id,
        "capture_script": capture_script,
        "frame_id": frame_id,
        "browser_stream": "rxdb",
        "secret_value_in_payload": false,
        "frame_data_in_payload": false,
        "status": "pending_sync"
    }))
}

pub(crate) fn run_business_os_web_stack_cli_json_local(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    match args.first().map(String::as_str) {
        Some("person-research") => run_business_os_web_stack_person_research(root, args),
        Some("auth-assist-request") => run_business_os_web_stack_auth_assist_request(root, args),
        Some("auth-assist-signup") => run_business_os_web_stack_auth_assist_signup(root, args),
        Some("auth-assist-login") => run_business_os_web_stack_auth_assist_login(root, args),
        Some("source-capture") => run_business_os_web_stack_source_capture(root, args),
        Some("authenticated-automation") => {
            let mut source = String::new();
            std::io::stdin()
                .read_to_string(&mut source)
                .context("failed to read authenticated-automation source from stdin")?;
            run_business_os_web_stack_authenticated_automation(root, args, &source)
        }
        Some("auth-assist-status") => {
            let session_id = flag_value(args, "--session-id").context(
                "usage: ctox business-os web-stack auth-assist-status --session-id <id>",
            )?;
            crate::business_os::browser_session_status(root, session_id)
        }
        Some("context-capture") => run_business_os_web_stack_context_capture(root, args),
        Some("context-extract") => run_business_os_web_stack_context_extract(root, args),
        Some("redaction-audit") => run_web_stack_redaction_audit(root, args),
        Some("browser-doctor") => {
            let report = ctox_web_stack::browser_doctor_report(
                root,
                flag_value(args, "--dir").map(PathBuf::from),
            )?;
            Ok(serde_json::json!({
                "ok": report
                    .get("automation_ready")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                "browser_stream": "rxdb",
                "secret_value_in_payload": false,
                "frame_data_in_payload": false,
                "doctor": report,
            }))
        }
        Some(other) => anyhow::bail!("unknown business-os web-stack command `{other}`"),
        None => anyhow::bail!("usage: ctox business-os web-stack <command>"),
    }
}

pub(crate) fn run_business_os_web_stack_cli_json(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    if let Some(payload) = crate::service::run_business_os_web_stack_via_service(root, args)? {
        return Ok(payload);
    }
    run_business_os_web_stack_cli_json_local(root, args)
}

fn run_business_os_web_stack_person_research(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    let company = flag_value(args, "--company")
        .context("usage: ctox business-os web-stack person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--auto-auth-assist] [--task-id <id>] [--workspace <path>] [--no-workspace]")?;
    let country_raw = flag_value(args, "--country")
        .context("business-os web-stack person-research requires --country <DE|AT|CH>")?;
    let country = ctox_web_stack::sources::Country::from_iso(country_raw)
        .with_context(|| format!("unsupported --country `{country_raw}`"))?;
    let mode_raw = flag_value(args, "--mode").context(
        "business-os web-stack person-research requires --mode <new_record|update_firm|update_person|update_inventory_general|have_data>",
    )?;
    let mode = ctox_web_stack::sources::ResearchMode::from_str(mode_raw)
        .with_context(|| format!("unsupported --mode `{mode_raw}`"))?;
    let fields = flag_values(args, "--field")
        .into_iter()
        .filter_map(ctox_web_stack::sources::FieldKey::from_str)
        .collect::<Vec<_>>();
    let include_private = flag_values(args, "--include-private")
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let workspace = business_os_web_stack_workspace(root, args);
    let persist_workspace = !args.iter().any(|arg| arg == "--no-workspace");
    let request = ctox_web_stack::PersonResearchRequest {
        company: company.to_string(),
        country,
        mode,
        fields,
        include_private,
        person_priorities: Vec::new(),
        known_person_records: Vec::new(),
        workspace,
        persist_workspace,
    };
    let mut payload = ctox_web_stack::run_ctox_person_research_tool(root, &request)?;
    if args.iter().any(|arg| arg == "--auto-auth-assist") {
        let requesting_task_id = flag_value(args, "--task-id")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context(
                "person-research --auto-auth-assist requires --task-id <research-command-id>",
            )?;
        let owner_user_id =
            resolve_web_stack_auth_owner_user_id(root, args, requesting_task_id, false)?.context(
                "person-research --auto-auth-assist requires a verified owner matching --task-id",
            )?;
        let mut capture_summaries = Vec::new();
        let mut capture_outcomes = BTreeMap::new();
        let mut remaining_tasks = Vec::new();
        let tasks = authenticated_person_research_capture_tasks(&payload);
        for task in &tasks {
            let Some(source_id) = task.get("source_id").and_then(serde_json::Value::as_str) else {
                remaining_tasks.push(task.clone());
                continue;
            };
            if let Some(capture_succeeded) = capture_outcomes.get(source_id) {
                if !capture_succeeded {
                    remaining_tasks.push(task.clone());
                }
                continue;
            }
            if !web_stack_authenticated_source_capture_supported(source_id) {
                capture_outcomes.insert(source_id.to_string(), false);
                remaining_tasks.push(task.clone());
                continue;
            }
            let capture_args = vec![
                "source-capture".to_string(),
                "--source-id".to_string(),
                source_id.to_string(),
                "--company".to_string(),
                company.to_string(),
                "--country".to_string(),
                country.as_iso().to_string(),
                "--task-id".to_string(),
                requesting_task_id.to_string(),
                "--owner-user-id".to_string(),
                owner_user_id.clone(),
                "--timeout-ms".to_string(),
                "180000".to_string(),
            ];
            let capture_succeeded =
                match run_business_os_web_stack_source_capture(root, &capture_args) {
                    Ok(capture) => {
                        match merge_authenticated_person_research_capture(&mut payload, &capture) {
                            Ok(summary) => {
                                let succeeded = summary
                                    .get("ok")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(false);
                                capture_summaries.push(summary);
                                succeeded
                            }
                            Err(_) => {
                                capture_summaries
                                    .push(authenticated_capture_failure_summary(source_id));
                                false
                            }
                        }
                    }
                    Err(_) => {
                        capture_summaries.push(authenticated_capture_failure_summary(source_id));
                        false
                    }
                };
            capture_outcomes.insert(source_id.to_string(), capture_succeeded);
            if !capture_succeeded {
                remaining_tasks.push(task.clone());
            }
        }
        payload["browser_assist_tasks"] = serde_json::Value::Array(remaining_tasks.clone());
        payload["authenticated_source_captures"] = serde_json::Value::Array(capture_summaries);

        let mut commands = Vec::new();
        let mut queued_sources = BTreeSet::new();
        for task in remaining_tasks {
            let Some(source_id) = task.get("source_id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !queued_sources.insert(source_id.to_string()) {
                continue;
            }
            let command = enqueue_web_stack_auth_assist_request(
                root,
                source_id,
                None,
                None,
                None,
                None,
                requesting_task_id,
                "ctox_web_stack",
                "ctox_business_os_web_stack_person_research",
                Some(owner_user_id.as_str()),
                true,
                true,
            )?;
            commands.push(command);
        }
        payload["auth_assist_commands"] = serde_json::Value::Array(commands);
        payload["auto_auth_assist"] = serde_json::json!({
            "enabled": true,
            "stream": "rxdb",
            "secret_value_in_payload": false,
            "command_count": payload
                .get("auth_assist_commands")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
        });
        repersist_augmented_person_research(&request, &mut payload);
    }
    Ok(payload)
}

fn web_stack_authenticated_source_capture_supported(source_id: &str) -> bool {
    matches!(
        source_id.trim().to_ascii_lowercase().as_str(),
        "dnbhoovers.com" | "leadfeeder.com" | "rocketreach.com" | "xing.com"
    )
}

pub(crate) fn authenticated_person_research_capture_tasks(
    payload: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut tasks = Vec::new();
    let mut sources = BTreeSet::new();
    for task in payload
        .get("browser_assist_tasks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(source_id) = task.get("source_id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !web_stack_authenticated_source_capture_supported(source_id) {
            continue;
        }
        if sources.insert(source_id.to_ascii_lowercase()) {
            tasks.push(task.clone());
        }
    }
    for recommendation in payload
        .get("browser_assist_recommendations")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(source_id) = recommendation
            .get("source_id")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        if !web_stack_authenticated_source_capture_supported(source_id)
            || !person_research_recommendation_has_missing_targets(payload, recommendation)
            || !sources.insert(source_id.to_ascii_lowercase())
        {
            continue;
        }
        tasks.push(serde_json::json!({
            "source_id": source_id,
            "target_fields": recommendation
                .get("target_fields")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
            "reason": "missing_target_fields_after_public_research",
            "status": "auth_assist_required",
            "stream": "rxdb",
            "browser_assist": recommendation,
            "next_command": format!("ctox business-os web-stack auth-assist-request --source-id {source_id}"),
            "secret_value_in_payload": false,
            "frame_data_in_payload": false,
        }));
    }
    for plan in payload
        .get("plan")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(source_id) = plan.get("source_id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !web_stack_authenticated_source_capture_supported(source_id)
            || !person_research_recommendation_has_missing_targets(payload, plan)
            || !sources.insert(source_id.to_ascii_lowercase())
        {
            continue;
        }
        tasks.push(serde_json::json!({
            "source_id": source_id,
            "target_fields": plan
                .get("target_fields")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
            "reason": "missing_target_fields_after_public_research",
            "status": "auth_assist_required",
            "stream": "rxdb",
            "browser_assist": serde_json::Value::Null,
            "next_command": format!("ctox business-os web-stack auth-assist-request --source-id {source_id}"),
            "secret_value_in_payload": false,
            "frame_data_in_payload": false,
        }));
    }
    tasks
}

fn person_research_recommendation_has_missing_targets(
    payload: &serde_json::Value,
    recommendation: &serde_json::Value,
) -> bool {
    recommendation
        .get("target_fields")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .any(|field| {
            payload
                .pointer(&format!("/fields/{field}/value"))
                .is_none_or(|value| {
                    value.is_null() || value.as_str().is_some_and(|value| value.trim().is_empty())
                })
        })
}

pub(crate) fn merge_authenticated_person_research_capture(
    payload: &mut serde_json::Value,
    capture: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let source_id = capture
        .get("source_id")
        .and_then(serde_json::Value::as_str)
        .context("authenticated source capture has no source_id")?;
    let capture_ok = capture
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let records = capture
        .get("records")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let merged_count = if capture_ok {
        ctox_web_stack::merge_person_research_source_records(payload, source_id, &records)?
    } else {
        0
    };
    Ok(serde_json::json!({
        "ok": capture_ok,
        "source_id": source_id,
        "source_status": capture.get("source_status").and_then(serde_json::Value::as_str).unwrap_or("failed"),
        "record_count": records.len(),
        "merged_count": merged_count,
        "session_id": capture.get("session_id").cloned().unwrap_or(serde_json::Value::Null),
        "source_url": capture.get("source_url").cloned().unwrap_or(serde_json::Value::Null),
        "secret_value_in_payload": false,
        "browser_stream": "rxdb",
    }))
}

fn authenticated_capture_failure_summary(source_id: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "source_id": source_id,
        "source_status": "capture_failed",
        "record_count": 0,
        "merged_count": 0,
        "secret_value_in_payload": false,
        "browser_stream": "rxdb",
    })
}

pub(crate) fn repersist_augmented_person_research(
    request: &ctox_web_stack::PersonResearchRequest,
    payload: &mut serde_json::Value,
) {
    if !request.persist_workspace {
        return;
    }
    let workspace = request.workspace.clone().or_else(|| {
        payload
            .pointer("/workspace/path")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from)
    });
    let Some(workspace) = workspace else {
        payload["workspace_error"] =
            serde_json::Value::String("person-research workspace path is unavailable".to_string());
        return;
    };
    match ctox_web_stack::persist_person_research_workspace(&workspace, request, payload) {
        Ok(summary) => {
            payload["workspace"] = summary;
            if let Some(object) = payload.as_object_mut() {
                object.remove("workspace_error");
            }
        }
        Err(error) => {
            payload["workspace_error"] = serde_json::Value::String(error.to_string());
        }
    }
}

fn business_os_web_stack_workspace(root: &Path, args: &[String]) -> Option<PathBuf> {
    flag_value(args, "--workspace")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
}

fn handle_business_os_web_stack(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("person-research") => {
            let payload = run_business_os_web_stack_cli_json(root, args)?;
            print_json(&payload)
        }
        Some("auth-assist-request") => {
            let summary = run_business_os_web_stack_auth_assist_request(root, args)?;
            print_json(&summary)
        }
        Some("auth-assist-signup") => {
            let signup = run_business_os_web_stack_cli_json(root, args)?;
            print_json(&signup)
        }
        Some("auth-assist-login") => {
            let login = run_business_os_web_stack_cli_json(root, args)?;
            print_json(&login)
        }
        Some("source-capture") => {
            let capture = run_business_os_web_stack_cli_json(root, args)?;
            print_json(&capture)
        }
        Some("authenticated-automation") => {
            let mut source = String::new();
            std::io::stdin()
                .read_to_string(&mut source)
                .context("failed to read authenticated-automation source from stdin")?;
            let output = run_business_os_web_stack_authenticated_automation(root, args, &source)?;
            print_json(&output)
        }
        Some("auth-assist-status") => {
            let session_id = flag_value(args, "--session-id").context(
                "usage: ctox business-os web-stack auth-assist-status --session-id <id>",
            )?;
            let status = crate::business_os::browser_session_status(root, session_id)?;
            print_json(&status)
        }
        Some("context-capture") => {
            let session_id = flag_value(args, "--session-id").context(
                "usage: ctox business-os web-stack context-capture --session-id <id> [--source-id <id>] [--task-id <id>] [--no-handoff]",
            )?;
            let capture = crate::business_os::browser_context_capture(
                root,
                crate::business_os::BrowserContextCaptureRequest {
                    session_id: session_id.to_string(),
                    source_id: flag_value(args, "--source-id").map(str::to_string),
                    requesting_task_id: flag_value(args, "--task-id").map(str::to_string),
                    enqueue_handoff: !args.iter().any(|arg| arg == "--no-handoff"),
                },
            )?;
            print_json(&capture)
        }
        Some("context-extract") => {
            let session_id = flag_value(args, "--session-id").context(
                "usage: ctox business-os web-stack context-extract --session-id <id> [--source-id <id>] [--capture-script <id>] [--task-id <id>]",
            )?;
            let snapshot = crate::business_os::browser_context_capture(
                root,
                crate::business_os::BrowserContextCaptureRequest {
                    session_id: session_id.to_string(),
                    source_id: flag_value(args, "--source-id").map(str::to_string),
                    requesting_task_id: flag_value(args, "--task-id").map(str::to_string),
                    enqueue_handoff: false,
                },
            )?;
            let context = snapshot
                .get("browser_context")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let source_id = flag_value(args, "--source-id")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    context
                        .get("source_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
                .context("context-extract requires --source-id or a session payload source_id")?;
            let module = ctox_web_stack::sources::find(&source_id)
                .with_context(|| format!("unknown web-stack source: {source_id}"))?;
            let recipe = module.browser_recipe();
            let capture_script = flag_value(args, "--capture-script")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    context
                        .get("capture_script")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
                .or_else(|| {
                    recipe
                        .as_ref()
                        .and_then(|recipe| recipe.capture_script)
                        .map(str::to_string)
                })
                .with_context(|| {
                    format!(
                        "web-stack source `{}` has no browser capture script",
                        module.id()
                    )
                })?;
            let now = now_ms();
            let command_id = format!("browser_extract_harness_{}_{}", now, Uuid::new_v4());
            let frame_id = context
                .get("frame_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let requesting_task_id = flag_value(args, "--task-id").unwrap_or_default();
            let document = serde_json::json!({
                "id": command_id.clone(),
                "command_id": command_id.clone(),
                "module": "browser",
                "command_type": "browser.capture.extract",
                "type": "browser.capture.extract",
                "record_id": session_id,
                "inbound_channel": "ctox_harness",
                "status": "pending_sync",
                "payload": {
                    "session_id": session_id,
                    "source_id": source_id,
                    "capture_script": capture_script,
                    "frame_id": frame_id,
                    "browser_context_artifact": {
                        "kind": "browser_context",
                        "schema_version": 1,
                        "stream": "rxdb",
                        "source_module": "web_stack",
                        "source_id": source_id,
                        "capture_script": capture_script,
                        "browser_context": context,
                        "secret_value_in_payload": false,
                        "frame_data_in_payload": false
                    },
                    "requesting_task_id": requesting_task_id,
                    "secret_value_in_payload": false,
                    "frame_data_in_payload": false
                },
                "client_context": {
                    "source": "ctox-harness.browser-context-extract",
                    "source_module": "ctox_harness",
                    "command_path": "ctox_browser_context_extract",
                    "actor": {
                        "user_id": "ctox-harness",
                        "display_name": "CTOX Harness",
                        "role": "admin",
                        "is_admin": true
                    }
                },
                "created_at_ms": now,
                "updated_at_ms": now
            });
            let stored = crate::business_os::enqueue_business_command_document(root, document)?;
            print_json(&serde_json::json!({
                "ok": true,
                "command_id": stored.get("command_id").and_then(serde_json::Value::as_str).unwrap_or(command_id.as_str()),
                "session_id": session_id,
                "source_id": source_id,
                "capture_script": capture_script,
                "frame_id": frame_id,
                "browser_stream": "rxdb",
                "secret_value_in_payload": false,
                "frame_data_in_payload": false,
                "status": "pending_sync"
            }))
        }
        Some("redaction-audit") => {
            let audit = run_web_stack_redaction_audit(root, args)?;
            print_json(&audit)
        }
        Some("browser-doctor") => {
            let report = ctox_web_stack::browser_doctor_report(
                root,
                flag_value(args, "--dir").map(PathBuf::from),
            )?;
            print_json(&serde_json::json!({
                "ok": report
                    .get("automation_ready")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                "browser_stream": "rxdb",
                "secret_value_in_payload": false,
                "frame_data_in_payload": false,
                "doctor": report,
            }))
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os web-stack command `{other}`"),
    }
}

fn handle_business_os_files(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("sync") => {
            let path = args
                .get(1)
                .context("usage: ctox business-os files sync <path>")?;
            crate::business_os::sync_desktop_file_from_path(root, Path::new(path))?;
            print_json(&serde_json::json!({
                "ok": true,
                "path": path,
            }))
        }
        Some("sync-workspace") => {
            let path = args
                .get(1)
                .context("usage: ctox business-os files sync-workspace <path>")?;
            let indexed =
                crate::business_os::sync_desktop_files_from_workspace_root(root, Path::new(path))?;
            print_json(&serde_json::json!({
                "ok": true,
                "path": path,
                "indexed": indexed,
            }))
        }
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os files command `{other}`"),
    }
}

fn handle_business_os_modules(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        None | Some("list") => {
            let activation = load_activation(root)?;
            let modules = available_modules()
                .into_iter()
                .map(|(id, label, module_type)| {
                    let enabled = activation.enabled_modules.iter().any(|module| module == id);
                    let skills: Vec<_> = SKILL_APP_BINDINGS
                        .iter()
                        .filter(|binding| binding.module_id == id)
                        .map(|binding| binding.skill_id)
                        .collect();
                    serde_json::json!({
                        "id": id,
                        "label": label,
                        "type": module_type,
                        "enabled": enabled,
                        "skills": skills
                    })
                })
                .collect::<Vec<_>>();
            print_json(&serde_json::json!({
                "ok": true,
                "enabled_modules": activation.enabled_modules,
                "enabled_skills": activation.enabled_skills,
                "modules": modules
            }))
        }
        Some("enable") => {
            let module_id = args
                .get(1)
                .context("usage: ctox business-os modules enable <module>")?;
            if is_core_module(module_id) {
                return print_json(&serde_json::json!({
                    "ok": true,
                    "module": module_id,
                    "enabled": true,
                    "note": "core module is always enabled"
                }));
            }
            ensure_known_skill_module(module_id)?;
            let mut activation = load_activation(root)?;
            activation_add(&mut activation.enabled_modules, module_id);
            let installed = enable_module_skills(root, &mut activation, module_id)?;
            save_activation(root, &activation)?;
            print_json(&serde_json::json!({
                "ok": true,
                "module": module_id,
                "enabled_modules": activation.enabled_modules,
                "enabled_skills": activation.enabled_skills,
                "installed": installed
            }))
        }
        Some("disable") => {
            let module_id = args.get(1).context(
                "usage: ctox business-os modules disable <module> [--force-remove-skills]",
            )?;
            if is_core_module(module_id) {
                anyhow::bail!("core Business OS module cannot be disabled: {module_id}");
            }
            ensure_known_skill_module(module_id)?;
            let force = args.iter().any(|arg| arg == "--force-remove-skills");
            let mut activation = load_activation(root)?;
            activation
                .enabled_modules
                .retain(|module| module != module_id);
            let removed = disable_module_skills(root, &mut activation, module_id, force)?;
            save_activation(root, &activation)?;
            print_json(&serde_json::json!({
                "ok": true,
                "module": module_id,
                "enabled_modules": activation.enabled_modules,
                "enabled_skills": activation.enabled_skills,
                "removed": removed
            }))
        }
        Some("--help") | Some("-h") => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os modules command `{other}`"),
    }
}

fn handle_business_os_skills(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        None | Some("list") => {
            let activation = load_activation(root)?;
            let skills = SKILL_APP_BINDINGS
                .iter()
                .map(|binding| {
                    let state = skill_store::source_pack_install_state(root, binding.skill_id)?;
                    Ok(serde_json::json!({
                        "skill_id": binding.skill_id,
                        "pack": binding.pack,
                        "title": binding.title,
                        "module_id": binding.module_id,
                        "submodule_id": binding.submodule_id,
                        "enabled": activation.enabled_skills.iter().any(|skill| skill == binding.skill_id),
                        "installed": state.installed,
                        "managed": state.managed,
                        "path": state.path
                    }))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            print_json(&serde_json::json!({
                "ok": true,
                "enabled_modules": activation.enabled_modules,
                "enabled_skills": activation.enabled_skills,
                "skills": skills
            }))
        }
        Some("enable") => {
            let skill_id = args
                .get(1)
                .context("usage: ctox business-os skills enable <skill>")?;
            let binding = find_skill_binding(skill_id)?;
            let mut activation = load_activation(root)?;
            activation_add(&mut activation.enabled_modules, binding.module_id);
            activation_add(&mut activation.enabled_skills, binding.skill_id);
            let path = skill_store::install_source_pack(root, binding.skill_id)?;
            skill_store::bootstrap_from_roots(root)?;
            save_activation(root, &activation)?;
            print_json(&serde_json::json!({
                "ok": true,
                "skill": binding,
                "path": path,
                "enabled_modules": activation.enabled_modules,
                "enabled_skills": activation.enabled_skills
            }))
        }
        Some("disable") => {
            let skill_id = args
                .get(1)
                .context("usage: ctox business-os skills disable <skill> [--force-remove]")?;
            let binding = find_skill_binding(skill_id)?;
            let force = args.iter().any(|arg| arg == "--force-remove");
            let mut activation = load_activation(root)?;
            activation
                .enabled_skills
                .retain(|skill| skill != binding.skill_id);
            let result = skill_store::remove_installed_source_pack(root, binding.skill_id, force)?;
            save_activation(root, &activation)?;
            print_json(&serde_json::json!({
                "ok": true,
                "skill": binding,
                "disable": result,
                "enabled_modules": activation.enabled_modules,
                "enabled_skills": activation.enabled_skills
            }))
        }
        Some("--help") | Some("-h") => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os skills command `{other}`"),
    }
}

fn print_business_os_help() {
    println!("{}", business_os_usage());
    println!();
    println!("{}", business_os_status_text(Path::new(".")));
}

fn business_os_usage() -> String {
    business_os_usage_base()
        .replace(
            "  ctox business-os app validate <module-id> [--installed|--source] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]\n  ctox business-os app refresh-catalog",
            "  ctox business-os app references [--query <text>] [--limit <n>|--all] [--json]\n  ctox business-os app validate <module-id> [--installed|--source] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]\n  ctox business-os app refresh-catalog\n  ctox business-os app smoke <module-id> [--installed|--source] [--url <business-os-url>] [--json] [--timeout-ms <n>] [--output <path>] [--screenshot <path>]\n  ctox business-os app e2e <module-id> [--installed|--source] [--url <business-os-url>] [--json] [--timeout-ms <n>] [--output <path>] [--screenshot <path>] [--marker <value>] [--scenario <id>] [--require-scenario]\n  ctox business-os app audit <module-id> [--installed|--source] [--profile quick|release|full] [--url <business-os-url>] [--deployed-url <url> [--active --approval-id <id>]]",
        )
        .replace(
            "  ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]",
            "  ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]\n  ctox business-os app bench status --run-id <id> [--validate]",
        )
        .replace(
            "  ctox business-os peer rotate\n  ctox business-os peer start\n  ctox business-os desktop invite",
            "  ctox business-os peer rotate\n  ctox business-os peer rotate-room\n  ctox business-os peer rotate-native\n  ctox business-os peer start\n  ctox business-os auth issue-capability --user <user-id> [--display-name <name>] [--role chef|admin|founder|user] [--ensure-user]\n  ctox business-os desktop invite",
        )
        .replace(
            "  ctox business-os desktop invite [--display-name <name>] [--ttl-hours <n> | --expires-at <rfc3339>] [--format json|link] [--output <path>]",
            "  ctox business-os desktop invite [--display-name <name>] [--user <id>] [--user-id <id>] [--user-display-name <name>] [--role chef|admin|founder|user] [--ttl-hours <n> | --expires-at <rfc3339>] [--format json|link] [--output <path>]",
        )
        .replace(
            "  ctox business-os backup prune-drills [--dry-run]",
            "  ctox business-os backup inspect-manifest --manifest <path>\n  ctox business-os backup key-escrow-status\n  ctox business-os backup prune-drills [--dry-run]",
        )
        .replace(
            "  ctox business-os commands process <command-id>",
            "  ctox business-os commands process <command-id>\n  ctox business-os commands retry <command-id>",
        )
        .replace(
            "  ctox business-os commands dispatch (--input <path> | --json <json> | <json>)",
            "  ctox business-os commands dispatch (--input <path> | --json <json> | <json>)\n  ctox business-os commands diagnostics --json\n  ctox business-os commands inspect <command-id>\n  ctox business-os commands gc (--dry-run | --apply)\n  ctox business-os commands reconcile (--dry-run | --apply)\n  ctox business-os harness-bench catalog\n  ctox business-os harness-bench run (--dry-run | --confirm-live) [--run-id <id>] [--actor <user-id>] [--reviewer <user-id>] [--family <id>] [--case <H001>] [--limit <n>]\n  ctox business-os harness-bench status --run-id <id> [--fail-on-inflight]",
        )
        .replace(
            "  ctox business-os web-stack source-capture --source-id <dnbhoovers.com|leadfeeder.com|xing.com> --company <name> [--country <DE|AT|CH>] [--session-id <id>] [--timeout-ms <n>] [--dir <path>]",
            "  ctox business-os web-stack auth-assist-signup --source-id <id> --target-url <url> --credential-ref <ctox-secret://scope/name> --login-hint <hint> --confirm-provisioning [--task-id <id>] [--timeout-ms <n>] [--dir <path>]\n  ctox business-os web-stack auth-assist-login --source-id <id> --credential-ref <ctox-secret://scope/name> [--target-url <url>] [--login-hint <hint>] [--task-id <id>] [--timeout-ms <n>] [--dir <path>] [--credential-selector <selector>] [--verify-selector <selector>]\n  ctox business-os web-stack source-capture --source-id <dnbhoovers.com|leadfeeder.com|rocketreach.com|xing.com> --company <name> [--country <DE|AT|CH>] [--credential-ref <ctox-secret://scope/name>] [--login-hint <hint>] [--session-id <id>] [--timeout-ms <n>] [--dir <path>]\n  ctox business-os web-stack authenticated-automation --source-id <id> --target-url <url> --credential-ref <ctox-secret://scope/name> [--login-hint <hint>] [--task-id <id>] [--timeout-ms <n>] [--dir <path>] [--credential-selector <selector>] [--verify-selector <selector>] < automation.js",
        )
}

fn business_os_usage_base() -> &'static str {
    "usage:\n  ctox business-os status\n  ctox business-os serve [--addr 127.0.0.1:8765]\n  ctox business-os customer-apps audit\n  ctox business-os mcp status\n  ctox business-os mcp tools\n  ctox business-os mcp policy\n  ctox business-os mcp policy keys\n  ctox business-os mcp policy set [--enabled true|false] [--allow-reads true|false] [--allow-writes true|false] [--allow-approvals true|false] [--allow-external-effects true|false] [--rate-limit-per-minute <n>] [--audit-retention-days <n>] [--allow-actor <id>]... [--allow-workspace <id>]... [--allow-module <id>]... [--allow-collection <name>]... [--deny-tool business_os.<tool>]... [--clear-deny-tools]\n  ctox business-os mcp call <tool-name> [--args <json>]\n  ctox business-os mcp audit [--limit <n>] [--format json|jsonl] [--output <path>] [--prune]\n  ctox business-os mcp serve [--addr 127.0.0.1:8788]\n  ctox business-os mcp connect --url wss://mcp.ctox.dev/connect/<instance-id> [--token <token>] [--once] [--max-reconnect-delay-ms <n>] [--heartbeat-interval-ms <n>] [--max-connection-age-ms <n>]\n  ctox business-os mcp gateway-status --url https://mcp.ctox.dev/status/<instance-id> [--token <token>]\n  ctox business-os peer status\n  ctox business-os peer rotate\n  ctox business-os peer start\n  ctox business-os desktop invite [--display-name <name>] [--ttl-hours <n> | --expires-at <rfc3339>] [--format json|link] [--output <path>]\n  ctox business-os rxdb status [--json]\n  ctox business-os rxdb repair-optional-drift --collection <name> [--dry-run] [--force]\n  ctox business-os turn status\n  ctox business-os turn set [--url turns:host:5349] [--secret <coturn use-auth-secret>]\n  ctox business-os app create --instruction <text> [--module-id <id>]\n  ctox business-os app modify <module-id> --instruction <text>\n  ctox business-os app validate <module-id> [--installed|--source] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]\n  ctox business-os app refresh-catalog\n  ctox business-os app finalize <module-id> --task-id <queue-task-id> [--installed|--source] [--reason <text>]\n  ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]\n  ctox business-os repair queue-projections (--dry-run | --apply)\n  ctox business-os backup restore-drill [--module <module-id>]\n  ctox business-os backup prune-drills [--dry-run]\n  ctox business-os commands process <command-id>\n  ctox business-os commands dispatch (--input <path> | --json <json> | <json>)\n  ctox business-os web-stack person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--auto-auth-assist] [--task-id <id>] [--workspace <path>] [--no-workspace]\n  ctox business-os web-stack auth-assist-request --source-id <id> [--target-url <url>] [--task-id <id>]\n  ctox business-os web-stack source-capture --source-id <dnbhoovers.com|leadfeeder.com|xing.com> --company <name> [--country <DE|AT|CH>] [--session-id <id>] [--timeout-ms <n>] [--dir <path>]\n  ctox business-os web-stack auth-assist-status --session-id <id>\n  ctox business-os web-stack context-capture --session-id <id> [--source-id <id>] [--task-id <id>] [--no-handoff]\n  ctox business-os web-stack context-extract --session-id <id> [--source-id <id>] [--capture-script <id>] [--task-id <id>]\n  ctox business-os web-stack redaction-audit --canary <value> [--canary <value>]... [--path <path>]...\n  ctox business-os web-stack browser-doctor [--dir <path>]\n  ctox business-os files sync <path>\n  ctox business-os files sync-workspace <path>\n  ctox business-os modules list\n  ctox business-os modules enable <module>\n  ctox business-os modules disable <module> [--force-remove-skills]\n  ctox business-os skills list\n  ctox business-os skills enable <skill>\n  ctox business-os skills disable <skill> [--force-remove]"
}

fn exists_label(exists: bool) -> &'static str {
    if exists {
        "ok"
    } else {
        "missing"
    }
}

pub(super) fn existing_dir_path(root: &Path, candidates: &[&str]) -> PathBuf {
    candidates
        .iter()
        .map(|candidate| root.join(candidate))
        .find(|path| path.is_dir())
        .unwrap_or_else(|| root.join(candidates[0]))
}

fn app_instruction_arg(args: &[String], skip_first_positional: bool) -> Option<String> {
    flag_value(args, "--instruction")
        .or_else(|| flag_value(args, "--request"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            let mut skip_first = skip_first_positional;
            let mut free = Vec::new();
            let mut index = 0usize;
            while index < args.len() {
                let arg = &args[index];
                if arg.starts_with("--") {
                    if app_command_flag_has_value(arg.as_str()) {
                        index = index.saturating_add(2);
                    } else {
                        index = index.saturating_add(1);
                    }
                    continue;
                }
                if skip_first {
                    skip_first = false;
                    index = index.saturating_add(1);
                    continue;
                }
                free.push(arg.as_str());
                index = index.saturating_add(1);
            }
            let text = free.join(" ").trim().to_owned();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
}

fn app_command_flag_has_value(flag: &str) -> bool {
    matches!(
        flag,
        "--instruction"
            | "--request"
            | "--module-id"
            | "--app-id"
            | "--title"
            | "--description"
            | "--category"
            | "--version"
            | "--actor"
            | "--actor-user"
            | "--command-id"
    )
}

fn sanitize_business_os_app_module_id(value: &str) -> anyhow::Result<String> {
    let slug = value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    anyhow::ensure!(!slug.is_empty(), "module id is required");
    Ok(slug.chars().take(72).collect())
}

fn title_from_module_id(module_id: &str) -> String {
    let title = module_id
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() {
        "Business OS App".to_owned()
    } else {
        title
    }
}

fn normalize_business_os_app_version(value: &str) -> anyhow::Result<String> {
    let version = value.trim();
    anyhow::ensure!(
        version
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .is_ok_and(|parts| parts.len() == 3),
        "Business OS app version must use semver without a v prefix, for example 0.1.0"
    );
    Ok(version.to_owned())
}

fn business_os_app_cli_actor(args: &[String]) -> serde_json::Value {
    let id = flag_value(args, "--actor")
        .or_else(|| flag_value(args, "--actor-user"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            crate::business_os::store::session(None, None)
                .user
                .map(|user| user.id)
        })
        .unwrap_or_else(|| "local-dev".to_owned());
    serde_json::json!({
        "id": id,
        "display_name": id,
    })
}

pub(super) fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

pub(super) fn args_have_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

fn flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter_map(|window| {
            if window[0] == flag {
                Some(window[1].as_str())
            } else {
                None
            }
        })
        .collect()
}

/// Read a business command document for `commands dispatch` from one of:
/// `--input <path>` (JSON file), `--json <inline>`, or the first positional
/// argument as inline JSON. Always parsed locally; never fetched over a network.
fn read_command_document(args: &[String]) -> anyhow::Result<serde_json::Value> {
    let raw = if let Some(path) = flag_value(args, "--input") {
        std::fs::read_to_string(path)
            .with_context(|| format!("failed to read command document from {path}"))?
    } else if let Some(inline) = flag_value(args, "--json") {
        inline.to_string()
    } else if let Some(inline) = args.get(1).filter(|value| !value.starts_with("--")) {
        inline.to_string()
    } else {
        anyhow::bail!(
            "usage: ctox business-os commands dispatch (--input <path> | --json <json> | <json>)"
        );
    };
    serde_json::from_str(&raw).context("command document is not valid JSON")
}

fn apply_mcp_policy_bool_arg(args: &[String], flag: &str, target: &mut bool) -> anyhow::Result<()> {
    let Some(raw) = flag_value(args, flag) else {
        return Ok(());
    };
    *target = parse_cli_bool(raw).with_context(|| format!("invalid value for {flag}: {raw}"))?;
    Ok(())
}

fn apply_mcp_policy_usize_arg(
    args: &[String],
    flag: &str,
    target: &mut usize,
) -> anyhow::Result<()> {
    let Some(raw) = flag_value(args, flag) else {
        return Ok(());
    };
    *target = raw
        .trim()
        .parse::<usize>()
        .with_context(|| format!("invalid value for {flag}: {raw}"))?;
    Ok(())
}

fn apply_mcp_policy_values_arg(args: &[String], flag: &str, target: &mut Vec<String>) {
    let mut seen = target
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    target.retain(|value| !value.trim().is_empty());
    for value in flag_values(args, flag) {
        for item in value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if seen.insert(item.to_string()) {
                target.push(item.to_string());
            }
        }
    }
}

fn parse_cli_bool(raw: &str) -> anyhow::Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enabled" => Ok(true),
        "0" | "false" | "no" | "off" | "disabled" => Ok(false),
        _ => anyhow::bail!("expected one of true/false, yes/no, on/off, 1/0"),
    }
}

fn mcp_policy_deny_tools_from_args(args: &[String]) -> anyhow::Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut tools = Vec::new();
    for value in flag_values(args, "--deny-tool")
        .into_iter()
        .chain(flag_values(args, "--deny-tools"))
    {
        for tool in value
            .split(',')
            .map(str::trim)
            .filter(|tool| !tool.is_empty())
        {
            anyhow::ensure!(
                tool.starts_with("business_os."),
                "--deny-tool only accepts Business OS MCP tool names"
            );
            if seen.insert(tool.to_string()) {
                tools.push(tool.to_string());
            }
        }
    }
    Ok(tools)
}

fn mcp_policy_env_projection(env_map: &BTreeMap<String, String>) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for key in MCP_POLICY_KEYS {
        if let Some(value) = env_map.get(*key) {
            object.insert((*key).to_string(), serde_json::Value::String(value.clone()));
        }
    }
    serde_json::Value::Object(object)
}

fn mcp_policy_to_env_map(
    policy: &crate::business_os::mcp_channel::BusinessOsMcpPolicy,
) -> BTreeMap<String, String> {
    let mut env_map = BTreeMap::new();
    env_map.insert(
        "CTOX_BUSINESS_OS_MCP_ENABLED".to_string(),
        policy.enabled.to_string(),
    );
    env_map.insert(
        "CTOX_BUSINESS_OS_MCP_ALLOW_READS".to_string(),
        policy.allow_reads.to_string(),
    );
    env_map.insert(
        "CTOX_BUSINESS_OS_MCP_ALLOW_WRITES".to_string(),
        policy.allow_writes.to_string(),
    );
    env_map.insert(
        "CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS".to_string(),
        policy.allow_approvals.to_string(),
    );
    env_map.insert(
        "CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS".to_string(),
        policy.allow_external_effects.to_string(),
    );
    env_map.insert(
        "CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE".to_string(),
        policy.rate_limit_per_minute.to_string(),
    );
    env_map.insert(
        "CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS".to_string(),
        policy.audit_retention_days.to_string(),
    );
    insert_mcp_policy_csv_value(
        &mut env_map,
        "CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS",
        &policy.allowed_actors,
    );
    insert_mcp_policy_csv_value(
        &mut env_map,
        "CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES",
        &policy.allowed_workspaces,
    );
    insert_mcp_policy_csv_value(
        &mut env_map,
        "CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES",
        &policy.allowed_modules,
    );
    insert_mcp_policy_csv_value(
        &mut env_map,
        "CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS",
        &policy.allowed_collections,
    );
    insert_mcp_policy_csv_value(
        &mut env_map,
        "CTOX_BUSINESS_OS_MCP_DENY_TOOLS",
        &policy.denied_tools,
    );
    env_map
}

fn insert_mcp_policy_csv_value(
    env_map: &mut BTreeMap<String, String>,
    key: &str,
    values: &[String],
) {
    let values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !values.is_empty() {
        env_map.insert(key.to_string(), values.join(","));
    }
}

fn run_web_stack_redaction_audit(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    let canaries = flag_values(args, "--canary")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.as_bytes().to_vec())
        .collect::<Vec<_>>();
    anyhow::ensure!(
        !canaries.is_empty(),
        "usage: ctox business-os web-stack redaction-audit --canary <value> [--canary <value>]... [--path <path>]..."
    );
    let scan_paths = flag_values(args, "--path")
        .into_iter()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .collect::<Vec<_>>();
    let scan_paths = if scan_paths.is_empty() {
        default_web_stack_redaction_audit_paths(root)
    } else {
        scan_paths
    };
    let max_files = flag_value(args, "--max-files")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5_000);
    let max_file_bytes = flag_value(args, "--max-file-bytes")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(64 * 1024 * 1024);
    let mut state = RedactionAuditState {
        canaries: &canaries,
        max_files,
        max_file_bytes,
        scanned_files: 0,
        skipped_files: 0,
        truncated_files: 0,
        findings: Vec::new(),
    };
    for path in &scan_paths {
        scan_redaction_audit_path(path, &mut state)
            .with_context(|| format!("scan redaction audit path {}", path.display()))?;
    }
    Ok(serde_json::json!({
        "ok": state.findings.is_empty(),
        "scan": {
            "paths": scan_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
            "canary_count": canaries.len(),
            "scanned_files": state.scanned_files,
            "skipped_files": state.skipped_files,
            "truncated_files": state.truncated_files,
            "max_files": max_files,
            "max_file_bytes": max_file_bytes,
        },
        "findings": state.findings,
        "secret_value_in_payload": false,
        "frame_data_in_payload": false,
    }))
}

fn default_web_stack_redaction_audit_paths(root: &Path) -> Vec<PathBuf> {
    [
        "runtime/ctox.sqlite3",
        "runtime/ctox.sqlite3-wal",
        "runtime/ctox.sqlite3-shm",
        "runtime/browser",
        "runtime/logs",
        "artifacts",
        "reports",
    ]
    .into_iter()
    .map(|path| root.join(path))
    .filter(|path| path.exists())
    .collect()
}

struct RedactionAuditState<'a> {
    canaries: &'a [Vec<u8>],
    max_files: usize,
    max_file_bytes: u64,
    scanned_files: usize,
    skipped_files: usize,
    truncated_files: usize,
    findings: Vec<serde_json::Value>,
}

fn scan_redaction_audit_path(
    path: &Path,
    state: &mut RedactionAuditState<'_>,
) -> anyhow::Result<()> {
    if state.scanned_files >= state.max_files {
        state.skipped_files = state.skipped_files.saturating_add(1);
        return Ok(());
    }
    if !path.exists() || should_skip_redaction_audit_path(path) {
        state.skipped_files = state.skipped_files.saturating_add(1);
        return Ok(());
    }
    if path.is_dir() {
        let mut entries = fs::read_dir(path)
            .with_context(|| format!("read audit directory {}", path.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            scan_redaction_audit_path(&entry, state)?;
            if state.scanned_files >= state.max_files {
                break;
            }
        }
        return Ok(());
    }
    if !path.is_file() {
        state.skipped_files = state.skipped_files.saturating_add(1);
        return Ok(());
    }
    scan_redaction_audit_file(path, state)
}

fn scan_redaction_audit_file(
    path: &Path,
    state: &mut RedactionAuditState<'_>,
) -> anyhow::Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("stat audit file {}", path.display()))?;
    let mut file =
        fs::File::open(path).with_context(|| format!("open audit file {}", path.display()))?;
    let limit = metadata.len().min(state.max_file_bytes);
    let mut buf = Vec::with_capacity(limit.min(1024 * 1024) as usize);
    let mut handle = file.by_ref().take(limit);
    handle
        .read_to_end(&mut buf)
        .with_context(|| format!("read audit file {}", path.display()))?;
    state.scanned_files = state.scanned_files.saturating_add(1);
    let truncated = metadata.len() > limit;
    if truncated {
        state.truncated_files = state.truncated_files.saturating_add(1);
    }
    let mut matches = Vec::new();
    for (index, canary) in state.canaries.iter().enumerate() {
        if !canary.is_empty() && bytes_contains(&buf, canary) {
            matches.push(index);
        }
    }
    if !matches.is_empty() {
        state.findings.push(serde_json::json!({
            "path": path.display().to_string(),
            "canary_indexes": matches,
            "bytes_scanned": buf.len(),
            "truncated": truncated,
        }));
    }
    Ok(())
}

fn should_skip_redaction_audit_path(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        matches!(
            value.as_ref(),
            ".git" | "node_modules" | "target" | "cargo-target" | "core-rxdb-integration-target"
        )
    })
}

fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub(crate) fn enqueue_web_stack_auth_assist_request(
    root: &Path,
    source_id: &str,
    target_url_override: Option<&str>,
    credential_ref: Option<&str>,
    login_hint: Option<&str>,
    preferred_auth_assist: Option<&serde_json::Value>,
    requesting_task_id: &str,
    source_module: &str,
    command_path: &str,
    owner_user_id: Option<&str>,
    deterministic_for_task: bool,
    accept_as_trusted_local: bool,
) -> anyhow::Result<serde_json::Value> {
    let module = ctox_web_stack::sources::find(source_id);
    let recipe = module.and_then(|module| module.browser_recipe());
    let source_defaults = web_stack_auth_source_defaults(source_id);
    let target_url = target_url_override
        .map(str::to_string)
        .or_else(|| recipe.as_ref().map(|recipe| recipe.login_url.clone()))
        .or_else(|| source_defaults.map(|defaults| defaults.login_url.to_string()))
        .with_context(|| {
            format!(
                "web-stack source `{}` has no browser auth-assist recipe",
                source_id
            )
        })?;
    let source_id = module.map(|module| module.id()).unwrap_or(source_id.trim());
    anyhow::ensure!(!source_id.is_empty(), "web-stack source id is required");
    let allowed_domains = recipe
        .as_ref()
        .map(|recipe| recipe.allowed_domains.clone())
        .filter(|domains| !domains.is_empty())
        .or_else(|| {
            source_defaults.map(|defaults| {
                defaults
                    .allowed_domains
                    .iter()
                    .map(|domain| (*domain).to_string())
                    .collect()
            })
        })
        .unwrap_or_else(|| allowed_domains_from_url(&target_url));
    let explicit_secret_name = credential_ref
        .and_then(|value| parse_local_ctox_secret_ref(value).ok())
        .filter(|reference| reference.scope == crate::secrets::credential_scope())
        .map(|reference| reference.name);
    let secret_name = explicit_secret_name
        .as_deref()
        .or_else(|| {
            recipe
                .as_ref()
                .and_then(|recipe| recipe.required_secret_name)
        })
        .or_else(|| module.and_then(|module| module.requires_credential()))
        .or_else(|| source_defaults.map(|defaults| defaults.secret_name))
        .unwrap_or_default();
    // The session owner must be the human who requested the research. A
    // worker-owned session (`ctox_harness`) is invisible in the requesting
    // user's Browser app, so the unlock view had nothing to render and the
    // login the research was waiting for could never happen. Callers that do
    // not know the owner fall back to the requesting task's actor here.
    let resolved_owner_user_id = resolve_web_stack_auth_owner_user_id_with_env(
        root,
        &[],
        requesting_task_id,
        owner_user_id,
        accept_as_trusted_local,
    )?;
    let resolved_owner_user_id = match resolved_owner_user_id {
        Some(owner_user_id) => owner_user_id,
        None => anyhow::bail!(
            "auth assist owner unresolved (task={}, source_module={})",
            requesting_task_id,
            source_module
        ),
    };
    let owner_user_id = resolved_owner_user_id.as_str();
    let source_slug = rxdb_id_slug(source_id);
    // Authentication state belongs to a user/source pair, not to one research
    // command. A task-scoped session id left every completed capture alive and
    // created a fresh Chromium process for the next company, exhausting the
    // per-user browser budget after three leads. Keep the command id task-
    // scoped for traceability while reusing one live browser/profile per
    // source and owner.
    let session_suffix = format!("{}_{}", source_slug, rxdb_id_slug(owner_user_id));
    let generated_session_id = format!("browser_session_web_stack_auth_{session_suffix}");
    if let Some(existing) = crate::business_os::store::reusable_web_stack_auth_assist_request(
        root,
        source_id,
        secret_name,
        owner_user_id,
    )? {
        // Do not perpetuate pre-fix task-scoped sessions. After deployment the
        // daemon starts the stable source/user session and reuses it for every
        // following company.
        if existing
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            == Some(generated_session_id.as_str())
        {
            return Ok(existing);
        }
    }
    let now = now_ms();
    let dedupe_key = format!(
        "{}:{}",
        source_id,
        requesting_task_id.trim().to_ascii_lowercase()
    );
    let command_id = if deterministic_for_task && !requesting_task_id.trim().is_empty() {
        format!(
            "web_stack_auth_assist_{}_{}",
            source_slug,
            rxdb_id_slug(requesting_task_id)
        )
    } else {
        format!("web_stack_auth_assist_harness_{}_{}", now, Uuid::new_v4())
    };
    let session_id = preferred_auth_assist
        .and_then(|assist| assist.get("session_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| reusable_web_stack_auth_identifier(value, "browser_session_", &source_slug))
        .map(str::to_string)
        .unwrap_or(generated_session_id);
    let generated_tab_id = format!("browser_tab_web_stack_auth_{session_suffix}");
    let tab_id = preferred_auth_assist
        .and_then(|assist| assist.get("tab_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| reusable_web_stack_auth_identifier(value, "browser_tab_", &source_slug))
        .map(str::to_string)
        .unwrap_or(generated_tab_id);
    let verify_selector = recipe
        .as_ref()
        .and_then(|recipe| recipe.verify_selector)
        .unwrap_or_default();
    let credential_selector = recipe
        .as_ref()
        .and_then(|recipe| recipe.credential_selector)
        .unwrap_or_default();
    let capture_script = recipe
        .as_ref()
        .and_then(|recipe| recipe.capture_script)
        .unwrap_or_default();
    let document = serde_json::json!({
        "id": command_id.clone(),
        "command_id": command_id.clone(),
        "module": "ctox",
        "command_type": "web_stack.auth_assist.request",
        "type": "web_stack.auth_assist.request",
        "record_id": source_id,
        "status": "pending_sync",
        "payload": {
            "session_id": session_id.clone(),
            "tab_id": tab_id.clone(),
            "source_id": source_id,
            "secret_name": secret_name,
            "target_url": target_url.clone(),
            "allowed_domains": allowed_domains.clone(),
            "verify_selector": verify_selector,
            "credential_selector": credential_selector,
            "credential_ref": credential_ref,
            "login_hint": login_hint,
            "capture_script": capture_script,
            "purpose": "web_stack_auth",
            "owner_user_id": owner_user_id,
            "expires_at_ms": now + 30 * 60 * 1000,
            "browser_stream": "rxdb",
            "secret_value_in_rxdb": false,
            "dedupe_key": dedupe_key,
            "requesting_task_id": requesting_task_id,
            "instruction": "Melden Sie sich in der geoeffneten Quelle an und loesen Sie gegebenenfalls MFA, Captcha oder eine andere interaktive Freigabe. Klicken Sie danach im CTOX-Browser auf `Erledigt - Recherche fortsetzen`. Zugangswerte bleiben ausschliesslich im CTOX Secret Store und werden nie in Prompt, Ergebnis, Log oder RxDB geschrieben.",
            "required_skills": ["universal-scraping", "web-unlock"],
        },
        "client_context": {
            "source_module": source_module,
            "command_path": command_path,
            "owner_user_id": owner_user_id,
            "actor": {
                "id": owner_user_id,
                "display_name": owner_user_id,
                "role": "admin",
                "is_admin": true
            }
        },
        "created_at_ms": now,
        "updated_at_ms": now
    });
    let stored = if accept_as_trusted_local {
        let accepted =
            crate::business_os::store::accept_rxdb_business_command(root, document.clone())?;
        let mut projection = document;
        if let Some(object) = projection.as_object_mut() {
            for key in [
                "status",
                "execution_mode",
                "execution_task_id",
                "target_task_id",
                "target_record_id",
                "task_id",
                "task_status",
                "result",
            ] {
                if let Some(value) = accepted.get(key) {
                    object.insert(key.to_string(), value.clone());
                }
            }
            object.insert(
                "updated_at_ms".to_string(),
                serde_json::Value::from(now_ms()),
            );
        }
        crate::business_os::enqueue_business_command_document(root, projection)?;
        accepted
    } else {
        crate::business_os::enqueue_business_command_document(root, document)?
    };
    // Make the unlock target visible immediately: project session + tab so
    // the Browser app can render the request and auto-start the session. A
    // projection failure must not fail the auth request itself.
    if let Err(error) = crate::business_os::store::project_web_stack_auth_session_placeholder(
        root,
        &session_id,
        &tab_id,
        owner_user_id,
        &target_url,
        source_id,
    ) {
        eprintln!(
            "[business-os] web-stack auth-assist session projection failed \
             session_id={session_id}: {error:#}"
        );
    }
    let status = stored
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("pending_sync");
    Ok(serde_json::json!({
        "ok": true,
        "command_id": stored.get("command_id").and_then(serde_json::Value::as_str).unwrap_or_default(),
        "session_id": session_id,
        "tab_id": tab_id,
        "source_id": source_id,
        "target_url": target_url,
        "allowed_domains": allowed_domains,
        "required_secret_name": secret_name,
        "credential_ref": credential_ref,
        "login_hint": login_hint,
        "verify_selector": verify_selector,
        "credential_selector": credential_selector,
        "capture_script": capture_script,
        "owner_user_id": owner_user_id,
        "dedupe_key": dedupe_key,
        "requesting_task_id": requesting_task_id,
        "instruction": "Melden Sie sich in der geoeffneten Quelle an und loesen Sie gegebenenfalls MFA, Captcha oder eine andere interaktive Freigabe. Klicken Sie danach im CTOX-Browser auf `Erledigt - Recherche fortsetzen`.",
        "deduped_by_command_id": deterministic_for_task,
        "browser_stream": "rxdb",
        "secret_value_in_payload": false,
        "status": status,
        "task_id": stored.get("task_id").and_then(serde_json::Value::as_str).unwrap_or_default(),
        "execution_task_id": stored.get("execution_task_id").and_then(serde_json::Value::as_str).unwrap_or_default(),
        "trusted_local_intake": accept_as_trusted_local
    }))
}

fn reusable_web_stack_auth_identifier(value: &str, prefix: &str, source_slug: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 180
        && value.starts_with(prefix)
        && value.contains(source_slug)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn optional_web_stack_credential_ref(value: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    anyhow::ensure!(
        !value.chars().any(char::is_whitespace),
        "credential_ref must be a reference URI without whitespace"
    );
    anyhow::ensure!(
        [
            "ctox-secret://",
            "secret://",
            "vault://",
            "op://",
            "keychain://"
        ]
        .iter()
        .any(|prefix| value.starts_with(prefix)),
        "credential_ref must be a secret reference URI, not a raw credential value"
    );
    Ok(Some(value.to_string()))
}

fn optional_web_stack_login_hint(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn resolve_web_stack_auth_owner_user_id(
    root: &Path,
    args: &[String],
    requesting_task_id: &str,
    accept_as_trusted_local: bool,
) -> anyhow::Result<Option<String>> {
    let env_owner = env::var("CTOX_OWNER_USER_ID").ok();
    resolve_web_stack_auth_owner_user_id_with_env(
        root,
        args,
        requesting_task_id,
        env_owner.as_deref(),
        accept_as_trusted_local,
    )
}

#[derive(Debug, Deserialize)]
struct WebStackCommandSessionClaims {
    schema: String,
    actor: String,
    command_id: String,
    payload_hash: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
}

fn resolve_web_stack_auth_owner_user_id_with_env(
    root: &Path,
    args: &[String],
    requesting_task_id: &str,
    env_owner_user_id: Option<&str>,
    accept_as_trusted_local: bool,
) -> anyhow::Result<Option<String>> {
    let claimed_owner = flag_value(args, "--owner-user-id")
        .or(env_owner_user_id)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let command_session_owner = flag_value(args, "--command-session")
        .map(|token| web_stack_auth_owner_from_command_session(root, token))
        .transpose()?
        .flatten();
    let task_owner = web_stack_auth_owner_from_task_link(root, requesting_task_id)?;
    let task_metadata_owner = web_stack_auth_owner_from_task_metadata(root, requesting_task_id)?;
    let command_owner = web_stack_auth_owner_from_command_authorization(root, requesting_task_id)?;
    let chat_owner = web_stack_auth_owner_from_chat(root, requesting_task_id)?;
    let verified_owner = command_session_owner
        .or(task_owner)
        .or(task_metadata_owner)
        .or(command_owner)
        .or(chat_owner);

    if let Some(verified) = verified_owner {
        if claimed_owner.is_some_and(|claimed| claimed != verified) {
            eprintln!(
                "[business-os] web-stack auth owner override task={} claimed_id={} verified_id={}",
                requesting_task_id,
                claimed_owner.unwrap_or_default(),
                verified
            );
        }
        return Ok(Some(verified));
    }
    if accept_as_trusted_local {
        return Ok(claimed_owner.map(str::to_string));
    }
    if let Some(claimed) = claimed_owner {
        eprintln!(
            "[business-os] web-stack auth owner rejected without verified context task={} claimed_id={}",
            requesting_task_id, claimed
        );
    }
    Ok(None)
}

fn web_stack_auth_owner_from_command_session(
    root: &Path,
    token: &str,
) -> anyhow::Result<Option<String>> {
    let (payload, signature) = token
        .trim()
        .split_once('.')
        .context("malformed Business OS internal command-session token")?;
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature)
        .context("invalid Business OS internal command-session signature")?;
    let secret = crate::secrets::read_secret_value(
        root,
        "business_os",
        "mcp_internal_command_session_signing_secret",
    )?;
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    ring::hmac::verify(&key, payload.as_bytes(), &signature)
        .map_err(|_| anyhow::anyhow!("invalid Business OS internal command-session signature"))?;
    let claims: WebStackCommandSessionClaims =
        serde_json::from_slice(&base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload)?)?;
    anyhow::ensure!(
        claims.schema == "ctox.business_os.mcp_command_session.v1",
        "unsupported Business OS internal command-session schema"
    );
    let current_time = now_ms() as i64;
    anyhow::ensure!(
        claims.issued_at_ms <= current_time && current_time < claims.expires_at_ms,
        "Business OS internal command session expired"
    );
    let command = channels::inspect_business_command(root, &claims.command_id)?
        .context("Business OS internal command session references an unknown command")?;
    anyhow::ensure!(
        command
            .pointer("/command/payload_hash")
            .and_then(serde_json::Value::as_str)
            == Some(claims.payload_hash.as_str()),
        "Business OS internal command payload changed"
    );
    anyhow::ensure!(
        command
            .pointer("/command/execution_phase")
            .and_then(serde_json::Value::as_str)
            != Some("terminal"),
        "Business OS internal command is already terminal"
    );
    let authorization =
        crate::business_os::store::revalidate_business_command_execution_authorization(
            root,
            &claims.command_id,
        )?;
    let session_user_id = authorization
        .pointer("/actor/id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("Business OS internal command session has no verified user")?;
    anyhow::ensure!(
        session_user_id == claims.actor,
        "Business OS internal command authorization changed"
    );
    Ok(Some(session_user_id.to_string()))
}

fn web_stack_auth_owner_from_command_context(context: &serde_json::Value) -> Option<String> {
    let parsed_client_context = match context.pointer("/command/client_context") {
        Some(serde_json::Value::String(value)) => {
            serde_json::from_str(value).unwrap_or(serde_json::Value::Null)
        }
        Some(client_context) => client_context.clone(),
        None => serde_json::Value::Null,
    };
    let from_client_context = ["/owner_user_id", "/actor/id", "/user_id"]
        .into_iter()
        .find_map(|pointer| {
            parsed_client_context
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    from_client_context
        .or_else(|| web_stack_auth_owner_from_native_authorization(context))
        .or_else(|| {
            context
                .pointer("/command/payload/owner_user_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn web_stack_auth_owner_from_native_authorization(context: &serde_json::Value) -> Option<String> {
    let authorization = context.pointer("/command/native_authorization")?;
    (authorization
        .get("contract")
        .and_then(serde_json::Value::as_str)
        == Some("ctox-business-command-authorization-v1")
        && authorization
            .get("allowed")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        && authorization
            .pointer("/actor/trusted")
            .and_then(serde_json::Value::as_bool)
            == Some(true))
    .then(|| {
        authorization
            .pointer("/actor/id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
    .flatten()
}

fn web_stack_auth_owner_from_task_link(
    root: &Path,
    requesting_task_id: &str,
) -> anyhow::Result<Option<String>> {
    let requesting_task_id = requesting_task_id.trim();
    if requesting_task_id.is_empty() {
        return Ok(None);
    }
    Ok(
        channels::inspect_business_command_for_task(root, requesting_task_id)?
            .as_ref()
            .and_then(web_stack_auth_owner_from_command_context),
    )
}

/// Queue tasks spawned by a Business OS command (person-research gap closure)
/// carry `business_os_command_id` in their metadata instead of a
/// `business_command_task_links` row, because the spawning command is already
/// terminal. The human owner is the command's verified actor.
fn web_stack_auth_owner_from_task_metadata(
    root: &Path,
    requesting_task_id: &str,
) -> anyhow::Result<Option<String>> {
    let requesting_task_id = requesting_task_id.trim();
    if requesting_task_id.is_empty() {
        return Ok(None);
    }
    let Some(task) = channels::load_queue_task(root, requesting_task_id)? else {
        return Ok(None);
    };
    let Some(command_id) = task
        .metadata
        .get("business_os_command_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    Ok(channels::inspect_business_command(root, command_id)?
        .as_ref()
        .and_then(web_stack_auth_owner_from_command_context))
}

fn web_stack_auth_owner_from_command_authorization(
    root: &Path,
    requesting_task_id: &str,
) -> anyhow::Result<Option<String>> {
    let requesting_task_id = requesting_task_id.trim();
    if requesting_task_id.is_empty() {
        return Ok(None);
    }
    Ok(
        channels::inspect_business_command(root, requesting_task_id)?
            .as_ref()
            .and_then(web_stack_auth_owner_from_native_authorization),
    )
}

fn web_stack_auth_owner_from_chat(
    root: &Path,
    requesting_task_id: &str,
) -> anyhow::Result<Option<String>> {
    let requesting_task_id = requesting_task_id.trim();
    if requesting_task_id.is_empty() {
        return Ok(None);
    }
    Ok(crate::business_os::store::pull_collection_record(
        root,
        "business_chats",
        requesting_task_id,
    )?
    .and_then(|chat| {
        chat.get("owner_user_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedWebStackCredential {
    credential_value: String,
    login_hint: Option<String>,
    login_hint_from_secret: bool,
}

fn resolve_web_stack_credential(
    secret_value: &str,
    explicit_login_hint: Option<String>,
) -> ResolvedWebStackCredential {
    let parsed = serde_json::from_str::<serde_json::Value>(secret_value).ok();
    let object = parsed.as_ref().and_then(serde_json::Value::as_object);
    let bundled_credential = object.and_then(|value| {
        ["password", "credential", "secret"]
            .into_iter()
            .find_map(|key| value.get(key).and_then(serde_json::Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    });
    let bundled_login_hint = object.and_then(|value| {
        ["username", "email", "login", "login_hint"]
            .into_iter()
            .find_map(|key| value.get(key).and_then(serde_json::Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    });
    let login_hint_from_secret = explicit_login_hint.is_none() && bundled_login_hint.is_some();
    ResolvedWebStackCredential {
        credential_value: bundled_credential.unwrap_or_else(|| secret_value.to_string()),
        login_hint: explicit_login_hint.or(bundled_login_hint),
        login_hint_from_secret,
    }
}

fn parse_local_ctox_secret_ref(value: &str) -> anyhow::Result<LocalCtoxSecretRef> {
    let parsed = Url::parse(value)
        .with_context(|| format!("failed to parse credential_ref `{value}` as URI"))?;
    anyhow::ensure!(
        parsed.scheme() == "ctox-secret",
        "auth-assist-login can resolve only local ctox-secret://scope/name references"
    );
    anyhow::ensure!(
        parsed.username().is_empty() && parsed.password().is_none(),
        "ctox-secret credential_ref must not contain userinfo"
    );
    anyhow::ensure!(
        parsed.query().is_none() && parsed.fragment().is_none(),
        "ctox-secret credential_ref must not contain query or fragment"
    );
    let scope = parsed
        .host_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("ctox-secret credential_ref must use ctox-secret://scope/name")?;
    let mut path_segments = parsed
        .path_segments()
        .context("ctox-secret credential_ref must use ctox-secret://scope/name")?;
    let name = path_segments
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("ctox-secret credential_ref must include a secret name")?;
    anyhow::ensure!(
        path_segments.next().is_none(),
        "ctox-secret credential_ref must use exactly one name segment"
    );
    Ok(LocalCtoxSecretRef {
        scope: scope.to_string(),
        name: name.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_web_stack_auth_assist_login_source_with_continuation(
    target_url: &str,
    source_id: &str,
    login_hint: Option<&str>,
    credential_ref: &str,
    secret_value: &str,
    credential_selector: &str,
    verify_selector: &str,
    continuation_source: Option<&str>,
) -> anyhow::Result<String> {
    let mut source = format!(
        "// ctox-browser: timeout_ms=45000\nconst targetUrl = {};\nconst sourceId = {};\nconst loginHint = {};\nconst credentialRef = {};\nconst credentialValue = {};\nconst configuredCredentialSelector = {};\nconst configuredVerifySelector = {};\n",
        serde_json::to_string(target_url)?,
        serde_json::to_string(source_id)?,
        match login_hint {
            Some(value) => serde_json::to_string(value)?,
            None => "null".to_string(),
        },
        serde_json::to_string(credential_ref)?,
        serde_json::to_string(secret_value)?,
        serde_json::to_string(credential_selector)?,
        serde_json::to_string(verify_selector)?,
    );
    source.push_str(
        r#"
const targetOrigin = new URL(targetUrl).origin;
const startedAt = Date.now();
const trimText = (value, max = 180) => {
  const text = String(value ?? "").replace(/\s+/g, " ").trim();
  return text.length > max ? text.slice(0, max - 1) + "..." : text;
};
const emptyAuthSignals = () => ({
  mfa_required: false,
  login_error_detected: false,
  otp_field_count: 0,
  mfa_terms: [],
  error_terms: [],
  login_error_text: "",
});
const cssEscape = (value) => globalThis.CSS && typeof globalThis.CSS.escape === "function"
  ? globalThis.CSS.escape(String(value))
  : String(value).replace(/["\\]/g, "\\$&");
const browserCandidateFieldsInFrame = async (frame, kind, relaxed) => frame.evaluate(({ kind, relaxed }) => {
  const cssEscape = (value) => globalThis.CSS && typeof globalThis.CSS.escape === "function"
    ? globalThis.CSS.escape(String(value))
    : String(value).replace(/["\\]/g, "\\$&");
  const visible = (element) => {
    const style = globalThis.getComputedStyle(element);
    const box = element.getBoundingClientRect();
    if (style.display === "none" || element.disabled) return false;
    // The relaxed pass keeps a field that is only *currently* unpaintable: a
    // step the page has not expanded yet, or an element caught mid-transition
    // with opacity 0 and a zero-height box. Filling such a field is worth a
    // try; refusing to even see it is how a login ends as "field missing".
    if (relaxed) return true;
    return style.visibility !== "hidden"
      && Number(style.opacity || "1") > 0
      && box.width > 0
      && box.height > 0
      && element.getAttribute("aria-hidden") !== "true";
  };
  const labelFor = (element) => {
    const labels = [];
    if (element.id) {
      for (const label of Array.from(document.querySelectorAll(`label[for="${cssEscape(element.id)}"]`))) {
        labels.push(label.innerText || label.textContent || "");
      }
    }
    const parentLabel = element.closest("label");
    if (parentLabel) labels.push(parentLabel.innerText || parentLabel.textContent || "");
    return labels.join(" ");
  };
  const descriptorFor = (element, source) => {
    const tag = element.tagName.toLowerCase();
    const id = element.getAttribute("id");
    const name = element.getAttribute("name");
    const type = element.getAttribute("type");
    const placeholder = element.getAttribute("placeholder");
    let selector = null;
    if (id) selector = `#${cssEscape(id)}`;
    else if (name) selector = `${tag}[name="${cssEscape(name)}"]`;
    else if (placeholder) selector = `${tag}[placeholder="${cssEscape(placeholder)}"]`;
    else if (type) selector = `${tag}[type="${cssEscape(type)}"]`;
    else selector = tag;
    let index = 0;
    try {
      const matches = Array.from(document.querySelectorAll(selector));
      index = Math.max(0, matches.indexOf(element));
    } catch {}
    return {
      selector,
      index,
      source,
      tag,
      type: type || null,
      name: name || null,
      autocomplete: element.getAttribute("autocomplete") || null,
      placeholder_present: !!placeholder,
    };
  };
  const tokensFor = (element) => [
    element.getAttribute("type"),
    element.getAttribute("name"),
    element.getAttribute("id"),
    element.getAttribute("autocomplete"),
    element.getAttribute("placeholder"),
    element.getAttribute("aria-label"),
    labelFor(element),
  ].filter(Boolean).join(" ").toLowerCase();
  const scoreFor = (element) => {
    const tokens = tokensFor(element);
    const type = String(element.getAttribute("type") || "").toLowerCase();
    if (kind === "credential") {
      let score = type === "password" ? 100 : 0;
      if (/(password|passwort|passwd|pwd|kennwort|secret)/.test(tokens)) score += 80;
      if (/(otp|totp|mfa|2fa|code|verification|confirm)/.test(tokens)) score -= 70;
      return score;
    }
    let score = 0;
    if (type === "email") score += 100;
    if (/(email|e-mail|mail|username|user name|login|logon|account|benutzer|nutzer)/.test(tokens)) score += 80;
    if (type === "password" || /(password|passwort|passwd|pwd|kennwort)/.test(tokens)) score -= 100;
    if (/(search|query|filter)/.test(tokens)) score -= 50;
    return score;
  };
  const fields = Array.from(document.querySelectorAll("input, textarea, [contenteditable='true']"))
    .filter(visible)
    .filter((element) => {
      const type = String(element.getAttribute("type") || "").toLowerCase();
      return !["hidden", "submit", "button", "checkbox", "radio", "file"].includes(type);
    })
    .map((element) => ({ element, score: scoreFor(element) }))
    .filter((entry) => entry.score > 0)
    .sort((left, right) => right.score - left.score);
  return fields.slice(0, 8).map((entry) => descriptorFor(entry.element, relaxed ? "heuristic-relaxed" : "heuristic"));
}, { kind, relaxed });
// The login form is not always in the main frame. Several vendor logins render
// it inside an iframe, where page.evaluate() cannot see it at all — the scan
// then came back empty and the attempt was reported as a missing credential
// field even though the field was right there. dismissConsent() already
// searches every frame; the field scan has to do the same.
const browserCandidateFields = async (kind) => {
  const frames = page.frames();
  for (const relaxed of [false, true]) {
    const collected = [];
    for (let frameIndex = 0; frameIndex < frames.length; frameIndex += 1) {
      let found = [];
      try {
        found = await browserCandidateFieldsInFrame(frames[frameIndex], kind, relaxed);
      } catch {
        continue;
      }
      for (const descriptor of found) {
        collected.push({
          ...descriptor,
          frame_index: frameIndex,
          frame_url: frameIndex === 0 ? null : (frames[frameIndex].url() || null),
        });
      }
    }
    // Only widen the visibility test when the strict pass found nothing, so a
    // normal page keeps its precise, well-ordered result.
    if (collected.length) return collected;
  }
  return [];
};
const fillField = async (kind, value, configuredSelector = "") => {
  const candidates = [];
  if (configuredSelector && kind === "credential") {
    candidates.push({ selector: configuredSelector, index: 0, source: "configured", configured: true });
  }
  candidates.push(...await browserCandidateFields(kind));
  const frames = page.frames();
  for (const candidate of candidates) {
    if (!candidate.selector) continue;
    // A configured selector carries no frame of its own, so it has to be tried
    // in every frame — otherwise a hand-configured selector for a field inside
    // an iframe silently resolves to nothing in the main frame.
    const scopes = Number.isInteger(candidate.frame_index)
      ? [frames[candidate.frame_index] || page]
      : frames;
    for (const scope of scopes) {
    try {
      const locator = scope.locator(candidate.selector).nth(Number(candidate.index || 0));
      if ((await locator.count()) < 1) continue;
      await locator.fill(String(value), { timeout: 3500 });
      // A React form that has not finished hydrating drops a programmatic
      // fill: the DOM carries the value, the component state stays empty, and
      // the submit that follows validates against nothing and re-renders the
      // same step. Read the value back and, if it did not take, type it the
      // way a person would.
      let confirmed = await locator.inputValue().catch(() => null);
      if (confirmed !== String(value)) {
        try {
          await locator.click({ timeout: 2000 });
          await locator.fill("", { timeout: 2000 }).catch(() => null);
          await locator.pressSequentially(String(value), { delay: 25, timeout: 5000 });
          confirmed = await locator.inputValue().catch(() => null);
        } catch {}
      }
      return {
        selector: candidate.selector,
        index: Number(candidate.index || 0),
        source: candidate.source || "unknown",
        configured: !!candidate.configured,
        tag: candidate.tag || null,
        type: candidate.type || null,
        name: candidate.name || null,
        autocomplete: candidate.autocomplete || null,
        placeholder_present: !!candidate.placeholder_present,
        value_confirmed: confirmed === String(value),
        frame_index: frames.indexOf(scope) < 0 ? 0 : frames.indexOf(scope),
        frame_url: scope === page || scope === frames[0] ? null : (scope.url() || null),
      };
    } catch {}
    }
  }
  return null;
};
// A consent overlay intercepts a click even when the target itself is found
// and visible — Playwright resolves the element and then times out waiting for
// it to become actionable. That is exactly how the D&B Hoovers login stalled:
// the submit button resolved, the click never landed, and a later fallback
// reported a transition that had not happened. Dismiss and retry once.
const clickThrough = async (locator, timeout = 3500) => {
  try {
    await locator.click({ timeout });
    return true;
  } catch (error) {
    const retried = await dismissConsent();
    if (retried && retried.clicked) {
      await page.waitForTimeout(300).catch(() => null);
      try {
        await locator.click({ timeout });
        return true;
      } catch {}
    }
    // Last resort: dispatch the click on the element itself. A cookie banner
    // that our selectors do not know still covers the button and swallows a
    // real mouse event, but it cannot intercept this one. Reached only after
    // a genuine failure, so a truly unclickable target still ends up throwing.
    await locator.evaluate((element) => element.click());
    return true;
  }
};
const clickSubmit = async (credentialField) => {
  const submitSelectors = [
    "button[type='submit']",
    "input[type='submit']",
    "button:has-text('Sign in')",
    "button:has-text('Log in')",
    "button:has-text('Login')",
    "button:has-text('Continue')",
    "button:has-text('Next')",
    "button:has-text('Anmelden')",
    "button:has-text('Einloggen')",
    "button:has-text('Weiter')",
    "button:has-text('Fortfahren')",
    "[role='button']:has-text('Sign in')",
    "[role='button']:has-text('Log in')",
    "[role='button']:has-text('Continue')",
    "[role='button']:has-text('Next')",
    "[role='button']:has-text('Weiter')",
    "[role='button']:has-text('Fortfahren')",
  ];
  // The submit button lives in the same frame as the field it submits. Resolve
  // that frame first; searching the main frame for a button that sits inside an
  // iframe finds nothing and loses the form-scoped match.
  const frames = page.frames();
  const fieldScope = credentialField && Number.isInteger(credentialField.frame_index)
    ? (frames[credentialField.frame_index] || page)
    : page;
  if (credentialField && credentialField.selector) {
    try {
      const field = fieldScope.locator(credentialField.selector).nth(Number(credentialField.index || 0));
      const form = field.locator("xpath=ancestor::form[1]");
      if ((await form.count()) > 0) {
        for (const selector of submitSelectors) {
          const locator = form.locator(selector).first();
          if ((await locator.count()) < 1 || !(await locator.isVisible().catch(() => false))) continue;
          await clickThrough(locator);
          return { mode: "click", scope: "field-form", selector };
        }
      }
    } catch {}
  }
  for (const selector of submitSelectors) {
    for (const scope of [fieldScope, ...frames]) {
      try {
        const locator = scope.locator(selector).first();
        if ((await locator.count()) < 1 || !(await locator.isVisible().catch(() => false))) continue;
        await clickThrough(locator);
        return { mode: "click", selector, frame_url: scope === frames[0] ? null : (scope.url() || null) };
      } catch {}
    }
  }
  if (credentialField && credentialField.selector) {
    try {
      await fieldScope.locator(credentialField.selector).nth(Number(credentialField.index || 0)).press("Enter", { timeout: 3500 });
      return { mode: "press", key: "Enter", selector: credentialField.selector };
    } catch {}
  }
  return null;
};
const clickLoginEntry = async () => {
  const targetOrigin = (() => { try { return new URL(targetUrl).origin; } catch { return null; } })();
  try {
    const candidates = await page.locator("a[href]").evaluateAll((links) => links.map((link, index) => {
      const style = globalThis.getComputedStyle(link);
      const box = link.getBoundingClientRect();
      return {
        index,
        href: link.href || "",
        text: String(link.innerText || link.textContent || "").replace(/\s+/g, " ").trim().slice(0, 80),
        visible: style.visibility !== "hidden" && style.display !== "none" && box.width > 0 && box.height > 0,
      };
    }));
    for (const candidate of candidates) {
      if (!candidate.visible || !candidate.href) continue;
      let parsed = null;
      try { parsed = new URL(candidate.href, targetUrl); } catch { continue; }
      if (targetOrigin && parsed.origin !== targetOrigin) continue;
      const signal = `${parsed.pathname} ${candidate.text}`.toLowerCase();
      if (!/(^|[\s/_-])(login|log-in|signin|sign-in|anmelden|einloggen)([\s/_-]|$)/.test(signal)) continue;
      if (/(signup|sign-up|register|create-account|registrieren)/.test(signal)) continue;
      await page.locator("a[href]").nth(candidate.index).click({ timeout: 3500 });
      return { mode: "same-origin-link", path: parsed.pathname.slice(0, 160) };
    }
  } catch {}
  const buttonSelectors = [
    "button:has-text('Sign in')",
    "button:has-text('Log in')",
    "button:has-text('Login')",
    "button:has-text('Anmelden')",
    "button:has-text('Einloggen')",
    "[role='button']:has-text('Sign in')",
    "[role='button']:has-text('Log in')",
    "[role='button']:has-text('Anmelden')",
  ];
  for (const selector of buttonSelectors) {
    try {
      const locator = page.locator(selector).first();
      if ((await locator.count()) < 1 || !(await locator.isVisible())) continue;
      await clickThrough(locator);
      return { mode: "login-button", selector };
    } catch {}
  }
  return null;
};
const dismissConsent = async () => {
  const selectors = [
    // Reject-only first: it clears the banner just as well as accepting and
    // keeps the session free of non-essential cookies. TrustArc (D&B Hoovers)
    // was missing entirely, so its overlay stayed up and swallowed the click
    // on the login form's own submit button — the login never left step one.
    "button#truste-consent-required",
    "button.trustarc-declineall-btn",
    "button:has-text('Required Only')",
    "button:has-text('Nur erforderliche')",
    "button#onetrust-reject-all-handler",
    "button#onetrust-accept-btn-handler",
    "button[data-testid='uc-accept-all-button']",
    "button[aria-label*='Accept all' i]",
    "button[aria-label*='Alle akzeptieren' i]",
    "button:has-text('Accept all')",
    "button:has-text('Alle akzeptieren')",
    "button:has-text('Allow all')",
    "button:has-text('Zustimmen')",
  ];
  // TrustArc and several other CMPs render the banner inside an iframe, where
  // page.locator() cannot see it: the overlay then stays up and eats the click
  // meant for the page beneath. Search every frame, main frame first.
  for (const selector of selectors) {
    for (const frame of [page, ...page.frames()]) {
      try {
        const locator = frame.locator(selector).first();
        if ((await locator.count()) < 1 || !(await locator.isVisible().catch(() => false))) continue;
        await locator.click({ timeout: 2500 });
        await page.waitForTimeout(150).catch(() => null);
        return { clicked: true, selector, frame_url: frame === page ? null : frame.url() };
      } catch {}
    }
  }
  return { clicked: false };
};
const pageSignals = async () => {
  let title = "";
  try { title = await page.title(); } catch {}
  let formState = {};
  let authSignals = emptyAuthSignals();
  try {
    const pageState = await page.evaluate(() => {
      const trimLocal = (value, max = 180) => {
        const text = String(value ?? "").replace(/\s+/g, " ").trim();
        return text.length > max ? text.slice(0, max - 1) + "..." : text;
      };
      const visible = (element) => {
        const style = globalThis.getComputedStyle(element);
        const box = element.getBoundingClientRect();
        return style.visibility !== "hidden"
          && style.display !== "none"
          && Number(style.opacity || "1") > 0
          && box.width > 0
          && box.height > 0;
      };
      const tokensFor = (element) => [
        element.getAttribute("type"),
        element.getAttribute("name"),
        element.getAttribute("id"),
        element.getAttribute("autocomplete"),
        element.getAttribute("placeholder"),
        element.getAttribute("aria-label"),
        element.textContent,
      ].filter(Boolean).join(" ").toLowerCase();
      const visibleText = String(document.body ? document.body.innerText || "" : "").replace(/\s+/g, " ").trim();
      const lowerText = visibleText.toLowerCase();
      const normalizedText = lowerText.normalize("NFKD").replace(/[\u0300-\u036f]/g, "").replace(/ł/g, "l");
      const matchingTerms = (entries) => entries
        .filter((entry) => entry.pattern.test(normalizedText))
        .map((entry) => entry.term);
      const mfaTerms = matchingTerms([
        { term: "mfa", pattern: /\bmfa\b/ },
        { term: "2fa", pattern: /\b2fa\b/ },
        { term: "two-factor", pattern: /two[-\s]?factor/ },
        { term: "multi-factor", pattern: /multi[-\s]?factor/ },
        { term: "one-time-code", pattern: /one[-\s]?time[-\s]?code/ },
        { term: "otp", pattern: /\botp\b/ },
        { term: "verification-code", pattern: /verification\s+code/ },
        { term: "security-code", pattern: /security\s+code/ },
        { term: "authenticator", pattern: /authenticator/ },
        { term: "sicherheitscode", pattern: /sicherheitscode/ },
        { term: "verifizierungscode", pattern: /verifizierungscode/ },
        { term: "zweifaktor", pattern: /zweifaktor|zwei[-\s]?faktor|zweistufig/ },
      ]);
      const errorTerms = matchingTerms([
        { term: "invalid", pattern: /\binvalid\b/ },
        { term: "incorrect", pattern: /\bincorrect\b/ },
        { term: "wrong", pattern: /\bwrong\b/ },
        { term: "login-failed", pattern: /login\s+failed|sign\s+in\s+failed|authentication\s+failed/ },
        { term: "denied", pattern: /\bdenied\b|access\s+denied/ },
        { term: "locked", pattern: /\blocked\b/ },
        { term: "disabled", pattern: /\bdisabled\b/ },
        { term: "too-many", pattern: /too\s+many/ },
        { term: "expired", pattern: /\bexpired\b/ },
        { term: "ungueltig", pattern: /ungultig|ungueltig/ },
        { term: "falsches-passwort", pattern: /falsches\s+passwort/ },
        { term: "anmeldung-fehlgeschlagen", pattern: /anmeldung\s+fehlgeschlagen/ },
        { term: "gesperrt", pattern: /gesperrt/ },
        { term: "abgelaufen", pattern: /abgelaufen/ },
        { term: "identifiants-incorrects", pattern: /identifiants?\s+(incorrects?|invalides?)/ },
        { term: "connexion-echouee", pattern: /connexion\s+(a\s+)?echoue|echec\s+de\s+(la\s+)?connexion/ },
        { term: "credenciales-incorrectas", pattern: /credenciales?\s+(incorrectas?|invalidas?)/ },
        { term: "inicio-sesion-fallido", pattern: /inicio\s+de\s+sesion\s+fallido/ },
        { term: "credenziali-non-valide", pattern: /credenziali\s+(non\s+valide|errate)/ },
        { term: "inloggegevens-onjuist", pattern: /inloggegevens\s+(onjuist|ongeldig)/ },
        { term: "nieprawidlowe-dane", pattern: /nieprawidlowe\s+dane|bledne\s+haslo/ },
      ]);
      const otpFieldCount = Array.from(document.querySelectorAll("input, textarea"))
        .filter(visible)
        .filter((element) => /(otp|totp|mfa|2fa|code|verification|verifizierung|sicherheitscode|one[-\s]?time)/.test(tokensFor(element)))
        .length;
      const errorNodes = Array.from(document.querySelectorAll([
        "[role='alert']",
        "[data-testid*='error' i]",
        "[class*='error' i]",
        "[id*='error' i]",
      ].join(",")))
        .filter(visible)
        .map((element) => trimLocal(element.innerText || element.textContent || "", 240))
        .filter(Boolean);
      const loginErrorText = trimLocal(errorNodes.join(" ") || (errorTerms.length > 0 ? visibleText : ""), 240);
      return {
        form_state: {
          visible_password_fields: Array.from(document.querySelectorAll("input[type='password']")).filter(visible).length,
          visible_email_fields: Array.from(document.querySelectorAll("input[type='email']")).filter(visible).length,
          visible_forms: Array.from(document.querySelectorAll("form")).filter(visible).length,
        },
        auth_signals: {
          mfa_required: mfaTerms.length > 0 || otpFieldCount > 0,
          login_error_detected: errorTerms.length > 0 || errorNodes.length > 0,
          otp_field_count: otpFieldCount,
          mfa_terms: mfaTerms.slice(0, 8),
          error_terms: errorTerms.slice(0, 8),
          login_error_text: loginErrorText,
        },
      };
    });
    formState = pageState.form_state || {};
    authSignals = pageState.auth_signals || emptyAuthSignals();
  } catch {}
  return { url: page.url(), title, form_state: formState, auth_signals: authSignals };
};
const waitForAuthTransition = async (previousUrl, timeoutMs = 12000, verifySelector = "") => {
  const deadline = Date.now() + timeoutMs;
  let signals = await pageSignals();
  let verifyFound = false;
  let settledReason = "timeout";
  while (Date.now() < deadline) {
    await Promise.race([
      page.waitForLoadState("domcontentloaded", { timeout: 1500 }).catch(() => null),
      page.waitForTimeout(250).catch(() => null),
    ]).catch(() => null);
    if (verifySelector) {
      verifyFound = await page.locator(verifySelector).first().isVisible().catch(() => false);
      if (verifyFound) {
        signals = await pageSignals();
        settledReason = "verify-selector-visible";
        break;
      }
    }
    signals = await pageSignals();
    const authSignals = signals.auth_signals || emptyAuthSignals();
    if (authSignals.mfa_required === true || authSignals.login_error_detected === true) {
      settledReason = "auth-terminal-signal";
      break;
    }
    const transientDocument = /^Loading https?:\/\//i.test(String(signals.title || ""))
      || (!signals.title
        && Number(signals.form_state?.visible_forms || 0) === 0
        && Number(signals.form_state?.visible_password_fields || 0) === 0);
    const formGone = Number(signals.form_state?.visible_password_fields || 0) === 0;
    const urlChanged = signals.url !== previousUrl;
    if (!verifySelector && !transientDocument && (formGone || urlChanged)) {
      settledReason = "navigation-settled";
      break;
    }
    await page.waitForTimeout(250).catch(() => null);
  }
  return { signals, verify_found: verifyFound, settled_reason: settledReason };
};
const waitForLoginEntryTransition = async (previousUrl, timeoutMs = 12000) => {
  const deadline = Date.now() + timeoutMs;
  let signals = await pageSignals();
  while (Date.now() < deadline) {
    await Promise.race([
      page.waitForLoadState("domcontentloaded", { timeout: 1500 }).catch(() => null),
      page.waitForTimeout(250).catch(() => null),
    ]).catch(() => null);
    signals = await pageSignals();
    const formState = signals.form_state || {};
    const transientDocument = /^Loading https?:\/\//i.test(String(signals.title || ""))
      || (!signals.title && Number(formState.visible_forms || 0) === 0);
    const authFormVisible = Number(formState.visible_password_fields || 0) > 0
      || Number(formState.visible_email_fields || 0) > 0
      || Number(formState.visible_forms || 0) > 0;
    if (!transientDocument && (signals.url !== previousUrl || authFormVisible)) break;
    await page.waitForTimeout(250).catch(() => null);
  }
  return signals;
};
const waitForCredentialTransition = async (previousUrl, timeoutMs = 12000) => {
  const deadline = Date.now() + timeoutMs;
  let signals = await pageSignals();
  while (Date.now() < deadline) {
    await Promise.race([
      page.waitForLoadState("domcontentloaded", { timeout: 1500 }).catch(() => null),
      page.waitForTimeout(250).catch(() => null),
    ]).catch(() => null);
    signals = await pageSignals();
    const credentialCandidates = await browserCandidateFields("credential").catch(() => []);
    if (credentialCandidates.length > 0) break;
    const authSignals = signals.auth_signals || emptyAuthSignals();
    if (authSignals.mfa_required === true || authSignals.login_error_detected === true) break;
    await page.waitForTimeout(signals.url === previousUrl ? 250 : 100).catch(() => null);
  }
  return signals;
};
const gotoTargetWithRetry = async () => {
  let lastError = null;
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    try {
      return await ctoxBrowser.goto(targetUrl, { waitUntil: "domcontentloaded", timeoutMs: 20000, limit: 80, textMax: 120 });
    } catch (error) {
      lastError = error;
      if (attempt < 2) await page.waitForTimeout(500).catch(() => null);
    }
  }
  throw lastError || new Error("login target navigation failed");
};
const before = await gotoTargetWithRetry();
await page.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => null);
const consentTransition = await dismissConsent();
const beforeSignals = await pageSignals();
const preAuthenticatedVerifyFound = configuredVerifySelector
  ? await page.locator(configuredVerifySelector).first().isVisible().catch(() => false)
  : false;
// A stored session sends us straight past the login form: Leadfeeder answers
// /login with its dashboard, XING with an in-app page. The verify selector is
// the only thing that used to notice, and once its markup drifts the run walks
// into the login path, finds no password field precisely because it is already
// signed in, and reports `credential-field-not-found` on a working session.
// Landing somewhere other than the login URL with no credential field and no
// error is the same evidence, and it does not rot when a class name changes.
const preAuthenticatedByLanding = await (async () => {
  if (preAuthenticatedVerifyFound) return false;
  const landedElsewhere = beforeSignals.url && beforeSignals.url !== targetUrl;
  if (!landedElsewhere) return false;
  const signals = beforeSignals.auth_signals || emptyAuthSignals();
  if (signals.mfa_required === true || signals.login_error_detected === true) return false;
  if (Number(beforeSignals.form_state?.visible_password_fields || 0) > 0) return false;
  const credentialCandidates = await browserCandidateFields("credential").catch(() => []);
  return credentialCandidates.length === 0;
})();
let loginOutcome = null;
if (preAuthenticatedVerifyFound || preAuthenticatedByLanding) {
  const observed = await ctoxBrowser.observe({ limit: 50, textMax: 140 });
  loginOutcome = {
    ok: true,
    reason: "already-authenticated",
    login_state: "authenticated",
    source_id: sourceId,
    target_url: targetUrl,
    credential_ref: credentialRef,
    login_hint_present: !!loginHint,
    login_transition: null,
    consent_transition: consentTransition,
    mfa_required: false,
    login_error_detected: false,
    auth_signals: beforeSignals.auth_signals,
    login_field: null,
    credential_field: null,
    submit: null,
    verify_selector: configuredVerifySelector,
    verify_selector_found: true,
    auth_transition_settled_reason: "already-authenticated",
    before: { url: beforeSignals.url, title: beforeSignals.title, form_state: beforeSignals.form_state, auth_signals: beforeSignals.auth_signals },
    credential_submit_base: null,
    after: { url: beforeSignals.url, title: beforeSignals.title, form_state: beforeSignals.form_state, auth_signals: beforeSignals.auth_signals },
    url_changed: false,
    credential_url_changed: false,
    same_origin_after_login: true,
    observed: { url: observed.url, title: observed.title, document_text: trimText(observed.documentText) },
    elapsed_ms: Date.now() - startedAt,
    redaction: "credential value is not returned",
  };
} else {
let loginField = null;
if (loginHint) {
  loginField = await fillField("login", loginHint);
}
let credentialField = await fillField("credential", credentialValue, configuredCredentialSelector);
let loginEntry = null;
let afterLoginEntrySignals = null;
if (!credentialField && !loginField) {
  loginEntry = await clickLoginEntry();
  if (loginEntry) {
    afterLoginEntrySignals = await waitForLoginEntryTransition(beforeSignals.url, 12000);
    if (loginHint) {
      loginField = await fillField("login", loginHint);
    }
    credentialField = await fillField("credential", credentialValue, configuredCredentialSelector);
  }
}
let loginTransition = null;
let afterLoginStepSignals = null;
if (!credentialField && loginField) {
  loginTransition = await clickSubmit(loginField);
  if (loginTransition) {
    afterLoginStepSignals = await waitForCredentialTransition(beforeSignals.url, 12000);
    credentialField = await fillField("credential", credentialValue, configuredCredentialSelector);
  }
}
if (!credentialField) {
  const observed = await ctoxBrowser.observe({ limit: 40, textMax: 120 });
  const missingFieldSignals = await pageSignals();
  return {
    ok: false,
    reason: loginTransition ? "credential-field-not-found-after-login-transition" : "credential-field-not-found",
    login_state: "credential_field_missing",
    source_id: sourceId,
    target_url: targetUrl,
    credential_ref: credentialRef,
    login_hint_present: !!loginHint,
    login_entry: loginEntry,
    after_login_entry: afterLoginEntrySignals
      ? { url: afterLoginEntrySignals.url, title: afterLoginEntrySignals.title, form_state: afterLoginEntrySignals.form_state, auth_signals: afterLoginEntrySignals.auth_signals }
      : null,
    login_transition: loginTransition,
    consent_transition: consentTransition,
    mfa_required: missingFieldSignals.auth_signals?.mfa_required === true,
    login_error_detected: missingFieldSignals.auth_signals?.login_error_detected === true,
    auth_signals: missingFieldSignals.auth_signals,
    before: { url: beforeSignals.url, title: beforeSignals.title, form_state: beforeSignals.form_state, auth_signals: beforeSignals.auth_signals },
    after_login_step: afterLoginStepSignals
      ? { url: afterLoginStepSignals.url, title: afterLoginStepSignals.title, form_state: afterLoginStepSignals.form_state, auth_signals: afterLoginStepSignals.auth_signals }
      : null,
    observed: { url: observed.url, title: observed.title, document_text: trimText(observed.documentText) },
    redaction: "credential value is not returned",
  };
}
const submit = await clickSubmit(credentialField);
const credentialSubmitBaseSignals = await pageSignals();
const authTransition = await waitForAuthTransition(
  credentialSubmitBaseSignals.url,
  12000,
  configuredVerifySelector,
);
let afterSignals = authTransition.signals;
let verifyFound = configuredVerifySelector ? authTransition.verify_found : null;
const observed = await ctoxBrowser.observe({ limit: 50, textMax: 140 });
const urlChanged = afterSignals.url !== beforeSignals.url;
const credentialUrlChanged = afterSignals.url !== credentialSubmitBaseSignals.url;
const originAfter = (() => { try { return new URL(afterSignals.url).origin; } catch { return null; } })();
const passwordFieldsAfter = Number(afterSignals.form_state?.visible_password_fields || 0);
const formGone = passwordFieldsAfter === 0;
const authSignals = afterSignals.auth_signals || emptyAuthSignals();
const mfaRequired = authSignals.mfa_required === true;
const loginErrorDetected = authSignals.login_error_detected === true;
const verifySelectorMissing = !!configuredVerifySelector && verifyFound !== true;
// A verify selector is a hint, not the only truth. Leadfeeder lands on its
// dashboard and XING on an in-app page, both plainly authenticated, while the
// configured selector no longer matches the current markup — reporting those
// as failed login sent the operator chasing a login that had already worked,
// and each false failure queued another auth-assist request. The substantive
// evidence counts too: we submitted, the credential form is gone, and the page
// has moved off the login URL. MFA and a detected login error still veto.
const substantiveLoginSignal =
  !!submit && (formGone || credentialUrlChanged || originAfter !== targetOrigin);
const baseLoginSignal = configuredVerifySelector
  ? verifyFound === true || substantiveLoginSignal
  : substantiveLoginSignal;
const ok = baseLoginSignal && !mfaRequired && !loginErrorDetected;
const loginState = ok
  ? "authenticated"
  : mfaRequired
    ? "mfa_required"
    : loginErrorDetected
      ? "login_error"
      : verifySelectorMissing
        ? "verify_selector_missing"
        : "inconclusive";
const reason = ok
  ? "login-signals-satisfied"
  : mfaRequired
    ? "mfa-required"
    : loginErrorDetected
      ? "login-error-detected"
      : verifySelectorMissing
        ? "verify-selector-not-found"
        : "login-signals-insufficient";
loginOutcome = {
  ok,
  reason,
  login_state: loginState,
  source_id: sourceId,
  target_url: targetUrl,
  credential_ref: credentialRef,
  login_hint_present: !!loginHint,
  login_transition: loginTransition,
  consent_transition: consentTransition,
  mfa_required: mfaRequired,
  login_error_detected: loginErrorDetected,
  auth_signals: authSignals,
  login_field: loginField,
  credential_field: credentialField,
  submit,
  verify_selector: configuredVerifySelector || null,
  verify_selector_found: verifyFound,
  auth_transition_settled_reason: authTransition.settled_reason,
  before: { url: beforeSignals.url, title: beforeSignals.title, form_state: beforeSignals.form_state, auth_signals: beforeSignals.auth_signals },
  credential_submit_base: { url: credentialSubmitBaseSignals.url, title: credentialSubmitBaseSignals.title, form_state: credentialSubmitBaseSignals.form_state, auth_signals: credentialSubmitBaseSignals.auth_signals },
  after: { url: afterSignals.url, title: afterSignals.title, form_state: afterSignals.form_state, auth_signals: afterSignals.auth_signals },
  url_changed: urlChanged,
  credential_url_changed: credentialUrlChanged,
  same_origin_after_login: originAfter === targetOrigin,
  observed: { url: observed.url, title: observed.title, document_text: trimText(observed.documentText) },
  elapsed_ms: Date.now() - startedAt,
  redaction: "credential value is not returned",
};
}
"#,
    );
    if let Some(continuation_source) = continuation_source {
        source.push_str(
            "\nif (!loginOutcome.ok) return loginOutcome;\nconst postAuthResult = await (async () => {\n",
        );
        source.push_str(continuation_source);
        source.push_str("\n})();\nreturn { ...loginOutcome, post_auth_result: postAuthResult };\n");
    } else {
        source.push_str("\nreturn loginOutcome;\n");
    }
    Ok(source)
}

#[allow(clippy::too_many_arguments)]
fn build_web_stack_auth_assist_signup_source(
    target_url: &str,
    source_id: &str,
    login_hint: &str,
    credential_ref: &str,
    secret_value: &str,
    email_selector: &str,
    credential_selector: &str,
    confirm_credential_selector: &str,
    submit_selector: &str,
    verify_selector: &str,
    accept_terms: bool,
    terms_selector: &str,
    display_name: &str,
    display_name_selector: &str,
    tenant_name: &str,
    tenant_name_selector: &str,
) -> anyhow::Result<String> {
    let mut source = format!(
        "// ctox-browser: timeout_ms=60000\nconst targetUrl = {};\nconst sourceId = {};\nconst loginHint = {};\nconst credentialRef = {};\nconst credentialValue = {};\nconst configuredEmailSelector = {};\nconst configuredCredentialSelector = {};\nconst configuredConfirmCredentialSelector = {};\nconst configuredSubmitSelector = {};\nconst configuredVerifySelector = {};\nconst shouldAcceptTerms = {};\nconst configuredTermsSelector = {};\nconst displayNameHint = {};\nconst configuredDisplayNameSelector = {};\nconst tenantNameHint = {};\nconst configuredTenantNameSelector = {};\n",
        serde_json::to_string(target_url)?,
        serde_json::to_string(source_id)?,
        serde_json::to_string(login_hint)?,
        serde_json::to_string(credential_ref)?,
        serde_json::to_string(secret_value)?,
        serde_json::to_string(email_selector)?,
        serde_json::to_string(credential_selector)?,
        serde_json::to_string(confirm_credential_selector)?,
        serde_json::to_string(submit_selector)?,
        serde_json::to_string(verify_selector)?,
        serde_json::to_string(&accept_terms)?,
        serde_json::to_string(terms_selector)?,
        serde_json::to_string(display_name)?,
        serde_json::to_string(display_name_selector)?,
        serde_json::to_string(tenant_name)?,
        serde_json::to_string(tenant_name_selector)?,
    );
    source.push_str(
        r#"
const targetOrigin = new URL(targetUrl).origin;
const startedAt = Date.now();
const trimText = (value, max = 180) => {
  const text = String(value ?? "").replace(/\s+/g, " ").trim();
  return text.length > max ? text.slice(0, max - 1) + "..." : text;
};
const candidateFields = async (kind) => page.evaluate(({ kind }) => {
  const cssEscape = (value) => globalThis.CSS && typeof globalThis.CSS.escape === "function"
    ? globalThis.CSS.escape(String(value))
    : String(value).replace(/["\\]/g, "\\$&");
  const visible = (element) => {
    const style = globalThis.getComputedStyle(element);
    const box = element.getBoundingClientRect();
    return style.visibility !== "hidden"
      && style.display !== "none"
      && Number(style.opacity || "1") > 0
      && box.width > 0
      && box.height > 0
      && !element.disabled
      && element.getAttribute("aria-hidden") !== "true";
  };
  const labelFor = (element) => {
    const labels = [];
    if (element.id) {
      for (const label of Array.from(document.querySelectorAll(`label[for="${cssEscape(element.id)}"]`))) {
        labels.push(label.innerText || label.textContent || "");
      }
    }
    const parentLabel = element.closest("label");
    if (parentLabel) labels.push(parentLabel.innerText || parentLabel.textContent || "");
    return labels.join(" ");
  };
  const descriptorFor = (element, source) => {
    const tag = element.tagName.toLowerCase();
    const id = element.getAttribute("id");
    const name = element.getAttribute("name");
    const type = element.getAttribute("type");
    const placeholder = element.getAttribute("placeholder");
    let selector = null;
    if (id) selector = `#${cssEscape(id)}`;
    else if (name) selector = `${tag}[name="${cssEscape(name)}"]`;
    else if (placeholder) selector = `${tag}[placeholder="${cssEscape(placeholder)}"]`;
    else if (type) selector = `${tag}[type="${cssEscape(type)}"]`;
    else selector = tag;
    let index = 0;
    try {
      const matches = Array.from(document.querySelectorAll(selector));
      index = Math.max(0, matches.indexOf(element));
    } catch {}
    return { selector, index, source, tag, type: type || null, name: name || null, autocomplete: element.getAttribute("autocomplete") || null, placeholder_present: !!placeholder };
  };
  const tokensFor = (element) => [
    element.getAttribute("type"),
    element.getAttribute("name"),
    element.getAttribute("id"),
    element.getAttribute("autocomplete"),
    element.getAttribute("placeholder"),
    element.getAttribute("aria-label"),
    labelFor(element),
  ].filter(Boolean).join(" ").toLowerCase();
  const scoreFor = (element) => {
    const tokens = tokensFor(element);
    const type = String(element.getAttribute("type") || "").toLowerCase();
    if (kind === "credential" || kind === "confirm_credential") {
      let score = type === "password" ? 100 : 0;
      if (/(password|passwort|passwd|pwd|kennwort)/.test(tokens)) score += 80;
      if (kind === "confirm_credential" && /(confirm|repeat|again|bestaetig|bestätig|wiederhol)/.test(tokens)) score += 70;
      if (kind === "credential" && /(confirm|repeat|again|bestaetig|bestätig|wiederhol)/.test(tokens)) score -= 50;
      return score;
    }
    if (kind === "display_name") {
      let score = 0;
      if (/(name|display|full name|fullname|company|firma|organisation|organization)/.test(tokens)) score += 80;
      if (type === "password" || type === "email") score -= 80;
      return score;
    }
    let score = 0;
    if (type === "email") score += 100;
    if (/(email|e-mail|mail|username|user name|login|account|benutzer|nutzer)/.test(tokens)) score += 80;
    if (type === "password" || /(password|passwort|passwd|pwd|kennwort)/.test(tokens)) score -= 100;
    return score;
  };
  const fields = Array.from(document.querySelectorAll("input, textarea, [contenteditable='true']"))
    .filter(visible)
    .filter((element) => {
      const type = String(element.getAttribute("type") || "").toLowerCase();
      return !["hidden", "submit", "button", "checkbox", "radio", "file"].includes(type);
    })
    .map((element) => ({ element, score: scoreFor(element) }))
    .filter((entry) => entry.score > 0)
    .sort((left, right) => right.score - left.score);
  return fields.slice(0, 8).map((entry) => descriptorFor(entry.element, "heuristic"));
}, { kind });
const fillField = async (kind, value, configuredSelector = "") => {
  if (!value) return null;
  const candidates = [];
  if (configuredSelector) candidates.push({ selector: configuredSelector, index: 0, source: "configured", configured: true });
  candidates.push(...await candidateFields(kind));
  for (const candidate of candidates) {
    if (!candidate.selector) continue;
    try {
      const locator = page.locator(candidate.selector).nth(Number(candidate.index || 0));
      if ((await locator.count()) < 1) continue;
      await locator.fill(String(value), { timeout: 3500 });
      return { ...candidate, index: Number(candidate.index || 0), configured: !!candidate.configured };
    } catch {}
  }
  return null;
};
const clickTerms = async () => {
  if (!shouldAcceptTerms) return null;
  const selectors = [];
  if (configuredTermsSelector) selectors.push(configuredTermsSelector);
  selectors.push(
    "input[type='checkbox'][name*='terms' i]",
    "input[type='checkbox'][id*='terms' i]",
    "input[type='checkbox'][name*='privacy' i]",
    "input[type='checkbox'][id*='privacy' i]",
    "label:has-text('Terms')",
    "label:has-text('Privacy')",
    "label:has-text('AGB')",
    "label:has-text('Datenschutz')"
  );
  for (const selector of selectors) {
    try {
      const locator = page.locator(selector).first();
      if ((await locator.count()) < 1) continue;
      const tag = await locator.evaluate((element) => element.tagName.toLowerCase()).catch(() => "");
      if (tag === "input") await locator.check({ timeout: 3500 }).catch(async () => locator.click({ timeout: 3500 }));
      else await locator.click({ timeout: 3500 });
      return { selector, mode: "terms-accepted" };
    } catch {}
  }
  return { mode: "terms-not-found" };
};
const clickSubmit = async () => {
  const selectors = [];
  if (configuredSubmitSelector) selectors.push(configuredSubmitSelector);
  selectors.push(
    "button[type='submit']",
    "input[type='submit']",
    "button:has-text('Create account')",
    "button:has-text('Sign up')",
    "button:has-text('Register')",
    "button:has-text('Get started')",
    "button:has-text('Continue')",
    "button:has-text('Konto erstellen')",
    "button:has-text('Registrieren')",
    "button:has-text('Anmelden')",
    "button:has-text('Weiter')",
    "[role='button']:has-text('Create account')",
    "[role='button']:has-text('Sign up')",
    "[role='button']:has-text('Register')",
    "[role='button']:has-text('Registrieren')"
  );
  for (const selector of selectors) {
    try {
      const locator = page.locator(selector).first();
      if ((await locator.count()) < 1) continue;
      await locator.click({ timeout: 3500 });
      return { mode: "click", selector };
    } catch {}
  }
  try {
    await page.keyboard.press("Enter");
    return { mode: "press", key: "Enter" };
  } catch {}
  return null;
};
const pageSignals = async () => {
  let title = "";
  try { title = await page.title(); } catch {}
  try {
    const pageState = await page.evaluate(() => {
      const visible = (element) => {
        const style = globalThis.getComputedStyle(element);
        const box = element.getBoundingClientRect();
        return style.visibility !== "hidden"
          && style.display !== "none"
          && Number(style.opacity || "1") > 0
          && box.width > 0
          && box.height > 0;
      };
      const text = String(document.body ? document.body.innerText || "" : "").replace(/\s+/g, " ").trim();
      const lower = text.toLowerCase();
      const terms = (entries) => entries.filter((entry) => entry.pattern.test(lower)).map((entry) => entry.term);
      const verificationTerms = terms([
        { term: "verify-email", pattern: /verify\s+(your\s+)?email|verification\s+email|confirm\s+(your\s+)?email/ },
        { term: "check-email", pattern: /check\s+(your\s+)?email|sent\s+.*email/ },
        { term: "bestaetigung", pattern: /bestaetig|bestätig|verifizier|e-?mail\s+gesendet/ },
      ]);
      const existingTerms = terms([
        { term: "already-registered", pattern: /already\s+(registered|exists|have\s+an\s+account)/ },
        { term: "account-exists", pattern: /account\s+(already\s+)?exists|user\s+(already\s+)?exists/ },
        { term: "konto-vorhanden", pattern: /konto\s+(existiert|vorhanden)|bereits\s+(registriert|angemeldet)/ },
      ]);
      const errorTerms = terms([
        { term: "invalid", pattern: /\binvalid\b/ },
        { term: "weak", pattern: /weak\s+password|password\s+too\s+short|too\s+weak/ },
        { term: "required", pattern: /\brequired\b|missing\s+field/ },
        { term: "failed", pattern: /sign\s+up\s+failed|registration\s+failed|could\s+not\s+create/ },
        { term: "ungueltig", pattern: /ungueltig|ungültig/ },
        { term: "fehlgeschlagen", pattern: /registrierung\s+fehlgeschlagen|konto.*nicht.*erstellt/ },
      ]);
      const errorNodes = Array.from(document.querySelectorAll([
        "[role='alert']",
        "[data-testid*='error' i]",
        "[class*='error' i]",
        "[id*='error' i]",
      ].join(",")))
        .filter(visible)
        .map((element) => String(element.innerText || element.textContent || "").replace(/\s+/g, " ").trim())
        .filter(Boolean);
      return {
        visible_text_sample: text.slice(0, 240),
        form_state: {
          visible_password_fields: Array.from(document.querySelectorAll("input[type='password']")).filter(visible).length,
          visible_email_fields: Array.from(document.querySelectorAll("input[type='email']")).filter(visible).length,
          visible_forms: Array.from(document.querySelectorAll("form")).filter(visible).length,
        },
        signup_signals: {
          verification_required: verificationTerms.length > 0,
          already_registered: existingTerms.length > 0,
          signup_error_detected: (errorTerms.length > 0 || errorNodes.length > 0) && existingTerms.length === 0,
          verification_terms: verificationTerms.slice(0, 8),
          existing_account_terms: existingTerms.slice(0, 8),
          error_terms: errorTerms.slice(0, 8),
          error_text: (errorNodes.join(" ") || "").slice(0, 240),
        },
      };
    });
    return { url: page.url(), title, form_state: pageState.form_state || {}, signup_signals: pageState.signup_signals || {}, visible_text_sample: pageState.visible_text_sample || "" };
  } catch {
    return { url: page.url(), title, form_state: {}, signup_signals: {}, visible_text_sample: "" };
  }
};
const waitForTransition = async (previousUrl, timeoutMs = 15000) => {
  await Promise.race([
    page.waitForLoadState("networkidle", { timeout: timeoutMs }).catch(() => null),
    page.waitForURL((url) => String(url) !== previousUrl, { timeout: timeoutMs }).catch(() => null),
    page.waitForTimeout(1500).catch(() => null),
  ]).catch(() => null);
  await page.waitForTimeout(500).catch(() => null);
  return pageSignals();
};

const before = await ctoxBrowser.goto(targetUrl, { waitUntil: "domcontentloaded", timeoutMs: 30000, limit: 80, textMax: 120 });
await page.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => null);
const beforeSignals = await pageSignals();
const emailField = await fillField("login", loginHint, configuredEmailSelector);
const credentialField = await fillField("credential", credentialValue, configuredCredentialSelector);
let confirmCredentialField = null;
if (configuredConfirmCredentialSelector) {
  confirmCredentialField = await fillField("confirm_credential", credentialValue, configuredConfirmCredentialSelector);
} else {
  confirmCredentialField = await fillField("confirm_credential", credentialValue, "");
}
const displayNameField = displayNameHint ? await fillField("display_name", displayNameHint, configuredDisplayNameSelector) : null;
const tenantNameField = tenantNameHint ? await fillField("display_name", tenantNameHint, configuredTenantNameSelector) : null;
const terms = await clickTerms();
if (!emailField || !credentialField) {
  const observed = await ctoxBrowser.observe({ limit: 40, textMax: 120 });
  const signals = await pageSignals();
  return {
    ok: false,
    reason: !emailField ? "signup-email-field-not-found" : "signup-credential-field-not-found",
    signup_state: "signup_field_missing",
    source_id: sourceId,
    target_url: targetUrl,
    credential_ref: credentialRef,
    login_hint_present: !!loginHint,
    before: { url: beforeSignals.url, title: beforeSignals.title, form_state: beforeSignals.form_state, signup_signals: beforeSignals.signup_signals },
    after: { url: signals.url, title: signals.title, form_state: signals.form_state, signup_signals: signals.signup_signals },
    observed: { url: observed.url, title: observed.title, document_text: trimText(observed.documentText) },
    redaction: "credential value is not returned",
  };
}
const submit = await clickSubmit();
const submitBaseSignals = await pageSignals();
const afterSignals = await waitForTransition(submitBaseSignals.url, 15000);
let verifyFound = null;
if (configuredVerifySelector) {
  verifyFound = await page.locator(configuredVerifySelector).first().isVisible({ timeout: 3500 }).catch(() => false);
}
const observed = await ctoxBrowser.observe({ limit: 50, textMax: 140 });
const signals = afterSignals.signup_signals || {};
const verificationRequired = signals.verification_required === true;
const alreadyRegistered = signals.already_registered === true;
const signupErrorDetected = signals.signup_error_detected === true;
const passwordFieldsAfter = Number(afterSignals.form_state?.visible_password_fields || 0);
const formGone = passwordFieldsAfter === 0;
const submitUrlChanged = afterSignals.url !== submitBaseSignals.url;
const originAfter = (() => { try { return new URL(afterSignals.url).origin; } catch { return null; } })();
const baseProvisioningSignal = configuredVerifySelector
  ? verifyFound === true
  : !!submit && (formGone || submitUrlChanged || originAfter !== targetOrigin);
const ok = alreadyRegistered || (baseProvisioningSignal && !verificationRequired && !signupErrorDetected);
const signupState = alreadyRegistered
  ? "already_registered"
  : ok
    ? "provisioned"
    : verificationRequired
      ? "verification_required"
      : signupErrorDetected
        ? "signup_error"
        : "inconclusive";
const reason = alreadyRegistered
  ? "account-already-registered"
  : ok
    ? "signup-signals-satisfied"
    : verificationRequired
      ? "verification-required"
      : signupErrorDetected
        ? "signup-error-detected"
        : "signup-signals-insufficient";
return {
  ok,
  reason,
  signup_state: signupState,
  source_id: sourceId,
  target_url: targetUrl,
  credential_ref: credentialRef,
  login_hint_present: !!loginHint,
  email_field: emailField,
  credential_field: credentialField,
  confirm_credential_field: confirmCredentialField,
  display_name_field: displayNameField,
  tenant_name_field: tenantNameField,
  terms,
  submit,
  verify_selector: configuredVerifySelector || null,
  verify_selector_found: verifyFound,
  verification_required: verificationRequired,
  already_registered: alreadyRegistered,
  signup_error_detected: signupErrorDetected,
  signup_signals: signals,
  before: { url: beforeSignals.url, title: beforeSignals.title, form_state: beforeSignals.form_state, signup_signals: beforeSignals.signup_signals },
  submit_base: { url: submitBaseSignals.url, title: submitBaseSignals.title, form_state: submitBaseSignals.form_state, signup_signals: submitBaseSignals.signup_signals },
  after: { url: afterSignals.url, title: afterSignals.title, form_state: afterSignals.form_state, signup_signals: afterSignals.signup_signals },
  submit_url_changed: submitUrlChanged,
  same_origin_after_signup: originAfter === targetOrigin,
  observed: { url: observed.url, title: observed.title, document_text: trimText(observed.documentText) },
  elapsed_ms: Date.now() - startedAt,
  redaction: "credential value is not returned",
};
"#,
    );
    Ok(source)
}

fn redact_secret_value_from_json(value: &mut serde_json::Value, secret_value: &str) {
    if secret_value.is_empty() {
        return;
    }
    match value {
        serde_json::Value::String(text) => {
            if text.contains(secret_value) {
                *text = text.replace(secret_value, "<redacted-credential-value>");
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_secret_value_from_json(item, secret_value);
            }
        }
        serde_json::Value::Object(object) => {
            for child in object.values_mut() {
                redact_secret_value_from_json(child, secret_value);
            }
        }
        _ => {}
    }
}

fn available_modules() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut modules = CORE_MODULES
        .iter()
        .map(|(id, label)| (*id, *label, "core"))
        .collect::<Vec<_>>();
    let mut skill_modules = BTreeSet::new();
    for binding in SKILL_APP_BINDINGS {
        skill_modules.insert(binding.module_id);
    }
    for module_id in skill_modules {
        modules.push((module_id, module_label(module_id), "skill_app"));
    }
    modules
}

fn module_label(module_id: &str) -> &'static str {
    match module_id {
        "documents" => "Documents",
        "content" => "Content Studio",
        "developer" => "Developer Studio",
        "deployment" => "Deployment",
        "security" => "Security",
        "integrations" => "Integration Hub",
        "research" => "Research Desk",
        "support" => "Support Desk",
        _ => "Unknown",
    }
}

fn load_activation(root: &Path) -> anyhow::Result<BusinessOsActivation> {
    let mut activation =
        persistence::load_json_payload::<BusinessOsActivation>(root, ACTIVATION_PAYLOAD_KEY)?
            .unwrap_or_else(default_activation);
    normalize_activation(&mut activation);
    Ok(activation)
}

fn save_activation(root: &Path, activation: &BusinessOsActivation) -> anyhow::Result<()> {
    persistence::store_json_payload(root, ACTIVATION_PAYLOAD_KEY, Some(activation))
}

fn default_activation() -> BusinessOsActivation {
    BusinessOsActivation {
        schema_version: 1,
        enabled_modules: CORE_MODULES
            .iter()
            .map(|(id, _)| (*id).to_string())
            .collect(),
        enabled_skills: Vec::new(),
    }
}

fn normalize_activation(activation: &mut BusinessOsActivation) {
    activation.schema_version = 1;
    for (module_id, _) in CORE_MODULES {
        activation_add(&mut activation.enabled_modules, module_id);
    }
    activation.enabled_modules.sort();
    activation.enabled_modules.dedup();
    activation.enabled_skills.sort();
    activation.enabled_skills.dedup();
}

fn activation_add(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|item| item == value) {
        values.push(value.to_string());
    }
}

fn is_core_module(module_id: &str) -> bool {
    CORE_MODULES.iter().any(|(id, _)| *id == module_id)
}

fn ensure_known_skill_module(module_id: &str) -> anyhow::Result<()> {
    if SKILL_APP_BINDINGS
        .iter()
        .any(|binding| binding.module_id == module_id)
    {
        Ok(())
    } else {
        anyhow::bail!("unknown Business OS skill module: {module_id}")
    }
}

fn find_skill_binding(skill_id: &str) -> anyhow::Result<&'static SkillAppBinding> {
    SKILL_APP_BINDINGS
        .iter()
        .find(|binding| binding.skill_id == skill_id)
        .with_context(|| format!("unknown Business OS packed skill: {skill_id}"))
}

fn enable_module_skills(
    root: &Path,
    activation: &mut BusinessOsActivation,
    module_id: &str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut installed = Vec::new();
    for binding in SKILL_APP_BINDINGS
        .iter()
        .filter(|binding| binding.module_id == module_id)
    {
        activation_add(&mut activation.enabled_skills, binding.skill_id);
        let path = skill_store::install_source_pack(root, binding.skill_id)?;
        installed.push(serde_json::json!({
            "skill_id": binding.skill_id,
            "path": path
        }));
    }
    skill_store::bootstrap_from_roots(root)?;
    Ok(installed)
}

fn disable_module_skills(
    root: &Path,
    activation: &mut BusinessOsActivation,
    module_id: &str,
    force: bool,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut removed = Vec::new();
    for binding in SKILL_APP_BINDINGS
        .iter()
        .filter(|binding| binding.module_id == module_id)
    {
        activation
            .enabled_skills
            .retain(|skill| skill != binding.skill_id);
        let result = skill_store::remove_installed_source_pack(root, binding.skill_id, force)?;
        removed.push(serde_json::to_value(result)?);
    }
    Ok(removed)
}

fn print_json(value: &serde_json::Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_os_usage_documents_atomic_authenticated_automation() {
        let usage = business_os_usage();
        assert!(
            usage.contains("ctox business-os web-stack authenticated-automation --source-id <id>")
        );
        assert!(usage.contains("< automation.js"));
        assert!(usage.contains(
            "source-capture --source-id <dnbhoovers.com|leadfeeder.com|rocketreach.com|xing.com>"
        ));
        assert!(usage.contains("[--credential-ref <ctox-secret://scope/name>]"));
    }

    #[test]
    fn business_os_usage_documents_terminal_app_create_retry() {
        assert!(business_os_usage().contains("ctox business-os commands retry <command-id>"));
    }

    #[test]
    fn rxdb_status_includes_production_readiness_contract() {
        let status = enrich_rxdb_peer_status_with_production_readiness(serde_json::json!({
            "running": true,
            "replicationUp": true,
            "heartbeat": {
                "fresh": true,
            },
            "health": {
                "errorTotal": 0,
            },
            "health_stages": {
                "command_consumer_alive": true,
                "turn_credential_ready": true,
                "projection_outbox": 0,
            },
            "criticalTasks": [
                { "name": "command-consumer", "alive": true }
            ],
            "circuitBreaker": {
                "state": "closed",
            },
            "command_plane": {
                "pending_sync_count": 0,
                "oldest_pending_age_ms": null,
            },
        }));
        assert_eq!(
            status
                .pointer("/productionReadiness/schema")
                .and_then(serde_json::Value::as_str),
            Some("ctox.sync.production_readiness_95.status.v1")
        );
        assert_eq!(
            status
                .pointer("/productionReadiness/sloTargets/wanReplicationP95Ms")
                .and_then(serde_json::Value::as_u64),
            Some(5_000)
        );
        assert_eq!(
            status
                .pointer("/productionReadiness/releaseGates/fullMatrixMinimumModes")
                .and_then(serde_json::Value::as_u64),
            Some(40)
        );
        assert!(status
            .pointer("/productionReadiness/blockers")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|blockers| blockers
                .iter()
                .any(|blocker| blocker == "missing_evidence:canary_72h")));
        assert_eq!(
            status
                .pointer("/productionReadiness/evidenceArtifacts/wanTurnMatrix")
                .and_then(serde_json::Value::as_str),
            Some("runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json")
        );
        assert_eq!(
            status
                .pointer("/productionReadiness/evidenceArtifacts/templateCatalog")
                .and_then(serde_json::Value::as_str),
            Some("node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js")
        );
        assert!(status
            .pointer("/productionReadiness/missingEvidence")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|evidence| evidence
                .iter()
                .any(|entry| entry == "app_runtime_package_gate")));
        let rendered = render_rxdb_peer_status_text(&status);
        assert!(rendered.contains("Production readiness 9.5:"));
    }

    #[test]
    fn web_stack_redaction_audit_reports_canary_without_echoing_value() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime_dir = root.path().join("runtime");
        fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let canary = "ctox-redaction-canary-secret";
        fs::write(
            runtime_dir.join("ctox.sqlite3"),
            format!("prefix {canary} suffix"),
        )
        .expect("write canary file");
        fs::write(runtime_dir.join("clean.log"), "clean").expect("write clean file");

        let args = vec![
            "redaction-audit".to_string(),
            "--canary".to_string(),
            canary.to_string(),
            "--path".to_string(),
            "runtime".to_string(),
        ];
        let report = run_web_stack_redaction_audit(root.path(), &args).expect("audit");
        assert_eq!(
            report.get("ok").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            report
                .get("findings")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        let serialized = serde_json::to_string(&report).expect("serialize report");
        assert!(
            !serialized.contains(canary),
            "audit report must not echo canary values"
        );
        assert_eq!(
            report
                .pointer("/secret_value_in_payload")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            report
                .pointer("/frame_data_in_payload")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn web_stack_auth_owner_resolution_prefers_verified_task_over_flag_and_env(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let auth_assist = enqueue_web_stack_auth_assist_request(
            root.path(),
            "leadfeeder.com",
            Some("https://app.leadfeeder.com/"),
            Some("ctox-secret://credentials/LEADFEEDER_BROWSER_LOGIN"),
            None,
            None,
            "owner-resolution-request",
            "ctox_harness",
            "ctox_web_auth_assist_request",
            Some("task-owner"),
            false,
            true,
        )?;
        crate::business_os::store::deliver_business_command_outbox(root.path(), 16)?;
        let task_id = auth_assist
            .get("task_id")
            .and_then(serde_json::Value::as_str)
            .context("auth-assist task id")?;
        let command_id = auth_assist
            .get("command_id")
            .and_then(serde_json::Value::as_str)
            .context("auth-assist command id")?;
        let flag_args = vec!["--owner-user-id".to_string(), "flag-owner".to_string()];
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &flag_args,
                task_id,
                Some("env-owner"),
                true,
            )?
            .as_deref(),
            Some("task-owner")
        );
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &[],
                task_id,
                Some("env-owner"),
                true,
            )?
            .as_deref(),
            Some("task-owner")
        );
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &flag_args,
                task_id,
                Some("env-owner"),
                false,
            )?
            .as_deref(),
            Some("task-owner")
        );
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(root.path(), &[], task_id, None, true)?
                .as_deref(),
            Some("task-owner")
        );
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &[],
                command_id,
                None,
                true,
            )?,
            None
        );
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &[],
                "missing-task-or-command",
                None,
                true,
            )?,
            None
        );
        Ok(())
    }

    #[test]
    fn web_stack_auth_owner_resolution_uses_command_authorization_chat_and_session(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let command_id = "leadgen-lead-research-fd2769a3";
        let (capability_token, _) =
            crate::business_os::store::issue_business_os_capability_token_for_managed_user(
                root.path(),
                "michael.welsch@metric-space.ai",
                "Michael Welsch",
                "admin",
                chrono::Utc::now().timestamp_millis(),
            )?;
        let accepted = crate::business_os::store::accept_rxdb_business_command_with_origin(
            root.path(),
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "company-kuka",
                "payload": {
                    "title": "KUKA research",
                    "instruction": "Research KUKA",
                    "writeback_contract": { "allowed_collections": [] }
                },
                "client_context": {
                    "actor": { "id": "forged" },
                    "capability_token": capability_token
                }
            }),
            crate::business_os::store::CommandOrigin::ReplicatedPeer,
        )?;
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &[],
                command_id,
                None,
                false,
            )?
            .as_deref(),
            Some("michael.welsch@metric-space.ai")
        );

        crate::business_os::store::upsert_projection_record(
            root.path(),
            "business_chats",
            "chat-kuka",
            now_ms() as i64,
            serde_json::json!({
                "id": "chat-kuka",
                "owner_user_id": "chat-owner@metric-space.ai"
            }),
        )?;
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &[],
                "chat-kuka",
                None,
                false,
            )?
            .as_deref(),
            Some("chat-owner@metric-space.ai")
        );
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &[],
                "KUKA Deutschland GmbH",
                None,
                false,
            )?,
            None
        );

        let canonical = channels::business_command_projection(root.path(), command_id)?;
        let command_session =
            crate::business_os::mcp_channel::issue_internal_command_session_token(
                root.path(),
                command_id,
                canonical
                    .get("payload_hash")
                    .and_then(serde_json::Value::as_str)
                    .context("payload hash")?,
                "michael.welsch@metric-space.ai",
                "admin",
                "research",
                &serde_json::json!({ "allowed_collections": [] }),
            )?;
        let session_args = vec!["--command-session".to_string(), command_session];
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &session_args,
                "unrelated model text",
                None,
                false,
            )?
            .as_deref(),
            Some("michael.welsch@metric-space.ai")
        );
        Ok(())
    }

    #[test]
    fn source_capture_auth_assist_requires_owner_and_uses_explicit_trusted_owner(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let unbound = vec![
            "source-capture".to_string(),
            "--source-id".to_string(),
            "leadfeeder.com".to_string(),
            "--task-id".to_string(),
            "KUKA Deutschland GmbH".to_string(),
        ];
        let error = enqueue_web_stack_source_capture_auth_assist(
            root.path(),
            &unbound,
            "leadfeeder.com",
            None,
            false,
        )
        .expect_err("unverified source capture must fail")
        .to_string();
        assert_eq!(
            error,
            "auth assist owner unresolved (task=KUKA Deutschland GmbH, source_module=ctox_web_source_capture)"
        );

        let trusted = vec![
            "source-capture".to_string(),
            "--source-id".to_string(),
            "leadfeeder.com".to_string(),
            "--task-id".to_string(),
            "leadgen-lead-research-fd2769a3".to_string(),
            "--owner-user-id".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        ];
        let auth_assist = enqueue_web_stack_source_capture_auth_assist(
            root.path(),
            &trusted,
            "leadfeeder.com",
            None,
            true,
        )?;
        assert_eq!(
            auth_assist
                .get("owner_user_id")
                .and_then(serde_json::Value::as_str),
            Some("michael.welsch@metric-space.ai")
        );
        Ok(())
    }

    #[test]
    fn web_stack_auth_assist_reuses_active_task_across_request_ids() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let first = enqueue_web_stack_auth_assist_request(
            root.path(),
            "leadfeeder.com",
            Some("https://app.leadfeeder.com/"),
            Some("ctox-secret://credentials/LEADFEEDER_BROWSER_LOGIN"),
            None,
            None,
            "request-one",
            "ctox_harness",
            "ctox_web_auth_assist_request",
            Some("user-a"),
            false,
            true,
        )?;
        crate::business_os::store::deliver_business_command_outbox(root.path(), 16)?;
        let first_task_id = first
            .get("task_id")
            .and_then(serde_json::Value::as_str)
            .context("first auth-assist task id")?;
        let owner_args = vec!["--task-id".to_string(), first_task_id.to_string()];
        assert_eq!(
            resolve_web_stack_auth_owner_user_id_with_env(
                root.path(),
                &owner_args,
                first_task_id,
                None,
                true,
            )?
            .as_deref(),
            Some("user-a")
        );
        let second = enqueue_web_stack_auth_assist_request(
            root.path(),
            "leadfeeder.com",
            Some("https://app.leadfeeder.com/"),
            Some("ctox-secret://credentials/LEADFEEDER_BROWSER_LOGIN"),
            None,
            None,
            "request-two",
            "ctox_harness",
            "ctox_web_auth_assist_request",
            Some("user-a"),
            false,
            true,
        )?;

        assert_eq!(first.get("command_id"), second.get("command_id"));
        assert_eq!(first.get("task_id"), second.get("task_id"));
        assert_eq!(first.get("session_id"), second.get("session_id"));
        assert!(first
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|session_id| {
                session_id.contains("leadfeeder_com_user_a") && !session_id.contains("request_one")
            }));
        assert_eq!(
            second
                .get("owner_user_id")
                .and_then(serde_json::Value::as_str),
            Some("user-a")
        );
        assert_eq!(
            second
                .get("deduped_by_active_auth_assist")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            second
                .get("route_status")
                .and_then(serde_json::Value::as_str),
            Some("pending")
        );
        let other_owner = enqueue_web_stack_auth_assist_request(
            root.path(),
            "leadfeeder.com",
            Some("https://app.leadfeeder.com/"),
            Some("ctox-secret://credentials/LEADFEEDER_BROWSER_LOGIN"),
            None,
            None,
            "request-three",
            "ctox_harness",
            "ctox_web_auth_assist_request",
            Some("user-b"),
            false,
            true,
        )?;
        assert_ne!(first.get("command_id"), other_owner.get("command_id"));
        assert_ne!(first.get("session_id"), other_owner.get("session_id"));
        assert_eq!(
            other_owner
                .get("owner_user_id")
                .and_then(serde_json::Value::as_str),
            Some("user-b")
        );
        Ok(())
    }

    #[test]
    fn web_stack_auth_assist_reuses_queued_task_after_handoff_window_expires() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        let first = enqueue_web_stack_auth_assist_request(
            root.path(),
            "leadfeeder.com",
            Some("https://app.leadfeeder.com/"),
            Some("ctox-secret://credentials/LEADFEEDER_BROWSER_LOGIN"),
            None,
            None,
            "expired-request",
            "ctox_harness",
            "ctox_web_auth_assist_request",
            Some("user-a"),
            false,
            true,
        )?;
        let first_command_id = first
            .get("command_id")
            .and_then(serde_json::Value::as_str)
            .expect("first command id");
        crate::business_os::store::deliver_business_command_outbox(root.path(), 16)?;
        let conn = crate::business_os::store::open_store(root.path())?;
        let payload_json: String = conn.query_row(
            "SELECT payload_json FROM business_commands WHERE command_id = ?1",
            [first_command_id],
            |row| row.get(0),
        )?;
        let mut payload: serde_json::Value = serde_json::from_str(&payload_json)?;
        payload["expires_at_ms"] = serde_json::json!(1);
        conn.execute(
            "UPDATE business_commands SET payload_json = ?1 WHERE command_id = ?2",
            [
                serde_json::to_string(&payload)?,
                first_command_id.to_string(),
            ],
        )?;
        drop(conn);

        let second = enqueue_web_stack_auth_assist_request(
            root.path(),
            "leadfeeder.com",
            Some("https://app.leadfeeder.com/"),
            Some("ctox-secret://credentials/LEADFEEDER_BROWSER_LOGIN"),
            None,
            None,
            "fresh-request",
            "ctox_harness",
            "ctox_web_auth_assist_request",
            Some("user-a"),
            false,
            true,
        )?;

        assert_eq!(first.get("command_id"), second.get("command_id"));
        assert_eq!(first.get("task_id"), second.get("task_id"));
        assert_eq!(
            second
                .get("deduped_by_active_auth_assist")
                .and_then(serde_json::Value::as_bool),
            Some(true),
        );
        Ok(())
    }

    #[test]
    fn web_stack_person_research_resolves_relative_workspace_against_root() {
        let root = Path::new("/srv/ctox/current");
        let relative = vec![
            "person-research".to_string(),
            "--workspace".to_string(),
            "runtime/research/person/example".to_string(),
        ];
        assert_eq!(
            business_os_web_stack_workspace(root, &relative),
            Some(root.join("runtime/research/person/example"))
        );

        let absolute = vec![
            "person-research".to_string(),
            "--workspace".to_string(),
            "/var/lib/ctox/research/example".to_string(),
        ];
        assert_eq!(
            business_os_web_stack_workspace(root, &absolute),
            Some(PathBuf::from("/var/lib/ctox/research/example"))
        );
    }

    #[test]
    fn web_stack_auth_assist_login_source_classifies_login_states() -> anyhow::Result<()> {
        let source = build_web_stack_auth_assist_login_source_with_continuation(
            "https://example.test/login",
            "example-source",
            Some("user@example.test"),
            "ctox-secret://appsec/user-a",
            "secret-value",
            "input[name='password']",
            "[data-testid='account-home']",
            None,
        )?;

        assert!(source.contains("login_state"));
        assert!(source.contains("mfa_required"));
        assert!(source.contains("login_error_detected"));
        assert!(source.contains("verify_selector_missing"));
        assert!(source.contains("mfa-required"));
        assert!(source.contains("login-error-detected"));
        assert!(source.contains("verify-selector-not-found"));
        assert!(source.contains("waitForAuthTransition"));
        assert!(source.contains("login_transition"));
        assert!(source.contains("clickLoginEntry"));
        assert!(source.contains("const dismissConsent = async () =>"));
        assert!(source.contains("button#onetrust-accept-btn-handler"));
        assert!(source.contains("waitForLoginEntryTransition"));
        assert!(source.contains("waitForCredentialTransition"));
        assert!(source.contains("browserCandidateFields(\"credential\")"));
        assert!(source.contains("gotoTargetWithRetry"));
        assert!(source.contains("attempt <= 2"));
        assert!(source.contains("same-origin-link"));
        assert!(source.contains("after_login_entry"));
        assert!(source.contains("parsed.origin !== targetOrigin"));
        assert!(source.contains("credential-field-not-found-after-login-transition"));
        assert!(source.contains("scope: \"field-form\""));
        assert!(source.contains("xpath=ancestor::form[1]"));
        assert!(source.contains("credentialSubmitBaseSignals"));
        assert!(source.contains("credential_url_changed"));
        assert!(source.contains("while (Date.now() < deadline)"));
        assert!(source.contains("verify-selector-visible"));
        assert!(source.contains("auth-terminal-signal"));
        assert!(source.contains("transientDocument"));
        assert!(source.contains("auth_transition_settled_reason"));
        assert!(source.contains("lowerText.normalize(\"NFKD\")"));
        assert!(source.contains("ungultig|ungueltig"));
        assert!(source.contains("identifiants-incorrects"));
        assert!(source.contains("credenciales-incorrectas"));
        assert!(source.contains("credenziali-non-valide"));
        assert!(source.contains("inloggegevens-onjuist"));
        assert!(source.contains("nieprawidlowe-dane"));
        assert!(source.contains("baseLoginSignal && !mfaRequired && !loginErrorDetected"));

        Ok(())
    }

    #[test]
    fn web_stack_credential_bundle_keeps_login_hint_inside_secret_boundary() {
        let bundled = resolve_web_stack_credential(
            r#"{"username":"user@example.test","password":"secret-value"}"#,
            None,
        );
        assert_eq!(bundled.credential_value, "secret-value");
        assert_eq!(bundled.login_hint.as_deref(), Some("user@example.test"));
        assert!(bundled.login_hint_from_secret);

        let explicit = resolve_web_stack_credential(
            r#"{"email":"bundled@example.test","secret":"secret-value"}"#,
            Some("operator@example.test".to_string()),
        );
        assert_eq!(
            explicit.login_hint.as_deref(),
            Some("operator@example.test")
        );
        assert!(!explicit.login_hint_from_secret);

        let legacy = resolve_web_stack_credential("legacy-password", None);
        assert_eq!(legacy.credential_value, "legacy-password");
        assert_eq!(legacy.login_hint, None);
        assert!(!legacy.login_hint_from_secret);
    }

    #[test]
    fn web_stack_authenticated_automation_runs_continuation_after_login_gate() -> anyhow::Result<()>
    {
        let source = build_web_stack_auth_assist_login_source_with_continuation(
            "https://example.test/login",
            "example-source",
            Some("user@example.test"),
            "ctox-secret://appsec/user-a",
            "secret-value",
            "input[name='password']",
            "[data-testid='account-home']",
            Some("return { ok: true, task_type: 'same-origin-api-map' };"),
        )?;

        assert!(source.contains("let loginOutcome = null"));
        assert!(source.contains("preAuthenticatedVerifyFound"));
        assert!(source.contains("already-authenticated"));
        assert!(source.contains("if (!loginOutcome.ok) return loginOutcome"));
        assert!(source.contains("const postAuthResult = await (async () =>"));
        assert!(source.contains("task_type: 'same-origin-api-map'"));
        assert!(source.contains("post_auth_result: postAuthResult"));

        Ok(())
    }

    #[test]
    fn web_stack_authenticated_capture_is_source_scoped_and_secret_free() -> anyhow::Result<()> {
        let source = build_web_stack_authenticated_source_capture(
            "dnbhoovers.com",
            "Example Industrial GmbH",
            "DE",
        )?;

        assert!(source.contains("app.dnbhoovers.com"));
        assert!(source.contains("hostAllowed"));
        assert!(source.contains("wrong_origin"));
        assert!(source.contains("D&B Hoovers exact company result"));
        assert!(!source.contains("credentialValue"));
        assert!(!source.contains("password"));
        Ok(())
    }

    #[test]
    fn web_stack_existing_authenticated_capture_providers_remain_available() -> anyhow::Result<()> {
        let dnb =
            build_web_stack_authenticated_source_capture("dnbhoovers.com", "Example AG", "DE")?;
        assert!(dnb.contains("app.dnbhoovers.com"));
        assert!(dnb.contains("D&B Hoovers exact company result"));

        let leadfeeder =
            build_web_stack_authenticated_source_capture("leadfeeder.com", "Example AG", "DE")?;
        assert!(leadfeeder.contains("app.leadfeeder.com"));
        assert!(leadfeeder.contains("Leadfeeder exact company result"));

        for source in [dnb, leadfeeder] {
            assert!(source.contains("hostAllowed"));
            assert!(source.contains("wrong_origin"));
            assert!(!source.contains("credentialValue"));
            assert!(!source.contains("password"));
        }
        Ok(())
    }

    fn parse_rocketreach_records_for_test(
        company: &str,
        snapshots: serde_json::Value,
    ) -> serde_json::Value {
        let script = format!(
            "{}\nconst result = parseRocketReachRecords({}, {});\nprocess.stdout.write(JSON.stringify(result));",
            ROCKETREACH_BROWSER_RECORD_PARSER,
            serde_json::to_string(company).expect("company json"),
            serde_json::to_string(&snapshots).expect("snapshots json")
        );
        let output = std::process::Command::new("node")
            .args(["--input-type=module", "--eval", &script])
            .output()
            .expect("Node.js is required for RocketReach capture parser tests");
        assert!(
            output.status.success(),
            "RocketReach capture parser failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("RocketReach parser result json")
    }

    #[test]
    fn web_stack_rocketreach_capture_is_provider_scoped_and_secret_free() -> anyhow::Result<()> {
        let defaults =
            web_stack_auth_source_defaults("rocketreach.com").expect("RocketReach auth defaults");
        assert_eq!(defaults.login_url, "https://rocketreach.co/login");
        assert_eq!(
            defaults.allowed_domains,
            &["rocketreach.com", "rocketreach.co"]
        );
        assert_eq!(defaults.secret_name, "ROCKETREACH_BROWSER_LOGIN");

        let source = build_web_stack_authenticated_source_capture(
            "rocketreach.com",
            "Example Manufacturing AG",
            "DE",
        )?;
        for expected in [
            "rocketreach.com",
            "rocketreach.co",
            "RocketReach company identity verified",
            "person_vorname",
            "person_nachname",
            "person_position",
            "person_email",
            "person_telefon",
            "embeddedPeople",
            "protectedFieldCount",
        ] {
            assert!(
                source.contains(expected),
                "missing capture guard: {expected}"
            );
        }
        assert!(!source.contains("ROCKETREACH_BROWSER_LOGIN"));
        assert!(!source.contains("credentialValue"));
        assert!(!source.contains("password"));
        assert!(!source.contains("console."));
        Ok(())
    }

    #[test]
    fn web_stack_rocketreach_capture_requires_company_identity_for_protected_fields() {
        let matching = parse_rocketreach_records_for_test(
            "Example Manufacturing AG",
            serde_json::json!([
                {
                    "sourceUrl": "https://rocketreach.co/example-manufacturing-ag-profile_bexample",
                    "title": "Example Manufacturing AG Company Profile | RocketReach",
                    "headings": ["Example Manufacturing AG"],
                    "bodyLines": ["Example Manufacturing AG"],
                    "embeddedPeople": [],
                    "links": [{
                        "url": "https://rocketreach.com/ada-lovelace-email_bexample",
                        "text": "Ada Lovelace",
                        "contextLines": [
                            "Ada Lovelace",
                            "Chief Technology Officer",
                            "Example Manufacturing AG",
                            "ada.lovelace@example.test",
                            "+49 711 1234567"
                        ]
                    }]
                },
                {
                    "sourceUrl": "https://rocketreach.com/ada-lovelace-email_bexample",
                    "title": "Ada Lovelace Email & Phone | RocketReach",
                    "headings": ["Ada Lovelace"],
                    "bodyLines": [
                        "Ada Lovelace",
                        "Chief Technology Officer",
                        "Example Manufacturing AG",
                        "ada.lovelace@example.test",
                        "+49 711 1234567"
                    ],
                    "embeddedPeople": [],
                    "links": []
                }
            ]),
        );
        assert_eq!(
            matching
                .get("companyMatched")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        let records = matching
            .get("records")
            .and_then(serde_json::Value::as_array)
            .expect("records");
        for field in [
            "firma_name",
            "person_vorname",
            "person_nachname",
            "person_position",
            "person_email",
            "person_telefon",
        ] {
            assert!(
                records.iter().any(|record| {
                    record.get("field").and_then(serde_json::Value::as_str) == Some(field)
                }),
                "missing RocketReach field {field}"
            );
        }
        assert!(records.iter().all(|record| record
            .get("source_url")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|url| url.starts_with("https://rocketreach.co/")
                || url.starts_with("https://rocketreach.com/"))));

        let wrong_company = parse_rocketreach_records_for_test(
            "Example Manufacturing AG",
            serde_json::json!([{
                "sourceUrl": "https://rocketreach.co/another-company-profile_bother",
                "title": "Another Company GmbH Company Profile | RocketReach",
                "headings": ["Another Company GmbH"],
                "bodyLines": ["Another Company GmbH"],
                "embeddedPeople": [{
                    "name": "Ada Lovelace",
                    "company": "Another Company GmbH",
                    "sourceUrl": "https://rocketreach.co/ada-lovelace-email_bother",
                    "email": "ada.lovelace@example.test"
                }],
                "links": []
            }]),
        );
        assert_eq!(
            wrong_company
                .get("companyMatched")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            wrong_company
                .get("records")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(0)
        );
    }

    #[test]
    fn web_stack_xing_capture_delegates_to_current_provider_routes() -> anyhow::Result<()> {
        let source = build_web_stack_authenticated_source_capture(
            "xing.com",
            "Example Industrial GmbH",
            "DE",
        )?;

        assert!(source.contains("https://www.xing.com/search/companies"));
        assert!(source.contains("https://www.xing.com/search/members"));
        assert!(!source.contains("https://www.xing.com/search/people"));
        assert!(source.contains("XING canonical profile URL"));
        assert!(!source.contains("console."));
        assert!(!source.contains("credentialValue"));

        Ok(())
    }

    #[test]
    fn web_stack_source_capture_command_is_registered() {
        let root = tempfile::tempdir().expect("temp root");
        let args = vec![
            "source-capture".to_string(),
            "--source-id".to_string(),
            "unsupported.example".to_string(),
            "--company".to_string(),
            "Example AG".to_string(),
        ];
        let error = run_business_os_web_stack_cli_json_local(root.path(), &args)
            .expect_err("unsupported source must be rejected")
            .to_string();

        assert!(error.contains("source-capture supports only authenticated"));
        assert!(!error.contains("unknown business-os web-stack command"));
    }

    #[test]
    fn authenticated_capture_merges_only_redacted_planned_evidence() -> anyhow::Result<()> {
        let mut payload = serde_json::json!({
            "plan": [{
                "source_id": "dnbhoovers.com",
                "tier": "C",
                "target_fields": ["umsatz", "mitarbeiter"]
            }],
            "fields": {
                "umsatz": {"value": null, "confidence": "missing", "candidates": []},
                "mitarbeiter": {"value": null, "confidence": "missing", "candidates": []}
            }
        });
        let capture = serde_json::json!({
            "ok": true,
            "source_id": "dnbhoovers.com",
            "source_status": "succeeded",
            "session_id": "browser-session-1",
            "source_url": "https://app.dnbhoovers.com/company/example",
            "credential_ref": "ctox-secret://credentials/provider-login",
            "records": [{
                "field": "umsatz",
                "value": "530 Mio.",
                "confidence": "high",
                "source_url": "https://app.dnbhoovers.com/company/example",
                "note": "structured company result"
            }, {
                "field": "mitarbeiter",
                "value": "2.830",
                "confidence": "high",
                "source_url": "https://app.dnbhoovers.com/company/example"
            }]
        });

        let summary = merge_authenticated_person_research_capture(&mut payload, &capture)?;

        assert_eq!(summary["ok"], true);
        assert_eq!(summary["record_count"], 2);
        assert_eq!(summary["merged_count"], 2);
        assert_eq!(payload["fields"]["umsatz"]["value"], "530 Mio.");
        assert_eq!(payload["fields"]["mitarbeiter"]["value"], "2.830");
        let serialized = serde_json::to_string(&summary)?;
        assert!(!serialized.contains("credential_ref"));
        assert!(!serialized.contains("provider-login"));
        assert_eq!(summary["secret_value_in_payload"], false);
        Ok(())
    }

    #[test]
    fn authenticated_capture_is_planned_for_missing_credentialed_target() {
        let payload = serde_json::json!({
            "fields": {
                "person_funktion": {"value": null, "confidence": "missing", "candidates": []},
                "person_xing": {"value": "https://www.xing.com/profile/Ada_Lovelace", "confidence": "medium", "candidates": []}
            },
            "browser_assist_tasks": [],
            "browser_assist_recommendations": [{
                "source_id": "xing.com",
                "target_fields": ["person_funktion", "person_xing"],
                "target_url": "https://login.xing.com/"
            }]
        });

        let tasks = authenticated_person_research_capture_tasks(&payload);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["source_id"], "xing.com");
        assert_eq!(
            tasks[0]["reason"],
            "missing_target_fields_after_public_research"
        );
        assert_eq!(tasks[0]["secret_value_in_payload"], false);
    }

    #[test]
    fn authenticated_capture_is_not_planned_when_all_targets_are_populated() {
        let payload = serde_json::json!({
            "fields": {
                "person_funktion": {"value": "Einkaufsleitung"},
                "person_xing": {"value": "https://www.xing.com/profile/Ada_Lovelace"}
            },
            "browser_assist_tasks": [],
            "browser_assist_recommendations": [{
                "source_id": "xing.com",
                "target_fields": ["person_funktion", "person_xing"]
            }]
        });

        assert!(authenticated_person_research_capture_tasks(&payload).is_empty());
    }

    #[test]
    fn authenticated_capture_task_deduplicates_failure_and_recommendation() {
        let payload = serde_json::json!({
            "fields": {
                "person_email": {"value": null}
            },
            "browser_assist_tasks": [{
                "source_id": "rocketreach.com",
                "target_fields": ["person_email"],
                "reason": "blocked"
            }],
            "browser_assist_recommendations": [{
                "source_id": "rocketreach.com",
                "target_fields": ["person_email"]
            }]
        });

        let tasks = authenticated_person_research_capture_tasks(&payload);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["reason"], "blocked");
    }

    #[test]
    fn authenticated_capture_does_not_execute_unsupported_browser_assists() {
        let payload = serde_json::json!({
            "browser_assist_tasks": [{
                "source_id": "handelsregister.de",
                "target_fields": ["firma_name"],
                "reason": "blocked"
            }, {
                "source_id": "xing.com",
                "target_fields": ["person_xing"],
                "reason": "blocked"
            }]
        });

        let tasks = authenticated_person_research_capture_tasks(&payload);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["source_id"], "xing.com");
    }

    #[test]
    fn authenticated_capture_uses_plan_when_provider_has_no_browser_recipe() {
        let payload = serde_json::json!({
            "plan": [{
                "source_id": "rocketreach.com",
                "target_fields": ["person_position", "person_email", "person_telefon"]
            }],
            "fields": {
                "person_position": {"value": null},
                "person_email": {"value": null},
                "person_telefon": {"value": null}
            },
            "browser_assist_tasks": [],
            "browser_assist_recommendations": []
        });

        let tasks = authenticated_person_research_capture_tasks(&payload);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["source_id"], "rocketreach.com");
        assert_eq!(tasks[0]["target_fields"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn leadfeeder_capture_uses_canonical_person_research_fields() -> anyhow::Result<()> {
        let source =
            build_web_stack_authenticated_source_capture("leadfeeder.com", "Example AG", "DE")?;

        assert!(source.contains("push(\"firma_domain\", website"));
        assert!(!source.contains("push(\"firma_website\", website"));
        assert!(source.contains("push(\"mitarbeiter\", employees"));
        Ok(())
    }

    #[test]
    fn web_stack_auth_assist_signup_source_classifies_provisioning_states() -> anyhow::Result<()> {
        let source = build_web_stack_auth_assist_signup_source(
            "https://example.test/register",
            "example-source",
            "user@example.test",
            "ctox-secret://appsec/user-a",
            "secret-value",
            "input[type='email']",
            "input[name='password']",
            "input[name='confirmPassword']",
            "button[type='submit']",
            "[data-testid='account-home']",
            true,
            "input[name='terms']",
            "User A",
            "input[name='displayName']",
            "Tenant A",
            "input[name='tenantName']",
        )?;

        assert!(source.contains("signup_state"));
        assert!(source.contains("already_registered"));
        assert!(source.contains("verification_required"));
        assert!(source.contains("signup_error_detected"));
        assert!(source.contains("account-already-registered"));
        assert!(source.contains("verification-required"));
        assert!(source.contains("signup-signals-satisfied"));
        assert!(source.contains("shouldAcceptTerms"));
        assert!(source.contains("confirm_credential"));
        assert!(source.contains("credential value is not returned"));

        Ok(())
    }

    #[test]
    fn matching_skill_binding_resolves_to_on_disk_skill_pack() {
        // Regression: suggested_skill_for_command + SKILL_APP_BINDINGS once used
        // "business-os-matching", which names no skill bundle on disk (the dir is
        // business-os-requirement-matching). A suggested_skill that resolves to
        // nothing means the LLM matching skill never binds and requirement
        // scoring silently falls back to the keyword-only native scorer (§5.2).
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while !dir.join("src/skills/packs").is_dir() {
            assert!(
                dir.pop(),
                "could not locate src/skills/packs from CARGO_MANIFEST_DIR"
            );
        }
        let binding = find_skill_binding("business-os-requirement-matching")
            .expect("requirement-matching binding present");
        let skill_md = dir
            .join("src/skills/packs")
            .join(binding.pack)
            .join(binding.skill_id)
            .join("SKILL.md");
        assert!(
            skill_md.is_file(),
            "matching skill pack missing at {}",
            skill_md.display()
        );
    }

    #[test]
    fn app_bench_help_does_not_submit_or_cleanup() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let installed_root = root.path().join("runtime/business-os/installed-modules");
        fs::create_dir_all(installed_root.join("bench_old"))?;
        fs::create_dir_all(installed_root.join("real_inventory"))?;

        let result =
            handle_business_os_app_bench(root.path(), &["run".to_string(), "--help".to_string()])?;
        assert_eq!(
            result.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            result
                .pointer("/runner_contract/submits_real_business_commands")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(
            installed_root.join("bench_old").is_dir(),
            "help must not cleanup existing bench apps"
        );
        assert!(
            installed_root.join("real_inventory").is_dir(),
            "help must not touch non-bench apps"
        );
        assert!(
            channels::list_queue_tasks(root.path(), &[], 16)?.is_empty(),
            "help must not submit queue tasks"
        );
        assert!(
            !root
                .path()
                .join(BUSINESS_OS_APP_BENCH_EVIDENCE_DIR)
                .exists(),
            "help must not write bench evidence"
        );

        let status_result = handle_business_os_app_bench(
            root.path(),
            &["status".to_string(), "--help".to_string()],
        )?;
        assert_eq!(
            status_result.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(
            channels::list_queue_tasks(root.path(), &[], 16)?.is_empty(),
            "status help must not submit queue tasks"
        );
        Ok(())
    }

    #[test]
    fn app_bench_run_submits_real_tasks_without_writing_app_artifacts() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let installed_root = root.path().join("runtime/business-os/installed-modules");
        fs::create_dir_all(installed_root.join("bench_old"))?;
        fs::create_dir_all(installed_root.join("real_inventory"))?;

        let args = vec![
            "--run-id".to_string(),
            "rtest".to_string(),
            "--suite".to_string(),
            "core-five".to_string(),
            "--model".to_string(),
            "minimax-m3".to_string(),
            "--context".to_string(),
            "256k".to_string(),
        ];
        let summary = run_business_os_app_bench(root.path(), &args)?;
        assert_eq!(
            summary.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            summary
                .get("accepted_count")
                .and_then(serde_json::Value::as_u64),
            Some(5)
        );
        assert_eq!(
            summary
                .pointer("/runner_contract/creates_app_files")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(
            !installed_root.join("bench_old").exists(),
            "bench cleanup should remove only old bench-tagged apps"
        );
        assert!(
            installed_root.join("real_inventory").is_dir(),
            "bench cleanup must preserve non-bench runtime apps"
        );
        for key in [
            "subscriptions",
            "inventory",
            "projects",
            "contracts",
            "quality",
        ] {
            assert!(
                !installed_root.join(format!("bench_{key}_rtest")).exists(),
                "bench runner must not create app artifacts for {key}"
            );
        }

        let events_path = root
            .path()
            .join(BUSINESS_OS_APP_BENCH_EVIDENCE_DIR)
            .join("rtest/events.jsonl");
        let summary_path = root
            .path()
            .join(BUSINESS_OS_APP_BENCH_EVIDENCE_DIR)
            .join("rtest/summary.json");
        assert!(events_path.is_file(), "bench JSONL evidence must exist");
        assert!(summary_path.is_file(), "bench summary evidence must exist");

        let tasks = channels::list_queue_tasks(root.path(), &[], 16)?;
        assert_eq!(tasks.len(), 5);
        let expected_actor_id = crate::business_os::store::session_with_persisted_user(
            root.path(),
            crate::business_os::store::session(None, None),
        )?
        .user
        .map(|user| user.id)
        .unwrap_or_else(|| "local-dev".to_owned());
        let conn = crate::business_os::store::open_store(root.path())?;
        let mut stmt = conn
            .prepare("SELECT client_context_json FROM business_commands ORDER BY command_id ASC")?;
        let contexts = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(contexts.len(), 5);
        for raw_context in contexts {
            let context: serde_json::Value = serde_json::from_str(&raw_context)?;
            assert_eq!(
                context
                    .pointer("/actor/id")
                    .and_then(serde_json::Value::as_str),
                Some(expected_actor_id.as_str()),
                "bench tasks without --actor must persist the local Business OS user"
            );
        }
        for task in tasks {
            assert_eq!(
                task.suggested_skill.as_deref(),
                Some(BUSINESS_OS_APP_BENCH_SKILL)
            );
            assert!(
                task.thread_key
                    .starts_with("business-os/app-creator/bench-"),
                "bench app tasks must use isolated app-creator threads, got {}",
                task.thread_key
            );
            assert_ne!(
                task.thread_key, "business-os/creator",
                "bench app tasks must not inherit the shared legacy creator thread"
            );
            assert!(task.prompt.contains("ctox.business_os.app.create"));
            assert!(task
                .prompt
                .contains("runtime/business-os/installed-modules/bench_"));
            assert!(task
                .prompt
                .contains("ctox business-os app references --query \"<workflow data keywords>\" --json --limit 8"));
            assert!(
                !task.prompt.contains("Client context JSON") && !task.prompt.contains("\"actor\""),
                "app worker prompts must not leak raw client context or actor JSON"
            );
        }
        Ok(())
    }

    #[test]
    fn app_create_cli_enqueues_real_task_without_writing_app_artifacts() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let module_id = "cli-inventory";
        let installed_root = root.path().join("runtime/business-os/installed-modules");

        handle_business_os_app(
            root.path(),
            &[
                "create".to_string(),
                "--module-id".to_string(),
                module_id.to_string(),
                "--instruction".to_string(),
                "Build a small inventory app with one CTOX follow-up automation.".to_string(),
                "--actor".to_string(),
                "cli-admin".to_string(),
            ],
        )?;

        assert!(
            !installed_root.join(module_id).exists(),
            "app create CLI must not write app artifacts"
        );
        let tasks = channels::list_queue_tasks(root.path(), &[], 8)?;
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(
            task.suggested_skill.as_deref(),
            Some(BUSINESS_OS_APP_BENCH_SKILL)
        );
        assert!(task.prompt.contains("ctox.business_os.app.create"));
        assert!(task
            .prompt
            .contains("runtime/business-os/installed-modules/cli-inventory"));
        assert!(task.prompt.contains(
            "ctox business-os app references --query \"<workflow data keywords>\" --json --limit 8"
        ));
        Ok(())
    }

    #[test]
    fn app_modify_cli_enqueues_app_modify_skill_task() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;

        handle_business_os_app(
            root.path(),
            &[
                "modify".to_string(),
                "cli-inventory".to_string(),
                "--instruction".to_string(),
                "Add a due-date field and keep existing data.".to_string(),
                "--actor".to_string(),
                "cli-admin".to_string(),
            ],
        )?;

        let tasks = channels::list_queue_tasks(root.path(), &[], 8)?;
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(
            task.suggested_skill.as_deref(),
            Some(BUSINESS_OS_APP_BENCH_SKILL)
        );
        assert!(task.prompt.contains("ctox.business_os.app.modify"));
        assert!(task
            .prompt
            .contains("runtime/business-os/installed-modules/cli-inventory"));
        Ok(())
    }

    #[test]
    fn app_references_mark_source_only_manifest_fields_as_non_templates() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let module_dir = root.path().join("src/apps/business-os/modules/app-store");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            module_dir.join("module.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "id": "app-store",
                "title": "App Store",
                "description": "Developer tool",
                "entry": "modules/app-store/index.html",
                "category": "Development",
                "collections": ["business_commands"],
                "layout": {
                    "shell": "full-workspace",
                    "icon_svg": "<svg></svg>",
                    "left": "filters",
                    "center": "catalog",
                    "right": "details"
                },
                "store": {
                    "installable": true
                }
            }))?,
        )?;

        let report = business_os_app_reference_candidates(root.path(), &[])?;
        let module = report
            .get("modules")
            .and_then(serde_json::Value::as_array)
            .and_then(|modules| modules.first())
            .context("expected reference module")?;
        assert_eq!(
            module
                .get("reference_kind")
                .and_then(serde_json::Value::as_str),
            Some("internal-shell-reference")
        );
        assert_eq!(
            module
                .get("recommended_for_generated_business_app")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(
            module.pointer("/layout/icon_svg").is_none(),
            "reference catalog must not expose source inline SVG as a template"
        );
        assert_eq!(
            module
                .pointer("/layout/right_pane_is_exception")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(module
            .get("warnings")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|warnings| warnings
                .iter()
                .any(|warning| warning.as_str().unwrap_or("").contains("layout.icon_svg"))));
        assert!(report
            .get("runtime_rules")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|rules| rules
                .iter()
                .any(|rule| rule.as_str().unwrap_or("").contains("store.installable"))));
        Ok(())
    }

    #[test]
    fn app_references_default_to_small_ranked_catalog() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let modules_root = root.path().join("src/apps/business-os/modules");
        for idx in 0..12 {
            let id = if idx == 3 {
                "office-assets".to_string()
            } else if idx == 5 {
                "inventory".to_string()
            } else {
                format!("workflow-{idx}")
            };
            let module_dir = modules_root.join(&id);
            fs::create_dir_all(&module_dir)?;
            let description = if id == "office-assets" {
                "Track office asset inventory, owners, and replacement dates."
            } else if id == "inventory" {
                "Inventory counts, warehouse stock, and reorder dates."
            } else {
                "General business workflow reference."
            };
            fs::write(
                module_dir.join("module.json"),
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": id.as_str(),
                    "title": title_from_module_id(&id),
                    "description": description,
                    "entry": format!("modules/{id}/index.html"),
                    "category": "Operations",
                    "collections": [format!("{id}_records")],
                    "layout": { "shell": "full-workspace", "left": "List", "center": "Detail" }
                }))?,
            )?;
        }

        let default_report = business_os_app_reference_candidates(root.path(), &[])?;
        assert_eq!(
            default_report
                .get("returned")
                .and_then(serde_json::Value::as_u64),
            Some(BUSINESS_OS_APP_REFERENCE_DEFAULT_LIMIT as u64)
        );
        assert_eq!(
            default_report
                .get("truncated")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );

        let query_report = business_os_app_reference_candidates(
            root.path(),
            &[
                "--query".to_string(),
                "asset inventory".to_string(),
                "--limit".to_string(),
                "3".to_string(),
            ],
        )?;
        let query_modules = query_report
            .get("modules")
            .and_then(serde_json::Value::as_array)
            .context("query modules")?;
        assert!(query_modules.len() <= 3);
        assert!(
            query_modules.iter().any(|module| module
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| id == "office-assets" || id == "inventory")),
            "query should surface workflow-relevant references"
        );

        let all_report = business_os_app_reference_candidates(root.path(), &["--all".to_string()])?;
        assert_eq!(
            all_report
                .get("returned")
                .and_then(serde_json::Value::as_u64),
            Some(12)
        );
        assert_eq!(
            all_report
                .get("truncated")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        Ok(())
    }

    #[test]
    fn app_bench_status_records_partial_artifacts_without_marking_green() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let args = vec![
            "--run-id".to_string(),
            "rstatus".to_string(),
            "--suite".to_string(),
            "core-five".to_string(),
            "--model".to_string(),
            "minimax-m3".to_string(),
            "--context".to_string(),
            "256k".to_string(),
        ];
        let summary = run_business_os_app_bench(root.path(), &args)?;
        let task_id = summary
            .pointer("/submitted_tasks/0/accepted/task_id")
            .and_then(serde_json::Value::as_str)
            .context("missing first task id")?;
        channels::lease_queue_task(root.path(), task_id, "ctox-service")?;

        let module_dir = root
            .path()
            .join("runtime/business-os/installed-modules/bench_subscriptions_rstatus");
        fs::create_dir_all(&module_dir)?;
        fs::write(module_dir.join("module.json"), "{}")?;

        let status_args = vec!["--run-id".to_string(), "rstatus".to_string()];
        let report = collect_business_os_app_bench_status(root.path(), &status_args)?;
        assert_eq!(
            report.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            report
                .get("bench_green")
                .and_then(serde_json::Value::as_bool),
            Some(false),
            "partial artifacts must not be reported as a green bench"
        );
        assert_eq!(
            report
                .get("needs_attention")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            report
                .get("expected_count")
                .and_then(serde_json::Value::as_u64),
            Some(5)
        );
        assert_eq!(
            report
                .pointer("/counts/leased")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            report
                .pointer("/counts/pending")
                .and_then(serde_json::Value::as_u64),
            Some(4)
        );
        assert_eq!(
            report
                .pointer("/counts/artifact_dirs_present")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            report
                .pointer("/counts/artifact_dirs_missing")
                .and_then(serde_json::Value::as_u64),
            Some(4)
        );
        assert_eq!(
            report
                .pointer("/counts/apps_with_missing_required_files")
                .and_then(serde_json::Value::as_u64),
            Some(5)
        );
        let first_app = report
            .get("apps")
            .and_then(serde_json::Value::as_array)
            .and_then(|apps| apps.first())
            .context("missing app status")?;
        assert_eq!(
            first_app
                .pointer("/artifacts/exists")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(
            first_app
                .pointer("/artifacts/required_missing")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|missing| {
                    missing
                        .iter()
                        .any(|item| item.as_str() == Some("index.html"))
                }),
            "partial module report must list missing required files"
        );
        let status_path = report
            .get("status_path")
            .and_then(serde_json::Value::as_str)
            .context("missing status path")?;
        assert!(
            Path::new(status_path).is_file(),
            "bench status evidence must be written"
        );
        let events_path = root
            .path()
            .join(BUSINESS_OS_APP_BENCH_EVIDENCE_DIR)
            .join("rstatus/events.jsonl");
        let events = fs::read_to_string(events_path)?;
        assert!(events.contains("\"event\":\"status_collected\""));
        Ok(())
    }

    #[test]
    fn app_bench_rejects_retired_128k_context() {
        let root = tempfile::tempdir().expect("temp root");
        let args = vec![
            "--run-id".to_string(),
            "rtest".to_string(),
            "--context".to_string(),
            "128k".to_string(),
        ];
        let error =
            run_business_os_app_bench(root.path(), &args).expect_err("128k must be rejected");
        assert!(
            format!("{error:#}").contains("256k context"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn app_validate_success_does_not_finalize_matching_leased_creator_task() {
        let root = tempfile::tempdir().expect("temp root");
        let module_id = "projects";
        let script_dir = root.path().join("src/apps/business-os/scripts");
        fs::create_dir_all(&script_dir).expect("create validator script dir");
        fs::write(
            script_dir.join("validate-app-module.mjs"),
            "process.exit(0);\n",
        )
        .expect("write green validator");
        let task_context = format!(
            "Business OS app task metadata:\n- module_id: {module_id}\n- install_target: runtime-installed-module\n- app_directory: runtime/business-os/installed-modules/{module_id}\nBusiness OS command:\n- type: ctox.business_os.app.create\nRequired CTOX resources: business-os-app-module-development\n"
        );
        let created = channels::create_queue_task(
            root.path(),
            channels::QueueTaskCreateRequest {
                title: "Projects Bench".to_string(),
                prompt: task_context,
                thread_key: "business-os/creator".to_string(),
                workspace_root: Some(root.path().display().to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("business-os-app-module-development".to_string()),
                parent_message_key: None,
                extra_metadata: Some(serde_json::json!({
                    "source": "business-os",
                    "idempotency_key": format!("test-ctox.business_os.app.create-{module_id}"),
                    "business_os_command_id": format!(
                        "test-ctox.business_os.app.create-{module_id}"
                    ),
                    "business_os_module": "creator",
                    "business_os_inbound_channel": "creator",
                    "business_os_command_type": "ctox.business_os.app.create",
                    "business_os_record_id": module_id,
                    "business_os_attachments": [],
                    "client_context": {
                        "source": "business-os-app-creator-test",
                        "target": "app"
                    }
                })),
            },
        )
        .expect("create queue task");
        channels::lease_queue_task(root.path(), &created.message_key, "ctox-service")
            .expect("lease queue task");

        handle_business_os_app(
            root.path(),
            &[
                "validate".to_string(),
                module_id.to_string(),
                "--installed".to_string(),
            ],
        )
        .expect("validate should pass");

        let reloaded = channels::load_queue_task(root.path(), &created.message_key)
            .expect("load queue task")
            .expect("queue task exists");
        assert_eq!(reloaded.route_status, "leased");
        assert_eq!(reloaded.status_note.as_deref(), None);
        assert_eq!(reloaded.acked_at.as_deref(), None);
    }

    #[test]
    fn mcp_policy_cli_bool_parser_accepts_strict_values() {
        assert_eq!(parse_cli_bool("true").unwrap(), true);
        assert_eq!(parse_cli_bool("off").unwrap(), false);
        assert!(parse_cli_bool("maybe").is_err());
    }

    #[test]
    fn mcp_policy_deny_tools_are_deduplicated_and_scoped() {
        let args = vec![
            "set".to_string(),
            "--deny-tool".to_string(),
            "business_os.execute_action,business_os.approve".to_string(),
            "--deny-tool".to_string(),
            "business_os.execute_action".to_string(),
        ];
        let tools = mcp_policy_deny_tools_from_args(&args).unwrap();
        assert_eq!(
            tools,
            vec![
                "business_os.execute_action".to_string(),
                "business_os.approve".to_string()
            ]
        );

        let invalid = vec![
            "set".to_string(),
            "--deny-tool".to_string(),
            "run_shell".to_string(),
        ];
        assert!(mcp_policy_deny_tools_from_args(&invalid).is_err());
    }

    #[test]
    fn mcp_policy_value_args_are_deduplicated() {
        let args = vec![
            "set".to_string(),
            "--allow-module".to_string(),
            "customers,outbound".to_string(),
            "--allow-module".to_string(),
            "customers".to_string(),
        ];
        let mut values = Vec::new();

        apply_mcp_policy_values_arg(&args, "--allow-module", &mut values);

        assert_eq!(
            values,
            vec!["customers".to_string(), "outbound".to_string()]
        );
    }

    #[test]
    fn mcp_policy_env_projection_only_reports_mcp_keys() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_BUSINESS_OS_MCP_ENABLED".to_string(),
            "false".to_string(),
        );
        env_map.insert(
            "CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE".to_string(),
            "10".to_string(),
        );
        env_map.insert(
            "CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS".to_string(),
            "30".to_string(),
        );
        env_map.insert(
            "CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES".to_string(),
            "customers".to_string(),
        );
        env_map.insert("OPENAI_API_KEY".to_string(), "secret".to_string());

        let projection = mcp_policy_env_projection(&env_map);

        assert_eq!(
            projection
                .get("CTOX_BUSINESS_OS_MCP_ENABLED")
                .and_then(serde_json::Value::as_str),
            Some("false")
        );
        assert_eq!(
            projection
                .get("CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE")
                .and_then(serde_json::Value::as_str),
            Some("10")
        );
        assert_eq!(
            projection
                .get("CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS")
                .and_then(serde_json::Value::as_str),
            Some("30")
        );
        assert_eq!(
            projection
                .get("CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES")
                .and_then(serde_json::Value::as_str),
            Some("customers")
        );
        assert!(projection.get("OPENAI_API_KEY").is_none());
    }

    #[test]
    fn mcp_policy_set_persists_typed_policy_state() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let mut legacy_env = BTreeMap::new();
        legacy_env.insert(
            "CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS".to_string(),
            "30".to_string(),
        );
        crate::inference::runtime_env::save_runtime_env_map(root.path(), &legacy_env)?;

        let args = vec![
            "set".to_string(),
            "--enabled".to_string(),
            "true".to_string(),
            "--audit-retention-days".to_string(),
            "7".to_string(),
            "--allow-module".to_string(),
            "customers,customers,outbound".to_string(),
            "--deny-tool".to_string(),
            "business_os.execute_action".to_string(),
        ];
        handle_business_os_mcp_policy(root.path(), &args)?;

        let policy = crate::business_os::mcp_channel::mcp_policy(root.path());
        assert!(policy.enabled);
        assert_eq!(policy.audit_retention_days, 7);
        assert_eq!(
            policy.allowed_modules,
            vec!["customers".to_string(), "outbound".to_string()]
        );
        assert_eq!(
            policy.denied_tools,
            vec!["business_os.execute_action".to_string()]
        );
        assert_eq!(
            crate::inference::runtime_env::effective_operator_env_map(root.path())?
                .get("CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS")
                .map(String::as_str),
            Some("30")
        );
        Ok(())
    }

    #[test]
    fn desktop_invite_help_is_detected_before_generating_credentials() {
        assert!(desktop_invite_help_requested(&[
            "invite".to_string(),
            "--help".to_string(),
        ]));
        assert!(desktop_invite_help_requested(&[
            "invite".to_string(),
            "-h".to_string(),
        ]));
        assert!(!desktop_invite_help_requested(&[
            "invite".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ]));
    }

    #[test]
    fn desktop_invite_contract_matches_electron_pairing_schema() {
        let root = tempfile::tempdir().expect("temp root");
        let args = vec![
            "invite".to_string(),
            "--display-name".to_string(),
            "Lab Instance".to_string(),
            "--expires-at".to_string(),
            "2099-01-01T00:00:00.000Z".to_string(),
        ];
        let invite = build_desktop_invite(root.path(), &args).expect("invite");

        assert_eq!(
            invite.get("type").and_then(serde_json::Value::as_str),
            Some("ctox-business-os-invite")
        );
        assert_eq!(
            invite.get("version").and_then(serde_json::Value::as_i64),
            Some(1)
        );
        assert_eq!(
            invite
                .get("display_name")
                .and_then(serde_json::Value::as_str),
            Some("Lab Instance")
        );
        assert_eq!(
            invite.get("transport").and_then(serde_json::Value::as_str),
            Some("webrtc")
        );
        assert_eq!(
            invite.get("data_plane").and_then(serde_json::Value::as_str),
            Some("rxdb-webrtc")
        );
        assert_eq!(
            invite
                .get("http_bridge_available")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(invite
            .get("sync_room")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .starts_with("ctox-business-os:"));
        assert!(invite
            .get("signaling_urls")
            .and_then(serde_json::Value::as_array)
            .map(|urls| !urls.is_empty())
            .unwrap_or(false));
        assert!(invite.get("signaling_room_password").is_none());
        assert_eq!(
            invite
                .get("signaling_auth_version")
                .and_then(serde_json::Value::as_str),
            Some("ctox-role-bound-v1")
        );
        assert!(!invite
            .get("signaling_browser_token")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .is_empty());
        assert_eq!(
            invite
                .get("signaling_browser_token_hash")
                .and_then(serde_json::Value::as_str)
                .map(str::len),
            Some(64)
        );
        assert_eq!(
            invite
                .get("signaling_native_token_hash")
                .and_then(serde_json::Value::as_str)
                .map(str::len),
            Some(64)
        );
        assert!(!invite
            .pointer("/session/capability_token")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .is_empty());
        assert!(
            invite
                .pointer("/session/capability_expires_at_ms")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default()
                > 0
        );

        let desktop_link = invite
            .get("desktop_link")
            .and_then(serde_json::Value::as_str)
            .expect("desktop link");
        let payload = desktop_link
            .strip_prefix("ctox-business-os-desktop://pair?payload=")
            .expect("desktop invite link prefix");
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .expect("decode payload");
        let decoded_invite: serde_json::Value =
            serde_json::from_slice(&decoded).expect("decoded invite json");
        assert_eq!(
            decoded_invite
                .get("type")
                .and_then(serde_json::Value::as_str),
            Some("ctox-business-os-invite")
        );
        assert!(decoded_invite.get("native_peer_id").is_some());
        assert!(decoded_invite
            .pointer("/session/capability_token")
            .is_some());
        assert_eq!(decoded_invite.get("desktop_link"), None);
    }

    #[test]
    fn app_browser_evidence_paths_are_relative_to_caller_cwd() {
        let cwd = PathBuf::from("/tmp/ctox-caller");
        assert_eq!(
            app_browser_evidence_arg("--output", "output/e2e.json", &cwd),
            "/tmp/ctox-caller/output/e2e.json"
        );
        assert_eq!(
            app_browser_evidence_arg("--screenshot", "/tmp/e2e.png", &cwd),
            "/tmp/e2e.png"
        );
        assert_eq!(
            app_browser_evidence_arg("--url", "http://127.0.0.1:8765", &cwd),
            "http://127.0.0.1:8765"
        );
    }
}

fn rxdb_id_slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if slug.is_empty() {
        "source".to_string()
    } else {
        slug
    }
}

fn allowed_domains_from_url(target_url: &str) -> Vec<String> {
    Url::parse(target_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .map(|host| vec![host])
        .unwrap_or_default()
}
