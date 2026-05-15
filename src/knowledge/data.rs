// Origin: CTOX
// License: Apache-2.0
//
// Level 2 — management CLI for record-shape knowledge tables.
// Verbs operate on the catalog (`knowledge_data_tables`) and the
// associated Parquet files as opaque blobs. No content interpretation,
// no schema reading, no row access. That is Level 3.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;

const KNOWLEDGE_DATA_ROOT: &str = "runtime/knowledge/data";

pub fn handle_data_command(root: &Path, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str);
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match sub {
        None | Some("--help") | Some("-h") | Some("help") => print_json(&help_payload()),
        // Level 2 — catalog lifecycle
        Some("create") => create(root, rest),
        Some("list") => list(root, rest),
        Some("describe") => describe(root, rest),
        Some("clone") => clone(root, rest),
        Some("rename") => rename(root, rest),
        Some("archive") => archive(root, rest),
        Some("restore") => restore(root, rest),
        Some("delete") => delete(root, rest),
        Some("tag") => tag(root, rest),
        Some("untag") => untag(root, rest),
        // Level 3 — operational (Polars-backed)
        Some("head") => super::ops::head(root, rest),
        Some("schema") => super::ops::schema(root, rest),
        Some("stats") => super::ops::stats(root, rest),
        Some("count") => super::ops::count(root, rest),
        Some("select") => super::ops::select(root, rest),
        Some("append") => super::ops::append(root, rest),
        Some("update") => super::ops::update(root, rest),
        Some("delete-rows") => super::ops::delete_rows(root, rest),
        Some("add-column") => super::ops::add_column(root, rest),
        Some("drop-column") => super::ops::drop_column(root, rest),
        Some("import") => super::ops::import(root, rest),
        Some("export") => super::ops::export(root, rest),
        Some(unknown) => {
            print_json(&json!({
                "ok": false,
                "form": "data",
                "error": format!("unknown subcommand: {unknown}"),
                "lifecycle_verbs": catalog_verbs(),
                "operational_verbs": super::ops::operational_verbs(),
            }))?;
            bail!("unknown knowledge data subcommand: {unknown}");
        }
    }
}

fn help_payload() -> Value {
    json!({
        "ok": true,
        "form": "data",
        "scope": "record-shape knowledge tables — Level 2 catalog lifecycle + Level 3 operational verbs",
        "lifecycle_verbs": catalog_verbs(),
        "operational_verbs": super::ops::operational_verbs(),
        "note": "Lifecycle verbs touch only the catalog (knowledge_data_tables). Operational verbs read/write the underlying Parquet content via Polars. For real data-science work, use `clone` to fork a table, drive Python scripts via the shell tool against the Parquet path, and bring results back via `import`."
    })
}

fn catalog_verbs() -> Value {
    json!([
        {"verb": "create",   "args": "--domain X --key Y [--source-system S] [--title T] [--description D]"},
        {"verb": "list",     "args": "[--domain X] [--source-system S] [--tag k=v] [--include-archived]"},
        {"verb": "describe", "args": "--domain X --key Y"},
        {"verb": "clone",    "args": "--from-domain A --from-key B --to-domain C --to-key D [--title T] [--description D] [--source-system S]"},
        {"verb": "rename",   "args": "--domain X --key Y --to-domain X2 --to-key Y2"},
        {"verb": "archive",  "args": "--domain X --key Y"},
        {"verb": "restore",  "args": "--domain X --key Y"},
        {"verb": "delete",   "args": "--domain X --key Y --confirm <key>"},
        {"verb": "tag",      "args": "--domain X --key Y --tag k=v"},
        {"verb": "untag",    "args": "--domain X --key Y --tag k"},
    ])
}

// ----- verbs ---------------------------------------------------------------

