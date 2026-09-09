use super::*;
use ctox_sync::{
    contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResponse, SyncIpcResult},
    host_config::HostMember,
    ipc::{IPC_MAX_FRAME_BYTES, IPC_PROTOCOL_VERSION},
    native::{NativeAdmission, NativePeerRole, NativeSyncOptions},
};
use rxdb::{
    plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation,
    rx_database::{create_rx_database, RxDatabaseCreator},
    storage::sqlite::{index_mod::get_rx_storage_sqlite, types::RxStorageSqliteSettings},
    types::{HashFunction, HashOutput},
};
use std::collections::HashMap;
struct Hash;
impl HashFunction for Hash {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
    }
}

pub(super) fn run<S, F>(root: &Path, stop: S, started: F) -> Result<()>
where
    S: std::future::Future<Output = io::Result<()>>,
    F: FnOnce(HostStarted) -> Result<()>,
{
    // This process lease precedes opening Raft/RxDB and outlives the Tokio
    // runtime, including blocking storage work during unwind or shutdown.
    let _lease = HostDirectoryLock::acquire(&directory(root))?;
    let config = configuration(root)?;
    let key = key(root)?;
    config.validate_key(&key)?;
    let initial_transport = transport(root, &config)?;
    let storage = config.directory(root)?;
    std::fs::create_dir_all(&storage)?;
    // Only IPC uses an ephemeral OS runtime directory. Long installation paths
    // do not dictate the Unix socket address or cause a network fallback.
    let ipc = ctox_sync::local_host::private_ipc_directory()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let mut descriptor = None;
    runtime.block_on(async {
        let database = create_rx_database(RxDatabaseCreator {
            name: format!("ctox-execution-{}", config.node_id()),
            storage: get_rx_storage_sqlite(RxStorageSqliteSettings {
                database_path: storage.join("native.sqlite3"),
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
        .context("cannot open native Sync database")?;
        let provider_root = root.to_path_buf();
        let provider_config = config.clone();
        let options = NativeSyncOptions {
            local_session_provider: None,
            peer_role: match config.local {
                HostMember::Voter { .. } => NativePeerRole::CtoxInstance,
                HostMember::Worker { .. } => NativePeerRole::WorkjetExecutor,
            },
            database: database.clone(),
            collections: Vec::new(),
            signaling_urls: Arc::new(move || match transport(&provider_root, &provider_config) {
                Ok(value) => value.signaling_urls,
                Err(_) => {
                    eprintln!("native Sync signaling credentials are unavailable or invalid");
                    Vec::new()
                }
            }),
            room: config.room(),
            peer_session_id: key.public_identity(),
            ice_servers: initial_transport.native_ice_servers(),
            bringup_timeout: Duration::from_secs(15),
            admission: NativeAdmission {
                // These admit control candidates only. Every authority exchange
                // independently checks its pinned key and current membership.
                peer: Arc::new(|_| true),
                session: Arc::new(|payload, _| {
                    let role = payload
                        .pointer("/peerSession/role")
                        .and_then(serde_json::Value::as_str);
                    let identity = payload
                        .pointer("/peerSession/sessionId")
                        .and_then(serde_json::Value::as_str);
                    if matches!(role, Some("ctox_instance" | "workjet_executor"))
                        && identity.is_some_and(|value| {
                            value.len() == 72
                                && value.starts_with("ed25519:")
                                && value[8..]
                                    .bytes()
                                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                        })
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
        };
        let result =
            ctox_sync::host_runtime::run(&config, root, ipc.path(), key, options, stop, |ready| {
                descriptor =
                    Some(DescriptorGuard::publish(root, &ready).map_err(io::Error::other)?);
                started(ready).map_err(io::Error::other)
            })
            .await
            .map_err(|error| anyhow::anyhow!("native Sync host failed ({:?})", error.kind()));
        let closed = database
            .close()
            .await
            .context("cannot close native Sync database");
        result.and(closed)
    })
}

pub(super) fn status(root: &Path) -> Result<()> {
    let Some(config) = load_config(root)? else {
        return print(serde_json::json!({"configured": false, "listener":"inactive"}));
    };
    let descriptor = std::fs::File::open(directory(root).join("listener.json"))
        .ok()
        .and_then(|file| serde_json::from_reader::<_, Descriptor>(file.take(16384)).ok())
        .filter(|value| {
            value.version == 1
                && value.node_id == config.node_id()
                && value.scope_id == config.scope_id
                && value.ipc_endpoint.is_absolute()
        });
    let endpoint = if let Some(descriptor) = descriptor {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let alive = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(2), hello(&descriptor)).await
        });
        if matches!(alive, Ok(Ok(()))) {
            Some(descriptor.ipc_endpoint)
        } else {
            None
        }
    } else {
        None
    };
    print(
        serde_json::json!({"configured": true, "nodeId":config.node_id(), "scopeId":config.scope_id,
        "identity":config.identity()?, "listener":if endpoint.is_some() {"active"} else {"inactive"}, "ipcEndpoint":endpoint}),
    )
}
async fn hello(descriptor: &Descriptor) -> io::Result<()> {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
    };
    let mut stream = UnixStream::connect(&descriptor.ipc_endpoint).await?;
    let request = SyncIpcRequest {
        version: IPC_PROTOCOL_VERSION,
        request_id: "host-status".into(),
        operation: SyncIpcOperation::Hello {},
    };
    let bytes = serde_json::to_vec(&request).map_err(io::Error::other)?;
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    let length = stream.read_u32().await? as usize;
    if length == 0 || length > IPC_MAX_FRAME_BYTES {
        return Err(io::Error::other("invalid local native Sync response size"));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    let response: SyncIpcResponse = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if response.version != IPC_PROTOCOL_VERSION || response.request_id != request.request_id {
        return Err(io::Error::other("incompatible local native Sync response"));
    }
    match response.result {
        SyncIpcResult::Ready {
            node_id,
            scope_id,
            protocol_version,
        } if node_id == descriptor.node_id
            && scope_id == descriptor.scope_id
            && protocol_version == IPC_PROTOCOL_VERSION =>
        {
            Ok(())
        }
        _ => Err(io::Error::other(
            "local native Sync listener does not match the configured host",
        )),
    }
}
