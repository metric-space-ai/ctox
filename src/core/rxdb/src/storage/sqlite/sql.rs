//! SQL helpers for the SQLite storage backend.

#[cfg(test)]
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::rx_error::{new_rx_error, RxResult};
use crate::types::{BulkWriteRow, FilledMangoQuery, RxJsonSchema, RxQueryPlan};

use super::metrics::timed_sqlite_statement;
use super::types::sqlite_error;

const CHANGED_TABLES_TABLE: &str = "__rxdb_changed_tables";
const DOCUMENTS_BY_ID_BATCH_SIZE: usize = 500;

#[cfg(test)]
thread_local! {
    static SQLITE_JSON_DOCUMENT_DECODE_COUNT: Cell<usize> = const { Cell::new(0) };
}
#[cfg(test)]
static SQLITE_DOCUMENT_BY_ID_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static SQLITE_DOCUMENTS_BY_IDS_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub fn reset_sqlite_json_document_decode_count() {
    SQLITE_JSON_DOCUMENT_DECODE_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub fn sqlite_json_document_decode_count() -> usize {
    SQLITE_JSON_DOCUMENT_DECODE_COUNT.with(Cell::get)
}

#[cfg(test)]
pub fn reset_sqlite_document_lookup_counts() {
    SQLITE_DOCUMENT_BY_ID_CALL_COUNT.store(0, Ordering::SeqCst);
    SQLITE_DOCUMENTS_BY_IDS_CALL_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(test)]
pub fn sqlite_document_by_id_call_count() -> usize {
    SQLITE_DOCUMENT_BY_ID_CALL_COUNT.load(Ordering::SeqCst)
}

#[cfg(test)]
pub fn sqlite_documents_by_ids_call_count() -> usize {
    SQLITE_DOCUMENTS_BY_IDS_CALL_COUNT.load(Ordering::SeqCst)
}

pub fn table_name(database_name: &str, collection_name: &str, schema_version: i32) -> String {
    format!("{database_name}__{collection_name}__v{schema_version}")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Owner DDL deliberately creates no per-table sequence. P3-05 installs the
/// global `_changes` triggers in this same transaction before it commits.
pub fn ensure_owned_collection_table(conn: &Connection, table: &str) -> RxResult<()> {
    // DDL belongs exclusively to P3-06; never create a second current table.
    let valid: bool = conn.query_row(
        "SELECT COUNT(*)=5 FROM pragma_table_info(?1) WHERE name IN ('id','rev','schema_ver','data','deleted')",
        [table], |r| r.get(0)).map_err(sqlite_error)?;
    if !valid || !table.starts_with("c_") {
        return Err(super::types::owner_job_error(
            "canonical collection table required",
        ));
    }
    Ok(())
}

/// One bounded startup transfer. Renaming the old source fences legacy writers;
/// a durable receipt counts every row, including NULL-body tombstones. A
/// pre-populated target without our receipt is an ambiguous double activation.
pub fn migrate_legacy_page(
    conn: &Connection,
    source: &str,
    target: &str,
    version: i32,
) -> RxResult<bool> {
    let active: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM _collections WHERE table_name=?1 AND schema_ver=?2 AND state='active')",params![target,version],|r|r.get(0)).map_err(sqlite_error)?;
    if !active {
        return Err(super::types::owner_job_error(
            "legacy import generation no longer active",
        ));
    }
    conn.execute_batch("CREATE TABLE IF NOT EXISTS _rxdb_imports(source TEXT PRIMARY KEY,target TEXT NOT NULL UNIQUE,expected INTEGER NOT NULL,copied INTEGER NOT NULL DEFAULT 0,cursor TEXT,state TEXT NOT NULL CHECK(state IN ('copying','complete')))").map_err(sqlite_error)?;
    let receipt: Option<(String, i64, i64, Option<String>, String)> = conn
        .query_row(
            "SELECT target,expected,copied,cursor,state FROM _rxdb_imports WHERE source=?1",
            [source],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(sqlite_error)?;
    let archive = format!("_rxdb_import_{target}");
    let exists = |name: &str| -> RxResult<bool> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [name],
            |r| r.get(0),
        )
        .map_err(sqlite_error)
    };
    let (expected, copied, cursor) = match receipt {
        Some((bound, expected, copied, cursor, state)) => {
            if bound != target || exists(source)? {
                return Err(super::types::owner_job_error(
                    "legacy import binding changed or source recreated",
                ));
            }
            if state == "complete" {
                return Ok(true);
            }
            (expected, copied, cursor)
        }
        None => {
            if !exists(source)? {
                return Ok(true);
            }
            let occupied: bool = conn
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {})", quote_identifier(target)),
                    [],
                    |r| r.get(0),
                )
                .map_err(sqlite_error)?;
            if occupied || exists(&archive)? {
                return Err(super::types::owner_job_error(
                    "double collection activation refused",
                ));
            }
            let expected: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {}", quote_identifier(source)),
                    [],
                    |r| r.get(0),
                )
                .map_err(sqlite_error)?;
            conn.execute_batch(&format!(
                "ALTER TABLE {} RENAME TO {}",
                quote_identifier(source),
                quote_identifier(&archive)
            ))
            .map_err(sqlite_error)?;
            conn.execute("INSERT INTO _rxdb_imports(source,target,expected,state) VALUES(?1,?2,?3,'copying')",params![source,target,expected]).map_err(sqlite_error)?;
            (expected, 0, None)
        }
    };
    let mut stmt=conn.prepare(&format!("SELECT id,revision,deleted,data FROM {} WHERE (?1 IS NULL OR id>?1) ORDER BY id LIMIT 500",quote_identifier(&archive))).map_err(sqlite_error)?;
    let mut cursor_rows = stmt.query([cursor.as_deref()]).map_err(sqlite_error)?;
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    let mut complete = false;
    // A row cap alone can hold the writer for tens of MiB of lead documents.
    // Read incrementally; do not materialize 500 large bodies before yielding.
    while rows.len() < 500 && bytes < 1024 * 1024 {
        let Some(row) = cursor_rows.next().map_err(sqlite_error)? else {
            complete = true;
            break;
        };
        let id: String = row.get(0).map_err(sqlite_error)?;
        let stored_rev: Option<String> = row.get(1).map_err(sqlite_error)?;
        let deleted: i64 = row.get(2).map_err(sqlite_error)?;
        let raw: String = row.get(3).map_err(sqlite_error)?;
        bytes = bytes.saturating_add(raw.len());
        let mut body: Value = serde_json::from_str(&raw)
            .map_err(|_| super::types::owner_job_error("invalid legacy document JSON"))?;
        let valid = |rev: &str| {
            rev.split_once('-').is_some_and(|(height, hash)| {
                height.parse::<u64>().is_ok() && !hash.is_empty() && !hash.contains('-')
            })
        };
        let stored = stored_rev.as_deref().unwrap_or_default();
        let body_rev = body.get("_rev").and_then(Value::as_str).unwrap_or_default();
        let rev = if valid(stored) {
            stored.to_owned()
        } else if valid(body_rev) {
            body_rev.to_owned()
        } else {
            let identity = serde_json::to_vec(&(source, &id, stored, &raw))
                .map_err(|_| super::types::owner_job_error("legacy revision identity encoding"))?;
            format!("1-{:x}", Sha256::digest(identity))
        };
        let deleted =
            i64::from(deleted != 0 || body.get("_deleted").and_then(Value::as_bool) == Some(true));
        let object = body
            .as_object_mut()
            .ok_or_else(|| super::types::owner_job_error("legacy document is not an object"))?;
        object.insert("_rev".into(), Value::String(rev.clone()));
        object.insert("_deleted".into(), Value::Bool(deleted != 0));
        rows.push((id, rev, deleted, body.to_string()));
    }
    drop(cursor_rows);
    drop(stmt);
    for (id, rev, deleted, data) in &rows {
        conn.execute(
            &format!(
                "INSERT INTO {}(id,rev,schema_ver,deleted,data) VALUES(?1,?2,?3,?4,?5)",
                quote_identifier(target)
            ),
            params![
                id,
                rev,
                version,
                deleted,
                if *deleted != 0 { None } else { Some(data) }
            ],
        )
        .map_err(sqlite_error)?;
    }
    let total = copied + rows.len() as i64;
    if complete {
        let source_count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", quote_identifier(&archive)),
                [],
                |r| r.get(0),
            )
            .map_err(sqlite_error)?;
        if total != expected || source_count != expected {
            return Err(super::types::owner_job_error(
                "legacy import row count changed",
            ));
        }
        conn.execute_batch(&format!("DROP TABLE {}", quote_identifier(&archive)))
            .map_err(sqlite_error)?;
        conn.execute(
            "UPDATE _rxdb_imports SET copied=?2,state='complete' WHERE source=?1",
            params![source, total],
        )
        .map_err(sqlite_error)?;
        Ok(true)
    } else {
        conn.execute(
            "UPDATE _rxdb_imports SET copied=?2,cursor=?3 WHERE source=?1",
            params![source, total, rows.last().map(|r| r.0.as_str())],
        )
        .map_err(sqlite_error)?;
        Ok(false)
    }
}

