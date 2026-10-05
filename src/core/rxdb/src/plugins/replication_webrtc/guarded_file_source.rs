//! Native ephemeral frames: authorization stays held through the actual send.
//! No wire field constructs a source or grants document/guest authority.
use super::file_fetch_handler::{FileFetchChunk, FileFetchRequest, FILE_FETCH_ERROR_UNAUTHORIZED};
use super::protocol_contract_generated::{CTOX_FILE_RPC_CHUNK, CTOX_QUERY_MAX_RUNTIME_MS};
use super::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCWireFrame, WEBRTC_BUFFERED_HIGH_WATER,
};
use crate::rx_error::{new_rx_error, RxResult};
use base64::Engine;
use serde_json::{json, Value};
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::task::Poll;
use std::time::{Duration, Instant};

/// Implemented by the retained native lifecycle owner. Metadata is native
/// output for peer filtering, never input used to reconstruct guest permission.
pub trait GuardedFileSource: Send + Sync {
    fn byte_len(&self, file_id: &str) -> RxResult<u64>;
    /// Invoke send exactly once while current worker/policy/controller authority
    /// is held. A terminal send succeeds only after all requested bytes were sent.
    /// The borrowed current checker stays inside this callback's native guards.
    /// Send may call it repeatedly on the same blocking worker while awaiting
    /// transport. Neither checker nor data slices may escape the callback.
    fn with_current_chunk(
        &self,
        file_id: &str,
        offset: u64,
        max_bytes: usize,
        terminal: bool,
        capability_token: &str,
        send: &mut dyn FnMut(&Value, &[u8], &mut dyn FnMut() -> RxResult<()>) -> RxResult<()>,
    ) -> RxResult<()>;
}

#[cfg(test)]
#[path = "guarded_file_source_tests.rs"]
mod tests;

fn denied(message: &str) -> crate::rx_error::RxError {
    new_rx_error(
        FILE_FETCH_ERROR_UNAUTHORIZED,
        Some(json!({"message":message})),
    )
}

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Frames cannot resume partial generations: a successful terminal describes
/// one complete current image, not a mixture of old cached chunks and new bytes.
pub(super) async fn stream_guarded_file<H: WebRTCConnectionHandler + 'static>(
    handler: Arc<H>,
    peer: H::Peer,
    request: FileFetchRequest,
    source: Arc<dyn GuardedFileSource>,
    cancelled: Arc<AtomicBool>,
) -> RxResult<()> {
    let _cancel_on_drop = CancelOnDrop(cancelled.clone());
    if request.range.is_some() || !request.known_sequences.is_empty() {
        return Err(denied(
            "ephemeral frames require one complete current fetch",
        ));
    }
    let read_source = source.clone();
    let read_id = request.file_id.clone();
    let len = tokio::task::spawn_blocking(move || read_source.byte_len(&read_id))
        .await
        .map_err(|_| denied("native frame source stopped"))??;
    if len == 0 || len > 16 * 1024 * 1024 {
        return Err(denied("native frame length is invalid"));
    }
    let deadline = Instant::now() + Duration::from_millis(CTOX_QUERY_MAX_RUNTIME_MS as u64);
    let runtime = tokio::runtime::Handle::current();
    let mut offset = 0u64;
    let mut sequence = 0u32;
    loop {
        while handler.buffered_bytes(&peer) > WEBRTC_BUFFERED_HIGH_WATER {
            if cancelled.load(Ordering::SeqCst)
                || Instant::now() >= deadline
                || !handler.is_peer_current(&peer)
            {
                return Err(denied("native frame delivery cancelled or expired"));
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
        if cancelled.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return Err(denied("native frame delivery cancelled or expired"));
        }
        let terminal = offset == len;
        let count = (len - offset).min(8 * 1024) as usize;
        let current_handler = handler.clone();
        let current_peer = peer.clone();
        let current_source = source.clone();
        let current_request = request.clone();
        let current_cancelled = cancelled.clone();
        let current_runtime = runtime.clone();
        // Native SQLite guards stay on this blocking worker, including the
        // bounded network send. No queued unguarded bytes cross this boundary.
        tokio::task::spawn_blocking(move || {
            let mut invoked = false;
            let capability_token = current_handler
                .peer_capability_token(&current_peer)
                .ok_or_else(|| denied("native frame peer capability is missing"))?;
            current_source.with_current_chunk(
                &current_request.file_id,
                offset,
                count,
                terminal,
                &capability_token,
                &mut |metadata, bytes, current| {
                    if invoked || bytes.len() != count {
                        return Err(denied("native frame send callback is invalid"));
                    }
                    let mut authorize = || {
                        current()?;
                        if current_cancelled.load(Ordering::SeqCst)
                            || Instant::now() >= deadline
                            || !current_handler.is_peer_current(&current_peer)
                            || current_handler
                                .peer_capability_token(&current_peer)
                                .as_deref()
                                != Some(capability_token.as_str())
                            || !current_handler.is_collection_authorized_for_peer(
                                &current_peer,
                                &current_request.collection_name,
                            )
                        {
                            return Err(denied("native frame send authority changed"));
                        }
                        let filter = current_handler
                            .document_filter_for_peer(
                                &current_peer,
                                &current_request.collection_name,
                            )
                            .ok_or_else(|| {
                                denied("native frame peer document policy is missing")
                            })?;
                        if !filter(metadata) {
                            return Err(denied("peer cannot read this native frame"));
                        }
                        // The peer checks may have consumed time. Native lease,
                        // principal and frame expiry are checked again before IO.
                        current()
                    };
                    authorize()?;
                    invoked = true;
                    let frame = WebRTCWireFrame::Message(WebRTCMessage {
                        id: format!("{}-f{}", current_request.request_id, sequence),
                        method: CTOX_FILE_RPC_CHUNK.into(),
                        collection: Some(current_request.collection_name.clone()),
                        params: vec![serde_json::to_value(FileFetchChunk {
                            request_id: current_request.request_id.clone(),
                            sequence,
                            bytes_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                            hash: Some(super::file_fetch_handler::sha256_hex(bytes)),
                            complete: terminal,
                            cancelled: None,
                        })
                        .map_err(|_| denied("native frame encoding failed"))?],
                    });
                    current_runtime.block_on(async {
                        let mut sending = Box::pin(current_handler.send(&current_peer, frame));
                        // Dropping spawn_blocking's JoinHandle does not stop it.
                        // Poll cancellation/current native authority even if the
                        // transport never wakes its own Pending future.
                        let mut watchdog = tokio::time::interval(Duration::from_millis(16));
                        watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        tokio::time::timeout(
                            deadline.saturating_duration_since(Instant::now()),
                            std::future::poll_fn(|cx| {
                                while watchdog.poll_tick(cx).is_ready() {}
                                if let Err(error) = authorize() {
                                    return Poll::Ready(Err(error));
                                }
                                sending.as_mut().poll(cx)
                            }),
                        )
                        .await
                        .map_err(|_| denied("native frame send timed out"))??;
                        // Cancellation/expiry while send returned cannot publish
                        // a completed observation or grant a later input.
                        authorize()
                    })
                },
            )?;
            if !invoked {
                return Err(denied("native frame source did not send"));
            }
            Ok(())
        })
        .await
        .map_err(|_| denied("native frame send worker stopped"))??;
        if terminal {
            return Ok(());
        }
        offset += count as u64;
        sequence = sequence
            .checked_add(1)
            .ok_or_else(|| denied("native frame sequence overflow"))?;
    }
}
