use super::*;
#[path = "../business_os/command_projection_reads.rs"]
mod native_projection_reads;
#[tokio::test]
async fn command_projection_feeds_native_readers_atomically_without_legacy_fallback() {
    let (_dir, writer, readers, ingress) = commands().await;
    writer.submit(WriteJob::new(JobOptions::default(),|tx| {
        tx.execute_batch("CREATE TABLE business_commands(command_id TEXT PRIMARY KEY,module TEXT,status TEXT); INSERT INTO business_commands VALUES('command-1','poison-module','failed');")?;
        Ok(())
    })).unwrap().await.unwrap();
    let (created, lease) = begin(&ingress).await;
    let done = send(
        &ingress,
        "executor-a",
        command(
            1,
            930,
            created.rev,
            "done",
            Some(lease.fence),
            json!({"value":1}),
        ),
        NOW,
    )
    .await;
    readers
        .read(|tx| {
            assert_eq!(
                native_projection_reads::projected_command_status(tx, "command-1")?,
                Some("failed".into())
            );
            Ok(())
        })
        .await
        .unwrap();
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            activate_command_projections(tx, 1).map_err(Into::into)
        }))
        .unwrap()
        .await
        .unwrap();
    let standalone = rusqlite::Connection::open_with_flags(
        _dir.path().join("sync.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        native_projection_reads::projected_command_status(&standalone, "command-1").unwrap(),
        Some("completed".into())
    );
    assert!(standalone.is_autocommit());
    drop(standalone);
    let expected = done.clone();
    readers
        .read(move |tx| {
            let projection = read_command_projection(tx, "command-1")?.unwrap().unwrap();
            assert_eq!(projection.source_rev, expected.rev.unwrap());
            assert_eq!(projection.source_seq, expected.seq.unwrap());
            let rev: String = tx.query_row(
                "SELECT rev FROM c_business_commands WHERE id='command-1'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(projection.source_rev, rev);
            assert_eq!(
                native_projection_reads::projected_command_module(tx, "command-1")?,
                Some("fixture".into())
            );
            assert_eq!(
                native_projection_reads::projected_command_status(tx, "command-1")?,
                Some("completed".into())
            );
            let start = std::time::Instant::now();
            for _ in 0..1000 {
                std::hint::black_box(tx.query_row(
                    "SELECT status FROM business_commands WHERE command_id=?1",
                    ["command-1"],
                    |r| r.get::<_, String>(0),
                )?);
            }
            eprintln!(
                "P9_PROJECTION_MEASUREMENT legacy_status_lookup_ns_per_op={}",
                start.elapsed().as_nanos() / 1000
            );
            let start = std::time::Instant::now();
            for _ in 0..1000 {
                std::hint::black_box(native_projection_reads::projected_command_status(
                    tx,
                    "command-1",
                )?);
            }
            eprintln!(
                "P9_PROJECTION_MEASUREMENT status_lookup_ns_per_op={}",
                start.elapsed().as_nanos() / 1000
            );
            Ok(())
        })
        .await
        .unwrap();
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute(
                "DELETE FROM _command_projections WHERE command_id='command-1'",
                [],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let uncovered = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            activate_command_projections(tx, 1).map_err(Into::into)
        }))
        .unwrap()
        .await;
    assert!(uncovered.is_err());
    readers
        .read(|tx| {
            assert!(native_projection_reads::projected_command_module(tx, "command-1")?.is_none());
            assert!(native_projection_reads::projected_command_status(tx, "command-1")?.is_none());
            Ok(())
        })
        .await
        .unwrap();
    let rebuilt = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            backfill_command_projections(tx, 1, None, 10).map_err(Into::into)
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(rebuilt, (Some("command-1".into()), true));
    readers
        .read(|tx| {
            assert_eq!(
                native_projection_reads::projected_command_status(tx, "command-1")?,
                Some("completed".into())
            );
            Ok(())
        })
        .await
        .unwrap();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