/// Read-only envelope projection, not another physical table or writer. The
/// authoritative revision/deletion columns override any old body metadata.
fn read_table(table: &str) -> String {
    if !table.starts_with("c_") {
        return quote_identifier(table);
    }
    let name = quote_identifier(table);
    let literal = quote_sql_literal(table);
    format!("(SELECT t.id,t.rev AS revision,t.deleted,COALESCE(json_extract(t.data,'$._meta.lwt'),0) AS lastWriteTime,json_set(COALESCE(t.data,'{{}}'),'$.'||json_quote(COALESCE((SELECT COALESCE(json_extract(schema_json,'$.primaryKey.key'),json_extract(schema_json,'$.primaryKey')) FROM _collections WHERE table_name={literal}),'id')),t.id,'$._rev',t.rev,'$._deleted',json(CASE t.deleted WHEN 1 THEN 'true' ELSE 'false' END),'$._attachments',json(COALESCE(json_extract(t.data,'$._attachments'),'{{}}')),'$._meta.lwt',COALESCE(json_extract(t.data,'$._meta.lwt'),0)) AS data FROM {name} t)")
}

pub fn ensure_collection_table(conn: &Connection, table: &str) -> RxResult<()> {
    let quoted = quote_identifier(table);
    let lwt_index = quote_identifier(&format!("{table}_lwt_id_idx"));
    let deleted_index = quote_identifier(&format!("{table}_deleted_lwt_id_idx"));
    let changed_tables = quote_identifier(CHANGED_TABLES_TABLE);
    let table_literal = quote_sql_literal(table);
    let insert_trigger = quote_identifier(&format!("{table}__rxdb_changed_insert"));
    let update_trigger = quote_identifier(&format!("{table}__rxdb_changed_update"));
    let delete_trigger = quote_identifier(&format!("{table}__rxdb_changed_delete"));
    let _statement_timer = timed_sqlite_statement();
    conn.execute_batch(&format!(
        r#"
        CREATE TABLE IF NOT EXISTS {quoted}(
            id TEXT NOT NULL PRIMARY KEY UNIQUE,
            revision TEXT,
            deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
            lastWriteTime REAL NOT NULL,
            data TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS {lwt_index}
            ON {quoted}(lastWriteTime, id);
        CREATE INDEX IF NOT EXISTS {deleted_index}
            ON {quoted}(deleted, lastWriteTime, id);

        CREATE TABLE IF NOT EXISTS {changed_tables}(
            table_name TEXT NOT NULL PRIMARY KEY,
            changed_at INTEGER NOT NULL
        );

        CREATE TRIGGER IF NOT EXISTS {insert_trigger}
            AFTER INSERT ON {quoted}
        BEGIN
            INSERT INTO {changed_tables}(table_name, changed_at)
            VALUES ({table_literal}, 1)
            ON CONFLICT(table_name) DO UPDATE SET changed_at = changed_at + 1;
        END;

        CREATE TRIGGER IF NOT EXISTS {update_trigger}
            AFTER UPDATE ON {quoted}
        BEGIN
            INSERT INTO {changed_tables}(table_name, changed_at)
            VALUES ({table_literal}, 1)
            ON CONFLICT(table_name) DO UPDATE SET changed_at = changed_at + 1;
        END;

        CREATE TRIGGER IF NOT EXISTS {delete_trigger}
            AFTER DELETE ON {quoted}
        BEGIN
            INSERT INTO {changed_tables}(table_name, changed_at)
            VALUES ({table_literal}, 1)
            ON CONFLICT(table_name) DO UPDATE SET changed_at = changed_at + 1;
        END;
        "#
    ))
    .map_err(sqlite_error)
}

pub fn ensure_collection_schema_indexes(
    conn: &Connection,
    table: &str,
    schema: &RxJsonSchema,
    primary_path: &str,
) -> RxResult<()> {
    let quoted = quote_identifier(table);
    let mut created = HashSet::new();
    for index in &schema.indexes {
        ensure_schema_index(conn, &quoted, table, primary_path, index, &mut created)?;
    }
    Ok(())
}

fn ensure_schema_index(
    conn: &Connection,
    quoted_table: &str,
    table: &str,
    primary_path: &str,
    fields: &[String],
    created: &mut HashSet<String>,
) -> RxResult<()> {
    if fields.is_empty() {
        return Ok(());
    }
    let mut expressions = fields
        .iter()
        .map(|field| field_sql_expression(primary_path, field))
        .collect::<Vec<_>>();
    let primary_expression = quote_identifier("id");
    if !fields
        .iter()
        .any(|field| sqlite_backing_column(primary_path, field) == Some("id"))
    {
        expressions.push(primary_expression);
    }
    let index_key = fields.join("__");
    if !created.insert(index_key.clone()) {
        return Ok(());
    }
    let index_name = quote_identifier(&format!(
        "{}_json_{}_idx",
        table,
        sanitize_index_name(&index_key)
    ));
    let _statement_timer = timed_sqlite_statement();
    conn.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS {index_name}
         ON {quoted_table}({});",
        expressions.join(", ")
    ))
    .map_err(sqlite_error)?;
    Ok(())
}

