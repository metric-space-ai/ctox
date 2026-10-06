//! SSH storage uses SFTP, a pinned server key and a native SecretStore credential.
//! No shell command, ssh-agent, user config, or credential-bearing URI is used.
use crate::{validate_relative_path, StorageConnection};
use anyhow::{ensure, Context, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    time::Duration,
};

pub struct SshStorageOptions {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub root: String,
    pub host_key_sha256: String,
    pub private_key: String,
    pub passphrase: Option<String>,
}

pub fn connect(options: SshStorageOptions) -> Result<Box<dyn StorageConnection>> {
    ensure!(
        options.port != 0 && !options.host.is_empty() && !options.username.is_empty(),
        "invalid SSH endpoint"
    );
    let timeout = Duration::from_secs(10);
    let addresses: Vec<_> = (options.host.as_str(), options.port)
        .to_socket_addrs()?
        .take(8)
        .collect();
    let mut stream = None;
    for address in addresses {
        if let Ok(socket) = TcpStream::connect_timeout(&address, timeout) {
            stream = Some(socket);
            break;
        }
    }
    let stream = stream.context("SSH storage connection failed")?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut session = ssh2::Session::new()?;
    session.set_timeout(10_000);
    session.set_tcp_stream(stream);
    session.handshake()?;
    let (host_key, _) = session.host_key().context("SSH server key missing")?;
    let observed = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(host_key))
    );
    ensure!(
        observed == options.host_key_sha256,
        "SSH storage host key mismatch"
    );
    session.userauth_pubkey_memory(
        &options.username,
        None,
        &options.private_key,
        options.passphrase.as_deref(),
    )?;
    ensure!(session.authenticated(), "SSH storage authentication failed");
    let sftp = session.sftp()?;
    let root = PathBuf::from(&options.root);
    ensure!(
        root.is_absolute() && sftp.realpath(&root)? == root && sftp.lstat(&root)?.is_dir(),
        "storage root must be a canonical directory"
    );
    Ok(Box::new(SshStorage {
        session,
        sftp,
        root,
    }))
}

struct SshStorage {
    session: ssh2::Session,
    sftp: ssh2::Sftp,
    root: PathBuf,
}
impl SshStorage {
    fn path(&self, relative: &str) -> Result<PathBuf> {
        if relative.starts_with(".ctox-transfer-") {
            ensure!(
                !relative.contains('/')
                    && relative.ends_with(".part")
                    && relative.len() == 84
                    && relative[15..79].bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid storage staging path"
            );
        } else {
            validate_relative_path(relative)?;
        }
        let path = self.root.join(relative);
        let mut parent = self.root.clone();
        for component in Path::new(relative)
            .parent()
            .into_iter()
            .flat_map(Path::components)
        {
            parent.push(component);
            ensure!(
                self.sftp.lstat(&parent)?.is_dir(),
                "storage path parent is not a directory"
            );
        }
        match self.sftp.lstat(&path) {
            Ok(stat) => ensure!(stat.is_file(), "storage object is not a regular file"),
            Err(error) if absent(&error) => (),
            Err(error) => return Err(error.into()),
        }
        Ok(path)
    }
    fn file(&self, path: &str, write: bool) -> Result<ssh2::File> {
        let path = self.path(path)?;
        let flags = if write {
            ssh2::OpenFlags::READ | ssh2::OpenFlags::WRITE
        } else {
            ssh2::OpenFlags::READ
        };
        let mut file = self
            .sftp
            .open_mode(path, flags, 0o600, ssh2::OpenType::File)?;
        ensure!(
            file.stat()?.is_file(),
            "storage handle is not a regular file"
        );
        Ok(file)
    }
}
fn absent(error: &ssh2::Error) -> bool {
    matches!(error.code(), ssh2::ErrorCode::SFTP(2 | 10))
}
impl StorageConnection for SshStorage {
    fn length(&mut self, path: &str) -> Result<Option<u64>> {
        let path = self.path(path)?;
        match self.sftp.lstat(path) {
            Ok(stat) => Ok(Some(stat.size.context("remote length missing")?)),
            Err(error) if absent(&error) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn read(&mut self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>> {
        ensure!(length <= 1024 * 1024, "storage range too large");
        let mut file = self.file(path, false)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes)?;
        file.close()?;
        Ok(bytes)
    }
    fn create(&mut self, path: &str) -> Result<()> {
        let path = self.path(path)?;
        let mut file = self.sftp.open_mode(
            path,
            ssh2::OpenFlags::WRITE | ssh2::OpenFlags::CREATE | ssh2::OpenFlags::EXCLUSIVE,
            0o600,
            ssh2::OpenType::File,
        )?;
        file.fsync()?;
        file.close()?;
        Ok(())
    }
    fn truncate(&mut self, path: &str, length: u64) -> Result<()> {
        let mut file = self.file(path, true)?;
        let mut stat = file.stat()?;
        stat.size = Some(length);
        file.setstat(stat)?;
        file.fsync()?;
        file.close()?;
        Ok(())
    }
    fn write(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        let mut file = self.file(path, true)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
        file.fsync()?;
        file.close()?;
        Ok(())
    }
    fn publish(&mut self, staging: &str, destination: &str) -> Result<()> {
        self.sftp.rename(
            self.path(staging)?,
            self.path(destination)?,
            Some(ssh2::RenameFlags::empty()),
        )?;
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.session
            .disconnect(None, "transfer attempt finished", None)?;
        Ok(())
    }
}
