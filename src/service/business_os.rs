// Origin: CTOX
// License: Apache-2.0

use anyhow::Context;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

const BUSINESS_STACK_SKILL: &str = "skills/system/product_engineering/business-stack/SKILL.md";
const BUSINESS_STACK_INSTALLER: &str =
    "skills/system/product_engineering/business-stack/scripts/install_business_stack.py";
const BUSINESS_STACK_TEMPLATE: &str = "templates/business-basic";

pub fn handle_business_os_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        None | Some("status") => {
            println!("{}", business_os_status_text(root));
            Ok(())
        }
        Some("install") => install_business_os(root, &args[1..]),
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
    "usage:\n  ctox business-os status\n  ctox business-os install --target <empty-dir> [--init-git] [--dry-run] [--no-copy-env]"
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
