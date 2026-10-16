//! Foreground-owned V4 responder. P4 owns WebRTC; P8 owns the only cursor.
//! No HTTP business data, second SQLite connection, or V3 fallback.
use super::{
    store_browser::{validate_user, BrowserAdmission, BrowserPeer},
    store_inputs::StoreInputs,
};
use crate::business_os::{launch_context_v4, policy::sync_rights};
use ctox_store::{
    change_log::{
        self,
        subscription::{
            protocol::{CoreMessage, OrderedWrite, Subscribe, Write},
            write_contract::OrderedWriter,
            ReadAuthority, Subscription, SubscriptionEvent, SubscriptionKey, WriteAuthority,
            WritePermission,
        },
    },
    StoreError,
};
use futures_util::{
    future::BoxFuture,
    stream::{self, BoxStream},
    StreamExt,
};
use rxdb::{
    plugins::replication_webrtc::{
        index_mod::{
            hello::{ApiRange, Codec, HelloPolicy, HelloReject, RejectReason, TransportOffer},
            v4_router::V4RouterConfig,
            v4_session::V4StreamSource,
        },
        webrtc_types::WebRTCPublicationGuard,
    },
    rx_error::{new_rx_error, RxResult},
};
use serde_json::Value;
use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Original typed causes stay in memory. Log only their classes/codes: RxError
/// parameters and io messages may contain signaling URLs or credentials.
#[derive(Clone, Debug)]
struct StartupFailure {
    stage: &'static str,
    cause: Arc<anyhow::Error>,
}
impl StartupFailure {
    fn at(stage: &'static str, error: impl Into<anyhow::Error>) -> Self {
        Self {
            stage,
            cause: Arc::new(error.into()),
        }
    }
    fn codes(&self) -> String {
        self.cause
            .chain()
            .map(|cause| {
                if let Some(rx) = cause.downcast_ref::<rxdb::rx_error::RxError>() {
                    let code = rx.code();
                    if code.len() <= 80
                        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    {
                        return format!("RxError({code})");
                    }
                    return "RxError(redacted-code)".into();
                }
                if let Some(io) = cause.downcast_ref::<std::io::Error>() {
                    return format!("io({:?})", io.kind());
                }
                if let Some(store) = cause.downcast_ref::<StoreError>() {
                    return match store {
                        StoreError::Busy => "store(Busy)",
                        StoreError::Closed => "store(Closed)",
                        StoreError::Cancelled => "store(Cancelled)",
                        StoreError::DeadlineExceeded => "store(DeadlineExceeded)",
                        StoreError::AlreadyOwned => "store(AlreadyOwned)",
                        StoreError::Panicked => "store(Panicked)",
                        StoreError::Invalid(_) => "store(Invalid)",
                        StoreError::Sqlite(_) => "store(Sqlite)",
                        StoreError::Io(_) => "store(Io)",
                    }
                    .into();
                }
                "cause(redacted)".into()
            })
            .collect::<Vec<_>>()
            .join(" <- ")
    }
}
impl std::fmt::Display for StartupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "browser V4 startup stage={} cause={}",
            self.stage,
            self.codes()
        )
    }
}
// Do not expose raw source Display through anyhow's automatic CLI rendering.
impl std::error::Error for StartupFailure {}