fn quote_sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sanitize_index_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

#[derive(Debug, Clone)]
pub struct CompiledSqliteQuery {
    pub sql: String,
    pub params: Vec<SqlValue>,
}

pub fn compile_query_sql(
    table: &str,
    primary_path: &str,
    query: &FilledMangoQuery,
) -> Option<CompiledSqliteQuery> {
    let (where_sql, mut params) = compile_selector_sql(primary_path, &query.selector)?;
    let order_sql = compile_order_sql(primary_path, query)?;
    let mut sql = format!("SELECT data FROM {}", read_table(table));
    if !where_sql.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_sql);
    }
    if !order_sql.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_sql);
    }
    if let Some(limit) = query.limit {
        sql.push_str(" LIMIT ?");
        params.push(sql_integer_param(limit)?);
        if let Some(skip) = query.skip.filter(|skip| *skip > 0) {
            sql.push_str(" OFFSET ?");
            params.push(sql_integer_param(skip)?);
        }
    } else if let Some(skip) = query.skip.filter(|skip| *skip > 0) {
        sql.push_str(" LIMIT -1 OFFSET ?");
        params.push(sql_integer_param(skip)?);
    }
    Some(CompiledSqliteQuery { sql, params })
}

pub fn compile_count_sql(
    table: &str,
    primary_path: &str,
    query: &FilledMangoQuery,
) -> Option<CompiledSqliteQuery> {
    let (where_sql, mut params) = compile_selector_sql(primary_path, &query.selector)?;
    let has_window = query.limit.is_some() || query.skip.unwrap_or(0) > 0;
    let sql = if has_window {
        let order_sql = compile_order_sql(primary_path, query)?;
        let mut inner = format!("SELECT 1 FROM {}", read_table(table));
        if !where_sql.is_empty() {
            inner.push_str(" WHERE ");
            inner.push_str(&where_sql);
        }
        if !order_sql.is_empty() {
            inner.push_str(" ORDER BY ");
            inner.push_str(&order_sql);
        }
        if let Some(limit) = query.limit {
            inner.push_str(" LIMIT ?");
            params.push(sql_integer_param(limit)?);
        } else {
            inner.push_str(" LIMIT -1");
        }
        if let Some(skip) = query.skip.filter(|skip| *skip > 0) {
            inner.push_str(" OFFSET ?");
            params.push(sql_integer_param(skip)?);
        }
        format!("SELECT COUNT(*) FROM ({inner})")
    } else {
        let mut sql = format!("SELECT COUNT(*) FROM {}", read_table(table));
        if !where_sql.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
        }
        sql
    };
    Some(CompiledSqliteQuery { sql, params })
}

