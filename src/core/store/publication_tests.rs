use super::*;
use crate::{registry::Registry, writer::WriterOptions, Cancellation, DbId};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Policy;
impl CurrentAuthority for Policy {
    fn acquire(
        &self,
        tx: &Transaction<'_>,
        publication: &Publication,
    ) -> Result<AuthorityPermit, AuthorityError> {
        let allowed = tx
            .query_row(
                "SELECT allowed FROM policy WHERE subject=?",
                [&publication.subject],
                |r| r.get::<_, bool>(0),
            )
            .unwrap_or(false);
        if allowed {
            Ok(AuthorityPermit::holding(()))
        } else {
            Err(AuthorityError::Rejected(Rejection::Revoked))
        }
    }
}
async fn open(path: &std::path::Path) -> WriterHandle {
    let writer = Registry
        .open(DbId::new(path).unwrap(), WriterOptions::default())
        .unwrap();
    writer.ready().await.unwrap();
    writer.submit(WriteJob::new(JobOptions::default(), |tx| {
        install(tx)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS domain(id TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS effects(id TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS policy(subject TEXT PRIMARY KEY,allowed INTEGER); INSERT OR IGNORE INTO policy VALUES('owner',1)")?;
        Ok(())
    })).unwrap().await.unwrap();
    writer
}
fn message(id: &str, target: &str) -> Publication {
    Publication {
        op_id: id.into(),
        target: target.into(),
        kind: "project".into(),
        subject: "owner".into(),
        payload: b"body".to_vec(),
    }
}
fn handler() -> Apply {
    Arc::new(|tx, p| {
        tx.execute("INSERT INTO effects VALUES(?)", [&p.op_id])?;
        Ok(())
    })
}
async fn staged(source: &WriterHandle, p: Publication) {
    assert_eq!(
        stage(source, JobOptions::default(), p.clone(), 100, move |tx| {
            tx.execute("INSERT INTO domain VALUES(?)", [&p.op_id])?;
            Ok(())
        })
        .await
        .unwrap(),
        Stage::Inserted
    );
}
async fn rows(writer: &WriterHandle, table: &'static str) -> i64 {
    writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            Ok(tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
        }))
        .unwrap()
        .await
        .unwrap()
}
async fn deliver(a: &WriterHandle, b: &WriterHandle, target: &str, now: i64) -> Delivery {
    deliver_one(
        a,
        b,
        target.into(),
        JobOptions::default(),
        Receiver {
            authority: Arc::new(Policy),
            apply: handler(),
        },
        now,
        RetryPolicy::default(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn domain_and_outbox_are_one_transaction_and_duplicate_skips_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let a = open(&directory.path().join("a")).await;
    let calls = Arc::new(AtomicUsize::new(0));
    for expected in [Stage::Inserted, Stage::Existing] {
        let calls = calls.clone();
        assert_eq!(
            stage(
                &a,
                JobOptions::default(),
                message("one", "b"),
                100,
                move |tx| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    tx.execute("INSERT INTO domain VALUES('one')", [])?;
                    Ok(())
                }
            )
            .await
            .unwrap(),
            expected
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut conflicting = message("one", "b");
    conflicting.payload = b"different".to_vec();
    assert!(
        stage(&a, JobOptions::default(), conflicting, 100, |_| panic!(
            "conflict ran domain mutation"
        ))
        .await
        .is_err()
    );
    assert!(stage(
        &a,
        JobOptions::default(),
        message("rollback", "b"),
        100,
        |tx| {
            tx.execute("INSERT INTO domain VALUES('rollback')", [])?;
            Err(StoreError::Invalid("rollback".into()))
        }
    )
    .await
    .is_err());
    assert_eq!(rows(&a, "domain").await, 1);
    assert_eq!(rows(&a, "_publication_outbox").await, 1);
    a.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancellation_before_commit_rolls_back_domain_and_outbox() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    let cancellation = Cancellation::default();
    let cancel = cancellation.clone();
    let result = stage(
        &a,
        JobOptions {
            cancellation,
            ..Default::default()
        },
        message("one", "b"),
        100,
        move |tx| {
            tx.execute("INSERT INTO domain VALUES('one')", [])?;
            cancel.cancel();
            Ok(())
        },
    )
    .await;
    assert_eq!(result, Err(StoreError::Cancelled));
    assert_eq!(rows(&a, "domain").await, 0);
    assert_eq!(rows(&a, "_publication_outbox").await, 0);
    a.shutdown().await.unwrap();
}

#[tokio::test]
async fn crash_after_target_commit_recovers_without_repeating_effect() {
    let d = tempfile::tempdir().unwrap();
    let ap = d.path().join("a");
    let bp = d.path().join("b");
    let a = open(&ap).await;
    let b = open(&bp).await;
    staged(&a, message("one", "b")).await;
    let policy = RetryPolicy::default();
    let claim = claim_next(&a, JobOptions::default(), "b".into(), 100, policy)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        apply_claim(
            &b,
            JobOptions::default(),
            claim,
            Arc::new(Policy),
            handler(),
            100
        )
        .await
        .unwrap(),
        Delivery::Applied
    );
    // No source completion: simulate daemon death / restart at the uncertainty boundary.
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
    let a = open(&ap).await;
    let b = open(&bp).await;
    assert_eq!(
        deliver(&a, &b, "b", 100 + policy.lease_ms).await,
        Delivery::AlreadyApplied
    );
    assert_eq!(rows(&b, "effects").await, 1);
    assert_eq!(
        status(&a, JobOptions::default(), "one".into())
            .await
            .unwrap()
            .unwrap()
            .state,
        "delivered"
    );
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_effect_rolls_back_inbox_and_retries_then_dead_letters() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    let b = open(&d.path().join("b")).await;
    staged(&a, message("one", "b")).await;
    let fail: Apply = Arc::new(|tx, _| {
        tx.execute("INSERT INTO effects VALUES('partial')", [])?;
        Err(StoreError::Busy)
    });
    let policy = RetryPolicy {
        max_attempts: 2,
        ..Default::default()
    };
    assert_eq!(
        deliver_one(
            &a,
            &b,
            "b".into(),
            JobOptions::default(),
            Receiver {
                authority: Arc::new(Policy),
                apply: fail.clone()
            },
            100,
            policy
        )
        .await
        .unwrap(),
        Delivery::Retrying
    );
    assert_eq!(rows(&b, "effects").await, 0);
    assert_eq!(rows(&b, "_publication_inbox").await, 0);
    assert_eq!(deliver(&a, &b, "b", 101).await, Delivery::Idle);
    assert_eq!(
        deliver_one(
            &a,
            &b,
            "b".into(),
            JobOptions::default(),
            Receiver {
                authority: Arc::new(Policy),
                apply: fail
            },
            350,
            policy
        )
        .await
        .unwrap(),
        Delivery::DeadLetter(Rejection::RetryExhausted)
    );
    let status = status(&a, JobOptions::default(), "one".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.state, "dead_letter");
    assert_eq!(status.attempts, 2);
    assert_eq!(status.error_code.as_deref(), Some("retry_exhausted"));
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test]
async fn revocation_queued_before_effect_is_seen_on_targets_current_writer() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    let b = open(&d.path().join("b")).await;
    staged(&a, message("one", "b")).await;
    let claim = claim_next(
        &a,
        JobOptions::default(),
        "b".into(),
        100,
        RetryPolicy::default(),
    )
    .await
    .unwrap()
    .unwrap();
    let revoke = b
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute("UPDATE policy SET allowed=0", [])?;
            Ok(())
        }))
        .unwrap();
    assert_eq!(
        apply_claim(
            &b,
            JobOptions::default(),
            claim.clone(),
            Arc::new(Policy),
            handler(),
            100
        )
        .await
        .unwrap(),
        Delivery::DeadLetter(Rejection::Revoked)
    );
    revoke.await.unwrap();
    assert_eq!(rows(&b, "effects").await, 0);
    finish(
        &a,
        JobOptions::default(),
        claim,
        Delivery::DeadLetter(Rejection::Revoked),
        100,
        RetryPolicy::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        status(&a, JobOptions::default(), "one".into())
            .await
            .unwrap()
            .unwrap()
            .error_code
            .as_deref(),
        Some("revoked")
    );
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test]
async fn stale_source_completion_cannot_acknowledge_a_new_claim() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    staged(&a, message("one", "b")).await;
    let policy = RetryPolicy::default();
    let first = claim_next(&a, JobOptions::default(), "b".into(), 100, policy)
        .await
        .unwrap()
        .unwrap();
    let next = claim_next(
        &a,
        JobOptions::default(),
        "b".into(),
        100 + policy.lease_ms,
        policy,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        finish(
            &a,
            JobOptions::default(),
            first,
            Delivery::Applied,
            100,
            policy
        )
        .await
        .unwrap(),
        Delivery::LostClaim
    );
    assert_eq!(
        status(&a, JobOptions::default(), "one".into())
            .await
            .unwrap()
            .unwrap()
            .attempts,
        next.attempt
    );
    a.shutdown().await.unwrap();
}

