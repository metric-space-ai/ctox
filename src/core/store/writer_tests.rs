use super::*;
use crate::registry::Registry;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

fn database() -> (tempfile::TempDir, DbId) {
    let directory = tempfile::tempdir().unwrap();
    let id = DbId::new(directory.path().join("store.sqlite3")).unwrap();
    (directory, id)
}
async fn opened(id: DbId) -> WriterHandle {
    let writer = Registry.open(id, WriterOptions::default()).unwrap();
    writer.ready().await.unwrap();
    writer
}
async fn execute(writer: &WriterHandle, sql: &'static str) {
    writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            tx.execute_batch(sql)?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
}
async fn count(writer: &WriterHandle) -> i64 {
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            Ok(tx.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?)
        }))
        .unwrap()
        .await
        .unwrap()
}

#[tokio::test]
async fn exactly_one_owner_across_registries_and_reopen_after_shutdown() {
    let (_directory, id) = database();
    let writer = opened(id.clone()).await;
    assert!(matches!(
        Registry.open(id.clone(), WriterOptions::default()),
        Err(StoreError::AlreadyOwned)
    ));
    execute(&writer, "CREATE TABLE records(id INTEGER)").await;
    let previous = writer.generation();
    writer.shutdown().await.unwrap();
    assert!(matches!(
        writer.submit(WriteJob::new(JobOptions::default(), |_| Ok(()))),
        Err(StoreError::Closed)
    ));
    let replacement = opened(id).await;
    assert_ne!(replacement.generation(), previous);
    assert_eq!(count(&replacement).await, 0);
    replacement.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_alias_is_the_same_owner() {
    let (directory, id) = database();
    let writer = opened(id).await;
    let alias = directory.path().join("alias.sqlite3");
    std::os::unix::fs::symlink(writer.db_id().path(), &alias).unwrap();
    assert!(matches!(
        Registry.open(DbId::new(alias).unwrap(), WriterOptions::default()),
        Err(StoreError::AlreadyOwned)
    ));
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn sqlite_and_jobs_run_on_one_os_thread_with_safe_pragmas() {
    let (_directory, id) = database();
    let caller = std::thread::current().id();
    let writer = opened(id).await;
    let mut ids = Vec::new();
    for _ in 0..4 {
        ids.push(
            writer
                .submit(WriteJob::new(JobOptions::default(), |tx| {
                    assert_eq!(
                        tx.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))?,
                        "wal"
                    );
                    assert_eq!(
                        tx.query_row("PRAGMA synchronous", [], |r| r.get::<_, i64>(0))?,
                        1
                    );
                    assert_eq!(
                        tx.query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))?,
                        0
                    );
                    assert_eq!(
                        tx.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))?,
                        1
                    );
                    Ok(std::thread::current().id())
                }))
                .unwrap()
                .await
                .unwrap(),
        );
    }
    assert!(ids.iter().all(|id| *id == ids[0] && *id != caller));
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn queue_is_bounded_and_shutdown_drains_every_accepted_job() {
    let (_directory, id) = database();
    let writer = opened(id).await;
    execute(&writer, "CREATE TABLE records(id INTEGER)").await;
    let (entered_send, entered) = mpsc::channel();
    let (release, release_receive) = mpsc::channel();
    let running = writer
        .submit(WriteJob::new(JobOptions::default(), move |_| {
            entered_send.send(()).unwrap();
            release_receive.recv().unwrap();
            Ok(())
        }))
        .unwrap();
    // This test deliberately parks the writer with an OS channel to exercise
    // backpressure deterministically, not by timing a sleeping async worker.
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    let mut accepted = Vec::new();
    for _ in 0..QUEUE_CAPACITY {
        accepted.push(
            writer
                .submit(WriteJob::new(JobOptions::default(), |tx| {
                    tx.execute("INSERT INTO records VALUES (1)", [])?;
                    Ok(())
                }))
                .unwrap(),
        );
    }
    assert!(matches!(
        writer.submit(WriteJob::new(JobOptions::default(), |_| Ok(()))),
        Err(StoreError::Busy)
    ));
    let clone = writer.clone();
    let closing = tokio::spawn(async move { clone.shutdown().await });
    // close admission explicitly before allowing the blocked operation through.
    writer.inner.close();
    release.send(()).unwrap();
    running.await.unwrap();
    for job in accepted {
        job.await.unwrap();
    }
    closing.await.unwrap().unwrap();
    let replacement = opened(writer.db_id().clone()).await;
    assert_eq!(count(&replacement).await, QUEUE_CAPACITY as i64);
    replacement.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancellation_deadline_and_panic_rollback_without_poisoning_writer() {
    let (_directory, id) = database();
    let writer = opened(id).await;
    execute(&writer, "CREATE TABLE records(id INTEGER)").await;
    let cancellation = Cancellation::default();
    let cancel_in_job = cancellation.clone();
    let cancelled = writer
        .submit(WriteJob::new(
            JobOptions {
                cancellation,
                ..Default::default()
            },
            move |tx| {
                tx.execute("INSERT INTO records VALUES (1)", [])?;
                cancel_in_job.cancel();
                Ok(())
            },
        ))
        .unwrap();
    assert_eq!(cancelled.await, Err(StoreError::Cancelled));
    let panicked = writer
        .submit(WriteJob::new(
            JobOptions::default(),
            |tx| -> Result<(), StoreError> {
                tx.execute("INSERT INTO records VALUES (2)", [])?;
                panic!("injected job panic")
            },
        ))
        .unwrap();
    assert_eq!(panicked.await, Err(StoreError::Panicked));
    let expired = JobOptions {
        deadline: Some(Instant::now()),
        ..Default::default()
    };
    assert!(matches!(
        writer.submit(WriteJob::new(expired, |_| Ok(()))),
        Err(StoreError::DeadlineExceeded)
    ));
    assert_eq!(count(&writer).await, 0);
    execute(&writer, "INSERT INTO records VALUES (3)").await;
    assert_eq!(count(&writer).await, 1);
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn same_kind_batches_use_savepoints_and_publish_only_after_commit() {
    let (_directory, id) = database();
    let hooks = Arc::new(AtomicUsize::new(0));
    let hook_calls = hooks.clone();
    let writer = Registry
        .open(
            id,
            WriterOptions {
                head_seq: Some(Arc::new(move |connection| {
                    let value: u64 =
                        connection
                            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?;
                    hook_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(value)
                })),
                ..Default::default()
            },
        )
        .unwrap();
    writer.ready().await.unwrap();
    execute(&writer, "CREATE TABLE records(id INTEGER PRIMARY KEY)").await;
    let (entered_send, entered) = mpsc::channel();
    let (release, release_receive) = mpsc::channel();
    let blocker = writer
        .submit(WriteJob::new(JobOptions::default(), move |_| {
            entered_send.send(()).unwrap();
            release_receive.recv().unwrap();
            Ok(())
        }))
        .unwrap();
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    let mut commits = writer.subscribe_commits();
    commits.latest();
    let before = hooks.load(Ordering::SeqCst);
    let mut jobs = Vec::new();
    for i in [1, 1, 2] {
        jobs.push(
            writer
                .submit(WriteJob::new(
                    JobOptions {
                        batch_key: Some("records".into()),
                        ..Default::default()
                    },
                    move |tx| {
                        tx.execute("INSERT INTO records VALUES (?)", [i])?;
                        Ok(i)
                    },
                ))
                .unwrap(),
        );
    }
    assert!(!commits.has_changed().unwrap());
    release.send(()).unwrap();
    blocker.await.unwrap();
    let mut results = Vec::new();
    for job in jobs {
        results.push(job.await);
    }
    assert_eq!(results[0], Ok(1));
    assert!(matches!(results[1], Err(StoreError::Sqlite(_))));
    assert_eq!(results[2], Ok(2));
    assert_eq!(
        hooks.load(Ordering::SeqCst) - before,
        2,
        "blocker plus one batched transaction"
    );
    let notification = commits.latest().unwrap();
    assert_eq!(notification.head_seq, 2);
    assert_eq!(notification.generation, writer.generation());
    // Independent WAL reader proves the notification is not pre-commit.
    let path = writer.db_id().path().to_path_buf();
    let rows = tokio::task::spawn_blocking(move || {
        let reader =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader
            .query_row("SELECT COUNT(*) FROM records", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(rows, 2);
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn failing_head_hook_rolls_back_and_does_not_notify() {
    let (_directory, id) = database();
    let writer = Registry
        .open(
            id.clone(),
            WriterOptions {
                head_seq: Some(Arc::new(|_| {
                    Err(StoreError::Invalid("injected head failure".into()))
                })),
                ..Default::default()
            },
        )
        .unwrap();
    writer.ready().await.unwrap();
    let commits = writer.subscribe_commits();
    assert!(writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute_batch("CREATE TABLE never_committed(id)")?;
            Ok(())
        }))
        .unwrap()
        .await
        .is_err());
    assert!(!commits.has_changed().unwrap());
    writer.shutdown().await.unwrap();
    let writer = opened(id).await;
    let exists = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            Ok(tx.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='never_committed'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(exists, 0);
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn dropping_last_handle_releases_file_and_failed_start_can_be_retried() {
    let (directory, id) = database();
    let writer = opened(id.clone()).await;
    let mut state = writer.inner.state.subscribe();
    drop(writer);
    loop {
        if matches!(*state.borrow_and_update(), Lifecycle::Stopped(_)) {
            break;
        }
        state.changed().await.unwrap();
    }
    let writer = opened(id).await;
    writer.shutdown().await.unwrap();
    let bad = DbId::new(directory.path().join("directory.sqlite3")).unwrap();
    std::fs::create_dir(bad.path()).unwrap();
    let failed = Registry
        .open(bad.clone(), WriterOptions::default())
        .unwrap();
    assert!(failed.ready().await.is_err());
    assert!(failed.shutdown().await.is_err());
    std::fs::remove_dir(bad.path()).unwrap();
    let repaired = opened(bad).await;
    repaired.shutdown().await.unwrap();
}

#[test]
fn file_lock_is_explicitly_unlocked_before_duplicate_descriptor_closes() {
    let (_directory, id) = database();
    let owner = FileOwner::acquire(&id).unwrap();
    let duplicate = owner.0.try_clone().unwrap();
    assert!(matches!(
        FileOwner::acquire(&id),
        Err(StoreError::AlreadyOwned)
    ));
    drop(owner);
    let next = FileOwner::acquire(&id).unwrap();
    drop(duplicate);
    drop(next);
}

#[cfg(unix)]
#[tokio::test]
async fn hard_link_aliases_are_rejected_instead_of_bypassing_file_ownership() {
    let (directory, id) = database();
    let writer = opened(id).await;
    let alias = directory.path().join("hardlink.sqlite3");
    std::fs::hard_link(writer.db_id().path(), &alias).unwrap();
    assert!(matches!(DbId::new(&alias), Err(StoreError::Invalid(_))));
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn queued_cancelled_or_expired_jobs_never_execute() {
    let (_directory, id) = database();
    let writer = opened(id).await;
    let (entered_send, entered) = mpsc::channel();
    let (release, release_receive) = mpsc::channel();
    let blocker = writer
        .submit(WriteJob::new(JobOptions::default(), move |_| {
            entered_send.send(()).unwrap();
            release_receive.recv().unwrap();
            Ok(())
        }))
        .unwrap();
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    let cancellation = Cancellation::default();
    let cancelled = writer
        .submit(WriteJob::new(
            JobOptions {
                cancellation: cancellation.clone(),
                ..Default::default()
            },
            |_| -> Result<(), StoreError> { panic!("cancelled queued operation executed") },
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(20);
    let expired = writer
        .submit(WriteJob::new(
            JobOptions {
                deadline: Some(deadline),
                ..Default::default()
            },
            |_| -> Result<(), StoreError> { panic!("expired queued operation executed") },
        ))
        .unwrap();
    cancellation.cancel();
    // Only the blocking helper waits; this never sleeps a Tokio worker.
    tokio::task::spawn_blocking(move || {
        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
        )
    })
    .await
    .unwrap();
    release.send(()).unwrap();
    blocker.await.unwrap();
    assert_eq!(cancelled.await, Err(StoreError::Cancelled));
    assert_eq!(expired.await, Err(StoreError::DeadlineExceeded));
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn immediate_transaction_returns_busy_before_running_the_operation() {
    let (_directory, id) = database();
    let writer = opened(id).await;
    execute(&writer, "CREATE TABLE records(id INTEGER)").await;
    let path = writer.db_id().path().to_owned();
    let (release, release_receive) = mpsc::channel();
    let (entered_send, entered) = mpsc::channel();
    let external = tokio::task::spawn_blocking(move || {
        // Fault injection only: production consumers cannot open this writer.
        let connection = Connection::open(path).unwrap();
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        entered_send.send(()).unwrap();
        release_receive.recv().unwrap();
        connection.execute_batch("ROLLBACK").unwrap();
    });
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    let job = writer
        .submit(WriteJob::new(
            JobOptions::default(),
            |_| -> Result<(), StoreError> {
                panic!("IMMEDIATE did not acquire the reservation first")
            },
        ))
        .unwrap();
    assert_eq!(job.await, Err(StoreError::Busy));
    release.send(()).unwrap();
    external.await.unwrap();
    execute(&writer, "INSERT INTO records VALUES(1)").await;
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn commit_failure_never_acknowledges_or_publishes_a_head() {
    let (_directory, id) = database();
    let writer = Registry
        .open(
            id,
            WriterOptions {
                head_seq: Some(Arc::new(|connection| {
                    Ok(connection
                        .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?)
                })),
                ..Default::default()
            },
        )
        .unwrap();
    writer.ready().await.unwrap();
    execute(&writer, "CREATE TABLE parent(id INTEGER PRIMARY KEY); CREATE TABLE records(id INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED)").await;
    let mut commits = writer.subscribe_commits();
    commits.latest();
    let failed = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute("INSERT INTO records VALUES(99)", [])?;
            Ok(())
        }))
        .unwrap();
    assert!(matches!(failed.await, Err(StoreError::Sqlite(_))));
    assert!(!commits.has_changed().unwrap());
    assert_eq!(count(&writer).await, 0);
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn acknowledgement_and_notification_wait_for_the_commit_boundary() {
    let (_directory, id) = database();
    let hold = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let held = hold.clone();
    let (entered_send, entered) = mpsc::channel();
    let (release, release_receive) = mpsc::channel();
    let release_receive = Mutex::new(release_receive);
    let writer = Registry
        .open(
            id,
            WriterOptions {
                head_seq: Some(Arc::new(move |connection| {
                    if held.load(Ordering::Acquire) {
                        entered_send.send(()).unwrap();
                        release_receive.lock().unwrap().recv().unwrap();
                    }
                    Ok(connection
                        .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?)
                })),
                ..Default::default()
            },
        )
        .unwrap();
    writer.ready().await.unwrap();
    execute(&writer, "CREATE TABLE records(id INTEGER)").await;
    let mut commits = writer.subscribe_commits();
    commits.latest();
    hold.store(true, Ordering::Release);
    let mut job = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute("INSERT INTO records VALUES(1)", [])?;
            Ok(())
        }))
        .unwrap();
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    assert!(!commits.has_changed().unwrap());
    assert!(matches!(
        job.0.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    hold.store(false, Ordering::Release);
    release.send(()).unwrap();
    job.await.unwrap();
    assert_eq!(commits.latest().unwrap().head_seq, 1);
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn commit_subscription_closes_with_owner_even_while_old_handles_remain() {
    let (_directory, id) = database();
    let writer = opened(id).await;
    let mut commits = writer.subscribe_commits();
    writer.shutdown().await.unwrap();
    assert_eq!(commits.changed().await, Err(StoreError::Closed));
    let mut late_subscriber = writer.subscribe_commits();
    assert_eq!(late_subscriber.changed().await, Err(StoreError::Closed));
}
