use super::*;
use crate::{
    plugins::replication_webrtc::{PeerWithMessage, PeerWithResponse, WebRTCPublicationGuard},
    rx_error::RxResult,
    rxjs_compat::{RxStream, RxSubject},
};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Guard(bool);
impl WebRTCPublicationGuard for Guard {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        if self.0 {
            publish()
        } else {
            Err(new_rx_error("retired-fixture", None))
        }
    }
}
struct Handler {
    support: bool,
    ordinary: AtomicUsize,
    guarded: AtomicUsize,
    responses: RxSubject<PeerWithResponse<(String, u64)>>,
    disconnects: RxSubject<(String, u64)>,
}
#[async_trait::async_trait]
impl WebRTCConnectionHandler for Handler {
    type Peer = (String, u64);
    fn connect_stream(&self) -> RxStream<Self::Peer> {
        RxSubject::new().subscribe()
    }
    fn disconnect_stream(&self) -> RxStream<Self::Peer> {
        self.disconnects.subscribe()
    }
    fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>> {
        RxSubject::new().subscribe()
    }
    fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>> {
        self.responses.subscribe()
    }
    fn error_stream(&self) -> RxStream<RxError> {
        RxSubject::new().subscribe()
    }
    async fn send(&self, _: &Self::Peer, _: WebRTCWireFrame) -> RxResult<()> {
        self.ordinary.fetch_add(1, Ordering::SeqCst);
        Err(new_rx_error("ordinary-send-forbidden", None))
    }
    async fn send_guarded(
        &self,
        peer: &Self::Peer,
        frame: WebRTCWireFrame,
        guard: Arc<dyn WebRTCPublicationGuard>,
    ) -> RxResult<()> {
        self.guarded.fetch_add(1, Ordering::SeqCst);
        if !self.support {
            return Err(new_rx_error(
                "ctox_webrtc_publication_guard_unsupported",
                None,
            ));
        }
        let WebRTCWireFrame::Message(message) = frame else {
            panic!("expected request")
        };
        guard.with_current(&mut || {
            // Synchronous responses during send expose subscription ordering and
            // same-route/replacement-generation correlation bugs.
            for (response_peer, id, result) in [
                (
                    (peer.0.clone(), peer.1 + 1),
                    message.id.clone(),
                    "replacement",
                ),
                (
                    ("foreign-route".into(), peer.1),
                    message.id.clone(),
                    "foreign",
                ),
                (peer.clone(), "different-request".into(), "other-request"),
                (peer.clone(), message.id.clone(), "exact"),
            ] {
                self.responses.next(PeerWithResponse {
                    peer: response_peer,
                    response: WebRTCResponse {
                        id,
                        result: serde_json::json!(result),
                        error: None,
                        collection: None,
                    },
                });
            }
            Ok(())
        })
    }
    async fn close(&self) -> RxResult<()> {
        Ok(())
    }
}
fn handler(support: bool) -> Arc<Handler> {
    Arc::new(Handler {
        support,
        ordinary: AtomicUsize::new(0),
        guarded: AtomicUsize::new(0),
        responses: RxSubject::new(),
        disconnects: RxSubject::new(),
    })
}
async fn request(handler: Arc<Handler>, guard: bool) -> RxResult<WebRTCResponse> {
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        send_message_and_await_answer_guarded(
            handler,
            ("exact-route".into(), 7),
            WebRTCMessage {
                id: "actual-request".into(),
                method: "fixture".into(),
                params: vec![],
                collection: None,
            },
            Arc::new(Guard(guard)),
        ),
    )
    .await
    .expect("guarded round trip must settle")
}
#[tokio::test]
async fn guarded_request_subscribes_before_send_and_correlates_exact_generation_and_id() {
    let handler = handler(true);
    assert_eq!(
        request(handler.clone(), true).await.unwrap().result,
        serde_json::json!("exact")
    );
    assert_eq!(handler.guarded.load(Ordering::SeqCst), 1);
    assert_eq!(handler.ordinary.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn unsupported_or_retired_guarded_request_never_calls_ordinary_send() {
    for (support, live, code) in [
        (false, true, "ctox_webrtc_publication_guard_unsupported"),
        (true, false, "retired-fixture"),
    ] {
        let handler = handler(support);
        assert_eq!(
            request(handler.clone(), live).await.unwrap_err().code(),
            code
        );
        assert_eq!(handler.guarded.load(Ordering::SeqCst), 1);
        assert_eq!(handler.ordinary.load(Ordering::SeqCst), 0);
    }
}
