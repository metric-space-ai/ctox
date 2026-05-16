// Origin: CTOX
// License: Apache-2.0
//
// `ctox knowledge facts ...` — single-fact / ticket-scoped notes sub-form.
//
// Facts are durable but narrow: one piece of information attached to a
// specific case or ticket, without warranting a full skill, runbook, or
// data table. Backed by the `ticket_knowledge_entries` table; this module
// is a thin delegation surface around the existing `ctox ticket
// knowledge-*` handlers in `src/mission/tickets.rs`.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use serde_json::json;
use std::path::Path;

use crate::mission::tickets;

const DEFAULT_LIST_LIMIT: usize = 50;

pub(super) fn handle_command(root: &Path, args: &[String]) -> Result<()> {
    let verb = args.first().map(String::as_str);
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match verb {
        None | Some("--help") | Some("-h") | Some("help") => super::print_json(&help_payload()),
        Some("bootstrap") => bootstrap(root, rest),
        Some("list") => list(root, rest),
        Some("show") => show(root, rest),
        Some("load") => load(root, rest),
        Some(unknown) => {
            super::print_json(&json!({
                "ok": false,
                "form": "facts",
                "error": format!("unknown subcommand: {unknown}"),
                "available_verbs": available_verbs(),
            }))?;
            bail!("unknown knowledge facts subcommand: {unknown}");
        }
    }
}

fn help_payload() -> serde_json::Value {
    json!({
        "ok": true,
        "form": "facts",
        "scope": "single-fact / ticket-scoped durable notes (table `ticket_knowledge_entries`)",
        "available_verbs": available_verbs(),
        "note": "Delegates to the same SQLite tables as `ctox ticket knowledge-*`. The two entry points share state.",
    })
}

fn available_verbs() -> serde_json::Value {
    json!([
        {"verb": "bootstrap", "args": "--system <name>"},
        {"verb": "list",      "args": "[--system <name>] [--domain <name>] [--status <value>] [--limit <n>]"},
        {"verb": "show",      "args": "--system <name> --domain <name> --key <value>"},
        {"verb": "load",      "args": "--ticket-key <key> [--domains <csv>]"},
    ])
}

fn bootstrap(root: &Path, args: &[String]) -> Result<()> {
    let system = required(args, "--system", USAGE_BOOTSTRAP)?;
    let entries = tickets::refresh_observed_ticket_knowledge(root, system)?;
    super::print_json(&json!({
        "ok": true,
        "system": system,
        "count": entries.len(),
        "entries": entries,
    }))
}

fn list(root: &Path, args: &[String]) -> Result<()> {
    let system = find_flag(args, "--system");
    let domain = find_flag(args, "--domain");
    let status = find_flag(args, "--status");
    let limit = find_flag(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_LIST_LIMIT);
    let entries = tickets::list_ticket_knowledge_entries(root, system, domain, status, limit)?;
    super::print_json(&json!({
        "ok": true,
        "count": entries.len(),
        "entries": entries,
    }))
}

fn show(root: &Path, args: &[String]) -> Result<()> {
    let system = required(args, "--system", USAGE_SHOW)?;
    let domain = required(args, "--domain", USAGE_SHOW)?;
    let key = required(args, "--key", USAGE_SHOW)?;
    let entry = tickets::load_ticket_knowledge_entry(root, system, domain, key)?
        .context("ticket knowledge entry not found")?;
    super::print_json(&json!({"ok": true, "entry": entry}))
}

fn load(root: &Path, args: &[String]) -> Result<()> {
    let ticket_key = required(args, "--ticket-key", USAGE_LOAD)?;
    let domains = find_flag(args, "--domains").map(|raw| {
        raw.split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    });
    let load = tickets::create_ticket_knowledge_load(root, ticket_key, domains.as_deref())?;
    super::print_json(&json!({"ok": true, "knowledge_load": load}))
}

fn required<'a>(args: &'a [String], flag: &str, usage: &'static str) -> Result<&'a str> {
    find_flag(args, flag).with_context(|| format!("missing {flag}. usage: {usage}"))
}

fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(String::as_str)
}

const USAGE_BOOTSTRAP: &str = "ctox knowledge facts bootstrap --system <name>";
const USAGE_SHOW: &str = "ctox knowledge facts show --system <name> --domain <name> --key <value>";
const USAGE_LOAD: &str = "ctox knowledge facts load --ticket-key <key> [--domains <csv>]";
