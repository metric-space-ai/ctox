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
    fn authorize(&self, _: &Transaction<'_>, participant: &str, _: &Write) -> rusqlite::Result<()> {
        if participant != "alice" {
            return Err(invalid("forbidden"));
        }
        Ok(())
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &crate::schema::Collection,
        doc: &Value,
    ) -> rusqlite::Result<()> {
        if !doc["v"].is_number() {
            return Err(invalid("v must be numeric"));
        }
        Ok(())
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
struct TestInstanceKey {
    _dir: tempfile::TempDir,
    private: PathBuf,
    public: PathBuf,
}
impl TestInstanceKey {
    fn new() -> Self {
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
    broken.payload[30] ^= 1;
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