pub fn compile_query_plan_candidate_sql(
    table: &str,
    primary_path: &str,
    query_plan: &RxQueryPlan,
) -> Option<CompiledSqliteQuery> {
    if query_plan.index.is_empty()
        || query_plan.index.len() != query_plan.start_keys.len()
        || query_plan.index.len() != query_plan.end_keys.len()
    {
        return None;
    }

    let mut clauses = Vec::new();
    let mut params = Vec::new();
    for (idx, field) in query_plan.index.iter().enumerate() {
        let expression = field_sql_expression(primary_path, field);
        let start = &query_plan.start_keys[idx];
        let end = &query_plan.end_keys[idx];
        let has_start = !is_index_min(start) && !is_index_max(start);
        let has_end = !is_index_min(end) && !is_index_max(end);
        match (has_start, has_end) {
            (true, true) if start == end => {
                clauses.push(format!("{expression} = ?"));
                params.push(json_value_to_sql_param(start)?);
            }
            (true, true) => {
                clauses.push(format!(
                    "{expression} {} ?",
                    if query_plan.inclusive_start {
                        ">="
                    } else {
                        ">"
                    }
                ));
                params.push(json_value_to_sql_param(start)?);
                clauses.push(format!(
                    "{expression} {} ?",
                    if query_plan.inclusive_end { "<=" } else { "<" }
                ));
                params.push(json_value_to_sql_param(end)?);
            }
            (true, false) => {
                clauses.push(format!(
                    "{expression} {} ?",
                    if query_plan.inclusive_start {
                        ">="
                    } else {
                        ">"
                    }
                ));
                params.push(json_value_to_sql_param(start)?);
            }
            (false, true) => {
                clauses.push(format!(
                    "{expression} {} ?",
                    if query_plan.inclusive_end { "<=" } else { "<" }
                ));
                params.push(json_value_to_sql_param(end)?);
            }
            (false, false) => {}
        }
    }
    if clauses.is_empty() {
        return None;
    }

    let order_sql = query_plan
        .index
        .iter()
        .map(|field| format!("{} ASC", field_sql_expression(primary_path, field)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "SELECT data FROM {} WHERE {}",
        read_table(table),
        clauses.join(" AND ")
    );
    if !order_sql.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_sql);
    }
    Some(CompiledSqliteQuery { sql, params })
}

