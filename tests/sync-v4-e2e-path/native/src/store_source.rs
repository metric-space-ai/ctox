//! Test-only host adapter over the REAL P8 Store, writer and commit subscriptions.
use ctox_store::{
    change_log::{
        self,
        subscription::{
            self, protocol::*, ReadAuthority, Subscription, SubscriptionKey, WriteAuthority,
        },
    },
    config::{AdmissionBudget, ConnectionConfig},
    read_pool::ReadPool,
    registry::Registry,
    writer::{WriterHandle, WriterOptions},
    DbId, JobOptions, WriteJob,
};
use futures::{
    future::BoxFuture,
    stream::{self, BoxStream},
    StreamExt,
};
use ring::signature::KeyPair;
use rusqlite::Transaction;
use rxdb::{
    plugins::replication_webrtc::{
        index_mod::v4_session::V4StreamSource, webrtc_types::WebRTCPublicationGuard,
    },
    rx_error::{new_rx_error, RxResult},
};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[path = "../../../../src/core/store/rights.rs"]
pub mod rights;
pub use rights::scope;
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn error(e: impl std::fmt::Display) -> rxdb::rx_error::RxError {
    eprintln!("e2e store: {e}");
    new_rx_error("e2e_store", None)
}
pub struct Key;
impl scope::InstanceSigningKeyStore for Key {
    fn with_instance_signing_key<T>(
        &self,
        f: impl FnOnce(&[u8]) -> scope::Result<T>,
    ) -> scope::Result<T> {
        let mut bytes = vec![
            0x30, 0x53, 0x02, 0x01, 0x01, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        bytes.extend([42; 32]);
        bytes.extend([0xa1, 0x23, 0x03, 0x21, 0x00]);
        bytes.extend(Self::pair().public_key().as_ref());
        f(&bytes)
    }
}
// InstanceSigningKeyStore expects PKCS#8; public fixture bytes, never credentials.
impl Key {
    pub fn pair() -> ring::signature::Ed25519KeyPair {
        ring::signature::Ed25519KeyPair::from_seed_unchecked(&[42; 32]).unwrap()
    }
}
impl InstanceSigner for Key {
    fn instance_id(&self) -> &str {
        "e2e-fixture"
    }
    fn key_id(&self) -> &str {
        "public-fixture-42"
    }
    fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        Ok(Self::pair().sign(bytes).as_ref().to_vec())
    }
}
pub fn resolved() -> scope::ResolvedScope {
    scope::ResolvedScope::new(
        1,
        [
            "notes",
            "business_commands",
            "business_apps",
            "business_app_versions",
            "files",
            "shell_preferences",
            "app_drafts",
        ]
        .into_iter()
        .map(|name| scope::Projection {
            collection: name.into(),
            row_filter: scope::RowFilter::All,
            field_mask: scope::FieldMask::All,
        })
        .collect(),
        "alice",
    )
    .unwrap()
}
struct Policy {
    hash: String,
}
impl ReadAuthority for Policy {
    fn authorize(&self, _: &Transaction<'_>, k: &SubscriptionKey) -> rusqlite::Result<()> {
        if k.participant != "alice"
            || k.scope_hash != self.hash
            || resolved().projection(&k.collection).is_none()
        {
            return Err(rusqlite::Error::InvalidParameterName(format!("fixture read authority denied participant={} collection={} scope={}", k.participant, k.collection, k.scope_hash)));
        }
        Ok(())
    }
    fn project(
        &self,
        _: &Transaction<'_>,
        _: &SubscriptionKey,
        d: &change_log::ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        Ok(d.body.clone())
    }
}
impl WriteAuthority for Policy {
    fn authorize(&self, _: &Transaction<'_>, actor: &str, r: &Write) -> rusqlite::Result<subscription::WritePermission> {
        if actor != "alice" || resolved().projection(&r.collection).is_none() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(subscription::WritePermission::Permit)
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &ctox_store::schema::Collection,
        doc: &Value,
    ) -> rusqlite::Result<subscription::WritePermission> {
        if !doc.is_object() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(subscription::WritePermission::Permit)
    }
}
#[derive(Clone)]
pub struct Source {
    pub writer: WriterHandle,
    reader: ReadPool,
    peers: Arc<Mutex<HashSet<String>>>,
    results: tokio::sync::broadcast::Sender<Value>,
    pub drop_next: Arc<std::sync::atomic::AtomicBool>,
}
impl Source {
    pub async fn open(path: &str) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let db = DbId::new(path)?;
        let writer=Registry.open(db.clone(),WriterOptions{head_seq:Some(Arc::new(|c|Ok(c.query_row("SELECT v FROM _seq WHERE singleton=1",[],|r|r.get(0))?))),configure:Some(Arc::new(|c|{ConnectionConfig::default().apply_writer(c)?;let tx=c.unchecked_transaction()?;
   for name in ["notes","business_commands","business_apps","business_app_versions","files","shell_preferences","app_drafts"]{ctox_store::schema::create_collection(&tx,&ctox_store::schema::CollectionDefinition{name:name.into(),app_id:(if name=="notes"{"notes"}else{"ctox-platform"}).into(),schema_ver:1,schema_json:json!({"type":"object","properties":{}}),class:"full".into(),client_limit:10000,byte_budget:10000000,rights_ver:1,app_ver:"1".into()})?;}
   tx.execute_batch("CREATE TABLE IF NOT EXISTS e2e_activations(app_id TEXT PRIMARY KEY,report TEXT NOT NULL); INSERT INTO c_notes VALUES('seed','r0',1,'{\"id\":\"seed\",\"title\":\"actual SQLite seed\"}',0);")?;tx.commit()?;Ok(())}))})?;
        writer.ready().await?;
        // Register the data-only test app through the real platform collections.
        // API reads are local; the kernel subscribes app collections from manifests.
        // No UI/artifact-loading acceptance is claimed by this transport bench.
        writer.submit(WriteJob::new(JobOptions::default(), |tx| {
            let app = json!({"id":"notes","active_ver":"1","migrated_at_seq":"0"});
            let version = json!({"id":"notes@1","app_id":"notes","app_ver":"1","migrated_at_seq":"0","manifest":{
                "app_id":"notes","app_ver":"1","entry":"","files":{},
                "collections":{"notes":{"schema":{"type":"object","properties":{}}}},
                "generations":{},"retires":{},"layout":{},"styles":[],"handlers":{},
                "min_app_api":"4.0","capabilities_required":[],
                "provenance":{"base_release":null,"base_hash":null,"locally_modified":false},
                "author":"native-fixture","created_at":"2026-10-15T00:00:00Z"
            }});
            tx.execute("INSERT INTO c_business_apps VALUES('notes','r0',1,?1,0)",[app.to_string()])?;
            tx.execute("INSERT INTO c_business_app_versions VALUES('notes@1','r0',1,?1,0)",[version.to_string()])?;
            Ok(())
        }))?.await?;
        let reader = ReadPool::for_database(
            &db,
            writer.generation(),
            ConnectionConfig::default(),
            AdmissionBudget::new(
                std::num::NonZeroUsize::new(32).unwrap(),
                Duration::from_secs(2),
                Duration::from_secs(1),
            )
            .unwrap(),
        )
        .await?;
        let (results, _) = tokio::sync::broadcast::channel(128);
        Ok(Arc::new(Self {
            writer,
            reader,
            peers: Arc::new(Mutex::new(HashSet::new())),
            results,
            drop_next: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }))
    }
    pub fn admit(&self, peer: String) {
        self.peers.lock().unwrap().insert(peer);
    }
    fn check(&self, peer: &str) -> RxResult<()> {
        if self.peers.lock().unwrap().contains(peer) {
            Ok(())
        } else {
            Err(error("unauthenticated peer"))
        }
    }
    pub async fn head(&self) -> RxResult<(String, u64)> {
        self.reader
            .read(|tx| {
                let h = change_log::head(tx)?;
                Ok((h.epoch, h.head_seq))
            })
            .await
            .map_err(error)
    }
    pub async fn shutdown(&self) {
        let _ = self.reader.shutdown().await;
        let _ = self.writer.shutdown().await;
    }
    pub async fn expire(&self)->RxResult<()> {
        let ordered=subscription::write_contract::OrderedWriter::new(self.writer.clone(),Arc::new(Policy{hash:resolved().hash().map_err(error)?}));
        let batch=ordered.expire("alice".into(),now()).await.map_err(error)?;
        crate::emit(json!({"kind":"gap_deadline","at_ms":batch.next_deadline_ms}));
        for result in batch.results {
            let mut value=serde_json::to_value(result).map_err(error)?;value["type"]="write_result".into();
            if self.results.receiver_count()>0 {self.results.send(value).map_err(error)?;} else {crate::emit(json!({"kind":"result_retained_offline","op_id":value["op_id"]}));}
        }
        Ok(())
    }
}
struct Guard;
impl WebRTCPublicationGuard for Guard {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        publish()
    }
}
impl V4StreamSource for Source {
    fn publication(
        &self,
        peer: &str,
        _: &Value,
    ) -> RxResult<Arc<dyn WebRTCPublicationGuard>> {
        self.check(peer)?;
        Ok(Arc::new(Guard))
    }
    fn subscribe(
        &self,
        peer: &str,
        request: Value,
    ) -> BoxFuture<'static, RxResult<BoxStream<'static, RxResult<Value>>>> {
        let source = self.clone();
        let checked = self.check(peer);
        Box::pin(async move {
            checked?;
            let ClientMessage::Subscribe(sub) = serde_json::from_value(request).map_err(error)?
            else {
                return Err(error("subscribe required"));
            };
            crate::emit(json!({"kind":"subscribe","collection":sub.collection}));
            let hash = resolved().hash().map_err(error)?;
            if sub.scope_hash != hash {
                return Err(error("scope mismatch"));
            }
            let subscription = Subscription::new(
                source.reader.clone(),
                &source.writer,
                SubscriptionKey {
                    participant: "alice".into(),
                    collection: sub.collection,
                    scope_hash: hash.clone(),
                    gen: sub.gen,
                },
                sub.checkpoint,
            )
            .map_err(error)?;
            let pages = stream::unfold((subscription, hash), |(mut sub, hash)| async move {
                let page = sub
                    .next_page(256, Arc::new(Policy { hash: hash.clone() }), &Key, now)
                    .await;
                if let Ok(packet) = &page {crate::emit(json!({"kind":"page_produced","collection":packet.collection,"seq_to":packet.seq_to,"docs":packet.docs.iter().map(|d|d.id.clone()).collect::<Vec<_>>()}));}
                Some((
                    page.map_err(error)
                        .and_then(|p| serde_json::to_value(p).map_err(error)),
                    (sub, hash),
                ))
            })
            .boxed();
            Ok(pages)
        })
    }
    fn write(&self, peer: &str, request: Value) -> BoxFuture<'static, RxResult<()>> {
        let source = self.clone();
        let checked = self.check(peer);
        Box::pin(async move {
            checked?;
            let ClientMessage::Write(write) = serde_json::from_value(request).map_err(error)?
            else {
                return Err(error("write required"));
            };
            let ordered=subscription::write_contract::OrderedWriter::new(source.writer.clone(),Arc::new(Policy{hash:resolved().hash().map_err(error)?}));
            let batch=ordered.submit("alice".into(),write,now())
                .await
                .map_err(error)?;
            crate::emit(json!({"kind":"gap_deadline","at_ms":batch.next_deadline_ms}));
            for result in batch.results {
                crate::emit(json!({"kind":"write_committed","op_id":result.result.op_id,"seq":result.result.seq,"status":result.result.status,"reason":result.result.reason,"client_seq":result.client_seq}));
                if source.drop_next.swap(false,std::sync::atomic::Ordering::SeqCst){std::future::pending::<()>().await;}
                let mut value=serde_json::to_value(result).map_err(error)?;value["type"]="write_result".into();source.results.send(value).map_err(error)?;
            }
            Ok(())
        })
    }
    fn write_results(&self, peer: &str) -> RxResult<BoxStream<'static, RxResult<Value>>> {
        self.check(peer)?;
        let receiver = self.results.subscribe();
        Ok(stream::unfold(receiver, |mut receiver| async move {
            match receiver.recv().await {
                Ok(value) => Some((Ok(value), receiver)),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
                Err(error_value) => Some((Err(error(error_value)), receiver)),
            }
        })
        .boxed())
    }
    fn app_activation(&self, peer: &str, report: Value) -> BoxFuture<'static, RxResult<()>> {
        let source = self.clone();
        let checked = self.check(peer);
        Box::pin(async move {
            checked?;
            source.writer.submit(WriteJob::new(JobOptions::default(),move|tx|{tx.execute("INSERT INTO e2e_activations VALUES(?1,?2) ON CONFLICT(app_id) DO UPDATE SET report=excluded.report",rusqlite::params![report["app_id"].as_str().unwrap_or(""),report.to_string()])?;Ok(())})).map_err(error)?.await.map_err(error)?;
            crate::emit(json!({"kind":"activation_committed"}));
            Ok(())
        })
    }
    fn priority(&self, peer: &str, _: Value) -> BoxFuture<'static, RxResult<()>> {
        let checked = self.check(peer);
        Box::pin(async move { checked })
    }
}
