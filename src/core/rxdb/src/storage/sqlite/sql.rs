//! SQL helpers for the SQLite storage backend.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::rx_error::{new_rx_error, RxResult};
use crate::types::BulkWriteRow;

use super::types::sqlite_error;

const CHANGED_TABLES_TABLE: &str = "__rxdb_changed_tables";

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

fn quote_sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
    let mut stmt = conn
        .prepare(&format!("SELECT data FROM {}", quote_identifier(table)))
        .map_err(sqlite_error)?;
    let mut rows = stmt.query([]).map_err(sqlite_error)?;
    while let Some(row) = rows.next().map_err(sqlite_error)? {
        let data: String = row.get(0).map_err(sqlite_error)?;
        let doc = serde_json::from_str(&data).map_err(|err| {
            new_rx_error(
                "SQLITE_JSON",
                Some(serde_json::json!({ "message": err.to_string() })),
            )
        })?;
        if !visit(doc)? {
            break;
        }
    }
    Ok(())
}

pub fn document_by_id(conn: &Connection, table: &str, id: &str) -> RxResult<Option<Value>> {
    let data: Option<String> = conn
        .query_row(
            &format!("SELECT data FROM {} WHERE id = ?", quote_identifier(table)),
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sqlite_error)?;
    data.map(|text| {
        serde_json::from_str(&text).map_err(|err| {
            new_rx_error(
                "SQLITE_JSON",
                Some(serde_json::json!({ "message": err.to_string() })),
            )
        })
    })
    .transpose()
}

pub fn drop_table(conn: &Connection, table: &str) -> RxResult<()> {
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
