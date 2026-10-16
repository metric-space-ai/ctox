//! V4 application routing. Hosts MUST supply the authoritative store adapter.
//! No collection cache, synthetic data or V3 replication fallback lives here.
use super::hello::WireSeq;
use super::lanes::Lane;
use super::scheduler::{Incoming, NativeSendScheduler};
use crate::rx_error::{new_rx_error, RxResult};
use futures::{future::BoxFuture, stream::BoxStream, StreamExt};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// The P8 adapter binds this peer to current identity, issued scopes and rights.
/// It signs pages, reads the commit-driven change log and uses the ONE writer
/// transaction for document + op_id result. Replayed requests recheck authority.
pub trait V4StreamSource: Send + Sync {
    fn control(&self, _: &str, _: Value) -> BoxFuture<'static, RxResult<Option<Value>>> {
        Box::pin(async { Err(invalid()) })
    }
    /// Missing or stale authority is an error, including every retained replay.
    /// An optional guard would silently turn denied publication into unrestricted IO.
    fn publication(
        &self,
        peer: &str,
        packet: &Value,
    ) -> RxResult<Arc<dyn super::super::webrtc_types::WebRTCPublicationGuard>>;
    fn subscribe(
        &self,
        peer: &str,
        request: Value,
    ) -> BoxFuture<'static, RxResult<BoxStream<'static, RxResult<Value>>>>;
    /// Completes after durable ingress, including a bounded pending write behind
    /// a client_seq gap. It MUST NOT wait for that gap's predecessor/result.
    fn write(&self, peer: &str, request: Value) -> BoxFuture<'static, RxResult<()>>;
    /// Ready and expired results from the authoritative writer. A gap-closing
    /// write may release several results; deadlines use this same stream.
    /// Subscribe before admitting writes. No default/empty production fallback.
    fn write_results(&self, peer: &str) -> RxResult<BoxStream<'static, RxResult<Value>>>;
    fn app_activation(&self, peer: &str, report: Value) -> BoxFuture<'static, RxResult<()>>;
    fn priority(&self, peer: &str, hint: Value) -> BoxFuture<'static, RxResult<()>>;
}
#[path = "v4_scope_grant_control.rs"]
mod grant_control;
fn invalid() -> crate::rx_error::RxError {
    new_rx_error("ctox_v4_application_contract", None)
}

/// Association lifetime owns every lane and subscription task. Dropping it
/// aborts pending reads; source cursors resume only from committed checkpoints.
struct SubscriptionTask(tokio::task::JoinHandle<()>);
impl Drop for SubscriptionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub struct V4Session {
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for V4Session {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl V4Session {
    pub fn start(
        peer: String,
        scheduler: NativeSendScheduler,
        source: Arc<dyn V4StreamSource>,
    ) -> RxResult<Self> {
        let seq = Arc::new(AtomicU64::new(1));
        // Acquire every receiver before spawning: an incomplete association
        // must not leave a detached result/subscription task behind on error.
        let incoming = Lane::ALL
            .into_iter()
            .map(|lane| {
                scheduler
                    .take_incoming(Some(lane))
                    .map(|rx| (lane, rx))
                    .ok_or_else(invalid)
            })
            .collect::<RxResult<Vec<_>>>()?;
        let mut results = source.write_results(&peer)?;
        let mut tasks = Vec::new();
        let (result_sender, result_source, result_peer, result_seq) =
            (scheduler.clone(), source.clone(), peer.clone(), seq.clone());
        tasks.push(tokio::spawn(async move {
            while let Some(result) = results.next().await {
                let outcome = async {
                    let result = result?;
                    if result["type"] != "write_result" {
                        return Err(invalid());
                    }
                    let bytes = serde_json::to_vec(&result).map_err(|_| invalid())?;
                    let guard = result_source.publication(&result_peer, &result)?;
                    result_sender
                        .send(
                            Lane::Commands,
                            WireSeq(result_seq.fetch_add(1, Ordering::Relaxed)),
                            &bytes,
                            Some(guard),
                        )
                        .await
                }
                .await;
                if outcome.is_err() {
                    result_sender.close_now();
                    return;
                }
            }
            result_sender.close_now();
        }));
        for (lane, mut incoming) in incoming {
            let (sender, source, peer, seq) =
                (scheduler.clone(), source.clone(), peer.clone(), seq.clone());
            tasks.push(tokio::spawn(async move {
                // Per-lane tasks: a slow page never serializes command/control ingress.
                let mut subscriptions =
                    std::collections::HashMap::<String, SubscriptionTask>::new();
                while let Some(item) = incoming.recv().await {
                    let Incoming::Reliable {
                        bytes, delivery, ..
                    } = item
                    else {
                        break;
                    };
                    let result = async {
                        let message: Value =
                            serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                        match message.get("type").and_then(Value::as_str) {
                            Some("scope.grant.request") if lane == Lane::Control => {
                                grant_control::dispatch(&*source, &peer, message, &sender, &seq)
                                    .await?;
                            }
                            Some("subscribe") if lane == Lane::Control => {
                                let collection = message["collection"]
                                    .as_str()
                                    .ok_or_else(invalid)?
                                    .to_owned();
                                let mut stream = source.subscribe(&peer, message).await?;
                                subscriptions.remove(&collection);
                                let (sender, seq, source, peer) =
                                    (sender.clone(), seq.clone(), source.clone(), peer.clone());
                                let task = tokio::spawn(async move {
                                    while let Some(packet) = stream.next().await {
                                        let Ok(packet) = packet else {
                                            sender.close_now();
                                            break;
                                        };
                                        let lane = if packet["type"] == "live" {
                                            Lane::Live
                                        } else {
                                            Lane::InitialSync
                                        };
                                        let Ok(bytes) = serde_json::to_vec(&packet) else {
                                            sender.close_now();
                                            break;
                                        };
                                        let Ok(guard) = source.publication(&peer, &packet) else {
                                            sender.close_now();
                                            break;
                                        };
                                        if sender
                                            .send(
                                                lane,
                                                WireSeq(seq.fetch_add(1, Ordering::Relaxed)),
                                                &bytes,
                                                Some(guard),
                                            )
                                            .await
                                            .is_err()
                                        {
                                            break;
                                        }
                                    }
                                });
                                subscriptions.insert(collection, SubscriptionTask(task));
                            }
                            Some("write") if lane == Lane::Commands => {
                                source.write(&peer, message).await?;
                            }
                            Some("app_activation") if lane == Lane::Commands => {
                                source.app_activation(&peer, message).await?
                            }
                            Some("priority") if lane == Lane::Control => {
                                source.priority(&peer, message).await?
                            }
                            _ => return Err(invalid()),
                        }
                        Ok(())
                    }
                    .await;
                    if result.is_err() {
                        sender.close_now();
                        break;
                    }
                    if let Some(token) = delivery {
                        if token.acknowledge().is_err() {
                            sender.close_now();
                            break;
                        }
                    }
                }
                drop(subscriptions);
            }));
        }
        Ok(Self { tasks })
    }
}

// E25 CAS adapter is independent of the P4 session owner.
#[path = "v4_transfer.rs"]
mod transfer;
pub use transfer::{
    AuthorizedTransferBlob, NativeTransferChunk, NativeTransferRouter, TransferBlobAccess,
    TransferGet, TRANSFER_CHUNK_BYTES,
};
