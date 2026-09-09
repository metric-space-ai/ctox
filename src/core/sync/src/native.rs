//! Host-independent native RxDB/WebRTC lifecycle. Hosts supply data and policy.
use futures_util::FutureExt;
use rxdb::{
    plugins::replication_webrtc::{
        replicate_web_rtc_multi_with_validators, CollectionAuthzHook, CollectionEagerPullHook,
        CollectionLiveChangeHook, DocumentReadAuthzHook, DocumentWriteAuthzHook, RTCIceServer,
        RxWebRTCReplicationPool, SignalingClient, WebRTCConnectionHandler,
        WebRTCPeerSessionValidator, WebRTCRsConfig, WebRTCRsConnectionHandler,
    },
    rx_collection::RxCollection,
    rx_database::RxDatabase,
};
use std::{io, panic::AssertUnwindSafe, sync::Arc, time::Duration};

pub type NativePool = Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>;
pub use rxdb::plugins::replication_webrtc::NativePeerRole;
pub type SignalingUrls = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

/// One current saved-target identity. Pins come from authenticated enrollment,
/// never signaling or the proof response. The credential callback must retain
/// that same target/account binding and recheck it when invoked after proof.
pub struct NativeSessionTarget {
    pub public_identity: String,
    pub instance_id: String,
    pub credentials: rxdb::plugins::replication_webrtc::LocalSessionProvider<
        rxdb::plugins::replication_webrtc::WebRTCRsConnection,
    >,
}

/// Resolve public pins without reading a token or signing a remote nonce.
/// The native lifecycle invokes credentials only after channel-bound proof.
pub type NativeSessionTargetProvider = Arc<
    dyn Fn(
            rxdb::plugins::replication_webrtc::WebRTCRsConnection,
        ) -> futures_util::future::BoxFuture<
            'static,
            Result<NativeSessionTarget, rxdb::rx_error::RxError>,
        > + Send
        + Sync,
>;

/// Admission and business authorization remain the responsibility of the host.
/// Neither a signaling role nor an execution vote grants access to collections.
pub struct NativeAdmission {
    pub peer: Arc<dyn Fn(&String) -> bool + Send + Sync>,
    pub session: WebRTCPeerSessionValidator,
    pub collection_read: Option<CollectionAuthzHook>,
    pub collection_write: Option<CollectionAuthzHook>,
    pub document_read: Option<DocumentReadAuthzHook>,
    pub document_write: Option<DocumentWriteAuthzHook>,
    pub eager_pull: Option<CollectionEagerPullHook>,
    pub live_change: Option<CollectionLiveChangeHook>,
}

pub struct NativeSyncOptions {
    pub peer_role: NativePeerRole,
    /// Resolve the trusted target first; the native core gates credentials on
    /// its fresh signed proof. Execution votes cannot supply a data identity.
    pub local_session_provider: Option<NativeSessionTargetProvider>,
    /// Host-owned identity/persistence, independent from the replicated set.
    pub database: Arc<RxDatabase>,
    pub collections: Vec<Arc<RxCollection>>,
    /// Called again by the existing signaling reconnect supervisor.
    pub signaling_urls: SignalingUrls,
    pub room: String,
    pub peer_session_id: String,
    pub ice_servers: Vec<RTCIceServer>,
    pub admission: NativeAdmission,
    pub bringup_timeout: Duration,
}

/// Owns the transport, not the host database, projections or command workers.
/// Explicit shutdown is awaited before the host closes its database. Drop is a
/// cancellation/unwind backstop and schedules the same idempotent cleanup.
pub struct NativeSyncSession {
    resources: Resources,
    room: String,
    data_client: bool,
}

#[derive(Default)]
struct Resources {
    execution: Option<ExecutionAttachment>,
    signaling: Option<Arc<SignalingClient>>,
    handler: Option<Arc<WebRTCRsConnectionHandler>>,
    pool: Option<NativePool>,
}
enum ExecutionAttachment {
    Voter(Arc<crate::native_execution::NativeExecutionGroup>),
    Worker(Arc<crate::native_execution::NativeExecutionWorker>),
}
impl ExecutionAttachment {
    async fn shutdown(&self) -> io::Result<()> {
        match self {
            Self::Voter(host) => host.shutdown().await,
            Self::Worker(host) => host.shutdown().await,
        }
    }
}
impl Resources {
    async fn close(&self) {
        if let Some(execution) = &self.execution {
            let _ = execution.shutdown().await;
        }
        if let Some(pool) = &self.pool {
            pool.cancel().await;
        } else if let Some(handler) = &self.handler {
            let _ = handler.close().await;
        } else if let Some(signaling) = &self.signaling {
            signaling.close().await;
        }
    }
    fn disarm(&mut self) {
        self.execution = None;
        self.pool = None;
        self.handler = None;
        self.signaling = None;
    }
}
impl Drop for Resources {
    fn drop(&mut self) {
        if self.signaling.is_none() && self.handler.is_none() && self.pool.is_none() {
            return;
        }
        let signaling = self.signaling.take();
        let execution = self.execution.take();
        let handler = self.handler.take();
        let pool = self.pool.take();
        // Dropping outside a runtime cannot drive asynchronous IO. During a
        // runtime shutdown Tokio also destroys its tasks; no new runtime is made.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Some(execution) = execution {
                    let _ = execution.shutdown().await;
                }
                if let Some(pool) = pool {
                    pool.cancel().await;
                } else if let Some(handler) = handler {
                    let _ = handler.close().await;
                } else if let Some(signaling) = signaling {
                    signaling.close().await;
                }
            });
        }
    }
}

