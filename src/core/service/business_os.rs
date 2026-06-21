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

const BUSINESS_STACK_SKILL_CANDIDATES: &[&str] = &[
    "src/skills/system/product_engineering/business-stack/SKILL.md",
    "skills/system/product_engineering/business-stack/SKILL.md",
];
const BUSINESS_STACK_INSTALLER_CANDIDATES: &[&str] = &[
    "skills/system/product_engineering/business-stack/scripts/install_business_stack.py",
    "src/skills/system/product_engineering/business-stack/scripts/install_business_stack.py",
];
const BUSINESS_STACK_TEMPLATE: &str = "templates/business-basic";
const BUSINESS_OS_APP_CANDIDATES: &[&str] = &["src/apps/business-os", "business-os"];
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
        Some("app") => handle_business_os_app(root, &args[1..]),
        Some("repair") => handle_business_os_repair(root, &args[1..]),
        Some("backup") => handle_business_os_backup(root, &args[1..]),
        Some("install") => install_business_os(root, &args[1..]),
        Some("commands") => handle_business_os_commands(root, &args[1..]),
        Some("auth") => handle_business_os_auth(root, &args[1..]),
        Some("desktop") => handle_business_os_desktop(root, &args[1..]),
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

fn build_desktop_invite(root: &Path, args: &[String]) -> anyhow::Result<serde_json::Value> {
    let config = crate::business_os::store::sync_config(root)?;
    let display_name = flag_value(args, "--display-name")
        .or_else(|| flag_value(args, "--name"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(config.instance_id.as_str());
    let expires_at = flag_value(args, "--expires-at")
        .map(str::to_string)
        .unwrap_or_else(|| {
            let ttl_hours = flag_value(args, "--ttl-hours")
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(168);
            (chrono::Utc::now() + chrono::Duration::hours(ttl_hours))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        });
    let mut invite = serde_json::json!({
        "type": "ctox-business-os-invite",
        "version": 1,
        "display_name": display_name,
        "instance_id": config.instance_id,
        "sync_room": config.sync_room,
        "signaling_urls": config.signaling_urls,
        "signaling_room_password": config.signaling_room_password,
        "transport": "webrtc",
        "expires_at": expires_at,
        "data_plane": "rxdb-webrtc",
        "http_bridge_available": false,
        "secret_value_in_payload": true,
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
    let skill = existing_file_path(root, BUSINESS_STACK_SKILL_CANDIDATES);
    let installer = existing_file_path(root, BUSINESS_STACK_INSTALLER_CANDIDATES);
    let template = root.join(BUSINESS_STACK_TEMPLATE);
    let manifest = template.join("ctox-business.json");
    let native_app = existing_dir_path(root, BUSINESS_OS_APP_CANDIDATES);

    format!(
        "CTOX Business OS\n\
         Source skill: {skill_status}  {skill}\n\
         Installer:    {installer_status}  {installer}\n\
         Template:     {template_status}  {template}\n\
         Manifest:     {manifest_status}  {manifest}\n\n\
         Native app:   {native_app_status}  {native_app}\n\
         Native store: {native_store}\n\n\
         Serve the native no-build Business OS:\n\
           ctox business-os serve --addr 127.0.0.1:8765\n\n\
         Install into a separate customer-owned repository:\n\
           ctox business-os install --target <empty-dir> --init-git\n\n\
         Preview first:\n\
           ctox business-os install --target <empty-dir> --dry-run\n\n\
         Runtime contract:\n\
           - Web deploy can host the RxDB Business OS app shell.\n\
           - CTOX core runs as the outbound RxDB/WebRTC peer.\n\
           - SQLite state, commands, module manifests, and files sync over RxDB.\n\
           - Only system Business OS apps are installed by default.\n\
           - Non-system apps are installed through the app store only.\n",
        skill_status = exists_label(skill.is_file()),
        installer_status = exists_label(installer.is_file()),
        template_status = exists_label(template.is_dir()),
        manifest_status = exists_label(manifest.is_file()),
        native_app_status = exists_label(native_app.join("index.html").is_file()),
        skill = skill.display(),
        installer = installer.display(),
        template = template.display(),
        manifest = manifest.display(),
        native_app = native_app.display(),
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
        Some("rotate") | Some("rotate-room") => print_json(&serde_json::to_value(
            crate::business_os::store::rotate_sync_room_password(root)?,
        )?),
        Some("start") => crate::business_os::run_native_peer_foreground(root),
        Some("ensure") => {
            crate::business_os::ensure_native_peer(root)?;
            print_json(&serde_json::json!({
                "ok": true,
                "running": crate::business_os::store::sync_config(root)?.native_rxdb_peer_available,
            }))
        }
        Some("--help") | Some("-h") => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os peer command `{other}`"),
    }
}

fn handle_business_os_rxdb(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
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
    if accepted.get("status").and_then(serde_json::Value::as_str) != Some("accepted") {
        anyhow::bail!("Business OS app command was not accepted");
    }
    Ok(())
}

const BUSINESS_OS_APP_BENCH_EVIDENCE_DIR: &str = "runtime/business-os/app-creation-bench";
const BUSINESS_OS_APP_BENCH_SOURCE: &str = "ctox-cli.business-os-app-bench";
const BUSINESS_OS_APP_BENCH_SKILL: &str = "business-os-app-module-development";
const BUSINESS_OS_APP_BENCH_USAGE: &str = "ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]\nctox business-os app bench status --run-id <id> [--validate]";

#[derive(Clone, Copy)]
struct BusinessOsAppBenchCase {
    key: &'static str,
    title: &'static str,
    description: &'static str,
    minimum_scope: &'static str,
    automation: &'static str,
}

const BUSINESS_OS_APP_BENCH_CORE_FIVE: &[BusinessOsAppBenchCase] = &[
    BusinessOsAppBenchCase {
        key: "subscriptions",
        title: "Subscriptions",
        description: "Abo-Vertraege, MRR, renewal date, and churn risk.",
        minimum_scope: "subscription contracts, MRR, renewal date, churn risk",
        automation: "Create a CTOX follow-up for renewal or churn-risk review.",
    },
    BusinessOsAppBenchCase {
        key: "inventory",
        title: "Inventory",
        description: "Items, stock locations, minimum stock, and stock movement.",
        minimum_scope: "items, stock locations, minimum stock, stock movement",
        automation: "Create a CTOX follow-up for low-stock review.",
    },
    BusinessOsAppBenchCase {
        key: "projects",
        title: "Projects",
        description: "Time/material vs fixed-price, milestones, and budget vs actual.",
        minimum_scope: "time/material vs fixed-price, milestones, budget vs actual",
        automation: "Create a CTOX follow-up for over-budget or overdue milestone review.",
    },
    BusinessOsAppBenchCase {
        key: "contracts",
        title: "Contracts",
        description: "Customer contracts, SLA, renewal, and termination window.",
        minimum_scope: "customer contracts, SLA, renewal, termination window",
        automation: "Create a CTOX follow-up for renewal or cancellation deadline review.",
    },
    BusinessOsAppBenchCase {
        key: "quality",
        title: "Quality",
        description: "Complaints, corrective actions, audits, owner, and due date.",
        minimum_scope: "complaints, corrective actions, audits, owner, due date",
        automation: "Create a CTOX follow-up or local ticket for compliance action.",
    },
];

fn handle_business_os_app_bench(root: &Path, args: &[String]) -> anyhow::Result<serde_json::Value> {
    match args.first().map(String::as_str) {
        Some("run") => run_business_os_app_bench(root, &args[1..]),
        Some("status") => collect_business_os_app_bench_status(root, &args[1..]),
        Some("--help") | Some("-h") | None => Ok(serde_json::json!({
            "ok": true,
            "usage": BUSINESS_OS_APP_BENCH_USAGE
        })),
        Some(other) => anyhow::bail!("unknown business-os app bench command `{other}`"),
    }
}

fn run_business_os_app_bench(root: &Path, args: &[String]) -> anyhow::Result<serde_json::Value> {
    if args_have_help(args) {
        return Ok(serde_json::json!({
            "ok": true,
            "usage": BUSINESS_OS_APP_BENCH_USAGE,
            "runner_contract": {
                "creates_app_files": false,
                "repairs_app_files": false,
                "submits_real_business_commands": false,
                "install_target": "runtime-installed-module"
            }
        }));
    }
    let suite = flag_value(args, "--suite").unwrap_or("core-five");
    anyhow::ensure!(
        suite == "core-five",
        "unsupported Business OS app bench suite `{suite}`"
    );
    let model = flag_value(args, "--model").unwrap_or("minimax-m3");
    let context = flag_value(args, "--context").unwrap_or("256k");
    anyhow::ensure!(
        context == "256k" || context == "262144",
        "Business OS app bench must use the 256k context default"
    );
    let run_id = flag_value(args, "--run-id")
        .map(sanitize_bench_run_id)
        .transpose()?
        .unwrap_or_else(|| format!("r{}", now_ms()));
    let actor_id = flag_value(args, "--actor")
        .or_else(|| flag_value(args, "--actor-user"))
        .map(str::to_owned)
        .unwrap_or_else(|| {
            crate::business_os::store::session(None, None)
                .user
                .map(|user| user.id)
                .unwrap_or_else(|| "local-dev".to_owned())
        });
    let clean = !args.iter().any(|arg| arg == "--no-clean");
    let run_dir = root.join(BUSINESS_OS_APP_BENCH_EVIDENCE_DIR).join(&run_id);
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create bench evidence dir {}", run_dir.display()))?;
    let events_path = run_dir.join("events.jsonl");
    let mut events = Vec::new();
    append_bench_event(
        &events_path,
        &serde_json::json!({
            "event": "bench_started",
            "run_id": run_id.as_str(),
            "suite": suite,
            "model": model,
            "context": context,
            "source": BUSINESS_OS_APP_BENCH_SOURCE,
            "created_at_ms": now_ms()
        }),
    )?;

    let removed_modules = if clean {
        cleanup_business_os_app_bench_modules(root)?
    } else {
        Vec::new()
    };
    append_bench_event(
        &events_path,
        &serde_json::json!({
            "event": "cleanup_finished",
            "run_id": run_id.as_str(),
            "removed_modules": removed_modules.clone(),
            "created_at_ms": now_ms()
        }),
    )?;

    for case in BUSINESS_OS_APP_BENCH_CORE_FIVE {
        let module_id = format!("bench_{}_{}", case.key, run_id);
        let command_id = format!("cmd_app_bench_{}_{}", case.key, run_id);
        let document = business_os_app_bench_command_document(
            &command_id,
            case,
            &module_id,
            suite,
            model,
            context,
            &run_id,
            actor_id.as_str(),
        );
        let accepted = crate::business_os::store::accept_rxdb_business_command(root, document)?;
        let event = serde_json::json!({
            "event": "task_submitted",
            "run_id": run_id.as_str(),
            "case": case.key,
            "module_id": module_id,
            "command_id": command_id,
            "accepted": accepted,
            "created_at_ms": now_ms()
        });
        append_bench_event(&events_path, &event)?;
        events.push(event);
    }

    let submitted = events
        .iter()
        .filter_map(|event| event.get("accepted"))
        .collect::<Vec<_>>();
    let accepted_count = submitted
        .iter()
        .filter(|accepted| {
            accepted.get("status").and_then(serde_json::Value::as_str) == Some("accepted")
        })
        .count();
    let ok = accepted_count == BUSINESS_OS_APP_BENCH_CORE_FIVE.len();
    let summary = serde_json::json!({
        "ok": ok,
        "run_id": run_id.as_str(),
        "suite": suite,
        "model": model,
        "context": context,
        "source": BUSINESS_OS_APP_BENCH_SOURCE,
        "evidence_dir": run_dir.display().to_string(),
        "events_path": events_path.display().to_string(),
        "removed_modules": removed_modules,
        "submitted_tasks": events,
        "accepted_count": accepted_count,
        "expected_count": BUSINESS_OS_APP_BENCH_CORE_FIVE.len(),
        "runner_contract": {
            "creates_app_files": false,
            "repairs_app_files": false,
            "submits_real_business_commands": true,
            "install_target": "runtime-installed-module"
        }
    });
    fs::write(
        run_dir.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )
    .with_context(|| format!("failed to write bench summary in {}", run_dir.display()))?;
    append_bench_event(
        &events_path,
        &serde_json::json!({
            "event": "bench_finished",
            "run_id": run_id.as_str(),
            "ok": ok,
            "accepted_count": accepted_count,
            "expected_count": BUSINESS_OS_APP_BENCH_CORE_FIVE.len(),
            "created_at_ms": now_ms()
        }),
    )?;
    Ok(summary)
}

fn business_os_app_bench_command_document(
    command_id: &str,
    case: &BusinessOsAppBenchCase,
    module_id: &str,
    suite: &str,
    model: &str,
    context: &str,
    run_id: &str,
    actor_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": command_id,
        "command_id": command_id,
        "module": "creator",
        "command_type": "ctox.business_os.app.create",
        "type": "ctox.business_os.app.create",
        "record_id": module_id,
        "status": "pending_sync",
        "payload": {
            "title": format!("Build {}", case.title),
            "instruction": format!(
                "Build a small Business OS {} app for {}. Include one normal CTOX follow-up automation: {}",
                case.title,
                case.minimum_scope,
                case.automation
            ),
            "module_id": module_id,
            "app_id": module_id,
            "app_title": case.title,
            "description": case.description,
            "category": "operations",
            "install_target": "runtime-installed-module",
            "target": "app",
            "mode": "app",
            "desired_version": "0.1.0",
            "required_skills": [BUSINESS_OS_APP_BENCH_SKILL],
            "bench": {
                "suite": suite,
                "run_id": run_id,
                "case": case.key,
                "minimum_scope": case.minimum_scope,
                "required_automation": case.automation
            }
        },
        "client_context": {
            "source": BUSINESS_OS_APP_BENCH_SOURCE,
            "target": "app",
            "mode": "app",
            "module_id": module_id,
            "install_target": "runtime-installed-module",
            "required_skills": [BUSINESS_OS_APP_BENCH_SKILL],
            "bench": {
                "suite": suite,
                "run_id": run_id,
                "case": case.key,
                "model": model,
                "context": context
            },
            "actor": {
                "id": actor_id,
                "display_name": "CTOX App Bench",
                "role": "admin",
                "is_admin": true
            }
        },
        "created_at_ms": now_ms(),
        "updated_at_ms": now_ms()
    })
}

fn cleanup_business_os_app_bench_modules(root: &Path) -> anyhow::Result<Vec<String>> {
    let installed_root = root.join("runtime/business-os/installed-modules");
    if !installed_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut removed = Vec::new();
    for entry in fs::read_dir(&installed_root)
        .with_context(|| format!("failed to read {}", installed_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !(name.starts_with("bench_") || name.starts_with("bench-")) {
            continue;
        }
        fs::remove_dir_all(entry.path())
            .with_context(|| format!("failed to remove bench app {}", entry.path().display()))?;
        removed.push(name);
    }
    removed.sort();
    Ok(removed)
}

fn append_bench_event(path: &Path, event: &serde_json::Value) -> anyhow::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open bench evidence {}", path.display()))?;
    let line = serde_json::to_string(event)?;
    std::io::Write::write_all(&mut file, line.as_bytes())
        .and_then(|_| std::io::Write::write_all(&mut file, b"\n"))
        .with_context(|| format!("failed to write bench evidence {}", path.display()))
}

fn sanitize_bench_run_id(raw: &str) -> anyhow::Result<String> {
    let value = raw.trim();
    anyhow::ensure!(!value.is_empty(), "bench run id must not be empty");
    anyhow::ensure!(
        value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'),
        "bench run id may only contain ASCII letters, digits, '_' and '-'"
    );
    Ok(value.to_string())
}

fn collect_business_os_app_bench_status(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    if args_have_help(args) {
        return Ok(serde_json::json!({
            "ok": true,
            "usage": BUSINESS_OS_APP_BENCH_USAGE
        }));
    }
    let run_id = flag_value(args, "--run-id")
        .or_else(|| {
            args.iter()
                .find(|arg| !arg.starts_with("--"))
                .map(String::as_str)
        })
        .context("usage: ctox business-os app bench status --run-id <id> [--validate]")?;
    let run_id = sanitize_bench_run_id(run_id)?;
    let run_dir = root.join(BUSINESS_OS_APP_BENCH_EVIDENCE_DIR).join(&run_id);
    let summary_path = run_dir.join("summary.json");
    let events_path = run_dir.join("events.jsonl");
    let summary_raw = fs::read_to_string(&summary_path)
        .with_context(|| format!("failed to read bench summary {}", summary_path.display()))?;
    let summary: serde_json::Value =
        serde_json::from_str(&summary_raw).context("bench summary is not valid JSON")?;
    let validate = args.iter().any(|arg| arg == "--validate");
    let mut apps = Vec::new();
    let mut counts = BenchStatusCounts::default();
    let submitted = summary
        .get("submitted_tasks")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let expected_count = submitted.len();
    for item in submitted {
        let case = item
            .get("case")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let module_id = item
            .get("module_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let command_id = item
            .get("command_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let task_id = item
            .pointer("/accepted/task_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let task = if task_id.is_empty() {
            None
        } else {
            channels::load_queue_task(root, task_id)?
        };
        let route_status = task
            .as_ref()
            .map(|task| task.route_status.as_str())
            .unwrap_or("missing");
        counts.observe_route_status(route_status);
        let module_dir = root
            .join("runtime/business-os/installed-modules")
            .join(module_id);
        let artifacts = bench_app_artifact_report(&module_dir)?;
        if artifacts
            .get("exists")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            counts.artifact_dirs_present = counts.artifact_dirs_present.saturating_add(1);
        } else {
            counts.artifact_dirs_missing = counts.artifact_dirs_missing.saturating_add(1);
        }
        if artifacts
            .get("required_missing")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| !items.is_empty())
        {
            counts.apps_with_missing_required_files =
                counts.apps_with_missing_required_files.saturating_add(1);
        }
        let validation = if validate && module_dir.is_dir() {
            let validator_args = vec!["--installed".to_string(), "--json".to_string()];
            match run_business_os_app_validator(root, module_id, &validator_args) {
                Ok(output) => {
                    let success = output.status.success();
                    counts.observe_validation(success);
                    serde_json::json!({
                        "ran": true,
                        "ok": success,
                        "status": output.status.code(),
                        "stdout": truncate_bench_text(&String::from_utf8_lossy(&output.stdout), 12000),
                        "stderr": truncate_bench_text(&String::from_utf8_lossy(&output.stderr), 4000)
                    })
                }
                Err(error) => {
                    counts.observe_validation(false);
                    serde_json::json!({
                        "ran": true,
                        "ok": false,
                        "error": error.to_string()
                    })
                }
            }
        } else {
            counts.validation_skipped = counts.validation_skipped.saturating_add(1);
            serde_json::json!({
                "ran": false,
                "reason": if validate { "module_dir_missing" } else { "not_requested" }
            })
        };
        apps.push(serde_json::json!({
            "case": case,
            "module_id": module_id,
            "command_id": command_id,
            "task_id": task_id,
            "queue": task.as_ref().map(|task| serde_json::json!({
                "route_status": task.route_status.as_str(),
                "status_note": task.status_note.as_deref(),
                "lease_owner": task.lease_owner.as_deref(),
                "leased_at": task.leased_at.as_deref(),
                "acked_at": task.acked_at.as_deref(),
                "created_at": task.created_at.as_str(),
                "updated_at": task.updated_at.as_str(),
                "workspace_root": task.workspace_root.as_deref(),
                "suggested_skill": task.suggested_skill.as_deref()
            })).unwrap_or_else(|| serde_json::json!({
                "route_status": "missing"
            })),
            "module_dir": module_dir.display().to_string(),
            "artifacts": artifacts,
            "validation": validation
        }));
    }
    let finished_at_ms = now_ms();
    let status_path = run_dir.join(format!("status-{finished_at_ms}.json"));
    let bench_green = expected_count > 0
        && counts.handled == expected_count
        && counts.artifact_dirs_present == expected_count
        && counts.apps_with_missing_required_files == 0
        && counts.validation_passed == expected_count;
    let needs_attention = counts.failed > 0
        || counts.blocked > 0
        || counts.cancelled > 0
        || counts.missing > 0
        || counts.other > 0
        || counts.artifact_dirs_missing > 0
        || counts.apps_with_missing_required_files > 0
        || counts.validation_failed > 0;
    let report = serde_json::json!({
        "ok": true,
        "bench_green": bench_green,
        "needs_attention": needs_attention,
        "run_id": run_id,
        "suite": summary.get("suite").cloned().unwrap_or(serde_json::Value::Null),
        "model": summary.get("model").cloned().unwrap_or(serde_json::Value::Null),
        "context": summary.get("context").cloned().unwrap_or(serde_json::Value::Null),
        "expected_count": expected_count,
        "status_collected_at_ms": finished_at_ms,
        "validate": validate,
        "counts": counts.to_json(),
        "apps": apps,
        "evidence_dir": run_dir.display().to_string(),
        "status_path": status_path.display().to_string()
    });
    fs::write(&status_path, serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("failed to write bench status {}", status_path.display()))?;
    append_bench_event(
        &events_path,
        &serde_json::json!({
            "event": "status_collected",
            "run_id": report.get("run_id").and_then(serde_json::Value::as_str).unwrap_or_default(),
            "status_path": status_path.display().to_string(),
            "validate": validate,
            "bench_green": bench_green,
            "needs_attention": needs_attention,
            "counts": report.get("counts").cloned().unwrap_or(serde_json::Value::Null),
            "created_at_ms": finished_at_ms
        }),
    )?;
    Ok(report)
}

#[derive(Default)]
struct BenchStatusCounts {
    pending: usize,
    leased: usize,
    handled: usize,
    failed: usize,
    blocked: usize,
    cancelled: usize,
    missing: usize,
    other: usize,
    validation_passed: usize,
    validation_failed: usize,
    validation_skipped: usize,
    artifact_dirs_present: usize,
    artifact_dirs_missing: usize,
    apps_with_missing_required_files: usize,
}

impl BenchStatusCounts {
    fn observe_route_status(&mut self, route_status: &str) {
        match route_status {
            "pending" => self.pending = self.pending.saturating_add(1),
            "leased" => self.leased = self.leased.saturating_add(1),
            "handled" => self.handled = self.handled.saturating_add(1),
            "failed" => self.failed = self.failed.saturating_add(1),
            "blocked" => self.blocked = self.blocked.saturating_add(1),
            "cancelled" => self.cancelled = self.cancelled.saturating_add(1),
            "missing" => self.missing = self.missing.saturating_add(1),
            _ => self.other = self.other.saturating_add(1),
        }
    }

    fn observe_validation(&mut self, success: bool) {
        if success {
            self.validation_passed = self.validation_passed.saturating_add(1);
        } else {
            self.validation_failed = self.validation_failed.saturating_add(1);
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "pending": self.pending,
            "leased": self.leased,
            "handled": self.handled,
            "failed": self.failed,
            "blocked": self.blocked,
            "cancelled": self.cancelled,
            "missing": self.missing,
            "other": self.other,
            "validation_passed": self.validation_passed,
            "validation_failed": self.validation_failed,
            "validation_skipped": self.validation_skipped,
            "artifact_dirs_present": self.artifact_dirs_present,
            "artifact_dirs_missing": self.artifact_dirs_missing,
            "apps_with_missing_required_files": self.apps_with_missing_required_files
        })
    }
}

fn bench_app_artifact_report(module_dir: &Path) -> anyhow::Result<serde_json::Value> {
    const REQUIRED: &[&str] = &[
        "module.json",
        "collections.schema.json",
        "schema.js",
        "index.html",
        "index.css",
        "index.js",
        "icon.svg",
        "locales/en.json",
        "locales/de.json",
    ];
    let mut files = Vec::new();
    if module_dir.is_dir() {
        collect_relative_files(module_dir, module_dir, &mut files)?;
    }
    files.sort();
    let file_set = files.iter().cloned().collect::<BTreeSet<_>>();
    let mut required_missing = REQUIRED
        .iter()
        .filter(|path| !file_set.iter().any(|file| file == *path))
        .map(|path| serde_json::Value::String((*path).to_string()))
        .collect::<Vec<_>>();
    let tests_present = files
        .iter()
        .any(|file| file.starts_with("tests/") && file.ends_with(".test.mjs"));
    if !tests_present {
        required_missing.push(serde_json::Value::String("tests/*.test.mjs".to_string()));
    }
    Ok(serde_json::json!({
        "exists": module_dir.is_dir(),
        "file_count": files.len(),
        "files": files,
        "tests_present": tests_present,
        "required_missing": required_missing
    }))
}

fn collect_relative_files(root: &Path, dir: &Path, output: &mut Vec<String>) -> anyhow::Result<()> {
    if output.len() >= 512 {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_relative_files(root, &path, output)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            output.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn truncate_bench_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let kept = raw.chars().take(max_chars).collect::<String>();
    format!("{kept}\n... truncated ...")
}

fn business_os_app_reference_candidates(
    root: &Path,
    args: &[String],
) -> anyhow::Result<serde_json::Value> {
    let query = flag_value(args, "--query")
        .or_else(|| {
            args.iter()
                .find(|arg| !arg.starts_with("--"))
                .map(String::as_str)
        })
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let source_app_root = existing_dir_path(root, BUSINESS_OS_APP_CANDIDATES);
    let mut roots = vec![("source", source_app_root.join("modules"))];
    let installed_app_root =
        if root.join("runtime").exists() || root.join("runtime/business-os").exists() {
            root.join("runtime/business-os")
        } else {
            root.join("business-os")
        };
    roots.push(("installed", installed_app_root.join("installed-modules")));

    let mut modules = Vec::new();
    for (source, modules_root) in roots {
        if !modules_root.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&modules_root)
            .with_context(|| format!("failed to read {}", modules_root.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let module_dir = entry.path();
            let manifest_path = module_dir.join("module.json");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest_text = fs::read_to_string(&manifest_path)
                .with_context(|| format!("failed to read {}", manifest_path.display()))?;
            let manifest: serde_json::Value = serde_json::from_str(&manifest_text)
                .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
            let fallback_id = entry.file_name().to_string_lossy().to_string();
            let id = manifest
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(fallback_id.as_str())
                .to_owned();
            if id.trim().is_empty() {
                continue;
            }
            let title = manifest
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(id.as_str())
                .to_owned();
            let description = manifest
                .get("description")
                .or_else(|| manifest.get("store").and_then(|store| store.get("summary")))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();
            let searchable =
                format!("{id} {title} {description} {}", manifest_text).to_ascii_lowercase();
            if !query.is_empty() && !searchable.contains(&query) {
                continue;
            }
            let category = manifest
                .get("category")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let reference_kind = business_os_app_reference_kind(&id, &category);
            let layout = business_os_app_reference_layout(manifest.get("layout"));
            let warnings = business_os_app_reference_warnings(&manifest, &reference_kind);
            modules.push(serde_json::json!({
                "id": id,
                "title": title,
                "description": description,
                "source": source,
                "reference_kind": reference_kind,
                "recommended_for_generated_business_app": reference_kind == "business-workflow-reference",
                "path": module_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
                "entry": manifest.get("entry").cloned().unwrap_or(serde_json::Value::Null),
                "collections": manifest.get("collections").cloned().unwrap_or_else(|| serde_json::json!([])),
                "layout": layout,
                "category": category,
                "warnings": warnings,
                "runtime_manifest_contract": {
                    "entry": format!("installed-modules/<module-id>/index.html"),
                    "install_scope": "installed",
                    "icon": "Use icon.svg. Do not copy layout.icon_svg or inline SVG into module.json.",
                    "store": "Do not set store.installable for runtime-installed modules.",
                    "layout": "Prefer left + center or a modal/drawer. Use layout.right only with layout.third_pane_justification."
                }
            }));
        }
    }
    modules.sort_by(|a, b| {
        let a_recommended = a
            .get("recommended_for_generated_business_app")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let b_recommended = b
            .get("recommended_for_generated_business_app")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if a_recommended != b_recommended {
            return b_recommended.cmp(&a_recommended);
        }
        let a_title = a
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let b_title = b
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        a_title.cmp(b_title).then_with(|| {
            let a_id = a
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let b_id = b
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            a_id.cmp(b_id)
        })
    });
    Ok(serde_json::json!({
        "ok": true,
        "query": query,
        "instruction": "Choose the three most relevant business-workflow references yourself by matching workflow, data shape, and UI shape. Internal shell/developer modules are poor defaults unless the requested app is itself a shell/developer tool.",
        "runtime_rules": [
            "Do not copy source manifest entry paths. Runtime apps use entry installed-modules/<module-id>/index.html.",
            "Do not copy layout.icon_svg or any inline SVG from source manifests. Runtime apps keep SVG markup in icon.svg.",
            "Do not copy store.installable into runtime-installed module.json.",
            "Do not copy layout.right unless the app truly needs a third pane and module.json includes layout.third_pane_justification.",
            "The skill contract and validator override any source reference field that conflicts with runtime-installed app rules."
        ],
        "modules": modules,
    }))
}

fn business_os_app_reference_kind(id: &str, category: &serde_json::Value) -> &'static str {
    let category = category.as_str().unwrap_or("").trim().to_ascii_lowercase();
    if matches!(
        id,
        "app-store" | "browser" | "coding-agents" | "creator" | "credentials" | "ctox"
    ) || matches!(
        category.as_str(),
        "development" | "security" | "system" | "workspace"
    ) {
        "internal-shell-reference"
    } else {
        "business-workflow-reference"
    }
}

fn business_os_app_reference_layout(layout: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(layout) = layout.and_then(serde_json::Value::as_object) else {
        return serde_json::Value::Null;
    };
    let mut output = serde_json::Map::new();
    for key in [
        "shell",
        "launch_kind",
        "left",
        "center",
        "top",
        "bottom",
        "drawers",
        "third_pane_justification",
    ] {
        if let Some(value) = layout.get(key) {
            output.insert(key.to_string(), value.clone());
        }
    }
    if let Some(value) = layout.get("right") {
        output.insert("right".to_string(), value.clone());
        output.insert(
            "right_pane_is_exception".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    output.insert(
        "icon_source".to_string(),
        serde_json::Value::String("icon.svg for generated runtime apps".to_string()),
    );
    serde_json::Value::Object(output)
}

fn business_os_app_reference_warnings(
    manifest: &serde_json::Value,
    reference_kind: &str,
) -> Vec<&'static str> {
    let mut warnings = Vec::new();
    if reference_kind != "business-workflow-reference" {
        warnings.push(
            "Internal shell/developer module: inspect sparingly; do not use as a default business-app UI template.",
        );
    }
    if manifest.pointer("/layout/icon_svg").is_some() {
        warnings.push(
            "Source manifest contains layout.icon_svg; generated runtime apps must use icon.svg instead.",
        );
    }
    if manifest.pointer("/store/installable").is_some() {
        warnings.push(
            "Source manifest contains store.installable; runtime-installed module.json must not copy it.",
        );
    }
    if manifest.pointer("/layout/right").is_some()
        && manifest
            .pointer("/layout/third_pane_justification")
            .is_none()
    {
        warnings.push(
            "Source manifest uses layout.right without third_pane_justification; generated apps should prefer two panes or modal/detail workflows.",
        );
    }
    warnings
}

fn run_business_os_app_validator(
    root: &Path,
    module_id: &str,
    args: &[String],
) -> anyhow::Result<std::process::Output> {
    if module_id.is_empty()
        || module_id == "."
        || module_id == ".."
        || module_id.contains('/')
        || module_id.contains('\\')
    {
        anyhow::bail!("invalid Business OS app module id `{module_id}`");
    }
    let script = root.join("src/apps/business-os/scripts/validate-app-module.mjs");
    anyhow::ensure!(
        script.is_file(),
        "Business OS app validator is not available at {}",
        script.display()
    );
    let mut command = Command::new(resolve_business_os_validator_node(root));
    command.current_dir(root).arg(script).arg(module_id);
    let mut workspace_root = root.to_path_buf();
    let mut has_mode = false;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--installed" | "--source" | "--json" | "--skip-tests" | "--skip-node-check" => {
                if args[idx] == "--installed" || args[idx] == "--source" {
                    has_mode = true;
                }
                command.arg(&args[idx]);
                idx += 1;
            }
            "--workspace" => {
                let value = args
                    .get(idx + 1)
                    .with_context(|| format!("{} requires a value", args[idx]))?;
                workspace_root = PathBuf::from(value);
                idx += 2;
            }
            "--task-id" | "--reason" => {
                idx += 2;
            }
            value if value.starts_with("--") => {
                anyhow::bail!("unsupported business-os app validator option `{value}`")
            }
            _ => {
                idx += 1;
            }
        }
    }
    if !has_mode
        && workspace_root
            .join("runtime/business-os/installed-modules")
            .is_dir()
    {
        command.arg("--installed");
    }
    command.arg("--workspace").arg(&workspace_root);
    command
        .output()
        .context("failed to run Business OS app validator")
}

