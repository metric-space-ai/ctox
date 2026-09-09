#![cfg(feature = "webrtc")]
use ctox_sync::native::{NativeAdmission, NativeSyncOptions, NativeSyncSession};
use futures::{SinkExt, StreamExt};
use rxdb::{
    plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation,
    rx_database::{create_rx_database, RxCollectionCreator, RxDatabase, RxDatabaseCreator},
    storage::sqlite::{index_mod::get_rx_storage_sqlite, types::RxStorageSqliteSettings},
    types::{HashFunction, HashOutput},
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct Hash;
impl HashFunction for Hash {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
    }
}

async fn options(url: String) -> (tempfile::TempDir, Arc<RxDatabase>, NativeSyncOptions) {
    options_with_records(url, true).await
}

async fn options_with_records(
    url: String,
    with_records: bool,
) -> (tempfile::TempDir, Arc<RxDatabase>, NativeSyncOptions) {
    let directory = tempfile::tempdir().unwrap();
    let database = create_rx_database(RxDatabaseCreator {
        name: format!(
            "native-{}",
            directory.path().file_name().unwrap().to_string_lossy()
        ),
        storage: get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: directory.path().join("records.sqlite"),
        }),
        multi_instance: false,
        password: None,
        hash_function: Arc::new(Hash),
        options: HashMap::new(),
        ignore_duplicate: false,
        close_duplicates: false,
        event_reduce: false,
        allow_slow_count: false,
    })
    .await
    .unwrap();
    let collections = if with_records {
        database
            .add_collections(HashMap::from([(
                "records".into(),
                RxCollectionCreator {
                    schema: serde_json::from_value(json!({
                        "version":0,"primaryKey":"id","type":"object",
                        "properties":{"id":{"type":"string","maxLength":64}},"required":["id"]
                    }))
                    .unwrap(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap()
    } else {
        HashMap::new()
    };
    let options = NativeSyncOptions {
        local_session_provider: None,
        peer_role: ctox_sync::native::NativePeerRole::CtoxInstance,
        database: Arc::clone(&database),
        collections: collections.into_values().collect(),
        signaling_urls: Arc::new(move || vec![url.clone()]),
        room: "native-lifecycle".into(),
        peer_session_id: "native-session".into(),
        ice_servers: Vec::new(),
        admission: NativeAdmission {
            peer: Arc::new(|_| false),
            session: Arc::new(|_, _| WebRTCPeerSessionValidation::Reject),
            collection_read: Some(Arc::new(|_, _| false)),
            collection_write: Some(Arc::new(|_, _| false)),
            document_read: None,
            document_write: None,
            eager_pull: None,
            live_change: None,
        },
        bringup_timeout: Duration::from_secs(5),
    };
    (directory, database, options)
}

#[derive(Clone, Copy)]
enum Admission {
    Accept,
    AcceptWithImmediateSignal,
    Reject,
    Pending,
}
struct Signal {
    url: String,
    joined: oneshot::Receiver<()>,
    closed: oneshot::Receiver<()>,
    task: JoinHandle<()>,
}
impl Drop for Signal {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Signal {
    async fn start(admission: Admission) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (joined_tx, joined) = oneshot::channel();
        let (closed_tx, closed) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            socket
                .send(Message::text(
                    json!({"type":"init","yourPeerId":"native000001"}).to_string(),
                ))
                .await
                .unwrap();
            let mut joined_tx = Some(joined_tx);
            while let Some(message) = socket.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        let value: Value = serde_json::from_str(&text).unwrap();
                        if value["type"] == "join" {
                            assert_eq!(value["room"], "native-lifecycle");
                            joined_tx.take().unwrap().send(()).unwrap();
                            let response = match admission {
                                Admission::Accept | Admission::AcceptWithImmediateSignal => {
                                    Some(json!({"type":"joined","otherPeerIds":[],"peers":[]}))
                                }
                                Admission::Reject => Some(
                                    json!({"type":"ctoxError","scope":"join","code":"peer_revoked","reason":"fixture rejection"}),
                                ),
                                Admission::Pending => None,
                            };
                            if let Some(response) = response {
                                socket
                                    .send(Message::text(response.to_string()))
                                    .await
                                    .unwrap();
                                if matches!(admission, Admission::AcceptWithImmediateSignal) {
                                    // A malformed SDP produces a deterministic receiver receipt;
                                    // losing this first signal used to leave no observable event.
                                    socket
                                        .send(Message::text(
                                            json!({
                                                "type":"signal", "room":"native-lifecycle", "senderPeerId":"native000002",
                                                "receiverPeerId":"native000001",
                                                "data":{"type":"offer", "sdp":17}
                                            })
                                            .to_string(),
                                        ))
                                        .await
                                        .unwrap();
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            let _ = closed_tx.send(());
        });
        Self {
            url,
            joined,
            closed,
            task,
        }
    }
    async fn assert_closed(&mut self) {
        tokio::time::timeout(Duration::from_secs(3), &mut self.closed)
            .await
            .expect("native lifecycle leaked its signaling connection")
            .unwrap();
    }
}

#[tokio::test]
async fn prepared_native_handler_receives_the_first_signal_at_room_admission() {
    use rxdb::plugins::replication_webrtc::{
        replicate_web_rtc_multi_with_validators, RTCIceServer, SignalingClient,
        WebRTCConnectionHandler, WebRTCRsConfig, WebRTCRsConnectionHandler,
    };
    let mut signal = Signal::start(Admission::AcceptWithImmediateSignal).await;
    let (_directory, database, options) = options(signal.url.clone()).await;
    let url = signal.url.clone();
    let signaling = SignalingClient::connect_with_url_list_provider(move || vec![url.clone()])
        .await
        .unwrap();
    let mut config = WebRTCRsConfig::new(signaling.clone(), options.room.clone());
    config.ice_servers = vec![RTCIceServer::default()];
    let handler = WebRTCRsConnectionHandler::prepare_with_signaling(config)
        .await
        .unwrap();
    assert!(!signaling.join_accepted());
    assert!(matches!(
        signal.joined.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    let mut errors = handler.error_stream();
    let pool = replicate_web_rtc_multi_with_validators(
        options.database,
        options.collections,
        handler,
        Some(Arc::new(move |connection| {
            (options.admission.peer)(&connection.peer_id().to_owned())
        })),
        Some(options.admission.session),
        Some(options.room.clone()),
        Some(Arc::from(options.peer_session_id)),
    )
    .await
    .unwrap();
    signaling.join(options.room).await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), errors.next())
        .await
        .expect("first signaling frame was lost")
        .expect("handler error stream closed");
    assert!(
        error.to_string().contains("decode SDP signal failed"),
        "{error}"
    );
    pool.cancel().await;
    signal.assert_closed().await;
    database.close().await.unwrap();
}

#[tokio::test]
async fn rejected_room_closes_transport_before_returning_error() {
    let mut signal = Signal::start(Admission::Reject).await;
    let (_directory, database, options) = options(signal.url.clone()).await;
    let error = NativeSyncSession::start(options)
        .await
        .err()
        .expect("room was rejected");
    assert!(error.to_string().contains("bring-up failed"), "{error}");
    signal.assert_closed().await;
    database.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_bringup_closes_the_already_started_signaling_supervisor() {
    let mut signal = Signal::start(Admission::Pending).await;
    let (_directory, database, options) = options(signal.url.clone()).await;
    let start = tokio::spawn(NativeSyncSession::start(options));
    tokio::time::timeout(Duration::from_secs(3), &mut signal.joined)
        .await
        .unwrap()
        .unwrap();
    start.abort();
    assert!(start
        .await
        .err()
        .expect("start task was aborted")
        .is_cancelled());
    signal.assert_closed().await;
    database.close().await.unwrap();
}

#[tokio::test]
async fn deadline_closes_unaccepted_room() {
    let mut signal = Signal::start(Admission::Pending).await;
    let (_directory, database, mut options) = options(signal.url.clone()).await;
    options.bringup_timeout = Duration::from_millis(250);
    let error = NativeSyncSession::start(options)
        .await
        .err()
        .expect("join must time out");
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    signal.assert_closed().await;
    database.close().await.unwrap();
}

#[tokio::test]
async fn session_shutdown_closes_pool_but_preserves_host_database() {
    let mut signal = Signal::start(Admission::Accept).await;
    let (_directory, database, options) = options(signal.url.clone()).await;
    let session = NativeSyncSession::start(options).await.unwrap();
    let pool = session.pool().clone();
    let collection = pool
        .collections()
        .into_iter()
        .find(|collection| collection.name == "records")
        .unwrap();
    session.shutdown().await;
    signal.assert_closed().await;
    assert!(pool.canceled.load(std::sync::atomic::Ordering::SeqCst));
    // The transport does not own or close host persistence.
    collection
        .insert(json!({"id":"written-after-transport-stop"}))
        .await
        .unwrap();
    database.close().await.unwrap();
}

#[tokio::test]
async fn control_only_session_preserves_host_database_without_creating_collections() {
    let mut signal = Signal::start(Admission::Accept).await;
    let (_directory, database, options) = options_with_records(signal.url.clone(), false).await;
    let session = NativeSyncSession::start(options).await.unwrap();
    let pool = session.pool().clone();
    assert!(pool.collections().is_empty());
    assert!(database.collections.lock().is_empty());
    session.shutdown().await;
    signal.assert_closed().await;
    assert!(pool.canceled.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!database.closed());
    database.close().await.unwrap();
}

#[tokio::test]
async fn foreign_database_is_rejected_before_signaling() {
    let (_first_root, first_db, mut first) = options("unused".into()).await;
    let (_other_root, other_db, _) = options_with_records("unused".into(), false).await;
    first.database = other_db.clone();
    first.signaling_urls = Arc::new(|| panic!("invalid collection owner reached signaling"));
    let error = NativeSyncSession::start(first)
        .await
        .err()
        .expect("foreign collection rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    first_db.close().await.unwrap();
    other_db.close().await.unwrap();
}

#[tokio::test]
async fn incompatible_execution_group_closes_session_without_creating_authority_store() {
    use ctox_sync::{
        authority::{auth::SigningIdentity, Peer},
        native_execution::ExecutionGroupOptions,
    };
    use std::collections::BTreeMap;
    let mut signal = Signal::start(Admission::Accept).await;
    let (directory, database, options) = options(signal.url.clone()).await;
    let mut session = NativeSyncSession::start(options).await.unwrap();
    let keys: BTreeMap<_, _> = (1..=3)
        .map(|id| {
            (
                id,
                Arc::new(
                    SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap())
                        .unwrap(),
                ),
            )
        })
        .collect();
    let store_path = directory.path().join("authority.sqlite");
    let error = session
        .attach_execution(
            ExecutionGroupOptions {
                timing: Default::default(),
                node_id: 1,
                scope_id: "scope".into(),
                room: "another-room".into(),
                peers: keys
                    .iter()
                    .map(|(&id, key)| {
                        (
                            id,
                            Peer {
                                identity: key.public_identity(),
                                executor: id != 3,
                                data_replica: id != 3,
                            },
                        )
                    })
                    .collect(),
                routes: (1..=3).map(|id| (id, format!("native{id:06}"))).collect(),
                store_path: store_path.clone(),
                ipc_directory: directory.path().join("ipc"),
            },
            keys[&1].clone(),
        )
        .await
        .err()
        .expect("room mismatch must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(!store_path.exists());
    signal.assert_closed().await;
    assert!(session
        .pool()
        .canceled
        .load(std::sync::atomic::Ordering::SeqCst));
    database.close().await.unwrap();
}

#[tokio::test]
async fn dropping_running_session_closes_transport() {
    let mut signal = Signal::start(Admission::Accept).await;
    let (_directory, database, options) = options(signal.url.clone()).await;
    let session = NativeSyncSession::start(options).await.unwrap();
    drop(session);
    signal.assert_closed().await;
    database.close().await.unwrap();
}
