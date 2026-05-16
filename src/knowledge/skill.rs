// Origin: CTOX
// License: Apache-2.0
//
// `ctox knowledge skill ...` — procedural-knowledge sub-form.
//
// This module is a thin delegation surface in front of the durable
// procedural-knowledge store that lives in the ticket subsystem (main-skill +
// skillbooks + runbooks + labeled runbook items, plus their embeddings).
// The handlers themselves live in `src/mission/tickets.rs`; the `ctox ticket
// source-skill-*` CLI continues to work for backward compatibility, and this
// module just gives the same operations the canonical name they should have
// had all along — they belong to the *knowledge* namespace, not the *ticket*
// namespace, because the ticket coupling is only an artifact of the original
// import path. The data shape is identical regardless of which entry point
// the caller used.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use serde_json::json;
use std::path::Path;

use crate::mission::tickets;

pub(super) fn handle_command(root: &Path, args: &[String]) -> Result<()> {
    let verb = args.first().map(String::as_str);
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match verb {
        None | Some("--help") | Some("-h") | Some("help") => super::print_json(&help_payload()),
        Some("list") => list(root, rest),
        Some("show") => show(root, rest),
        Some("query") => query(root, rest),
        Some("set") => set(root, rest),
        Some("import-bundle") => import_bundle(root, rest),
        Some("resolve") => resolve(root, rest),
        Some("compose-reply") => compose_reply(root, rest),
        Some("review-note") => review_note(root, rest),
        Some(unknown) => {
            super::print_json(&json!({
                "ok": false,
                "form": "skill",
                "error": format!("unknown subcommand: {unknown}"),
                "available_verbs": available_verbs(),
            }))?;
            bail!("unknown knowledge skill subcommand: {unknown}");
        }
    }
}

fn help_payload() -> serde_json::Value {
    json!({
        "ok": true,
        "form": "skill",
        "scope": "procedural durable knowledge — main-skill + skillbooks + runbooks + labeled runbook items, with embeddings",
        "available_verbs": available_verbs(),
        "note": "Delegates to the same SQLite tables as `ctox ticket source-skill-*`. The two entry points share state.",
    })
}

fn available_verbs() -> serde_json::Value {
    json!([
        {"verb": "list",          "args": "[--system <name>]"},
        {"verb": "show",          "args": "--system <name>"},
        {"verb": "query",         "args": "--system <name> --query <text> [--top-k <n>]"},
        {"verb": "set",           "args": "--system <name> --skill <name> [--archetype <value>] [--status <active|inactive>] [--origin <value>] [--artifact-path <path>] [--notes <text>]"},
        {"verb": "import-bundle", "args": "--system <name> --bundle-dir <path> [--embedding-model <model>] [--skip-embeddings]"},
        {"verb": "resolve",       "args": "(--ticket-key <key> | --case-id <id>) [--top-k <n>]"},
        {"verb": "compose-reply", "args": "(--ticket-key <key> | --case-id <id>) [--send-policy <suggestion|draft|send>] [--subject <text>] [--body-only]"},
        {"verb": "review-note",   "args": "(--ticket-key <key> | --case-id <id>) --body <text> [--top-k <n>]"},
    ])
}

fn list(root: &Path, args: &[String]) -> Result<()> {
    let system = find_flag(args, "--system");
    let bindings = tickets::list_ticket_source_skill_bindings(root, system)?;
    super::print_json(&json!({
        "ok": true,
        "count": bindings.len(),
        "source_skills": bindings,
    }))
}

fn show(root: &Path, args: &[String]) -> Result<()> {
    let system = required(args, "--system", USAGE_SHOW)?;
    let view = tickets::show_ticket_source_skill(root, system)?;
    super::print_json(&json!({"ok": true, "source_skill": view}))
}

fn query(root: &Path, args: &[String]) -> Result<()> {
    let system = required(args, "--system", USAGE_QUERY)?;
    let query = required(args, "--query", USAGE_QUERY)?;
    let top_k = find_flag(args, "--top-k")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(3);
    let result = tickets::query_ticket_source_skill(root, system, query, top_k)?;
    super::print_json(&result)
}