#[tokio::test]
async fn dedupe_binding_and_compaction_floor_prevent_blind_reexecution() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    let b = open(&d.path().join("b")).await;
    staged(&a, message("one", "b")).await;
    let claim = claim_next(
        &a,
        JobOptions::default(),
        "b".into(),
        100,
        RetryPolicy::default(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        apply_claim(
            &b,
            JobOptions::default(),
            claim.clone(),
            Arc::new(Policy),
            handler(),
            100
        )
        .await
        .unwrap(),
        Delivery::Applied
    );
    let mut conflicting = claim.clone();
    conflicting.publication.subject = "other".into();
    // Grant the test's other subject through the *current* target policy.
    b.submit(WriteJob::new(JobOptions::default(), |tx| {
        tx.execute("INSERT INTO policy VALUES('other',1)", [])?;
        Ok(())
    }))
    .unwrap()
    .await
    .unwrap();
    assert_eq!(
        apply_claim(
            &b,
            JobOptions::default(),
            conflicting,
            Arc::new(Policy),
            handler(),
            100
        )
        .await
        .unwrap(),
        Delivery::DeadLetter(Rejection::Conflict)
    );
    advance_horizon(&b, JobOptions::default(), 101)
        .await
        .unwrap();
    assert_eq!(
        compact_receipts(&b, JobOptions::default()).await.unwrap(),
        1
    );
    assert_eq!(rows(&b, "_publication_inbox").await, 0);
    assert_eq!(
        apply_claim(
            &b,
            JobOptions::default(),
            claim,
            Arc::new(Policy),
            handler(),
            102
        )
        .await
        .unwrap(),
        Delivery::DeadLetter(Rejection::Expired)
    );
    assert_eq!(rows(&b, "effects").await, 1);
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test]
async fn opposite_directions_make_progress_without_mutual_queue_waits() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    let b = open(&d.path().join("b")).await;
    staged(&a, message("a-to-b", "b")).await;
    staged(&b, message("b-to-a", "a")).await;
    let (ab, ba) = tokio::join!(deliver(&a, &b, "b", 100), deliver(&b, &a, "a", 100));
    assert_eq!(ab, Delivery::Applied);
    assert_eq!(ba, Delivery::Applied);
    assert_eq!(rows(&a, "effects").await, 1);
    assert_eq!(rows(&b, "effects").await, 1);
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