pub(super) struct BrowserIngress {
    inputs: Weak<StoreInputs>,
    state: std::sync::atomic::AtomicU8,
    failure: std::sync::Mutex<Option<StartupFailure>>,
    changed: tokio::sync::Notify,
    task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    session: tokio::sync::Mutex<
        Option<(
            ctox_sync::native::NativeSyncSession,
            Arc<rxdb::rx_database::RxDatabase>,
        )>,
    >,
}
impl BrowserIngress {
    pub fn new(inputs: &Arc<StoreInputs>) -> Arc<Self> {
        Arc::new(Self {
            inputs: Arc::downgrade(inputs),
            state: std::sync::atomic::AtomicU8::new(0),
            failure: std::sync::Mutex::new(None),
            changed: tokio::sync::Notify::new(),
            task: std::sync::Mutex::new(None),
            session: tokio::sync::Mutex::new(None),
        })
    }
    /// Exactly one owner-start task. HTTP never starts or retries signaling.
    pub fn start_owned(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
    ) -> Result<(), StoreError> {
        use std::sync::atomic::Ordering;
        self.state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| StoreError::AlreadyOwned)?;
        let owner = self.clone();
        *self.task.lock().map_err(|_| StoreError::Closed)? = Some(runtime.spawn(async move {
            // Bounded owner-owned retries; HTTP neither starts nor waits for them.
            for attempt in 0..3 {
                if owner.state.load(Ordering::Acquire) != 1 { return; }
                match owner.start().await {
                    Ok(()) => {
                        let _ = owner.state.compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire);
                        owner.changed.notify_waiters();
                        return;
                    }
                    Err(error) => {
                        eprintln!("[ctox v4] browser ingress attempt={} {error}", attempt + 1);
                        *owner.failure.lock().unwrap_or_else(|p| p.into_inner()) = Some(error);
                        if attempt == 2 { break; }
                        let changed = owner.changed.notified();
                        if owner.state.load(Ordering::Acquire) != 1 { return; }
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_millis(if attempt == 0 {250} else {1000})) => (),
                            _ = changed => (),
                        }
                    }
                }
            }
            let _ = owner.state.compare_exchange(1, 3, Ordering::AcqRel, Ordering::Acquire);
            owner.changed.notify_waiters();
        }));
        Ok(())
    }
    pub fn ready(&self) -> Result<(), StoreError> {
        if self.state.load(std::sync::atomic::Ordering::Acquire) == 2 {
            Ok(())
        } else {
            Err(StoreError::Busy)
        }
    }
    #[cfg(test)]
    pub(super) fn owner_started(&self) -> bool {
        self.state.load(std::sync::atomic::Ordering::Acquire) != 0
    }
    pub async fn wait_failed(&self) -> std::io::Result<()> {
        loop {
            let changed = self.changed.notified();
            if self.state.load(std::sync::atomic::Ordering::Acquire) == 3 {
                let failure = self
                    .failure
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone()
                    .expect("failed ingress retains its typed cause");
                return Err(std::io::Error::other(failure));
            }
            changed.await;
        }
    }
    async fn start(&self) -> Result<(), StartupFailure> {
        let mut session = self.session.lock().await;
        let inputs = self
            .inputs
            .upgrade()
            .ok_or_else(|| StartupFailure::at("owner", StoreError::Closed))?;
        inputs
            .instance_signer()
            .map_err(|e| StartupFailure::at("identity", e))?;
        if session.is_some() {
            return Ok(());
        }
        let root = inputs.root.clone();
        let (config, urls) =
            crate::persistence::run_core_sync(move || launch_context_v4::native_transport(&root))
                .await
                .map_err(|e| StartupFailure::at("routing", e))?;
        let storage =
            super::store_rxdb::native_storage(inputs.sync.writer.db_id().path().to_owned())
                .await
                .map_err(|e| StartupFailure::at("storage", e))?;
        let database =
            rxdb::rx_database::create_rx_database(rxdb::rx_database::RxDatabaseCreator {
                name: format!("v4-browser-{}", config.instance_id),
                storage,
                multi_instance: false,
                password: None,
                hash_function: Arc::new(Hash),
                options: Default::default(),
                ignore_duplicate: false,
                close_duplicates: false,
                event_reduce: false,
                allow_slow_count: false,
            })
            .await
            .map_err(|e| StartupFailure::at("database", e))?;
        let admission = inputs
            .browser_admission()
            .map_err(|e| StartupFailure::at("admission", e))?;
        let source = Arc::new(Source {
            inputs: Arc::downgrade(&inputs),
            admission: admission.clone(),
        });
        let router = V4RouterConfig {
            policy: HelloPolicy {
                instance_id: config.instance_id.clone(),
                bootstrap_min: 1,
                bootstrap_max: 1,
                app_api_ver: ApiRange {
                    min: "4.0".into(),
                    max: "4.0".into(),
                },
                capabilities: vec![
                    "agent.task".into(),
                    "agent.task.cancel".into(),
                    "agent.coding.turn".into(),
                    "agent.coding.cancel".into(),
                ],
                required_capabilities: vec![],
                transport: TransportOffer {
                    codecs: vec![Codec::Gzip],
                    max_frame_bytes: 16 * 1024,
                    fragmentation: true,
                    reassembly: true,
                },
            },
            authenticate: admission.authenticator(),
            create_local_hello: Arc::new(|_, _| {
                Box::pin(async {
                    Err(HelloReject::new(
                        RejectReason::AuthenticationFailed,
                        "browser tickets only",
                    ))
                })
            }),
            handshake_timeout: Duration::from_secs(15),
            application: Some(source),
        };
        let options = ctox_sync::native::NativeSyncOptions {
            peer_role: ctox_sync::native::NativePeerRole::CtoxInstance,
            local_session_provider: None,
            database: database.clone(),
            collections: vec![],
            signaling_urls: urls,
            room: config.sync_room,
            peer_session_id: config.native_peer_id,
            ice_servers: config
                .ice_servers
                .into_iter()
                .map(|mut value| {
                    if let Some(url) = value["urls"].as_str() {
                        value["urls"] = serde_json::json!([url]);
                    }
                    serde_json::from_value(value)
                })
                .collect::<Result<_, _>>()
                .map_err(|e| StartupFailure::at("ice", e))?,
            bringup_timeout: Duration::from_secs(15),
            admission: ctox_sync::native::NativeAdmission {
                peer: Arc::new(|_| true),
                session: Arc::new(|_, _| {
                    rxdb::plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation::Reject
                }),
                collection_read: Some(Arc::new(|_, _| false)),
                collection_write: Some(Arc::new(|_, _| false)),
                document_read: None,
                document_write: None,
                eager_pull: None,
                live_change: None,
            },
        };
        match ctox_sync::native::NativeSyncSession::start_with_pool_setup(options, move |pool| {
            pool.connection_handler.configure_v4_browser(router)
        })
        .await
        {
            Ok(mut native) => {
                if inputs.instance_signer().is_err() {
                    let _ = native.shutdown().await;
                    let _ = database.close().await;
                    return Err(StartupFailure::at("retired", StoreError::Closed));
                }
                *session = Some((native, database));
                Ok(())
            }
            Err(error) => {
                let _ = database.close().await;
                Err(StartupFailure::at("signaling", error))
            }
        }
    }
    pub async fn shutdown(&self) -> Result<(), StoreError> {
        self.state.store(4, std::sync::atomic::Ordering::Release);
        self.changed.notify_waiters();
        let task = self.task.lock().map_err(|_| StoreError::Closed)?.take();
        if let Some(task) = task {
            // The bounded bring-up owns its database until it closes on failure.
            // Let it observe retired inputs; cancellation must not orphan that DB.
            let _ = task.await;
        }
        if let Some((mut session, database)) = self.session.lock().await.take() {
            session.shutdown().await;
            database
                .close()
                .await
                .map_err(|_| StoreError::Invalid("browser database close".into()))?;
        }
        Ok(())
    }
}
struct Hash;
impl rxdb::types::HashFunction for Hash {
    fn hash<'a>(&'a self, input: String) -> rxdb::types::HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
    }
}
fn invalid() -> rxdb::rx_error::RxError {
    new_rx_error("ctox_v4_browser_authority", None)
}
fn sql_invalid(s: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(s.into())
}
struct Source {
    inputs: Weak<StoreInputs>,
    admission: Arc<BrowserAdmission>,
}
struct Reader {
    admission: Arc<BrowserAdmission>,
    peer: Arc<BrowserPeer>,
}
impl Reader {
    fn scope(
        &self,
        tx: &rusqlite::Transaction<'_>,
        key: &SubscriptionKey,
    ) -> rusqlite::Result<sync_rights::scope::ResolvedScope> {
        if self.peer.expires <= Instant::now()
            || key.participant != self.peer.user
            || self.peer.scopes.get(&key.collection) != Some(&key.scope_hash)
        {
            return Err(sql_invalid("subscription binding"));
        }
        validate_user(tx, &self.peer.user, &self.peer.role, self.peer.local)
            .map_err(|_| sql_invalid("principal revoked"))?;
        let active = ctox_store::schema::active_collection(tx, &key.collection)?
            .ok_or_else(|| sql_invalid("collection unavailable"))?;
        if active.gen != key.gen {
            return Err(sql_invalid("generation changed"));
        }
        let rights = sync_rights::read(tx)?;
        self.admission
            .authority()
            .router
            .with_snapshot(rights.rights_ver, &self.peer.epoch, |_, _| {
                if rights.rights_ver != self.peer.rights_ver {
                    return Err("rights changed".into());
                }
                let scope =
                    rights.resolve_collection(&self.peer.role, &self.peer.user, &key.collection)?;
                if scope.hash()? != key.scope_hash {
                    return Err("scope changed".into());
                }
                Ok(scope)
            })
            .map_err(sql_invalid)
    }
}
impl ReadAuthority for Reader {
    fn authorize(
        &self,
        tx: &rusqlite::Transaction<'_>,
        key: &SubscriptionKey,
    ) -> rusqlite::Result<()> {
        self.scope(tx, key).map(|_| ())
    }
    fn project(
        &self,
        tx: &rusqlite::Transaction<'_>,
        key: &SubscriptionKey,
        doc: &change_log::ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        let scope = self.scope(tx, key)?;
        match sync_rights::scope::scope_filter::project(
            scope.projection(&key.collection),
            &key.collection,
            &doc.id,
            doc.seq,
            doc.body.as_ref(),
        )
        .map_err(sql_invalid)?
        {
            sync_rights::scope::scope_filter::ProjectedChange::Put { doc, .. } => Ok(Some(doc)),
            sync_rights::scope::scope_filter::ProjectedChange::Unproject { .. } => Ok(None),
        }
    }
}
struct Publication {
    inputs: Weak<StoreInputs>,
    admission: Arc<BrowserAdmission>,
    peer: Arc<BrowserPeer>,
}
impl WebRTCPublicationGuard for Publication {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        self.inputs
            .upgrade()
            .ok_or_else(invalid)?
            .instance_signer()
            .map_err(|_| invalid())?;
        if self.peer.expires <= Instant::now() {
            return Err(invalid());
        }
        let mut result = None;
        self.admission
            .authority()
            .router
            .with_snapshot(self.peer.rights_ver, &self.peer.epoch, |_, _| {
                result = Some(publish());
                Ok(())
            })
            .map_err(|_| invalid())?;
        result.ok_or_else(invalid)?
    }
}
impl V4StreamSource for Source {
    fn publication(&self, peer: &str, packet: &Value) -> RxResult<Arc<dyn WebRTCPublicationGuard>> {
        let binding = self.admission.peer(peer).map_err(|_| invalid())?;
        if matches!(packet["type"].as_str(), Some("page" | "live"))
            && (packet["participant"] != binding.user
                || packet["epoch"] != binding.epoch
                || packet["collection"]
                    .as_str()
                    .and_then(|c| binding.scopes.get(c))
                    .map(String::as_str)
                    != packet["scope_hash"].as_str())
        {
            return Err(invalid());
        }
        Ok(Arc::new(Publication {
            inputs: self.inputs.clone(),
            admission: self.admission.clone(),
            peer: binding,
        }))
    }
    fn subscribe(
        &self,
        peer: &str,
        mut request: Value,
    ) -> BoxFuture<'static, RxResult<BoxStream<'static, RxResult<Value>>>> {
        let result = (|| {
            let binding = self.admission.peer(peer).map_err(|_| invalid())?;
            let inputs = self.inputs.upgrade().ok_or_else(invalid)?;
            inputs.instance_signer().map_err(|_| invalid())?;
            request.as_object_mut().ok_or_else(invalid)?.remove("type");
            let sub: Subscribe = serde_json::from_value(request).map_err(|_| invalid())?;
            if binding.scopes.get(&sub.collection) != Some(&sub.scope_hash) {
                return Err(invalid());
            }
            let key = SubscriptionKey {
                participant: binding.user.clone(),
                collection: sub.collection,
                scope_hash: sub.scope_hash,
                gen: sub.gen,
            };
            let cursor = Subscription::new(
                inputs.sync.readers.clone(),
                &inputs.sync.writer,
                key,
                sub.checkpoint,
            )
            .map_err(|_| invalid())?;
            let authority: Arc<dyn ReadAuthority> = Arc::new(Reader {
                admission: self.admission.clone(),
                peer: binding,
            });
            let signer = inputs.instance_signer().map_err(|_| invalid())?;
            Ok(stream::try_unfold(
                (cursor, authority, signer),
                |(mut cursor, authority, signer)| async move {
                    let event = cursor
                        .next_event(128, authority.clone(), &*signer, now)
                        .await
                        .map_err(|_| invalid())?;
                    let value = match event {
                        SubscriptionEvent::Page(p) => serde_json::to_value(p),
                        SubscriptionEvent::RequiresPage(p) => {
                            serde_json::to_value(CoreMessage::RequiresPage(p))
                        }
                    }
                    .map_err(|_| invalid())?;
                    Ok(Some((value, (cursor, authority, signer))))
                },
            )
            .boxed())
        })();
        Box::pin(async move { result })
    }
    fn write_results(&self, peer: &str) -> RxResult<BoxStream<'static, RxResult<Value>>> {
        let binding = self.admission.peer(peer).map_err(|_| invalid())?;
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let mut slot = binding.results.lock().map_err(|_| invalid())?;
        if slot.is_some() {
            return Err(invalid());
        }
        *slot = Some(tx);
        Ok(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|value| (value, rx))
        })
        .boxed())
    }
    fn write(&self, peer: &str, mut request: Value) -> BoxFuture<'static, RxResult<()>> {
        let setup = (|| {
            let binding = self.admission.peer(peer).map_err(|_| invalid())?;
            let inputs = self.inputs.upgrade().ok_or_else(invalid)?;
            request.as_object_mut().ok_or_else(invalid)?.remove("type");
            let write: OrderedWrite = serde_json::from_value(request).map_err(|_| invalid())?;
            let sender = binding
                .results
                .lock()
                .map_err(|_| invalid())?
                .clone()
                .ok_or_else(invalid)?;
            Ok((binding, inputs, write, sender, self.admission.clone()))
        })();
        Box::pin(async move {
            let (binding, inputs, write, sender, self_admission) = setup?;
            let writer = OrderedWriter::new(
                inputs.sync.writer.clone(),
                Arc::new(browser_write::BrowserWrite::new(
                    self_admission,
                    binding.clone(),
                )),
            );
            let batch = writer
                .submit(binding.device.clone(), write, now())
                .await
                .map_err(|_| invalid())?;
            // E14: this point is AFTER the owning writer's commit completion.
            // P4's reliable result lane retains delivery until its transport ACK.
            let deadline = batch.next_deadline_ms;
            deliver_batch(batch, &sender).await?;
            arm_gap(&binding, writer, sender, deadline)?;
            Ok(())
        })
    }
    fn app_activation(&self, _: &str, _: Value) -> BoxFuture<'static, RxResult<()>> {
        Box::pin(async { Err(new_rx_error("ctox_v4_app_activation_not_bound", None)) })
    }
    fn priority(&self, peer: &str, _: Value) -> BoxFuture<'static, RxResult<()>> {
        let result = self.admission.peer(peer).map(|_| ()).map_err(|_| invalid());
        Box::pin(async move { result })
    }
}
#[path = "store_browser_write.rs"]
mod browser_write;
#[path = "store_browser_gap.rs"]
pub(super) mod gap;
async fn deliver_batch(
    batch: ctox_store::change_log::subscription::write_contract::WriteBatch,
    sender: &tokio::sync::mpsc::Sender<RxResult<Value>>,
) -> RxResult<()> {
    for result in batch.results {
        let result =
            serde_json::to_value(CoreMessage::WriteResult(result)).map_err(|_| invalid())?;
        tokio::time::timeout(Duration::from_secs(5), sender.send(Ok(result)))
            .await
            .map_err(|_| invalid())?
            .map_err(|_| invalid())?;
    }
    Ok(())
}
/// P8 supplies a durable one-shot gap deadline, not a polling/retry loop. A
/// peer has at most one timer; an idle connection still receives expiry results.
fn arm_gap(
    peer: &Arc<BrowserPeer>,
    writer: OrderedWriter,
    sender: tokio::sync::mpsc::Sender<RxResult<Value>>,
    deadline: Option<u64>,
) -> RxResult<()> {
    let deliver: gap::Delivery = Arc::new(move |batch| {
        let sender = sender.clone();
        Box::pin(async move {
            let _ = deliver_batch(batch, &sender).await;
        })
    });
    peer.gap
        .arm(
            writer,
            peer.device.clone(),
            peer.expires,
            deliver,
            deadline,
            now,
        )
        .map_err(|_| invalid())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(all(test, unix))]
#[path = "store_browser_ingress_tests.rs"]
mod tests;
