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
        // Incremental procedural-knowledge writers (Tier 4).
        Some("new") => new_main_skill(root, rest),
        Some("add-skillbook") => add_skillbook(root, rest),
        Some("add-runbook") => add_runbook(root, rest),
        Some("add-item") => add_item(root, rest),
        Some("refresh-item-embedding") => refresh_item_embedding(root, rest),
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
        // discovery + read
        {"verb": "list",          "args": "[--system <name>]"},
        {"verb": "show",          "args": "--system <name>"},
        {"verb": "query",         "args": "--system <name> --query <text> [--top-k <n>]"},
        // ticket-binding + bundle import (whole bundle on disk)
        {"verb": "set",           "args": "--system <name> --skill <name> [--archetype <value>] [--status <active|inactive>] [--origin <value>] [--artifact-path <path>] [--notes <text>]"},
        {"verb": "import-bundle", "args": "--system <name> --bundle-dir <path> [--embedding-model <model>] [--skip-embeddings]"},
        // ticket-resolution surfaces
        {"verb": "resolve",       "args": "(--ticket-key <key> | --case-id <id>) [--top-k <n>]"},
        {"verb": "compose-reply", "args": "(--ticket-key <key> | --case-id <id>) [--send-policy <suggestion|draft|send>] [--subject <text>] [--body-only]"},
        {"verb": "review-note",   "args": "(--ticket-key <key> | --case-id <id>) --body <text> [--top-k <n>]"},
        // incremental writes (grow procedural knowledge turn-by-turn)
        {"verb": "new",                     "args": "--id <main_skill_id> --title <text> --primary-channel <text> --entry-action <text> [--resolver-contract <json>] [--execution-contract <json>] [--resolve-flow <step,...>] [--writeback-flow <step,...>] [--linked-skillbooks <id,...>] [--linked-runbooks <id,...>]"},
        {"verb": "add-skillbook",           "args": "--id <skillbook_id> --title <text> --version <text> --mission <text> [--runtime-policy <text>] [--answer-contract <text>] [--non-negotiable-rules <csv>] [--workflow-backbone <csv>] [--routing-taxonomy <csv>] [--linked-runbooks <id,...>]"},
        {"verb": "add-runbook",             "args": "--id <runbook_id> --skillbook <skillbook_id> --title <text> --version <text> --problem-domain <text> [--status <active|draft|inactive>] [--item-labels <csv>]"},
        {"verb": "add-item",                "args": "--id <item_id> --runbook <runbook_id> --skillbook <skillbook_id> --label <REG-XX> --title <text> --problem-class <text> --chunk-text <text> [--version <text>] [--status <active|draft|inactive>] [--embedding-model <model>] [--skip-embedding]"},
        {"verb": "refresh-item-embedding",  "args": "--id <item_id> [--embedding-model <model>]"},
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

// ----- Incremental procedural-knowledge writers (Tier 4) ------------------
//
// These verbs create a new main-skill / skillbook / runbook / runbook-item
// row directly, without going through the disk-resident bundle JSON files
// that `import-bundle` requires. They are the right entry point when the
// agent learns one new procedural fact in the middle of a turn and wants to
// make it durable immediately. `add-item` also (by default) triggers a
// single-vector embedding refresh through the standard auxiliary backend so
// the new chunk is retrievable via `ctox knowledge skill query` on the next
// call.

fn new_main_skill(root: &Path, args: &[String]) -> Result<()> {
    let id = required(args, "--id", USAGE_NEW)?;
    let title = required(args, "--title", USAGE_NEW)?;
    let primary_channel = required(args, "--primary-channel", USAGE_NEW)?;
    let entry_action = required(args, "--entry-action", USAGE_NEW)?;
    let resolver_contract = parse_optional_json(args, "--resolver-contract")?;
    let execution_contract = parse_optional_json(args, "--execution-contract")?;
    let resolve_flow = parse_csv(args, "--resolve-flow");
    let writeback_flow = parse_csv(args, "--writeback-flow");
    let linked_skillbooks = parse_csv(args, "--linked-skillbooks");
    let linked_runbooks = parse_csv(args, "--linked-runbooks");
    let record = tickets::create_or_update_main_skill(
        root,
        id,
        title,
        primary_channel,
        entry_action,
        resolver_contract,
        execution_contract,
        resolve_flow,
        writeback_flow,
        linked_skillbooks,
        linked_runbooks,
    )?;
    super::print_json(&json!({"ok": true, "main_skill": record}))
}

