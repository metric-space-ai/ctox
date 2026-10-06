//! SFTP GET via SSH subsystem. Streaming reads, `--ssh-host-key-md` check.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::http::HttpProgress;
use crate::options::OptionSet;
use crate::storage::FileStorage;
use russh::keys::ssh_key::PublicKey;
use russh::keys::PublicKeyOrCertificate;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::watch;
use url::Url;

pub struct SftpJob {
    pub uris: Vec<String>,
    pub dest: PathBuf,
    pub opts: OptionSet,
    pub progress: HttpProgress,
    pub cancel: watch::Receiver<bool>,
}

/// Hash the SSH host-key blob the same way C++ aria2 (`SSHSession`) does:
/// SHA-1 or MD5 of the raw public-key bytes. TYPE names match `--ssh-host-key-md`.
pub fn hash_ssh_host_key(kind: &str, blob: &[u8]) -> Result<Vec<u8>> {
    match kind {
        "sha-1" | "sha1" => {
            use sha1::{Digest, Sha1};
            Ok(Sha1::digest(blob).to_vec())
        }
        "md5" => {
            use md5::{Digest, Md5};
            Ok(Md5::digest(blob).to_vec())
        }
        _ => Err(Error::Sftp(format!("ssh-host-key-md type {kind}"))),
    }
}

pub fn host_key_blob(key: &PublicKey) -> Result<Vec<u8>> {
    key.to_bytes()
        .map_err(|e| Error::Sftp(format!("host key: {e}")))
}

fn parse_ssh_host_key_md(s: &str) -> Result<(String, Vec<u8>)> {
    let (kind, hex) = s
        .split_once('=')
        .ok_or_else(|| Error::Sftp("ssh-host-key-md wants TYPE=DIGEST".into()))?;
    let compact: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let digest = hex::decode(&compact).map_err(|e| Error::Sftp(format!("ssh-host-key-md: {e}")))?;
    Ok((kind.trim().to_ascii_lowercase(), digest))
}

fn key_ok(expected: &Option<(String, Vec<u8>)>, pk: &PublicKeyOrCertificate) -> bool {
    let Some((kind, want)) = expected else {
        return true;
    };
    let blob = match pk {
        PublicKeyOrCertificate::PublicKey { key, .. } => match host_key_blob(key) {
            Ok(b) => b,
            Err(_) => return false,
        },
        PublicKeyOrCertificate::Certificate(c) => match c.to_bytes() {
            Ok(b) => b,
            Err(_) => return false,
        },
    };
    match hash_ssh_host_key(kind, &blob) {
        Ok(got) => got == *want,
        Err(_) => false,
    }
}

struct HostCheck {
    expected: Option<(String, Vec<u8>)>,
}

impl russh::client::Handler for HostCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(key_ok(&self.expected, server_public_key))
    }
}

fn credentials(u: &Url, opts: &OptionSet) -> (String, String) {
    let user = if !u.username().is_empty() {
        u.username().to_string()
    } else {
        opts.get("ftp-user").unwrap_or("anonymous").to_string()
    };
    let pass = if let Some(p) = u.password() {
        p.to_string()
    } else {
        opts.get("ftp-passwd").unwrap_or("ARIA2USER@").to_string()
    };
    (user, pass)
}

static LAST_SFTP_SEEK: AtomicU64 = AtomicU64::new(0);

/// C++ SftpDownloadCommand: SSH_FXP_SEEK count (once at resume, never per-chunk).
pub fn last_sftp_seek() -> u64 {
    LAST_SFTP_SEEK.load(Ordering::SeqCst)
}

pub fn reset_sftp_seek() {
    LAST_SFTP_SEEK.store(0, Ordering::SeqCst);
}

