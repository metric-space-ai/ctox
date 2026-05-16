// Origin: CTOX
// License: Apache-2.0
//
// `ctox knowledge search --query "<text>"` — union search across all four
// durable-knowledge forms in CTOX.
//
// The four forms searched here:
//   1. Skill bundles (`ctox_skill_bundles`) — system, user, and pack skills.
//      Source of truth: the in-binary `skills/system/` tree, plus user/pack
//      additions imported by `ctox skills user create` and `ctox skills packs
//      install`. Read via `crate::skill_store::list_skill_bundles`.
//   2. Procedural main skills (`knowledge_main_skills`) — the heads of
//      skill + skillbook + runbook + runbook-item bundles imported via
//      `ctox knowledge skill import-bundle` (or its alias `ctox ticket
//      source-skill-import-bundle`). Currently empty on hosts where no
//      bundle import has run.
//   3. Record-shape data tables (`knowledge_data_tables`) — record-shape
//      knowledge with Parquet content. Catalog rows carry domain, table_key,
//      title, description, tags, row count.
//   4. Ticket-scoped facts (`ticket_knowledge_entries`) — single-fact and
//      ticket-scoped notes.
//
// The match is intentionally simple: case-insensitive substring (`LIKE`) on
// the most discoverable text columns of each form. Per-form caps keep the
// output bounded; the caller can refine by jumping into the form's own CLI
// (`ctox skills show <name>`, `ctox knowledge data describe`, etc.). This
// is a discovery surface, not a ranking engine.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use std::path::Path;

use crate::paths;
use crate::persistence;
use crate::skill_store;

const DEFAULT_PER_FORM_LIMIT: usize = 10;
const HARD_PER_FORM_CEILING: usize = 100;

pub(super) fn handle_command(root: &Path, args: &[String]) -> Result<()> {
    let query_raw = required(args, "--query", USAGE)?.trim();
    if query_raw.is_empty() {
        bail!("--query must not be empty");
    }
    let limit = find_flag(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(HARD_PER_FORM_CEILING))
        .unwrap_or(DEFAULT_PER_FORM_LIMIT);
    let form_filter = find_flag(args, "--form").map(|raw| raw.trim().to_ascii_lowercase());

    let needle = format!("%{}%", query_raw.to_lowercase());

    let mut payload = Map::new();
    payload.insert("ok".to_string(), Value::Bool(true));
    payload.insert("query".to_string(), Value::String(query_raw.to_string()));
    payload.insert("limit_per_form".to_string(), Value::from(limit as u64));

    let conn = open_db(root)?;

    let skills = search_skill_bundles(root, query_raw, limit, form_filter.as_deref())?;
    let procedural = search_procedural_main_skills(&conn, &needle, limit, form_filter.as_deref())?;
    let data_tables = search_data_tables(&conn, &needle, limit, form_filter.as_deref())?;
    let facts = search_ticket_facts(&conn, &needle, limit, form_filter.as_deref())?;

    let total = skills.len() + procedural.len() + data_tables.len() + facts.len();
    payload.insert("total_hits".to_string(), Value::from(total as u64));
    payload.insert(
        "results".to_string(),
        json!({
            "skills": skills,
            "procedural_main_skills": procedural,
            "data_tables": data_tables,
            "ticket_facts": facts,
        }),
    );

    super::print_json(&Value::Object(payload))
}

