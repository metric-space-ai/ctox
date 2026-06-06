// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use url::Url;
use uuid::Uuid;

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
        "business-os-matching",
        "business",
        "Business OS Matching",
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
        Some("repair") => handle_business_os_repair(root, &args[1..]),
        Some("install") => install_business_os(root, &args[1..]),
        Some("commands") => handle_business_os_commands(root, &args[1..]),
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
            let mut env_map = crate::inference::runtime_env::effective_operator_env_map(root)
                .unwrap_or_else(|_| BTreeMap::new());
            apply_mcp_policy_flag(
                args,
                &mut env_map,
                "--enabled",
                "CTOX_BUSINESS_OS_MCP_ENABLED",
            )?;
            apply_mcp_policy_flag(
                args,
                &mut env_map,
                "--allow-reads",
                "CTOX_BUSINESS_OS_MCP_ALLOW_READS",
            )?;
            apply_mcp_policy_flag(
                args,
                &mut env_map,
                "--allow-writes",
                "CTOX_BUSINESS_OS_MCP_ALLOW_WRITES",
            )?;
            apply_mcp_policy_flag(
                args,
                &mut env_map,
                "--allow-approvals",
                "CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS",
            )?;
            apply_mcp_policy_flag(
                args,
                &mut env_map,
                "--allow-external-effects",
                "CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS",
            )?;
            apply_mcp_policy_usize_flag(
                args,
                &mut env_map,
                "--rate-limit-per-minute",
                "CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE",
            )?;
            apply_mcp_policy_usize_flag(
                args,
                &mut env_map,
                "--audit-retention-days",
                "CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS",
            )?;
            if args.iter().any(|arg| arg == "--clear-deny-tools") {
                env_map.remove("CTOX_BUSINESS_OS_MCP_DENY_TOOLS");
            }
            for (flag, key) in [
                (
                    "--clear-allowed-actors",
                    "CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS",
                ),
                (
                    "--clear-allowed-workspaces",
                    "CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES",
                ),
                (
                    "--clear-allowed-modules",
                    "CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES",
                ),
                (
                    "--clear-allowed-collections",
                    "CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS",
                ),
            ] {
                if args.iter().any(|arg| arg == flag) {
                    env_map.remove(key);
                }
            }
            apply_mcp_policy_csv_flag(
                args,
                &mut env_map,
                "--allow-actor",
                "CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS",
            );
            apply_mcp_policy_csv_flag(
                args,
                &mut env_map,
                "--allow-workspace",
                "CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES",
            );
            apply_mcp_policy_csv_flag(
                args,
                &mut env_map,
                "--allow-module",
                "CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES",
            );
            apply_mcp_policy_csv_flag(
                args,
                &mut env_map,
                "--allow-collection",
                "CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS",
            );
            let deny_tools = mcp_policy_deny_tools_from_args(args)?;
            if !deny_tools.is_empty() {
                env_map.insert(
                    "CTOX_BUSINESS_OS_MCP_DENY_TOOLS".to_string(),
                    deny_tools.join(","),
                );
            }
            crate::inference::runtime_env::save_runtime_env_map(root, &env_map)?;
            print_json(&serde_json::json!({
                "ok": true,
                "policy": crate::business_os::mcp_channel::mcp_policy(root),
                "keys": mcp_policy_env_projection(&env_map)
            }))
        }
        Some("keys") => {
            let env_map = crate::inference::runtime_env::effective_operator_env_map(root)
                .unwrap_or_else(|_| BTreeMap::new());
            print_json(&serde_json::json!({
                "ok": true,
                "keys": mcp_policy_env_projection(&env_map)
            }))
        }
        Some("--help") | Some("-h") => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os mcp policy command `{other}`"),
        None => {
            let env_map = crate::inference::runtime_env::effective_operator_env_map(root)
                .unwrap_or_else(|_| BTreeMap::new());
            print_json(&serde_json::json!({
                "ok": true,
                "policy": crate::business_os::mcp_channel::mcp_policy(root),
                "keys": mcp_policy_env_projection(&env_map)
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

fn business_os_usage() -> &'static str {
    "usage:\n  ctox business-os status\n  ctox business-os serve [--addr 127.0.0.1:8765]\n  ctox business-os mcp status\n  ctox business-os mcp tools\n  ctox business-os mcp policy\n  ctox business-os mcp policy keys\n  ctox business-os mcp policy set [--enabled true|false] [--allow-reads true|false] [--allow-writes true|false] [--allow-approvals true|false] [--allow-external-effects true|false] [--rate-limit-per-minute <n>] [--audit-retention-days <n>] [--allow-actor <id>]... [--allow-workspace <id>]... [--allow-module <id>]... [--allow-collection <name>]... [--deny-tool business_os.<tool>]... [--clear-deny-tools]\n  ctox business-os mcp call <tool-name> [--args <json>]\n  ctox business-os mcp audit [--limit <n>] [--format json|jsonl] [--output <path>] [--prune]\n  ctox business-os mcp serve [--addr 127.0.0.1:8788]\n  ctox business-os mcp connect --url wss://mcp.ctox.dev/connect/<instance-id> [--token <token>] [--once] [--max-reconnect-delay-ms <n>] [--heartbeat-interval-ms <n>] [--max-connection-age-ms <n>]\n  ctox business-os mcp gateway-status --url https://mcp.ctox.dev/status/<instance-id> [--token <token>]\n  ctox business-os peer status\n  ctox business-os peer rotate\n  ctox business-os peer start\n  ctox business-os rxdb repair-optional-drift --collection <name> [--dry-run] [--force]\n  ctox business-os repair queue-projections (--dry-run | --apply)\n  ctox business-os install --target <empty-dir> [--init-git] [--dry-run] [--no-copy-env]\n  ctox business-os commands process <command-id>\n  ctox business-os commands dispatch (--input <path> | --json <json> | <json>)\n  ctox business-os web-stack person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--auto-auth-assist] [--task-id <id>] [--workspace <path>] [--no-workspace]\n  ctox business-os web-stack auth-assist-request --source-id <id> [--target-url <url>] [--task-id <id>]\n  ctox business-os web-stack auth-assist-status --session-id <id>\n  ctox business-os web-stack context-capture --session-id <id> [--source-id <id>] [--task-id <id>] [--no-handoff]\n  ctox business-os web-stack context-extract --session-id <id> [--source-id <id>] [--capture-script <id>] [--task-id <id>]\n  ctox business-os web-stack redaction-audit --canary <value> [--canary <value>]... [--path <path>]...\n  ctox business-os web-stack browser-doctor [--dir <path>]\n  ctox business-os files sync <path>\n  ctox business-os files sync-workspace <path>\n  ctox business-os modules list\n  ctox business-os modules enable <module>\n  ctox business-os modules disable <module> [--force-remove-skills]\n  ctox business-os skills list\n  ctox business-os skills enable <skill>\n  ctox business-os skills disable <skill> [--force-remove]"
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

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
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

fn apply_mcp_policy_flag(
    args: &[String],
    env_map: &mut BTreeMap<String, String>,
    flag: &str,
    key: &str,
) -> anyhow::Result<()> {
    let Some(raw) = flag_value(args, flag) else {
        return Ok(());
    };
    let value = parse_cli_bool(raw).with_context(|| format!("invalid value for {flag}: {raw}"))?;
    env_map.insert(key.to_string(), value.to_string());
    Ok(())
}

fn apply_mcp_policy_usize_flag(
    args: &[String],
    env_map: &mut BTreeMap<String, String>,
    flag: &str,
    key: &str,
) -> anyhow::Result<()> {
    let Some(raw) = flag_value(args, flag) else {
        return Ok(());
    };
    let value = raw
        .trim()
        .parse::<usize>()
        .with_context(|| format!("invalid value for {flag}: {raw}"))?;
    env_map.insert(key.to_string(), value.to_string());
    Ok(())
}

fn apply_mcp_policy_csv_flag(
    args: &[String],
    env_map: &mut BTreeMap<String, String>,
    flag: &str,
    key: &str,
) {
    let values = flag_values(args, flag)
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !values.is_empty() {
        env_map.insert(key.to_string(), values.join(","));
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
    fn mcp_policy_csv_flags_are_deduplicated() {
        let args = vec![
            "set".to_string(),
            "--allow-module".to_string(),
            "customers,outbound".to_string(),
            "--allow-module".to_string(),
            "customers".to_string(),
        ];
        let mut env_map = BTreeMap::new();

        apply_mcp_policy_csv_flag(
            &args,
            &mut env_map,
            "--allow-module",
            "CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES",
        );

        assert_eq!(
            env_map
                .get("CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES")
                .map(String::as_str),
            Some("customers,outbound")
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
