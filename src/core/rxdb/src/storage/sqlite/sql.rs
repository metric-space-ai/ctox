//! SQL helpers for the SQLite storage backend.

#[cfg(test)]
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde_json::Value;

use crate::rx_error::{new_rx_error, RxResult};
use crate::types::{BulkWriteRow, FilledMangoQuery, RxJsonSchema};

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

pub fn ensure_collection_table(conn: &Connection, table: &str) -> RxResult<()> {
    let quoted = quote_identifier(table);
    let lwt_index = quote_identifier(&format!("{table}_lwt_id_idx"));
    let deleted_index = quote_identifier(&format!("{table}_deleted_lwt_id_idx"));
    let changed_tables = quote_identifier(CHANGED_TABLES_TABLE);
    let table_literal = quote_sql_literal(table);
    let insert_trigger = quote_identifier(&format!("{table}__rxdb_changed_insert"));
    let update_trigger = quote_identifier(&format!("{table}__rxdb_changed_update"));
    let delete_trigger = quote_identifier(&format!("{table}__rxdb_changed_delete"));
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
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
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
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
    let mut sql = format!("SELECT data FROM {}", quote_identifier(table));
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
        let mut inner = format!("SELECT 1 FROM {}", quote_identifier(table));
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
        let mut sql = format!("SELECT COUNT(*) FROM {}", quote_identifier(table));
        if !where_sql.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
        }
        sql
    };
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
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
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
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
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
                        let placeholders = std::iter::repeat("?")
                            .take(values.len())
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

pub fn insert_document(
    conn: &Connection,
    table: &str,
    primary_path: &str,
    document: &Value,
) -> RxResult<()> {
    let id = document_id(document, primary_path)?;
    let revision = document
        .get("_rev")
        .and_then(Value::as_str)
        .unwrap_or_default();
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
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
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
    let id = document_id(document, primary_path)?;
    let revision = document
        .get("_rev")
        .and_then(Value::as_str)
        .unwrap_or_default();
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
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
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

pub fn all_documents(conn: &Connection, table: &str) -> RxResult<Vec<Value>> {
    let mut ret = Vec::new();
    for_each_document(conn, table, |doc| {
        ret.push(doc);
        Ok(true)
    })?;
    Ok(ret)
}

/// Walks every row of the table without first materializing the full Vec.
/// The visitor returns `Ok(true)` to continue or `Ok(false)` to stop early.
/// V1.5 query streaming relies on this for bounded-memory reads on large
/// collections.
pub fn for_each_document<F>(conn: &Connection, table: &str, mut visit: F) -> RxResult<()>
where
    F: FnMut(Value) -> RxResult<bool>,
{
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
    let mut stmt = conn
        .prepare(&format!("SELECT data FROM {}", quote_identifier(table)))
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
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
    let data: Option<String> = conn
        .query_row(
            &format!("SELECT data FROM {} WHERE id = ?", quote_identifier(table)),
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sqlite_error)?;
    data.map(|text| decode_document_json(&text)).transpose()
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
    let quoted_table = quote_identifier(table);
    for chunk in ids.chunks(DOCUMENTS_BY_ID_BATCH_SIZE) {
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = if with_deleted {
            format!("SELECT id, data FROM {quoted_table} WHERE id IN ({placeholders})")
        } else {
            format!(
                "SELECT id, data FROM {quoted_table} WHERE id IN ({placeholders}) AND deleted = 0"
            )
        };
        crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
        let mut statement = conn.prepare(&sql).map_err(sqlite_error)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(String::as_str)), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sqlite_error)?;
        for row in rows {
            let (id, data) = row.map_err(sqlite_error)?;
            let doc = decode_document_json(&data)?;
            by_id.insert(id, doc);
        }
    }

    Ok(ids.iter().filter_map(|id| by_id.get(id).cloned()).collect())
}

pub fn drop_table(conn: &Connection, table: &str) -> RxResult<()> {
    crate::storage::sqlite::instance::record_sqlite_statement_executed(1);
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
