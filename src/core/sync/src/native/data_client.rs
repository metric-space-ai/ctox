//! Connection-scoped discovery for query-only native data consumers.
//! Signaling narrows routes; NativeSessionTarget alone verifies the source.
use super::NativePool;
use futures_util::{FutureExt, StreamExt};
use parking_lot::Mutex;
use rxdb::plugins::replication_webrtc::{SignalingClient, WebRTCConnectionHandler};
use std::{
    collections::{BTreeMap, BTreeSet},
    panic::AssertUnwindSafe,
    sync::Arc,
    time::Duration,
};
use tokio::task::JoinHandle;

const MAX_CANDIDATES: usize = 8;
const MAX_ATTEMPTS: u8 = 3;

pub(super) struct DataClientDiscovery {
    task: Mutex<Option<JoinHandle<()>>>,
}

impl DataClientDiscovery {
    pub(super) fn start(
        pool: &NativePool,
        signaling: Arc<SignalingClient>,
        peer_gate: Arc<dyn Fn(&String) -> bool + Send + Sync>,
    ) -> Self {
        // Subscribe before join. The behavior stream also carries the current
        // roster if the transport has already observed it.
        let mut lists = signaling.peer_list_stream();
        let handler = pool.connection_handler.clone();
        let mut opened = handler.connect_stream();
        let mut closed = handler.disconnect_stream();
        let weak_pool = Arc::downgrade(pool);
        let terminal_pool = weak_pool.clone();
        let task = tokio::spawn(async move {
            let discover = async move {
                let mut advertised = BTreeSet::new();
                let mut attempted = BTreeMap::<String, u8>::new();
                let mut own_route = None;
                let mut retry = false;
                loop {
                    tokio::select! {
                        list = lists.next() => {
                            let Some(list) = list else { break; };
                            advertised = list.into_iter().collect();
                        },
                        event = opened.next() => { if event.is_none() { break; } },
                        event = closed.next() => { if event.is_none() { break; } },
                        _ = tokio::time::sleep(Duration::from_secs(2)), if retry => {},
                    }
                    let Some(pool) = weak_pool.upgrade() else {
                        break;
                    };
                    if pool.canceled.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    drop(pool);
                    let current = signaling.own_peer_id();
                    if current != own_route {
                        own_route = current;
                        attempted.clear();
                    }
                    retry = false;
                    let Some(own) = own_route.as_ref() else {
                        continue;
                    };
                    // Fail closed on missing/mismatched own admission. Only a
                    // new roster can change this; do not busy-poll credentials.
                    if signaling.peer_role(own).as_deref() != Some("browser") {
                        continue;
                    }
                    let candidates: BTreeSet<String> = advertised
                        .iter()
                        .filter(|route| {
                            *route != own
                                && peer_gate(route)
                                && signaling.peer_role(route).as_deref() == Some("ctox_instance")
                        })
                        .cloned()
                        .collect();
                    if candidates.len() > MAX_CANDIDATES {
                        break;
                    }
                    attempted.retain(|route, _| candidates.contains(route));
                    for route in candidates {
                        if handler.connection_for_peer(&route).is_some() {
                            continue;
                        }
                        let attempts = attempted.entry(route.clone()).or_default();
                        if *attempts >= MAX_ATTEMPTS {
                            continue;
                        }
                        *attempts += 1;
                        let _ = tokio::time::timeout(
                            Duration::from_secs(3),
                            handler.connect_data_peer(route),
                        )
                        .await;
                        retry |= *attempts < MAX_ATTEMPTS;
                    }
                    // No timer survives once all routes are connected or their
                    // bounded budget is exhausted. Re-advertisement after leave
                    // or a new local signaling identity starts a fresh budget.
                }
            };
            let _ = AssertUnwindSafe(discover).catch_unwind().await;
            // An unexpectedly ended/panicked discovery cannot leave a seemingly
            // usable transport behind. Normal shutdown aborts and awaits us first.
            if let Some(pool) = terminal_pool.upgrade() {
                pool.cancel().await;
            }
        });
        Self {
            task: Mutex::new(Some(task)),
        }
    }

    pub(super) async fn shutdown(&self) {
        let task = self.task.lock().take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for DataClientDiscovery {
    fn drop(&mut self) {
        if let Some(task) = self.task.get_mut().take() {
            task.abort();
        }
    }
}