impl NativeSyncSession {
    pub async fn start(options: NativeSyncOptions) -> io::Result<Self> {
        Self::start_with_pool_setup(options, |_| Ok(())).await
    }

    /// Start a query-only consumer on an existing browser-admitted data room.
    /// The caller retains the database and credential owner. No execution
    /// attachment or replicated collection is installed by this mode.
    pub async fn start_data_client(options: NativeSyncOptions) -> io::Result<Self> {
        if options.local_session_provider.is_none()
            || !options.collections.is_empty()
            || !options.database.collections.lock().is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "data client requires deferred credentials and query-only storage",
            ));
        }
        Self::start_mode(options, |_| Ok(()), true).await
    }

    /// Install host-owned request handlers and file sources before advertising
    /// this peer. Setup runs once, inside the supervised bring-up boundary;
    /// rejection or panic closes the transport without joining the room.
    /// The callback must only register resources, never block or start workers.
    pub async fn start_with_pool_setup<F>(options: NativeSyncOptions, setup: F) -> io::Result<Self>
    where
        F: FnOnce(&NativePool) -> Result<(), rxdb::rx_error::RxError> + Send,
    {
        Self::start_mode(options, setup, false).await
    }

    async fn start_mode<F>(
        options: NativeSyncOptions,
        setup: F,
        data_client: bool,
    ) -> io::Result<Self>
    where
        F: FnOnce(&NativePool) -> Result<(), rxdb::rx_error::RxError> + Send,
    {
        if options.room.trim().is_empty()
            || options.peer_session_id.trim().is_empty()
            || options.bringup_timeout.is_zero()
            || options
                .collections
                .iter()
                .any(|c| !Arc::ptr_eq(&c.database, &options.database))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "native sync requires a room, session, deadline and one database",
            ));
        }
        let mut resources = Resources::default();
        let room = options.room.clone();
        let timeout = options.bringup_timeout;
        let result = tokio::time::timeout(
            timeout,
            AssertUnwindSafe(async {
                let provider = options.signaling_urls;
                let signaling =
                    SignalingClient::connect_with_url_list_provider(move || provider()).await?;
                // Own the signaling supervisor BEFORE awaiting room admission. A
                // rejected or cancelled join must not leave it reconnecting forever.
                resources.signaling = Some(signaling.clone());
                let mut config = WebRTCRsConfig::new(signaling.clone(), options.room.clone());
                config.peer_role = options.peer_role;
                config.data_client = data_client;
                if !options.ice_servers.is_empty() {
                    config.ice_servers = options.ice_servers;
                }
                let handler = WebRTCRsConnectionHandler::prepare_with_signaling(config).await?;
                resources.handler = Some(handler.clone());
                let admission = options.admission;
                let local_peer_gate = admission.peer.clone();
                handler.set_collection_authz(admission.collection_read);
                handler.set_collection_write_authz(admission.collection_write);
                handler.set_document_read_authz(admission.document_read);
                handler.set_document_write_authz(admission.document_write);
                handler.set_collection_eager_pull(admission.eager_pull);
                handler.set_collection_live_change(admission.live_change);
                // Preserve the existing 20/20 batch sizes and 5-second retry tuning.
                resources.pool = Some(
                    replicate_web_rtc_multi_with_validators(
                        options.database,
                        options.collections,
                        handler,
                        Some(Arc::new(move |connection| {
                            (admission.peer)(&connection.peer_id().to_owned())
                        })),
                        Some(admission.session),
                        Some(options.room.clone()),
                        Some(Arc::from(options.peer_session_id)),
                    )
                    .await?,
                );
                let pool = resources.pool.as_ref().expect("prepared native pool");
                install_target_provider(pool, options.local_session_provider, local_peer_gate);
                setup(pool)?;
                // Only advertise this peer after every pool request/connect
                // subscriber and host handler exists. Peers may offer on join.
                signaling.join(options.room).await?;
                Ok::<(), rxdb::rx_error::RxError>(())
            })
            .catch_unwind(),
        )
        .await;
        let failure = match result {
            Ok(Ok(Ok(()))) => {
                return Ok(Self {
                    resources,
                    room,
                    data_client,
                })
            }
            Ok(Ok(Err(error))) => io::Error::other(format!("native sync bring-up failed: {error}")),
            Ok(Err(_)) => io::Error::other("native sync bring-up panicked"),
            Err(_) => io::Error::new(
                io::ErrorKind::TimedOut,
                format!("native sync bring-up timed out after {timeout:?}"),
            ),
        };
        resources.close().await;
        resources.disarm();
        Err(failure)
    }

    pub fn pool(&self) -> &NativePool {
        self.resources
            .pool
            .as_ref()
            .expect("a started native session owns its pool")
    }

    /// Offer to a current, browser-admitted data route using existing WebRTC.
    /// This is transport setup only. It never issues a ready user-data handle.
    /// The host owns bounded discovery/retry and must await session shutdown.
    pub async fn connect_data_peer(&self, route: String) -> io::Result<()> {
        if !self.data_client {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "data connections require a data-client session",
            ));
        }
        self.pool()
            .connection_handler
            .connect_data_peer(route)
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "data route is unavailable or has incompatible admission",
                )
            })
    }

    /// Read a bounded page through the already admitted data connection. The
    /// host still owns target-instance authentication and credential selection.
    pub async fn query_page(
        &self,
        connection: rxdb::plugins::replication_webrtc::WebRTCRsConnection,
        request: rxdb::plugins::replication_webrtc::query_fetch_handler::QueryFetchRequest,
    ) -> Result<
        rxdb::plugins::replication_webrtc::query_fetch_client::QueryPage,
        rxdb::rx_error::RxError,
    > {
        rxdb::plugins::replication_webrtc::query_fetch_client::fetch_query_page(
            self.pool().clone(),
            connection,
            request,
        )
        .await
    }

    /// Read a nonce-bound source proof from a current admitted DataChannel.
    /// The key and instance pin must come from trusted host enrollment, not
    /// signaling or the reply. This does not install credentials, authenticate
    /// a selected user, authorize a collection or issue a ready data session.
    pub async fn peer_identity_proof(
        &self,
        connection: rxdb::plugins::replication_webrtc::WebRTCRsConnection,
        expected_key: &str,
        expected_instance: &str,
    ) -> io::Result<crate::business_data_contract::NativeBusinessDataPeerIdentity> {
        request_identity_proof(
            self.pool(),
            connection,
            expected_key,
            expected_instance,
            true,
        )
        .await
    }

    fn ensure_attachable(&self) -> io::Result<()> {
        if self.data_client
            || self.resources.execution.is_some()
            || self
                .pool()
                .canceled
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "native session already owns an execution group or is stopped",
            ));
        }
        Ok(())
    }

    async fn accept_execution(
        &mut self,
        result: io::Result<ExecutionAttachment>,
    ) -> io::Result<()> {
        match result {
            Ok(host) => {
                self.resources.execution = Some(host);
                Ok(())
            }
            Err(error) => {
                self.resources.close().await;
                Err(error)
            }
        }
    }

    /// Attach exactly one confirmed authority group. Admission hooks are never
    /// replaced or relaxed by execution membership.
    pub async fn attach_execution(
        &mut self,
        options: crate::native_execution::ExecutionGroupOptions,
        key: Arc<crate::authority::auth::SigningIdentity>,
    ) -> io::Result<&Arc<crate::native_execution::NativeExecutionGroup>> {
        self.ensure_attachable()?;
        let result = crate::native_execution::NativeExecutionGroup::attach(
            self.pool(),
            self.resources
                .signaling
                .as_ref()
                .expect("started signaling"),
            &self.room,
            options,
            key,
        )
        .await
        .map(|host| ExecutionAttachment::Voter(Arc::new(host)));
        self.accept_execution(result).await?;
        match self.resources.execution.as_ref() {
            Some(ExecutionAttachment::Voter(host)) => Ok(host),
            _ => unreachable!("attached voter"),
        }
    }

    /// Attach a nonvoting worker to the same supervised IPC/transport lifecycle.
    /// The member record pins identity; remote quorum still authorizes every job.
    pub async fn attach_worker(
        &mut self,
        options: crate::native_execution::WorkerExecutionOptions,
        key: Arc<crate::authority::auth::SigningIdentity>,
    ) -> io::Result<&Arc<crate::native_execution::NativeExecutionWorker>> {
        self.ensure_attachable()?;
        let result = crate::native_execution::NativeExecutionWorker::attach_worker(
            self.pool(),
            self.resources
                .signaling
                .as_ref()
                .expect("started signaling"),
            &self.room,
            options,
            key,
        )
        .await
        .map(|host| ExecutionAttachment::Worker(Arc::new(host)));
        self.accept_execution(result).await?;
        match self.resources.execution.as_ref() {
            Some(ExecutionAttachment::Worker(host)) => Ok(host),
            _ => unreachable!("attached worker"),
        }
    }

    pub async fn shutdown(&self) {
        self.resources.close().await;
    }
}