fn create(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_CREATE)?;
    let table_key = required_flag(args, "--key", USAGE_CREATE)?;
    validate_identifier("domain", domain)?;
    validate_identifier("key", table_key)?;
    let source_system = find_flag(args, "--source-system")
        .unwrap_or("agent")
        .to_string();
    let title = find_flag(args, "--title").unwrap_or(table_key).to_string();
    let description = find_flag(args, "--description").unwrap_or("").to_string();

    let conn = open_runtime_db(root)?;
    if find_table(&conn, domain, table_key)?.is_some() {
        bail!("knowledge data table already exists: domain={domain} key={table_key}");
    }

    let parquet_path = compute_parquet_path(root, domain, table_key);
    if let Some(parent) = parquet_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parquet parent {}", parent.display()))?;
    }

    let table_id = format!("kdt-{}", Uuid::new_v4());
    let now = now_rfc3339();
    let parquet_path_str = parquet_path.to_string_lossy().into_owned();

    conn.execute(
        "INSERT INTO knowledge_data_tables (
             table_id, domain, table_key, source_system, title, description,
             parquet_path, schema_hash, row_count, bytes, tags_json, archived_at,
             created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '', 0, 0, '{}', NULL, ?8, ?8)",
        params![
            table_id,
            domain,
            table_key,
            source_system,
            title,
            description,
            parquet_path_str,
            now,
        ],
    )?;

    print_json(&json!({
        "ok": true,
        "table_id": table_id,
        "domain": domain,
        "key": table_key,
        "source_system": source_system,
        "parquet_path": parquet_path_str,
        "row_count": 0,
        "bytes": 0,
        "created_at": now,
    }))
}

fn list(root: &Path, args: &[String]) -> Result<()> {
    let domain = find_flag(args, "--domain");
    let source_system = find_flag(args, "--source-system");
    let tag_filter = find_flag(args, "--tag");
    let include_archived = args.iter().any(|a| a == "--include-archived");

    let conn = open_runtime_db(root)?;
    let mut sql = String::from(
        "SELECT table_id, domain, table_key, source_system, title, description,
                parquet_path, schema_hash, row_count, bytes, tags_json,
                archived_at, created_at, updated_at
         FROM knowledge_data_tables WHERE 1=1",
    );
    let mut filters: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(d) = domain {
        sql.push_str(" AND domain = ?");
        filters.push(Box::new(d.to_string()));
    }
    if let Some(s) = source_system {
        sql.push_str(" AND source_system = ?");
        filters.push(Box::new(s.to_string()));
    }
    if !include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }
    sql.push_str(" ORDER BY domain ASC, table_key ASC");

    let mut stmt = conn.prepare(&sql)?;
    let params_dyn: Vec<&dyn rusqlite::ToSql> = filters.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(params_dyn.as_slice(), row_to_value)?;
    let mut tables = Vec::new();
    for row in rows {
        let row = row?;
        if let Some(filter) = tag_filter {
            if !row_matches_tag(&row, filter)? {
                continue;
            }
        }
        tables.push(row);
    }

    print_json(&json!({
        "ok": true,
        "count": tables.len(),
        "tables": tables,
    }))
}

