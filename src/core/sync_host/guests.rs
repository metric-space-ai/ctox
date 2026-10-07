//! Native operator enrollment into the live host-owned guest registry.
//! This private IPC is provisioning only: no checkpoints, frames or business
//! records are transported here. The actual native Sync peer owns those bytes.
use super::*;
use crate::business_os::NativeGuestRegistry;
use ctox_sync::native::NativeSyncSession;
use rusqlite::OptionalExtension;
use std::{
    collections::BTreeSet,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

const CONFIG_KEY: &str = "native_guest_host_config";
const MAX_BYTES: usize = 16 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Configuration {
    version: u32,
    computer_id: String,
    required_capabilities: BTreeSet<String>,
    /// Explicit privileged local-operator grants; enrollment cannot mint them.
    #[serde(default)]
    provider_assignments: Vec<crate::business_os::ProviderAssignmentInput>,
}
impl Configuration {
    fn validate(&self) -> Result<()> {
        fn valid(value: &str) -> bool {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
        }
        anyhow::ensure!(
            self.version == 1
                && valid(&self.computer_id)
                && !self.required_capabilities.is_empty()
                && self.required_capabilities.len() <= 32
                && self.required_capabilities.iter().all(|c| valid(c))
                && self.provider_assignments.len() <= 64,
            "invalid native guest host configuration"
        );
        Ok(())
    }
}
pub(super) fn configure(root: &Path, input: &str) -> Result<()> {
    let value: Configuration = serde_json::from_str(input)?;
    value.validate()?;
    let _lease = HostDirectoryLock::acquire(&directory(root))?;
    configuration(root)?.validate_key(key(root)?.as_ref())?;
    crate::inference::runtime_env::set_runtime_env_value(
        root,
        CONFIG_KEY,
        &serde_json::to_string(&value)?,
    )?;
    crate::business_os::configure_provider_assignments(
        root,
        &value.computer_id,
        &value.provider_assignments,
    )
}
fn load(root: &Path) -> Result<Option<Configuration>> {
    // Distinguish unavailable storage from disabled configuration. No creation,
    // cache or runtime/environment merge can make this decision permissive.
    let conn = rusqlite::Connection::open_with_flags(
        crate::inference::runtime_env::runtime_config_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='runtime_env_kv')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let value: Option<String> = conn
        .query_row(
            "SELECT env_value FROM runtime_env_kv WHERE env_key = ?1",
            [CONFIG_KEY],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|value| {
            let config: Configuration = serde_json::from_str(&value)?;
            config.validate()?;
            Ok(config)
        })
        .transpose()
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnrollRequest {
    session_token: String,
    project_id: String,
    thread_id: String,
    worker_profile_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum EnrollResponse {
    Enrolled {
        guest_id: String,
        controller_id: String,
        generation: u64,
        scope_id: String,
    },
    Denied,
}
pub(super) struct Host {
    pub(super) registry: Arc<NativeGuestRegistry>,
    pub(super) endpoint: PathBuf,
    task: tokio::task::JoinHandle<()>,
}
impl Host {
    pub(super) fn start(
        root: &Path,
        ipc: &Path,
        authority: Arc<dyn ctox_sync::authority::client::ExecutionAuthority>,
        peer: &NativeSyncSession,
    ) -> Result<Option<Self>> {
        let Some(config) = load(root)? else {
            return Ok(None);
        };
        let registry = NativeGuestRegistry::new(root, authority, config.required_capabilities)?;
        registry.attach_frame_transport(peer)?;
        let imports = directory(root).join("guests");
        match std::fs::DirBuilder::new().mode(0o700).create(&imports) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        let metadata = std::fs::symlink_metadata(&imports)?;
        anyhow::ensure!(
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == unsafe { libc::geteuid() }
                && metadata.mode() & 0o077 == 0
                && std::fs::canonicalize(&imports)? == imports,
            "native guest import root is not private"
        );
        let endpoint = ipc.join("guest-control.sock");
        let listener = UnixListener::bind(&endpoint)?;
        let retained = registry.clone();
        let pool = Arc::downgrade(peer.pool());
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let Some(pool) = pool.upgrade() else {
                    break;
                };
                if pool.canceled.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let _ = tokio::time::timeout(
                    IO_TIMEOUT,
                    serve_connection(stream, &retained, &config.computer_id, &imports, || {
                        !pool.canceled.load(std::sync::atomic::Ordering::SeqCst)
                    }),
                )
                .await;
            }
        });
        Ok(Some(Self {
            registry,
            endpoint,
            task,
        }))
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.endpoint);
    }
}
async fn read<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    let size = stream.read_u32().await? as usize;
    anyhow::ensure!(
        size > 0 && size <= MAX_BYTES,
        "invalid native control frame"
    );
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
async fn write(stream: &mut UnixStream, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    anyhow::ensure!(
        bytes.len() <= MAX_BYTES,
        "native control response too large"
    );
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}
pub(crate) async fn serve_connection(
    mut stream: UnixStream,
    registry: &NativeGuestRegistry,
    computer_id: &str,
    imports: &Path,
    current: impl Fn() -> bool,
) -> Result<()> {
    anyhow::ensure!(
        stream.peer_cred()?.uid() == unsafe { libc::geteuid() },
        "foreign native control peer"
    );
    let request: EnrollRequest = read(&mut stream).await?;
    anyhow::ensure!(current(), "native control host stopped");
    let parent = tempfile::Builder::new()
        .prefix("enrollment-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(imports)?;
    let response = match registry.enroll_from_cookie_token(
        &request.session_token,
        computer_id,
        &request.project_id,
        &request.thread_id,
        &request.worker_profile_id,
        parent.path(),
    ) {
        Ok(assignment) => {
            // Retain the exact native-owned import parent with its controller.
            if assignment.destination.import_parent == parent.path() {
                parent.keep();
            }
            EnrollResponse::Enrolled {
                guest_id: assignment.destination.guest_id,
                controller_id: assignment.destination.controller_id,
                generation: assignment.destination.controller_generation,
                scope_id: assignment.scope_id,
            }
        }
        // Never echo credentials, payloads or lower-level policy diagnostics.
        Err(_) => EnrollResponse::Denied,
    };
    anyhow::ensure!(current(), "native control host stopped before response");
    write(&mut stream, &response).await
}
pub(super) fn enroll(root: &Path, ids: &[&str], token: &str) -> Result<()> {
    anyhow::ensure!(
        ids.len() == 3 && !token.trim().is_empty() && token.trim().len() <= 512,
        "native enrollment requires three canonical IDs and an opaque session on stdin"
    );
    let config = configuration(root)?;
    let descriptor: Descriptor = serde_json::from_reader(
        std::fs::File::open(directory(root).join("listener.json"))?.take(16384),
    )?;
    anyhow::ensure!(
        descriptor.version == 1
            && descriptor.node_id == config.node_id()
            && descriptor.scope_id == config.scope_id,
        "native host descriptor mismatch"
    );
    let endpoint = descriptor
        .guest_endpoint
        .context("native guest enrollment is not configured")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let response: EnrollResponse = runtime.block_on(async {
        tokio::time::timeout(IO_TIMEOUT, async {
            let mut stream = UnixStream::connect(endpoint).await?;
            anyhow::ensure!(
                stream.peer_cred()?.uid() == unsafe { libc::geteuid() },
                "foreign native control host"
            );
            write(
                &mut stream,
                &EnrollRequest {
                    session_token: token.trim().into(),
                    project_id: ids[0].into(),
                    thread_id: ids[1].into(),
                    worker_profile_id: ids[2].into(),
                },
            )
            .await?;
            read(&mut stream).await
        })
        .await
        .context("native enrollment control timed out")?
    })?;
    match response {
        EnrollResponse::Denied => anyhow::bail!("native guest enrollment denied"),
        response => print(response),
    }
}