fn set(root: &Path, args: &[String]) -> Result<()> {
    let system = required(args, "--system", USAGE_SET)?;
    let skill = required(args, "--skill", USAGE_SET)?;
    let archetype = find_flag(args, "--archetype").unwrap_or("operating-model");
    let status = find_flag(args, "--status").unwrap_or("active");
    let origin = find_flag(args, "--origin").unwrap_or("ticket-onboarding");
    let artifact_path = find_flag(args, "--artifact-path");
    let notes = find_flag(args, "--notes");
    let binding = tickets::put_ticket_source_skill_binding(
        root,
        system,
        skill,
        archetype,
        status,
        origin,
        artifact_path,
        notes,
    )?;
    super::print_json(&json!({"ok": true, "source_skill": binding}))
}

fn import_bundle(root: &Path, args: &[String]) -> Result<()> {
    let system = required(args, "--system", USAGE_IMPORT_BUNDLE)?;
    let bundle_dir = required(args, "--bundle-dir", USAGE_IMPORT_BUNDLE)?;
    let result = tickets::import_ticket_source_skill_bundle(
        root,
        system,
        bundle_dir,
        find_flag(args, "--embedding-model"),
        flag_present(args, "--skip-embeddings"),
    )?;
    super::print_json(&result)
}

fn resolve(root: &Path, args: &[String]) -> Result<()> {
    let top_k = find_flag(args, "--top-k")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(3);
    let result = tickets::resolve_ticket_source_skill_for_target(
        root,
        find_flag(args, "--ticket-key"),
        find_flag(args, "--case-id"),
        top_k,
    )?;
    super::print_json(&result)
}

fn compose_reply(root: &Path, args: &[String]) -> Result<()> {
    let result = tickets::compose_ticket_source_skill_reply(
        root,
        find_flag(args, "--ticket-key"),
        find_flag(args, "--case-id"),
        find_flag(args, "--send-policy").unwrap_or("suggestion"),
        find_flag(args, "--subject"),
        flag_present(args, "--body-only"),
    )?;
    super::print_json(&result)
}

fn review_note(root: &Path, args: &[String]) -> Result<()> {
    let body = required(args, "--body", USAGE_REVIEW_NOTE)?;
    let top_k = find_flag(args, "--top-k")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(1);
    if let Some(ticket_key) = find_flag(args, "--ticket-key") {
        let review = tickets::review_ticket_note_with_source_skill(root, ticket_key, body, top_k)?;
        super::print_json(&json!({"ok": true, "review": review}))
    } else if let Some(_case_id) = find_flag(args, "--case-id") {
        bail!(
            "ctox knowledge skill review-note: --case-id support requires the case-id → ticket-key resolution from the ticket dispatcher; use --ticket-key directly or fall back to `ctox ticket source-skill-review-note --case-id ...`"
        );
    } else {
        bail!("{}", USAGE_REVIEW_NOTE);
    }
}

fn required<'a>(args: &'a [String], flag: &str, usage: &'static str) -> Result<&'a str> {
    find_flag(args, flag).with_context(|| format!("missing {flag}. usage: {usage}"))
}

fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(String::as_str)
}

fn flag_present(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

const USAGE_SHOW: &str = "ctox knowledge skill show --system <name>";
const USAGE_QUERY: &str = "ctox knowledge skill query --system <name> --query <text> [--top-k <n>]";
const USAGE_SET: &str = "ctox knowledge skill set --system <name> --skill <name> [--archetype <value>] [--status <active|inactive>] [--origin <value>] [--artifact-path <path>] [--notes <text>]";
const USAGE_IMPORT_BUNDLE: &str = "ctox knowledge skill import-bundle --system <name> --bundle-dir <path> [--embedding-model <model>] [--skip-embeddings]";
const USAGE_REVIEW_NOTE: &str =
    "ctox knowledge skill review-note --ticket-key <key> --body <text> [--top-k <n>]";