fn describe(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_DESCRIBE)?;
    let table_key = required_flag(args, "--key", USAGE_DESCRIBE)?;

    let conn = open_runtime_db(root)?;
    let Some(row) = find_table(&conn, domain, table_key)? else {
        bail!("knowledge data table not found: domain={domain} key={table_key}");
    };

    let parquet_path_str = row
        .get("parquet_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    let parquet_path = PathBuf::from(parquet_path_str);
    let parquet_exists = parquet_path.exists();
    let parquet_bytes = if parquet_exists {
        fs::metadata(&parquet_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    print_json(&json!({
        "ok": true,
        "table": row,
        "filesystem": {
            "parquet_exists": parquet_exists,
            "parquet_bytes_on_disk": parquet_bytes,
        }
    }))
}

fn clone(root: &Path, args: &[String]) -> Result<()> {
    let from_domain = required_flag(args, "--from-domain", USAGE_CLONE)?;
    let from_key = required_flag(args, "--from-key", USAGE_CLONE)?;
    let to_domain = required_flag(args, "--to-domain", USAGE_CLONE)?;
    let to_key = required_flag(args, "--to-key", USAGE_CLONE)?;
    validate_identifier("to-domain", to_domain)?;
    validate_identifier("to-key", to_key)?;

    let conn = open_runtime_db(root)?;
    let Some(src) = find_table(&conn, from_domain, from_key)? else {
        bail!("source knowledge data table not found: domain={from_domain} key={from_key}");
    };
    if find_table(&conn, to_domain, to_key)?.is_some() {
        bail!("destination knowledge data table already exists: domain={to_domain} key={to_key}");
    }

    let source_system = find_flag(args, "--source-system")
        .map(str::to_string)
        .unwrap_or_else(|| {
            src.get("source_system")
                .and_then(Value::as_str)
                .unwrap_or("agent")
                .to_string()
        });
    let title = find_flag(args, "--title")
        .map(str::to_string)
        .unwrap_or_else(|| {
            src.get("title")
                .and_then(Value::as_str)
                .unwrap_or(to_key)
                .to_string()
        });
    let description = find_flag(args, "--description")
        .map(str::to_string)
        .unwrap_or_else(|| {
            src.get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        });

    let new_parquet_path = compute_parquet_path(root, to_domain, to_key);
    if let Some(parent) = new_parquet_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let src_parquet = src
        .get("parquet_path")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let mut copied_bytes = 0u64;
    if let Some(src_path) = src_parquet.as_ref() {
        if src_path.exists() {
            copied_bytes = fs::copy(src_path, &new_parquet_path).with_context(|| {
                format!(
                    "failed to copy parquet from {} to {}",
                    src_path.display(),
                    new_parquet_path.display()
                )
            })?;
        }
    }

    let new_table_id = format!("kdt-{}", Uuid::new_v4());
    let now = now_rfc3339();
    let schema_hash = src
        .get("schema_hash")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let row_count = src.get("row_count").and_then(Value::as_i64).unwrap_or(0);
    let bytes = src.get("bytes").and_then(Value::as_i64).unwrap_or(0);
    let tags_json = src
        .get("tags_json")
        .and_then(Value::as_str)
        .unwrap_or("{}")
        .to_string();

    conn.execute(
        "INSERT INTO knowledge_data_tables (
             table_id, domain, table_key, source_system, title, description,
             parquet_path, schema_hash, row_count, bytes, tags_json, archived_at,
             created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, ?12, ?12)",
        params![
            new_table_id,
            to_domain,
            to_key,
            source_system,
            title,
            description,
            new_parquet_path.to_string_lossy().into_owned(),
            schema_hash,
            row_count,
            bytes,
            tags_json,
            now,
        ],
    )?;

    print_json(&json!({
        "ok": true,
        "cloned": {
            "from": {"domain": from_domain, "key": from_key},
            "to": {"domain": to_domain, "key": to_key},
            "table_id": new_table_id,
            "parquet_path": new_parquet_path.to_string_lossy(),
            "parquet_bytes_copied": copied_bytes,
        }
    }))
}

fn rename(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_RENAME)?;
    let table_key = required_flag(args, "--key", USAGE_RENAME)?;
    let to_domain = required_flag(args, "--to-domain", USAGE_RENAME)?;
    let to_key = required_flag(args, "--to-key", USAGE_RENAME)?;
    validate_identifier("to-domain", to_domain)?;
    validate_identifier("to-key", to_key)?;

    let conn = open_runtime_db(root)?;
    let Some(src) = find_table(&conn, domain, table_key)? else {
        bail!("knowledge data table not found: domain={domain} key={table_key}");
    };
    if (domain != to_domain || table_key != to_key)
        && find_table(&conn, to_domain, to_key)?.is_some()
    {
        bail!("destination already exists: domain={to_domain} key={to_key}");
    }

    let old_parquet = src
        .get("parquet_path")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let new_parquet = compute_parquet_path(root, to_domain, to_key);
    if let Some(parent) = new_parquet.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut moved = false;
    if let Some(old_path) = old_parquet.as_ref() {
        if old_path.exists() && old_path != &new_parquet {
            fs::rename(old_path, &new_parquet).with_context(|| {
                format!(
                    "failed to rename parquet {} -> {}",
                    old_path.display(),
                    new_parquet.display()
                )
            })?;
            moved = true;
        }
    }

    let now = now_rfc3339();
    conn.execute(
        "UPDATE knowledge_data_tables
            SET domain = ?1, table_key = ?2, parquet_path = ?3, updated_at = ?4
          WHERE domain = ?5 AND table_key = ?6",
        params![
            to_domain,
            to_key,
            new_parquet.to_string_lossy().into_owned(),
            now,
            domain,
            table_key,
        ],
    )?;

    print_json(&json!({
        "ok": true,
        "renamed": {
            "from": {"domain": domain, "key": table_key},
            "to": {"domain": to_domain, "key": to_key},
            "parquet_path": new_parquet.to_string_lossy(),
            "parquet_file_moved": moved,
        }
    }))
}

