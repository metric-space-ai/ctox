// Origin: CTOX
// License: Apache-2.0
//
// `ctox knowledge link / unlink / references` — structural cross-links
// between durable-knowledge items.
//
// The two CTOX knowledge axes (procedural skill+runbooks, record-shape data
// tables) reference each other in practice: a runbook item may say "for
// this step, consult `engineering/vendor_matrix`"; a data row may carry
// "this value was derived via procedure `runbook_item:REG-07`". This module
// stores those edges as a small, queryable graph in
// `knowledge_cross_references`. The edge is intentionally an additive
// annotation — the host tables on either side know nothing about the
// reference, so dropping the edges never breaks the underlying records.
//
// Reference encoding: `<kind>:<id>`. The id itself may contain colons for
// composite keys (`data_table:engineering/vendor_matrix`,
// `data_row:engineering/vendor_matrix#row-uuid`). The CLI accepts the
// whole `<kind>:<id>` token via `--from` / `--to` flags and splits on the
// first colon.
//
// Schema bootstrap lives here (idempotent) so the table is created on first
// touch without requiring `ctox knowledge data` or `ctox ticket init` to
// run first.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;

use crate::paths;
use crate::persistence;

const CROSS_REF_TABLE: &str = "knowledge_cross_references";

/// Canonical edge kinds. Edges may also use kinds outside this list —
/// nothing enforces it at the schema level — but staying inside the list
/// keeps cross-search results uniform. The names are deliberately the same
/// strings used by `ctox knowledge search`.
const CANONICAL_KINDS: &[&str] = &[
    "main_skill",     // knowledge_main_skills.main_skill_id
    "skillbook",      // knowledge_skillbooks.skillbook_id
    "runbook",        // knowledge_runbooks.runbook_id
    "runbook_item",   // knowledge_runbook_items.item_id
    "data_table",     // knowledge_data_tables: encoded as "<domain>/<table_key>"
    "data_row",       // a specific row inside a data table: "<domain>/<table_key>#<row-id>"
    "ticket_fact",    // ticket_knowledge_entries: "<source_system>:<domain>:<knowledge_key>"
    "skill_bundle",   // ctox_skill_bundles: skill_id or skill_name
];

/// Canonical relation labels. Same caveat — nothing enforced — but
/// agents that stick to these read more cleanly in cross-search output.
const CANONICAL_RELATIONS: &[&str] = &[
    "derived_via",  // "this row was produced by following that runbook item"
    "consult",      // "if you're executing this runbook step, consult that table"
    "produced_by",  // "this table came out of executing that skill"
    "cites",        // "this report cites that fact"
    "covers",       // "this skillbook covers that problem domain"
    "refines",      // "this runbook item refines/specializes that one"
    "supersedes",   // "this entry replaces an older version of that entry"
];

pub(super) fn handle_command(root: &Path, verb: Option<&str>, args: &[String]) -> Result<()> {
    match verb {
        Some("link") => link(root, args),
        Some("unlink") => unlink(root, args),
        Some("references") => references(root, args),
        // also expose the canonical vocab so an agent can ask what's allowed
        Some("kinds") => super::print_json(&json!({
            "ok": true,
            "canonical_kinds": CANONICAL_KINDS,
            "canonical_relations": CANONICAL_RELATIONS,
        })),
        _ => unreachable!("dispatcher in mod.rs gates which verbs reach this module"),
    }
}

