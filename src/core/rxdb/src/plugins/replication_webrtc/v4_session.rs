//! V4 application routing. Hosts MUST supply the authoritative store adapter.
//! No collection cache, synthetic data or V3 replication fallback lives here.
use super::hello::WireSeq;
use super::lanes::Lane;
use super::scheduler::{Incoming, NativeSendScheduler};
use crate::rx_error::{new_rx_error, RxResult};
use futures::{future::BoxFuture, stream::BoxStream, StreamExt};
use serde_json::Value;
use std::sync::{atomic::{AtomicU64, Ordering}, Arc};

/// The P8 adapter binds this peer to current identity, issued scopes and rights.
/// It signs pages, reads the commit-driven change log and uses the ONE writer
/// transaction for document + op_id result. Replayed requests recheck authority.
pub trait V4StreamSource: Send + Sync {
    fn publication(&self, peer: &str, packet: &Value) -> RxResult<Option<Arc<dyn super::super::webrtc_types::WebRTCPublicationGuard>>>;
    fn subscribe(&self, peer: &str, request: Value) -> BoxFuture<'static, RxResult<BoxStream<'static, RxResult<Value>>>>;
    fn write(&self, peer: &str, request: Value) -> BoxFuture<'static, RxResult<Value>>;
    fn app_activation(&self, peer: &str, report: Value) -> BoxFuture<'static, RxResult<()>>;
    fn priority(&self, peer: &str, hint: Value) -> BoxFuture<'static, RxResult<()>>;
}
fn invalid() -> crate::rx_error::RxError { new_rx_error("ctox_v4_application_contract", None) }

/// Association lifetime owns every lane and subscription task. Dropping it
/// aborts pending reads; source cursors resume only from committed checkpoints.
struct SubscriptionTask(tokio::task::JoinHandle<()>);
impl Drop for SubscriptionTask { fn drop(&mut self) { self.0.abort(); } }
pub struct V4Session { tasks: Vec<tokio::task::JoinHandle<()>> }
impl Drop for V4Session { fn drop(&mut self) { for task in &self.tasks { task.abort(); } } }
impl V4Session {
    pub fn start(peer: String, scheduler: NativeSendScheduler, source: Arc<dyn V4StreamSource>) -> RxResult<Self> {
        let seq = Arc::new(AtomicU64::new(1));
        let mut tasks = Vec::new();
        for lane in Lane::ALL {
            let mut incoming = scheduler.take_incoming(Some(lane)).ok_or_else(invalid)?;
            let (sender, source, peer, seq) = (scheduler.clone(), source.clone(), peer.clone(), seq.clone());
            tasks.push(tokio::spawn(async move {
                // Per-lane tasks: a slow page never serializes command/control ingress.
                let mut subscriptions = std::collections::HashMap::<String, SubscriptionTask>::new();
                while let Some(item) = incoming.recv().await {
                    let Incoming::Reliable { bytes, delivery, .. } = item else { break; };
                    let result = async {
                        let message: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                        match message.get("type").and_then(Value::as_str) {
                            Some("subscribe") if lane == Lane::Control => {
                                let collection = message["collection"].as_str().ok_or_else(invalid)?.to_owned();
                                let mut stream = source.subscribe(&peer, message).await?;
                                subscriptions.remove(&collection);
                                let (sender, seq, source, peer) = (sender.clone(), seq.clone(), source.clone(), peer.clone());
                                let task = tokio::spawn(async move {
                                    while let Some(packet) = stream.next().await {
                                        let Ok(packet) = packet else { sender.close_now(); break; };
                                        let lane = if packet["type"] == "live" { Lane::Live } else { Lane::InitialSync };
                                        let Ok(bytes) = serde_json::to_vec(&packet) else { sender.close_now(); break; };
                                        let Ok(guard) = source.publication(&peer, &packet) else { sender.close_now(); break; };
                                        if sender.send(lane, WireSeq(seq.fetch_add(1, Ordering::Relaxed)), &bytes, guard).await.is_err() { break; }
                                    }
                                });
                                subscriptions.insert(collection, SubscriptionTask(task));
                            }
                            Some("write") if lane == Lane::Commands => {
                                let result = source.write(&peer, message).await?;
                                let bytes = serde_json::to_vec(&result).map_err(|_| invalid())?;
                                let guard = source.publication(&peer, &result)?;
                                sender.send(Lane::Commands, WireSeq(seq.fetch_add(1, Ordering::Relaxed)), &bytes, guard).await?;
                            }
                            Some("app_activation") if lane == Lane::Commands => source.app_activation(&peer, message).await?,
                            Some("priority") if lane == Lane::Control => source.priority(&peer, message).await?,
                            _ => return Err(invalid()),
                        }
                        Ok(())
                    }.await;
                    if result.is_err() { sender.close_now(); break; }
                    if let Some(token) = delivery { if token.acknowledge().is_err() { sender.close_now(); break; } }
                }
                drop(subscriptions);
            }));
        }
        Ok(Self { tasks })
    }
}