fn search_skill_bundles(
    root: &Path,
    query: &str,
    limit: usize,
    form_filter: Option<&str>,
) -> Result<Vec<Value>> {
    if let Some(form) = form_filter {
        if form != "skills" && form != "skill_bundles" {
            return Ok(Vec::new());
        }
    }
    let bundles = skill_store::list_skill_bundles(root)
        .with_context(|| "failed to list skill bundles for union search")?;
    let needle = query.to_ascii_lowercase();
    let mut hits: Vec<Value> = Vec::new();
    for bundle in bundles {
        let name_lc = bundle.skill_name.to_ascii_lowercase();
        let desc_lc = bundle.description.to_ascii_lowercase();
        let cluster_lc = bundle.cluster.to_ascii_lowercase();
        if !name_lc.contains(&needle) && !desc_lc.contains(&needle) && !cluster_lc.contains(&needle)
        {
            continue;
        }
        hits.push(json!({
            "form": "skill",
            "skill_id": bundle.skill_id,
            "skill_name": bundle.skill_name,
            "class": bundle.class,
            "state": bundle.state,
            "cluster": bundle.cluster,
            "description": bundle.description,
            "source_path": bundle.source_path,
        }));
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

fn search_procedural_main_skills(
    conn: &Connection,
    needle_like: &str,
    limit: usize,
    form_filter: Option<&str>,
) -> Result<Vec<Value>> {
    if let Some(form) = form_filter {
        if form != "procedural" && form != "main_skills" && form != "main-skills" {
            return Ok(Vec::new());
        }
    }
    if !table_exists(conn, "knowledge_main_skills")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT main_skill_id, title, primary_channel, entry_action, updated_at
         FROM knowledge_main_skills
         WHERE LOWER(title) LIKE ?1
            OR LOWER(primary_channel) LIKE ?1
            OR LOWER(entry_action) LIKE ?1
            OR LOWER(main_skill_id) LIKE ?1
         ORDER BY updated_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![needle_like, limit as i64], |row| {
        let main_skill_id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let primary_channel: String = row.get(2)?;
        let entry_action: String = row.get(3)?;
        let updated_at: String = row.get(4)?;
        Ok(json!({
            "form": "procedural_main_skill",
            "main_skill_id": main_skill_id,
            "title": title,
            "primary_channel": primary_channel,
            "entry_action": entry_action,
            "updated_at": updated_at,
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn search_data_tables(
    conn: &Connection,
    needle_like: &str,
    limit: usize,
    form_filter: Option<&str>,
) -> Result<Vec<Value>> {
    if let Some(form) = form_filter {
        if form != "data" && form != "data_tables" && form != "data-tables" {
            return Ok(Vec::new());
        }
    }
    if !table_exists(conn, "knowledge_data_tables")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT domain, table_key, source_system, title, description, row_count, updated_at
         FROM knowledge_data_tables
         WHERE archived_at IS NULL
           AND ( LOWER(domain) LIKE ?1
              OR LOWER(table_key) LIKE ?1
              OR LOWER(title) LIKE ?1
              OR LOWER(description) LIKE ?1
              OR LOWER(source_system) LIKE ?1 )
         ORDER BY updated_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![needle_like, limit as i64], |row| {
        let domain: String = row.get(0)?;
        let table_key: String = row.get(1)?;
        let source_system: String = row.get(2)?;
        let title: String = row.get(3)?;
        let description: String = row.get(4)?;
        let row_count: i64 = row.get(5)?;
        let updated_at: String = row.get(6)?;
        Ok(json!({
            "form": "data_table",
            "domain": domain,
            "table_key": table_key,
            "source_system": source_system,
            "title": title,
            "description": description,
            "row_count": row_count,
            "updated_at": updated_at,
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn search_ticket_facts(
    conn: &Connection,
    needle_like: &str,
    limit: usize,
    form_filter: Option<&str>,
) -> Result<Vec<Value>> {
    if let Some(form) = form_filter {
        if form != "facts" && form != "ticket_facts" && form != "ticket-facts" {
            return Ok(Vec::new());
        }
    }
    if !table_exists(conn, "ticket_knowledge_entries")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT entry_id, source_system, domain, knowledge_key, title, summary, status, updated_at
         FROM ticket_knowledge_entries
         WHERE LOWER(title) LIKE ?1
            OR LOWER(summary) LIKE ?1
            OR LOWER(domain) LIKE ?1
            OR LOWER(knowledge_key) LIKE ?1
         ORDER BY updated_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![needle_like, limit as i64], |row| {
        let entry_id: String = row.get(0)?;
        let source_system: String = row.get(1)?;
        let domain: String = row.get(2)?;
        let knowledge_key: String = row.get(3)?;
        let title: String = row.get(4)?;
        let summary: String = row.get(5)?;
        let status: String = row.get(6)?;
        let updated_at: String = row.get(7)?;
        Ok(json!({
            "form": "ticket_fact",
            "entry_id": entry_id,
            "source_system": source_system,
            "domain": domain,
            "knowledge_key": knowledge_key,
            "title": title,
            "summary": summary,
            "status": status,
            "updated_at": updated_at,
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
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

fn open_db(root: &Path) -> Result<Connection> {
    let path = paths::core_db(root);
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open core db at {}", path.display()))?;
    conn.busy_timeout(persistence::sqlite_busy_timeout_duration())
        .context("failed to set sqlite busy_timeout")?;
    Ok(conn)
}

fn required<'a>(args: &'a [String], flag: &str, usage: &'static str) -> Result<&'a str> {
    find_flag(args, flag).with_context(|| format!("missing {flag}. usage: {usage}"))
}

fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(String::as_str)
}

const USAGE: &str =
    "ctox knowledge search --query <text> [--limit <n>] [--form <skills|procedural|data|facts>]";