fn archive(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_ARCHIVE)?;
    let table_key = required_flag(args, "--key", USAGE_ARCHIVE)?;

    let conn = open_runtime_db(root)?;
    let now = now_rfc3339();
    let updated = conn.execute(
        "UPDATE knowledge_data_tables
            SET archived_at = ?1, updated_at = ?1
          WHERE domain = ?2 AND table_key = ?3 AND archived_at IS NULL",
        params![now, domain, table_key],
    )?;
    if updated == 0 {
        bail!("no active knowledge data table to archive: domain={domain} key={table_key}");
    }
    print_json(&json!({
        "ok": true,
        "archived": {"domain": domain, "key": table_key, "archived_at": now},
    }))
}

fn restore(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_RESTORE)?;
    let table_key = required_flag(args, "--key", USAGE_RESTORE)?;

    let conn = open_runtime_db(root)?;
    let now = now_rfc3339();
    let updated = conn.execute(
        "UPDATE knowledge_data_tables
            SET archived_at = NULL, updated_at = ?1
          WHERE domain = ?2 AND table_key = ?3 AND archived_at IS NOT NULL",
        params![now, domain, table_key],
    )?;
    if updated == 0 {
        bail!("no archived knowledge data table to restore: domain={domain} key={table_key}");
    }
    print_json(&json!({
        "ok": true,
        "restored": {"domain": domain, "key": table_key},
    }))
}

fn delete(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_DELETE)?;
    let table_key = required_flag(args, "--key", USAGE_DELETE)?;
    let confirm = required_flag(args, "--confirm", USAGE_DELETE)?;
    if confirm != table_key {
        bail!("--confirm must equal --key ({table_key}) to authorize delete");
    }

    let conn = open_runtime_db(root)?;
    let Some(row) = find_table(&conn, domain, table_key)? else {
        bail!("knowledge data table not found: domain={domain} key={table_key}");
    };
    let parquet_path = row
        .get("parquet_path")
        .and_then(Value::as_str)
        .map(PathBuf::from);

    let mut file_removed = false;
    let mut parent_removed = false;
    if let Some(path) = parquet_path.as_ref() {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("failed to remove parquet {}", path.display()))?;
            file_removed = true;
        }
        // Best-effort cleanup of the empty <domain>/ subdirectory under
        // runtime/knowledge/data/. Ignore errors — leaving an empty dir
        // behind is harmless.
        if let Some(parent) = path.parent() {
            if let Ok(mut entries) = fs::read_dir(parent) {
                if entries.next().is_none() {
                    if fs::remove_dir(parent).is_ok() {
                        parent_removed = true;
                    }
                }
            }
        }
    }

    conn.execute(
        "DELETE FROM knowledge_data_tables WHERE domain = ?1 AND table_key = ?2",
        params![domain, table_key],
    )?;

    print_json(&json!({
        "ok": true,
        "deleted": {
            "domain": domain,
            "key": table_key,
            "parquet_file_removed": file_removed,
            "parent_dir_removed": parent_removed,
        }
    }))
}

fn tag(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_TAG)?;
    let table_key = required_flag(args, "--key", USAGE_TAG)?;
    let raw = required_flag(args, "--tag", USAGE_TAG)?;
    let Some((k, v)) = raw.split_once('=') else {
        bail!("--tag expects key=value");
    };
    let k = k.trim();
    let v = v.trim();
    if k.is_empty() {
        bail!("--tag key cannot be empty");
    }

    let conn = open_runtime_db(root)?;
    let Some(current) = find_table(&conn, domain, table_key)? else {
        bail!("knowledge data table not found: domain={domain} key={table_key}");
    };
    let tags_str = current
        .get("tags_json")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    let mut tags: Map<String, Value> = serde_json::from_str(tags_str).unwrap_or_default();
    tags.insert(k.to_string(), Value::String(v.to_string()));
    let new_tags = serde_json::to_string(&tags)?;
    let now = now_rfc3339();
    conn.execute(
        "UPDATE knowledge_data_tables
            SET tags_json = ?1, updated_at = ?2
          WHERE domain = ?3 AND table_key = ?4",
        params![new_tags, now, domain, table_key],
    )?;
    print_json(&json!({
        "ok": true,
        "domain": domain,
        "key": table_key,
        "tags": tags,
    }))
}

