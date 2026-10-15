use super::*;
use crate::{
    config::{AdmissionBudget, ConnectionConfig},
    registry::Registry,
    writer::WriterOptions,
    DbId, JobOptions, WriteJob,
};
use serde_json::json;
use std::{path::PathBuf, process::Command, time::Duration};

struct NativePolicy;
impl ReadAuthority for NativePolicy {
    fn authorize(&self, _: &Transaction<'_>, key: &SubscriptionKey) -> rusqlite::Result<()> {
        if key.participant != "alice" || key.scope_hash != "alice/public-v1" {
            return Err(invalid("scope revoked"));
        }
        Ok(())
    }
    fn project(
        &self,
        _: &Transaction<'_>,
        _: &SubscriptionKey,
        doc: &ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        Ok(doc.body.as_ref().filter(|v| v["visible"] != false).cloned())
    }
}
impl WriteAuthority for NativePolicy {
    fn authorize(
        &self,
        _: &Transaction<'_>,
        participant: &str,
        _: &Write,
    ) -> rusqlite::Result<WritePermission> {
        Ok(if participant == "alice" {
            WritePermission::Permit
        } else {
            WritePermission::Reject("forbidden".into())
        })
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &crate::schema::Collection,
        doc: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(if doc["v"].is_number() {
            WritePermission::Permit
        } else {
            WritePermission::Reject("invalid_document".into())
        })
    }
}
fn definition(name: &str) -> crate::schema::CollectionDefinition {
    crate::schema::CollectionDefinition {
        name: name.into(),
        app_id: "test".into(),
        schema_ver: 1,
        schema_json: json!({"version":1,"type":"object","properties":{"v":{"type":"number"}}}),
        class: "full".into(),
        client_limit: 100,
        byte_budget: 1_000_000,
        rights_ver: 1,
        app_ver: "1".into(),
    }
}
fn decode_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
/// Public RFC8032 TEST vector only. Production code has no OpenSSL process/key-file route.
pub(super) struct TestInstanceKey {
    _dir: tempfile::TempDir,
    private: PathBuf,
    public: PathBuf,
}
impl TestInstanceKey {
    pub(super) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let private = dir.path().join("test-ed25519.der");
        let public = dir.path().join("test-ed25519.pub.der");
        std::fs::write(
            &private,
            decode_hex(concat!(
                "302e020100300506032b657004220420",
                "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"
            )),
        )
        .unwrap();
        std::fs::write(
            &public,
            decode_hex(concat!(
                "302a300506032b6570032100",
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
            )),
        )
        .unwrap();
        Self {
            _dir: dir,
            private,
            public,
        }
    }
}
impl InstanceSigner for TestInstanceKey {
    fn instance_id(&self) -> &str {
        "isolated-core"
    }
    fn key_id(&self) -> &str {
        "rfc8032-public-test-key"
    }
    fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        let input = self._dir.path().join("sign-input");
        std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
        let output = Command::new("openssl")
            .args(["pkeyutl", "-sign", "-rawin", "-keyform", "DER", "-inkey"])
            .arg(&self.private)
            .arg("-in")
            .arg(input)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into())
        }
    }
}
impl InstanceVerifier for TestInstanceKey {
    fn verify(
        &self,
        instance: &str,
        key: &str,
        bytes: &[u8],
        signature: &[u8],
    ) -> Result<(), String> {
        if instance != self.instance_id() || key != self.key_id() {
            return Err("wrong instance key".into());
        }
        let input = self._dir.path().join("verify-input");
        let sig = self._dir.path().join("verify-sig");
        std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
        std::fs::write(&sig, signature).map_err(|e| e.to_string())?;
        let output = Command::new("openssl")
            .args([
                "pkeyutl", "-verify", "-rawin", "-pubin", "-keyform", "DER", "-inkey",
            ])
            .arg(&self.public)
            .arg("-in")
            .arg(input)
            .arg("-sigfile")
            .arg(sig)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err("signature rejected".into())
        }
    }
}
async fn next(
    subscription: &mut Subscription,
    key: &TestInstanceKey,
    limit: usize,
) -> StreamPacket {
    tokio::time::timeout(
        Duration::from_secs(2),
        subscription.next_page(limit, Arc::new(NativePolicy), key, || 1234),
    )
    .await
    .expect("commit-driven page must arrive")
    .unwrap()
}
#[tokio::test]
async fn initial_changes_move_forward_and_retained_commit_wakes_the_identical_live_read() {
    let dir = tempfile::tempdir().unwrap();
    let db = DbId::new(dir.path().join("sync.sqlite")).unwrap();
    let writer=Registry.open(db.clone(),WriterOptions{
        configure:Some(Arc::new(|c|{ConnectionConfig::default().apply_writer(c)?;let tx=c.unchecked_transaction()?;
            crate::schema::create_collection(&tx,&definition("items"))?;crate::schema::create_collection(&tx,&definition("empty"))?;
            tx.execute_batch("INSERT INTO c_items VALUES('a','a1',1,'{\"v\":1}',0);
                INSERT INTO c_items VALUES('b','b1',1,'{\"v\":1}',0);INSERT INTO c_items VALUES('c','c1',1,'{\"v\":1}',0);")?;
            tx.commit()?;Ok(())})),
        head_seq:Some(Arc::new(|c|c.query_row("SELECT v FROM _seq WHERE singleton=1",[],|r|r.get(0)).map_err(Into::into)))
    }).unwrap();
    writer.ready().await.unwrap();
    let budget = AdmissionBudget::new(
        std::num::NonZeroUsize::new(32).unwrap(),
        Duration::from_secs(2),
        Duration::from_secs(1),
    )
    .unwrap();
    let readers = ReadPool::for_database(
        &db,
        writer.generation(),
        ConnectionConfig::default(),
        budget,
    )
    .await
    .unwrap();
    let key = TestInstanceKey::new();
    let scope = SubscriptionKey {
        participant: "alice".into(),
        collection: "items".into(),
        scope_hash: "alice/public-v1".into(),
        gen: 1,
    };
    let mut sub = Subscription::new(readers.clone(), &writer, scope.clone(), None).unwrap();
    let first = next(&mut sub, &key, 1).await;
    first.verify(&key).unwrap();
    assert_eq!(
        (&first.docs[0].id, first.docs[0].seq),
        (&"a".to_string(), Seq(1))
    );
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute(
                "UPDATE c_items SET rev='b2',data='{\"v\":2}' WHERE id='b'",
                [],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let second = next(&mut sub, &key, 1).await;
    assert_eq!(second.docs[0].id, "c");
    let moved = next(&mut sub, &key, 1).await;
    assert_eq!(moved.docs[0].id, "b");
    assert_eq!(moved.docs[0].body.as_ref().unwrap()["v"], 2);
    assert_eq!(moved.docs[0].seq, Seq(4));
    let empty = next(&mut sub, &key, 1).await;
    assert!(empty.docs.is_empty());
    assert_eq!(empty.seq_to, Seq(4));
    assert!(empty.complete);
    // Commit occurs AFTER the final initial page, BEFORE the next wait. No registration gap may lose it.
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute(
                "UPDATE c_items SET rev='a2',data='{\"v\":9}' WHERE id='a'",
                [],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let live = next(&mut sub, &key, 100).await;
    assert_eq!(live.kind, StreamKind::Live);
    assert_eq!(live.docs[0].id, "a");
    live.verify(&key).unwrap();
    let mut version = DocumentVersion(live.docs[0].seq);
    assert!(
        !version.admit(first.docs[0].seq),
        "stale initial page must not reset the newer live document"
    );
    assert!(!version.admit(live.docs[0].seq), "duplicate is idempotent");
    let mut other = Subscription::new(
        readers.clone(),
        &writer,
        SubscriptionKey {
            collection: "empty".into(),
            ..scope
        },
        None,
    )
    .unwrap();
    let proof = next(&mut other, &key, 10).await;
    assert!(proof.docs.is_empty());
    assert_eq!(proof.seq_to, live.head_seq);
    assert_eq!(proof.backlog, 0);
    let mut tampered = live.clone();
    tampered.scope_hash = "foreign".into();
    assert!(tampered.verify(&key).is_err());
    let mut tampered = live.clone();
    tampered.docs[0].body = Some(json!({"v":0}));
    assert!(tampered.verify(&key).is_err());
    let encoded = serde_json::to_value(&live).unwrap();
    assert!(encoded["seq_to"].is_string());
    let frame = live.signed_frame().unwrap();
    let binding = StreamBinding {
        instance_id: live.instance_id.clone(),
        participant: live.participant.clone(),
        collection: live.collection.clone(),
        scope_hash: live.scope_hash.clone(),
        gen: live.gen,
        epoch: live.epoch.clone(),
    };
    assert_eq!(
        frame.verify(&binding, 1_000_000, &key).unwrap().packet(),
        &live
    );
    let mut broken = frame.clone();
    broken.payload = String::from_utf8(broken.payload)
        .unwrap()
        .replace("\"core_sent_at_ms\":1234", "\"core_sent_at_ms\":1235")
        .into_bytes();
    assert_ne!(broken.payload, frame.payload);
    assert!(broken.verify(&binding, 1_000_000, &key).is_err());
    let mut other_binding = binding.clone();
    other_binding.scope_hash = "another".into();
    assert!(frame.verify(&other_binding, 1_000_000, &key).is_err());
    let checkpoint = sub.checkpoint().unwrap();
    let mut wrong_scope = checkpoint.clone();
    wrong_scope.scope_hash = "another".into();
    assert!(Subscription::new(
        readers.clone(),
        &writer,
        sub.key().clone(),
        Some(wrong_scope)
    )
    .is_err());
    let mut foreign = checkpoint.clone();
    foreign.epoch = "wrong".into();
    let mut wrong =
        Subscription::new(readers.clone(), &writer, sub.key().clone(), Some(foreign)).unwrap();
    assert!(wrong
        .next_page(1, Arc::new(NativePolicy), &key, || 0)
        .await
        .is_err());
    // Even an incorrectly supplied equal generation cannot bind another database to this writer.
    let other_db = DbId::new(dir.path().join("foreign.sqlite")).unwrap();
    let other_writer = Registry
        .open(
            other_db.clone(),
            WriterOptions {
                configure: Some(Arc::new(|c| {
                    ConnectionConfig::default().apply_writer(c)?;
                    let tx = c.unchecked_transaction()?;
                    crate::schema::create_collection(&tx, &definition("items"))?;
                    tx.commit()?;
                    Ok(())
                })),
                head_seq: None,
            },
        )
        .unwrap();
    other_writer.ready().await.unwrap();
    let other_pool = ReadPool::for_database(
        &other_db,
        writer.generation(),
        ConnectionConfig::default(),
        budget,
    )
    .await
    .unwrap();
    let mut mismatched =
        Subscription::new(other_pool.clone(), &writer, sub.key().clone(), None).unwrap();
    assert!(mismatched
        .next_page(1, Arc::new(NativePolicy), &key, || 0)
        .await
        .is_err());
    other_pool.shutdown().await.unwrap();
    other_writer.shutdown().await.unwrap();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
const NOW: u64 = 1_800_000_000_000;
fn op_id(time: u64, n: u16) -> String {
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&time.to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[14..].copy_from_slice(&n.to_be_bytes());
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn put(n: u16, base: Option<String>) -> Write {
    Write {
        op_id: op_id(NOW, n),
        prev_op_id: None,
        collection: "items".into(),
        id: "doc".into(),
        base_rev: base,
        base_seq: None,
        gen: 1,
        schema_ver: 1,
        app_ver: "1".into(),
        op: WriteOp::Put,
        body: Some(json!({"v":n})),
    }
}
#[test]
fn operation_replay_chain_expiry_and_remove_are_atomic_and_payload_free() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    crate::schema::create_collection(&tx, &definition("items")).unwrap();
    let a = put(1, None);
    let first = apply_write(&tx, "alice", &a, NOW, &NativePolicy).unwrap();
    assert_eq!(first.status, WriteStatus::Accepted);
    let identity: String = tx
        .query_row(
            "SELECT request_json FROM _ops WHERE op_id=?1",
            [&a.op_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        serde_json::from_str::<Value>(&identity).unwrap()["body"].is_null(),
        "dedup metadata must not retain a deleted business payload"
    );
    let head = change_log::head(&tx).unwrap();
    let replay = apply_write(&tx, "alice", &a, NOW + 1, &NativePolicy).unwrap();
    assert_eq!(first, replay);
    assert_eq!(head, change_log::head(&tx).unwrap());
    let mut changed = a.clone();
    changed.body = Some(json!({"v":999}));
    assert_eq!(
        apply_write(&tx, "alice", &changed, NOW + 1, &NativePolicy).unwrap(),
        first
    );
    assert!(!tx
        .query_row::<String, _, _>(
            "SELECT request_json FROM _ops WHERE op_id=?1",
            [&a.op_id],
            |r| r.get(0)
        )
        .unwrap()
        .contains("999"));
    let mut b = put(2, Some(format!("op:{}", a.op_id)));
    b.prev_op_id = Some(a.op_id.clone());
    let second = apply_write(&tx, "alice", &b, NOW + 2, &NativePolicy).unwrap();
    assert_eq!(second.status, WriteStatus::Accepted);
    let mut c = put(3, Some(format!("op:{}", b.op_id)));
    c.prev_op_id = Some(b.op_id.clone());
    c.op = WriteOp::Patch;
    let third = apply_write(&tx, "alice", &c, NOW + 3, &NativePolicy).unwrap();
    assert_eq!(third.status, WriteStatus::Accepted);
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT data FROM c_items WHERE id='doc'", [], |r| r.get(0))
            .unwrap(),
        "{\"v\":3}"
    );
    let mut foreign = put(4, Some(format!("op:{}", a.op_id)));
    foreign.id = "another".into();
    foreign.prev_op_id = Some(a.op_id.clone());
    assert_eq!(
        apply_write(&tx, "alice", &foreign, NOW + 4, &NativePolicy)
            .unwrap()
            .reason
            .as_deref(),
        Some("invalid_predecessor")
    );
    let mut pending = put(5, Some(format!("op:{}", op_id(NOW, 6))));
    pending.prev_op_id = Some(op_id(NOW, 6));
    assert!(apply_write(&tx, "alice", &pending, NOW + 5, &NativePolicy)
        .unwrap_err()
        .to_string()
        .contains("pending_predecessor"));
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM _ops WHERE op_id=?1",
            [&pending.op_id],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
    let mut remove = put(7, third.rev.clone());
    remove.op = WriteOp::Delete;
    remove.body = None;
    let deleted = apply_write(&tx, "alice", &remove, NOW + 7, &NativePolicy).unwrap();
    assert_eq!(deleted.status, WriteStatus::Accepted);
    assert!(tx
        .query_row::<bool, _, _>("SELECT data IS NULL AND deleted=1 FROM c_items", [], |r| r
            .get(0))
        .unwrap());
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM _revs WHERE body IS NOT NULL",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
    let mut resurrection = put(8, deleted.rev.clone());
    resurrection.body = Some(json!({"v":8}));
    assert_eq!(
        apply_write(&tx, "alice", &resurrection, NOW + 8, &NativePolicy)
            .unwrap()
            .reason
            .as_deref(),
        Some("conflict")
    );
    let create = put(9, None);
    let created = apply_write(&tx, "alice", &create, NOW + 9, &NativePolicy).unwrap();
    assert_eq!(created.status, WriteStatus::Accepted);
    let mut old_delete = remove.clone();
    old_delete.op_id = op_id(NOW, 10);
    assert_eq!(
        apply_write(&tx, "alice", &old_delete, NOW + 10, &NativePolicy)
            .unwrap()
            .reason
            .as_deref(),
        Some("conflict")
    );
    let mut denied = put(11, None);
    denied.id = "denied".into();
    let refusal = apply_write(&tx, "bob", &denied, NOW + 11, &NativePolicy).unwrap();
    assert_eq!(refusal.status, WriteStatus::Rejected);
    assert_eq!(refusal.reason.as_deref(), Some("forbidden"));
    assert_eq!(
        apply_write(&tx, "bob", &denied, NOW + 12, &NativePolicy).unwrap(),
        refusal
    );
    let mut stale_delete = put(12, created.rev.clone());
    stale_delete.op = WriteOp::Delete;
    stale_delete.body = None;
    stale_delete.schema_ver = 0;
    stale_delete.app_ver = "old".into();
    assert_eq!(
        apply_write(&tx, "alice", &stale_delete, NOW + 12, &NativePolicy)
            .unwrap()
            .status,
        WriteStatus::Migrated
    );
    let mut invalid = put(13, None);
    invalid.id = "invalid".into();
    invalid.body = Some(json!({"v":"wrong"}));
    assert_eq!(
        apply_write(&tx, "alice", &invalid, NOW + 13, &NativePolicy)
            .unwrap()
            .reason
            .as_deref(),
        Some("invalid_document")
    );
    // Expiry must be independent of whether background GC has already removed the predecessor.
    let mut late = put(14, Some(format!("op:{}", c.op_id)));
    late.op_id = op_id(NOW + OP_HORIZON_MS + 100, 14);
    late.prev_op_id = Some(c.op_id.clone());
    assert_eq!(
        apply_write(
            &tx,
            "alice",
            &late,
            NOW + OP_HORIZON_MS + 100,
            &NativePolicy
        )
        .unwrap()
        .reason
        .as_deref(),
        Some("expired")
    );
    let before = change_log::head(&tx).unwrap();
    purge_operations(&tx, NOW + OP_HORIZON_MS + 100, 500).unwrap();
    assert_eq!(
        apply_write(&tx, "alice", &a, NOW + OP_HORIZON_MS + 100, &NativePolicy)
            .unwrap()
            .reason
            .as_deref(),
        Some("expired")
    );
    assert_eq!(before, change_log::head(&tx).unwrap());
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name='c_items' OR name='_ops'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn clock_exchange_rejects_replay_and_sequences_never_pass_through_json_numbers() {
    let probe = ClockProbe {
        nonce: "fresh".into(),
        client_sent_at_ms: 1000,
    };
    let reply = ClockReply {
        nonce: "fresh".into(),
        client_sent_at_ms: 1000,
        core_received_at_ms: 1110,
        core_sent_at_ms: 1114,
    };
    let hello = CoreMessage::HelloOk(HelloOk {
        epoch: "epoch".into(),
        head_seq: Seq(9_007_199_254_740_993),
        scope_tokens: Vec::new(),
        rights_ver: 1,
        clock: reply.clone(),
    });
    let hello_json = serde_json::to_value(&hello).unwrap();
    assert_eq!(hello_json["type"], "hello_ok");
    assert_eq!(hello_json["head_seq"], "9007199254740993");
    assert_eq!(
        serde_json::from_value::<CoreMessage>(hello_json).unwrap(),
        hello
    );
    let estimate = reply.estimate(&probe, 1024).unwrap();
    assert_eq!(estimate.core_minus_client_ms, 100);
    assert_eq!(estimate.round_trip_ms, 20);
    let mut wrong = reply.clone();
    wrong.nonce = "old".into();
    assert!(wrong.estimate(&probe, 1024).is_err());
    let large = Seq(9_007_199_254_740_993);
    let json = serde_json::to_string(&large).unwrap();
    assert_eq!(json, "\"9007199254740993\"");
    assert_eq!(serde_json::from_str::<Seq>(&json).unwrap(), large);
    assert!(serde_json::from_str::<Seq>("9007199254740993").is_err());
    assert!(serde_json::from_str::<Seq>("\"01\"").is_err());
}

async fn generation_runtime() -> (
    tempfile::TempDir,
    crate::schema::GenerationRunner,
    crate::writer::WriterHandle,
    ReadPool,
) {
    let dir = tempfile::tempdir().unwrap();
    let db = DbId::new(dir.path().join("generations.sqlite")).unwrap();
    let writer = Registry
        .open(
            db.clone(),
            WriterOptions {
                configure: Some(Arc::new(|c| {
                    ConnectionConfig::default().apply_writer(c)?;
                    let tx = c.unchecked_transaction()?;
                    crate::schema::create_collection(&tx, &definition("items"))?;
                    tx.execute_batch("INSERT INTO c_items VALUES('one','old',1,'{\"v\":1}',0);")?;
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
    let runner = crate::schema::GenerationRunner::new(writer.clone(), readers.clone()).unwrap();
    (dir, runner, writer, readers)
}
fn generation_lock() -> crate::schema::PublishLock {
    crate::schema::PublishLock {
        app_id: "test".into(),
        base_app_ver: "1".into(),
        target_app_ver: "2".into(),
        operation_id: "generation-2".into(),
    }
}
fn generation_scope(gen: u64, name: &str) -> SubscriptionKey {
    SubscriptionKey {
        participant: "alice".into(),
        collection: name.into(),
        scope_hash: "alice/public-v1".into(),
        gen,
    }
}
fn next_definition() -> crate::schema::CollectionDefinition {
    let mut def = definition("items");
    def.schema_ver = 2;
    def.app_ver = "2".into();
    def.schema_json["properties"]["v"]["type"] = json!("string");
    def
}
fn transformed(
    page: &change_log::ChangePage,
) -> Result<Vec<crate::schema::MigrationOutput>, StoreError> {
    Ok(page
        .docs
        .iter()
        .map(|doc| {
            if doc.op == Operation::Delete {
                crate::schema::MigrationOutput::Delete
            } else {
                crate::schema::MigrationOutput::Upsert {
                    rev: format!("migrated:{}", doc.rev),
                    body: json!({"v":"new"}),
                }
            }
        })
        .collect())
}
#[tokio::test]
async fn generation_batch_retry_is_idempotent_and_stale_source_cannot_publish() {
    let (_dir, runner, writer, readers) = generation_runtime().await;
    let prepared = runner
        .prepare(generation_lock(), next_definition())
        .await
        .unwrap();
    assert_eq!((prepared.source_gen, prepared.gen), (1, 2));
    let saved = runner
        .migrate_batch(generation_lock(), "items".into(), 100, transformed)
        .await
        .unwrap();
    assert_eq!(saved.state, "ready");
    let before = readers
        .read(|tx| {
            Ok((
                change_log::head(tx)?,
                tx.query_row("SELECT count(*) FROM _archive", [], |r| r.get::<_, u64>(0))?,
            ))
        })
        .await
        .unwrap();
    let repeated = runner
        .migrate_batch(generation_lock(), "items".into(), 101, |_| {
            panic!("ready retry must not transform again")
        })
        .await
        .unwrap();
    assert_eq!(saved, repeated);
    assert_eq!(
        before,
        readers
            .read(|tx| Ok((
                change_log::head(tx)?,
                tx.query_row("SELECT count(*) FROM _archive", [], |r| r.get::<_, u64>(0))?
            )))
            .await
            .unwrap()
    );
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute(
                "UPDATE c_items SET rev='late',data='{\"v\":9}' WHERE id='one'",
                [],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    assert!(
        runner.publish(generation_lock(), 102).await.is_err(),
        "late source write must fence publication"
    );
    runner
        .migrate_batch(generation_lock(), "items".into(), 103, transformed)
        .await
        .unwrap();
    let watermark = runner.publish(generation_lock(), 104).await.unwrap();
    assert_eq!(
        runner.publish(generation_lock(), 105).await.unwrap(),
        watermark,
        "publish retry returns the committed receipt"
    );
    let mut rebound = generation_lock();
    rebound.target_app_ver = "forged".into();
    assert!(
        runner.publish(rebound, 106).await.is_err(),
        "operation receipt has an immutable binding"
    );
    readers
        .read(move |tx| {
            let active = crate::schema::active_collection(tx, "items")?.unwrap();
            assert_eq!(active.gen, 2);
            assert_eq!(active.migrated_at_seq, Some(watermark));
            let rev: String =
                tx.query_row("SELECT rev FROM c_items__g2 WHERE id='one'", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(rev, "migrated:late");
            Ok(())
        })
        .await
        .unwrap();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[tokio::test]
async fn old_generation_never_receives_new_rows_and_requires_its_own_checkpoint() {
    let (_dir, runner, writer, readers) = generation_runtime().await;
    let key = TestInstanceKey::new();
    let mut old =
        Subscription::new(readers.clone(), &writer, generation_scope(1, "items"), None).unwrap();
    let first = next(&mut old, &key, 10).await;
    let checkpoint = old.checkpoint().unwrap();
    assert_eq!(first.docs[0].body.as_ref().unwrap()["v"], 1);
    runner
        .prepare(generation_lock(), next_definition())
        .await
        .unwrap();
    runner
        .migrate_batch(generation_lock(), "items".into(), 100, transformed)
        .await
        .unwrap();
    let mut new =
        Subscription::new(readers.clone(), &writer, generation_scope(2, "items"), None).unwrap();
    let background = next(&mut new, &key, 10).await;
    assert_eq!(background.gen, 2);
    assert_eq!(background.docs[0].body.as_ref().unwrap()["v"], "new");
    let still_old = next(&mut old, &key, 10).await;
    assert!(
        still_old.docs.is_empty(),
        "new generation must not enter old cursor stream"
    );
    assert_eq!(still_old.gen, 1);
    let watermark = runner.publish(generation_lock(), 101).await.unwrap();
    let event = tokio::time::timeout(
        Duration::from_secs(2),
        old.next_event(10, Arc::new(NativePolicy), &key, || 1),
    )
    .await
    .unwrap()
    .unwrap();
    let SubscriptionEvent::RequiresPage(required) = event else {
        panic!("archived generation must require resnapshot")
    };
    assert_eq!(required.gen, Some(2));
    assert_eq!(required.previous_gen, 1);
    assert_eq!(required.checkpoint_seq, Seq(0));
    assert_eq!(required.migrated_at_seq, Some(Seq(watermark)));
    assert_eq!(old.checkpoint().unwrap().gen, 1);
    assert!(Subscription::new(
        readers.clone(),
        &writer,
        generation_scope(2, "items"),
        Some(checkpoint)
    )
    .is_err());
    let wire = serde_json::to_value(CoreMessage::RequiresPage(required)).unwrap();
    assert_eq!(wire["type"], "requires_page");
    assert_eq!(wire["checkpoint_seq"], "0");
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
#[tokio::test]
async fn unknown_app_activates_without_restart_with_an_existing_subscription() {
    let (_dir, runner, writer, readers) = generation_runtime().await;
    let incarnation = writer.generation();
    let key = TestInstanceKey::new();
    let mut existing =
        Subscription::new(readers.clone(), &writer, generation_scope(1, "items"), None).unwrap();
    next(&mut existing, &key, 10).await;
    let mut def = definition("runtime_unknown_documents");
    def.app_id = format!("unknown-app-{}", uuid::Uuid::new_v4());
    let lock = crate::schema::PublishLock {
        app_id: def.app_id.clone(),
        base_app_ver: String::new(),
        target_app_ver: "1".into(),
        operation_id: "runtime-install".into(),
    };
    runner.prepare(lock.clone(), def).await.unwrap();
    readers
        .read(|tx| {
            assert!(
                crate::schema::active_collection(tx, "runtime_unknown_documents")?.is_none(),
                "prepare must not activate"
            );
            Ok(())
        })
        .await
        .unwrap();
    let first_lock = lock.clone();
    assert!(runner
        .publish_with(first_lock, 99, |_, _| Err(StoreError::Invalid(
            "catalog failure".into()
        )))
        .await
        .is_err());
    readers
        .read(|tx| {
            assert!(crate::schema::active_collection(tx, "runtime_unknown_documents")?.is_none());
            Ok(())
        })
        .await
        .unwrap();
    runner
        .publish_with(lock.clone(), 100, |tx, watermark| {
            tx.execute_batch(
                "CREATE TABLE _generation_catalog_fixture(watermark INTEGER NOT NULL)",
            )?;
            tx.execute(
                "INSERT INTO _generation_catalog_fixture VALUES(?1)",
                [integer(watermark)?],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    runner
        .publish_with(lock, 101, |_, _| {
            panic!("committed finalizer must not repeat")
        })
        .await
        .unwrap();
    let command = Write {
        op_id: "01a3185c-5000-7000-8000-000000000001".into(),
        prev_op_id: None,
        collection: "runtime_unknown_documents".into(),
        id: "runtime-doc".into(),
        base_rev: None,
        base_seq: None,
        gen: 1,
        schema_ver: 1,
        app_ver: "1".into(),
        op: WriteOp::Put,
        body: Some(json!({"v":42})),
    };
    let result = write_contract::OrderedWriter::new(writer.clone(), Arc::new(NativePolicy))
        .submit(
            "alice".into(),
            OrderedWrite {
                client_seq: Seq(1),
                write: command,
            },
            1_800_000_000_000,
        )
        .await
        .unwrap();
    assert_eq!(result.results[0].result.status, WriteStatus::Accepted);
    let mut installed = Subscription::new(
        readers.clone(),
        &writer,
        generation_scope(1, "runtime_unknown_documents"),
        None,
    )
    .unwrap();
    let page = next(&mut installed, &key, 10).await;
    assert_eq!(page.docs[0].body.as_ref().unwrap()["v"], 42);
    let continued = next(&mut existing, &key, 10).await;
    assert!(continued.docs.is_empty());
    assert_eq!(continued.kind, StreamKind::Live);
    assert_eq!(continued.seq_to, page.seq_to);
    assert_eq!(
        writer.generation(),
        incarnation,
        "same writer and reader pool must continue"
    );
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn prepared_writes_mirror_both_generations_in_the_ordered_writer_transaction() {
    let (_dir, runner, writer, readers) = generation_runtime().await;
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            crate::schema::create_collection(tx, &definition("stable"))?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    runner
        .prepare(generation_lock(), next_definition())
        .await
        .unwrap();
    let input = Write {
        op_id: "01a3185c-5000-7000-8000-000000000001".into(),
        prev_op_id: None,
        collection: "items".into(),
        id: "one".into(),
        base_rev: Some("old".into()),
        base_seq: Some(Seq(1)),
        gen: 1,
        schema_ver: 1,
        app_ver: "1".into(),
        op: WriteOp::Put,
        body: Some(json!({"v":2})),
    };
    let unprepared = write_contract::OrderedWriter::new(writer.clone(), Arc::new(NativePolicy));
    assert!(unprepared
        .submit(
            "alice".into(),
            OrderedWrite {
                client_seq: Seq(1),
                write: input.clone()
            },
            1_800_000_000_000
        )
        .await
        .is_err());
    readers
        .read(|tx| {
            assert_eq!(change_log::head(tx)?.head_seq, 1);
            assert_eq!(
                tx.query_row("SELECT count(*) FROM c_items__g2", [], |r| r
                    .get::<_, u64>(0))?,
                0
            );
            Ok(())
        })
        .await
        .unwrap();
    struct Prepared;
    impl WriteAuthority for Prepared {
        fn authorize(
            &self,
            tx: &Transaction<'_>,
            p: &str,
            w: &Write,
        ) -> rusqlite::Result<WritePermission> {
            WriteAuthority::authorize(&NativePolicy, tx, p, w)
        }
        fn validate_document(
            &self,
            _: &Transaction<'_>,
            _: &str,
            _: &crate::schema::Collection,
            _: &Value,
        ) -> rusqlite::Result<WritePermission> {
            Ok(WritePermission::Permit)
        }
        fn generation_output(
            &self,
            _: &Transaction<'_>,
            _: &str,
            w: &Write,
            target: &crate::schema::Collection,
            document: Option<&Value>,
        ) -> rusqlite::Result<Option<crate::schema::MigrationOutput>> {
            assert_eq!(target.gen, 2);
            assert_eq!(document.unwrap()["v"], 2);
            Ok(Some(crate::schema::MigrationOutput::Upsert {
                rev: format!("mirror:{}", w.op_id),
                body: json!({"v":"2"}),
            }))
        }
    }
    let prepared = write_contract::OrderedWriter::new(writer.clone(), Arc::new(Prepared));
    let result = prepared
        .submit(
            "alice".into(),
            OrderedWrite {
                client_seq: Seq(1),
                write: input.clone(),
            },
            1_800_000_000_000,
        )
        .await
        .unwrap();
    assert_eq!(result.results[0].result.status, WriteStatus::Accepted);
    readers
        .read(|tx| {
            let old = change_log::read_page(tx, "items", 1, 0, 10)?;
            let new = change_log::read_page(tx, "items", 2, 0, 10)?;
            assert_eq!(old.docs[0].body.as_ref().unwrap()["v"], 2);
            assert_eq!(new.docs[0].body.as_ref().unwrap()["v"], "2");
            assert_eq!(old.docs[0].seq + 1, new.docs[0].seq);
            Ok(())
        })
        .await
        .unwrap();
    runner
        .migrate_batch(generation_lock(), "items".into(), 100, transformed)
        .await
        .unwrap();
    let watermark = runner.publish(generation_lock(), 101).await.unwrap();
    let key = TestInstanceKey::new();
    let mut sub =
        Subscription::new(readers.clone(), &writer, generation_scope(2, "items"), None).unwrap();
    next(&mut sub, &key, 10).await;
    let checkpoint = sub.checkpoint().unwrap();
    let mut stable = Subscription::new(
        readers.clone(),
        &writer,
        generation_scope(1, "stable"),
        None,
    )
    .unwrap();
    next(&mut stable, &key, 10).await;
    let stable_checkpoint = stable.checkpoint().unwrap();
    readers
        .read(move |tx| {
            assert!(
                crate::schema::validate_activation_checkpoints(
                    tx,
                    "isolated-core",
                    "alice",
                    "test",
                    "2",
                    &[checkpoint.clone()],
                    &NativePolicy
                )
                .is_err(),
                "unchanged collections need a checkpoint too"
            );
            let good = crate::schema::validate_activation_checkpoints(
                tx,
                "isolated-core",
                "alice",
                "test",
                "2",
                &[checkpoint.clone(), stable_checkpoint.clone()],
                &NativePolicy,
            )?;
            assert_eq!(good, watermark);
            let mut stale = checkpoint.clone();
            stale.gen = 1;
            assert!(crate::schema::validate_activation_checkpoints(
                tx,
                "isolated-core",
                "alice",
                "test",
                "2",
                &[stale, stable_checkpoint.clone()],
                &NativePolicy
            )
            .is_err());
            let mut behind = checkpoint.clone();
            behind.seq = Seq(watermark - 1);
            assert!(crate::schema::validate_activation_checkpoints(
                tx,
                "isolated-core",
                "alice",
                "test",
                "2",
                &[behind, stable_checkpoint.clone()],
                &NativePolicy
            )
            .is_err());
            let mut stable_behind = stable_checkpoint.clone();
            stable_behind.seq = Seq(watermark - 1);
            assert!(
                crate::schema::validate_activation_checkpoints(
                    tx,
                    "isolated-core",
                    "alice",
                    "test",
                    "2",
                    &[checkpoint.clone(), stable_behind],
                    &NativePolicy
                )
                .is_err(),
                "same-gen collections use the publication watermark"
            );
            let mut foreign = checkpoint;
            foreign.participant = "mallory".into();
            assert!(crate::schema::validate_activation_checkpoints(
                tx,
                "isolated-core",
                "alice",
                "test",
                "2",
                &[foreign, stable_checkpoint],
                &NativePolicy
            )
            .is_err());
            Ok(())
        })
        .await
        .unwrap();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn pending_generation_reopens_and_bootstraps_new_publication_receipts() {
    let (_dir, runner, writer, readers) = generation_runtime().await;
    runner
        .prepare(generation_lock(), next_definition())
        .await
        .unwrap();
    let ready = runner
        .migrate_batch(generation_lock(), "items".into(), 100, transformed)
        .await
        .unwrap();
    let db = writer.db_id().clone();
    let incarnation = writer.generation();
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute_batch("DROP TABLE _schema_publications")?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
    drop(runner);
    let writer = Registry
        .open(
            db.clone(),
            WriterOptions {
                configure: Some(Arc::new(|c| {
                    ConnectionConfig::default().apply_writer(c)?;
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
    assert_ne!(writer.generation(), incarnation);
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
    let runner = crate::schema::GenerationRunner::new(writer.clone(), readers.clone()).unwrap();
    assert_eq!(
        runner
            .migrate_batch(generation_lock(), "items".into(), 101, |_| panic!(
                "persisted ready batch must not repeat"
            ))
            .await
            .unwrap(),
        ready
    );
    let watermark = runner.publish(generation_lock(), 102).await.unwrap();
    assert_eq!(
        runner.publish(generation_lock(), 103).await.unwrap(),
        watermark
    );
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}

impl crate::schema::ActivationAuthority for NativePolicy {
    fn authorize_app(
        &self,
        _: &Transaction<'_>,
        participant: &str,
        _: &str,
        _: &str,
    ) -> rusqlite::Result<()> {
        if participant != "alice" {
            return Err(invalid("app activation forbidden"));
        }
        Ok(())
    }
}
#[tokio::test]
async fn core_activation_is_an_authenticated_client_ack_after_the_generation_barrier() {
    let (_dir, runner, writer, readers) = generation_runtime().await;
    runner
        .prepare(generation_lock(), next_definition())
        .await
        .unwrap();
    runner
        .migrate_batch(generation_lock(), "items".into(), 100, transformed)
        .await
        .unwrap();
    let watermark = runner.publish(generation_lock(), 101).await.unwrap();
    readers
        .read(|tx| {
            assert!(
                crate::schema::client_activation(tx, "alice", "test", "2")?.is_none(),
                "publish never infers client activation"
            );
            Ok(())
        })
        .await
        .unwrap();
    let received = AppActivation {
        app_id: "test".into(),
        app_ver: "2".into(),
        state: AppActivationState::Received,
        at_seq: Seq(0),
        checkpoints: vec![],
        reason: None,
    };
    let json = serde_json::to_value(ClientMessage::AppActivation(received.clone())).unwrap();
    assert_eq!(json["type"], "app_activation");
    assert_eq!(json["at_seq"], "0");
    assert!(runner
        .acknowledge_activation(
            "isolated-core".into(),
            "mallory".into(),
            received.clone(),
            1000,
            Arc::new(NativePolicy)
        )
        .await
        .is_err());
    runner
        .acknowledge_activation(
            "isolated-core".into(),
            "alice".into(),
            received.clone(),
            1001,
            Arc::new(NativePolicy),
        )
        .await
        .unwrap();
    let key = TestInstanceKey::new();
    let mut sub =
        Subscription::new(readers.clone(), &writer, generation_scope(2, "items"), None).unwrap();
    next(&mut sub, &key, 10).await;
    let checkpoint = sub.checkpoint().unwrap();
    let mut activated = AppActivation {
        state: AppActivationState::Activated,
        at_seq: Seq(watermark),
        checkpoints: vec![checkpoint],
        ..received.clone()
    };
    activated.checkpoints[0].gen = 1;
    assert!(runner
        .acknowledge_activation(
            "isolated-core".into(),
            "alice".into(),
            activated.clone(),
            1002,
            Arc::new(NativePolicy)
        )
        .await
        .is_err());
    readers
        .read(|tx| {
            assert_eq!(
                crate::schema::client_activation(tx, "alice", "test", "2")?
                    .unwrap()
                    .state,
                AppActivationState::Received,
                "rejected report cannot advance state"
            );
            Ok(())
        })
        .await
        .unwrap();
    activated.checkpoints[0].gen = 2;
    let first = runner
        .acknowledge_activation(
            "isolated-core".into(),
            "alice".into(),
            activated.clone(),
            1003,
            Arc::new(NativePolicy),
        )
        .await
        .unwrap();
    assert_eq!(first.state, AppActivationState::Activated);
    let replay = runner
        .acknowledge_activation(
            "isolated-core".into(),
            "alice".into(),
            activated,
            1004,
            Arc::new(NativePolicy),
        )
        .await
        .unwrap();
    assert_eq!(first, replay, "same report retains its original receipt");
    assert!(
        runner
            .acknowledge_activation(
                "isolated-core".into(),
                "alice".into(),
                AppActivation {
                    at_seq: Seq(watermark),
                    ..received
                },
                1005,
                Arc::new(NativePolicy)
            )
            .await
            .is_err(),
        "late received cannot downgrade activated"
    );
    let db = writer.db_id().clone();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
    let mut reopened = rusqlite::Connection::open(db.path()).unwrap();
    let tx = reopened.transaction().unwrap();
    assert_eq!(
        crate::schema::client_activation(&tx, "alice", "test", "2").unwrap(),
        Some(first.clone())
    );
    let wire = serde_json::to_value(CoreMessage::AppActivationResult(first)).unwrap();
    assert_eq!(wire["type"], "app_activation_result");
    assert_eq!(wire["at_seq"], watermark.to_string());
}
