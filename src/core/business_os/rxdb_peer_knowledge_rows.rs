// Origin: CTOX
// License: Apache-2.0

//! Parquet-backed `knowledge_tables` source for `rxdb.rows.fetch`.
//!
//! The RxDB collection stays a catalog until the later projection cut. This
//! source is what a peer actually reads: one active catalog row, then an eager
//! parquet window. Unknown and archived tables are `ROWS_TABLE_NOT_FOUND`.
//! Every other failure uses a code the rows handler classifies as a retryable
//! `ROWS_SOURCE_ERROR`.

use super::rxdb_peer::WebRtcPool;
use rxdb::plugins::replication_webrtc::rows_fetch_handler::{
    RowsWindow, RowsWindowFn, ROWS_FETCH_ERROR_TABLE_NOT_FOUND,
};
use rxdb::rx_error::{new_rx_error, RxResult};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Register the Business-OS knowledge row source before any handshake so
/// `has_any_source()` announces `ctox-rxdb-rows-fetch-v1`.
pub(super) fn register_knowledge_row_source(pool: &WebRtcPool, root: &Path) {
    let source = knowledge_row_source(root.to_path_buf());
    pool.rows_fetch_registry
        .register_rows_source("knowledge_tables", source);
    eprintln!("[business-os] knowledge row source registered for `knowledge_tables`");
}

fn knowledge_row_source(root: PathBuf) -> Arc<RowsWindowFn> {
    Arc::new(move |table_id, offset, limit| knowledge_rows_window(&root, table_id, offset, limit))
}

fn knowledge_rows_window(
    root: &Path,
    table_id: &str,
    offset: usize,
    limit: usize,
) -> RxResult<RowsWindow> {
    match crate::knowledge::knowledge_table_row_window(root, table_id, offset, limit) {
        Ok(window) => Ok(RowsWindow {
            rows: window.rows,
            row_count: window.row_count,
            content_hash: window.content_hash,
            schema_hash: window.schema_hash,
        }),
        Err(err) => {
            let message = format!("{err:#}");
            // `rows_fetch_error_code_for_rx_error` maps this code (and
            // NOT_FOUND / ENOENT) to non-retryable ROWS_TABLE_NOT_FOUND.
            // Any other code becomes retryable ROWS_SOURCE_ERROR, so this
            // branch must not reuse NOT_FOUND, ENOENT, or the auth codes.
            let code = if message.contains("unknown knowledge table")
                || message.contains("archived knowledge table")
            {
                ROWS_FETCH_ERROR_TABLE_NOT_FOUND
            } else {
                "KNOWLEDGE_ROWS_SOURCE"
            };
            Err(new_rx_error(code, Some(json!({ "message": message }))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rxdb::plugins::replication_webrtc::{RxWebRTCReplicationPool, WebRTCRsConnectionHandler};
    use serde_json::json;

    #[test]
    fn knowledge_row_source_returns_window_and_not_found() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        crate::knowledge::seed_knowledge_table_for_test(
            root,
            "kdt-measured",
            "drone_bearing_design",
            "measured_load_points",
            &[json!({
                "source_id": "source-1",
                "rpm": 1000.0,
            })],
            None,
        )?;

        let source = knowledge_row_source(root.to_path_buf());
        let window = source("kdt-measured", 0, 10).expect("registered source returns a window");
        assert_eq!(window.row_count, 1);
        assert_eq!(window.rows.len(), 1);
        assert_eq!(window.rows[0]["row_id"], json!("MLP-0001"));
        assert!(!window.content_hash.is_empty());
        assert!(!window.schema_hash.is_empty());

        let missing = source("kdt-missing", 0, 10).expect_err("unknown table");
        assert_eq!(missing.code(), ROWS_FETCH_ERROR_TABLE_NOT_FOUND);

        crate::knowledge::seed_knowledge_table_for_test(
            root,
            "kdt-archived",
            "archived_domain",
            "notes",
            &[json!({ "id": "n1" })],
            Some("2020-01-01T00:00:00Z"),
        )?;
        let archived = source("kdt-archived", 0, 10).expect_err("archived table");
        assert_eq!(archived.code(), ROWS_FETCH_ERROR_TABLE_NOT_FOUND);

        let pool = RxWebRTCReplicationPool::new_multi(Vec::new(), WebRTCRsConnectionHandler::new());
        register_knowledge_row_source(&pool, root);
        assert!(pool.rows_fetch_registry.has_source("knowledge_tables"));
        assert!(pool.rows_fetch_registry.has_any_source());
        Ok(())
    }
}
