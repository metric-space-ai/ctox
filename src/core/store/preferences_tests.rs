use super::*;
use crate::change_log::subscription::protocol::*;
use crate::change_log::subscription::{write_contract::submit_in_transaction, *};
use crate::{
    config::{AdmissionBudget, ConnectionConfig},
    read_pool::ReadPool,
    registry::Registry,
    schema,
    writer::WriterOptions,
    DbId, WriteJob,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
const NOW: u64 = 1_800_000_000_000;
struct Auth(Context);
impl WriteAuthority for Auth {
    fn preference_context(&self, p: &str) -> Option<Context> {
        (p == "session").then(|| self.0.clone())
    }
    fn authorize(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &Write,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Permit)
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &schema::Collection,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Permit)
    }
}
impl ReadAuthority for Auth {
    fn preference_context(&self, p: &str) -> Option<Context> {
        (p == "session").then(|| self.0.clone())
    }
    fn authorize(&self, _: &Transaction<'_>, _: &SubscriptionKey) -> rusqlite::Result<()> {
        Ok(())
    }
    fn project(
        &self,
        _: &Transaction<'_>,
        _: &SubscriptionKey,
        d: &crate::change_log::ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        Ok(d.body.clone())
    }
}
fn definition() -> schema::CollectionDefinition {
    let input: Value = serde_json::from_str(include_str!(
        "../../apps/business-os/v4/schemas/ctox-platform.collections.schema.json"
    ))
    .unwrap();
    schema::CollectionDefinition {
        name: COLLECTION.into(),
        app_id: "ctox-platform".into(),
        schema_ver: 0,
        schema_json: input["collections"][COLLECTION]["schema"].clone(),
        class: "full".into(),
        client_limit: 0,
        byte_budget: 17408,
        rights_ver: 1,
        app_ver: "4.0".into(),
    }
}
fn write(n: u16, id: &str, body: Option<Value>, base: Option<String>) -> OrderedWrite {
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&NOW.to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[14..].copy_from_slice(&n.to_be_bytes());
    OrderedWrite {
        client_seq: Seq(n.into()),
        write: Write {
            op_id: uuid::Uuid::from_bytes(bytes).to_string(),
            prev_op_id: None,
            collection: COLLECTION.into(),
            id: id.into(),
            base_rev: base,
            expected_revision: None,
            base_seq: None,
            gen: 1,
            schema_ver: 0,
            app_ver: "4.0".into(),
            op: if body.is_some() {
                WriteOp::Put
            } else {
                WriteOp::Delete
            },
            body,
        },
    }
}
fn submit(
    tx: &Transaction<'_>,
    auth: &Auth,
    n: u16,
    doc: Value,
    base: Option<String>,
) -> WriteResult {
    let w = write(n, doc["id"].as_str().unwrap(), Some(doc.clone()), base);
    submit_in_transaction(tx, "session", &w, NOW, auth)
        .unwrap()
        .results
        .pop()
        .unwrap()
        .result
}
#[test]
fn prefs_context_rejects_payload_identity_and_bounds_keys_and_values() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    schema::create_collection(&tx, &definition()).unwrap();
    let ctx = Context::authenticated("calendar", "alice", 1).unwrap();
    let auth = Auth(ctx.clone());
    assert_ne!(
        ctx.scope_hash(),
        Context::authenticated("calendar", "bob", 1)
            .unwrap()
            .scope_hash()
    );
    assert_ne!(
        ctx.scope_hash(),
        Context::authenticated("mail", "alice", 1)
            .unwrap()
            .scope_hash()
    );
    let foreign = Context::authenticated("calendar", "bob", 1)
        .unwrap()
        .document("theme", json!("secret"))
        .unwrap();
    assert_eq!(
        submit(&tx, &auth, 1, foreign, None).code.as_deref(),
        Some("forbidden")
    );
    let foreign = Context::authenticated("mail", "alice", 1)
        .unwrap()
        .document("theme", json!("secret"))
        .unwrap();
    assert_eq!(
        submit(&tx, &auth, 2, foreign, None).code.as_deref(),
        Some("forbidden")
    );
    let mut spoof = ctx.document("theme", json!(true)).unwrap();
    spoof["user_id"] = json!("bob");
    assert_eq!(
        submit(&tx, &auth, 3, spoof, None).code.as_deref(),
        Some("forbidden")
    );
    let ok = ctx
        .document("theme", json!([1,true,null,{"x":"ü"}]))
        .unwrap();
    assert_ne!(
        submit(&tx, &auth, 4, ok, None).status,
        WriteStatus::Rejected
    );
    assert!(ctx.id("A").is_err());
    assert!(ctx.id(&"x".repeat(65)).is_err());
    assert!(ctx.id("../bad").is_err());
    assert!(ctx.id(&"x".repeat(64)).is_ok());
    let mut huge = ctx.document("big", json!(null)).unwrap();
    huge["value"] = json!("ü".repeat(8192));
    assert_eq!(
        submit(&tx, &auth, 5, huge, None).code.as_deref(),
        Some("quota")
    );
    let mut extra = ctx.document("extra", json!(null)).unwrap();
    extra["actor"] = json!("alice");
    assert_eq!(
        submit(&tx, &auth, 6, extra, None).code.as_deref(),
        Some("invalid")
    );
}
#[test]
fn prefs_writer_quota_is_atomic_updates_and_delete_release_capacity() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    schema::create_collection(&tx, &definition()).unwrap();
    let ctx = Context::authenticated("calendar", "alice", 1).unwrap();
    let auth = Auth(ctx.clone());
    let mut first = None;
    for n in 1..=64 {
        let result = submit(
            &tx,
            &auth,
            n,
            ctx.document(&format!("k{n}"), json!(n)).unwrap(),
            None,
        );
        assert_ne!(result.status, WriteStatus::Rejected);
        if n == 1 {
            first = result.rev;
        }
    }
    let before = crate::change_log::head(&tx).unwrap().head_seq;
    let full = submit(
        &tx,
        &auth,
        65,
        ctx.document("overflow", json!(1)).unwrap(),
        None,
    );
    assert_eq!(full.code.as_deref(), Some("quota"));
    assert_eq!(crate::change_log::head(&tx).unwrap().head_seq, before);
    let update = submit(
        &tx,
        &auth,
        66,
        ctx.document("k1", json!("changed")).unwrap(),
        first,
    );
    assert_ne!(update.status, WriteStatus::Rejected);
    let del = write(67, &ctx.id("k1").unwrap(), None, update.rev);
    assert_ne!(
        submit_in_transaction(&tx, "session", &del, NOW, &auth)
            .unwrap()
            .results[0]
            .result
            .status,
        WriteStatus::Rejected
    );
    assert_ne!(
        submit(
            &tx,
            &auth,
            68,
            ctx.document("overflow", json!(1)).unwrap(),
            None
        )
        .status,
        WriteStatus::Rejected
    );
    let replay = write(
        68,
        &ctx.id("overflow").unwrap(),
        Some(ctx.document("overflow", json!(1)).unwrap()),
        None,
    );
    assert_ne!(
        submit_in_transaction(&tx, "session", &replay, NOW, &auth)
            .unwrap()
            .results[0]
            .result
            .status,
        WriteStatus::Rejected
    );
}
struct Signer;
impl InstanceSigner for Signer {
    fn instance_id(&self) -> &str {
        "isolated"
    }
    fn key_id(&self) -> &str {
        "test-only"
    }
    fn sign(&self, _: &[u8]) -> Result<Vec<u8>, String> {
        Ok(vec![0; 64])
    }
}
#[tokio::test]
async fn prefs_stream_never_discloses_foreign_app_user_or_tombstone_even_to_admin_authority() {
    let dir = tempfile::tempdir().unwrap();
    let db = DbId::new(dir.path().join("prefs.sqlite")).unwrap();
    let writer = Registry
        .open(
            db.clone(),
            WriterOptions {
                configure: Some(Arc::new(|db| {
                    ConnectionConfig::default().apply_writer(db)?;
                    let tx = db.unchecked_transaction()?;
                    schema::create_collection(&tx, &definition())?;
                    tx.commit()?;
                    Ok(())
                })),
                head_seq: Some(Arc::new(|db| {
                    db.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| r.get(0))
                        .map_err(Into::into)
                })),
            },
        )
        .unwrap();
    writer.ready().await.unwrap();
    writer
        .submit(WriteJob::new(Default::default(), |tx| {
            for (n, (app, user)) in [
                ("calendar", "alice"),
                ("mail", "alice"),
                ("calendar", "bob"),
            ]
            .into_iter()
            .enumerate()
            {
                let ctx = Context::authenticated(app, user, 1).unwrap();
                let doc = ctx.document("theme", json!("private")).unwrap();
                let mut w = write(
                    (n + 1) as u16,
                    doc["id"].as_str().unwrap(),
                    Some(doc.clone()),
                    None,
                );
                w.client_seq = Seq(1);
                let result = submit_in_transaction(
                    tx,
                    &format!("{app}:{user}"),
                    &w,
                    NOW,
                    &Allow(ctx.clone()),
                )?;
                assert_ne!(result.results[0].result.status, WriteStatus::Rejected);
                if app == "mail" {
                    let mut del = write(
                        10,
                        doc["id"].as_str().unwrap(),
                        None,
                        result.results[0].result.rev.clone(),
                    );
                    del.client_seq = Seq(2);
                    assert_ne!(
                        submit_in_transaction(
                            tx,
                            &format!("{app}:{user}"),
                            &del,
                            NOW,
                            &Allow(ctx)
                        )?
                        .results[0]
                            .result
                            .status,
                        WriteStatus::Rejected
                    );
                }
            }
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let ctx = Context::authenticated("calendar", "alice", 1).unwrap();
    let pool = ReadPool::for_database(
        &db,
        writer.generation(),
        ConnectionConfig::default(),
        AdmissionBudget::new(
            std::num::NonZeroUsize::new(2).unwrap(),
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let key = SubscriptionKey {
        participant: "session".into(),
        collection: COLLECTION.into(),
        scope_hash: ctx.scope_hash(),
        gen: 1,
    };
    let mut sub = Subscription::new(pool.clone(), &writer, key.clone(), None).unwrap();
    let page = sub
        .next_page(100, Arc::new(Auth(ctx.clone())), &Signer, || NOW)
        .await
        .unwrap();
    assert_eq!(page.docs.len(), 1);
    assert_eq!(page.docs[0].id, "calendar/alice/theme");
    assert!(page.docs[0].body.is_some());
    writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            let id = ctx.id("theme").unwrap();
            let current = schema::active_collection(tx, COLLECTION)?.unwrap();
            let table = crate::change_log::identifier(&current.table)?;
            let rev = tx.query_row(
                &format!("SELECT rev FROM {table} WHERE id=?1"),
                [&id],
                |r| r.get(0),
            )?;
            let mut del = write(20, &id, None, Some(rev));
            del.client_seq = Seq(1);
            assert_ne!(
                submit_in_transaction(tx, "session", &del, NOW, &Auth(ctx))?.results[0]
                    .result
                    .status,
                WriteStatus::Rejected
            );
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let page = sub
        .next_page(
            100,
            Arc::new(Auth(
                Context::authenticated("calendar", "alice", 1).unwrap(),
            )),
            &Signer,
            || NOW,
        )
        .await
        .unwrap();
    assert_eq!(page.docs.len(), 1);
    assert_eq!(page.docs[0].op, DocumentOp::Delete);
    assert_eq!(page.docs[0].id, "calendar/alice/theme");
    let mut wrong = key;
    wrong.scope_hash = Context::authenticated("calendar", "bob", 1)
        .unwrap()
        .scope_hash();
    let mut sub = Subscription::new(pool.clone(), &writer, wrong, None).unwrap();
    assert!(sub
        .next_page(
            100,
            Arc::new(Auth(
                Context::authenticated("calendar", "alice", 1).unwrap()
            )),
            &Signer,
            || NOW
        )
        .await
        .is_err());
    pool.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
struct Allow(Context);
impl WriteAuthority for Allow {
    fn preference_context(&self, _: &str) -> Option<Context> {
        Some(self.0.clone())
    }
    fn authorize(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &Write,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Permit)
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &schema::Collection,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Permit)
    }
}
