// Origin: CTOX
// License: Apache-2.0
//
// `ctox knowledge` namespace — single entry point for the four durable
// knowledge forms CTOX carries across turns. Each form has its own catalog
// in SQLite and its own curator skill; this module exposes them under a
// consistent CLI surface so an agent can discover, query, and (where the
// form supports it) write durable knowledge without having to know which
// historical subsystem each form's tables happen to live in.
//
// Forms:
//   - `data` — record-shape knowledge (tables in `knowledge_data_tables` +
//     Parquet content). Full lifecycle + operational verbs in `data` /
//     `ops`. The original — and only fully self-contained — form.
//   - `skill` — procedural knowledge: main-skill + skillbooks + runbooks +
//     labeled runbook items, with embeddings. Backed by
//     `knowledge_main_skills` / `knowledge_skillbooks` / `knowledge_runbooks`
//     / `knowledge_runbook_items` / `knowledge_embeddings`. Delegates to
//     `mission::tickets` handlers (the same ones reachable as
//     `ctox ticket source-skill-*` for backward compatibility).
//   - `facts` — ticket-scoped single-fact entries
//     (`ticket_knowledge_entries`). Delegates to the same `mission::tickets`
//     handlers reachable as `ctox ticket knowledge-*`.
//   - `search` — union discovery across all four forms plus skill bundles
//     (`ctox_skill_bundles`). Use this when you ask "does CTOX already know
//     anything about <topic>?" before opening a new knowledge entry.
//
// All write-side operations (`create`, `append`, `update`, `import-bundle`,
// `bootstrap`, …) must run in the **daemon process** so SQLite writes are
// not lost when the CLI is invoked from a sandboxed agent subshell (macOS
// Seatbelt isolates writes from sandboxed children). The CLI entry point in
// `main.rs` therefore routes the entire `ctox knowledge …` family through
// the service IPC channel when the daemon is running, and the daemon
// dispatches via `dispatch_capturing()`. A direct-dispatch fallback
// (`handle_knowledge_command`) is preserved for the no-daemon path so the
// CLI remains usable for offline debugging.

mod cross_refs;
mod data;
mod facts;
mod ops;
mod parquet_io;
mod search;
mod skill;

use anyhow::Context;
use anyhow::Result;
use serde_json::json;
use serde_json::Value;
use std::cell::RefCell;
use std::io::Write;
use std::path::Path;

thread_local! {
    /// When `Some`, `print_json` writes the serialised JSON into this buffer
    /// instead of stdout. Used by `dispatch_capturing` so the daemon's IPC
    /// handler can collect the handler's output and return it as an IPC
    /// response. Thread-local because each knowledge dispatch is synchronous
    /// inside a single task on the daemon thread pool — no cross-thread
    /// sharing of the sink occurs.
    static CAPTURE: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

pub fn handle_knowledge_command(root: &Path, args: &[String]) -> Result<()> {
    let form = args.first().map(String::as_str);
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match form {
        None | Some("--help") | Some("-h") | Some("help") => print_json(&top_level_help()),
        Some("data") => data::handle_data_command(root, rest),
        Some("skill") | Some("skills") => skill::handle_command(root, rest),
        Some("facts") | Some("fact") => facts::handle_command(root, rest),
        Some("search") => search::handle_command(root, rest),
        // Cross-references are top-level verbs (not a sub-form) because they
        // link items across forms — the form-aware dispatchers above would be
        // the wrong scope.
        Some("link") => cross_refs::handle_command(root, Some("link"), rest),
        Some("unlink") => cross_refs::handle_command(root, Some("unlink"), rest),
        Some("references") => cross_refs::handle_command(root, Some("references"), rest),
        Some("kinds") => cross_refs::handle_command(root, Some("kinds"), rest),
        Some(unknown) => {
            print_json(&json!({
                "ok": false,
                "error": format!("unknown knowledge form: {unknown}"),
                "available_forms": available_forms(),
            }))?;
            anyhow::bail!("unknown knowledge form: {unknown}");
        }
    }
}

/// Run a knowledge command with `print_json` redirected to an internal
/// buffer; return the resulting JSON value. Designed for the daemon IPC
/// handler — keeps SQLite writes in the daemon process so they survive the
/// sandboxed CLI caller's process exit.
pub fn dispatch_capturing(root: &Path, args: &[String]) -> Result<Value> {
    CAPTURE.with(|cell| *cell.borrow_mut() = Some(Vec::new()));
    let result = handle_knowledge_command(root, args);
    let captured = CAPTURE.with(|cell| cell.borrow_mut().take().unwrap_or_default());

    if !captured.is_empty() {
        // Every handler emits exactly one pretty-JSON payload; even the
        // bail-with-error path emits a `{"ok": false, ...}` JSON before
        // returning Err. Prefer that captured payload over the raw error.
        let value: Value = serde_json::from_slice(&captured)
            .context("knowledge command output is not valid JSON")?;
        return Ok(value);
    }

    // Defensive: no JSON emitted. Propagate the result's error or
    // synthesise a placeholder so the IPC envelope still carries something.
    result?;
    Ok(json!({"ok": true, "note": "knowledge command emitted no payload"}))
}

fn top_level_help() -> Value {
    json!({
        "ok": true,
        "namespace": "knowledge",
        "forms": available_forms(),
        "discovery": "Use `ctox knowledge search --query \"<topic>\"` first to see what CTOX already knows on a topic across all four forms before opening a new entry.",
    })
}

fn available_forms() -> Value {
    json!({
        "data":   "record-shape knowledge tables (rows sharing a schema, Parquet-backed). CLI: ctox knowledge data <verb>",
        "skill":  "procedural knowledge — main-skill + skillbooks + runbooks + labeled runbook items. CLI: ctox knowledge skill <verb>",
        "facts":  "ticket-scoped single-fact entries. CLI: ctox knowledge facts <verb>",
        "search": "union discovery across data tables, procedural skills, ticket facts, and skill bundles. CLI: ctox knowledge search --query <text>",
        "link":      "create a structural cross-reference between two durable items. CLI: ctox knowledge link --from <kind>:<id> --to <kind>:<id> --relation <name> [--note <text>]",
        "unlink":    "remove a structural cross-reference. CLI: ctox knowledge unlink --from <kind>:<id> --to <kind>:<id> --relation <name>",
        "references":"list cross-references touching one item. CLI: ctox knowledge references --of <kind>:<id> [--direction <out|in|both>] [--relation <name>] [--limit <n>]",
        "kinds":     "list the canonical cross-reference kinds and relations. CLI: ctox knowledge kinds",
    })
}

pub(crate) fn print_json(value: &Value) -> Result<()> {
    let serialized = serde_json::to_string_pretty(value)?;
    CAPTURE.with(|cell| -> Result<()> {
        let mut sink = cell.borrow_mut();
        match sink.as_mut() {
            Some(buf) => writeln!(buf, "{serialized}").context("write to knowledge capture buffer"),
            None => {
                println!("{serialized}");
                Ok(())
            }
        }
    })
}
