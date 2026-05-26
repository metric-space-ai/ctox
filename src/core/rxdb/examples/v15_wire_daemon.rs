//! Cross-process V1.5 wire daemon for E2E testing.
//!
//! Reads newline-delimited JSON RPC messages from stdin, dispatches them
//! through the real `run_query_fetch` / `run_file_fetch` paths, and writes
//! every emitted wire frame (response + chunk + error) as newline-delimited
//! JSON on stdout. The JS-side test driver spawns this binary, feeds it
//! requests, and reads the resulting frames — proving Rust↔JS bytes work
//! across a process boundary, not just in-process.
//!
//! Stdin frames:
//!   {"kind":"request","peerIdentity":"p1","message":{ id, method, params }}
//!   {"kind":"seed","collection":"...","docs":[ ... ]}
//!   {"kind":"shutdown"}
//!
//! Stdout frames:
//!   {"kind":"wire","peerIdentity":"p1","frame":<WebRTCWireFrame>}
//!   {"kind":"ready"}
//!   {"kind":"error","message":"..."}
//!
//! Run with: cargo run --release --example v15_wire_daemon

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::{json, Value};

use rxdb::plugins::replication_webrtc::file_fetch_handler::{
    run_file_fetch, FileFetchRegistry,
};
use rxdb::plugins::replication_webrtc::query_fetch_handler::{
    run_query_fetch, QueryFetchRegistry,
};
use rxdb::plugins::replication_webrtc::webrtc_types::{
    PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler, WebRTCMessage, WebRTCWireFrame,
};
use rxdb::replication_protocol::default_conflict_handler::DefaultConflictHandler;
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use rxdb::rx_schema::create_rx_schema;
use rxdb::rxjs_compat::{RxStream, RxSubject};
use rxdb::storage::sqlite::{create_storage_instance, get_rx_storage_sqlite, RxStorageSqliteSettings};
use rxdb::types::{
    BulkWriteRow, HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema,
    RxStorageInstance, RxStorageInstanceCreationParams,
};

struct TestHash;
impl HashFunction for TestHash {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { format!("hash:{input}") })
    }
}

#[derive(Clone, Default, Debug)]
struct DaemonPeer(String);
impl PartialEq for DaemonPeer { fn eq(&self, o: &Self) -> bool { self.0 == o.0 } }
impl Eq for DaemonPeer {}
impl std::hash::Hash for DaemonPeer {
    fn hash<H: std::hash::Hasher>(&self, s: &mut H) { self.0.hash(s) }
}

/// Connection handler whose `send` immediately writes the frame to a shared
/// stdout sink as `{"kind":"wire", peerIdentity, frame}` newline-delimited JSON.
struct StdoutHandler {
    out: Arc<Mutex<Box<dyn Write + Send>>>,
    sent_bytes: Arc<AtomicUsize>,
}

#[async_trait]
impl WebRTCConnectionHandler for StdoutHandler {
    type Peer = DaemonPeer;
    fn connect_stream(&self) -> RxStream<Self::Peer> { RxSubject::<Self::Peer>::new().subscribe() }
    fn disconnect_stream(&self) -> RxStream<Self::Peer> { RxSubject::<Self::Peer>::new().subscribe() }
    fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>> { RxSubject::<PeerWithMessage<Self::Peer>>::new().subscribe() }
    fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>> { RxSubject::<PeerWithResponse<Self::Peer>>::new().subscribe() }
    fn error_stream(&self) -> RxStream<rxdb::rx_error::RxError> { RxSubject::<rxdb::rx_error::RxError>::new().subscribe() }
    async fn send(&self, peer: &Self::Peer, frame: WebRTCWireFrame) -> Result<(), rxdb::rx_error::RxError> {
        let envelope = json!({
            "kind": "wire",
            "peerIdentity": peer.0,
            "frame": frame,
        });
        let line = serde_json::to_string(&envelope).unwrap_or_default();
        self.sent_bytes.fetch_add(line.len(), Ordering::SeqCst);
        let mut out = self.out.lock();
        let _ = writeln!(out, "{}", line);
        let _ = out.flush();
        Ok(())
    }
    async fn close(&self) -> Result<(), rxdb::rx_error::RxError> { Ok(()) }
    fn peer_identity(&self, peer: &Self::Peer) -> String { peer.0.clone() }
}

