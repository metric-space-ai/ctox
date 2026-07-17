// Origin: CTOX
// License: AGPL-3.0-only
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
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use url::Url;
use uuid::Uuid;

const KNOWLEDGE_DATA_ROOT: &str = "runtime/knowledge/data";

type KnowledgeFileChangeStamp = (bool, u64, u128, String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeTablesProjectionSourceStamp {
    catalog_rows: Vec<KnowledgeTablesCatalogSourceStamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KnowledgeTablesCatalogSourceStamp {
    table_id: String,
    domain: String,
    table_key: String,
    source_system: String,
    title: String,
    description: String,
    schema_hash: String,
    content_hash: String,
    row_count: i64,
    bytes: i64,
    tags_json: String,
    updated_at: String,
    parquet_path: String,
    parquet_file: KnowledgeFileChangeStamp,
}

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

/// Source and claim tables are allowed to retain discovery candidates, but
/// their evidence flag is derived here rather than trusted from caller input.
/// This helper is used both on write paths and immediately before projection so
/// direct file edits or legacy rows cannot reintroduce a self-asserted claim.
pub(super) fn normalize_evidence_rows(table_key: &str, rows: Vec<Value>) -> Result<Vec<Value>> {
    if !is_evidence_table(table_key) {
        return Ok(rows);
    }

    rows.into_iter()
        .map(|row| {
            let mut object = row
                .as_object()
                .cloned()
                .context("evidence knowledge rows must be JSON objects")?;
            let reasons = evidence_rejection_reasons(table_key, &object);
            let eligible = reasons.is_empty();
            object.insert("evidence_eligible".to_string(), Value::Bool(eligible));
            if eligible {
                object.remove("evidence_rejection_reason");
            } else {
                object.insert(
                    "evidence_rejection_reason".to_string(),
                    Value::String(reasons.join(",")),
                );
            }
            Ok(Value::Object(object))
        })
        .collect()
}

pub(super) fn is_evidence_table(table_key: &str) -> bool {
    let key = table_key.trim().to_ascii_lowercase().replace('-', "_");
    matches!(
        key.as_str(),
        "source_catalog"
            | "source_claims"
            | "claims"
            | "knowledge_claims"
            | "evidence_points"
            | "evidence_claims"
            | "claim_evidence"
    ) || key.ends_with("_claims")
}

fn evidence_rejection_reasons(table_key: &str, row: &Map<String, Value>) -> Vec<String> {
    let mut reasons = Vec::new();
    let canonical_url = row
        .get("canonical_url")
        .and_then(Value::as_str)
        .is_some_and(valid_canonical_url);
    if !canonical_url {
        reasons.push("invalid_canonical_url".to_string());
    }

    if row.get("verification_status").and_then(Value::as_str) != Some("verified") {
        reasons.push("verification_not_verified".to_string());
    }
    if row.get("transport_verified") != Some(&Value::Bool(true)) {
        reasons.push("transport_not_verified".to_string());
    }
    if row.get("content_extracted") != Some(&Value::Bool(true)) {
        reasons.push("content_not_extracted".to_string());
    }
    if row.get("actual_full_text_or_data") != Some(&Value::Bool(true)) {
        reasons.push("full_content_not_verified".to_string());
    }
    if !row
        .get("evidence_relevance_score")
        .and_then(Value::as_i64)
        .is_some_and(|score| score >= 8)
    {
        reasons.push("evidence_relevance_below_threshold".to_string());
    }

    let http_status = row.get("http_status").and_then(Value::as_i64);
    if !http_status.is_some_and(|status| (200..=299).contains(&status)) {
        reasons.push("http_status_not_2xx".to_string());
    } else if http_status == Some(204) {
        reasons.push("http_status_no_content".to_string());
    }
    if !row
        .get("snapshot_hash")
        .and_then(Value::as_str)
        .is_some_and(valid_snapshot_hash)
    {
        reasons.push("invalid_snapshot_hash".to_string());
    }

    let source_tier = row
        .get("source_tier")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if source_tier.is_empty()
        || source_tier.contains("metadata")
        || source_tier.contains("aggregat")
    {
        reasons.push("metadata_or_aggregated_source_tier".to_string());
    }
    if row
        .get("metadata_only")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        reasons.push("metadata_only".to_string());
    }

    let source_type = row
        .get("source_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if source_type.contains("metadata") || source_type == "aggregator" {
        reasons.push("metadata_or_aggregated_source_type".to_string());
    }
    if row
        .get("canonical_url")
        .and_then(Value::as_str)
        .is_some_and(is_metadata_canonical_url)
    {
        reasons.push("metadata_canonical_url".to_string());
    }
    if row
        .get("evidence_rejection_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.trim().is_empty())
    {
        reasons.push("evidence_rejection_reason_present".to_string());
    }
    append_trace_rejection_reasons(table_key, row, &mut reasons);
    reasons
}

fn append_trace_rejection_reasons(
    table_key: &str,
    row: &Map<String, Value>,
    reasons: &mut Vec<String>,
) {
    let has_nonempty = |key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    let key = table_key.trim().to_ascii_lowercase().replace('-', "_");
    if key.contains("claim") || key.contains("evidence") {
        if !has_nonempty("source_id") {
            reasons.push("missing_source_id".to_string());
        }
        if !has_nonempty("claim_id") && !has_nonempty("evidence_id") {
            reasons.push("missing_claim_or_evidence_id".to_string());
        }
        if !has_nonempty("snapshot_id") {
            reasons.push("missing_snapshot_id".to_string());
        }
        return;
    }

    let primary = if key == "source_catalog" || key.contains("source") {
        has_nonempty("source_id")
    } else {
        has_nonempty("source_id") || has_nonempty("claim_id") || has_nonempty("evidence_id")
    };
    let trace = [
        "trace_id",
        "run_id",
        "research_run_id",
        "source_row_ref",
        "provenance",
    ]
    .iter()
    .any(|key| has_nonempty(key));
    if !primary || !trace {
        reasons.push("missing_trace_identifier".to_string());
    }
}

fn valid_canonical_url(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed != value {
        return false;
    }
    Url::parse(trimmed)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

fn valid_snapshot_hash(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_metadata_canonical_url(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    [
        "https://doi.org/",
        "http://doi.org/",
        "https://api.crossref.org/",
        "https://api.openalex.org/",
        "https://api.semanticscholar.org/",
        "https://www.semanticscholar.org/",
        "https://scholar.google.",
        "https://www.researchgate.net/",
        "https://www.academia.edu/",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
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

pub fn knowledge_tables_projection_source_stamp(
    root: &Path,
) -> Result<KnowledgeTablesProjectionSourceStamp> {
    let conn = open_runtime_db(root)?;
    let mut stmt = conn.prepare(
        "SELECT table_id, domain, table_key, source_system, title, description,
                schema_hash, row_count, bytes, tags_json, updated_at
         FROM knowledge_data_tables
         WHERE archived_at IS NULL
         ORDER BY updated_at DESC, title, table_id",
    )?;
    let catalog_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let catalog_rows = catalog_rows
        .into_iter()
        .map(
            |(
                table_id,
                domain,
                table_key,
                source_system,
                title,
                description,
                schema_hash,
                row_count,
                bytes,
                tags_json,
                updated_at,
            )| {
                let parquet_path = compute_parquet_path(root, &domain, &table_key);
                let content_hash = knowledge_file_content_hash(&parquet_path);
                KnowledgeTablesCatalogSourceStamp {
                    table_id,
                    domain,
                    table_key,
                    source_system,
                    title,
                    description,
                    schema_hash,
                    content_hash,
                    row_count,
                    bytes,
                    tags_json,
                    updated_at,
                    parquet_path: parquet_path.display().to_string(),
                    parquet_file: knowledge_file_change_stamp(&parquet_path),
                }
            },
        )
        .collect();

    Ok(KnowledgeTablesProjectionSourceStamp { catalog_rows })
}

fn knowledge_file_change_stamp(path: &Path) -> KnowledgeFileChangeStamp {
    let Ok(metadata) = fs::metadata(path) else {
        return (false, 0, 0, String::new());
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    (
        metadata.is_file(),
        metadata.len(),
        modified_at,
        knowledge_file_content_hash(path),
    )
}

fn knowledge_file_content_hash(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let Ok(read) = file.read(&mut buffer) else {
            return String::new();
        };
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

/// Hard upper bound on rows projected for one logical knowledge table.
const KNOWLEDGE_TABLE_RXDB_ROW_CAP: usize = 5_000;

/// Maximum rows carried by one replicated `knowledge_tables` document.
///
/// A single wide evidence row can contain substantial provenance metadata.
/// Keeping chunks deliberately small prevents multi-megabyte RxDB documents
/// from stalling WebRTC replication while still preserving the complete
/// logical table in the browser.
const KNOWLEDGE_TABLE_RXDB_CHUNK_ROWS: usize = 200;

/// Build the `knowledge_tables` RxDB documents that carry record-shape
/// knowledge to the Business OS browser surfaces (Web Research + Knowledge).
///
/// This is the single native source of truth for that synced collection.
/// Business OS reads rows exclusively from the synced doc payload over
/// RxDB/WebRTC — there is no HTTP data path — so the rows must be embedded
/// here in the doc itself.
///
/// For each active catalog row in `knowledge_data_tables` we:
///   1. RE-RESOLVE the parquet path from `(domain, table_key)` against the
///      live state dir via [`compute_parquet_path`]. The `parquet_path`
///      column persisted in the catalog can be stale (it may point at a
///      deleted/old release dir), so it is never trusted for reading; the
///      resolved path is what we read and what we re-publish in the doc.
///   2. Read the parquet rows via the shared Polars helpers
///      (`scan_table` + `df_to_rows`), capped at
///      [`KNOWLEDGE_TABLE_RXDB_ROW_CAP`].
///   3. Add table-specific schema metadata for the browser, including
///      standardized metric columns and hover-help descriptions.
///   4. Emit deterministic row chunks. Chunk zero keeps the historical
///      `table:<table_id>` id; later chunks use
///      `table:<table_id>:chunk:<index>`. Browser modules merge chunks by
///      `logical_table_id`.
///
/// A missing or unreadable parquet file is not fatal: the doc is still
/// emitted (so the table appears in the catalog UI) but with an empty `rows`
/// array, and the resolved path is reported so the caller can see what was
/// expected.
pub fn knowledge_tables_rxdb_documents(root: &Path) -> Result<Vec<Value>> {
    let conn = open_runtime_db(root)?;
    let mut stmt = conn.prepare(
        "SELECT table_id, domain, table_key, source_system, title, description,
                schema_hash, row_count, bytes, updated_at
         FROM knowledge_data_tables
         WHERE archived_at IS NULL
         ORDER BY updated_at DESC, title",
    )?;
    let catalog_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // table_id
                row.get::<_, String>(1)?, // domain
                row.get::<_, String>(2)?, // table_key
                row.get::<_, String>(3)?, // source_system
                row.get::<_, String>(4)?, // title
                row.get::<_, String>(5)?, // description
                row.get::<_, String>(6)?, // schema_hash (catalog fallback)
                row.get::<_, i64>(7)?,    // row_count (catalog, may be stale)
                row.get::<_, i64>(8)?,    // bytes
                row.get::<_, String>(9)?, // updated_at (rfc3339)
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut documents = Vec::with_capacity(catalog_rows.len());
    for (
        table_id,
        domain,
        table_key,
        source_system,
        title,
        description,
        catalog_schema_hash,
        row_count,
        bytes,
        updated_at,
    ) in catalog_rows
    {
        // (1) Re-resolve against the live state dir; never trust the stored path.
        let resolved_path = compute_parquet_path(root, &domain, &table_key);
        let resolved_path_str = resolved_path.display().to_string();

        // (2) Read rows (capped). Missing/unreadable parquet is non-fatal.
        let (rows, resolved_row_count, live_schema_hash) = if resolved_path.is_file() {
            let live_schema_hash = super::parquet_io::scan_table(&resolved_path)
                .and_then(|mut lf| lf.collect_schema())
                .map(|schema| super::parquet_io::schema_hash(&schema))
                .unwrap_or_else(|_| catalog_schema_hash.clone());
            match super::parquet_io::read_rows_capped(&resolved_path, KNOWLEDGE_TABLE_RXDB_ROW_CAP)
            {
                Ok((rows, count)) => (rows, count, live_schema_hash),
                Err(err) => {
                    eprintln!(
                        "[knowledge] knowledge_tables projection: failed to read parquet rows for \
                         {domain}/{table_key} ({}): {err:#}",
                        resolved_path.display()
                    );
                    (Vec::new(), row_count, live_schema_hash)
                }
            }
        } else {
            (Vec::new(), row_count, catalog_schema_hash.clone())
        };

        let rows = normalize_evidence_rows(&table_key, rows)?;
        let (rows, quality_notes) = enrich_knowledge_table_rows(&table_key, rows);
        let columns = knowledge_table_columns(&table_key, &rows);
        let schema_value = json!({ "columns": columns.clone() });
        let quality_notes_value =
            Value::Array(quality_notes.into_iter().map(Value::String).collect());
        let logical_table_id = format!("table:{table_id}");
        let updated_at_ms =
            rfc3339_to_millis(&updated_at).unwrap_or_else(|| Utc::now().timestamp_millis());
        let content_hash = knowledge_file_content_hash(&resolved_path);
        let projected_row_count = rows.len();
        let chunk_count = projected_row_count
            .max(1)
            .div_ceil(KNOWLEDGE_TABLE_RXDB_CHUNK_ROWS);
        let rows_complete = projected_row_count as i64 == resolved_row_count;

        // (4) Mirror each row chunk at payload.rows and top-level rows. The
        // logical row_count remains the complete parquet count on every chunk.
        for chunk_index in 0..chunk_count {
            let start = chunk_index * KNOWLEDGE_TABLE_RXDB_CHUNK_ROWS;
            let end = (start + KNOWLEDGE_TABLE_RXDB_CHUNK_ROWS).min(projected_row_count);
            let chunk_rows = if start < end {
                rows[start..end].to_vec()
            } else {
                Vec::new()
            };
            let id = if chunk_index == 0 {
                logical_table_id.clone()
            } else {
                format!("{logical_table_id}:chunk:{chunk_index:04}")
            };
            let rows_value = Value::Array(chunk_rows);
            let payload = json!({
                "id": id,
                "logical_table_id": logical_table_id,
                "table_id": table_id,
                "kind": "dataframe",
                "domain": domain,
                "table_key": table_key,
                "source_system": source_system,
                "title": title,
                "description": description,
                "parquet_path": resolved_path_str,
                "row_count": resolved_row_count,
                "projected_row_count": projected_row_count,
                "rows_complete": rows_complete,
                "chunk_index": chunk_index,
                "chunk_count": chunk_count,
                "chunk_row_offset": start,
                "chunk_row_count": end.saturating_sub(start),
                "bytes": bytes,
                "content_hash": content_hash,
                "schema_hash": live_schema_hash,
                "updated_at": updated_at,
                "has_table": true,
                "columns": columns.clone(),
                "schema": schema_value.clone(),
                "quality_notes": quality_notes_value.clone(),
                "rows": rows_value.clone(),
            });

            documents.push(json!({
                "id": id,
                "logical_table_id": logical_table_id,
                "table_id": table_id,
                "kind": "dataframe",
                "title": title,
                "subtitle": format!("{domain} · {resolved_row_count} rows"),
                "summary": description,
                "source_path": resolved_path_str,
                "domain": domain,
                "table_key": table_key,
                "row_count": resolved_row_count,
                "projected_row_count": projected_row_count,
                "rows_complete": rows_complete,
                "chunk_index": chunk_index,
                "chunk_count": chunk_count,
                "chunk_row_offset": start,
                "chunk_row_count": end.saturating_sub(start),
                "content_hash": content_hash,
                "schema_hash": live_schema_hash,
                "updated_at": updated_at,
                "updated_at_ms": updated_at_ms,
                "columns": columns.clone(),
                "schema": schema_value.clone(),
                "quality_notes": quality_notes_value.clone(),
                "rows": rows_value,
                "payload": payload,
            }));
        }
    }

    Ok(documents)
}

/// Parse an RFC3339 timestamp into epoch milliseconds. Returns `None` when the
/// string is empty or unparseable so the caller can fall back to "now".
fn rfc3339_to_millis(value: &str) -> Option<i64> {
    if value.trim().is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

fn enrich_knowledge_table_rows(table_key: &str, rows: Vec<Value>) -> (Vec<Value>, Vec<String>) {
    if table_key != "measured_load_points" {
        return (rows, Vec::new());
    }
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| enrich_measured_load_point_row(index, row))
        .collect();
    (
        rows,
        vec![
            "propeller_size is split into metric prop_diameter_mm and prop_pitch_mm for Excel-ready analysis.".to_string(),
            "legacy radial_load_N is preserved in raw rows and is only exposed as tangential_equivalent_force_N when explicit metadata proves a torque/radius derivation; bearing radial load remains blank unless the source establishes it.".to_string(),
            "load_case separates steady propeller tests, vibration, shock/blast, and mixed derived rows; aggregate these cases separately.".to_string(),
        ],
    )
}

fn enrich_measured_load_point_row(index: usize, mut row: Value) -> Value {
    let Some(map) = row.as_object_mut() else {
        return row;
    };

    normalize_number_field(map, "motor_kv");
    normalize_number_field(map, "vibration_g_rms");
    normalize_number_field(map, "prop_diameter_in");
    normalize_number_field(map, "prop_pitch_in");
    normalize_number_field(map, "rpm");
    normalize_number_field(map, "thrust_N");
    normalize_number_field(map, "axial_load_N");
    normalize_number_field(map, "torque_Nm");
    normalize_number_field(map, "radial_load_N");
    normalize_optional_number_field(map, "tangential_equivalent_force_N");
    normalize_optional_number_field(map, "bearing_radial_load_N");

    if !map.contains_key("row_id") {
        map.insert(
            "row_id".to_string(),
            Value::String(format!("MLP-{:04}", index + 1)),
        );
    }

    if let Some(size) = string_from_map(map, "propeller_size") {
        map.entry("original_propeller_size".to_string())
            .or_insert(Value::String(size));
    }

    if let Some(diameter_in) = number_from_map(map, "prop_diameter_in") {
        insert_number(map, "prop_diameter_mm", diameter_in * 25.4);
    }
    if let Some(pitch_in) = number_from_map(map, "prop_pitch_in") {
        insert_number(map, "prop_pitch_mm", pitch_in * 25.4);
    }

    if !map.contains_key("propeller_size") {
        if let (Some(diameter), Some(pitch)) = (
            number_from_map(map, "prop_diameter_in"),
            number_from_map(map, "prop_pitch_in"),
        ) {
            map.insert(
                "propeller_size".to_string(),
                Value::String(format!(
                    "{}x{}",
                    compact_decimal(diameter),
                    compact_decimal(pitch)
                )),
            );
        }
    }

    if let Some(force) =
        number_from_map(map, "thrust_N").or_else(|| number_from_map(map, "axial_load_N"))
    {
        insert_number(map, "force_N", force);
    }
    if let Some(torque) = number_from_map(map, "torque_Nm") {
        insert_number(map, "moment_Nm", torque);
        insert_number(map, "torque_signed_Nm", torque);
    }
    // `radial_load_N` is never reinterpreted here. A bearing radial load and a
    // torque/radius tangential equivalent are different physical quantities.
    // Derived values must be supplied explicitly with their own provenance.
    map.entry("tangential_equivalent_force_N".to_string())
        .or_insert(Value::Null);
    map.entry("bearing_radial_load_N".to_string())
        .or_insert(Value::Null);

    let load_case = infer_load_case(map);
    map.entry("load_case".to_string())
        .or_insert(Value::String(load_case.to_string()));
    let (measurement_kind, is_derived) = infer_measurement_kind(map);
    map.entry("measurement_kind".to_string())
        .or_insert(Value::String(measurement_kind.to_string()));
    map.entry("is_derived".to_string())
        .or_insert(Value::Bool(is_derived));
    map.entry("original_unit_system".to_string())
        .or_insert(Value::String("mixed_source_metric_projection".to_string()));

    if let Some(source_row_ref) = explicit_source_row_ref(map) {
        map.insert("source_row_ref".to_string(), Value::String(source_row_ref));
    } else {
        map.entry("source_row_ref".to_string())
            .or_insert(Value::Null);
    }
    normalize_derivation_method(map);

    row
}

fn explicit_source_row_ref(map: &Map<String, Value>) -> Option<String> {
    [
        "source_row_ref",
        "original_row_ref",
        "source_row",
        "original_row",
        "source_row_number",
        "original_row_number",
        "source_row_index",
        "original_row_index",
    ]
    .iter()
    .find_map(|key| string_from_map(map, key))
}

fn normalize_derivation_method(map: &mut serde_json::Map<String, Value>) {
    let Some(Value::String(method)) = map.get_mut("derivation_method") else {
        return;
    };
    if !method.contains("radial_load_N=TORQUE_Nm/(prop_diameter_m/2)") {
        return;
    }
    *method = method.replace(
        "radial_load_N=TORQUE_Nm/(prop_diameter_m/2) when torque present",
        "tangential_equivalent_force_N=abs(TORQUE_Nm/(prop_diameter_m/2)) when torque present; bearing_radial_load_N remains blank unless a source or model gives true bearing radial load",
    );
}

fn knowledge_table_columns(table_key: &str, rows: &[Value]) -> Vec<Value> {
    match table_key {
        "measured_load_points" => vec![
            column_def("row_id", "Row ID", "", "string", "Stable row identifier for audit, export, and report references.", ""),
            column_def("source_id", "Source ID", "", "string", "Source catalog identifier used to trace this measurement back to the evidence source.", ""),
            column_def("source_row_ref", "Source row reference", "", "string", "Original source-row reference supplied by the source data; blank when the source provides no row reference.", ""),
            column_def("propeller_size", "Propeller size", "in", "string", "Propeller shorthand such as 9x5 means 9 inch diameter and 5 inch pitch. It is shown and exported as metric diameter x pitch in millimetres.", "propeller_size"),
            column_def("prop_diameter_mm", "Diameter", "mm", "number", "Propeller diameter split from propeller_size or prop_diameter_in and normalized to millimetres.", "numeric"),
            column_def("prop_pitch_mm", "Pitch", "mm", "number", "Propeller pitch split from propeller_size or prop_pitch_in and normalized to millimetres.", "numeric"),
            column_def("rpm", "RPM", "rpm", "number", "Rotational speed in revolutions per minute; exported without thousands separators.", "numeric"),
            column_def("force_N", "Force", "N", "number", "Primary axial force value in newtons. For propeller datasets this is usually thrust.", "numeric"),
            column_def("axial_load_N", "Axial load", "N", "number", "Axial load in newtons, retained for bearing-load calculations and traceability.", "numeric"),
            column_def("torque_Nm", "Torque", "N m", "number", "Measured or derived shaft torque in newton metres.", "numeric"),
            column_def("moment_Nm", "Moment", "N m", "number", "Moment alias for torque_Nm so exports can be consumed by tools that expect Moment/Torque terminology.", "numeric"),
            column_def("tangential_equivalent_force_N", "Tangential equivalent force", "N", "number", "Torque/radius equivalent force in newtons. This is not a measured bearing radial load; use bearing_radial_load_N only when a source provides or models true radial bearing load.", "numeric"),
            column_def("bearing_radial_load_N", "Bearing radial load", "N", "number", "True radial bearing load in newtons when directly measured or explicitly modelled. Blank means not established by the source row.", "numeric"),
            column_def("vibration_g_rms", "Vibration", "g RMS", "number", "Vibration acceleration level in g RMS when provided by the source or scenario.", "numeric"),
            column_def("load_case", "Load case", "", "string", "Scenario bucket used to avoid mixing steady propulsion, vibration, shock/blast, and derived rows in one statistic.", ""),
            column_def("measurement_kind", "Measurement kind", "", "string", "Measured rows come directly from source data; derived rows were computed or inferred from source fields.", ""),
            column_def("is_derived", "Derived", "", "boolean", "True when the value was computed or inferred instead of directly copied from the source row.", ""),
            column_def("confidence", "Confidence", "", "string", "Extraction or derivation confidence supplied by the data-building workflow.", ""),
            column_def("derivation_method", "Derivation method", "", "string", "Short description of how the measurement row was obtained or calculated.", ""),
            column_def("source_file", "Source file", "", "string", "Original or staged file name used for the measurement row.", ""),
        ],
        "source_catalog" => vec![
            column_def("source_id", "Source ID", "", "string", "Canonical source identifier used by reports, runbooks, and measurement rows.", ""),
            column_def("title", "Title", "", "string", "Human-readable source title.", ""),
            column_def("source_url", "Source URL", "", "string", "URL or DOI landing page for source verification.", ""),
            column_def("canonical_url", "Canonical URL", "", "string", "Canonical publisher, repository, standards-body, or manufacturer URL that was actually verified.", ""),
            column_def("source_class", "Source class", "", "string", "Source type such as dataset, scholarly, manufacturer, standard, or web.", ""),
            column_def("source_tier", "Source tier", "", "string", "Trust tier assigned during verification, for example primary, authoritative secondary, or discovery-only.", ""),
            column_def("bucket", "Bucket", "", "string", "Research bucket used for portfolio mapping.", ""),
            column_def("review_status", "Review status", "", "string", "Review state assigned during source curation.", ""),
            column_def("verification_status", "Verification status", "", "string", "Machine-readable verification state. Only verified sources may contribute evidence scores, Knowledge claims, or report citations.", ""),
            column_def("evidence_eligible", "Evidence eligible", "", "boolean", "True only after a relevant source returned a successful response and its retrieved content was stored with a snapshot hash.", ""),
            column_def("checked_at", "Checked at", "", "datetime", "Timestamp of the latest source reachability and content verification.", ""),
            column_def("http_status", "HTTP status", "", "number", "HTTP response status observed during the latest verification; redirects must resolve to canonical_url.", "numeric"),
            column_def("snapshot_hash", "Snapshot hash", "", "string", "Content hash of the stored source snapshot used for extraction, Knowledge claims, and report citations.", ""),
            column_def("provenance", "Provenance", "", "string", "Retrieval and extraction provenance needed to reproduce the source evidence.", ""),
            column_def("candidate_stage", "Candidate stage", "", "string", "Pipeline stage for the source candidate.", ""),
            column_def("year", "Year", "", "number", "Publication or source year when available.", "numeric"),
            column_def("doi", "DOI", "", "string", "Digital object identifier when available.", ""),
            column_def("discovery_score", "Discovery score", "", "number", "Search relevance used only to prioritize verification; it is not an evidence score.", "numeric"),
            column_def("evidence_score", "Evidence score", "", "number", "Evidence strength computed only for evidence-eligible verified sources.", "numeric"),
            column_def("contribution_note", "Contribution note", "", "string", "Why this source matters for the SKF drone-bearing use case.", ""),
            column_def("relevance_to_bearing_design", "Bearing-design relevance", "", "string", "Evidence value for bearing sizing, loads, materials, lubrication, sealing, or reliability.", ""),
            column_def("evidence_note", "Evidence note", "", "string", "Reviewer note about data quality and limitations.", ""),
        ],
        "load_data_library" => vec![
            column_def("library_id", "Library ID", "", "string", "Canonical load-data library row identifier.", ""),
            column_def("source_id", "Source ID", "", "string", "Source catalog identifier for traceability.", ""),
            column_def("source_url", "Source URL", "", "string", "URL or DOI landing page for source verification.", ""),
            column_def("record_type", "Record type", "", "string", "Type of data extracted from the source.", ""),
            column_def("load_channels_available", "Load channels available", "", "string", "Which load channels are available, for example force, torque, rpm, vibration, or shock.", ""),
            column_def("bearing_design_use", "Bearing-design use", "", "string", "How the extracted record informs bearing sizing or selection.", ""),
            column_def("limitations", "Limitations", "", "string", "Known limitations or caveats for reuse in calculations and reports.", ""),
            column_def("review_status", "Review status", "", "string", "Curation state for this load-data record.", ""),
        ],
        _ => infer_columns_from_rows(rows),
    }
}

fn column_def(
    key: &str,
    label: &str,
    unit: &str,
    dtype: &str,
    description: &str,
    value_kind: &str,
) -> Value {
    json!({
        "key": key,
        "name": key,
        "label": label,
        "unit": unit,
        "type": dtype,
        "description": description,
        "valueKind": value_kind,
    })
}

fn infer_columns_from_rows(rows: &[Value]) -> Vec<Value> {
    let Some(object) = rows.iter().find_map(Value::as_object) else {
        return Vec::new();
    };
    object
        .iter()
        .map(|(key, value)| {
            let dtype = if value.is_number() {
                "number"
            } else if value.is_boolean() {
                "boolean"
            } else {
                "string"
            };
            column_def(key, key, "", dtype, "Projected knowledge-table column.", "")
        })
        .collect()
}

fn normalize_number_field(map: &mut Map<String, Value>, key: &str) {
    if let Some(number) = map.get(key).and_then(json_number) {
        insert_number(map, key, number);
    }
}

fn normalize_optional_number_field(map: &mut Map<String, Value>, key: &str) {
    if !map.contains_key(key) {
        return;
    }
    let value = map.get(key).and_then(json_number);
    match value {
        Some(number) => insert_number(map, key, number),
        None => {
            map.insert(key.to_string(), Value::Null);
        }
    }
}

fn number_from_map(map: &Map<String, Value>, key: &str) -> Option<f64> {
    map.get(key).and_then(json_number)
}

fn string_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    let value = map.get(key)?;
    if let Some(text) = value.as_str() {
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    if value.is_number() || value.is_boolean() {
        return Some(value.to_string());
    }
    None
}

fn json_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => parse_decimal_number(text),
        _ => None,
    }
}

fn parse_decimal_number(value: &str) -> Option<f64> {
    let mut normalized = value.trim().replace(' ', "");
    if normalized.is_empty() {
        return None;
    }
    let comma = normalized.rfind(',');
    let dot = normalized.rfind('.');
    if let (Some(comma), Some(dot)) = (comma, dot) {
        if comma > dot {
            normalized = normalized.replace('.', "").replace(',', ".");
        } else {
            normalized = normalized.replace(',', "");
        }
    } else if comma.is_some() {
        normalized = normalized.replace(',', ".");
    }
    normalized
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn insert_number(map: &mut Map<String, Value>, key: &str, value: f64) {
    if let Some(number) = serde_json::Number::from_f64(value) {
        map.insert(key.to_string(), Value::Number(number));
    }
}

fn infer_load_case(map: &Map<String, Value>) -> &'static str {
    let text = row_text(map);
    if text.contains("shock") || text.contains("blast") || text.contains("overpressure") {
        "shock_overpressure"
    } else if text.contains("vibration") || map.contains_key("vibration_g_rms") {
        "vibration_test"
    } else if text.contains("csv") || text.contains("prop") || map.contains_key("propeller_size") {
        "steady_propeller_test"
    } else {
        "derived_or_mixed"
    }
}

fn infer_measurement_kind(map: &Map<String, Value>) -> (&'static str, bool) {
    let text = row_text(map);
    let has_derived_fields = map
        .get("is_derived")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || [
            "derivation_formula",
            "derivation_metadata",
            "calculation_method",
            "calculation_formula",
        ]
        .iter()
        .any(|key| map.get(*key).is_some_and(|value| !value.is_null()))
        || (map.contains_key("tangential_equivalent_force_N")
            && (text.contains("torque") && text.contains("radius")));
    if has_derived_fields {
        return ("measured_with_derived_fields", true);
    }
    if text.contains("direct experimental") || text.contains("measured") || text.contains(".csv") {
        ("measured", false)
    } else {
        ("derived", true)
    }
}

fn row_text(map: &Map<String, Value>) -> String {
    [
        "source_id",
        "source_file",
        "record_type",
        "confidence",
        "derivation_method",
        "load_case",
    ]
    .iter()
    .filter_map(|key| string_from_map(map, key))
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase()
}

fn compact_decimal(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    if value.fract().abs() < f64::EPSILON {
        return format!("{value:.0}");
    }
    let mut text = format!("{value:.6}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
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

const USAGE_CREATE: &str = "ctox knowledge data create --domain X --key Y [--source-system S] [--title T] [--description D]";
const USAGE_DESCRIBE: &str = "ctox knowledge data describe --domain X --key Y";
const USAGE_CLONE: &str = "ctox knowledge data clone --from-domain A --from-key B --to-domain C --to-key D [--title T] [--description D] [--source-system S]";
const USAGE_RENAME: &str =
    "ctox knowledge data rename --domain X --key Y --to-domain X2 --to-key Y2";
const USAGE_ARCHIVE: &str = "ctox knowledge data archive --domain X --key Y";
const USAGE_RESTORE: &str = "ctox knowledge data restore --domain X --key Y";
const USAGE_DELETE: &str = "ctox knowledge data delete --domain X --key Y --confirm <key>";
const USAGE_TAG: &str = "ctox knowledge data tag --domain X --key Y --tag k=v";
const USAGE_UNTAG: &str = "ctox knowledge data untag --domain X --key Y --tag k";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Insert a catalog row whose persisted `parquet_path` is deliberately
    /// STALE (an old/deleted release dir), then write the real parquet at the
    /// resolved live path. The projection must:
    ///   - read the rows from the RE-RESOLVED path (not the stale one),
    ///   - embed them at both `payload.rows` and top-level `rows`,
    ///   - report the resolved (existing) `parquet_path` and the true
    ///     `row_count`.
    #[test]
    fn knowledge_tables_projection_embeds_rows_and_resolves_live_parquet_path() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let domain = "drone_bearing_design";
        let table_key = "source_catalog";
        let table_id = "kdt-test-source-catalog";

        // (a) Catalog row with a stale parquet_path that does not exist.
        let conn = open_runtime_db(root)?;
        let stale_path = root
            .join("OLD_RELEASE_DELETED")
            .join(domain)
            .join(format!("{table_key}.parquet"));
        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO knowledge_data_tables (
                 table_id, domain, table_key, source_system, title, description,
                 parquet_path, schema_hash, row_count, bytes, tags_json, archived_at,
                 created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'agent', 'Source Catalog', 'curated sources',
                       ?4, '', 999, 0, '{}', NULL, ?5, ?5)",
            params![
                table_id,
                domain,
                table_key,
                stale_path.to_string_lossy().into_owned(),
                now
            ],
        )?;

        // (b) Write the real parquet at the LIVE resolved path with 3 rows.
        let rows = vec![
            json!({"source_id": "s1", "title": "Bearing handbook", "weight": 0.9}),
            json!({"source_id": "s2", "title": "Drone load study", "weight": 0.7}),
            json!({"source_id": "s3", "title": "Material spec", "weight": 0.5}),
        ];
        let df = super::super::parquet_io::rows_to_df(&rows)?;
        let live_path = compute_parquet_path(root, domain, table_key);
        super::super::parquet_io::commit_parquet(&live_path, df)?;
        assert!(
            live_path.is_file(),
            "live parquet should exist after commit"
        );
        assert!(!stale_path.exists(), "stale path must not exist");

        // (c) Run the projection.
        let docs = knowledge_tables_rxdb_documents(root)?;
        assert_eq!(docs.len(), 1, "exactly one knowledge_tables doc");
        let doc = &docs[0];

        // id scheme matches the browser/HTTP `table:<id>` convention.
        assert_eq!(doc["id"].as_str(), Some("table:kdt-test-source-catalog"));
        assert_eq!(doc["kind"].as_str(), Some("dataframe"));

        // Rows embedded at both top-level and payload.rows.
        let top_rows = doc["rows"].as_array().expect("top-level rows array");
        assert_eq!(top_rows.len(), 3, "top-level rows count");
        let payload_rows = doc["payload"]["rows"]
            .as_array()
            .expect("payload.rows array");
        assert_eq!(payload_rows.len(), 3, "payload.rows count");

        // row_count reflects the REAL parquet, not the stale catalog value (999).
        assert_eq!(doc["row_count"].as_i64(), Some(3));
        assert_eq!(doc["payload"]["row_count"].as_i64(), Some(3));

        // parquet_path is the RE-RESOLVED live path, which exists.
        let resolved = doc["payload"]["parquet_path"]
            .as_str()
            .expect("payload.parquet_path");
        assert_eq!(resolved, live_path.display().to_string());
        assert!(
            std::path::Path::new(resolved).is_file(),
            "resolved parquet_path must point at an existing file"
        );
        assert_ne!(
            resolved,
            stale_path.display().to_string(),
            "must not echo the stale catalog path"
        );
        assert!(doc["content_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:") && hash.len() == 71));
        assert_eq!(
            doc["content_hash"], doc["payload"]["content_hash"],
            "content hash must be present at both projection levels"
        );
        assert!(doc["schema_hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64));
        assert_eq!(
            doc["schema_hash"], doc["payload"]["schema_hash"],
            "schema hash must be present at both projection levels"
        );

        // updated_at_ms is present (required RxDB field) and parsed from the
        // catalog rfc3339 stamp.
        assert!(doc["updated_at_ms"].as_i64().is_some());

        // The actual row content rode through into the doc.
        let titles: Vec<&str> = payload_rows
            .iter()
            .filter_map(|row| row.get("title").and_then(Value::as_str))
            .collect();
        assert!(titles.contains(&"Bearing handbook"));
        Ok(())
    }

    #[test]
    fn knowledge_tables_projection_chunks_wide_tables_without_losing_rows() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let domain = "verified_research";
        let table_key = "measured_load_points";
        let table_id = "kdt-verified-measurements";
        let row_total = KNOWLEDGE_TABLE_RXDB_CHUNK_ROWS * 2 + 1;
        let now = now_rfc3339();
        let live_path = compute_parquet_path(root, domain, table_key);

        let conn = open_runtime_db(root)?;
        conn.execute(
            "INSERT INTO knowledge_data_tables (
                 table_id, domain, table_key, source_system, title, description,
                 parquet_path, schema_hash, row_count, bytes, tags_json, archived_at,
                 created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'verified-import', 'Measurements', 'verified rows',
                       ?4, '', ?5, 0, '{}', NULL, ?6, ?6)",
            params![
                table_id,
                domain,
                table_key,
                live_path.to_string_lossy().into_owned(),
                row_total as i64,
                now
            ],
        )?;

        let rows = (0..row_total)
            .map(|index| {
                json!({
                    "source_id": "source-verified",
                    "source_row": index as i64,
                    "rpm": 8_000.0 + index as f64,
                    "evidence_eligible": true,
                })
            })
            .collect::<Vec<_>>();
        let df = super::super::parquet_io::rows_to_df(&rows)?;
        super::super::parquet_io::commit_parquet(&live_path, df)?;

        let docs = knowledge_tables_rxdb_documents(root)?;
        assert_eq!(docs.len(), 3);
        assert_eq!(docs[0]["id"], json!("table:kdt-verified-measurements"));
        assert_eq!(
            docs[1]["id"],
            json!("table:kdt-verified-measurements:chunk:0001")
        );
        assert_eq!(
            docs[2]["id"],
            json!("table:kdt-verified-measurements:chunk:0002")
        );

        let chunk_lengths = docs
            .iter()
            .map(|doc| doc["rows"].as_array().map(Vec::len).unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            chunk_lengths,
            vec![
                KNOWLEDGE_TABLE_RXDB_CHUNK_ROWS,
                KNOWLEDGE_TABLE_RXDB_CHUNK_ROWS,
                1
            ]
        );
        assert!(docs.iter().all(|doc| {
            doc["logical_table_id"] == json!("table:kdt-verified-measurements")
                && doc["row_count"] == json!(row_total)
                && doc["projected_row_count"] == json!(row_total)
                && doc["rows_complete"] == json!(true)
                && doc["chunk_count"] == json!(3)
                && doc["payload"]["rows"] == doc["rows"]
        }));

        let projected_source_rows = docs
            .iter()
            .flat_map(|doc| {
                doc["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|row| row["source_row"].as_i64())
            })
            .collect::<Vec<_>>();
        assert_eq!(projected_source_rows.len(), row_total);
        assert_eq!(projected_source_rows[0], 0);
        assert_eq!(projected_source_rows[row_total - 1], row_total as i64 - 1);
        Ok(())
    }

    #[test]
    fn measured_load_point_projection_adds_metric_semantics() {
        let rows = vec![json!({
            "source_id": "SRC-1",
            "source_file": "prop-tests.csv",
            "propeller_size": "9x5",
            "prop_diameter_in": 9,
            "prop_pitch_in": 5,
            "thrust_N": 12.5,
            "torque_Nm": -0.2,
            "radial_load_N": "-1.75",
            "tangential_equivalent_force_N": 1.75,
            "vibration_g_rms": "1.500000",
            "derivation_formula": "abs(torque_Nm) / (prop_diameter_mm / 2000)",
            "derivation_method": "direct experimental CSV row; radial_load_N=TORQUE_Nm/(prop_diameter_m/2) when torque present"
        })];

        let (rows, notes) = enrich_knowledge_table_rows("measured_load_points", rows);
        assert_eq!(notes.len(), 3);
        let row = rows[0].as_object().expect("projected object row");
        assert_eq!(row["row_id"].as_str(), Some("MLP-0001"));
        assert_eq!(row["prop_diameter_mm"].as_f64(), Some(228.6));
        assert_eq!(row["prop_pitch_mm"].as_f64(), Some(127.0));
        assert_eq!(row["force_N"].as_f64(), Some(12.5));
        assert_eq!(row["moment_Nm"].as_f64(), Some(-0.2));
        assert_eq!(row["tangential_equivalent_force_N"].as_f64(), Some(1.75));
        assert_eq!(row["source_row_ref"], Value::Null);
        assert_eq!(
            row["measurement_kind"].as_str(),
            Some("measured_with_derived_fields")
        );
        assert_eq!(row["is_derived"].as_bool(), Some(true));
        assert_eq!(row["load_case"].as_str(), Some("vibration_test"));
        let method = row["derivation_method"]
            .as_str()
            .expect("normalized derivation method");
        assert!(method.contains("tangential_equivalent_force_N=abs"));
        assert!(!method.contains("radial_load_N=TORQUE_Nm"));

        let labels: Vec<String> = knowledge_table_columns("measured_load_points", &rows)
            .iter()
            .filter_map(|column| column["label"].as_str().map(str::to_string))
            .collect();
        assert!(labels.contains(&"Diameter".to_string()));
        assert!(labels.contains(&"Pitch".to_string()));
        assert!(labels.contains(&"Torque".to_string()));
        assert!(labels.contains(&"Tangential equivalent force".to_string()));
    }

    #[test]
    fn measured_load_projection_keeps_unproven_legacy_radial_values_noncanonical() {
        let rows = vec![
            json!({
                "source_id": "SRC-legacy",
                "source_file": "legacy.csv",
                "radial_load_N": "0",
                "torque_Nm": 0,
                "prop_diameter_in": 0,
                "derivation_method": "radial_load_N=torque_Nm/radius",
            }),
            json!({
                "source_id": "SRC-zero",
                "radial_load_N": -4.5,
                "tangential_equivalent_force_N": 0,
                "bearing_radial_load_N": "0",
                "original_row_number": 0,
            }),
        ];

        let (rows, _) = enrich_knowledge_table_rows("measured_load_points", rows);
        let legacy = rows[0].as_object().expect("legacy projected row");
        assert_eq!(legacy["radial_load_N"].as_f64(), Some(0.0));
        assert_eq!(legacy["tangential_equivalent_force_N"], Value::Null);
        assert_eq!(legacy["bearing_radial_load_N"], Value::Null);
        assert_eq!(legacy["source_row_ref"], Value::Null);

        let zero = rows[1].as_object().expect("zero projected row");
        assert_eq!(zero["tangential_equivalent_force_N"].as_f64(), Some(0.0));
        assert_eq!(zero["bearing_radial_load_N"].as_f64(), Some(0.0));
        assert_eq!(zero["source_row_ref"].as_str(), Some("0"));
    }

    #[test]
    fn source_catalog_projection_exposes_verification_and_provenance_columns() {
        let columns = knowledge_table_columns("source_catalog", &[]);
        let keys: Vec<&str> = columns
            .iter()
            .filter_map(|column| column["key"].as_str())
            .collect();

        for required in [
            "canonical_url",
            "source_tier",
            "verification_status",
            "evidence_eligible",
            "checked_at",
            "http_status",
            "snapshot_hash",
            "provenance",
            "discovery_score",
            "evidence_score",
        ] {
            assert!(
                keys.contains(&required),
                "source catalog must expose {required}"
            );
        }
    }

    #[test]
    fn evidence_normalization_derives_eligibility_and_preserves_candidates() -> anyhow::Result<()> {
        let valid = json!({
            "source_id": "src-1",
            "canonical_url": "https://publisher.example/source",
            "source_type": "web",
            "verification_status": "verified",
            "transport_verified": true,
            "content_extracted": true,
            "actual_full_text_or_data": true,
            "evidence_relevance_score": 32,
            "metadata_only": false,
            "http_status": 200,
            "snapshot_hash": format!("sha256:{}", "a".repeat(64)),
            "source_tier": "primary",
            "provenance": "research-run-1",
            "evidence_eligible": false
        });
        let mut forged = valid.clone();
        forged["canonical_url"] = json!("not a url");
        forged["snapshot_hash"] = json!("sha256:test");
        forged["evidence_eligible"] = json!(true);

        let rows = normalize_evidence_rows("source_catalog", vec![valid, forged])?;
        assert_eq!(rows[0]["evidence_eligible"], json!(true));
        assert_eq!(rows[1]["evidence_eligible"], json!(false));
        assert!(rows[1]["evidence_rejection_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("invalid_canonical_url")
                && reason.contains("invalid_snapshot_hash")));

        for (field, value, reason) in [
            (
                "verification_status",
                json!("unverified"),
                "verification_not_verified",
            ),
            ("transport_verified", json!(false), "transport_not_verified"),
            ("content_extracted", json!(false), "content_not_extracted"),
            (
                "actual_full_text_or_data",
                json!(false),
                "full_content_not_verified",
            ),
            (
                "evidence_relevance_score",
                json!(7),
                "evidence_relevance_below_threshold",
            ),
            ("http_status", json!(500), "http_status_not_2xx"),
            ("http_status", json!(204), "http_status_no_content"),
            ("metadata_only", json!(true), "metadata_only"),
            (
                "source_type",
                json!("paper_metadata"),
                "metadata_or_aggregated_source_type",
            ),
            (
                "canonical_url",
                json!("https://doi.org/10.1234/metadata"),
                "metadata_canonical_url",
            ),
            (
                "evidence_rejection_reason",
                json!("operator said ok"),
                "evidence_rejection_reason_present",
            ),
            (
                "source_tier",
                json!("metadata"),
                "metadata_or_aggregated_source_tier",
            ),
            ("provenance", json!(null), "missing_trace_identifier"),
        ] {
            let mut candidate = rows[0].clone();
            candidate[field] = value;
            let normalized = normalize_evidence_rows("source_catalog", vec![candidate])?;
            assert_eq!(normalized[0]["evidence_eligible"], json!(false));
            assert!(
                normalized[0]["evidence_rejection_reason"]
                    .as_str()
                    .is_some_and(|reasons| reasons.contains(reason)),
                "{field}"
            );
        }
        Ok(())
    }

    #[test]
    fn formatted_flags_and_hashes_do_not_qualify_without_full_gate() -> anyhow::Result<()> {
        let row = json!({
            "source_id": "src-1",
            "canonical_url": "https://publisher.example/source",
            "source_type": "web",
            "verification_status": "verified",
            "transport_verified": true,
            "content_extracted": true,
            "http_status": 200,
            "snapshot_hash": format!("sha256:{}", "d".repeat(64)),
            "source_tier": "primary",
            "provenance": "research-run-1",
            "evidence_eligible": true
        });
        let normalized = normalize_evidence_rows("source_catalog", vec![row])?;
        assert_eq!(normalized[0]["evidence_eligible"], json!(false));
        let reason = normalized[0]["evidence_rejection_reason"]
            .as_str()
            .expect("normalizer records why formatted flags are insufficient");
        assert!(reason.contains("full_content_not_verified"));
        assert!(reason.contains("evidence_relevance_below_threshold"));
        Ok(())
    }

    #[test]
    fn claim_normalization_requires_claim_source_and_snapshot_trace_ids() -> anyhow::Result<()> {
        let mut row = json!({
            "claim_id": "claim-1",
            "source_id": "src-1",
            "trace_id": "trace-1",
            "canonical_url": "https://publisher.example/source",
            "source_type": "web",
            "verification_status": "verified",
            "transport_verified": true,
            "content_extracted": true,
            "actual_full_text_or_data": true,
            "evidence_relevance_score": 32,
            "metadata_only": false,
            "http_status": 200,
            "snapshot_id": "snapshot:run-1:source-1",
            "snapshot_hash": format!("sha256:{}", "b".repeat(64)),
            "source_tier": "authoritative",
            "evidence_eligible": true
        });
        let eligible = normalize_evidence_rows("evidence_points", vec![row.clone()])?;
        assert_eq!(eligible[0]["evidence_eligible"], json!(true));

        row["claim_id"] = Value::Null;
        let rejected = normalize_evidence_rows("evidence_points", vec![row])?;
        assert_eq!(rejected[0]["evidence_eligible"], json!(false));
        assert!(rejected[0]["evidence_rejection_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("missing_claim_or_evidence_id")));

        let mut missing_source = eligible[0].clone();
        missing_source["source_id"] = Value::Null;
        let rejected = normalize_evidence_rows("evidence_points", vec![missing_source])?;
        assert!(rejected[0]["evidence_rejection_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("missing_source_id")));

        let mut missing_snapshot = eligible[0].clone();
        missing_snapshot["snapshot_id"] = Value::Null;
        let rejected = normalize_evidence_rows("evidence_points", vec![missing_snapshot])?;
        assert!(rejected[0]["evidence_rejection_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("missing_snapshot_id")));
        Ok(())
    }

    #[test]
    fn non_evidence_table_rows_are_unchanged() -> anyhow::Result<()> {
        let rows = vec![json!({"evidence_eligible": true, "value": 1})];
        assert_eq!(normalize_evidence_rows("measurements", rows.clone())?, rows);
        Ok(())
    }

    /// An archived catalog row must not be projected.
    #[test]
    fn knowledge_tables_projection_skips_archived() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let conn = open_runtime_db(root)?;
        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO knowledge_data_tables (
                 table_id, domain, table_key, source_system, title, description,
                 parquet_path, schema_hash, row_count, bytes, tags_json, archived_at,
                 created_at, updated_at
             ) VALUES ('kdt-archived', 'd', 'k', 'agent', 'Archived', '',
                       '/nope.parquet', '', 0, 0, '{}', ?1, ?1, ?1)",
            params![now],
        )?;
        let docs = knowledge_tables_rxdb_documents(root)?;
        assert!(docs.is_empty(), "archived tables are not projected");
        Ok(())
    }

    #[test]
    fn knowledge_tables_projection_source_stamp_tracks_live_parquet_file() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let conn = open_runtime_db(root)?;
        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO knowledge_data_tables (
                 table_id, domain, table_key, source_system, title, description,
                 parquet_path, schema_hash, row_count, bytes, tags_json, archived_at,
                 created_at, updated_at
             ) VALUES ('kdt-stamp', 'stamp_domain', 'stamp_key', 'agent',
                       'Stamp Table', 'tracks parquet metadata',
                       '/stale.parquet', '', 0, 0, '{}', NULL, ?1, ?1)",
            params![now],
        )?;

        let first = knowledge_tables_projection_source_stamp(root)?;
        let second = knowledge_tables_projection_source_stamp(root)?;
        assert_eq!(first, second);

        let live_path = compute_parquet_path(root, "stamp_domain", "stamp_key");
        let rows = vec![
            json!({"id": "row-1", "value": "first"}),
            json!({"id": "row-2", "value": "second and longer"}),
            json!({"id": "row-3", "value": "third and longer still"}),
        ];
        let df = super::super::parquet_io::rows_to_df(&rows)?;
        super::super::parquet_io::commit_parquet(&live_path, df)?;

        let changed = knowledge_tables_projection_source_stamp(root)?;
        assert_ne!(first, changed);
        assert_eq!(changed, knowledge_tables_projection_source_stamp(root)?);
        Ok(())
    }
}
