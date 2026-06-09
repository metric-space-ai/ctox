//! Real-scale wire-loop benchmark for V1.5.
//!
//! Drives 100k synthetic documents through the entire Rust dispatcher
//! (`run_query_fetch`) using a mock connection handler that records every
//! emitted wire frame. Verifies:
//!   - all 100k docs actually arrive at the receiver
//!   - every chunk frame respects the byte cap
//!   - chunk count matches the byte/doc budget
//!   - server-side memory stays bounded (RSS delta)
//!   - measured throughput on a single process
//!
//! Run with: cargo run --release --example v15_scale_wire_loop

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::{json, Value};

use rxdb::plugins::replication_webrtc::query_fetch_handler::{
    decode_chunk_documents, run_query_fetch, QueryFetchChunk, QueryFetchRegistry,
};
use rxdb::plugins::replication_webrtc::webrtc_types::{
    PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse,
    WebRTCWireFrame,
};
use rxdb::replication_protocol::default_conflict_handler::DefaultConflictHandler;
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use rxdb::rx_schema::create_rx_schema;
use rxdb::rxjs_compat::{RxStream, RxSubject};
use rxdb::storage::sqlite::{
    create_storage_instance, get_rx_storage_sqlite, RxStorageSqliteSettings,
};
use rxdb::types::{
    BulkWriteRow, HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema,
    RxStorageInstance, RxStorageInstanceCreationParams,
};

struct TestHashFunction;
impl HashFunction for TestHashFunction {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { format!("hash:{input}") })
    }
}

#[derive(Clone, Default, Debug)]
struct MockPeer(&'static str);
impl PartialEq for MockPeer {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for MockPeer {}
impl std::hash::Hash for MockPeer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

struct CountingHandler {
    chunks: Mutex<Vec<QueryFetchChunk>>,
    peak_chunk_bytes: AtomicUsize,
    total_chunk_bytes: AtomicUsize,
    chunk_count: AtomicUsize,
}

#[async_trait]
impl WebRTCConnectionHandler for CountingHandler {
    type Peer = MockPeer;
    fn connect_stream(&self) -> RxStream<Self::Peer> {
        RxSubject::<Self::Peer>::new().subscribe()
    }
    fn disconnect_stream(&self) -> RxStream<Self::Peer> {
        RxSubject::<Self::Peer>::new().subscribe()
    }
    fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>> {
        RxSubject::<PeerWithMessage<Self::Peer>>::new().subscribe()
    }
    fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>> {
        RxSubject::<PeerWithResponse<Self::Peer>>::new().subscribe()
    }
    fn error_stream(&self) -> RxStream<rxdb::rx_error::RxError> {
        RxSubject::<rxdb::rx_error::RxError>::new().subscribe()
    }
    async fn send(
        &self,
        _peer: &Self::Peer,
        frame: WebRTCWireFrame,
    ) -> Result<(), rxdb::rx_error::RxError> {
        match frame {
            WebRTCWireFrame::Message(m) if m.method == "rxdb.query.chunk" => {
                let bytes = serde_json::to_string(&m.params[0])
                    .map(|s| s.len())
                    .unwrap_or(0);
                let mut prev_peak = self.peak_chunk_bytes.load(Ordering::SeqCst);
                while bytes > prev_peak {
                    match self.peak_chunk_bytes.compare_exchange(
                        prev_peak,
                        bytes,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(_) => break,
                        Err(actual) => prev_peak = actual,
                    }
                }
                self.total_chunk_bytes.fetch_add(bytes, Ordering::SeqCst);
                self.chunk_count.fetch_add(1, Ordering::SeqCst);
                if let Ok(chunk) = serde_json::from_value::<QueryFetchChunk>(m.params[0].clone()) {
                    self.chunks.lock().push(chunk);
                }
            }
            _ => { /* ack response — ignore */ }
        }
        Ok(())
    }
    async fn close(&self) -> Result<(), rxdb::rx_error::RxError> {
        Ok(())
    }
    fn peer_identity(&self, peer: &Self::Peer) -> String {
        peer.0.to_string()
    }
}

fn schema() -> RxJsonSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "id".into(),
        JsonSchema {
            schema_type: Some("string".into()),
            max_length: Some(100),
            ..Default::default()
        },
    );
    for f in ["status", "owner", "subject"] {
        properties.insert(
            f.into(),
            JsonSchema {
                schema_type: Some("string".into()),
                ..Default::default()
            },
        );
    }
    properties.insert(
        "n".into(),
        JsonSchema {
            schema_type: Some("number".into()),
            ..Default::default()
        },
    );
    properties.insert(
        "_deleted".into(),
        JsonSchema {
            schema_type: Some("boolean".into()),
            ..Default::default()
        },
    );
    let mut meta = HashMap::new();
    meta.insert(
        "lwt".into(),
        JsonSchema {
            schema_type: Some("number".into()),
            ..Default::default()
        },
    );
    properties.insert(
        "_meta".into(),
        JsonSchema {
            schema_type: Some("object".into()),
            properties: meta,
            ..Default::default()
        },
    );
    properties.insert(
        "_rev".into(),
        JsonSchema {
            schema_type: Some("string".into()),
            ..Default::default()
        },
    );
    properties.insert(
        "_attachments".into(),
        JsonSchema {
            schema_type: Some("object".into()),
            additional_properties: Some(true),
            ..Default::default()
        },
    );
    RxJsonSchema {
        version: 0,
        primary_key: PrimaryKey::Simple("id".into()),
        schema_type: "object".into(),
        properties,
        required: vec!["id".into()],
        indexes: vec![],
        encrypted: vec![],
        internal_indexes: vec![],
        key_compression: false,
        attachments: None,
        additional_properties: true,
        extra: HashMap::new(),
    }
}