struct PermitDrop(Arc<AtomicBool>);
impl Drop for PermitDrop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
struct FencedAuthority(Arc<AtomicBool>);
impl CurrentAuthority for FencedAuthority {
    fn acquire(
        &self,
        _: &Transaction<'_>,
        _: &Publication,
    ) -> Result<AuthorityPermit, AuthorityError> {
        self.0.store(true, Ordering::Release);
        Ok(AuthorityPermit::holding(PermitDrop(self.0.clone())))
    }
}
#[tokio::test]
async fn authority_permit_survives_until_target_commit_and_drops_before_source_receipt() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    let held = Arc::new(AtomicBool::new(false));
    let checking = Arc::new(AtomicBool::new(false));
    let flag = held.clone();
    let check = checking.clone();
    let b = Registry
        .open(
            DbId::new(d.path().join("b")).unwrap(),
            WriterOptions {
                head_seq: Some(Arc::new(move |_| {
                    if check.load(Ordering::Acquire) {
                        assert!(
                            flag.load(Ordering::Acquire),
                            "authority released before commit"
                        );
                    }
                    Ok(0)
                })),
                ..Default::default()
            },
        )
        .unwrap();
    b.ready().await.unwrap();
    b.submit(WriteJob::new(JobOptions::default(), install))
        .unwrap()
        .await
        .unwrap();
    staged(&a, message("one", "b")).await;
    checking.store(true, Ordering::Release);
    assert_eq!(
        deliver_one(
            &a,
            &b,
            "b".into(),
            JobOptions::default(),
            Receiver {
                authority: Arc::new(FencedAuthority(held.clone())),
                apply: Arc::new(|_, _| Ok(()))
            },
            100,
            RetryPolicy::default()
        )
        .await
        .unwrap(),
        Delivery::Applied
    );
    assert!(!held.load(Ordering::Acquire));
    checking.store(false, Ordering::Release);
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test]
async fn claim_queries_use_ordered_indexes_and_receipt_compaction_is_bounded() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    a.submit(WriteJob::new(JobOptions::default(), |tx| {
        for (sql, index) in [
            (PENDING_SQL, "_publication_pending"),
            (LEASED_SQL, "_publication_leased"),
        ] {
            let mut explain = tx.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
            let details = explain
                .query_map(params!["b", 100], |row| row.get::<_, String>(3))?
                .collect::<Result<Vec<_>, _>>()?;
            assert!(details.iter().any(|row| row.contains(index)), "{details:?}");
            assert!(
                !details.iter().any(|row| row.contains("TEMP B-TREE")),
                "{details:?}"
            );
        }
        for id in 0..600 {
            tx.execute(
                "INSERT INTO _publication_inbox VALUES(?,'b','fixture','owner',X'00',1,1)",
                [id.to_string()],
            )?;
        }
        Ok(())
    }))
    .unwrap()
    .await
    .unwrap();
    advance_horizon(&a, JobOptions::default(), 2).await.unwrap();
    assert_eq!(
        compact_receipts(&a, JobOptions::default()).await.unwrap(),
        256
    );
    assert_eq!(rows(&a, "_publication_inbox").await, 344);
    a.shutdown().await.unwrap();
}

