//! Native ephemeral chunks carry authority to the independently draining queue.
use super::file_fetch_handler::{FileFetchChunk, FileFetchRequest, FILE_FETCH_ERROR_UNAUTHORIZED};
use super::protocol_contract_generated::{CTOX_FILE_RPC_CHUNK, CTOX_QUERY_MAX_RUNTIME_MS};
use super::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCPublicationGuard, WebRTCWireFrame,
    WEBRTC_BUFFERED_HIGH_WATER,
};
use crate::rx_error::{new_rx_error, RxResult};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// A native-only, single-chunk lease. Physical polls acquire its current fence;
/// no transaction, mutex guard or borrowed checker survives an await.
pub trait GuardedChunkLease: WebRTCPublicationGuard {
    /// Called only after the actual guarded transport completes successfully.
    /// Recheck and advance the exact contiguous generation under a fresh fence.
    fn complete(&self, current: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()>;
}
pub struct GuardedFileChunk {
    pub metadata: Value,
    pub bytes: Arc<[u8]>,
    pub lease: Arc<dyn GuardedChunkLease>,
}
pub trait GuardedFileSource: Send + Sync {
    fn byte_len(&self, file_id: &str) -> RxResult<u64>;
    fn prepare_chunk(
        &self,
        file_id: &str,
        offset: u64,
        max_bytes: usize,
        terminal: bool,
        capability_token: &str,
        cancelled: Arc<AtomicBool>,
    ) -> RxResult<GuardedFileChunk>;
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
struct CancelOnDrop(Option<Arc<AtomicBool>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.0 {
            cancelled.store(true, Ordering::SeqCst);
        }
    }
}
struct DeliveryGuard<H: WebRTCConnectionHandler> {
    handler: Arc<H>,
    peer: H::Peer,
    collection: String,
    capability: String,
    metadata: Value,
    native: Arc<dyn GuardedChunkLease>,
    cancelled: Arc<AtomicBool>,
    active: AtomicBool,
    deadline: Instant,
}
impl<H: WebRTCConnectionHandler> DeliveryGuard<H> {
    fn connection_current(&self) -> RxResult<()> {
        if !self.active.load(Ordering::SeqCst)
            || self.cancelled.load(Ordering::SeqCst)
            || Instant::now() >= self.deadline
            || !self.handler.is_peer_current(&self.peer)
            || self.handler.peer_capability_token(&self.peer).as_deref()
                != Some(self.capability.as_str())
        {
            return Err(denied(
                "native frame connection cancelled, replaced or expired",
            ));
        }
        Ok(())
    }
    fn peer_policy(&self) -> RxResult<()> {
        self.connection_current()?;
        if !self
            .handler
            .is_collection_authorized_for_peer(&self.peer, &self.collection)
        {
            return Err(denied("native frame collection denied"));
        }
        let filter = self
            .handler
            .document_filter_for_peer(&self.peer, &self.collection)
            .ok_or_else(|| denied("native frame document policy is missing"))?;
        if !filter(&self.metadata) {
            return Err(denied("peer cannot read this native frame"));
        }
        Ok(())
    }
}
impl<H: WebRTCConnectionHandler> WebRTCPublicationGuard for DeliveryGuard<H> {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        // Generic hooks may enter stores. Resolve them before native locks; the
        // native lease independently checks the same owner/read policy through
        // its already-held policy connection at the physical poll.
        self.peer_policy()?;
        let mut invoked = false;
        self.native.with_current(&mut || {
            self.connection_current()?;
            if invoked {
                return Err(denied("native frame publication callback repeated"));
            }
            invoked = true;
            publish()?;
            self.connection_current()
        })?;
        if !invoked {
            return Err(denied("native frame publication callback missing"));
        }
        Ok(())
    }
}
impl<H: WebRTCConnectionHandler> Drop for DeliveryGuard<H> {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

/// Complete current images only; cached/range resumes cannot combine generations.
pub(super) async fn stream_guarded_file<H: WebRTCConnectionHandler + 'static>(
    handler: Arc<H>,
    peer: H::Peer,
    request: FileFetchRequest,
    source: Arc<dyn GuardedFileSource>,
    cancelled: Arc<AtomicBool>,
) -> RxResult<()> {
    let mut cancel_on_drop = CancelOnDrop(Some(cancelled.clone()));
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
        let capability = handler
            .peer_capability_token(&peer)
            .ok_or_else(|| denied("native frame peer capability is missing"))?;
        let current_source = source.clone();
        let current_request = request.clone();
        let current_capability = capability.clone();
        let current_cancelled = cancelled.clone();
        let chunk = tokio::task::spawn_blocking(move || {
            current_source.prepare_chunk(
                &current_request.file_id,
                offset,
                count,
                terminal,
                &current_capability,
                current_cancelled,
            )
        })
        .await
        .map_err(|_| denied("native frame preparation stopped"))??;
        if chunk.bytes.len() != count {
            return Err(denied("native frame chunk length changed"));
        }
        let publication = Arc::new(DeliveryGuard {
            handler: handler.clone(),
            peer: peer.clone(),
            collection: request.collection_name.clone(),
            capability,
            metadata: chunk.metadata,
            native: chunk.lease.clone(),
            cancelled: cancelled.clone(),
            active: AtomicBool::new(true),
            deadline,
        });
        let frame = WebRTCWireFrame::Message(WebRTCMessage {
            id: format!("{}-f{}", request.request_id, sequence),
            method: CTOX_FILE_RPC_CHUNK.into(),
            collection: Some(request.collection_name.clone()),
            params: vec![serde_json::to_value(FileFetchChunk {
                request_id: request.request_id.clone(),
                sequence,
                bytes_base64: base64::engine::general_purpose::STANDARD.encode(&chunk.bytes),
                hash: Some(super::file_fetch_handler::sha256_hex(&chunk.bytes)),
                complete: terminal,
                cancelled: None,
            })
            .map_err(|_| denied("native frame encoding failed"))?],
        });
        // The owned publication guard travels into the actual queue. Outer
        // progress checks only bound cancellation; they never replace IO fencing.
        let result =
            tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), async {
                let sending = handler.send_guarded(&peer, frame, publication.clone());
                tokio::pin!(sending);
                let mut watchdog = tokio::time::interval(Duration::from_millis(16));
                watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        result = &mut sending => break result,
                        _ = watchdog.tick() => publication.with_current(&mut || Ok(()))?,
                    }
                }
            })
            .await
            .map_err(|_| denied("native frame send timed out"));
        let result = match result {
            Ok(result) => result,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            publication.active.store(false, Ordering::SeqCst);
            return Err(error);
        }
        publication.with_current(&mut || Ok(()))?;
        // Complete while this transfer is still active; any error/outer Drop
        // leaves the native pending generation consumed and permanently unusable.
        let current = publication.clone();
        tokio::task::spawn_blocking(move || {
            current.connection_current()?;
            current
                .native
                .complete(&mut || current.connection_current())?;
            current.active.store(false, Ordering::SeqCst);
            Ok::<(), crate::rx_error::RxError>(())
        })
        .await
        .map_err(|_| denied("native frame completion stopped"))??;
        if terminal {
            cancel_on_drop.0 = None;
            return Ok(());
        }
        offset += count as u64;
        sequence = sequence
            .checked_add(1)
            .ok_or_else(|| denied("native frame sequence overflow"))?;
    }
}