fn untag(root: &Path, args: &[String]) -> Result<()> {
    let domain = required_flag(args, "--domain", USAGE_UNTAG)?;
    let table_key = required_flag(args, "--key", USAGE_UNTAG)?;
    let k = required_flag(args, "--tag", USAGE_UNTAG)?;

    let conn = open_runtime_db(root)?;
    let Some(current) = find_table(&conn, domain, table_key)? else {
        bail!("knowledge data table not found: domain={domain} key={table_key}");
    };
    let tags_str = current
        .get("tags_json")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    let mut tags: Map<String, Value> = serde_json::from_str(tags_str).unwrap_or_default();
    let removed = tags.remove(k).is_some();
    let new_tags = serde_json::to_string(&tags)?;
    let now = now_rfc3339();
    conn.execute(
        "UPDATE knowledge_data_tables
            SET tags_json = ?1, updated_at = ?2
          WHERE domain = ?3 AND table_key = ?4",
        params![new_tags, now, domain, table_key],
    )?;
    print_json(&json!({
        "ok": true,
        "domain": domain,
        "key": table_key,
        "tag_removed": removed,
        "tags": tags,
    }))
}

// ----- helpers -------------------------------------------------------------
//
// Helpers that the Level 3 `ops` submodule reuses are exposed `pub(super)`
// so they can be referenced as `super::data::<name>`. Their signatures
// and behavior are stable contract for the operational verbs.

