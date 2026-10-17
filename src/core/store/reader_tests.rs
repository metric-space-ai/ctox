use super::*;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::time::Duration;

fn budget(capacity: usize) -> AdmissionBudget {
    AdmissionBudget::new(
        NonZeroUsize::new(capacity).unwrap(),
        Duration::from_secs(2),
        Duration::from_secs(5),
    )
    .unwrap()
}

fn database(path: &Path, value: i64) -> Connection {
    let writer = Connection::open(path).unwrap();
    ConnectionConfig::default().apply_writer(&writer).unwrap();
    writer.execute_batch("CREATE TABLE sample(value INTEGER); CREATE TABLE head(seq INTEGER, schema_gen INTEGER);").unwrap();
    writer
        .execute("INSERT INTO sample VALUES (?)", [value])
        .unwrap();
    writer
        .execute("INSERT INTO head VALUES (?, ?)", [value, value])
        .unwrap();
    writer
}

async fn pool(path: &Path, capacity: usize) -> ReadPool {
    ReadPool::open(path, 1, ConnectionConfig::default(), budget(capacity))
        .await
        .unwrap()
}

fn value(transaction: &Transaction<'_>) -> rusqlite::Result<i64> {
    transaction.query_row("SELECT value FROM sample", [], |row| row.get(0))
}

#[tokio::test]
async fn v4_read_jobs_are_blocking_readonly_transactions() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("db.sqlite3");
    let _writer = database(&path, 1);
    let pool = pool(&path, 8).await;
    let async_thread = std::thread::current().id();
    assert_eq!(
        pool.read(move |transaction| {
            assert_ne!(std::thread::current().id(), async_thread);
            assert!(!transaction.is_autocommit());
            assert_eq!(
                transaction.pragma_query_value(None, "query_only", |row| row.get::<_, i64>(0))?,
                1
            );
            // Even a trusted closure cannot turn a read-only file handle into a writer.
            transaction.pragma_update(None, "query_only", false)?;
            assert!(transaction
                .execute("UPDATE sample SET value=99", [])
                .is_err());
            value(transaction)
        })
        .await
        .unwrap(),
        1
    );
    pool.close();
    assert!(matches!(pool.read(value).await, Err(ReadPoolError::Closed)));
}

#[tokio::test]
async fn v4_snapshot_binds_documents_head_and_schema_across_concurrent_commit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("snapshot.sqlite3");
    let mut writer = database(&path, 1);
    let pool = pool(&path, 8).await;
    let (started, start) = mpsc::channel();
    let (committed, commit) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        start.recv_timeout(Duration::from_secs(3)).unwrap();
        let transaction = writer.transaction().unwrap();
        transaction
            .execute_batch("UPDATE sample SET value=2; UPDATE head SET seq=2, schema_gen=2;")
            .unwrap();
        transaction.commit().unwrap();
        committed.send(()).unwrap();
    });
    let snapshot = pool
        .read(move |transaction| {
            // The pool pins before the closure's first application query.
            started.send(()).unwrap();
            commit.recv_timeout(Duration::from_secs(3)).unwrap();
            let document = value(transaction)?;
            let (head, schema) =
                transaction.query_row("SELECT seq, schema_gen FROM head", [], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?;
            Ok((document, head, schema))
        })
        .await
        .unwrap();
    writer.join().unwrap();
    assert_eq!(snapshot, (1, 1, 1));
    assert_eq!(pool.read(value).await.unwrap(), 2);
}

async fn hold_readers(
    pool: &ReadPool,
) -> (
    Vec<mpsc::Sender<()>>,
    Vec<tokio::task::JoinHandle<Result<i64, ReadPoolError>>>,
) {
    let mut releases = Vec::new();
    let mut jobs = Vec::new();
    for _ in 0..READ_CONNECTIONS {
        let pool = pool.clone();
        let (started, start) = tokio::sync::oneshot::channel();
        let (release, released) = mpsc::channel();
        jobs.push(tokio::spawn(async move {
            pool.read(move |transaction| {
                started.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(3)).unwrap();
                value(transaction)
            })
            .await
        }));
        start.await.unwrap();
        releases.push(release);
    }
    (releases, jobs)
}