fn doc_schema() -> RxJsonSchema {
    let mut p = HashMap::new();
    p.insert("id".into(), JsonSchema { schema_type: Some("string".into()), max_length: Some(100), ..Default::default() });
    p.insert("payload".into(), JsonSchema { schema_type: Some("string".into()), ..Default::default() });
    p.insert("_deleted".into(), JsonSchema { schema_type: Some("boolean".into()), ..Default::default() });
    let mut meta = HashMap::new();
    meta.insert("lwt".into(), JsonSchema { schema_type: Some("number".into()), ..Default::default() });
    p.insert("_meta".into(), JsonSchema { schema_type: Some("object".into()), properties: meta, ..Default::default() });
    p.insert("_rev".into(), JsonSchema { schema_type: Some("string".into()), ..Default::default() });
    p.insert("_attachments".into(), JsonSchema { schema_type: Some("object".into()), additional_properties: Some(true), ..Default::default() });
    RxJsonSchema {
        version: 0, primary_key: PrimaryKey::Simple("id".into()),
        schema_type: "object".into(), properties: p,
        required: vec!["id".into()], indexes: vec![],
        encrypted: vec![], internal_indexes: vec![],
        key_compression: false, attachments: None,
        additional_properties: true, extra: HashMap::new(),
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let stdout = std::io::stdout();
    let out: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(stdout)));
    let sent_bytes = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(StdoutHandler { out: Arc::clone(&out), sent_bytes: Arc::clone(&sent_bytes) });

    // Storage + collection
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
        database_path: dir.path().join("daemon.sqlite3"),
    });
    let rx_schema = Arc::new(create_rx_schema(doc_schema(), Arc::new(TestHash), false).unwrap());
    let storage_instance: Arc<dyn RxStorageInstance> = create_storage_instance(
        &storage,
        RxStorageInstanceCreationParams {
            database_instance_token: "daemon".into(),
            database_name: "daemon".into(),
            collection_name: "demo".into(),
            schema: rx_schema.json_schema.clone(),
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        },
    )
    .await
    .expect("create");
    let database = RxDatabase::new("daemon", "tok", "stoken", false, Arc::new(TestHash), storage);
    let collection = RxCollection::new_with_schema(
        "demo",
        database,
        storage_instance,
        Arc::new(DefaultConflictHandler),
        rx_schema,
    );
    let query_registry = Arc::new(QueryFetchRegistry::new(8));
    query_registry.register(Arc::clone(&collection));
    let file_registry = Arc::new(FileFetchRegistry::new(8));
    file_registry.register_source(
        "demo",
        Arc::new(|_c, file_id, _r| {
            // Synthetic file: 800 KB of "FILE:<id>:..." pattern. Range support
            // omitted for this test; daemon callers ask for whole file.
            let pattern = format!("FILE:{}:", file_id);
            let mut buf = Vec::with_capacity(800 * 1024);
            while buf.len() < 800 * 1024 {
                buf.extend_from_slice(pattern.as_bytes());
            }
            Ok(buf)
        }),
    );

    // Signal ready so the JS driver can start sending.
    writeln!(out.lock(), "{}", json!({"kind":"ready"}).to_string()).ok();
    out.lock().flush().ok();

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let parsed: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(err) => {
                let _ = writeln!(out.lock(), "{}", json!({"kind":"error","message":format!("bad json: {err}")}));
                continue;
            }
        };
        match parsed.get("kind").and_then(Value::as_str) {
            Some("shutdown") => {
                let _ = writeln!(out.lock(), "{}", json!({"kind":"bye"}));
                break;
            }
            Some("seed") => {
                let docs = parsed.get("docs").and_then(Value::as_array).cloned().unwrap_or_default();
                let rows: Vec<BulkWriteRow> = docs.into_iter().map(|d| BulkWriteRow { previous: None, document: d }).collect();
                if !rows.is_empty() {
                    let _ = collection.storage_instance.bulk_write(rows, "seed").await;
                }
                let _ = writeln!(out.lock(), "{}", json!({"kind":"seeded"}));
            }
            Some("request") => {
                let peer_identity = parsed.get("peerIdentity").and_then(Value::as_str).unwrap_or("p1").to_string();
                let message: WebRTCMessage = serde_json::from_value(
                    parsed.get("message").cloned().unwrap_or(Value::Null),
                )
                .unwrap_or(WebRTCMessage { id: String::new(), method: String::new(), params: vec![] });

                let peer = DaemonPeer(peer_identity.clone());
                match message.method.as_str() {
                    "rxdb.query.fetch" => {
                        let r = Arc::clone(&query_registry);
                        let h = Arc::clone(&handler);
                        // Await inline so the next stdin line is read only
                        // after the chunk stream has been fully emitted.
                        // For E2E correctness this matters: the JS driver
                        // expects "request, then complete-chunk" before
                        // sending the next request.
                        let _ = run_query_fetch(r, h, peer, peer_identity, message).await;
                    }
                    "rxdb.query.cancel" => {
                        if let Some(rid) = message
                            .params
                            .first()
                            .and_then(|v| v.get("requestId").and_then(Value::as_str))
                        {
                            query_registry.cancel(rid);
                        }
                    }
                    "rxdb.file.fetch" => {
                        let r = Arc::clone(&file_registry);
                        let h = Arc::clone(&handler);
                        let _ = run_file_fetch(r, h, peer, peer_identity, message).await;
                    }
                    "rxdb.file.cancel" => {
                        if let Some(rid) = message
                            .params
                            .first()
                            .and_then(|v| v.get("requestId").and_then(Value::as_str))
                        {
                            file_registry.cancel(rid);
                        }
                    }
                    other => {
                        let _ = writeln!(out.lock(), "{}", json!({"kind":"error","message":format!("unknown method: {other}")}));
                    }
                }
            }
            _ => {}
        }
    }
}
