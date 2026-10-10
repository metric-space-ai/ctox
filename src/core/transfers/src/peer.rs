use super::*;
use std::{
    future::Future,
    io::{Seek, SeekFrom, Write},
    pin::Pin,
};

const RANGE_BYTES: u64 = 1024 * 1024;

/// Safe native failure categories. Raw RPC responses and URLs never enter job state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerReadFailure {
    Authorization,
    SourceNotReady,
    Identity,
    Grant,
    FilePermission,
    Timeout,
    Busy,
    SequenceGap,
    Disconnected,
    Closed,
    Rejected,
    InvalidChunk,
    Unavailable,
}

impl PeerReadFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::Authorization => "PEER_AUTHORIZATION_FAILED",
            Self::SourceNotReady => "PEER_SOURCE_NOT_READY",
            Self::Identity => "PEER_IDENTITY_UNVERIFIED",
            Self::Grant => "PEER_GRANT_AUTHORIZATION_UNAVAILABLE",
            Self::FilePermission => "PEER_FILE_PERMISSION_UNAVAILABLE",
            Self::Timeout => "PEER_FILE_TIMEOUT",
            Self::Busy => "PEER_FILE_BUSY",
            Self::SequenceGap => "PEER_FILE_SEQUENCE_GAP",
            Self::Disconnected => "PEER_FILE_DISCONNECTED",
            Self::Closed => "PEER_FILE_CLOSED",
            Self::Rejected => "PEER_FILE_REJECTED",
            Self::InvalidChunk => "PEER_FILE_INVALID_CHUNK",
            Self::Unavailable => "PEER_FILE_UNAVAILABLE",
        }
    }

    pub fn retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::Busy | Self::SequenceGap)
    }
}

impl std::fmt::Display for PeerReadFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}
impl std::error::Error for PeerReadFailure {}

/// Original host enrollment/account snapshot, not a credential or permission.
/// Native admission compares this with current authority on every attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PeerAccountBinding {
    pub target_id: String,
    /// Durable native account generation, never an Electron process counter.
    pub account_epoch: u64,
    /// Non-secret reference to an issued, narrowly scoped native grant.
    /// Legacy rows decode for diagnosis but cannot pass native admission.
    #[serde(default)]
    pub grant_id: String,
    /// SHA-256 of the existing native principal's serialized contract, including
    /// its authorization epoch and device. Avoids duplicating that wire schema.
    pub principal_sha256: String,
}

impl PeerAccountBinding {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.target_id.is_empty()
                && self.target_id.len() <= 256
                && self.target_id.trim() == self.target_id
                && !self.target_id.chars().any(char::is_control),
            "invalid enrolled target id"
        );
        ensure!(
            !self.grant_id.is_empty()
                && self.grant_id.len() <= 256
                && self.grant_id.trim() == self.grant_id
                && !self.grant_id.chars().any(char::is_control),
            "issued native transfer grant id required"
        );
        ensure!(
            self.principal_sha256.len() == 64
                && self
                    .principal_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid enrolled principal digest"
        );
        Ok(())
    }
}

/// Immutable source claims. The native resolver must independently authenticate
/// this instance/key and apply the current file policy on its admitted session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PeerSource {
    pub instance_id: String,
    pub public_key: String,
    pub collection: String,
    pub file_id: String,
    /// Older requests decode for diagnosis, but native admission rejects an
    /// absent binding. Never populate it from the current account on resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_binding: Option<PeerAccountBinding>,
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
        if let Some(binding) = &self.account_binding {
            binding.validate()?;
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

    /// Drain owned transports while the daemon runtime still exists. Stateless
    /// providers have nothing to close; authorization has no permissive default.
    fn shutdown(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
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
                result = &mut check => {
                    result.map_err(|error| {
                        // Preserve only our typed, parameter-free categories.
                        // Unknown provider text remains a closed authorization failure.
                        let failure = error
                            .downcast_ref::<PeerReadFailure>()
                            .copied()
                            .unwrap_or(PeerReadFailure::Authorization);
                        anyhow::Error::from(failure)
                    })?;
                    return Ok(true);
                }
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