fn run_business_os_app_smoke(
    root: &Path,
    module_id: &str,
    args: &[String],
) -> anyhow::Result<std::process::Output> {
    if module_id.is_empty()
        || module_id == "."
        || module_id == ".."
        || module_id.contains('/')
        || module_id.contains('\\')
    {
        anyhow::bail!("invalid Business OS app module id `{module_id}`");
    }
    let script = root.join("src/apps/business-os/scripts/smoke-app-module.mjs");
    anyhow::ensure!(
        script.is_file(),
        "Business OS app browser smoke is not available at {}",
        script.display()
    );
    let mut command = Command::new(resolve_business_os_validator_node(root));
    command.current_dir(root).arg(script).arg(module_id);
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--installed" | "--source" | "--json" => {
                command.arg(&args[idx]);
                idx += 1;
            }
            "--url" | "--create-action" | "--timeout-ms" | "--output" | "--screenshot" => {
                let value = args
                    .get(idx + 1)
                    .with_context(|| format!("{} requires a value", args[idx]))?;
                command.arg(&args[idx]).arg(value);
                idx += 2;
            }
            value if value.starts_with("--") => {
                anyhow::bail!("unsupported business-os app smoke option `{value}`")
            }
            _ => {
                idx += 1;
            }
        }
    }
    command
        .output()
        .context("failed to run Business OS app browser smoke")
}