#[tokio::test]
async fn v4_four_readers_and_bounded_admission_without_writer_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bounded.sqlite3");
    let writer = database(&path, 1);
    let pool = pool(&path, 4).await;
    let (releases, jobs) = hold_readers(&pool).await;
    assert!(matches!(pool.read(value).await, Err(ReadPoolError::Busy)));
    // Four snapshots do not reserve the single writer.
    writer.execute("UPDATE sample SET value=2", []).unwrap();
    for release in releases {
        release.send(()).unwrap();
    }
    for job in jobs {
        assert_eq!(job.await.unwrap().unwrap(), 1);
    }
    assert_eq!(pool.read(value).await.unwrap(), 2);
}

#[tokio::test]
async fn v4_sql_execution_deadline_interrupts_and_reader_is_reusable() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("deadline.sqlite3");
    let _writer = database(&path, 1);
    let mut admission = budget(4);
    admission.execution = Duration::from_millis(5);
    let pool = ReadPool::open(&path, 1, ConnectionConfig::default(), admission)
        .await
        .unwrap();
    let result = pool.read(|transaction| transaction.query_row(
        "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n", [], |row| row.get::<_, i64>(0)
    )).await;
    assert!(
        matches!(result, Err(ReadPoolError::ExecutionTimeout)),
        "{result:?}"
    );
    assert_eq!(pool.read(value).await.unwrap(), 1);
}

#[tokio::test]
async fn v4_failed_and_panicking_jobs_release_snapshot_and_connection() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("failure.sqlite3");
    let _writer = database(&path, 1);
    let pool = pool(&path, 4).await;
    assert!(matches!(
        pool.read(|_| Err::<(), _>(rusqlite::Error::InvalidQuery))
            .await,
        Err(ReadPoolError::Sqlite(_))
    ));
    assert!(matches!(
        pool.read::<(), _>(|_| panic!("deliberate read-job panic"))
            .await,
        Err(ReadPoolError::Worker(_))
    ));
    assert_eq!(pool.read(value).await.unwrap(), 1);
}