fn decode_document_json(data: &str) -> RxResult<Value> {
    #[cfg(test)]
    SQLITE_JSON_DOCUMENT_DECODE_COUNT.with(|count| count.set(count.get() + 1));

    serde_json::from_str(data).map_err(|err| {
        new_rx_error(
            "SQLITE_JSON",
            Some(serde_json::json!({ "message": err.to_string() })),
        )
    })
}

fn decode_document_row(
    revision: Option<String>,
    deleted: bool,
    last_write_time: f64,
    data: &str,
) -> RxResult<Value> {
    let mut document = decode_document_json(data)?;
    let object = document.as_object_mut().ok_or_else(|| {
        new_rx_error(
            "SQLITE_JSON",
            Some(serde_json::json!({ "message": "stored document is not an object" })),
        )
    })?;

    if let Some(revision) = revision.filter(|revision| !revision.is_empty()) {
        object.insert("_rev".to_string(), Value::String(revision));
    }
    object.insert("_deleted".to_string(), Value::Bool(deleted));
    object
        .entry("_attachments".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let meta = object
        .entry("_meta".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let meta = meta.as_object_mut().ok_or_else(|| {
        new_rx_error(
            "SQLITE_JSON",
            Some(serde_json::json!({ "message": "stored document _meta is not an object" })),
        )
    })?;
    meta.insert("lwt".to_string(), Value::from(last_write_time));
    Ok(document)
}

pub fn query_documents_with_compiled_sql(
    conn: &Connection,
    compiled: &CompiledSqliteQuery,
) -> RxResult<Vec<Value>> {
    let mut documents = Vec::new();
    for_each_document_with_compiled_sql(conn, compiled, |doc| {
        documents.push(doc);
        Ok(true)
    })?;
    Ok(documents)
}

pub fn for_each_document_with_compiled_sql<F>(
    conn: &Connection,
    compiled: &CompiledSqliteQuery,
    mut visit: F,
) -> RxResult<()>
where
    F: FnMut(Value) -> RxResult<bool>,
{
    let _statement_timer = timed_sqlite_statement();
    let mut statement = conn.prepare(&compiled.sql).map_err(sqlite_error)?;
    let rows = statement
        .query_map(params_from_iter(compiled.params.iter()), |row| {
            row.get::<_, String>(0)
        })
        .map_err(sqlite_error)?;
    for row in rows {
        let data = row.map_err(sqlite_error)?;
        let doc = decode_document_json(&data)?;
        if !visit(doc)? {
            break;
        }
    }
    Ok(())
}

pub fn count_with_compiled_sql(conn: &Connection, compiled: &CompiledSqliteQuery) -> RxResult<u64> {
    let _statement_timer = timed_sqlite_statement();
    let count: i64 = conn
        .query_row(
            &compiled.sql,
            params_from_iter(compiled.params.iter()),
            |row| row.get(0),
        )
        .map_err(sqlite_error)?;
    Ok(u64::try_from(count).unwrap_or(0))
}

fn compile_selector_sql(primary_path: &str, selector: &Value) -> Option<(String, Vec<SqlValue>)> {
    let selector = selector.as_object()?;
    if selector.is_empty() {
        return Some((String::new(), Vec::new()));
    }
    let mut clauses = Vec::new();
    let mut params = Vec::new();
    for (field, matcher) in selector {
        if field.starts_with('$') {
            return None;
        }
        let expression = field_sql_expression(primary_path, field);
        if let Some(operators) = matcher.as_object() {
            if operators.is_empty() {
                return None;
            }
            for (operator, value) in operators {
                match operator.as_str() {
                    "$eq" => {
                        clauses.push(format!("{expression} = ?"));
                        params.push(json_value_to_sql_param(value)?);
                    }
                    "$gt" => {
                        clauses.push(format!("{expression} > ?"));
                        params.push(json_value_to_sql_param(value)?);
                    }
                    "$gte" => {
                        clauses.push(format!("{expression} >= ?"));
                        params.push(json_value_to_sql_param(value)?);
                    }
                    "$lt" => {
                        clauses.push(format!("{expression} < ?"));
                        params.push(json_value_to_sql_param(value)?);
                    }
                    "$lte" => {
                        clauses.push(format!("{expression} <= ?"));
                        params.push(json_value_to_sql_param(value)?);
                    }
                    "$in" => {
                        let values = value.as_array()?;
                        if values.is_empty() {
                            clauses.push("0 = 1".to_string());
                            continue;
                        }
                        let placeholders = std::iter::repeat_n("?", values.len())
                            .collect::<Vec<_>>()
                            .join(", ");
                        clauses.push(format!("{expression} IN ({placeholders})"));
                        for value in values {
                            params.push(json_value_to_sql_param(value)?);
                        }
                    }
                    _ => return None,
                }
            }
        } else {
            clauses.push(format!("{expression} = ?"));
            params.push(json_value_to_sql_param(matcher)?);
        }
    }
    Some((clauses.join(" AND "), params))
}

fn compile_order_sql(primary_path: &str, query: &FilledMangoQuery) -> Option<String> {
    let mut parts = Vec::new();
    for sort_block in &query.sort {
        let (field, direction) = sort_block.iter().next()?;
        let direction = match direction.as_str() {
            "asc" | "ASC" => "ASC",
            "desc" | "DESC" => "DESC",
            _ => return None,
        };
        parts.push(format!(
            "{} {}",
            field_sql_expression(primary_path, field),
            direction
        ));
    }
    Some(parts.join(", "))
}

fn field_sql_expression(primary_path: &str, field: &str) -> String {
    if let Some(column) = sqlite_backing_column(primary_path, field) {
        return quote_identifier(column);
    }
    format!(
        "json_extract(data, {})",
        quote_sql_literal(&json_path_for_field(field))
    )
}

fn sqlite_backing_column<'a>(primary_path: &str, field: &'a str) -> Option<&'a str> {
    if field == primary_path || field == "id" {
        return Some("id");
    }
    match field {
        "_deleted" | "deleted" => Some("deleted"),
        "_meta.lwt" | "lastWriteTime" => Some("lastWriteTime"),
        _ => None,
    }
}

