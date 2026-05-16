// Origin: CTOX
// License: Apache-2.0

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::persistence;
use crate::skill_store;

const BUSINESS_STACK_SKILL: &str = "skills/system/product_engineering/business-stack/SKILL.md";
const BUSINESS_STACK_INSTALLER: &str =
    "skills/system/product_engineering/business-stack/scripts/install_business_stack.py";
const BUSINESS_STACK_TEMPLATE: &str = "templates/business-basic";
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
    binding("doc", "content", "Documents", "documents", "library"),
    binding("pdf", "content", "PDF", "documents", "library"),
    binding("spreadsheet", "content", "Spreadsheets", "documents", "spreadsheets"),
    binding("slides", "content", "Slides", "documents", "slides"),
    binding("technical-drawing-review", "content", "Technical Drawing Review", "documents", "drawings"),
    binding("transcribe", "content", "Transcribe", "documents", "transcripts"),
    binding("screenshot", "content", "Screenshot", "content", "assets"),
    binding("imagegen", "content", "Image Generation", "content", "images"),
    binding("sora", "content", "Sora", "content", "video"),
    binding("speech", "content", "Speech", "content", "voice"),
    binding("figma", "design", "Figma", "content", "design"),
    binding("figma-implement-design", "design", "Figma Implementation", "content", "design"),
    binding("frontend-skill", "development", "Frontend Skill", "content", "web"),
    binding("aspnet-core", "development", "ASP.NET Core", "developer", "frameworks"),
    binding("chatgpt-apps", "development", "ChatGPT Apps", "developer", "apps"),
    binding("develop-web-game", "development", "Web Game Development", "developer", "apps"),
    binding("jupyter-notebook", "development", "Jupyter Notebook", "developer", "notebooks"),
    binding("nextjs-postgres-port", "development", "Next.js Postgres Port", "developer", "frameworks"),
    binding("winui-app", "development", "WinUI App", "developer", "apps"),
    binding("gh-address-comments", "git", "Address PR Comments", "developer", "source-control"),
    binding("gh-fix-ci", "git", "Fix CI", "developer", "quality"),
    binding("yeet", "git", "Publish PR", "developer", "source-control"),
    binding("playwright", "testing", "Playwright", "developer", "quality"),
    binding("playwright-interactive", "testing", "Playwright Interactive", "developer", "quality"),
    binding("cloudflare-deploy", "deploy", "Cloudflare Deploy", "deployment", "cloudflare"),
    binding("netlify-deploy", "deploy", "Netlify Deploy", "deployment", "netlify"),
    binding("render-deploy", "deploy", "Render Deploy", "deployment", "render"),
    binding("vercel-deploy", "deploy", "Vercel Deploy", "deployment", "vercel"),
    binding("security-best-practices", "security", "Security Best Practices", "security", "best-practices"),
    binding("security-ownership-map", "security", "Security Ownership Map", "security", "ownership"),
    binding("security-threat-model", "security", "Security Threat Model", "security", "threat-models"),
    binding("linear", "integration", "Linear", "integrations", "linear"),
    binding("notion-knowledge-capture", "integration", "Notion Knowledge Capture", "integrations", "notion"),
    binding("notion-meeting-intelligence", "integration", "Notion Meeting Intelligence", "integrations", "notion"),
    binding("notion-spec-to-implementation", "integration", "Notion Spec to Implementation", "integrations", "notion"),
    binding("openai-docs", "reference", "OpenAI Docs", "research", "openai-docs"),
    binding("notion-research-documentation", "integration", "Notion Research Documentation", "research", "notion-research"),
    binding("sentry", "integration", "Sentry", "support", "monitoring"),
    binding("zammad-rest", "vendor", "Zammad REST", "support", "zammad"),
    binding("zammad-printengine-monitoring-sim", "vendor", "Zammad Print Engine Monitoring", "support", "monitoring"),
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
        Some("install") => install_business_os(root, &args[1..]),
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
    let skill = root.join(BUSINESS_STACK_SKILL);
    let installer = root.join(BUSINESS_STACK_INSTALLER);
    let template = root.join(BUSINESS_STACK_TEMPLATE);
    let manifest = template.join("ctox-business.json");

    format!(
        "CTOX Business OS\n\
         Source skill: {skill_status}  {skill}\n\
         Installer:    {installer_status}  {installer}\n\
         Template:     {template_status}  {template}\n\
         Manifest:     {manifest_status}  {manifest}\n\n\
         Install into a separate customer-owned repository:\n\
           ctox business-os install --target <empty-dir> --init-git\n\n\
         Preview first:\n\
           ctox business-os install --target <empty-dir> --dry-run\n\n\
         Runtime contract:\n\
           - Business OS code lives outside CTOX core after installation.\n\
           - CTOX core upgrades never overwrite the generated repository in place.\n\
           - The generated repo owns the Next.js app, Postgres schema, public website bridge, and business modules.\n",
        skill_status = exists_label(skill.is_file()),
        installer_status = exists_label(installer.is_file()),
        template_status = exists_label(template.is_dir()),
        manifest_status = exists_label(manifest.is_file()),
        skill = skill.display(),
        installer = installer.display(),
        template = template.display(),
        manifest = manifest.display(),
    )
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
            let module_id = args
                .get(1)
                .context("usage: ctox business-os modules disable <module> [--force-remove-skills]")?;
            if is_core_module(module_id) {
                anyhow::bail!("core Business OS module cannot be disabled: {module_id}");
            }
            ensure_known_skill_module(module_id)?;
            let force = args.iter().any(|arg| arg == "--force-remove-skills");
            let mut activation = load_activation(root)?;
            activation.enabled_modules.retain(|module| module != module_id);
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
            activation.enabled_skills.retain(|skill| skill != binding.skill_id);
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

    let installer = root.join(BUSINESS_STACK_INSTALLER);
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
    "usage:\n  ctox business-os status\n  ctox business-os install --target <empty-dir> [--init-git] [--dry-run] [--no-copy-env]\n  ctox business-os modules list\n  ctox business-os modules enable <module>\n  ctox business-os modules disable <module> [--force-remove-skills]\n  ctox business-os skills list\n  ctox business-os skills enable <skill>\n  ctox business-os skills disable <skill> [--force-remove]"
}

fn exists_label(exists: bool) -> &'static str {
    if exists {
        "ok"
    } else {
        "missing"
    }
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
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
    let mut activation = persistence::load_json_payload::<BusinessOsActivation>(
        root,
        ACTIVATION_PAYLOAD_KEY,
    )?
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
        activation.enabled_skills.retain(|skill| skill != binding.skill_id);
        let result = skill_store::remove_installed_source_pack(root, binding.skill_id, force)?;
        removed.push(serde_json::to_value(result)?);
    }
    Ok(removed)
}

fn print_json(value: &serde_json::Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
