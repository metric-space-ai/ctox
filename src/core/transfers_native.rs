//! Daemon-owned query-only database and native transfer session lifetime.
use anyhow::{ensure, Context, Result};
use ctox_sync::native::{NativeAdmission, NativePeerRole, NativeSyncOptions};
use ctox_transfers::{DownloadRequest, PeerRangeSource};
use futures_util::future::BoxFuture;
use rxdb::{
    plugins::replication_webrtc::{webrtc_types::WebRTCPeerSessionValidation, RTCIceServer},
    rx_database::{create_rx_database, RxDatabase, RxDatabaseCreator},
    storage::sqlite::{index_mod::get_rx_storage_sqlite, types::RxStorageSqliteSettings},
    types::{HashFunction, HashOutput},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::Mutex, task::JoinHandle};

struct Hash;
impl HashFunction for Hash {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
    }
}

#[derive(Default)]
struct DatabaseState {
    closed: bool,
    opening: Option<JoinHandle<Result<Arc<RxDatabase>>>>,
    database: Option<Arc<RxDatabase>>,
}

impl DatabaseState {
    async fn finish_open(&mut self) -> Result<()> {
        if let Some(opening) = &mut self.opening {
            // A cancelled authorization must not detach database initialization.
            // Shutdown can still retrieve and close the resulting database.
            let result = opening.await;
            self.opening = None;
            self.database = Some(result.context("native transfer database startup failed")??);
        }
        Ok(())
    }
}

struct QueryDatabase {
    path: PathBuf,
    state: Mutex<DatabaseState>,
}

impl QueryDatabase {
    fn new(root: &Path) -> Arc<Self> {
        Arc::new(Self {
            path: root.join("runtime/transfers/native-peer.sqlite3"),
            state: Mutex::new(DatabaseState::default()),
        })
    }

    async fn get(&self) -> Result<Arc<RxDatabase>> {
        let mut state = self.state.lock().await;
        ensure!(!state.closed, "native transfer database is shut down");
        if state.database.is_none() && state.opening.is_none() {
            let path = self.path.clone();
            state.opening = Some(tokio::spawn(async move {
                std::fs::create_dir_all(path.parent().context("native database path missing")?)?;
                create_rx_database(RxDatabaseCreator {
                    name: format!("ctox-transfer-{}", uuid::Uuid::new_v4()),
                    storage: get_rx_storage_sqlite(RxStorageSqliteSettings {
                        database_path: path,
                    }),
                    multi_instance: false,
                    password: None,
                    hash_function: Arc::new(Hash),
                    options: HashMap::new(),
                    ignore_duplicate: false,
                    close_duplicates: false,
                    event_reduce: false,
                    allow_slow_count: false,
                })
                .await
                .context("cannot open native transfer database")
            }));
        }
        state.finish_open().await?;
        state
            .database
            .clone()
            .context("native transfer database unavailable")
    }

    async fn close(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.closed = true;
        state.finish_open().await?;
        if let Some(database) = &state.database {
            database
                .close()
                .await
                .context("cannot close native transfer database")?;
        }
        state.database = None;
        Ok(())
    }

    async fn options(&self) -> Result<NativeSyncOptions> {
        Ok(NativeSyncOptions {
            database: self.get().await?,
            collections: Vec::new(),
            local_session_provider: None,
            peer_role: NativePeerRole::WorkjetExecutor,
            peer_session_id: format!("ctox-transfer-{}", uuid::Uuid::new_v4()),
            // NativeTransferAccountHost supplies the authenticated remote route.
            room: String::new(),
            signaling_urls: Arc::new(Vec::new),
            ice_servers: vec![RTCIceServer::default()],
            bringup_timeout: Duration::from_secs(20),
            admission: NativeAdmission {
                peer: Arc::new(|_| true),
                // Candidate admission grants no authority: the resolver requires
                // channel-bound source proof before releasing credentials.
                session: Arc::new(|payload, _| {
                    if payload
                        .pointer("/peerSession/role")
                        .and_then(serde_json::Value::as_str)
                        == Some("ctox_instance")
                    {
                        WebRTCPeerSessionValidation::Accept
                    } else {
                        WebRTCPeerSessionValidation::Reject
                    }
                }),
                collection_read: Some(Arc::new(|_, _| false)),
                collection_write: Some(Arc::new(|_, _| false)),
                document_read: None,
                document_write: None,
                eager_pull: None,
                live_change: None,
            },
        })
    }
}

