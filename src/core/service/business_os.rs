// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
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
        Some("install") => install_business_os(root, &args[1..]),
        Some("commands") => handle_business_os_commands(root, &args[1..]),
        Some("web-stack") => handle_business_os_web_stack(root, &args[1..]),
        Some("files") => handle_business_os_files(root, &args[1..]),
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
        Some("--help") | Some("-h") | None => {
            println!("{}", business_os_usage());
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown business-os commands command `{other}`"),
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
    "usage:\n  ctox business-os status\n  ctox business-os serve [--addr 127.0.0.1:8765]\n  ctox business-os peer status\n  ctox business-os peer rotate\n  ctox business-os peer start\n  ctox business-os rxdb repair-optional-drift --collection <name> [--dry-run] [--force]\n  ctox business-os install --target <empty-dir> [--init-git] [--dry-run] [--no-copy-env]\n  ctox business-os commands process <command-id>\n  ctox business-os web-stack person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--auto-auth-assist] [--task-id <id>] [--workspace <path>] [--no-workspace]\n  ctox business-os web-stack auth-assist-request --source-id <id> [--target-url <url>] [--task-id <id>]\n  ctox business-os web-stack auth-assist-status --session-id <id>\n  ctox business-os web-stack context-capture --session-id <id> [--source-id <id>] [--task-id <id>] [--no-handoff]\n  ctox business-os web-stack context-extract --session-id <id> [--source-id <id>] [--capture-script <id>] [--task-id <id>]\n  ctox business-os web-stack redaction-audit --canary <value> [--canary <value>]... [--path <path>]...\n  ctox business-os web-stack browser-doctor [--dir <path>]\n  ctox business-os files sync <path>\n  ctox business-os files sync-workspace <path>\n  ctox business-os modules list\n  ctox business-os modules enable <module>\n  ctox business-os modules disable <module> [--force-remove-skills]\n  ctox business-os skills list\n  ctox business-os skills enable <skill>\n  ctox business-os skills disable <skill> [--force-remove]"
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