fn install_target_provider(
    pool: &NativePool,
    provider: Option<NativeSessionTargetProvider>,
    peer_gate: Arc<dyn Fn(&String) -> bool + Send + Sync>,
) {
    use rxdb::plugins::replication_webrtc::{LocalSessionProvider, WebRTCRsConnection};
    let weak_pool = Arc::downgrade(pool);
    let wrapped: Option<LocalSessionProvider<WebRTCRsConnection>> = provider.map(|resolve| {
        Arc::new(move |connection: WebRTCRsConnection, nonce: Option<String>| {
            let resolve = resolve.clone();
            let weak_pool = weak_pool.clone();
            let peer_gate = peer_gate.clone();
            Box::pin(async move {
                let failure = || rxdb::rx_error::new_rx_error("RC_WEBRTC_PEER", Some(serde_json::json!({
                    "code": "native_target_identity_unavailable",
                    "message": "trusted target identity or current credentials unavailable"
                })));
                let pool = weak_pool.upgrade().ok_or_else(failure)?;
                let allowed = || !pool.canceled.load(std::sync::atomic::Ordering::SeqCst)
                    && pool.connection_handler.is_peer_current(&connection)
                    && peer_gate(&connection.peer_id().to_owned());
                if !allowed() { return Err(failure()); }
                let target = resolve(connection.clone()).await.map_err(|_| failure())?;
                if !allowed() { return Err(failure()); }
                request_identity_proof(&pool, connection.clone(), &target.public_identity,
                    &target.instance_id, false).await.map_err(|_| failure())?;
                if !allowed() { return Err(failure()); }
                // Only this point may access a bearer or sign the remote nonce.
                // The host callback must also check its current account epoch.
                let credentials = (target.credentials)(connection.clone(), nonce)
                    .await.map_err(|_| failure())?;
                if !allowed() { return Err(failure()); }
                Ok(credentials)
            }) as futures_util::future::BoxFuture<'static, _>
        }) as LocalSessionProvider<WebRTCRsConnection>
    });
    pool.connection_handler.set_local_session_provider(wrapped);
}

