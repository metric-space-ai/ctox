use super::*;
#[path = "../business_os/command_projection_reads.rs"]
mod native_projection_reads;

// Frozen release source copied into actual loader inputs; no registry seeding.
fn command_release_fixture() -> crate::import::base_release::ReleasePlan {
    let release: crate::schema::BaseReleaseInput =
        serde_json::from_str(include_str!("../../../tests/sync-v4-m2/base-release.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for path in ["tools/manifest/out", "manifests", "schemas"] {
        std::fs::create_dir_all(root.join(path)).unwrap();
    }
    std::fs::write(
        root.join("tools/manifest/out/ownership.json"),
        serde_json::to_vec(&release.ownership).unwrap(),
    )
    .unwrap();
    for manifest in release.manifests {
        let app = manifest["app_id"].as_str().unwrap();
        let file = if app == "ctox-platform" {
            root.join("platform.json")
        } else {
            root.join(format!("manifests/{app}.manifest.json"))
        };
        std::fs::write(file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }
    // This is deliberately the pre-v4 shape, not the canonical test expectation.
    std::fs::write(root.join("schemas/ctox-platform.collections.schema.json"),
        r#"{"_base_release":"main@4794af03d","collections":{"business_commands":{"schema":{"version":2,"type":"object","properties":{"command_type":{"type":"string"}},"required":["command_type"]}}}}"#).unwrap();
    crate::import::base_release::load(
        root,
        &root.join("platform.json"),
        &crate::schema::BaseReleaseDefaults {
            app_ver: "platform-7".into(),
            client_limit: 1000,
            byte_budget: 16 * 1024 * 1024,
            rights_ver: 1,
        },
    )
    .unwrap()
}
struct NoLegacyCommandMigration;
impl crate::import::base_release::MigrationEvaluator for NoLegacyCommandMigration {
    fn migrate(&self, _: &str, _: &Value, _: Value) -> Result<Value, StoreError> {
        Err(StoreError::Invalid(
            "legacy command migration must be supplied by publisher".into(),
        ))
    }
}

#[tokio::test]
async fn command_release_change_retires_a_live_fence_and_old_metadata_requires_cas() {
    let (_dir, writer, readers, ingress) = commands().await;
    let created = send(
        &ingress,
        "client",
        command(1, 950, None, "queued", None, Value::Null),
        NOW,
    )
    .await;
    assert_eq!(created.status, WriteStatus::Accepted);
    // Retained metadata from the previous native P9 source, not caller claims.
    writer.submit(WriteJob::new(JobOptions::default(),|tx|{
        tx.execute_batch("DROP TABLE _command_leases; CREATE TABLE _command_leases(command_id TEXT PRIMARY KEY,participant TEXT NOT NULL,fence INTEGER NOT NULL,expires_at_core INTEGER NOT NULL,last_now_ms INTEGER NOT NULL) WITHOUT ROWID; CREATE TABLE fenced_effect(value INTEGER); INSERT INTO fenced_effect VALUES(0)")?;
        tx.execute("INSERT INTO _command_leases VALUES('command-1','executor-a',7,?1,?2)",params![NOW+30_000,NOW])?;Ok(())
    })).unwrap().await.unwrap();
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            assert_eq!(
                check_executor_fence(tx, "executor-a", "command-1", Seq(7), NOW)?
                    .unwrap_err()
                    .reason,
                "stale_fence"
            );
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let renewed = ingress
        .claim_command("executor-a".into(), "command-1".into(), Some(Seq(7)), NOW)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(renewed.fence, Seq(8));
    // Native publication changes the authoritative release descriptor. A live
    // old lease must not authorize work against that new executable contract.
    writer.submit(WriteJob::new(JobOptions::default(),|tx|{
        tx.execute("UPDATE _collections SET app_ver='2' WHERE name='business_commands' AND state='active'",[])?;
        let rejected=with_command_fence(tx,"executor-a","command-1",Seq(8),NOW,&Policy,|tx|{
            tx.execute("UPDATE fenced_effect SET value=99",[])?;Ok(())
        })?.unwrap_err();
        assert_eq!((rejected.code.as_str(),rejected.reason.as_str()),("conflict","stale_fence"));Ok(())
    })).unwrap().await.unwrap();
    let next = ingress
        .claim_command("executor-b".into(), "command-1".into(), Some(Seq(8)), NOW)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.fence, Seq(9));
    readers
        .read(|tx| {
            assert_eq!(
                tx.query_row("SELECT value FROM fenced_effect", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            Ok(())
        })
        .await
        .unwrap();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_kernel_p5_wire_replays_the_native_canonical_document() {
    use crate::change_log::subscription::{protocol::ClientMessage, Subscription, SubscriptionKey};
    use std::io::{BufRead, BufReader, Write as IoWrite};
    use std::process::Stdio;
    let (_dir, writer, readers) = runtime().await;
    let plan = command_release_fixture();
    crate::import::base_release::publish(&writer, &plan, &NoLegacyCommandMigration, NOW as i64)
        .await
        .unwrap();
    let schema = readers
        .read(|tx| {
            let active = crate::schema::active_collection(tx, "business_commands")?.unwrap();
            assert_eq!(active.definition.app_id, "ctox-platform");
            assert_eq!(active.definition.schema_ver, 3);
            assert_eq!(active.definition.app_ver, "platform-7");
            Ok(active.definition.schema_json)
        })
        .await
        .unwrap();
    assert_eq!(schema, command_document_schema());
    assert_eq!(
        plan.manifests
            .iter()
            .find(|m| m["app_id"] == "ctox-platform")
            .unwrap()["collections"]["business_commands"]["schema"],
        schema
    );
    let before = readers.read(|tx| change_log::head(tx)).await.unwrap();
    crate::import::base_release::publish(&writer, &plan, &NoLegacyCommandMigration, NOW as i64)
        .await
        .unwrap();
    let after = readers.read(|tx| change_log::head(tx)).await.unwrap();
    assert_eq!(
        before.head_seq, after.head_seq,
        "publication replay must not mint changes"
    );
    let epoch = readers
        .read(|tx| Ok(change_log::head(tx)?.epoch))
        .await
        .unwrap();
    let ingress = OrderedWriter::new(writer.clone(), Arc::new(Policy));
    let signer = crate::change_log::subscription::tests::TestInstanceKey::new();
    let key = SubscriptionKey {
        participant: "alice".into(),
        collection: "business_commands".into(),
        scope_hash: "rights:1".into(),
        gen: 1,
    };
    let mut live = Subscription::new(readers.clone(), &writer, key.clone(), None).unwrap();
    live.next_page(100, Arc::new(Policy), &signer, || NOW)
        .await
        .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let kernel = root
        .join("src/apps/business-os/kernel")
        .canonicalize()
        .unwrap();
    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = OwnedChild(
        Command::new("node")
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("command_e2e.mjs"))
            .env("KERNEL_URL", format!("file://{}", kernel.display()))
            .env(
                "COMMAND_CONFIG",
                json!({"schema":schema,"epoch":epoch}).to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let mut input = child.0.stdin.take().unwrap();
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    let mut line = String::new();
    let mut executor_seq = 0;
    let mut first = None;
    let mut measured = None;
    let mut projection_us = Vec::new();
    loop {
        line.clear();
        if output.read_line(&mut line).unwrap() == 0 {
            break;
        }
        let request: Value = serde_json::from_str(&line).expect("component stdout is JSON");
        if request["type"] == "measured" {
            measured = Some(request);
            continue;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let value = match request["type"].as_str().unwrap() {
            "write" => {
                let ClientMessage::Write(write) =
                    serde_json::from_value(request["message"].clone())
                        .expect("real Kernel wire must parse strict native Write")
                else {
                    panic!("not write")
                };
                assert_eq!(write.write.schema_ver, 3);
                assert_eq!(write.write.app_ver, "platform-7");
                let mut doc = write.write.body.clone().unwrap();
                let id = write.write.id.clone();
                let result = ingress.submit("alice".into(), write, now).await.unwrap();
                assert_eq!(result.results.len(), 1);
                let receipt = result.results[0].clone();
                if receipt.result.status == WriteStatus::Accepted {
                    let lease = ingress
                        .claim_command("executor-a".into(), id.clone(), None, now)
                        .await
                        .unwrap()
                        .unwrap();
                    executor_seq += 1;
                    doc["status"] = json!("done");
                    doc["executor_fence"] = json!(lease.fence);
                    doc["result"] = json!({"value":doc["payload"]["value"]});
                    let terminal = OrderedWrite {
                        client_seq: Seq(executor_seq),
                        write: Write {
                            op_id: op_at(now, 5000 + executor_seq as u16),
                            prev_op_id: None,
                            collection: "business_commands".into(),
                            id: id.clone(),
                            base_rev: receipt.result.rev.clone(),
                            base_seq: receipt.result.seq,
                            gen: 1,
                            schema_ver: 3,
                            app_ver: "platform-7".into(),
                            op: WriteOp::Put,
                            body: Some(doc.clone()),
                        },
                    };
                    let done = ingress
                        .submit("executor-a".into(), terminal, now)
                        .await
                        .unwrap();
                    assert_eq!(done.results[0].result.status, WriteStatus::Accepted);
                    if first.is_none() {
                        writer
                            .submit(WriteJob::new(JobOptions::default(), |tx| {
                                activate_command_projections(tx, 1).map_err(Into::into)
                            }))
                            .unwrap()
                            .await
                            .unwrap();
                        first = Some(doc);
                    }
                    let started = std::time::Instant::now();
                    let status = readers
                        .read(move |tx| native_projection_reads::projected_command_status(tx, &id))
                        .await
                        .unwrap();
                    projection_us.push(started.elapsed().as_secs_f64() * 1e6);
                    assert_eq!(status.as_deref(), Some("completed"));
                } else {
                    assert_eq!(
                        receipt.result.status,
                        WriteStatus::Merged,
                        "native receipt: {:?}",
                        receipt
                    );
                    assert!(receipt.requires_page);
                }
                let page = live
                    .next_page(100, Arc::new(Policy), &signer, || now)
                    .await
                    .unwrap();
                json!({"receipt":CoreMessage::WriteResult(receipt),"page":page})
            }
            "subscribe" => {
                let ClientMessage::Subscribe(sub) =
                    serde_json::from_value(request["message"].clone())
                        .expect("real Kernel wire must parse strict native Subscribe")
                else {
                    panic!("not subscribe")
                };
                assert_eq!(sub.collection, key.collection);
                assert_eq!(sub.scope_hash, key.scope_hash);
                assert_eq!(sub.gen, key.gen);
                let mut replay =
                    Subscription::new(readers.clone(), &writer, key.clone(), sub.checkpoint)
                        .unwrap();
                serde_json::to_value(
                    replay
                        .next_page(100, Arc::new(Policy), &signer, || now)
                        .await
                        .unwrap(),
                )
                .unwrap()
            }
            "first_command" => first.clone().unwrap(),
            _ => panic!("unknown component request"),
        };
        writeln!(
            input,
            "{}",
            json!({"request":request["request"],"value":value})
        )
        .unwrap();
        input.flush().unwrap();
    }
    drop(input);
    assert!(
        child.0.wait().unwrap().success(),
        "real Kernel/native command harness failed"
    );
    let mut measured = measured.expect("missing actual runtime measurement");
    projection_us.sort_by(|a, b| a.total_cmp(b));
    measured["projection_read_p50_us"] = json!(projection_us[projection_us.len() / 2]);
    measured["projection_read_p99_us"] =
        json!(projection_us[(projection_us.len() * 99).div_ceil(100) - 1]);
    println!("P9_M8_COMPONENT {}", measured);
    assert!(measured["verified_pages"].as_u64().unwrap() > 0);
    assert!(measured["canonical_replays"].as_u64().unwrap() > 0);
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
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
        running.rev.clone(),
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
    let mut callback = command(
        4,
        905,
        running.rev,
        "running",
        Some(lease.fence),
        Value::Null,
    );
    callback.write.body.as_mut().unwrap()["on_complete"] = json!("redirect.callback");
    let callback = send(&ingress, "executor-a", callback, NOW).await;
    assert_eq!(callback.reason.as_deref(), Some("idempotency_conflict"));
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
    let replay_batch = ingress
        .submit(
            "client".into(),
            command(2, 914, None, "queued", None, Value::Null),
            NOW,
        )
        .await
        .unwrap();
    assert!(replay_batch.results[0].requires_page);
    let replay = replay_batch.results[0].result.clone();
    assert_eq!(replay.status, WriteStatus::Merged);
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