async fn commands() -> (
    tempfile::TempDir,
    crate::writer::WriterHandle,
    ReadPool,
    OrderedWriter,
) {
    let (dir, writer, readers) = runtime().await;
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            crate::schema::create_collection(tx, &definition("business_commands"))?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let ingress = OrderedWriter::new(writer.clone(), Arc::new(Policy));
    (dir, writer, readers, ingress)
}
fn command(
    seq: u64,
    n: u16,
    base: Option<String>,
    status: &str,
    fence: Option<Seq>,
    result: Value,
) -> OrderedWrite {
    let mut w = put(
        seq,
        n,
        "command-1",
        base,
        json!({"id":"command-1","app_id":"fixture","type":"task","payload":{"instruction":"do work"},"created_at":NOW,"status":status,"result":result}),
    );
    w.write.collection = "business_commands".into();
    if w.write.body.as_ref().unwrap()["result"].is_null() {
        w.write
            .body
            .as_mut()
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("result");
    }
    if let Some(fence) = fence {
        w.write.body.as_mut().unwrap()["executor_fence"] = json!(fence);
    }
    w
}
async fn send(ingress: &OrderedWriter, owner: &str, w: OrderedWrite, now: u64) -> WriteResult {
    ingress
        .submit(owner.into(), w, now)
        .await
        .unwrap()
        .results
        .remove(0)
        .result
}
async fn begin(ingress: &OrderedWriter) -> (WriteResult, CommandLease) {
    let created = send(
        ingress,
        "client",
        command(1, 900, None, "queued", None, Value::Null),
        NOW,
    )
    .await;
    assert_eq!(created.status, WriteStatus::Accepted);
    let denied = ingress
        .claim_command("client".into(), "command-1".into(), None, NOW)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(denied.code, "forbidden");
    let lease = ingress
        .claim_command("executor-a".into(), "command-1".into(), None, NOW)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease.fence, Seq(1));
    assert_eq!(lease.participant, "executor-a");
    (created, lease)
}
#[tokio::test]
async fn command_backward_transition_is_rejected_in_the_ordered_writer() {
    let (_dir, writer, readers, ingress) = commands().await;
    let (created, lease) = begin(&ingress).await;
    let running = send(
        &ingress,
        "executor-a",
        command(
            1,
            901,
            created.rev,
            "running",
            Some(lease.fence),
            Value::Null,
        ),
        NOW,
    )
    .await;
    assert_eq!(running.status, WriteStatus::Accepted);
    let backwards = send(
        &ingress,
        "executor-a",
        command(
            2,
            902,
            running.rev.clone(),
            "accepted",
            Some(lease.fence),
            Value::Null,
        ),
        NOW,
    )
    .await;
    assert_eq!(backwards.reason.as_deref(), Some("invalid_transition"));
    assert_eq!(backwards.code.as_deref(), Some("conflict"));
    let mut changed = command(
        3,
        903,
        running.rev,
        "running",
        Some(lease.fence),
        Value::Null,
    );
    changed.write.body.as_mut().unwrap()["payload"] = json!({"instruction":"different work"});
    let changed = send(&ingress, "executor-a", changed, NOW).await;
    assert_eq!(changed.reason.as_deref(), Some("idempotency_conflict"));
    let mut poison = command(1, 904, None, "queued", None, Value::Null);
    poison.write.id = "poison-command".into();
    poison.write.body.as_mut().unwrap()["id"] = json!("poison-command");
    poison.write.body.as_mut().unwrap()["execution_task_id"] = json!("foreign-task");
    let poison = send(&ingress, "attacker", poison, NOW).await;
    assert_eq!(poison.reason.as_deref(), Some("invalid_transition"));
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[tokio::test]
async fn command_terminal_outcome_commits_once_and_command_id_deduplicates() {
    let (_dir, writer, readers, ingress) = commands().await;
    let (created, lease) = begin(&ingress).await;
    let done = send(
        &ingress,
        "executor-a",
        command(
            1,
            911,
            created.rev,
            "done",
            Some(lease.fence),
            json!({"value":1}),
        ),
        NOW,
    )
    .await;
    assert_eq!(done.status, WriteStatus::Accepted);
    let duplicate = send(
        &ingress,
        "executor-a",
        command(
            2,
            912,
            done.rev.clone(),
            "done",
            Some(lease.fence),
            json!({"value":1}),
        ),
        NOW,
    )
    .await;
    assert_eq!(duplicate.seq, done.seq);
    assert_eq!(duplicate.rev, done.rev);
    let changed = send(
        &ingress,
        "executor-a",
        command(
            3,
            913,
            done.rev.clone(),
            "done",
            Some(lease.fence),
            json!({"value":2}),
        ),
        NOW,
    )
    .await;
    assert_eq!(changed.reason.as_deref(), Some("command_terminal"));
    let replay = send(
        &ingress,
        "client",
        command(2, 914, None, "queued", None, Value::Null),
        NOW,
    )
    .await;
    assert_eq!(replay.seq, done.seq);
    assert_eq!(replay.rev, done.rev);
    let body: Value = readers
        .read(|tx| {
            let s: String = tx.query_row(
                "SELECT data FROM c_business_commands WHERE id='command-1'",
                [],
                |r| r.get(0),
            )?;
            Ok(serde_json::from_str(&s).unwrap())
        })
        .await
        .unwrap();
    assert_eq!(body["status"], "done");
    assert_eq!(body["result"]["value"], 1);
    let mut removal = command(3, 915, done.rev, "queued", None, Value::Null);
    removal.write.op = WriteOp::Delete;
    removal.write.body = None;
    let removed = send(&ingress, "client", removal, NOW).await;
    assert_eq!(removed.status, WriteStatus::Accepted);
    readers.read(|tx| {
        let tombstone:(bool,Option<String>)=tx.query_row("SELECT deleted,data FROM c_business_commands WHERE id='command-1'",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        assert_eq!(tombstone,(true,None));
        let bodies:u64=tx.query_row("SELECT count(*) FROM _revs WHERE collection='business_commands' AND body IS NOT NULL",[],|r|r.get(0))?;
        assert_eq!(bodies,0);
        Ok(())
    }).await.unwrap();
    let resurrect = send(
        &ingress,
        "client",
        command(4, 916, None, "queued", None, Value::Null),
        NOW,
    )
    .await;
    assert_eq!(resurrect.reason.as_deref(), Some("command_terminal"));
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[tokio::test]
async fn command_stale_fence_is_a_typed_conflict_after_takeover() {
    let (_dir, writer, readers, ingress) = commands().await;
    let (created, lease) = begin(&ingress).await;
    let busy = ingress
        .claim_command(
            "executor-b".into(),
            "command-1".into(),
            Some(lease.fence),
            NOW + 1,
        )
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(busy.reason, "stale_fence");
    let saved = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            command_lease(tx, "executor-b", "command-1", &Policy).map_err(Into::into)
        }))
        .unwrap()
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(saved, lease);
    let next = ingress
        .claim_command(
            "executor-b".into(),
            "command-1".into(),
            Some(lease.fence),
            NOW + 30_001,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.fence, Seq(2));
    let effect=writer.submit(WriteJob::new(JobOptions::default(),move |tx| {
        with_command_fence(tx,"executor-a","command-1",lease.fence,NOW+30_001,&Policy,|tx| {
            tx.execute("INSERT INTO c_items(id,rev,schema_ver,data,deleted) VALUES('stale-effect','bad',1,'{}',0)",[])?;
            Ok(())
        }).map_err(Into::into)
    })).unwrap().await.unwrap().unwrap_err();
    assert_eq!(effect.reason, "stale_fence");
    let absent: bool = readers
        .read(|tx| {
            tx.query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM c_items WHERE id='stale-effect')",
                [],
                |r| r.get(0),
            )
        })
        .await
        .unwrap();
    assert!(absent);
    let stale = send(
        &ingress,
        "executor-a",
        command(
            1,
            920,
            created.rev.clone(),
            "running",
            Some(lease.fence),
            Value::Null,
        ),
        NOW + 30_001,
    )
    .await;
    assert_eq!(stale.reason.as_deref(), Some("stale_fence"));
    assert_eq!(stale.code.as_deref(), Some("conflict"));
    let encoded = serde_json::to_value(CoreMessage::WriteResult(OrderedWriteResult {
        client_seq: Seq(1),
        requires_page: false,
        result: stale.clone(),
    }))
    .unwrap();
    assert_eq!(encoded["code"], "conflict");
    assert_eq!(encoded["reason"], "stale_fence");
    let current = send(
        &ingress,
        "executor-b",
        command(
            1,
            921,
            created.rev,
            "running",
            Some(next.fence),
            Value::Null,
        ),
        NOW + 30_001,
    )
    .await;
    assert_eq!(current.status, WriteStatus::Accepted);
    let replay = send(
        &ingress,
        "executor-a",
        command(1, 920, None, "queued", None, Value::Null),
        NOW + 30_002,
    )
    .await;
    assert_eq!(replay, stale);
    writer.submit(WriteJob::new(JobOptions::default(),|tx| {
        tx.execute("UPDATE c_business_commands SET data=json_set(data,'$.status','invalid-import-state') WHERE id='command-1'",[])?;
        Ok(())
    })).unwrap().await.unwrap();
    let invalid = ingress
        .claim_command(
            "executor-b".into(),
            "command-1".into(),
            Some(next.fence),
            NOW + 30_002,
        )
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(invalid.code, "invalid");
    assert_eq!(invalid.reason, "invalid_command");
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[test]
fn command_transitions_are_the_fixture_projection_used_by_the_kernel() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/business-command-lifecycle-v2.json"
    ))
    .unwrap();
    let map = |phase: &str| match phase {
        "accepted" => "accepted",
        "running" | "awaiting_review" | "validating" => "running",
        _ => "queued",
    };
    let terminal = ["done", "failed", "cancelled"];
    let mut projected = std::collections::BTreeSet::new();
    for (from, tos) in fixture["allowedExecutionTransitions"].as_object().unwrap() {
        if from == "terminal" {
            continue;
        }
        for to in tos.as_array().unwrap() {
            let to = to.as_str().unwrap();
            if to == "terminal" {
                for end in terminal {
                    projected.insert((map(from), end));
                }
            } else {
                projected.insert((map(from), map(to)));
            }
        }
    }
    projected.insert(("queued", "rejected"));
    for end in ["done", "failed", "cancelled", "rejected"] {
        projected.insert((end, end));
    }
    for from in [
        "queued",
        "accepted",
        "running",
        "done",
        "failed",
        "cancelled",
        "rejected",
    ] {
        for to in [
            "queued",
            "accepted",
            "running",
            "done",
            "failed",
            "cancelled",
            "rejected",
        ] {
            assert_eq!(
                super::super::command::transition(from, to),
                projected.contains(&(from, to)),
                "{from} -> {to}"
            );
        }
    }
}
