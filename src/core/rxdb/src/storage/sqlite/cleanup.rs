//! Cleanup helper for SQLite storage.

use rusqlite::params;

use crate::plugins::utils::utils_time::now;
use crate::rx_error::RxResult;

use super::sql::quote_identifier;
use super::types::{sqlite_error, SharedSqliteConnection};

pub fn cleanup_deleted_documents(
    connection: &SharedSqliteConnection,
    table_name: &str,
    minimum_deleted_time: i64,
) -> RxResult<bool> {
    let max_deletion_time = now() - minimum_deleted_time as f64;
    let conn = crate::storage::sqlite::instance::lock_sqlite_writer(connection);
    let _statement_timer = crate::storage::sqlite::instance::timed_sqlite_statement();
    conn.execute(
        &format!(
            "DELETE FROM {} WHERE deleted = 1 AND lastWriteTime < ?",
            quote_identifier(table_name)
        ),
        params![max_deletion_time],
    )
    .map_err(sqlite_error)?;
    Ok(true)
}
