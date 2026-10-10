// Origin: CTOX
// License: AGPL-3.0-only
//! Service-owned source transport. JSON requests contain operations, never admission.
//! The receiver alone constructs AdmittedConsumerAuthority from its accepted peer.
use crate::{
    native_transfer_accounts::{NativeTransferAccount, NativeTransferAccountHost},
    transfers_native::QueryDatabase,
};
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    ipc::{IpcService, IpcServiceFuture, LocalIpcStream},
    local_host::{HostDirectoryLock, LocalIpcHost},
    native::NativeSyncSession,
};
use futures_util::StreamExt;
use rxdb::plugins::replication_webrtc::{
    webrtc_helper::send_message_and_await_answer_guarded, WebRTCConnectionHandler, WebRTCMessage,
    WebRTCPublicationGuard, WebRTCRsConnection,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    future::Future,
    io,
    path::Path,
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const METHOD: &str = "ctox.workjet.project.supervisor.execution.v1";
const CONSUMER_METHOD: &str = "ctox.workjet.consumer.v1";
const REQUEST_BYTES: usize = 256 * 1024;
const RESPONSE_BYTES: usize = 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(10);

fn unavailable() -> anyhow::Error {
    anyhow::anyhow!("native Supervisor source is unavailable")
}
fn wire_error() -> rxdb::rx_error::RxError {
    rxdb::rx_error::new_rx_error("native_supervisor_source_retired", None)
}

/// Created from the encrypted native enrollment, never a wire account or guest credential.
struct EnrollmentGuard {
    host: Arc<NativeTransferAccountHost>,
    original: NativeTransferAccount,
    fingerprint: String,
    expires_at_ms: i64,
    retired: Mutex<bool>,
}
impl EnrollmentGuard {
    fn capture(
        host: Arc<NativeTransferAccountHost>,
        original: NativeTransferAccount,
        expires_at_ms: i64,
    ) -> Result<Arc<Self>> {
        let fingerprint = host
            .with_current_enrollment(&original, None, |fingerprint| Ok(fingerprint.to_owned()))?;
        Ok(Arc::new(Self {
            host,
            original,
            fingerprint,
            expires_at_ms,
            retired: Mutex::new(false),
        }))
    }
    fn current<T>(&self, apply: impl FnOnce() -> Result<T>) -> Result<T> {
        let retired = self.retired.lock().map_err(|_| unavailable())?;
        ensure!(
            !*retired && chrono::Utc::now().timestamp_millis() < self.expires_at_ms,
            "native Supervisor source retired or expired"
        );
        self.host
            .with_current_enrollment(&self.original, Some(&self.fingerprint), |_| apply())
    }
    fn retire(&self) {
        *self.retired.lock().unwrap_or_else(|e| e.into_inner()) = true;
    }
}
impl WebRTCPublicationGuard for EnrollmentGuard {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        self.current(|| publish().map_err(|_| unavailable()))
            .map_err(|_| wire_error())
    }
}

