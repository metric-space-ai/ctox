use super::*;
use std::{
    future::Future,
    io::{Seek, SeekFrom, Write},
    pin::Pin,
};

const RANGE_BYTES: u64 = 1024 * 1024;

/// Immutable source claims. The native resolver must independently authenticate
/// this instance/key and apply the current file policy on its admitted session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PeerSource {
    pub instance_id: String,
    pub public_key: String,
    pub collection: String,
    pub file_id: String,
}
impl PeerSource {
    pub(crate) fn validate(&self) -> Result<()> {
        for (value, limit) in [
            (&self.instance_id, 256),
            (&self.public_key, 256),
            (&self.collection, 256),
            (&self.file_id, 1024),
        ] {
            ensure!(
                !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control),
                "invalid peer source identity"
            );
        }
        Ok(())
    }
}

/// No network stack lives here. Implementations use the existing admitted
/// NativeSyncSession::file_range and must cancel an in-flight range on drop.
/// They return bytes only; this worker exclusively owns filesystem writes.
pub trait PeerRangeSource: Send + Sync {
    /// Revalidate this exact job against the current enrolled account, session
    /// generation and file policy. Content identity is not an authorization grant.
    /// There is deliberately no permissive default for this required check.
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    fn read_range<'a>(
        &'a self,
        request: &'a DownloadRequest,
        offset: u64,
        length: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>>;
}

impl Worker {
    pub(super) async fn authorize_peer(
        &self,
        request: &DownloadRequest,
        stop: &AtomicBool,
    ) -> Result<bool> {
        let provider = self
            .peer
            .as_ref()
            .context("native peer transfer resolver is unavailable")?;
        let check = provider.authorize(request);
        tokio::pin!(check);
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        loop {
            if stop.load(Ordering::Acquire) || self.store.desired(&request.id)? != "run" {
                return Ok(false);
            }
            tokio::select! {
                result = &mut check => { result?; return Ok(true); }
                _ = ticker.tick() => {}
            }
        }
    }

    pub(super) async fn download_peer(
        &self,
        request: &DownloadRequest,
        staging: &Path,
        stop: &AtomicBool,
    ) -> Result<Option<PathBuf>> {
        let provider = self
            .peer
            .as_ref()
            .context("native peer transfer resolver is unavailable")?;
        let staging = staging.join("peer");
        private_directory(&staging)?;
        let partial = staging.join("payload");
        regular_or_absent(&partial)?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&partial)?;
        let mut offset = self.store.get(&request.id)?.completed_bytes;
        ensure!(offset <= request.size, "invalid durable peer offset");
        // A write that reached disk before its SQLite checkpoint is retried.
        // Missing/truncated staging can never inherit a stale durable offset.
        if file.metadata()?.len() < offset {
            offset = 0;
            self.peer_checkpoint(&request.id, offset)?;
        }
        file.set_len(offset)?;
        file.sync_all()?;
        sync_directory(&staging)?;
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        // Even an empty file must perform an authorized remote exchange.
        let mut empty_read = request.size == 0;
        while offset < request.size || empty_read {
            if stop.load(Ordering::Acquire) || self.store.desired(&request.id)? != "run" {
                return Ok(None);
            }
            let length = (request.size - offset).min(RANGE_BYTES);
            if !self.authorize_peer(request, stop).await? {
                return Ok(None);
            }
            let read = provider.read_range(request, offset, length);
            tokio::pin!(read);
            let bytes = loop {
                tokio::select! {
                    result = &mut read => break result?,
                    _ = ticker.tick() => {
                        if stop.load(Ordering::Acquire) || self.store.desired(&request.id)? != "run" {
                            return Ok(None);
                        }
                    }
                }
            };
            ensure!(
                bytes.len() as u64 == length,
                "peer returned an incomplete range"
            );
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            offset += length;
            // FULL-synchronous SQLite commits only after the corresponding bytes.
            self.peer_checkpoint(&request.id, offset)?;
            empty_read = false;
        }
        drop(file);
        if verify_file(&partial, request).is_err() {
            quarantine_partial(&staging)?;
            self.peer_checkpoint(&request.id, 0)?;
            bail!("content identity mismatch");
        }
        Ok(Some(partial))
    }

    fn peer_checkpoint(&self, id: &str, offset: u64) -> Result<()> {
        self.store.connection()?.execute(
            "UPDATE ctox_transfer_jobs SET completed_bytes=?2,updated_at=unixepoch() WHERE id=?1 AND state='running'",
            params![id, offset],
        )?;
        Ok(())
    }
}