fn run_business_os_app_e2e(
    root: &Path,
    module_id: &str,
    args: &[String],
) -> anyhow::Result<std::process::Output> {
    if module_id.is_empty()
        || module_id == "."
        || module_id == ".."
        || module_id.contains('/')
        || module_id.contains('\\')
    {
        anyhow::bail!("invalid Business OS app module id `{module_id}`");
    }
    let script = root.join("src/apps/business-os/scripts/e2e-app-module.mjs");
    anyhow::ensure!(
        script.is_file(),
        "Business OS app browser E2E is not available at {}",
        script.display()
    );
    let mut command = Command::new(resolve_business_os_validator_node(root));
    command.current_dir(root).arg(script).arg(module_id);
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--installed" | "--source" | "--json" => {
                command.arg(&args[idx]);
                idx += 1;
            }
            "--url" | "--timeout-ms" | "--output" | "--screenshot" | "--marker" => {
                let value = args
                    .get(idx + 1)
                    .with_context(|| format!("{} requires a value", args[idx]))?;
                command.arg(&args[idx]).arg(value);
                idx += 2;
            }
            value if value.starts_with("--") => {
                anyhow::bail!("unsupported business-os app e2e option `{value}`")
            }
            _ => {
                idx += 1;
            }
        }
    }
    command
        .output()
        .context("failed to run Business OS app browser E2E")
}

