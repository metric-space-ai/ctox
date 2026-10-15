//! Cleanup helper for SQLite storage.

use rusqlite::params;

use crate::plugins::utils::utils_time::now;
use crate::rx_error::RxResult;

use super::sql::quote_identifier;
use super::types::sqlite_error;

/// How many tombstones one cleanup call may delete.
///
/// The statement used to be unbounded. That is harmless while a database is
/// young and a live hazard once it is not: the first run against a real
/// instance would delete a six-figure row count inside a single write lock,
/// stalling every replication write behind it. Bounding the batch turns the
/// first run into many short pauses instead of one long one.
// ref: rxdb/src/plugins/cleanup/cleanup.ts (cleanupPolicy.awaitReplicationsInSync)
const CLEANUP_BATCH_LIMIT: usize = 2_000;

/// Delete at most [`CLEANUP_BATCH_LIMIT`] tombstones older than the retention
/// window. Returns `true` when the collection is clean, `false` when the batch
/// was full and more tombstones remain — the caller is expected to come back.
///
/// The `bool` is the upstream RxDB contract and it was previously a lie: this
/// function always returned `true`. Nothing called it, so nothing noticed.
pub fn cleanup_deleted_documents(
    conn: &rusqlite::Connection,
    table_name: &str,
    minimum_deleted_time: i64,
) -> RxResult<bool> {
    cleanup_deleted_documents_batch(conn, table_name, minimum_deleted_time, CLEANUP_BATCH_LIMIT)
}

pub(crate) fn cleanup_deleted_documents_batch(
    conn: &rusqlite::Connection,
    table_name: &str,
    minimum_deleted_time: i64,
    batch_limit: usize,
) -> RxResult<bool> {
    let max_deletion_time = now() - minimum_deleted_time as f64;
    let _statement_timer = crate::storage::sqlite::instance::timed_sqlite_statement();
    let quoted = quote_identifier(table_name);
    let deleted = conn
        .execute(
            &format!(
                "DELETE FROM {quoted} WHERE rowid IN (
                     SELECT rowid FROM {quoted}
                     WHERE deleted = 1 AND lastWriteTime < ?
                     LIMIT ?
                 )"
            ),
            params![max_deletion_time, batch_limit as i64],
        )
        .map_err(sqlite_error)?;
    Ok(deleted < batch_limit)
}