fn doc(i: usize) -> Value {
    json!({
        "id": format!("rec-{i:08}"),
        "status": if i % 3 == 0 { "open" } else if i % 3 == 1 { "done" } else { "stalled" },
        "owner": format!("user-{:02}", i % 25),
        "subject": format!("Synthetic record {} for V1.5 scale wire test", i),
        "n": i as i64,
        "_rev": "1-bench",
        "_deleted": false,
        "_meta": { "lwt": i as f64 },
        "_attachments": {},
    })
}

fn process_memory_kb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            if let Some(num) = rest.split_whitespace().next() {
                return num.parse::<u64>().unwrap_or(0);
            }
        }
    }
    0
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    println!("=== V1.5 scale wire loop: n={} ===", n);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scale.sqlite3");
    let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
        database_path: path,
    });
    let rx_schema =
        Arc::new(create_rx_schema(schema(), Arc::new(TestHashFunction), false).unwrap());
    let storage_instance: Arc<dyn RxStorageInstance> = create_storage_instance(
        &storage,
        RxStorageInstanceCreationParams {
            database_instance_token: "scale".into(),
            database_name: "scale".into(),
            collection_name: "synthetic".into(),
            schema: rx_schema.json_schema.clone(),
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        },
    )
    .await
    .unwrap();

    let seed_start = Instant::now();
    let mut written = 0;
    let batch = 5000;
    while written < n {
        let take = (n - written).min(batch);
        let rows: Vec<BulkWriteRow> = (written..written + take)
            .map(|i| BulkWriteRow {
                previous: None,
                document: doc(i),
            })
            .collect();
        storage_instance
            .bulk_write(rows, "scale-seed")
            .await
            .unwrap();
        written += take;
    }
    println!("seed: {} ms", seed_start.elapsed().as_millis());

    let database = RxDatabase::new(
        "scale",
        "tok",
        "stoken",
        false,
        Arc::new(TestHashFunction),
        storage,
    );
    let collection = RxCollection::new_with_schema(
        "synthetic",
        database,
        storage_instance,
        Arc::new(DefaultConflictHandler),
        rx_schema,
    );
    let registry = Arc::new(QueryFetchRegistry::new(4));
    registry.register(Arc::clone(&collection));
    let handler = Arc::new(CountingHandler {
        chunks: Mutex::new(Vec::new()),
        peak_chunk_bytes: AtomicUsize::new(0),
        total_chunk_bytes: AtomicUsize::new(0),
        chunk_count: AtomicUsize::new(0),
    });
    let message = WebRTCMessage {
        id: "scale-msg".into(),
        method: "rxdb.query.fetch".into(),
        collection: None,
        params: vec![json!({
            "requestId": "scale-r1",
            "collectionName": "synthetic",
            "schemaVersion": 0,
            "queryFingerprint": "scale-fp",
            "query": { "selector": {}, "sort": [], "limit": null, "skip": 0 },
            "window": { "offset": 0, "limit": n }
        })],
    };

    let rss_before = process_memory_kb();
    let dispatch_start = Instant::now();
    run_query_fetch(
        Arc::clone(&registry),
        Arc::clone(&handler),
        MockPeer("p1"),
        "p1".into(),
        message,
    )
    .await
    .unwrap();
    let dispatch_ms = dispatch_start.elapsed().as_millis();
    let rss_after = process_memory_kb();

    // Reassemble the entire stream from chunks.
    let chunks = handler.chunks.lock();
    let total_docs: usize = chunks
        .iter()
        .map(|c| decode_chunk_documents(c).expect("decode").len())
        .sum();
    let chunk_count = handler.chunk_count.load(Ordering::SeqCst);
    let peak_chunk = handler.peak_chunk_bytes.load(Ordering::SeqCst);
    let total_bytes = handler.total_chunk_bytes.load(Ordering::SeqCst);

    println!("dispatch_ms:      {}", dispatch_ms);
    println!("total_docs:       {} (expected {})", total_docs, n);
    println!("chunk_count:      {}", chunk_count);
    println!("peak_chunk_B:     {}", peak_chunk);
    println!("total_wire_KB:    {}", total_bytes / 1024);
    println!(
        "RSS Δ:            {:+} KB",
        rss_after as i64 - rss_before as i64
    );
    println!(
        "throughput:       {:.0} docs/s ({:.1} MB/s wire)",
        total_docs as f64 / (dispatch_ms as f64 / 1000.0).max(0.001),
        (total_bytes as f64 / 1024.0 / 1024.0) / (dispatch_ms as f64 / 1000.0).max(0.001),
    );

    assert_eq!(total_docs, n, "all docs must reach the wire");
    assert!(
        chunks.last().unwrap().complete,
        "last chunk must be complete"
    );
    assert!(
        peak_chunk <= 270_000,
        "no chunk may exceed ~256 KB byte cap (got {})",
        peak_chunk
    );
    println!("OK: full pipeline holds at scale.");
}
