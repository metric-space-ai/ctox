// P9 contract tests use the real WAL Store writer, not a simulated executor ledger.
#[path = "command_contract_tests.rs"]
mod command_tests;
use super::*;
use crate::change_log::{
    self,
    subscription::{ReadAuthority, Subscription, SubscriptionKey},
};
use crate::{
    config::{AdmissionBudget, ConnectionConfig},
    read_pool::ReadPool,
    registry::Registry,
    writer::WriterOptions,
    DbId,
};
use serde_json::json;
use std::{process::Command, sync::Arc, time::Duration};

const NOW: u64 = 1_800_000_000_000;
struct Policy;
impl WriteAuthority for Policy {
    fn authorize_command_intent(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &Value,
    ) -> rusqlite::Result<super::super::WritePermission> {
        Ok(super::super::WritePermission::Permit)
    }
    fn authorize_command_executor(
        &self,
        _: &Transaction<'_>,
        participant: &str,
        _: &Value,
    ) -> rusqlite::Result<super::super::WritePermission> {
        Ok(if participant.starts_with("executor-") {
            super::super::WritePermission::Permit
        } else {
            super::super::WritePermission::Reject("forbidden".into())
        })
    }
    fn authorize(
        &self,
        _: &Transaction<'_>,
        participant: &str,
        request: &Write,
    ) -> rusqlite::Result<super::super::WritePermission> {
        Ok(
            if participant == "revoked"
                || request
                    .body
                    .as_ref()
                    .is_some_and(|v| v["forbidden"] == true)
            {
                super::super::WritePermission::Reject("forbidden".into())
            } else {
                super::super::WritePermission::Permit
            },
        )
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &crate::schema::Collection,
        _: &Value,
    ) -> rusqlite::Result<super::super::WritePermission> {
        Ok(super::super::WritePermission::Permit)
    }
}
impl ReadAuthority for Policy {
    fn authorize(&self, _: &Transaction<'_>, key: &SubscriptionKey) -> rusqlite::Result<()> {
        if key.scope_hash != "rights:1" {
            return Err(invalid("revoked"));
        }
        Ok(())
    }
    fn project(
        &self,
        _: &Transaction<'_>,
        _: &SubscriptionKey,
        doc: &change_log::ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        Ok(doc.body.clone())
    }
}
fn definition(name: &str) -> crate::schema::CollectionDefinition {
    crate::schema::CollectionDefinition {
        name: name.into(),
        app_id: "fixture".into(),
        schema_ver: 1,
        schema_json: json!({"type":"object","properties":{}}),
        class: "full".into(),
        client_limit: 100,
        byte_budget: 1_000_000,
        rights_ver: 1,
        app_ver: "1".into(),
    }
}
fn op(n: u16) -> String {
    op_at(NOW, n)
}
fn op_at(issued: u64, n: u16) -> String {
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&issued.to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[14..].copy_from_slice(&n.to_be_bytes());
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn put(seq: u64, n: u16, id: &str, base: Option<String>, body: Value) -> OrderedWrite {
    OrderedWrite {
        client_seq: Seq(seq),
        write: Write {
            op_id: op(n),
            prev_op_id: None,
            collection: "items".into(),
            id: id.into(),
            base_rev: base,
            base_seq: None,
            gen: 1,
            schema_ver: 1,
            app_ver: "1".into(),
            op: WriteOp::Put,
            body: Some(body),
        },
    }
}
fn database() -> rusqlite::Connection {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    crate::schema::create_collection(&tx, &definition("items")).unwrap();
    tx.commit().unwrap();
    db
}
fn submit(
    db: &mut rusqlite::Connection,
    participant: &str,
    w: &OrderedWrite,
    now: u64,
) -> WriteBatch {
    let tx = db.transaction().unwrap();
    let batch = ingest(&tx, participant, w, now, &Policy).unwrap();
    tx.commit().unwrap();
    batch
}
fn doc(db: &rusqlite::Connection, id: &str) -> Value {
    let s: String = db
        .query_row("SELECT data FROM c_items WHERE id=?1", [id], |r| r.get(0))
        .unwrap();
    serde_json::from_str(&s).unwrap()
}
async fn runtime() -> (tempfile::TempDir, crate::writer::WriterHandle, ReadPool) {
    let dir = tempfile::tempdir().unwrap();
    let db = DbId::new(dir.path().join("sync.sqlite")).unwrap();
    let writer = Registry
        .open(
            db.clone(),
            WriterOptions {
                configure: Some(Arc::new(|c| {
                    ConnectionConfig::default().apply_writer(c)?;
                    let tx = c.unchecked_transaction()?;
                    crate::schema::create_collection(&tx, &definition("items"))?;
                    tx.commit()?;
                    Ok(())
                })),
                head_seq: Some(Arc::new(|c| {
                    c.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| r.get(0))
                        .map_err(Into::into)
                })),
            },
        )
        .unwrap();
    writer.ready().await.unwrap();
    let readers = ReadPool::for_database(
        &db,
        writer.generation(),
        ConnectionConfig::default(),
        AdmissionBudget::new(
            std::num::NonZeroUsize::new(32).unwrap(),
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    (dir, writer, readers)
}
#[tokio::test]
async fn participant_order_precedes_command_effect_and_duplicate_seq_is_a_receipt() {
    let (_dir, writer, readers) = runtime().await;
    let ingress = OrderedWriter::new(writer.clone(), Arc::new(Policy));
    let document = put(1, 1, "document", None, json!({"v":1}));
    let command = put(2, 2, "command", None, json!({"command":"run"}));
    let wire = serde_json::to_string(&ClientMessage::Write(command.clone())).unwrap();
    assert!(wire.contains("\"client_seq\":\"2\""));
    assert_eq!(
        serde_json::from_str::<ClientMessage>(&wire).unwrap(),
        ClientMessage::Write(command.clone())
    );
    let waiting = ingress
        .submit("alice".into(), command.clone(), NOW)
        .await
        .unwrap();
    assert!(waiting.results.is_empty());
    assert_eq!(waiting.next_deadline_ms, Some(NOW + GAP_MS + 1));
    let results = ingress
        .submit("alice".into(), document.clone(), NOW + 1)
        .await
        .unwrap();
    assert_eq!(
        results
            .results
            .iter()
            .map(|r| r.client_seq)
            .collect::<Vec<_>>(),
        vec![Seq(1), Seq(2)]
    );
    assert!(results.results[0].result.seq < results.results[1].result.seq);
    // Same sequence with a different op/body never produces a second effect.
    let repeat = ingress
        .submit(
            "alice".into(),
            put(2, 99, "unexpected", None, json!({"v":99})),
            NOW + 2,
        )
        .await
        .unwrap();
    assert_eq!(repeat.results[0], results.results[1]);
    let mut repeated_op = command.clone();
    repeated_op.client_seq = Seq(3);
    let op_repeat = ingress
        .submit("alice".into(), repeated_op, NOW + 3)
        .await
        .unwrap();
    assert_eq!(op_repeat.results[0].result, results.results[1].result);
    let waiting = ingress
        .submit(
            "alice".into(),
            put(5, 5, "missing-four", None, json!({})),
            NOW + 4,
        )
        .await
        .unwrap();
    assert!(waiting.results.is_empty());
    assert_eq!(
        ingress.recover_deadlines(None, 10).await.unwrap(),
        vec![("alice".into(), NOW + 4 + GAP_MS + 1)]
    );
    assert!(ingress
        .expire("alice".into(), NOW + 4 + GAP_MS)
        .await
        .unwrap()
        .results
        .is_empty());
    let expired = ingress
        .expire("alice".into(), NOW + 5 + GAP_MS)
        .await
        .unwrap();
    assert_eq!(expired.results[0].result.reason.as_deref(), Some("expired"));
    assert_eq!(expired.next_seq, Seq(6));
    let page = readers
        .read(|tx| change_log::read_page(tx, "items", 1, 0, 10))
        .await
        .unwrap();
    assert_eq!(
        page.docs.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec!["document", "command"]
    );
    assert_eq!(page.head_seq, 2);
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[test]
fn field_merge_reconstructs_base_and_preserves_server_collisions_and_delete_cas() {
    let mut db = database();
    let base=submit(&mut db,"alice",&put(1,1,"d",None,json!({"title":"base","count":0,"nested":{"left":0,"right":0},"remove":"old","nullish":null})),NOW).results[0].result.clone();
    let server=submit(&mut db,"bob",&put(1,2,"d",base.rev.clone(),json!({"title":"server","count":0,"nested":{"left":1,"right":0},"remove":"old","nullish":null})),NOW).results[0].result.clone();
    let mut client = put(
        2,
        3,
        "d",
        base.rev.clone(),
        json!({"title":"client","count":2,"nested":{"left":0,"right":2},"nullish":"value"}),
    );
    client.write.base_seq = base.seq;
    let merged = submit(&mut db, "alice", &client, NOW).results[0]
        .result
        .clone();
    assert_eq!(merged.status, WriteStatus::Merged);
    assert!(submit(&mut db, "alice", &client, NOW).results[0].requires_page);
    assert_eq!(
        doc(&db, "d"),
        json!({"title":"server","count":2,"nested":{"left":1,"right":2},"nullish":"value"})
    );
    assert!(merged.seq > server.seq);
    let mut patch = put(3, 4, "d", base.rev.clone(), json!({"nested":{"right":3}}));
    patch.write.op = WriteOp::Patch;
    submit(&mut db, "alice", &patch, NOW);
    assert_eq!(doc(&db, "d")["nested"], json!({"left":1,"right":2}));
    let mut delete = put(4, 5, "d", base.rev.clone(), Value::Null);
    delete.write.op = WriteOp::Delete;
    delete.write.body = None;
    assert_eq!(
        submit(&mut db, "alice", &delete, NOW).results[0]
            .result
            .reason
            .as_deref(),
        Some("conflict")
    );
    let mut missing = put(5, 6, "d", Some("unknown".into()), json!({}));
    assert_eq!(
        submit(&mut db, "alice", &missing, NOW).results[0]
            .result
            .reason
            .as_deref(),
        Some("conflict")
    );
    missing.client_seq = Seq(6);
    missing.write.op_id = op(7);
    missing.write.base_rev = merged.rev;
    // Atomic field values include arrays and missing vs explicit null.
    assert_eq!(
        merge_fields(
            &json!({"a":null,"b":[1]}),
            &json!({"b":[2]}),
            &json!({"a":2,"b":[3]})
        ),
        json!({"b":[2]})
    );
}
#[test]
fn prev_chain_is_resolved_after_ordered_arrival_and_rollback_has_no_receipt() {
    let mut db = database();
    let first = put(1, 1, "d", None, json!({"v":1}));
    let mut second = put(
        2,
        2,
        "d",
        Some(format!("op:{}", first.write.op_id)),
        json!({"v":2}),
    );
    second.write.prev_op_id = Some(first.write.op_id.clone());
    assert!(submit(&mut db, "alice", &second, NOW).results.is_empty());
    let tx = db.transaction().unwrap();
    let results = ingest(&tx, "alice", &first, NOW, &Policy).unwrap();
    assert_eq!(results.results[1].result.status, WriteStatus::Accepted);
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM c_items", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
    let results = submit(&mut db, "alice", &first, NOW);
    assert_eq!(results.results.len(), 2);
    assert_eq!(doc(&db, "d")["v"], 2);
    let mut bad = put(3, 3, "d", Some(format!("op:{}", op(999))), json!({"v":3}));
    bad.write.prev_op_id = Some(op(999));
    assert_eq!(
        submit(&mut db, "alice", &bad, NOW).results[0]
            .result
            .reason
            .as_deref(),
        Some("invalid_predecessor")
    );
    let good = submit(
        &mut db,
        "alice",
        &put(4, 4, "next", None, json!({"v":4})),
        NOW,
    );
    assert_eq!(good.results[0].result.status, WriteStatus::Accepted);
    let previous = op_at(NOW - OP_HORIZON_MS - 1, 998);
    let mut expired = put(5, 5, "d", Some(format!("op:{previous}")), json!({"v":5}));
    expired.write.prev_op_id = Some(previous);
    assert_eq!(
        submit(&mut db, "alice", &expired, NOW).results[0]
            .result
            .reason
            .as_deref(),
        Some("expired")
    );
}
#[test]
fn gap_expiry_overflow_restart_and_gc_do_not_execute_a_late_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inbox.sqlite");
    let mut db = rusqlite::Connection::open(&path).unwrap();
    {
        let tx = db.transaction().unwrap();
        crate::schema::create_collection(&tx, &definition("items")).unwrap();
        tx.commit().unwrap();
    }
    let waiting = put(2, 2, "later", None, json!({"v":2}));
    submit(&mut db, "alice", &waiting, NOW);
    drop(db);
    let mut db = rusqlite::Connection::open(&path).unwrap();
    {
        let tx = db.transaction().unwrap();
        assert_eq!(
            pending_deadlines(&tx, None, 10).unwrap(),
            vec![("alice".into(), NOW + GAP_MS + 1)]
        );
        tx.commit().unwrap();
    }
    {
        let tx = db.transaction().unwrap();
        let (_, r) = expire_waiting(&tx, "alice", 1, NOW + GAP_MS).unwrap();
        assert!(r.is_empty());
        tx.commit().unwrap();
    }
    {
        let tx = db.transaction().unwrap();
        let (_, r) = expire_waiting(&tx, "alice", 1, NOW + GAP_MS + 1).unwrap();
        assert_eq!(r[0].result.reason.as_deref(), Some("expired"));
        tx.commit().unwrap();
    }
    assert_eq!(
        submit(
            &mut db,
            "alice",
            &put(1, 1, "late", None, json!({})),
            NOW + GAP_MS + 2
        )
        .results[0]
            .result
            .reason
            .as_deref(),
        Some("expired")
    );
    // Changing client_seq cannot revive the op_id expired at an earlier order slot.
    let mut retry = waiting.clone();
    retry.client_seq = Seq(3);
    assert_eq!(
        submit(&mut db, "alice", &retry, NOW + GAP_MS + 3).results[0]
            .result
            .reason
            .as_deref(),
        Some("expired")
    );
    // A first delivery near the op_id horizon cannot extend that horizon by
    // another month merely by replaying through the client_seq receipt ledger.
    {
        let mut delayed = database();
        let request = put(1, 777, "delayed", None, json!({"v":1}));
        assert_eq!(
            submit(&mut delayed, "delayed", &request, NOW + OP_HORIZON_MS - 1).results[0]
                .result
                .status,
            WriteStatus::Accepted
        );
        assert_eq!(
            submit(&mut delayed, "delayed", &request, NOW + OP_HORIZON_MS + 1).results[0]
                .result
                .reason
                .as_deref(),
            Some("expired")
        );
    }
    for seq in 2..=(MAX_WAITING as u64 + 1) {
        submit(
            &mut db,
            "bounded",
            &put(seq, seq as u16 + 100, "queued", None, json!({"v":seq})),
            NOW,
        );
    }
    let overflow = submit(
        &mut db,
        "bounded",
        &put(MAX_WAITING as u64 + 2, 999, "overflow", None, json!({})),
        NOW,
    );
    assert_eq!(
        overflow.results[0].result.reason.as_deref(),
        Some("expired")
    );
    {
        let tx = db.transaction().unwrap();
        purge_results(&tx, NOW + OP_HORIZON_MS + GAP_MS + 10, 500).unwrap();
        super::super::purge_operations(&tx, NOW + OP_HORIZON_MS + GAP_MS + 10, 500).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(
        submit(
            &mut db,
            "alice",
            &waiting,
            NOW + OP_HORIZON_MS + GAP_MS + 10
        )
        .results[0]
            .result
            .reason
            .as_deref(),
        Some("expired")
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM c_items", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn unsent_native_rows_survive_gc_and_failed_writer_transaction() {
    let mut db = database();
    let waiting = put(2, 2, "d", None, json!({"draft":"retain"}));
    submit(&mut db, "alice", &waiting, NOW);
    {
        let tx = db.transaction().unwrap();
        purge_results(&tx, NOW + OP_HORIZON_MS + 1, 500).unwrap();
        super::super::purge_operations(&tx, NOW + OP_HORIZON_MS + 1, 500).unwrap();
        tx.commit().unwrap();
    }
    {
        let tx = db.transaction().unwrap();
        ingest(
            &tx,
            "alice",
            &put(1, 1, "first", None, json!({})),
            NOW,
            &Policy,
        )
        .unwrap();
        tx.rollback().unwrap();
    }
    let json: String = db
        .query_row(
            "SELECT request_json FROM _write_pending WHERE participant='alice' AND client_seq=2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Write>(&json).unwrap().body,
        waiting.write.body
    );
    assert_eq!(
        db.query_row(
            "SELECT next_seq FROM _write_order WHERE participant='alice'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
}
#[tokio::test]
async fn cursor_never_skips_a_revision_moved_during_initial_sync() {
    let (_dir, writer, readers) = runtime().await;
    let ingress = OrderedWriter::new(writer.clone(), Arc::new(Policy));
    for (seq, id) in [(1, "a"), (2, "b"), (3, "c")] {
        ingress
            .submit(
                "alice".into(),
                put(seq, seq as u16, id, None, json!({"v":1})),
                NOW,
            )
            .await
            .unwrap();
    }
    let first = readers
        .read(|tx| change_log::read_page(tx, "items", 1, 0, 1))
        .await
        .unwrap();
    assert_eq!(first.docs[0].id, "a");
    ingress
        .submit(
            "alice".into(),
            put(4, 4, "b", Some(format!("v4:{}", op(2))), json!({"v":2})),
            NOW,
        )
        .await
        .unwrap();
    let second = readers
        .read(move |tx| change_log::read_page(tx, "items", 1, first.cursor_to, 1))
        .await
        .unwrap();
    assert_eq!(second.docs[0].id, "c");
    let moved = readers
        .read(move |tx| change_log::read_page(tx, "items", 1, second.cursor_to, 1))
        .await
        .unwrap();
    assert_eq!(moved.docs[0].id, "b");
    assert_eq!(moved.docs[0].body.as_ref().unwrap()["v"], 2);
    let empty = readers
        .read(move |tx| change_log::read_page(tx, "items", 1, moved.cursor_to, 1))
        .await
        .unwrap();
    assert_eq!(empty.cursor_to, empty.head_seq);
    assert_eq!(empty.head_seq, 4);
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reader_snapshot_keeps_head_and_body_together_across_writer_commit() {
    let (_dir, writer, readers) = runtime().await;
    let ingress = OrderedWriter::new(writer.clone(), Arc::new(Policy));
    let initial = ingress
        .submit("alice".into(), put(1, 1, "d", None, json!({"v":1})), NOW)
        .await
        .unwrap();
    let (entered, ready) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let pool = readers.clone();
    let reading = tokio::spawn(async move {
        pool.read(move |tx| {
            let before = 1;
            entered.send(before).unwrap();
            resume.recv_timeout(Duration::from_secs(2)).unwrap();
            change_log::read_page(tx, "items", 1, 0, 10)
        })
        .await
    });
    assert_eq!(
        tokio::task::spawn_blocking(move || ready.recv_timeout(Duration::from_secs(2)).unwrap())
            .await
            .unwrap(),
        1
    );
    ingress
        .submit(
            "alice".into(),
            put(
                2,
                2,
                "d",
                initial.results[0].result.rev.clone(),
                json!({"v":2}),
            ),
            NOW,
        )
        .await
        .unwrap();
    release.send(()).unwrap();
    let snapshot = reading.await.unwrap().unwrap();
    assert_eq!(snapshot.head_seq, 1);
    assert_eq!(snapshot.cursor_to, 1);
    assert_eq!(snapshot.docs[0].seq, 1);
    assert_eq!(snapshot.docs[0].body.as_ref().unwrap()["v"], 1);
    let current = readers
        .read(|tx| change_log::read_page(tx, "items", 1, 0, 10))
        .await
        .unwrap();
    assert_eq!(current.head_seq, 2);
    assert_eq!(current.docs[0].body.as_ref().unwrap()["v"], 2);
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[tokio::test]
async fn rights_change_invalidates_checkpoint_instead_of_reusing_old_cursor() {
    let (_dir, writer, readers) = runtime().await;
    let old = SubscriptionKey {
        participant: "alice".into(),
        collection: "items".into(),
        scope_hash: "rights:1".into(),
        gen: 1,
    };
    let cp = Checkpoint {
        instance_id: "core".into(),
        participant: old.participant.clone(),
        collection: old.collection.clone(),
        scope_hash: old.scope_hash.clone(),
        gen: 1,
        epoch: "epoch".into(),
        seq: Seq(99),
    };
    let new = SubscriptionKey {
        scope_hash: "rights:2".into(),
        ..old
    };
    assert!(Subscription::new(readers.clone(), &writer, new, Some(cp)).is_err());
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
fn node(script: &str, result: Option<&WriteResult>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let kernel = std::env::var("CTOX_P8_KERNEL_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| root.join("src/apps/business-os/kernel"));
    let mut cmd = Command::new("node");
    cmd.current_dir(&root)
        .args(["--input-type=module", "-e", script])
        .env(
            "KERNEL_URL",
            format!("file://{}", kernel.canonicalize().unwrap().display()),
        );
    if let Some(result) = result {
        cmd.env("CORE_RESULT_JSON", serde_json::to_string(result).unwrap());
    }
    let output = cmd
        .output()
        .expect("Node is required for the real V4 kernel contract");
    assert!(
        output.status.success(),
        "kernel contract failed: {}\\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn rejected_core_result_archives_the_original_kernel_input() {
    let mut db = database();
    let result = submit(
        &mut db,
        "revoked",
        &put(1, 1, "d", None, json!({"draft":"keep"})),
        NOW,
    )
    .results[0]
        .result
        .clone();
    assert_eq!(result.reason.as_deref(), Some("forbidden"));
    node(
        r#"
      import assert from 'node:assert/strict';
      const {createKernel}=await import(process.env.KERNEL_URL+'/kernel.mjs');
      let events;
      const transport={connect:async(_identity,e)=>{events=e},subscribe:async()=>{},close(){}};
      const k=createKernel({instanceId:'p8-rejection-'+process.pid,userId:'alice',storage:'memory',multiTab:false,transport});
      try {
        await k.ready;const h=k.appApi('fixture','1').put('items',{id:'d',draft:'keep'});
        await h.local;
        await events.write_result({...JSON.parse(process.env.CORE_RESULT_JSON),op_id:h.op_id});
        const archived=await k.sim.archiveLocal();
        assert.equal(archived[0].body.draft,'keep');
        assert.equal((await h.result).reason,'forbidden');
        assert.equal((await k.sim.outbox()).length,0);
        await events.write_result({...JSON.parse(process.env.CORE_RESULT_JSON),op_id:h.op_id});
        assert.equal((await k.sim.archiveLocal()).length,1);
      } finally {await k.close();}
    "#,
        Some(&result),
    );
}
#[test]
fn unsent_kernel_rows_cannot_be_evicted_or_reclaimed() {
    node(
        r#"
      import assert from 'node:assert/strict';
      const {createKernel}=await import(process.env.KERNEL_URL+'/kernel.mjs');
      const k=createKernel({instanceId:'p8-pending-'+process.pid,userId:'alice',storage:'memory',multiTab:false});
      try {
        await k.ready;const h=k.appApi('fixture','1').put('items',{id:'d',draft:'keep'});
        await h.local;await assert.rejects(k.evict('items'),e=>e.code==='quota');
        assert.equal(k.appApi('fixture','1').get('items','d').get().draft,'keep');
        assert.equal((await k.sim.outbox())[0].body.draft,'keep');
      } finally {await k.close();}
    "#,
        None,
    );
}