struct Source {
    database: Arc<QueryDatabase>,
    session: Arc<NativeSyncSession>,
    peer: WebRTCRsConnection,
    enrollment: Arc<EnrollmentGuard>,
    // These are remote, non-secret association facts, not a constructor/permit.
    association: Value,
}
impl Source {
    async fn open(root: &Path, target: &str, directory: &Path) -> Result<Self> {
        let database = QueryDatabase::at_path(directory.join("peer.sqlite3"));
        let result = Self::open_with_database(root, target, database.clone()).await;
        if result.is_err() {
            database.close().await?;
        }
        result
    }
    async fn open_with_database(
        root: &Path,
        target: &str,
        database: Arc<QueryDatabase>,
    ) -> Result<Self> {
        let options_database = database.clone();
        let host = NativeTransferAccountHost::new(
            root.to_owned(),
            Arc::new(move |_| {
                let database = options_database.clone();
                Box::pin(async move { database.options().await.map_err(io::Error::other) })
            }),
        );
        let original = host
            .account(target)
            .await?
            .context("native Source target is not enrolled")?;
        for attempt in 0..2 {
            let (mut options, deadline) = match host.recovery_options(target).await? {
                Some(options) if attempt == 0 => (options, None),
                Some(_) => return Err(unavailable()),
                None => {
                    let (options, deadline) = host.native_options_with_deadline(target).await?;
                    (options, Some(deadline))
                }
            };
            ensure!(
                host.account(target).await?.as_ref() == Some(&original),
                "native Source account changed"
            );
            // Seal the credential generation before network awaits; never adopt a
            // rotation that happened during startup as if it admitted this peer.
            let enrollment = deadline
                .map(|deadline| {
                    EnrollmentGuard::capture(host.clone(), original.clone(), deadline.expires_at_ms)
                })
                .transpose()?;
            options.local_session_provider = Some(host.provider_for_account(original.clone()));
            let session = Arc::new(
                NativeSyncSession::start_data_client(options)
                    .await
                    .map_err(|_| unavailable())?,
            );
            let ready = async {
                let peer = tokio::time::timeout(Duration::from_secs(20), async {
                    loop {
                        ensure!(
                            host.account(target).await?.as_ref() == Some(&original),
                            "native Source account changed"
                        );
                        let pool = session.pool();
                        ensure!(
                            !pool.canceled.load(std::sync::atomic::Ordering::SeqCst),
                            "native Source stopped"
                        );
                        let peers = pool.connection_handler.current_connections();
                        ensure!(peers.len() <= 1, "ambiguous native Source");
                        if let Some(peer) = peers.into_iter().next() {
                            if pool.is_peer_ready_for_control(&peer) {
                                break Ok::<_, anyhow::Error>(peer);
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .map_err(|_| unavailable())??;
                let proof = tokio::time::timeout(
                    DEADLINE,
                    session.peer_identity_proof(
                        peer.clone(),
                        &original.public_identity,
                        &original.instance_id,
                    ),
                )
                .await
                .map_err(|_| unavailable())?
                .map_err(|_| unavailable())?;
                ensure!(
                    proof.principal.as_ref() == Some(&original.principal)
                        && host.account(target).await?.as_ref() == Some(&original),
                    "native Source principal changed"
                );
                Ok::<_, anyhow::Error>(peer)
            }
            .await;
            let peer = match ready {
                Ok(peer) => peer,
                Err(error) => {
                    session.shutdown().await;
                    let _ = database.close().await;
                    return Err(error);
                }
            };
            if let Some(enrollment) = enrollment {
                let mut source = Self {
                    database,
                    session,
                    peer,
                    enrollment,
                    association: Value::Null,
                };
                match source
                    .exchange_method(CONSUMER_METHOD, vec![json!({"version":1})])
                    .await
                {
                    Ok(reply) if association_matches(&original, &reply) => {
                        source.association = reply;
                        return Ok(source);
                    }
                    _ => {
                        source.shutdown().await?;
                        return Err(anyhow::anyhow!(
                            "native Source has no current Owner/computer association"
                        ));
                    }
                }
            }
            let renewed = host
                .provision_from_session(
                    crate::native_data_device::NativeDeviceKeyScope {
                        target_id: original.target_id.clone(),
                        source_instance_id: original.instance_id.clone(),
                        source_public_identity: original.public_identity.clone(),
                        account_epoch: original.account_epoch,
                    },
                    &session,
                    &peer,
                )
                .await;
            session.shutdown().await;
            if renewed.is_err() || host.account(target).await?.as_ref() != Some(&original) {
                database.close().await?;
                return Err(unavailable());
            }
        }
        database.close().await?;
        Err(unavailable())
    }
    fn live(&self) -> Result<()> {
        let pool = self.session.pool();
        ensure!(
            !pool.canceled.load(std::sync::atomic::Ordering::SeqCst)
                && pool.connection_handler.is_peer_current(&self.peer)
                && pool.is_peer_ready_for_control(&self.peer),
            "native Source generation retired"
        );
        self.enrollment.current(|| Ok(()))
    }
    async fn exchange_method(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        self.live()?;
        let pool = self.session.pool();
        let response = tokio::select! {
            biased;
            _ = pool.cancelled() => return Err(unavailable()),
            response = tokio::time::timeout(DEADLINE, send_message_and_await_answer_guarded(
                pool.connection_handler.clone(), self.peer.clone(), WebRTCMessage {
                    id: format!("supervisor-source-{}", uuid::Uuid::new_v4()), method: method.into(), params, collection: None,
                }, self.enrollment.clone(),
            )) => response.map_err(|_| unavailable())?.map_err(|_| unavailable())?,
        };
        self.live()?;
        ensure!(
            response.error.is_none()
                && serde_json::to_vec(&response.result)?.len() <= RESPONSE_BYTES,
            "native Supervisor response rejected"
        );
        Ok(response.result)
    }
    async fn exchange(&self, params: Vec<Value>) -> Result<Value> {
        let current = self
            .exchange_method(CONSUMER_METHOD, vec![json!({"version":1})])
            .await?;
        ensure!(
            current == self.association,
            "native Source computer association changed"
        );
        // The guarded receiver owns DTO validation, actual offer/lease and SDK identities.
        // There is no client-side authority reconstruction or transfer grant.
        self.exchange_method(METHOD, params).await
    }
    async fn publish(&self, stream: &mut dyn LocalIpcStream, bytes: &[u8]) -> io::Result<()> {
        let future = stream.write_all(bytes);
        tokio::pin!(future);
        tokio::time::timeout(DEADLINE, async {
            loop {
                let publication = std::future::poll_fn(|cx| {
                    self.session.pool().connection_handler.with_current_connection(&self.peer, || {
                        self.enrollment.current(|| Ok(future.as_mut().poll(cx)))
                    }).unwrap_or_else(|| Err(unavailable())).unwrap_or_else(|_| Poll::Ready(Err(io::Error::new(io::ErrorKind::PermissionDenied, "native Source publication retired"))))
                });
                tokio::select! {
                    biased;
                    result = publication => return result,
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {self.live().map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "native Source publication retired"))?;}
                }
            }
        }).await.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "native Source publication deadline"))?
    }
    async fn shutdown(&self) -> Result<()> {
        self.enrollment.retire(); // Synchronous before any transport/IPC drain.
        self.session.shutdown().await;
        self.database.close().await.map_err(|_| unavailable())
    }
}
fn association_matches(account: &NativeTransferAccount, reply: &Value) -> bool {
    let Some(device) = account.principal.device.as_ref() else {
        return false;
    };
    reply["version"] == 1
        && reply["consumer"]["actorUserId"] == account.principal.user_id
        && reply["consumer"]["actorEpoch"].as_u64() == Some(account.principal.authorization_epoch)
        && reply["consumer"]["pairingId"] == device.pairing_id
        && reply["consumer"]["deviceId"] == device.device_id
        && reply["consumer"]["proofKeyThumbprint"] == device.proof_key_thumbprint
        && [
            "ownerUserId",
            "computerId",
            "computerRevision",
            "pairingRevision",
        ]
        .iter()
        .all(|field| {
            reply["consumer"][field]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        })
}

/// One fixed enrolled target is captured by the process, never selected in JSON.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceRequest {
    version: u32,
    request_id: String,
    params: Vec<Value>,
}
impl SourceRequest {
    fn valid(&self) -> bool {
        self.version == 1
            && !self.request_id.is_empty()
            && self.request_id.len() <= 256
            && self.request_id.chars().all(|c| c.is_ascii_graphic())
            && self.params.len() == 1
            && self.params[0].is_object()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceResponse {
    version: u32,
    request_id: String,
    result: Value,
}
struct SourceIpc {
    source: Arc<Source>,
    slots: tokio::sync::Semaphore,
}
impl IpcService for SourceIpc {
    fn serve_connection(&self, mut stream: Box<dyn LocalIpcStream>) -> IpcServiceFuture<'_> {
        Box::pin(async move {
            let _slot = self.slots.try_acquire().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "native Source request budget exhausted",
                )
            })?;
            loop {
                let mut header = [0; 4];
                tokio::time::timeout(Duration::from_secs(60), stream.read_exact(&mut header))
                    .await
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::TimedOut, "native Source frame deadline")
                    })??;
                let size = u32::from_be_bytes(header) as usize;
                if size == 0 || size > REQUEST_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid native Source frame",
                    ));
                }
                let mut bytes = vec![0; size];
                tokio::time::timeout(DEADLINE, stream.read_exact(&mut bytes))
                    .await
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::TimedOut, "native Source payload deadline")
                    })??;
                let request: SourceRequest = serde_json::from_slice(&bytes).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid native Source operation envelope",
                    )
                })?;
                if !request.valid() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid native Source operation envelope",
                    ));
                }
                let result = match self.source.exchange(request.params).await {
                    Ok(reply) => json!({"kind":"reply", "reply":reply}),
                    Err(_) => {
                        json!({"kind":"rejected", "code":"native_supervisor_source_unavailable"})
                    }
                };
                let reply = serde_json::to_vec(&SourceResponse {
                    version: 1,
                    request_id: request.request_id,
                    result,
                })
                .map_err(io::Error::other)?;
                if reply.len() > RESPONSE_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "native Source reply budget exhausted",
                    ));
                }
                let mut frame = Vec::with_capacity(reply.len() + 4);
                frame.extend_from_slice(&(reply.len() as u32).to_be_bytes());
                frame.extend_from_slice(&reply);
                self.source.publish(stream.as_mut(), &frame).await?;
            }
        })
    }
}
async fn stop_signal() -> io::Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
    }
}