pub async fn download(job: SftpJob) -> Result<()> {
    let uri = job
        .uris
        .first()
        .cloned()
        .ok_or_else(|| Error::Sftp("no uri".into()))?;
    let u = Url::parse(&uri).map_err(|e| Error::Sftp(e.to_string()))?;
    if u.scheme() != "sftp" {
        return Err(Error::Sftp(format!("not sftp: {}", u.scheme())));
    }
    let host = u.host_str().ok_or_else(|| Error::Sftp("no host".into()))?;
    let port = u.port_or_known_default().unwrap_or(22);
    let path = if u.path().is_empty() {
        "/".to_string()
    } else {
        u.path().to_string()
    };
    let (user, pass) = credentials(&u, &job.opts);
    let timeout = Duration::from_secs(job.opts.u64("connect-timeout", 60).max(1));
    let expected = match job.opts.get("ssh-host-key-md").filter(|s| !s.is_empty()) {
        Some(s) => Some(parse_ssh_host_key_md(s)?),
        None => None,
    };

    let cfg = Arc::new(russh::client::Config::default());
    let mut session = tokio::time::timeout(
        timeout,
        russh::client::connect(cfg, (host, port), HostCheck { expected }),
    )
    .await
    .map_err(|_| Error::Sftp("connect-timeout".into()))?
    .map_err(|e| Error::Sftp(e.to_string()))?;

    if !session
        .authenticate_password(user, pass)
        .await
        .map_err(|e| Error::Sftp(e.to_string()))?
        .success()
    {
        return Err(Error::Sftp("auth failed".into()));
    }

    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| Error::Sftp(e.to_string()))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| Error::Sftp(e.to_string()))?;
    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| Error::Sftp(e.to_string()))?;

    let mut total = sftp
        .metadata(path.as_str())
        .await
        .ok()
        .and_then(|m| m.size)
        .unwrap_or(0);
    job.progress.total.store(total, Ordering::Relaxed);

    let mut resume_from = 0u64;
    if job.opts.bool("continue", false) {
        if let Ok(meta) = std::fs::metadata(&job.dest) {
            resume_from = meta.len();
            if total > 0 {
                resume_from = resume_from.min(total);
            }
            job.progress.completed.store(resume_from, Ordering::Relaxed);
        }
    }

    let alloc = crate::storage::alloc_mode_for(&job.opts, total);
    let store = FileStorage::from_opts(job.dest.clone(), total, alloc, &job.opts);
    store.ensure().await?;

    let mut file = sftp
        .open(path.as_str())
        .await
        .map_err(|e| Error::Sftp(e.to_string()))?;
    if total == 0 {
        if let Ok(m) = file.metadata().await {
            if let Some(sz) = m.size {
                total = sz;
                job.progress.total.store(total, Ordering::Relaxed);
            }
        }
    }
    if resume_from > 0 {
        file.seek(SeekFrom::Start(resume_from))
            .await
            .map_err(|e| Error::Sftp(e.to_string()))?;
        LAST_SFTP_SEEK.fetch_add(1, Ordering::SeqCst);
    }
    let mut offset = resume_from;
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        if *job.cancel.borrow() {
            return Err(Error::Sftp("canceled".into()));
        }
        if total > 0 && offset >= total {
            break;
        }
        let want = if total > 0 {
            ((total - offset) as usize).min(buf.len())
        } else {
            buf.len()
        };
        let n = file
            .read(&mut buf[..want])
            .await
            .map_err(|e| Error::Sftp(e.to_string()))?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        if !store.try_pwrite(offset, chunk)? && !store.try_cache(offset, chunk)? {
            store.write_body(offset, chunk).await?;
        }
        offset += n as u64;
        job.progress
            .completed
            .fetch_add(n as u64, Ordering::Relaxed);
    }
    store.flush().await?;
    if total == 0 {
        job.progress.total.store(offset, Ordering::Relaxed);
    } else {
        job.progress.completed.store(total.max(offset), Ordering::Relaxed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_colon_hex() {
        let (k, d) = parse_ssh_host_key_md("sha-1=aa:bb:cc:dd").unwrap();
        assert_eq!(k, "sha-1");
        assert_eq!(d, vec![0xaa, 0xbb, 0xcc, 0xdd]);
    }

    #[test]
    fn sha1_length() {
        assert_eq!(hash_ssh_host_key("sha-1", b"abc").unwrap().len(), 20);
    }
}
