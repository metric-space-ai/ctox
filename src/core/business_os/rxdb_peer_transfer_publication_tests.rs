//! Service/dispatcher regression harness. The transport deliberately makes its
//! first physical poll Pending at zero bytes. Native authority is real SQLite
//! and encrypted secrets; only the byte sink and connection lifecycle are fake.
use super::{rxdb_peer, rxdb_peer_transfer_publication, store};
use crate::native_data_device::{NativeDeviceKeyScope, NativeDeviceProofKey};
use async_trait::async_trait;
use futures_util::task::AtomicWaker;
use rxdb::{
    plugins::{
        replication_webrtc::{
            index_mod::replicate_web_rtc_multi_with_validators,
            webrtc_types::{PeerWithMessage, PeerWithResponse, WebRTCMessage, WebRTCResponse},
            RxWebRTCReplicationPool, WebRTCConnectionHandler, WebRTCPublicationGuard,
            WebRTCWireFrame,
        },
        storage_memory::get_rx_storage_memory,
    },
    rx_database::RxDatabase,
    rx_error::{new_rx_error, RxError, RxResult},
    rxjs_compat::{RxStream, RxSubject},
    types::{HashFunction, HashOutput},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::Poll,
    time::Duration,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Peer(&'static str, u64);

enum Event {
    Prepared(Value),
    Completed(bool),
    PlainTransferResponse,
}

struct Transport {
    connect: RxSubject<Peer>,
    disconnect: RxSubject<Peer>,
    message: RxSubject<PeerWithMessage<Peer>>,
    response: RxSubject<PeerWithResponse<Peer>>,
    error: RxSubject<RxError>,
    current: Mutex<Peer>,
    capabilities: Mutex<HashMap<Peer, String>>,
    protocol: UnboundedSender<WebRTCResponse>,
    events: UnboundedSender<Event>,
    released: AtomicBool,
    waker: AtomicWaker,
    bytes: AtomicUsize,
    polls: AtomicUsize,
    fenced_stores: Vec<PathBuf>,
}

#[async_trait]
impl WebRTCConnectionHandler for Transport {
    type Peer = Peer;
    fn connect_stream(&self) -> RxStream<Peer> {
        self.connect.subscribe()
    }
    fn disconnect_stream(&self) -> RxStream<Peer> {
        self.disconnect.subscribe()
    }
    fn message_stream(&self) -> RxStream<PeerWithMessage<Peer>> {
        self.message.subscribe()
    }
    fn response_stream(&self) -> RxStream<PeerWithResponse<Peer>> {
        self.response.subscribe()
    }
    fn error_stream(&self) -> RxStream<RxError> {
        self.error.subscribe()
    }
    fn peer_identity(&self, peer: &Peer) -> String {
        peer.0.into()
    }
    fn connection_identity(&self, peer: &Peer) -> String {
        format!("{}@{}", peer.0, peer.1)
    }
    fn is_peer_current(&self, peer: &Peer) -> bool {
        *self.current.lock().unwrap() == *peer
    }
    fn set_peer_capability_token(&self, peer: &Peer, token: String) {
        self.capabilities
            .lock()
            .unwrap()
            .insert(peer.clone(), token);
    }
    fn peer_capability_token(&self, peer: &Peer) -> Option<String> {
        self.capabilities.lock().unwrap().get(peer).cloned()
    }
    fn document_fields_for_peer(&self, _: &Peer, _: &str) -> Option<Vec<String>> {
        // This auxiliary-only fixture exposes no replicated document fields.
        Some(Vec::new())
    }
    async fn send(&self, _: &Peer, frame: WebRTCWireFrame) -> RxResult<()> {
        if let WebRTCWireFrame::Response(response) = frame {
            if response.id == "transfer-reply" {
                let _ = self.events.send(Event::PlainTransferResponse);
            } else {
                let _ = self.protocol.send(response);
            }
        }
        Ok(())
    }
    async fn send_guarded(
        &self,
        peer: &Peer,
        frame: WebRTCWireFrame,
        publication: Arc<dyn WebRTCPublicationGuard>,
    ) -> RxResult<()> {
        let WebRTCWireFrame::Response(response) = &frame else {
            panic!("only a service response enters the transfer queue");
        };
        assert_eq!(response.id, "transfer-reply");
        assert!(response.error.is_none());
        let encoded = serde_json::to_vec(&frame).unwrap();
        let mut prepared = false;
        let result = std::future::poll_fn(|cx| {
            self.waker.register(cx.waker());
            let mut polled = false;
            let mut outcome = Poll::Pending;
            let checked = publication.with_current(&mut || {
                assert!(!polled, "one physical poll per native fence");
                polled = true;
                // Same lock that replacement takes: the sink cannot switch
                // generations between its final check and byte emission.
                let current = self.current.lock().unwrap();
                if *current != *peer {
                    return Err(new_rx_error("fixture_connection_retired", None));
                }
                for path in &self.fenced_stores {
                    let writer = rusqlite::Connection::open_with_flags(
                        path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                    ).unwrap();
                    writer.busy_timeout(Duration::ZERO).unwrap();
                    let error = writer.execute_batch("BEGIN IMMEDIATE")
                        .expect_err("native policy/projection fence escaped the physical poll");
                    assert!(matches!(error, rusqlite::Error::SqliteFailure(code, _) if matches!(
                        code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                    )));
                }
                self.polls.fetch_add(1, Ordering::SeqCst);
                if self.released.load(Ordering::SeqCst) {
                    self.bytes.fetch_add(encoded.len(), Ordering::SeqCst);
                    outcome = Poll::Ready(Ok(()));
                }
                Ok(())
            });
            if let Err(error) = checked { return Poll::Ready(Err(error)); }
            assert!(polled, "guard must enter the physical callback");
            if !prepared {
                prepared = true;
                assert_eq!(self.bytes.load(Ordering::SeqCst), 0);
                let _ = self.events.send(Event::Prepared(response.result.clone()));
            }
            outcome
        }).await;
        let _ = self.events.send(Event::Completed(result.is_ok()));
        result
    }
    async fn close(&self) -> RxResult<()> {
        Ok(())
    }
}

struct TestHash;
impl HashFunction for TestHash {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::default_hash_sha256(&input) })
    }
}