fn json_path_for_field(field: &str) -> String {
    let mut path = "$".to_string();
    for part in field.split('.') {
        if part
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            path.push('.');
            path.push_str(part);
        } else {
            path.push_str(".\"");
            path.push_str(&part.replace('"', "\\\""));
            path.push('"');
        }
    }
    path
}

fn json_value_to_sql_param(value: &Value) -> Option<SqlValue> {
    match value {
        Value::Null => Some(SqlValue::Null),
        Value::Bool(value) => Some(SqlValue::Integer(i64::from(*value))),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Some(SqlValue::Integer(value))
            } else if let Some(value) = number.as_u64() {
                i64::try_from(value)
                    .map(SqlValue::Integer)
                    .ok()
                    .or_else(|| number.as_f64().map(SqlValue::Real))
            } else {
                number.as_f64().map(SqlValue::Real)
            }
        }
        Value::String(value) => Some(SqlValue::Text(value.clone())),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn sql_integer_param(value: u64) -> Option<SqlValue> {
    i64::try_from(value).ok().map(SqlValue::Integer)
}

fn is_index_min(value: &Value) -> bool {
    value.as_i64() == Some(-9_007_199_254_740_991)
}

fn is_index_max(value: &Value) -> bool {
    value.as_str() == Some("\u{ffff}")
}

