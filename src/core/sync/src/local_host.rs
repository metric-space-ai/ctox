//! Host-owned Unix IPC lifecycle. No network listener and no stale-socket guessing.
#[cfg(test)]
#[path = "local_host_tests.rs"]
mod tests;
use crate::{
    authority::client::ExecutionAuthority,
    ipc::{AuthorityIpc, IpcService},
};
use fs2::FileExt;
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::UnixListener as StdListener,
    },
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    net::UnixListener,
    sync::oneshot,
    task::{JoinHandle, JoinSet},
};

const MAX_CONNECTIONS: usize = 32;

/// Create the host-owned runtime directory with private rights at creation.
/// tempfile's directory default follows umask and is commonly world-readable.
pub fn private_ipc_directory() -> io::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("cs-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
}

pub struct LocalIpcHost {
    endpoint: PathBuf,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<io::Result<()>>>,
}
impl LocalIpcHost {
    /// `directory` is a dedicated private runtime directory, not a workspace.
    /// The host does not own the supplied authority or its WebRTC pool lifecycle.
    pub async fn start_authority(
        directory: PathBuf,
        node: Arc<dyn ExecutionAuthority>,
    ) -> io::Result<Self> {
        Self::start(directory, Arc::new(AuthorityIpc::new(node))).await
    }

    /// Bind one private endpoint for a host-owned service. This does not create
    /// or imply execution membership; all connections share the same ACL,
    /// concurrency budget and supervised shutdown as the authority adapter.
    pub async fn start(directory: PathBuf, service: Arc<dyn IpcService>) -> io::Result<Self> {
        let bound = tokio::task::spawn_blocking(move || BoundSocket::bind(&directory))
            .await
            .map_err(io::Error::other)??;
        let endpoint = bound.path.clone();
        let listener = UnixListener::from_std(bound.listener.try_clone()?)?;
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            // Keep the process lock and socket inode guard alive through connection teardown.
            let _bound = bound;
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stopped => break,
                    _ = connections.join_next(), if !connections.is_empty() => {},
                    accepted = listener.accept() => {
                        let (stream, _) = accepted?;
                        if connections.len() >= MAX_CONNECTIONS || stream.peer_cred()?.uid() != current_uid() {
                            drop(stream);
                            continue;
                        }
                        let service = service.clone();
                        connections.spawn(async move { service.serve_connection(Box::new(stream)).await });
                    }
                }
            }
            connections.abort_all();
            while connections.join_next().await.is_some() {}
            Ok(())
        });
        Ok(Self {
            endpoint,
            stop: Some(stop),
            task: Some(task),
        })
    }
    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }
    /// Wait until all local clients have lost their connection and the socket is released.
    pub async fn shutdown(mut self) -> io::Result<()> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.wait_stopped().await
    }
    /// Hosts must supervise unexpected accept-loop termination as a runtime failure.
    pub async fn wait_stopped(&mut self) -> io::Result<()> {
        let result = match self.task.as_mut() {
            Some(task) => task
                .await
                .map_err(io::Error::other)
                .and_then(|result| result),
            None => return Ok(()),
        };
        self.task.take();
        result
    }
}
impl Drop for LocalIpcHost {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn current_uid() -> u32 {
    // No pointers or shared mutable state: libc reads the effective process UID.
    unsafe { libc::geteuid() }
}
fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
struct BoundSocket {
    listener: StdListener,
    _lock: HostDirectoryLock,
    path: PathBuf,
    inode: (u64, u64),
}

/// Exclusive ownership of a private native runtime directory. Hosts acquire
/// this before opening their Raft store and retain it until runtime shutdown.
pub struct HostDirectoryLock {
    directory: PathBuf,
    _file: File,
}
impl HostDirectoryLock {
    pub fn acquire(directory: &Path) -> io::Result<Self> {
        Self::acquire_named(directory, "host.lock")
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    fn acquire_named(directory: &Path, name: &str) -> io::Result<Self> {
        if !directory.is_absolute() {
            return Err(denied(
                "authority IPC requires an absolute private directory",
            ));
        }
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)?;
        let meta = fs::symlink_metadata(directory)?;
        if !meta.is_dir() || meta.uid() != current_uid() || meta.mode() & 0o077 != 0 {
            return Err(denied(
                "authority IPC directory must be private and owned by this user",
            ));
        }
        let directory = fs::canonicalize(directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(directory.join(name))?;
        let lock_meta = lock.metadata()?;
        if !lock_meta.is_file() || lock_meta.uid() != current_uid() || lock_meta.mode() & 0o077 != 0
        {
            return Err(denied("authority IPC lock must be a private regular file"));
        }
        lock.try_lock_exclusive().map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "another authority host owns this endpoint",
                )
            } else {
                error
            }
        })?;
        Ok(Self {
            directory,
            _file: lock,
        })
    }
}

impl BoundSocket {
    fn bind(directory: &Path) -> io::Result<Self> {
        let lock = HostDirectoryLock::acquire_named(directory, "authority.lock")?;
        let path = lock.directory().join("authority.sock");
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_socket() && meta.uid() == current_uid() => {
                let probe =
                    socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
                probe.set_nonblocking(true)?;
                match probe.connect(&socket2::SockAddr::unix(&path)?) {
                    Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                        fs::remove_file(&path)?
                    }
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            "existing authority socket is live or cannot be proven stale",
                        ))
                    }
                }
            }
            Ok(_) => {
                return Err(denied(
                    "authority endpoint is occupied by a non-socket path",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let listener = StdListener::bind(&path)?;
        let meta = fs::symlink_metadata(&path)?;
        let bound = Self {
            listener,
            _lock: lock,
            path,
            inode: (meta.dev(), meta.ino()),
        };
        fs::set_permissions(&bound.path, fs::Permissions::from_mode(0o600))?;
        bound.listener.set_nonblocking(true)?;
        Ok(bound)
    }
}
impl Drop for BoundSocket {
    fn drop(&mut self) {
        if let Ok(meta) = fs::symlink_metadata(&self.path) {
            if (meta.dev(), meta.ino()) == self.inode && meta.file_type().is_socket() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
