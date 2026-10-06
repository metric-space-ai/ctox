//! SFTP with a pinned server key and borrowed native SecretStore credentials.
//! Pure Rust SSH avoids libssh2's OpenSSL collision with the daemon's BoringSSL.
use crate::{validate_relative_path, StorageConnection};
use anyhow::{ensure, Context, Result};
use russh::{
    client,
    keys::{decode_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey},
};
use russh_sftp::{
    client::{error::Error as SftpError, RawSftpSession},
    protocol::{FileAttributes, OpenFlags, StatusCode},
};
use std::{future::Future, sync::Arc, time::Duration};
use tokio::runtime::Runtime;

const DEADLINE: Duration = Duration::from_secs(10);
const PACKET: usize = 32 * 1024;

pub struct SshStorageOptions<'a> {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub root: String,
    pub host_key_sha256: String,
    pub private_key: &'a str,
    pub passphrase: Option<&'a str>,
}

struct PinnedKey(String);
impl client::Handler for PinnedKey {
    type Error = anyhow::Error;
    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool> {
        Ok(key.fingerprint(HashAlg::Sha256).to_string() == self.0)
    }
}

pub fn connect(options: SshStorageOptions<'_>) -> Result<Box<dyn StorageConnection>> {
    ensure!(
        options.port != 0 && !options.host.is_empty() && !options.username.is_empty(),
        "invalid SSH endpoint"
    );
    // A scoped thread permits borrowed credentials and also works when our
    // synchronous API is called from a current-thread Tokio runtime. It joins
    // before the SecretStore credential callback returns.
    let storage = std::thread::scope(|scope| {
        scope
            .spawn(move || -> Result<SshStorage> {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()?;
                let result = runtime.block_on(async {
                    tokio::time::timeout(DEADLINE, async {
                        let key =
                            Arc::new(decode_secret_key(options.private_key, options.passphrase)?);
                        let key_lifetime = Arc::downgrade(&key);
                        let mut session = client::connect(
                            Arc::new(client::Config::default()),
                            (options.host.as_str(), options.port),
                            PinnedKey(options.host_key_sha256),
                        )
                        .await?;
                        let algorithm = session.best_supported_rsa_hash().await?.flatten();
                        ensure!(
                            session
                                .authenticate_publickey(
                                    &options.username,
                                    PrivateKeyWithHashAlg::new(key, algorithm)
                                )
                                .await?
                                .success(),
                            "SSH storage authentication failed"
                        );
                        // Fail closed if the SSH implementation retains signing credentials
                        // after authentication; only the authenticated transport may survive.
                        ensure!(
                            key_lifetime.upgrade().is_none(),
                            "SSH authentication retained signing credentials"
                        );
                        let channel = session.channel_open_session().await?;
                        channel.request_subsystem(true, "sftp").await?;
                        let sftp = Arc::new(RawSftpSession::new(channel.into_stream()));
                        sftp.init().await?;
                        canonical_root(&sftp, &options.root).await?;
                        Ok::<_, anyhow::Error>((session, sftp))
                    })
                    .await
                    .context("SSH storage authentication timed out")?
                });
                match result {
                    Ok((session, sftp)) => Ok(SshStorage {
                        runtime: Some(runtime),
                        session: Some(session),
                        sftp,
                        root: options.root,
                        failed: false,
                    }),
                    Err(error) => {
                        runtime.shutdown_timeout(Duration::from_secs(1));
                        Err(error)
                    }
                }
            })
            .join()
    })
    .map_err(|_| anyhow::anyhow!("SSH authentication thread failed"))??;
    Ok(Box::new(storage))
}

