use ctox_transfers::Store;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

// thesen 08.10.2026: the worker polled every 250 ms with an IMMEDIATE
// transaction on Core even with an empty queue. While another writer held the
// store it waited out the busy timeout and then stopped with "storage failure;
// restart required" in almost every service process.
#[tokio::test]
async fn idle_worker_neither_waits_for_nor_fails_on_a_locked_store() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("state.sqlite");
    let store = Store::open(db.clone(), temp.path().join("objects")).unwrap();
    let worker = store.worker().unwrap();
    assert!(!worker.run_next(&AtomicBool::new(false)).await.unwrap());

    let holder = rusqlite::Connection::open(&db).unwrap();
    holder
        .execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE IF NOT EXISTS lock_probe (value INTEGER);
             INSERT INTO lock_probe VALUES (1);",
        )
        .unwrap();
    let started = Instant::now();
    let ran = worker
        .run_next(&AtomicBool::new(false))
        .await
        .expect("an idle poll must not fail while another writer holds the store");
    assert!(!ran);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "idle poll waited {:?} for the write lock",
        started.elapsed()
    );
    holder.execute_batch("ROLLBACK").unwrap();
}