#[tokio::test]
async fn v4_renewal_drains_snapshots_and_rejects_queued_old_generation() {
    use std::future::{poll_fn, Future};
    use std::task::Poll;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("generation.sqlite3");
    let _writer = database(&path, 1);
    let pool = pool(&path, 8).await;
    let (releases, jobs) = hold_readers(&pool).await;
    let renew = pool.renew_generation(1, 2);
    tokio::pin!(renew);
    poll_fn(|cx| {
        assert!(renew.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let old = pool.read(value);
    tokio::pin!(old);
    poll_fn(|cx| {
        assert!(old.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    for release in releases {
        release.send(()).unwrap();
    }
    let (renewed, old) = tokio::join!(renew, old);
    renewed.unwrap();
    assert!(matches!(
        old,
        Err(ReadPoolError::GenerationChanged {
            expected: 1,
            actual: 2
        })
    ));
    for job in jobs {
        assert_eq!(job.await.unwrap().unwrap(), 1);
    }
    assert_eq!(pool.generation(), 2);
    assert_eq!(pool.read(value).await.unwrap(), 1);
    assert!(matches!(
        pool.renew_generation(1, 3).await,
        Err(ReadPoolError::GenerationChanged { .. })
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn v4_replaced_file_requires_explicit_generation_renewal() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first.sqlite3");
    let second = temp.path().join("second.sqlite3");
    let _first = database(&first, 1);
    let _second = database(&second, 2);
    let path = temp.path().join("active.sqlite3");
    symlink(&first, &path).unwrap();
    let pool = pool(&path, 4).await;
    let next = temp.path().join("next.sqlite3");
    symlink(&second, &next).unwrap();
    std::fs::rename(next, path).unwrap();
    assert!(matches!(
        pool.read(value).await,
        Err(ReadPoolError::FileReplaced)
    ));
    pool.renew_generation(1, 2).await.unwrap();
    assert_eq!(pool.read(value).await.unwrap(), 2);
}

#[tokio::test]
async fn v4_missing_file_is_not_created_by_read_pool() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("missing.sqlite3");
    assert!(matches!(
        ReadPool::open(&path, 1, ConnectionConfig::default(), budget(4)).await,
        Err(ReadPoolError::Io(_))
    ));
    assert!(!path.exists());
}

#[tokio::test]
async fn v4_shared_job_contract_preserves_cancellation_and_deadline_errors() {
    use crate::Cancellation;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("typed-job.sqlite3");
    let _writer = database(&path, 1);
    let id = DbId::new(&path).unwrap();
    let pool = ReadPool::for_database(&id, DbGeneration(1), ConnectionConfig::default(), budget(4))
        .await
        .unwrap();
    let job = ReadJob::new(JobOptions::default(), |connection| {
        Ok(connection.query_row("SELECT value FROM sample", [], |row| row.get::<_, i64>(0))?)
    });
    assert_eq!(pool.read_job(job).await.unwrap(), 1);

    let cancellation = Cancellation::default();
    cancellation.cancel();
    let job = ReadJob::new(
        JobOptions {
            cancellation,
            ..JobOptions::default()
        },
        |_| panic!("cancelled callback must not run"),
    );
    assert_eq!(pool.read_job::<()>(job).await, Err(StoreError::Cancelled));
    let job = ReadJob::new(
        JobOptions {
            deadline: Some(Instant::now()),
            ..JobOptions::default()
        },
        |_| panic!("expired callback must not run"),
    );
    assert_eq!(
        pool.read_job::<()>(job).await,
        Err(StoreError::DeadlineExceeded)
    );
}

#[tokio::test]
async fn v4_job_cancellation_interrupts_sql_without_returning_partial_page() {
    use crate::Cancellation;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cancel-sql.sqlite3");
    let _writer = database(&path, 1);
    let pool = pool(&path, 4).await;
    let cancellation = Cancellation::default();
    let external_cancel = cancellation.clone();
    let (started, start) = tokio::sync::oneshot::channel();
    let job = ReadJob::new(
        JobOptions {
            cancellation,
            ..JobOptions::default()
        },
        move |connection| {
            started.send(()).unwrap();
            Ok(connection.query_row("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n", [], |row| row.get::<_, i64>(0))?)
        },
    );
    let running = {
        let pool = pool.clone();
        tokio::spawn(async move { pool.read_job(job).await })
    };
    start.await.unwrap();
    external_cancel.cancel();
    assert_eq!(running.await.unwrap(), Err(StoreError::Cancelled));
    assert_eq!(pool.read(value).await.unwrap(), 1);
}

#[tokio::test]
async fn v4_cancelled_future_keeps_capacity_until_blocking_job_terminates() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cancel-future.sqlite3");
    let _writer = database(&path, 1);
    let pool = pool(&path, 1).await;
    let (started, start) = tokio::sync::oneshot::channel();
    let (release, released) = mpsc::channel();
    let running = {
        let pool = pool.clone();
        tokio::spawn(async move {
            pool.read(move |transaction| {
                started.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(3)).unwrap();
                value(transaction)
            })
            .await
        })
    };
    start.await.unwrap();
    running.abort();
    assert!(running.await.unwrap_err().is_cancelled());
    assert!(matches!(pool.read(value).await, Err(ReadPoolError::Busy)));
    release.send(()).unwrap();
    pool.shutdown().await.unwrap();
    assert!(pool.0.readers.lock().unwrap().connections.is_empty());
    assert!(matches!(pool.read(value).await, Err(ReadPoolError::Closed)));
    pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn v4_admission_timeout_waits_asynchronously_without_spawning_sql() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("admission.sqlite3");
    let _writer = database(&path, 1);
    let mut admission = budget(8);
    admission.wait = Duration::from_millis(40);
    let pool = ReadPool::open(&path, 1, ConnectionConfig::default(), admission)
        .await
        .unwrap();
    let (releases, jobs) = hold_readers(&pool).await;
    let read = pool.read::<(), _>(|_| panic!("no blocking worker was admitted"));
    tokio::pin!(read);
    tokio::select! {
        biased;
        _ = tokio::time::sleep(Duration::from_millis(5)) => {}
        result = &mut read => panic!("admission completed before the runtime heartbeat: {result:?}"),
    }
    assert!(matches!(read.await, Err(ReadPoolError::AdmissionTimeout)));
    for release in releases {
        release.send(()).unwrap();
    }
    for job in jobs {
        assert_eq!(job.await.unwrap().unwrap(), 1);
    }
    assert_eq!(pool.read(value).await.unwrap(), 1);
}