async fn request_identity_proof(
    pool: &NativePool,
    connection: rxdb::plugins::replication_webrtc::WebRTCRsConnection,
    expected_key: &str,
    expected_instance: &str,
    require_ready: bool,
) -> io::Result<crate::business_data_contract::NativeBusinessDataPeerIdentity> {
    use crate::{business_data_contract::*, business_data_identity::*};
    use rxdb::plugins::replication_webrtc::{send_message_and_await_answer, WebRTCMessage};
    crate::authority::auth::public_key(expected_key)?;
    let failure = || {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "BusinessData peer identity unavailable or invalid",
        )
    };
    let current = || {
        !pool.canceled.load(std::sync::atomic::Ordering::SeqCst)
            && pool.connection_handler.is_peer_current(&connection)
            && (!require_ready || pool.is_peer_ready_for_control(&connection))
    };
    if !current() {
        return Err(failure());
    }
    let challenge = fresh_challenge()?;
    let request = NativeBusinessDataIdentityRequest {
        version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
        challenge: challenge.clone(),
    };
    // SDP access can await transport locks too. Both channel lookup and
    // wire exchange belong to one deadline and the pool cancellation scope.
    let exchange = async {
        let channel_binding = pool
            .connection_handler
            .channel_binding(&connection)
            .await
            .map_err(|_| failure())?;
        let response = send_message_and_await_answer(
            pool.connection_handler.clone(),
            connection.clone(),
            WebRTCMessage {
                id: format!("business-identity-{challenge}"),
                method: CTOX_BUSINESS_DATA_IDENTITY_METHOD.into(),
                params: vec![serde_json::to_value(request).map_err(|_| failure())?],
                collection: None,
            },
        )
        .await
        .map_err(|_| failure())?;
        Ok::<_, io::Error>((response, channel_binding))
    };
    let (response, channel_binding) = tokio::select! {
        biased;
        _ = pool.cancelled() => return Err(failure()),
        response = tokio::time::timeout(Duration::from_secs(10), exchange) => {
            response.map_err(|_| failure())??
        }
    };
    if !current()
        || response.error.is_some()
        || serde_json::to_vec(&response.result).map_or(true, |bytes| bytes.len() > 4096)
    {
        return Err(failure());
    }
    let proof = serde_json::from_value(response.result).map_err(|_| failure())?;
    verify_peer_identity(
        &proof,
        expected_key,
        expected_instance,
        &challenge,
        &channel_binding,
    )?;
    Ok(proof)
}
