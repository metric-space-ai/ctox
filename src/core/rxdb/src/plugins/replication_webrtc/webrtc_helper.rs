//! Port of `src/plugins/replication-webrtc/webrtc-helper.ts`.

use std::sync::Arc;

use tokio_stream::StreamExt;

use crate::plugins::replication_webrtc::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse, WebRTCWireFrame,
};
use crate::rx_error::{new_rx_error, RxError};
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

// ref: rxdb/src/plugins/replication-webrtc/webrtc-helper.ts:37-54
/// Send a message to the peer and await the answer. The answer is identified by
/// `message.id` on the response stream and is scoped to `peer`. Returns an
/// `RxError` with code `RC_WEBRTC_PEER` if the connection drops before an
/// answer arrives.
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
    // Subscribe to the response stream BEFORE sending to avoid races where
    // the answer arrives between send() and subscribe().
    let mut response_stream = handler.response_stream();
    handler
        .send(&peer, WebRTCWireFrame::Message(message))
        .await?;
    while let Some(item) = response_stream.next().await {
        if item.peer == peer_for_filter && item.response.id == request_id {
            return Ok(item.response);
        }
    }
    Err(new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "message": "WebRTC response stream ended before an answer was received",
            "requestId": request_id,
        })),
    ))
}