struct ManagedNativePeer {
    peer: Arc<dyn PeerRangeSource>,
    database: Arc<QueryDatabase>,
}

impl PeerRangeSource for ManagedNativePeer {
    fn authorize<'a>(&'a self, request: &'a DownloadRequest) -> BoxFuture<'a, Result<()>> {
        self.peer.authorize(request)
    }
    fn read_range<'a>(
        &'a self,
        request: &'a DownloadRequest,
        offset: u64,
        length: u64,
    ) -> BoxFuture<'a, Result<Vec<u8>>> {
        self.peer.read_range(request, offset, length)
    }
    fn shutdown(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move {
            self.peer.shutdown().await?;
            self.database.close().await
        })
    }
}

/// Initialization is lazy and occurs only inside the lease-owning daemon runtime.
/// HTTP-only operation neither opens this database nor starts a native session.
pub(crate) fn daemon_peer(root: &Path) -> Arc<dyn PeerRangeSource> {
    let database = QueryDatabase::new(root);
    let options_database = database.clone();
    let host = crate::native_transfer_accounts::NativeTransferAccountHost::new(
        root.to_path_buf(),
        Arc::new(move |_| {
            let database = options_database.clone();
            Box::pin(async move { database.options().await.map_err(std::io::Error::other) })
        }),
    );
    let provider_host = host.clone();
    let peer = Arc::new(
        crate::transfers_peer::NativeTransferPeerResolver::with_provider_lookup(
            host,
            Arc::new(move |target_id| {
                let host = provider_host.clone();
                Box::pin(async move { Ok(host.provider(target_id)) })
            }),
        ),
    );
    Arc::new(ManagedNativePeer { peer, database })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn query_database_is_lazy_shared_empty_and_cannot_reopen_after_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let owner = QueryDatabase::new(root.path());
        assert!(!owner.path.exists());
        let options = owner.options().await.unwrap();
        let same = owner.get().await.unwrap();
        assert!(Arc::ptr_eq(&options.database, &same));
        assert!(same.collections.lock().is_empty());
        assert!(options.collections.is_empty());
        assert!(options.local_session_provider.is_none());
        assert!(options.room.is_empty());
        assert!((options.signaling_urls)().is_empty());
        let next = owner.options().await.unwrap();
        assert_ne!(options.peer_session_id, next.peer_session_id);
        owner.close().await.unwrap();
        assert!(same.closed());
        assert!(owner.get().await.is_err());
        owner.close().await.unwrap();
    }

    #[tokio::test]
    async fn cancelled_database_wait_retains_startup_for_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let owner = QueryDatabase::new(root.path());
        let database = owner.get().await.unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        {
            let mut state = owner.state.lock().await;
            state.database = None;
            state.opening = Some(tokio::spawn(async move { Ok(receive.await?) }));
        }
        assert!(tokio::time::timeout(Duration::from_millis(10), owner.get())
            .await
            .is_err());
        send.send(database.clone())
            .unwrap_or_else(|_| panic!("database startup detached"));
        owner.close().await.unwrap();
        assert!(database.closed());
    }

    struct ClosingPeer(Arc<RxDatabase>);
    impl PeerRangeSource for ClosingPeer {
        fn authorize<'a>(&'a self, _: &'a DownloadRequest) -> BoxFuture<'a, Result<()>> {
            Box::pin(async { anyhow::bail!("unused") })
        }
        fn read_range<'a>(
            &'a self,
            _: &'a DownloadRequest,
            _: u64,
            _: u64,
        ) -> BoxFuture<'a, Result<Vec<u8>>> {
            Box::pin(async { anyhow::bail!("unused") })
        }
        fn shutdown(&self) -> BoxFuture<'_, Result<()>> {
            Box::pin(async move {
                ensure!(!self.0.closed(), "database closed before peer");
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn managed_shutdown_drains_peer_before_closing_database() {
        let root = tempfile::tempdir().unwrap();
        let database = QueryDatabase::new(root.path());
        let opened = database.get().await.unwrap();
        let managed = ManagedNativePeer {
            peer: Arc::new(ClosingPeer(opened.clone())),
            database,
        };
        managed.shutdown().await.unwrap();
        assert!(opened.closed());
    }
}
