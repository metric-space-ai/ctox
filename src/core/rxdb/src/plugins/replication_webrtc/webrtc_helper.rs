//! Port of `src/plugins/replication-webrtc/webrtc-helper.ts`.

use std::sync::Arc;

use tokio_stream::StreamExt;

use crate::plugins::replication_webrtc::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse, WebRTCWireFrame,
};
use crate::rx_error::{RxError, new_rx_error};
use crate::types::SharedHashFunction;

// ref: rxdb/src/plugins/replication-webrtc/webrtc-helper.ts:20-30
/// Deterministically pick which peer is master.
/// Both peers compute `H(own + "|" + other)` and `H(other + "|" + own)`; the
/// peer with the larger first hash is master. Hashing avoids 'aaaaaa' always
/// winning.
pub async fn is_master_in_webrtc_replication(
    hash_function: SharedHashFunction,
    own_storage_token: &str,
    other_storage_token: &str,
) -> bool {
    let a = format!("{own_storage_token}|{other_storage_token}");
    let b = format!("{other_storage_token}|{own_storage_token}");
    let ha = hash_function.hash(a).await;
    let hb = hash_function.hash(b).await;
    ha > hb
}

/// Upper bound for one request/answer round-trip. Generous on purpose: large
/// answers travel through the chunked frame transport whose own ack timeouts
/// (30s per window) bound a genuinely wedged peer well below this. The
/// browser side uses 15s/60s request timeouts, so 60s never fails first.
const REQUEST_ANSWER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

// ref: rxdb/src/plugins/replication-webrtc/webrtc-helper.ts:37-54
/// Send a message to the peer and await the answer. The answer is identified by
/// `message.id` on the response stream and is scoped to `peer`. Returns an
/// `RxError` with code `RC_WEBRTC_PEER` if the peer disconnects, the handler
/// shuts down, or no answer arrives within [`REQUEST_ANSWER_TIMEOUT`].
///
/// The disconnect race matters: the per-subscriber response stream ends only
/// when the whole HANDLER is dropped, not when one peer dies. Without racing
/// the peer's disconnect event, a request in flight when the peer dropped
/// hung its caller forever (stuck handshakes, stuck fork pull/push that
/// `cancel()` could not interrupt).
pub async fn send_message_and_await_answer<H>(
    handler: Arc<H>,
    peer: H::Peer,
    message: WebRTCMessage,
) -> Result<WebRTCResponse, RxError>
where
    H: WebRTCConnectionHandler + ?Sized,
{
    let request_id = message.id.clone();
    let peer_for_filter = peer.clone();
    // Subscribe to the response + disconnect streams BEFORE sending to avoid
    // races where the answer (or the disconnect) lands between send() and
    // subscribe().
    let mut response_stream = handler.response_stream();
    let mut disconnect_stream = handler.disconnect_stream();
    handler
        .send(&peer, WebRTCWireFrame::Message(message))
        .await?;
    let deadline = tokio::time::sleep(REQUEST_ANSWER_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            item = response_stream.next() => {
                match item {
                    Some(item) => {
                        if item.peer == peer_for_filter && item.response.id == request_id {
                            return Ok(item.response);
                        }
                    }
                    None => {
                        return Err(new_rx_error(
                            "RC_WEBRTC_PEER",
                            Some(serde_json::json!({
                                "message": "WebRTC response stream ended before an answer was received",
                                "requestId": request_id,
                            })),
                        ));
                    }
                }
            }
            gone = disconnect_stream.next() => {
                match gone {
                    Some(p) => {
                        if p == peer_for_filter {
                            return Err(new_rx_error(
                                "RC_WEBRTC_PEER",
                                Some(serde_json::json!({
                                    "message": "peer disconnected before an answer was received",
                                    "requestId": request_id,
                                })),
                            ));
                        }
                    }
                    None => {
                        return Err(new_rx_error(
                            "RC_WEBRTC_PEER",
                            Some(serde_json::json!({
                                "message": "WebRTC disconnect stream ended before an answer was received",
                                "requestId": request_id,
                            })),
                        ));
                    }
                }
            }
            _ = &mut deadline => {
                return Err(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": format!(
                            "no answer within {}s",
                            REQUEST_ANSWER_TIMEOUT.as_secs()
                        ),
                        "requestId": request_id,
                    })),
                ));
            }
        }
    }
}
