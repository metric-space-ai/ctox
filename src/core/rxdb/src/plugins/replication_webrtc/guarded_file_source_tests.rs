use super::super::file_fetch_handler::FileRange;
use super::super::webrtc_types::{PeerWithMessage, PeerWithResponse, WebRTCDocumentFilter};
use super::*;
use crate::rx_error::RxError;
use crate::rxjs_compat::{RxStream, RxSubject};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::atomic::AtomicUsize;

struct Source {
    lock: Mutex<()>,
    live: AtomicBool,
    delivered: AtomicBool,
    reads: AtomicUsize,
    bytes: Vec<u8>,
}
impl Source {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            lock: Mutex::new(()),
            live: AtomicBool::new(true),
            delivered: AtomicBool::new(false),
            reads: AtomicUsize::new(0),
            bytes: vec![7; 8 * 1024 + 1],
        })
    }
}
impl GuardedFileSource for Source {
    fn byte_len(&self, _: &str) -> RxResult<u64> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.bytes.len() as u64)
    }
    fn with_current_chunk(
        &self,
        _: &str,
        offset: u64,
        max: usize,
        terminal: bool,
        _capability_token: &str,
        send: &mut dyn FnMut(&Value, &[u8]) -> RxResult<()>,
    ) -> RxResult<()> {
        let _guard = self.lock.lock();
        if !self.live.load(Ordering::SeqCst) {
            return Err(denied("revoked native source"));
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        send(
            &json!({"owner_user_id":"actual-owner"}),
            &self.bytes[offset as usize..offset as usize + max],
        )?;
        if terminal {
            self.delivered.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
}
struct Handler {
    source: Arc<Source>,
    sent: Mutex<Vec<WebRTCWireFrame>>,
    capability: bool,
    backpressured: bool,
    policy: bool,
    fail_at: Option<usize>,
    revoke_after_first: bool,
}
impl Handler {
    fn new(source: Arc<Source>) -> Self {
        Self {
            source,
            sent: Mutex::new(Vec::new()),
            capability: true,
            backpressured: false,
            policy: true,
            fail_at: None,
            revoke_after_first: false,
        }
    }
}
#[async_trait]
impl WebRTCConnectionHandler for Handler {
    type Peer = String;
    fn connect_stream(&self) -> RxStream<String> {
        RxSubject::new().subscribe()
    }
    fn disconnect_stream(&self) -> RxStream<String> {
        RxSubject::new().subscribe()
    }
    fn message_stream(&self) -> RxStream<PeerWithMessage<String>> {
        RxSubject::new().subscribe()
    }
    fn response_stream(&self) -> RxStream<PeerWithResponse<String>> {
        RxSubject::new().subscribe()
    }
    fn error_stream(&self) -> RxStream<RxError> {
        RxSubject::new().subscribe()
    }
    fn document_fields_for_peer(&self, _: &String, _: &str) -> Option<Vec<String>> {
        None
    }
    fn buffered_bytes(&self, _: &String) -> usize {
        if self.backpressured {
            WEBRTC_BUFFERED_HIGH_WATER + 1
        } else {
            0
        }
    }
    fn peer_capability_token(&self, _: &String) -> Option<String> {
        self.capability.then(|| "authenticated-fixture".into())
    }
    fn document_filter_for_peer(&self, _: &String, _: &str) -> Option<WebRTCDocumentFilter> {
        self.policy.then(|| {
            Arc::new(|doc: &Value| doc["owner_user_id"] == "actual-owner") as WebRTCDocumentFilter
        })
    }
    async fn send(&self, _: &String, frame: WebRTCWireFrame) -> RxResult<()> {
        assert!(
            self.source.lock.try_lock().is_none(),
            "actual send escaped native guard"
        );
        let mut sent = self.sent.lock();
        if self.fail_at == Some(sent.len()) {
            return Err(denied("actual send failed"));
        }
        sent.push(frame);
        if self.revoke_after_first {
            self.source.live.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    async fn close(&self) -> RxResult<()> {
        Ok(())
    }
}
fn request() -> FileFetchRequest {
    FileFetchRequest {
        request_id: "request".into(),
        collection_name: "guest_frames".into(),
        file_id: "native-frame".into(),
        range: None,
        known_sequences: vec![],
    }
}
async fn run(handler: Arc<Handler>, request: FileFetchRequest) -> RxResult<()> {
    stream_guarded_file(
        handler.clone(),
        "peer-generation".into(),
        request,
        handler.source.clone(),
        Arc::new(AtomicBool::new(false)),
    )
    .await
}
#[tokio::test]
async fn guarded_send_keeps_native_lock_through_complete_transfer() {
    let source = Source::new();
    let handler = Arc::new(Handler::new(source.clone()));
    run(handler.clone(), request()).await.unwrap();
    assert!(source.delivered.load(Ordering::SeqCst));
    let sent = handler.sent.lock();
    assert_eq!(sent.len(), 3);
    let chunks: Vec<FileFetchChunk> = sent
        .iter()
        .map(|frame| {
            let WebRTCWireFrame::Message(message) = frame else {
                panic!("expected chunk");
            };
            serde_json::from_value(message.params[0].clone()).unwrap()
        })
        .collect();
    assert_eq!(
        chunks.iter().map(|c| c.sequence).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(!chunks[0].complete && !chunks[1].complete && chunks[2].complete);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(&chunks[0].bytes_base64)
            .unwrap()
            .len(),
        8 * 1024
    );
    assert!(chunks[2].bytes_base64.is_empty());
}
#[tokio::test]
async fn missing_peer_policy_or_capability_never_sends_bytes() {
    for (capability, policy) in [(false, true), (true, false)] {
        let source = Source::new();
        let handler = Arc::new(Handler {
            capability,
            policy,
            ..Handler::new(source.clone())
        });
        assert!(run(handler.clone(), request()).await.is_err());
        assert!(handler.sent.lock().is_empty());
        assert!(!source.delivered.load(Ordering::SeqCst));
    }
}
#[tokio::test]
async fn failed_send_and_midstream_revocation_cannot_complete_frame() {
    for (fail_at, revoke) in [
        (Some(0), false),
        (Some(1), false),
        (Some(2), false),
        (None, true),
    ] {
        let source = Source::new();
        let handler = Arc::new(Handler {
            fail_at,
            revoke_after_first: revoke,
            ..Handler::new(source.clone())
        });
        assert!(run(handler.clone(), request()).await.is_err());
        assert_eq!(handler.sent.lock().len(), fail_at.unwrap_or(1));
        assert!(!source.delivered.load(Ordering::SeqCst));
    }
}
#[tokio::test]
async fn dropping_backpressured_delivery_cancels_without_late_chunks() {
    tokio::time::timeout(Duration::from_secs(2), async {
        let source = Source::new();
        let handler = Arc::new(Handler {
            backpressured: true,
            ..Handler::new(source.clone())
        });
        let cancelled = Arc::new(AtomicBool::new(false));
        let task_handler = handler.clone();
        let task_cancelled = cancelled.clone();
        let task = tokio::spawn(async move {
            stream_guarded_file(
                task_handler.clone(),
                "peer-generation".into(),
                request(),
                task_handler.source.clone(),
                task_cancelled,
            )
            .await
        });
        while source.reads.load(Ordering::SeqCst) == 0 {
            assert!(!task.is_finished());
            tokio::task::yield_now().await;
        }
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(cancelled.load(Ordering::SeqCst));
        assert!(handler.sent.lock().is_empty());
        assert!(!source.delivered.load(Ordering::SeqCst));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn partial_or_cached_fetch_never_reads_native_frame() {
    for ranged in [false, true] {
        let source = Source::new();
        let handler = Arc::new(Handler::new(source.clone()));
        let mut request = request();
        if ranged {
            request.range = Some(FileRange {
                offset: 0,
                length: 1,
            });
        } else {
            request.known_sequences = vec![0];
        }
        assert!(run(handler.clone(), request).await.is_err());
        assert_eq!(source.reads.load(Ordering::SeqCst), 0);
        assert!(handler.sent.lock().is_empty());
    }
}