fn link(root: &Path, args: &[String]) -> Result<()> {
    let from = required(args, "--from", USAGE_LINK)?;
    let to = required(args, "--to", USAGE_LINK)?;
    let relation = required(args, "--relation", USAGE_LINK)?;
    let note = find_flag(args, "--note").unwrap_or("");
    let (from_kind, from_id) = split_ref(from)?;
    let (to_kind, to_id) = split_ref(to)?;
    validate_kind(&from_kind, "from-kind")?;
    validate_kind(&to_kind, "to-kind")?;
    validate_relation(relation)?;

    let conn = open_db(root)?;
    ensure_schema(&conn)?;
    let now = now_rfc3339();
    let cross_ref_id = stable_edge_id(&from_kind, &from_id, &to_kind, &to_id, relation);

    conn.execute(
        &format!(
            "INSERT INTO {CROSS_REF_TABLE}
                (cross_ref_id, from_kind, from_id, to_kind, to_id, relation, note,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(from_kind, from_id, to_kind, to_id, relation) DO UPDATE SET
                note = excluded.note,
                updated_at = excluded.updated_at"
        ),
        params![
            cross_ref_id,
            from_kind,
            from_id,
            to_kind,
            to_id,
            relation,
            note,
            now,
        ],
    )?;

    super::print_json(&json!({
        "ok": true,
        "edge": {
            "cross_ref_id": cross_ref_id,
            "from": {"kind": from_kind, "id": from_id},
            "to": {"kind": to_kind, "id": to_id},
            "relation": relation,
            "note": note,
            "updated_at": now,
        }
    }))
}

fn unlink(root: &Path, args: &[String]) -> Result<()> {
    let from = required(args, "--from", USAGE_UNLINK)?;
    let to = required(args, "--to", USAGE_UNLINK)?;
    let relation = required(args, "--relation", USAGE_UNLINK)?;
    let (from_kind, from_id) = split_ref(from)?;
    let (to_kind, to_id) = split_ref(to)?;

    let conn = open_db(root)?;
    ensure_schema(&conn)?;
    let removed = conn.execute(
        &format!(
            "DELETE FROM {CROSS_REF_TABLE}
             WHERE from_kind = ?1 AND from_id = ?2
               AND to_kind = ?3 AND to_id = ?4
               AND relation = ?5"
        ),
        params![from_kind, from_id, to_kind, to_id, relation],
    )?;
    super::print_json(&json!({
        "ok": true,
        "removed": removed,
        "from": {"kind": from_kind, "id": from_id},
        "to": {"kind": to_kind, "id": to_id},
        "relation": relation,
    }))
}

fn references(root: &Path, args: &[String]) -> Result<()> {
    let of_arg = required(args, "--of", USAGE_REFERENCES)?;
    let (of_kind, of_id) = split_ref(of_arg)?;
    let direction = find_flag(args, "--direction")
        .map(|raw| raw.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "both".to_string());
    if !matches!(direction.as_str(), "out" | "in" | "both") {
        bail!("--direction must be one of: out | in | both");
    }
    let relation_filter = find_flag(args, "--relation");
    let limit = find_flag(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(50);

    let conn = open_db(root)?;
    ensure_schema(&conn)?;

    let mut outgoing: Vec<Value> = Vec::new();
    let mut incoming: Vec<Value> = Vec::new();

    if direction == "out" || direction == "both" {
        outgoing = query_edges(
            &conn,
            EdgeQuery::From { kind: &of_kind, id: &of_id },
            relation_filter,
            limit,
        )?;
    }
    if direction == "in" || direction == "both" {
        incoming = query_edges(
            &conn,
            EdgeQuery::To { kind: &of_kind, id: &of_id },
            relation_filter,
            limit,
        )?;
    }

    super::print_json(&json!({
        "ok": true,
        "of": {"kind": of_kind, "id": of_id},
        "direction": direction,
        "outgoing": outgoing,
        "incoming": incoming,
    }))
}

enum EdgeQuery<'a> {
    From { kind: &'a str, id: &'a str },
    To { kind: &'a str, id: &'a str },
}

fn query_edges(
    conn: &Connection,
    query: EdgeQuery<'_>,
    relation_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<Value>> {
    let (sql, kind, id) = match query {
        EdgeQuery::From { kind, id } => (
            format!(
                "SELECT cross_ref_id, from_kind, from_id, to_kind, to_id, relation, note,
                        created_at, updated_at
                 FROM {CROSS_REF_TABLE}
                 WHERE from_kind = ?1 AND from_id = ?2
                   AND (?3 IS NULL OR relation = ?3)
                 ORDER BY updated_at DESC
                 LIMIT ?4"
            ),
            kind,
            id,
        ),
        EdgeQuery::To { kind, id } => (
            format!(
                "SELECT cross_ref_id, from_kind, from_id, to_kind, to_id, relation, note,
                        created_at, updated_at
                 FROM {CROSS_REF_TABLE}
                 WHERE to_kind = ?1 AND to_id = ?2
                   AND (?3 IS NULL OR relation = ?3)
                 ORDER BY updated_at DESC
                 LIMIT ?4"
            ),
            kind,
            id,
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![kind, id, relation_filter, limit as i64],
        |row| {
            let cross_ref_id: String = row.get(0)?;
            let from_kind: String = row.get(1)?;
            let from_id: String = row.get(2)?;
            let to_kind: String = row.get(3)?;
            let to_id: String = row.get(4)?;
            let relation: String = row.get(5)?;
            let note: String = row.get(6)?;
            let created_at: String = row.get(7)?;
            let updated_at: String = row.get(8)?;
            let mut entry = Map::new();
            entry.insert("cross_ref_id".into(), Value::String(cross_ref_id));
            entry.insert(
                "from".into(),
                json!({"kind": from_kind, "id": from_id}),
            );
            entry.insert(
                "to".into(),
                json!({"kind": to_kind, "id": to_id}),
            );
            entry.insert("relation".into(), Value::String(relation));
            entry.insert("note".into(), Value::String(note));
            entry.insert("created_at".into(), Value::String(created_at));
            entry.insert("updated_at".into(), Value::String(updated_at));
            Ok(Value::Object(entry))
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

/// Reads cross-references for a given `<kind, id>` and folds them into the
/// shape returned to `ctox knowledge search --with-references`. Returns an
/// empty `Vec` if the table doesn't exist yet (no edges ever written on
/// this host).
pub(super) fn fetch_for_search(
    conn: &Connection,
    kind: &str,
    id: &str,
    limit: usize,
) -> Result<Vec<Value>> {
    if !table_exists(conn, CROSS_REF_TABLE)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        &format!(
            "SELECT to_kind, to_id, relation, note
             FROM {CROSS_REF_TABLE}
             WHERE from_kind = ?1 AND from_id = ?2
             ORDER BY updated_at DESC
             LIMIT ?3"
        ),
    )?;
    let rows = stmt.query_map(params![kind, id, limit as i64], |row| {
        let to_kind: String = row.get(0)?;
        let to_id: String = row.get(1)?;
        let relation: String = row.get(2)?;
        let note: String = row.get(3)?;
        Ok(json!({
            "to_kind": to_kind,
            "to_id": to_id,
            "relation": relation,
            "note": note,
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(super) fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS knowledge_cross_references (
            cross_ref_id TEXT PRIMARY KEY,
            from_kind    TEXT NOT NULL,
            from_id      TEXT NOT NULL,
            to_kind      TEXT NOT NULL,
            to_id        TEXT NOT NULL,
            relation     TEXT NOT NULL,
            note         TEXT NOT NULL DEFAULT '',
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL,
            UNIQUE(from_kind, from_id, to_kind, to_id, relation)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_cross_refs_from
            ON knowledge_cross_references(from_kind, from_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_cross_refs_to
            ON knowledge_cross_references(to_kind, to_id);
        "#,
    )
    .context("ensure knowledge_cross_references schema")?;
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name=?1",
            params![name],
            |row| row.get(0),
        )
        .context("query sqlite_master for table existence")?;
    Ok(count > 0)
}

fn split_ref(token: &str) -> Result<(String, String)> {
    let token = token.trim();
    let (kind, id) = token
        .split_once(':')
        .with_context(|| format!("expected <kind>:<id>, got `{token}`"))?;
    let kind = kind.trim();
    let id = id.trim();
    if kind.is_empty() || id.is_empty() {
        bail!("kind and id must both be non-empty in `{token}`");
    }
    Ok((kind.to_string(), id.to_string()))
}

fn validate_kind(kind: &str, label: &str) -> Result<()> {
    if kind.is_empty() || kind.len() > 64 {
        bail!("{label} must be 1..=64 chars");
    }
    if !kind
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        bail!("{label} may only contain [a-zA-Z0-9_]");
    }
    Ok(())
}

fn validate_relation(relation: &str) -> Result<()> {
    let relation = relation.trim();
    if relation.is_empty() || relation.len() > 64 {
        bail!("relation must be 1..=64 chars");
    }
    if !relation
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("relation may only contain [a-zA-Z0-9_-]");
    }
    Ok(())
}

fn stable_edge_id(
    from_kind: &str,
    from_id: &str,
    to_kind: &str,
    to_id: &str,
    relation: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(from_kind.as_bytes());
    hasher.update(b"|");
    hasher.update(from_id.as_bytes());
    hasher.update(b"|");
    hasher.update(to_kind.as_bytes());
    hasher.update(b"|");
    hasher.update(to_id.as_bytes());
    hasher.update(b"|");
    hasher.update(relation.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    format!("kxr-{}", &hex[..16])
}

fn open_db(root: &Path) -> Result<Connection> {
    let path = paths::core_db(root);
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open core db at {}", path.display()))?;
    conn.busy_timeout(persistence::sqlite_busy_timeout_duration())
        .context("failed to set sqlite busy_timeout")?;
    Ok(conn)
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn required<'a>(args: &'a [String], flag: &str, usage: &'static str) -> Result<&'a str> {
    find_flag(args, flag).with_context(|| format!("missing {flag}. usage: {usage}"))
}

fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(String::as_str)
}

const USAGE_LINK: &str =
    "ctox knowledge link --from <kind>:<id> --to <kind>:<id> --relation <name> [--note <text>]";
const USAGE_UNLINK: &str =
    "ctox knowledge unlink --from <kind>:<id> --to <kind>:<id> --relation <name>";
const USAGE_REFERENCES: &str = "ctox knowledge references --of <kind>:<id> [--direction <out|in|both>] [--relation <name>] [--limit <n>]";