fn add_skillbook(root: &Path, args: &[String]) -> Result<()> {
    let id = required(args, "--id", USAGE_ADD_SKILLBOOK)?;
    let title = required(args, "--title", USAGE_ADD_SKILLBOOK)?;
    let version = required(args, "--version", USAGE_ADD_SKILLBOOK)?;
    let mission = required(args, "--mission", USAGE_ADD_SKILLBOOK)?;
    let runtime_policy = find_flag(args, "--runtime-policy").unwrap_or("");
    let answer_contract = find_flag(args, "--answer-contract").unwrap_or("");
    let non_negotiable_rules = parse_csv(args, "--non-negotiable-rules");
    let workflow_backbone = parse_csv(args, "--workflow-backbone");
    let routing_taxonomy = parse_csv(args, "--routing-taxonomy");
    let linked_runbooks = parse_csv(args, "--linked-runbooks");
    let record = tickets::create_or_update_skillbook(
        root,
        id,
        title,
        version,
        mission,
        runtime_policy,
        answer_contract,
        non_negotiable_rules,
        workflow_backbone,
        routing_taxonomy,
        linked_runbooks,
    )?;
    super::print_json(&json!({"ok": true, "skillbook": record}))
}

fn add_runbook(root: &Path, args: &[String]) -> Result<()> {
    let id = required(args, "--id", USAGE_ADD_RUNBOOK)?;
    let skillbook = required(args, "--skillbook", USAGE_ADD_RUNBOOK)?;
    let title = required(args, "--title", USAGE_ADD_RUNBOOK)?;
    let version = required(args, "--version", USAGE_ADD_RUNBOOK)?;
    let problem_domain = required(args, "--problem-domain", USAGE_ADD_RUNBOOK)?;
    let status = find_flag(args, "--status").unwrap_or("active");
    let item_labels = parse_csv(args, "--item-labels");
    let record = tickets::create_or_update_runbook(
        root,
        id,
        skillbook,
        title,
        version,
        status,
        problem_domain,
        item_labels,
    )?;
    super::print_json(&json!({"ok": true, "runbook": record}))
}

fn add_item(root: &Path, args: &[String]) -> Result<()> {
    let id = required(args, "--id", USAGE_ADD_ITEM)?;
    let runbook = required(args, "--runbook", USAGE_ADD_ITEM)?;
    let skillbook = required(args, "--skillbook", USAGE_ADD_ITEM)?;
    let label = required(args, "--label", USAGE_ADD_ITEM)?;
    let title = required(args, "--title", USAGE_ADD_ITEM)?;
    let problem_class = required(args, "--problem-class", USAGE_ADD_ITEM)?;
    let chunk_text = required(args, "--chunk-text", USAGE_ADD_ITEM)?;
    let version = find_flag(args, "--version").unwrap_or("v1");
    let status = find_flag(args, "--status").unwrap_or("active");
    let embedding_model = find_flag(args, "--embedding-model");
    let skip_embedding = flag_present(args, "--skip-embedding");
    let payload = tickets::add_or_update_runbook_item(
        root,
        id,
        runbook,
        skillbook,
        label,
        title,
        problem_class,
        chunk_text,
        version,
        status,
        embedding_model,
        skip_embedding,
    )?;
    super::print_json(&json!({"ok": true, "added": payload}))
}

fn refresh_item_embedding(root: &Path, args: &[String]) -> Result<()> {
    let id = required(args, "--id", USAGE_REFRESH_EMBEDDING)?;
    let embedding_model = find_flag(args, "--embedding-model");
    let payload = tickets::refresh_runbook_item_embedding(root, id, embedding_model)?;
    super::print_json(&payload)
}

fn parse_csv(args: &[String], flag: &str) -> Vec<String> {
    find_flag(args, flag)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_optional_json(args: &[String], flag: &str) -> Result<Option<serde_json::Value>> {
    match find_flag(args, flag) {
        Some(raw) => Ok(Some(
            serde_json::from_str(raw)
                .with_context(|| format!("{flag} is not valid JSON: {raw}"))?,
        )),
        None => Ok(None),
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
const USAGE_NEW: &str = "ctox knowledge skill new --id <main_skill_id> --title <text> --primary-channel <text> --entry-action <text> [--resolver-contract <json>] [--execution-contract <json>] [--resolve-flow <step,...>] [--writeback-flow <step,...>] [--linked-skillbooks <id,...>] [--linked-runbooks <id,...>]";
const USAGE_ADD_SKILLBOOK: &str = "ctox knowledge skill add-skillbook --id <skillbook_id> --title <text> --version <text> --mission <text> [--runtime-policy <text>] [--answer-contract <text>] [--non-negotiable-rules <csv>] [--workflow-backbone <csv>] [--routing-taxonomy <csv>] [--linked-runbooks <id,...>]";
const USAGE_ADD_RUNBOOK: &str = "ctox knowledge skill add-runbook --id <runbook_id> --skillbook <skillbook_id> --title <text> --version <text> --problem-domain <text> [--status <active|draft|inactive>] [--item-labels <csv>]";
const USAGE_ADD_ITEM: &str = "ctox knowledge skill add-item --id <item_id> --runbook <runbook_id> --skillbook <skillbook_id> --label <REG-XX> --title <text> --problem-class <text> --chunk-text <text> [--version <text>] [--status <active|draft|inactive>] [--embedding-model <model>] [--skip-embedding]";
const USAGE_REFRESH_EMBEDDING: &str =
    "ctox knowledge skill refresh-item-embedding --id <item_id> [--embedding-model <model>]";