/// The managed Root/NodeService consumer must retain this process across UI Quit.
/// This entry point does not install that consumer or enroll/grant authority.
pub(crate) fn serve(root: &Path, target: &str, directory: &Path) -> Result<()> {
    let _directory = HostDirectoryLock::acquire(directory)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let source = Arc::new(Source::open(root, target, directory).await?);
        let pool = source.session.pool();
        // Subscribe before exposing IPC, then revalidate to cover an earlier disconnect.
        let mut disconnected = pool.connection_handler.disconnect_stream();
        let mut local = match LocalIpcHost::start(directory.to_owned(), Arc::new(SourceIpc {source:source.clone(), slots:tokio::sync::Semaphore::new(2)})).await {
            Ok(local) => local,
            Err(_) => {source.shutdown().await?; return Err(unavailable());}
        };
        if let Err(error) = source.live() {
            source.enrollment.retire();
            let _ = local.shutdown().await;
            let _ = source.shutdown().await;
            return Err(error);
        }
        println!("{}", json!({"protocolVersion":1,"endpoint":local.endpoint(),"transportReady":true,"executionReady":false}));
        let expires_in = Duration::from_millis(
            source.enrollment.expires_at_ms.saturating_sub(chrono::Utc::now().timestamp_millis()).max(0) as u64,
        );
        let lost_generation = async {
            while let Some(peer) = disconnected.next().await {
                if peer == source.peer {
                    return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "original native Source disconnected"));
                }
            }
            Err(io::Error::new(io::ErrorKind::ConnectionAborted, "native Source disconnect stream ended"))
        };
        let ended = tokio::select! {
            result = stop_signal() => result,
            result = local.wait_stopped() => result,
            result = lost_generation => result,
            _ = pool.cancelled() => Err(io::Error::new(io::ErrorKind::ConnectionAborted, "native Source generation stopped")),
            _ = tokio::time::sleep(expires_in) => Err(io::Error::new(io::ErrorKind::PermissionDenied, "native Source enrollment expired")),
        };
        source.enrollment.retire();
        let ipc = local.shutdown().await;
        let native = source.shutdown().await;
        ended?; ipc?; native
    })
}

#[cfg(test)]
#[path = "native_supervisor_transport_tests.rs"]
mod tests;
