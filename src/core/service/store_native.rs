//! The foreground owns the one instance identity and the actual native host.
//! No local fixture authority or replacement membership is created here.
use super::store_inputs::{NativeSessionInputs, StoreInputs};
use ctox_store::change_log::subscription::protocol::InstanceSigner;
use ctox_store::{ReadJob, StoreError};
use ctox_sync::{
    authority::auth::SigningIdentity,
    host_config::HostMember,
    host_transport::HostTransport,
    native::{NativeAdmission, NativePeerRole, NativeSyncOptions},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(super) struct InstanceSigning {
    identity: Arc<SigningIdentity>,
    instance_id: String,
    public: String,
    active: Arc<AtomicBool>,
}
impl InstanceSigning {
    pub fn new(
        identity: Arc<SigningIdentity>,
        instance_id: String,
        active: Arc<AtomicBool>,
    ) -> Self {
        Self {
            instance_id,
            public: identity.public_identity(),
            identity,
            active,
        }
    }
}
impl InstanceSigner for InstanceSigning {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
    fn key_id(&self) -> &str {
        &self.public
    }
    fn sign(&self, canonical: &[u8]) -> Result<Vec<u8>, String> {
        if !self.active.load(Ordering::Acquire) {
            return Err("instance signer is retired".into());
        }
        let encoded =
            ctox_sync::authority::auth::CanonicalInstancePacket::from_stream_bytes(canonical)
                .or_else(|_| {
                    ctox_sync::authority::auth::CanonicalInstancePacket::from_launch_bytes(
                        canonical,
                    )
                })
                .or_else(|_| {
                    ctox_sync::authority::auth::CanonicalInstancePacket::from_manifest_bytes(
                        canonical,
                    )
                })
                .or_else(|_| {
                    ctox_sync::authority::auth::CanonicalInstancePacket::from_scope_bytes(canonical)
                })
                .map_err(|_| "instance packet is not canonical".to_owned())?;
        Ok(self.identity.sign_instance_packet(&encoded))
    }
}

#[cfg(unix)]
pub(super) struct NativeSessionOwner {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    active: Arc<AtomicBool>,
}

#[cfg(unix)]
impl NativeSessionOwner {
    pub async fn start(
        inputs: Arc<StoreInputs>,
        secrets: &crate::secrets::SecretStore,
        identity: Arc<SigningIdentity>,
        signer: Arc<dyn InstanceSigner>,
        active: Arc<AtomicBool>,
    ) -> Result<Option<Self>, StoreError> {
        let runtime = inputs.databases.get("runtime").ok_or(StoreError::Closed)?;
        let config = runtime
            .readers
            .read_job(ReadJob::new(Default::default(), |conn| {
                ctox_sync::host_config::load(conn)
                    .map_err(|_| StoreError::Invalid("native host configuration is invalid".into()))
            }))
            .await?;
        let Some(config) = config else {
            // Enrollment is explicit. Missing voters never become synthetic
            // local voters or an always-allow authority just to activate apps.
            return Ok(None);
        };
        if !matches!(config.local, HostMember::Voter { .. }) {
            return Err(StoreError::Invalid(
                "instance daemon requires its enrolled voter role".into(),
            ));
        }
        config
            .validate_key(&identity)
            .map_err(|_| StoreError::Invalid("native host signing identity mismatch".into()))?;
        let transport_config = config.clone();
        let transport: HostTransport = secrets
            .with_current_values(
                vec![(
                    crate::sync_host::SIGNING_IDENTITY_SECRET_KEY.0.into(),
                    format!("transport:{}", config.scope_id),
                )],
                move |values, _| {
                    let encoded = std::str::from_utf8(values[0])
                        .map_err(|_| anyhow::anyhow!("invalid native transport encoding"))?;
                    Ok(HostTransport::parse(encoded, &transport_config)?)
                },
            )
            .await
            .map_err(|_| StoreError::Invalid("native host transport is unavailable".into()))?;
        let root = inputs.root.clone();
        let directory = config
            .directory(&root)
            .map_err(|_| StoreError::Invalid("native host directory is invalid".into()))?;
        let (lease, ipc) = tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::PermissionsExt;
            let parent = directory
                .parent()
                .ok_or_else(|| std::io::Error::other("native directory missing"))?;
            let lease = ctox_sync::local_host::HostDirectoryLock::acquire(parent)?;
            std::fs::create_dir_all(&directory)?;
            // IPC addresses must fit SUN_LEN independently of profile/cache
            // depth. This is a private, random, owned OS-temporary directory.
            let ipc = tempfile::Builder::new()
                .prefix("cs4-")
                .permissions(std::fs::Permissions::from_mode(0o700))
                .tempdir_in("/tmp")?;
            Ok::<_, std::io::Error>((lease, ipc))
        })
        .await
        .map_err(|_| StoreError::Panicked)??;
        let storage =
            super::store_rxdb::native_storage(inputs.sync.writer.db_id().path().to_owned()).await?;
        let database =
            rxdb::rx_database::create_rx_database(rxdb::rx_database::RxDatabaseCreator {
                name: format!("ctox-execution-{}", config.node_id()),
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
            .map_err(|_| StoreError::Invalid("native control database attachment failed".into()))?;
        let options = NativeSyncOptions {
            local_session_provider: None,
            peer_role: NativePeerRole::CtoxInstance,
            database: database.clone(),
            collections: Vec::new(),
            room: config.room(),
            peer_session_id: identity.public_identity(),
            ice_servers: transport.native_ice_servers(),
            // The encrypted transport is pinned for this host session. No
            // SQLite/secret work runs in this synchronous runtime callback.
            signaling_urls: Arc::new(move || transport.signaling_urls.clone()),
            bringup_timeout: std::time::Duration::from_secs(15),
            admission: NativeAdmission {
                peer: Arc::new(|_| true),
                session: Arc::new(|payload, _| {
                    use rxdb::plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation as Decision;
                    let role = payload
                        .pointer("/peerSession/role")
                        .and_then(serde_json::Value::as_str);
                    let identity = payload
                        .pointer("/peerSession/sessionId")
                        .and_then(serde_json::Value::as_str);
                    if matches!(role, Some("ctox_instance" | "workjet_executor"))
                        && identity.is_some_and(|value| {
                            ctox_sync::authority::auth::validate_public_identity(value).is_ok()
                        })
                    {
                        Decision::Accept
                    } else {
                        Decision::Reject
                    }
                }),
                collection_read: Some(Arc::new(|_, _| false)),
                collection_write: Some(Arc::new(|_, _| false)),
                document_read: None,
                document_write: None,
                eager_pull: None,
                live_change: None,
            },
        };
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (ready, started) = tokio::sync::oneshot::channel();
        let live = active.clone();
        let task = tokio::spawn(async move {
            let _lease = lease;
            let _ipc = ipc;
            let callback_live = live.clone();
            let result = ctox_sync::host_runtime::run_with_native_session(
                &config,
                &root,
                _ipc.path(),
                identity,
                options,
                async { stopped.await.map_err(std::io::Error::other) },
                move |_, authority, _session| {
                    inputs
                        .publish_native_session(NativeSessionInputs {
                            instance_signer: signer,
                            execution_authority: authority,
                            active: callback_live,
                        })
                        .map_err(std::io::Error::other)?;
                    ready
                        .send(())
                        .map_err(|_| std::io::Error::other("native start cancelled"))
                },
            )
            .await;
            live.store(false, Ordering::Release);
            let closed = database
                .close()
                .await
                .map_err(|_| std::io::Error::other("native control database close failed"));
            result.and(closed)
        });
        let mut owner = Self {
            stop: Some(stop),
            task: Some(task),
            active,
        };
        match tokio::time::timeout(std::time::Duration::from_secs(20), started).await {
            Ok(Ok(())) => Ok(Some(owner)),
            _ => {
                let _ = owner.shutdown().await;
                Err(StoreError::Invalid("native host did not attach".into()))
            }
        }
    }

    pub async fn wait_stopped(&mut self) -> std::io::Result<()> {
        let Some(task) = self.task.as_mut() else {
            return std::future::pending().await;
        };
        // Keep the handle in self while pending: the foreground select may
        // cancel this wait on SIGTERM and shutdown must still await the task.
        let result = task.await;
        self.task.take();
        let result = result.map_err(|_| std::io::Error::other("native host task failed"))?;
        result.map_err(|_| std::io::Error::other("native host session failed"))?;
        Err(std::io::Error::other("native host stopped unexpectedly"))
    }
    pub async fn shutdown(&mut self) -> Result<(), StoreError> {
        self.active.store(false, Ordering::Release);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.await
                .map_err(|_| StoreError::Panicked)?
                .map_err(|_| StoreError::Invalid("native host shutdown failed".into()))?;
        }
        Ok(())
    }
}
#[cfg(unix)]
impl Drop for NativeSessionOwner {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        // The task retains its database, IPC directory and host lease until
        // run_with_native_session has completed the real session shutdown.
    }
}
#[cfg(unix)]
struct Hash;
#[cfg(unix)]
impl rxdb::types::HashFunction for Hash {
    fn hash<'a>(&'a self, input: String) -> rxdb::types::HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
    }
}

#[cfg(all(test, unix))]
#[path = "store_native_tests.rs"]
mod tests;