pub(super) struct PendingReply {
    database: Arc<RxDatabase>,
    pool: Arc<RxWebRTCReplicationPool<Transport>>,
    transport: Arc<Transport>,
    events: UnboundedReceiver<Event>,
    pub result: Value,
}

impl PendingReply {
    pub async fn start(
        root: &Path,
        token: &str,
        key_target: &str,
        method: &str,
        request: Value,
    ) -> Self {
        let identity = crate::sync_host::signing_identity(root)
            .unwrap()
            .public_identity();
        let instance = store::sync_connection_config(root).unwrap().instance_id;
        let key = NativeDeviceProofKey::load(
            root,
            &NativeDeviceKeyScope {
                target_id: key_target.into(),
                source_instance_id: instance,
                source_public_identity: identity,
                account_epoch: 1,
            },
        )
        .unwrap();
        let nonce = "n".repeat(43);
        let proof = key.sign_nonce(&nonce).unwrap();
        let protocol = json!({"peerSession":{
            "sessionId":"transfer-publication-fixture","capabilityToken":token,
            "deviceProof":{"version":"ctox-device-proof-v1","nonce":nonce,
                "publicJwk":key.public_jwk(),"signature":proof.signature}
        }});
        let peer = Peer("same-signaling-id", 1);
        let (protocol_tx, mut protocol_rx) = unbounded_channel();
        let (events_tx, events) = unbounded_channel();
        let transport = Arc::new(Transport {
            connect: RxSubject::new(),
            disconnect: RxSubject::new(),
            message: RxSubject::new(),
            response: RxSubject::new(),
            error: RxSubject::new(),
            current: Mutex::new(peer.clone()),
            capabilities: Mutex::new(HashMap::new()),
            protocol: protocol_tx,
            events: events_tx,
            released: AtomicBool::new(false),
            waker: AtomicWaker::new(),
            bytes: AtomicUsize::new(0),
            polls: AtomicUsize::new(0),
            fenced_stores: if method == crate::transfers_grant::TRANSFER_GRANT_METHOD {
                vec![
                    store::business_os_store_path(root),
                    store::rxdb_store_path(root),
                ]
            } else {
                vec![store::business_os_store_path(root)]
            },
        });
        let database = RxDatabase::new(
            "transfer-publication",
            "database-token",
            "storage-token",
            false,
            Arc::new(TestHash),
            get_rx_storage_memory(()),
        );
        let validator_root = root.to_path_buf();
        let pool = replicate_web_rtc_multi_with_validators(
            database.clone(),
            vec![],
            transport.clone(),
            None,
            Some(Arc::new(move |protocol, _| {
                // A fixed challenge isolates queue publication from nonce
                // negotiation, while exercising the real P256 device verifier.
                rxdb_peer::validate_device_bound_peer_session(
                    &validator_root,
                    protocol,
                    Some(&nonce),
                )
            })),
            None,
            None,
        )
        .await
        .unwrap();
        rxdb_peer_transfer_publication::register(&pool, root).unwrap();
        transport.message.next(PeerWithMessage {
            peer: peer.clone(),
            message: WebRTCMessage {
                id: "authenticated-protocol".into(),
                method: "ctoxProtocol".into(),
                params: vec![protocol],
                collection: None,
            },
        });
        let accepted = tokio::time::timeout(Duration::from_secs(5), protocol_rx.recv())
            .await
            .expect("protocol response deadline")
            .unwrap();
        assert_eq!(accepted.id, "authenticated-protocol");
        assert!(
            accepted.error.is_none(),
            "real device handshake must succeed"
        );
        assert_eq!(
            transport.peer_capability_token(&peer).as_deref(),
            Some(token)
        );
        transport.message.next(PeerWithMessage {
            peer,
            message: WebRTCMessage {
                id: "transfer-reply".into(),
                method: method.into(),
                params: vec![request],
                collection: None,
            },
        });
        let mut pending = Self {
            database,
            pool,
            transport,
            events,
            result: Value::Null,
        };
        pending.result = match pending.next().await {
            Event::Prepared(value) => value,
            Event::Completed(_) => panic!("response denied before the initial Pending poll"),
            Event::PlainTransferResponse => panic!("service bypassed guarded transport"),
        };
        assert_eq!(pending.transport.bytes.load(Ordering::SeqCst), 0);
        assert_eq!(pending.transport.polls.load(Ordering::SeqCst), 1);
        pending
    }

