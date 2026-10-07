//! The operator can inspect a transfer while another connection owns the writer.
use ctox_transfers::{DownloadRequest, Store, Transfer};
use rusqlite::Connection;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn transfer_status_reads_under_a_writer_without_creating_cli_ledger() {
    let state = tempfile::tempdir().unwrap();
    let database = state.path().join("ctox.sqlite3");
    let store = Store::open(&database, state.path().join("transfers")).unwrap();
    let request = DownloadRequest {
        id: "status-under-writer".into(),
        sources: vec!["https://invalid.invalid/unused".into()],
        peer_source: None,
        storage: None,
        sha256: "ab".repeat(32),
        size: 1,
    };
    store.enqueue(request.clone()).unwrap();
    let writer = Connection::open(&database).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode=WAL; BEGIN IMMEDIATE;")
        .unwrap();

    // WAL readers can proceed, but startup migration/ledger writes cannot.
    // Invoke the actual CLI; a predicate-only test would miss either side effect.
    let mut child = Command::new(env!("CARGO_BIN_EXE_ctox"))
        .args(["transfer", "status", request.id.as_str()])
        .env("CTOX_ROOT", env!("CARGO_MANIFEST_DIR"))
        .env("CTOX_STATE_ROOT", state.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            result => {
                // Always reap the owned child before releasing the writer/fixture.
                let _ = child.kill();
                let _ = child.wait();
                assert!(result.is_ok(), "failed to observe native CLI: {result:?}");
                timed_out = true;
                break;
            }
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        !timed_out && output.status.success(),
        "read-only status waited for the writer or failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Transfer = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual.request, request);
    assert_eq!(actual.state, "queued");
    let ledger_tables: i64 = writer
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('ctox_turns', 'ctox_turn_commands')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ledger_tables, 0, "status must not initialize a CLI audit ledger");
    writer.execute_batch("ROLLBACK;").unwrap();
}