pub(super) fn open_runtime_db(root: &Path) -> Result<Connection> {
    let path = crate::paths::core_db(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime db dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open runtime db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    ensure_local_schema(&conn)?;
    Ok(conn)
}

/// Idempotent schema bootstrap so knowledge commands work without
/// going through `tickets::open_db` first. Matches the Level 1 schema.
fn ensure_local_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS knowledge_data_tables (
            table_id      TEXT PRIMARY KEY,
            domain        TEXT NOT NULL,
            table_key     TEXT NOT NULL,
            source_system TEXT NOT NULL,
            title         TEXT NOT NULL,
            description   TEXT NOT NULL,
            parquet_path  TEXT NOT NULL,
            schema_hash   TEXT NOT NULL DEFAULT '',
            row_count     INTEGER NOT NULL DEFAULT 0,
            bytes         INTEGER NOT NULL DEFAULT 0,
            tags_json     TEXT NOT NULL DEFAULT '{}',
            archived_at   TEXT,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            UNIQUE(source_system, domain, table_key)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_data_tables_domain
            ON knowledge_data_tables(domain, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_knowledge_data_tables_source
            ON knowledge_data_tables(source_system, updated_at DESC);
        "#,
    )?;
    // Defensive column add for installs that created the table before
    // archived_at existed.
    let has_archived_at: bool = conn
        .prepare("PRAGMA table_info(knowledge_data_tables)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "archived_at");
    if !has_archived_at {
        conn.execute(
            "ALTER TABLE knowledge_data_tables ADD COLUMN archived_at TEXT",
            [],
        )?;
    }
    Ok(())
}

pub(super) fn find_table(
    conn: &Connection,
    domain: &str,
    table_key: &str,
) -> Result<Option<Map<String, Value>>> {
    let mut stmt = conn.prepare(
        "SELECT table_id, domain, table_key, source_system, title, description,
                parquet_path, schema_hash, row_count, bytes, tags_json,
                archived_at, created_at, updated_at
         FROM knowledge_data_tables
         WHERE domain = ?1 AND table_key = ?2",
    )?;
    let row = stmt
        .query_row(params![domain, table_key], row_to_value)
        .optional()?;
    Ok(row)
}

fn row_to_value(row: &rusqlite::Row) -> rusqlite::Result<Map<String, Value>> {
    let mut out = Map::new();
    out.insert("table_id".into(), Value::String(row.get(0)?));
    out.insert("domain".into(), Value::String(row.get(1)?));
    out.insert("table_key".into(), Value::String(row.get(2)?));
    out.insert("source_system".into(), Value::String(row.get(3)?));
    out.insert("title".into(), Value::String(row.get(4)?));
    out.insert("description".into(), Value::String(row.get(5)?));
    out.insert("parquet_path".into(), Value::String(row.get(6)?));
    out.insert("schema_hash".into(), Value::String(row.get(7)?));
    out.insert("row_count".into(), Value::from(row.get::<_, i64>(8)?));
    out.insert("bytes".into(), Value::from(row.get::<_, i64>(9)?));
    out.insert("tags_json".into(), Value::String(row.get(10)?));
    let archived: Option<String> = row.get(11)?;
    out.insert(
        "archived_at".into(),
        archived.map(Value::String).unwrap_or(Value::Null),
    );
    out.insert("created_at".into(), Value::String(row.get(12)?));
    out.insert("updated_at".into(), Value::String(row.get(13)?));
    Ok(out)
}

fn row_matches_tag(row: &Map<String, Value>, filter: &str) -> Result<bool> {
    let (k, v) = match filter.split_once('=') {
        Some(pair) => pair,
        None => bail!("--tag filter expects key=value"),
    };
    let tags_str = row.get("tags_json").and_then(Value::as_str).unwrap_or("{}");
    let tags: Map<String, Value> = serde_json::from_str(tags_str).unwrap_or_default();
    Ok(tags
        .get(k)
        .and_then(Value::as_str)
        .is_some_and(|cur| cur == v))
}

pub(super) fn compute_parquet_path(root: &Path, domain: &str, table_key: &str) -> PathBuf {
    root.join(KNOWLEDGE_DATA_ROOT)
        .join(domain)
        .join(format!("{table_key}.parquet"))
}

pub(super) fn validate_identifier(label: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 128 {
        bail!("{label} must be 1..=128 chars");
    }
    if value.starts_with('.') || value.contains("..") || value.contains('/') {
        bail!("{label} must not contain '/', '..', or start with '.'");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        bail!("{label} may only contain [a-zA-Z0-9_.-]");
    }
    Ok(())
}

pub(super) fn required_flag<'a>(
    args: &'a [String],
    flag: &str,
    usage: &'static str,
) -> Result<&'a str> {
    find_flag(args, flag).with_context(|| format!("missing {flag}. usage: {usage}"))
}

pub(super) fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(String::as_str)
}

pub(super) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub(super) fn print_json(value: &Value) -> Result<()> {
    // Delegate to the namespace-level sink so capture-mode (used by the
    // daemon IPC handler) intercepts the write instead of stdout. Keeping
    // this thin wrapper preserves the existing `pub(super)` import path
    // from `ops.rs` without a sweeping refactor of every callsite.
    crate::knowledge::print_json(value)
}

const USAGE_CREATE: &str =
    "ctox knowledge data create --domain X --key Y [--source-system S] [--title T] [--description D]";
const USAGE_DESCRIBE: &str = "ctox knowledge data describe --domain X --key Y";
const USAGE_CLONE: &str = "ctox knowledge data clone --from-domain A --from-key B --to-domain C --to-key D [--title T] [--description D] [--source-system S]";
const USAGE_RENAME: &str =
    "ctox knowledge data rename --domain X --key Y --to-domain X2 --to-key Y2";
const USAGE_ARCHIVE: &str = "ctox knowledge data archive --domain X --key Y";
const USAGE_RESTORE: &str = "ctox knowledge data restore --domain X --key Y";
const USAGE_DELETE: &str = "ctox knowledge data delete --domain X --key Y --confirm <key>";
const USAGE_TAG: &str = "ctox knowledge data tag --domain X --key Y --tag k=v";
const USAGE_UNTAG: &str = "ctox knowledge data untag --domain X --key Y --tag k";