struct BusyAuthority(Arc<AtomicBool>);
impl CurrentAuthority for BusyAuthority {
    fn acquire(
        &self,
        tx: &Transaction<'_>,
        publication: &Publication,
    ) -> Result<AuthorityPermit, AuthorityError> {
        if self.0.load(Ordering::Acquire) {
            Err(AuthorityError::Unavailable)
        } else {
            Policy.acquire(tx, publication)
        }
    }
}
#[tokio::test]
async fn unavailable_authority_retries_without_waiting_or_marking_a_permanent_denial() {
    let d = tempfile::tempdir().unwrap();
    let a = open(&d.path().join("a")).await;
    let b = open(&d.path().join("b")).await;
    staged(&a, message("one", "b")).await;
    let busy = Arc::new(AtomicBool::new(true));
    let receiver = || Receiver {
        authority: Arc::new(BusyAuthority(busy.clone())),
        apply: handler(),
    };
    assert_eq!(
        deliver_one(
            &a,
            &b,
            "b".into(),
            JobOptions::default(),
            receiver(),
            100,
            RetryPolicy::default()
        )
        .await
        .unwrap(),
        Delivery::Retrying
    );
    assert_eq!(rows(&b, "effects").await, 0);
    busy.store(false, Ordering::Release);
    assert_eq!(
        deliver_one(
            &a,
            &b,
            "b".into(),
            JobOptions::default(),
            receiver(),
            350,
            RetryPolicy::default()
        )
        .await
        .unwrap(),
        Delivery::Applied
    );
    assert_eq!(rows(&b, "effects").await, 1);
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}