    async fn next(&mut self) -> Event {
        tokio::time::timeout(Duration::from_secs(5), self.events.recv())
            .await
            .expect("transfer queue deadline")
            .expect("queue event")
    }

    pub fn replace_connection(&self) {
        let old = self.transport.current.lock().unwrap().clone();
        let replacement = Peer(old.0, old.1 + 1);
        assert_eq!(
            self.transport.peer_identity(&old),
            self.transport.peer_identity(&replacement)
        );
        let token = self.transport.peer_capability_token(&old).unwrap();
        self.transport
            .set_peer_capability_token(&replacement, token);
        *self.transport.current.lock().unwrap() = replacement;
    }

    pub async fn finish(mut self, allowed: bool) {
        self.transport.released.store(true, Ordering::SeqCst);
        self.transport.waker.wake();
        let delivered = match self.next().await {
            Event::Completed(delivered) => delivered,
            Event::Prepared(_) => panic!("queued reply prepared twice"),
            Event::PlainTransferResponse => panic!("denied guard fell back to plain send"),
        };
        let bytes = self.transport.bytes.load(Ordering::SeqCst);
        let polls = self.transport.polls.load(Ordering::SeqCst);
        self.pool.cancel().await;
        self.database.close().await.unwrap();
        assert_eq!(delivered, allowed);
        if allowed {
            assert!(bytes > 0);
            assert_eq!(polls, 2);
        } else {
            assert_eq!(bytes, 0, "revoked prepared success must emit zero bytes");
            assert_eq!(
                polls, 1,
                "revocation must stop before the next physical poll"
            );
        }
    }
}