pub(crate) fn resolve_business_os_validator_node(_root: &Path) -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(path) = env::var("PATH") {
        candidates.extend(env::split_paths(&path).map(|dir| dir.join("node")));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/node"),
        PathBuf::from("/usr/local/bin/node"),
        PathBuf::from("/usr/bin/node"),
    ]);
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("node"))
}

fn app_validator_args_from_finalize_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for arg in args.iter().skip(2) {
        if skip_next {
            skip_next = false;
            continue;
        }
        match arg.as_str() {
            "--task-id" | "--reason" => skip_next = true,
            "--installed" | "--source" | "--json" | "--skip-tests" | "--skip-node-check" => {
                out.push(arg.clone())
            }
            _ => {}
        }
    }
    if !out
        .iter()
        .any(|arg| arg == "--installed" || arg == "--source")
    {
        out.push("--installed".to_string());
    }
    out
}

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
            let mut idx = 1;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--user" | "--user-id" => {
                        user_id = args.get(idx + 1).cloned();
                        idx += 2;
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
            let (token, expires_at_ms) =
                crate::business_os::store::issue_business_os_capability_token(root, &user_id, now)?;
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
        Some("process") | Some("process-source-parse") => {
            let command_id = args
                .get(1)
                .context("usage: ctox business-os commands process <command-id>")?;
            let accepted =
                crate::business_os::store::process_source_parse_command(root, command_id)?;
            print_json(&serde_json::to_value(accepted)?)
        }
        Some("dispatch") => {
            // Agent-facing entry point for writeback commands (e.g. the
            // `outbound.pipeline.write_outreach_draft` draft writeback). The
            // document is fed through the exact same RxDB command-bus path the
            // native peer uses — no HTTP, no external gateway.
            let document = read_command_document(args)?;
            let accepted = crate::business_os::store::accept_rxdb_business_command(root, document)?;
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

fn handle_business_os_web_stack(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("person-research") => {
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
            let workspace = flag_value(args, "--workspace").map(PathBuf::from);
            let persist_workspace = !args.iter().any(|arg| arg == "--no-workspace");
            let mut payload = ctox_web_stack::run_ctox_person_research_tool(
                root,
                &ctox_web_stack::PersonResearchRequest {
                    company: company.to_string(),
                    country,
                    mode,
                    fields,
                    include_private,
                    workspace,
                    persist_workspace,
                },
            )?;
            if args.iter().any(|arg| arg == "--auto-auth-assist") {
                let generated_task_id = format!(
                    "person_research_{}_{}_{}",
                    rxdb_id_slug(&company),
                    country.as_iso().to_ascii_lowercase(),
                    mode.as_str()
                );
                let requesting_task_id = flag_value(args, "--task-id")
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(generated_task_id.as_str());
                let mut commands = Vec::new();
                let tasks = payload
                    .get("browser_assist_tasks")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for task in tasks {
                    let Some(source_id) = task.get("source_id").and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    let command = enqueue_web_stack_auth_assist_request(
                        root,
                        source_id,
                        None,
                        requesting_task_id,
                        "ctox_web_stack",
                        "ctox_business_os_web_stack_person_research",
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
            }
            print_json(&payload)
        }
        Some("auth-assist-request") => {
            let source_id = flag_value(args, "--source-id")
                .context("usage: ctox business-os web-stack auth-assist-request --source-id <id> [--target-url <url>] [--task-id <id>]")?;
            let target_url_override = flag_value(args, "--target-url");
            let requesting_task_id = flag_value(args, "--task-id").unwrap_or_default();
            let summary = enqueue_web_stack_auth_assist_request(
                root,
                source_id,
                target_url_override,
                requesting_task_id,
                "ctox_harness",
                "ctox_web_auth_assist_request",
                false,
            )?;
            print_json(&summary)
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

fn install_business_os(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let target = flag_value(args, "--target")
        .or_else(|| args.first().filter(|value| !value.starts_with("--")).map(String::as_str))
        .map(PathBuf::from)
        .context("usage: ctox business-os install --target <empty-dir> [--init-git] [--dry-run] [--no-copy-env]")?;

    let installer = existing_file_path(root, BUSINESS_STACK_INSTALLER_CANDIDATES);
    if !installer.is_file() {
        anyhow::bail!("Business OS installer is missing: {}", installer.display());
    }

    let mut command = Command::new("python3");
    command
        .arg(&installer)
        .arg("--ctox-repo")
        .arg(root)
        .arg("--target")
        .arg(target);

    if args.iter().any(|arg| arg == "--init-git") {
        command.arg("--init-git");
    }
    if args.iter().any(|arg| arg == "--dry-run") {
        command.arg("--dry-run");
    }
    if args.iter().any(|arg| arg == "--no-copy-env") {
        command.arg("--no-copy-env");
    }

    let status = command
        .status()
        .context("failed to run CTOX Business OS installer with python3")?;
    if !status.success() {
        anyhow::bail!("CTOX Business OS installer failed with status {status}");
    }
    Ok(())
}

fn print_business_os_help() {
    println!("{}", business_os_usage());
    println!();
    println!("{}", business_os_status_text(Path::new(".")));
}

fn business_os_usage() -> String {
    business_os_usage_base()
        .replace(
            "  ctox business-os app validate <module-id> [--installed|--source] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]",
            "  ctox business-os app create --instruction <text> [--module-id <id>]\n  ctox business-os app modify <module-id> --instruction <text>\n  ctox business-os app references [--query <text>] [--json]\n  ctox business-os app validate <module-id> [--installed|--source] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]\n  ctox business-os app smoke <module-id> [--installed|--source] [--url <business-os-url>] [--json] [--timeout-ms <n>] [--output <path>] [--screenshot <path>]\n  ctox business-os app e2e <module-id> [--installed|--source] [--url <business-os-url>] [--json] [--timeout-ms <n>] [--output <path>] [--screenshot <path>] [--marker <value>]",
        )
        .replace(
            "  ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]",
            "  ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]\n  ctox business-os app bench status --run-id <id> [--validate]",
        )
        .replace(
            "  ctox business-os backup prune-drills [--dry-run]",
            "  ctox business-os backup inspect-manifest --manifest <path>\n  ctox business-os backup key-escrow-status\n  ctox business-os backup prune-drills [--dry-run]",
        )
}

fn business_os_usage_base() -> &'static str {
    "usage:\n  ctox business-os status\n  ctox business-os serve [--addr 127.0.0.1:8765]\n  ctox business-os mcp status\n  ctox business-os mcp tools\n  ctox business-os mcp policy\n  ctox business-os mcp policy keys\n  ctox business-os mcp policy set [--enabled true|false] [--allow-reads true|false] [--allow-writes true|false] [--allow-approvals true|false] [--allow-external-effects true|false] [--rate-limit-per-minute <n>] [--audit-retention-days <n>] [--allow-actor <id>]... [--allow-workspace <id>]... [--allow-module <id>]... [--allow-collection <name>]... [--deny-tool business_os.<tool>]... [--clear-deny-tools]\n  ctox business-os mcp call <tool-name> [--args <json>]\n  ctox business-os mcp audit [--limit <n>] [--format json|jsonl] [--output <path>] [--prune]\n  ctox business-os mcp serve [--addr 127.0.0.1:8788]\n  ctox business-os mcp connect --url wss://mcp.ctox.dev/connect/<instance-id> [--token <token>] [--once] [--max-reconnect-delay-ms <n>] [--heartbeat-interval-ms <n>] [--max-connection-age-ms <n>]\n  ctox business-os mcp gateway-status --url https://mcp.ctox.dev/status/<instance-id> [--token <token>]\n  ctox business-os peer status\n  ctox business-os peer rotate\n  ctox business-os peer start\n  ctox business-os desktop invite [--display-name <name>] [--ttl-hours <n> | --expires-at <rfc3339>] [--format json|link] [--output <path>]\n  ctox business-os rxdb repair-optional-drift --collection <name> [--dry-run] [--force]\n  ctox business-os app create --instruction <text> [--module-id <id>]\n  ctox business-os app modify <module-id> --instruction <text>\n  ctox business-os app validate <module-id> [--installed|--source] [--workspace <path>] [--json] [--skip-tests] [--skip-node-check]\n  ctox business-os app finalize <module-id> --task-id <queue-task-id> [--installed|--source] [--reason <text>]\n  ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k [--run-id <id>] [--actor <user-id>] [--no-clean]\n  ctox business-os repair queue-projections (--dry-run | --apply)\n  ctox business-os backup restore-drill [--module <module-id>]\n  ctox business-os backup prune-drills [--dry-run]\n  ctox business-os install --target <empty-dir> [--init-git] [--dry-run] [--no-copy-env]\n  ctox business-os commands process <command-id>\n  ctox business-os commands dispatch (--input <path> | --json <json> | <json>)\n  ctox business-os web-stack person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--auto-auth-assist] [--task-id <id>] [--workspace <path>] [--no-workspace]\n  ctox business-os web-stack auth-assist-request --source-id <id> [--target-url <url>] [--task-id <id>]\n  ctox business-os web-stack auth-assist-status --session-id <id>\n  ctox business-os web-stack context-capture --session-id <id> [--source-id <id>] [--task-id <id>] [--no-handoff]\n  ctox business-os web-stack context-extract --session-id <id> [--source-id <id>] [--capture-script <id>] [--task-id <id>]\n  ctox business-os web-stack redaction-audit --canary <value> [--canary <value>]... [--path <path>]...\n  ctox business-os web-stack browser-doctor [--dir <path>]\n  ctox business-os files sync <path>\n  ctox business-os files sync-workspace <path>\n  ctox business-os modules list\n  ctox business-os modules enable <module>\n  ctox business-os modules disable <module> [--force-remove-skills]\n  ctox business-os skills list\n  ctox business-os skills enable <skill>\n  ctox business-os skills disable <skill> [--force-remove]"
}

fn exists_label(exists: bool) -> &'static str {
    if exists {
        "ok"
    } else {
        "missing"
    }
}

fn existing_file_path(root: &Path, candidates: &[&str]) -> PathBuf {
    candidates
        .iter()
        .map(|candidate| root.join(candidate))
        .find(|path| path.is_file())
        .unwrap_or_else(|| root.join(candidates[0]))
}

fn existing_dir_path(root: &Path, candidates: &[&str]) -> PathBuf {
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

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn args_have_help(args: &[String]) -> bool {
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

fn enqueue_web_stack_auth_assist_request(
    root: &Path,
    source_id: &str,
    target_url_override: Option<&str>,
    requesting_task_id: &str,
    source_module: &str,
    command_path: &str,
    deterministic_for_task: bool,
) -> anyhow::Result<serde_json::Value> {
    let module = ctox_web_stack::sources::find(source_id)
        .with_context(|| format!("unknown web-stack source: {source_id}"))?;
    let recipe = module.browser_recipe();
    let target_url = target_url_override
        .map(str::to_string)
        .or_else(|| recipe.as_ref().map(|recipe| recipe.login_url.clone()))
        .with_context(|| {
            format!(
                "web-stack source `{}` has no browser auth-assist recipe",
                module.id()
            )
        })?;
    let allowed_domains = recipe
        .as_ref()
        .map(|recipe| recipe.allowed_domains.clone())
        .filter(|domains| !domains.is_empty())
        .unwrap_or_else(|| allowed_domains_from_url(&target_url));
    let secret_name = recipe
        .as_ref()
        .and_then(|recipe| recipe.required_secret_name)
        .or_else(|| module.requires_credential())
        .unwrap_or_default();
    let now = now_ms();
    let source_slug = rxdb_id_slug(module.id());
    let dedupe_key = format!(
        "{}:{}",
        module.id(),
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
    let session_id = format!("browser_session_web_stack_auth_{source_slug}");
    let tab_id = format!("browser_tab_web_stack_auth_{source_slug}");
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
        "record_id": module.id(),
        "status": "pending_sync",
        "payload": {
            "session_id": session_id.clone(),
            "tab_id": tab_id.clone(),
            "source_id": module.id(),
            "secret_name": secret_name,
            "target_url": target_url.clone(),
            "allowed_domains": allowed_domains.clone(),
            "verify_selector": verify_selector,
            "credential_selector": credential_selector,
            "capture_script": capture_script,
            "purpose": "web_stack_auth",
            "expires_at_ms": now + 30 * 60 * 1000,
            "browser_stream": "rxdb",
            "secret_value_in_rxdb": false,
            "dedupe_key": dedupe_key,
            "requesting_task_id": requesting_task_id,
        },
        "client_context": {
            "source_module": source_module,
            "command_path": command_path,
            "actor": {
                "user_id": source_module,
                "display_name": source_module,
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
        "command_id": stored.get("command_id").and_then(serde_json::Value::as_str).unwrap_or_default(),
        "session_id": session_id,
        "tab_id": tab_id,
        "source_id": module.id(),
        "target_url": target_url,
        "allowed_domains": allowed_domains,
        "required_secret_name": secret_name,
        "verify_selector": verify_selector,
        "credential_selector": credential_selector,
        "capture_script": capture_script,
        "dedupe_key": dedupe_key,
        "deduped_by_command_id": deterministic_for_task,
        "browser_stream": "rxdb",
        "secret_value_in_payload": false,
        "status": "pending_sync"
    }))
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for task in tasks {
            assert_eq!(
                task.suggested_skill.as_deref(),
                Some(BUSINESS_OS_APP_BENCH_SKILL)
            );
            assert!(task.prompt.contains("ctox.business_os.app.create"));
            assert!(task
                .prompt
                .contains("runtime/business-os/installed-modules/bench_"));
            assert!(task
                .prompt
                .contains("ctox business-os app references --json"));
            assert!(
                task.prompt.contains("\"id\": \"local-dev\""),
                "bench tasks without --actor must target the local Business OS user"
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
        assert!(task
            .prompt
            .contains("ctox business-os app references --json"));
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
                extra_metadata: None,
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
        assert!(!invite
            .get("signaling_room_password")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .is_empty());

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
        assert_eq!(decoded_invite.get("desktop_link"), None);
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