struct SshStorage {
    runtime: Option<Runtime>,
    session: Option<client::Handle<PinnedKey>>,
    sftp: Arc<RawSftpSession>,
    root: String,
    failed: bool,
}
impl SshStorage {
    fn invoke<T: Send + 'static>(
        &mut self,
        operation: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Result<T> {
        ensure!(
            !self.failed,
            "SSH attempt must reconnect after an operation failure"
        );
        let runtime = self.runtime.as_ref().context("SSH connection closed")?;
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        runtime.spawn(async move {
            let result = tokio::time::timeout(DEADLINE, operation)
                .await
                .context("SSH storage operation timed out")
                .and_then(|r| r);
            let _ = send.send(result);
        });
        let result = receive.recv().context("SSH operation task terminated")?;
        self.failed = result.is_err();
        result
    }
}

async fn canonical_root(sftp: &RawSftpSession, root: &str) -> Result<()> {
    let canonical = sftp.realpath(root).await?;
    ensure!(
        root.starts_with('/')
            && canonical.files.len() == 1
            && canonical.files[0].filename == root
            && sftp.lstat(root).await?.attrs.file_type().is_dir(),
        "storage root is no longer a canonical directory"
    );
    Ok(())
}
async fn checked_path(sftp: &RawSftpSession, root: &str, relative: &str) -> Result<String> {
    if relative.starts_with(".ctox-transfer-") {
        ensure!(
            !relative.contains('/')
                && relative.ends_with(".part")
                && relative.len() == 84
                && relative.as_bytes()[15..79].iter().all(u8::is_ascii_hexdigit),
            "invalid storage staging path"
        );
    } else {
        validate_relative_path(relative)?;
    }
    // Revalidate even without reconnect: another server-side writer may replace
    // the admitted root or a parent with a link between storage operations.
    canonical_root(sftp, root).await?;
    let mut path = root.trim_end_matches('/').to_owned();
    let components: Vec<_> = relative.split('/').collect();
    for (index, component) in components.iter().enumerate() {
        path.push('/');
        path.push_str(component);
        if index + 1 < components.len() {
            ensure!(
                sftp.lstat(&path).await?.attrs.file_type().is_dir(),
                "storage path parent is not a directory"
            );
        }
    }
    match sftp.lstat(&path).await {
        Ok(stat) => ensure!(
            stat.attrs.file_type().is_file(),
            "storage object is not a regular file"
        ),
        Err(error) if absent(&error) => (),
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}
fn absent(error: &SftpError) -> bool {
    matches!(error, SftpError::Status(status) if status.status_code == StatusCode::NoSuchFile)
}
async fn file(sftp: &RawSftpSession, root: &str, path: &str, flags: OpenFlags) -> Result<String> {
    let path = checked_path(sftp, root, path).await?;
    let handle = sftp
        .open(
            path,
            flags,
            FileAttributes {
                permissions: Some(0o600),
                ..Default::default()
            },
        )
        .await?
        .handle;
    ensure!(
        sftp.fstat(&handle).await?.attrs.file_type().is_file(),
        "storage handle is not a regular file"
    );
    Ok(handle)
}
// Raw fsync deliberately fails if the server lacks the extension; the high-level
// SFTP File::sync_all silently succeeds in that case and cannot guard checkpoints.
async fn flush_close(sftp: &RawSftpSession, handle: String) -> Result<()> {
    sftp.fsync(&handle).await?;
    sftp.close(handle).await?;
    Ok(())
}
impl StorageConnection for SshStorage {
    fn length(&mut self, path: &str) -> Result<Option<u64>> {
        let (sftp, root, path) = (self.sftp.clone(), self.root.clone(), path.to_owned());
        self.invoke(async move {
            let path = checked_path(&sftp, &root, &path).await?;
            match sftp.lstat(path).await {
                Ok(stat) => Ok(Some(stat.attrs.size.context("remote length missing")?)),
                Err(error) if absent(&error) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
    }
    fn read(&mut self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>> {
        ensure!(length <= 1024 * 1024, "storage range too large");
        let (sftp, root, path) = (self.sftp.clone(), self.root.clone(), path.to_owned());
        self.invoke(async move {
            let handle = file(&sftp, &root, &path, OpenFlags::READ).await?;
            let mut bytes = Vec::with_capacity(length);
            while bytes.len() < length {
                let wanted = (length - bytes.len()).min(PACKET);
                let data = sftp
                    .read(
                        &handle,
                        offset
                            .checked_add(bytes.len() as u64)
                            .context("storage offset overflow")?,
                        wanted as u32,
                    )
                    .await?
                    .data;
                ensure!(
                    !data.is_empty() && data.len() <= wanted,
                    "invalid SFTP read length"
                );
                bytes.extend_from_slice(&data);
            }
            sftp.close(handle).await?;
            Ok(bytes)
        })
    }
    fn create(&mut self, path: &str) -> Result<()> {
        let (sftp, root, path) = (self.sftp.clone(), self.root.clone(), path.to_owned());
        self.invoke(async move {
            let handle = file(
                &sftp,
                &root,
                &path,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
            )
            .await?;
            flush_close(&sftp, handle).await
        })
    }
    fn truncate(&mut self, path: &str, length: u64) -> Result<()> {
        let (sftp, root, path) = (self.sftp.clone(), self.root.clone(), path.to_owned());
        self.invoke(async move {
            let handle = file(&sftp, &root, &path, OpenFlags::READ | OpenFlags::WRITE).await?;
            sftp.fsetstat(
                &handle,
                FileAttributes {
                    size: Some(length),
                    ..Default::default()
                },
            )
            .await?;
            flush_close(&sftp, handle).await
        })
    }
    fn write(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() <= 1024 * 1024, "storage range too large");
        let (sftp, root, path, bytes) = (
            self.sftp.clone(),
            self.root.clone(),
            path.to_owned(),
            bytes.to_vec(),
        );
        self.invoke(async move {
            let handle = file(&sftp, &root, &path, OpenFlags::READ | OpenFlags::WRITE).await?;
            for (index, chunk) in bytes.chunks(PACKET).enumerate() {
                sftp.write(
                    &handle,
                    offset
                        .checked_add((index * PACKET) as u64)
                        .context("storage offset overflow")?,
                    chunk.to_vec(),
                )
                .await?;
            }
            flush_close(&sftp, handle).await
        })
    }
    fn publish(&mut self, staging: &str, destination: &str) -> Result<()> {
        let (sftp, root, staging, destination) = (
            self.sftp.clone(),
            self.root.clone(),
            staging.to_owned(),
            destination.to_owned(),
        );
        self.invoke(async move {
            let staging = checked_path(&sftp, &root, &staging).await?;
            let destination = checked_path(&sftp, &root, &destination).await?;
            // SFTP v3 rename refuses an existing destination. Do not use the
            // OpenSSH posix-rename extension, which replaces it.
            sftp.rename(staging, destination).await?;
            Ok(())
        })
    }
    fn close(&mut self) -> Result<()> {
        let Some(runtime) = self.runtime.take() else {
            return Ok(());
        };
        let session = self.session.take();
        let sftp = self.sftp.clone();
        // Runtime shutdown must never run directly inside a caller's Tokio task.
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let result = runtime.block_on(async {
                        tokio::time::timeout(DEADLINE, async {
                            sftp.close_session()?;
                            if let Some(session) = session {
                                session
                                    .disconnect(
                                        russh::Disconnect::ByApplication,
                                        "transfer attempt finished",
                                        "en",
                                    )
                                    .await?;
                                session.await?;
                            }
                            Ok::<_, anyhow::Error>(())
                        })
                        .await
                        .context("SSH storage close timed out")?
                    });
                    runtime.shutdown_timeout(Duration::from_secs(1));
                    result
                })
                .join()
        })
        .map_err(|_| anyhow::anyhow!("SSH shutdown thread failed"))?
    }
}
impl Drop for SshStorage {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