fn document_columns(
    document: &Value,
    primary_path: &str,
) -> RxResult<(String, String, bool, f64, String)> {
    let id = document_id(document, primary_path)?;
    let revision = document
        .get("_rev")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let deleted = document
        .get("_deleted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let lwt = last_write_time(document);
    let data = serde_json::to_string(document).map_err(|err| {
        new_rx_error(
            "SQLITE_JSON",
            Some(serde_json::json!({ "message": err.to_string() })),
        )
    })?;
    Ok((id, revision, deleted, lwt, data))
}

pub fn insert_document(
    conn: &Connection,
    table: &str,
    primary_path: &str,
    document: &Value,
) -> RxResult<()> {
    let (id, revision, deleted, lwt, data) = document_columns(document, primary_path)?;
    if table.starts_with("c_") {
        conn.execute(&format!("INSERT INTO {}(id,rev,schema_ver,deleted,data) SELECT ?1,?2,schema_ver,?3,?4 FROM _collections WHERE table_name=?5 AND state='active'", quote_identifier(table)), params![id,revision,i64::from(deleted),if deleted {None} else {Some(data)},table]).map_err(sqlite_error).and_then(|n| if n==1 {Ok(())} else {Err(super::types::owner_job_error("collection generation not active"))})?;
        return Ok(());
    }
    let _statement_timer = timed_sqlite_statement();
    conn.execute(
        &format!(
            "INSERT INTO {} (id, revision, deleted, lastWriteTime, data) VALUES (?, ?, ?, ?, ?)",
            quote_identifier(table)
        ),
        params![id, revision, if deleted { 1 } else { 0 }, lwt, data],
    )
    .map_err(sqlite_error)?;
    Ok(())
}

pub fn update_document(
    conn: &Connection,
    table: &str,
    primary_path: &str,
    row: &BulkWriteRow,
) -> RxResult<()> {
    let document = &row.document;
    let (id, revision, deleted, lwt, data) = document_columns(document, primary_path)?;
    if table.starts_with("c_") {
        conn.execute(&format!("UPDATE {} SET rev=?1,deleted=?2,data=?3 WHERE id=?4 AND EXISTS(SELECT 1 FROM _collections WHERE table_name=?5 AND state='active')",quote_identifier(table)),params![revision,i64::from(deleted),if deleted {None} else {Some(data)},id,table]).map_err(sqlite_error).and_then(|n| if n==1 {Ok(())} else {Err(super::types::owner_job_error("collection generation not active"))})?;
        return Ok(());
    }
    let _statement_timer = timed_sqlite_statement();
    conn.execute(
        &format!(
            "UPDATE {} SET revision = ?, deleted = ?, lastWriteTime = ?, data = ? WHERE id = ?",
            quote_identifier(table)
        ),
        params![revision, if deleted { 1 } else { 0 }, lwt, data, id],
    )
    .map_err(sqlite_error)?;
    Ok(())
}

/// Walks every row of the table without first materializing the full Vec.
/// The visitor returns `Ok(true)` to continue or `Ok(false)` to stop early.
/// V1.5 query streaming relies on this for bounded-memory reads on large
/// collections.
pub fn for_each_document<F>(conn: &Connection, table: &str, mut visit: F) -> RxResult<()>
where
    F: FnMut(Value) -> RxResult<bool>,
{
    let _statement_timer = timed_sqlite_statement();
    let mut stmt = conn
        .prepare(&format!("SELECT data FROM {}", read_table(table)))
        .map_err(sqlite_error)?;
    let mut rows = stmt.query([]).map_err(sqlite_error)?;
    while let Some(row) = rows.next().map_err(sqlite_error)? {
        let data: String = row.get(0).map_err(sqlite_error)?;
        let doc = decode_document_json(&data)?;
        if !visit(doc)? {
            break;
        }
    }
    Ok(())
}

pub fn document_by_id(conn: &Connection, table: &str, id: &str) -> RxResult<Option<Value>> {
    #[cfg(test)]
    SQLITE_DOCUMENT_BY_ID_CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    let _statement_timer = timed_sqlite_statement();
    let row: Option<(Option<String>, i64, f64, String)> = conn
        .query_row(
            &format!(
                "SELECT revision, deleted, lastWriteTime, data FROM {} WHERE id = ?",
                read_table(table)
            ),
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(sqlite_error)?;
    row.map(|(revision, deleted, last_write_time, data)| {
        decode_document_row(revision, deleted != 0, last_write_time, &data)
    })
    .transpose()
}

pub fn documents_by_ids(
    conn: &Connection,
    table: &str,
    ids: &[String],
    with_deleted: bool,
) -> RxResult<Vec<Value>> {
    #[cfg(test)]
    SQLITE_DOCUMENTS_BY_IDS_CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut by_id: HashMap<String, Value> = HashMap::with_capacity(ids.len());
    let quoted_table = read_table(table);
    for chunk in ids.chunks(DOCUMENTS_BY_ID_BATCH_SIZE) {
        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = if with_deleted {
            format!(
                "SELECT id, revision, deleted, lastWriteTime, data FROM {quoted_table} WHERE id IN ({placeholders})"
            )
        } else {
            format!(
                "SELECT id, revision, deleted, lastWriteTime, data FROM {quoted_table} WHERE id IN ({placeholders}) AND deleted = 0"
            )
        };
        let _statement_timer = timed_sqlite_statement();
        let mut statement = conn.prepare(&sql).map_err(sqlite_error)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(String::as_str)), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(sqlite_error)?;
        for row in rows {
            let (id, revision, deleted, last_write_time, data) = row.map_err(sqlite_error)?;
            let doc = decode_document_row(revision, deleted != 0, last_write_time, &data)?;
            by_id.insert(id, doc);
        }
    }

    Ok(ids.iter().filter_map(|id| by_id.get(id).cloned()).collect())
}

pub fn drop_table(conn: &Connection, table: &str) -> RxResult<()> {
    let _statement_timer = timed_sqlite_statement();
    conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", quote_identifier(table)))
        .map_err(sqlite_error)
}

fn document_id(document: &Value, primary_path: &str) -> RxResult<String> {
    document
        .get(primary_path)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| {
            new_rx_error(
                "SQLITE_PRIMARY",
                Some(serde_json::json!({ "primaryPath": primary_path, "document": document })),
            )
        })
}

pub fn last_write_time(document: &Value) -> f64 {
    document
        .get("_meta")
        .and_then(|meta| meta.get("lwt"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::decode_document_row;
    use serde_json::json;

    #[test]
    fn native_sqlite_row_columns_restore_rxdb_envelope() {
        let document = decode_document_row(
            Some("3-native".to_string()),
            false,
            42.5,
            &json!({ "id": "cmd-native", "status": "pending_sync" }).to_string(),
        )
        .expect("decode native SQLite row");

        assert_eq!(
            document.get("_rev").and_then(|value| value.as_str()),
            Some("3-native")
        );
        assert_eq!(
            document
                .pointer("/_meta/lwt")
                .and_then(|value| value.as_f64()),
            Some(42.5)
        );
        assert_eq!(
            document.get("_deleted").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert!(document
            .get("_attachments")
            .is_some_and(|value| value.is_object()));
    }
}
