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

mod data;
mod ops;
mod parquet_io;

use anyhow::Result;
use serde_json::json;
use serde_json::Value;
use std::path::Path;

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

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
