// Origin: CTOX
// License: Apache-2.0
//
// `ctox knowledge` namespace. Record-shape knowledge is a first-class peer
// to skillbooks, runbooks, and ticket knowledge entries. The catalog table
// `knowledge_data_tables` is created in `mission::tickets::ensure_schema`
// and defensively re-created in `data::ensure_local_schema` so the
// knowledge module works without first opening the ticket subsystem.
//
// - Level 1: scaffold + CLI namespace.
// - Level 2: management verbs on the catalog (this module's `data` submodule).
// - Level 3: operational primitives for Python-script-driven content work.
//
// All write-side operations (`create`, `append`, `update`, …) must run in
// the **daemon process** so SQLite writes are not lost when the CLI is
// invoked from a sandboxed agent subshell (macOS Seatbelt isolates writes
// from sandboxed children). The CLI entry point in `main.rs` therefore
// routes `ctox knowledge data …` through the service IPC channel when the
// daemon is running, and the daemon dispatches via `dispatch_capturing()`.
// A direct-dispatch fallback (`handle_knowledge_command`) is preserved for
// the no-daemon path so the CLI remains usable for offline debugging.

mod data;
mod ops;
mod parquet_io;

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
        Some(unknown) => {
            print_json(&json!({
                "ok": false,
                "error": format!("unknown knowledge form: {unknown}"),
                "available_forms": ["data"],
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
        "forms": {
            "data": "record-shape knowledge tables — peer to skillbooks, runbooks, and ticket knowledge entries",
        },
        "note": "Level 2 surfaces catalog lifecycle verbs under `ctox knowledge data`. Level 3 (operational primitives for Python-script content work) lands later.",
    })
}

pub(crate) fn print_json(value: &Value) -> Result<()> {
    let serialized = serde_json::to_string_pretty(value)?;
    CAPTURE.with(|cell| -> Result<()> {
        let mut sink = cell.borrow_mut();
        match sink.as_mut() {
            Some(buf) => {
                writeln!(buf, "{serialized}").context("write to knowledge capture buffer")
            }
            None => {
                println!("{serialized}");
                Ok(())
            }
        }
    })
}
