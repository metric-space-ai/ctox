//! Resumable storage transfers share the download worker, lease and durable controls.
use super::*;
use std::io::Write;

const CHUNK: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageDirection {
    Download,
    Upload,
}

/// Non-secret admission snapshot. A registry resolver must compare every field
/// with current owner/computer capability and endpoint configuration on resume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StorageTransfer {
    pub owner_user_id: String,
    pub computer_id: String,
    pub endpoint_ref: String,
    pub endpoint_fingerprint: String,
    pub purpose: String,
    pub relative_path: String,
    pub direction: StorageDirection,
}
impl StorageTransfer {
    pub fn validate(&self) -> Result<()> {
        for s in [&self.owner_user_id, &self.computer_id, &self.endpoint_ref] {
            ensure!(
                !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control),
                "invalid storage identity"
            );
        }
        ensure!(
            self.endpoint_fingerprint.len() == 64
                && self
                    .endpoint_fingerprint
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid endpoint fingerprint"
        );
        ensure!(
            matches!(self.purpose.as_str(), "artifacts" | "backups" | "exchange"),
            "invalid storage purpose"
        );
        validate_relative_path(&self.relative_path)
    }
}

pub fn validate_relative_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 2048
            && !path.starts_with('/')
            && !path
                .chars()
                .any(|c| c.is_control()
                    || matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            && path
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".." && !p.ends_with([' ', '.']))
            && !path.split('/').any(|p| p.starts_with(".ctox-transfer-")),
        "invalid relative storage path"
    );
    Ok(())
}

/// The native host owns registry/policy/secret resolution. There is no default
/// authorization or ambient SSH agent, home-directory credentials, or mount.
pub trait StorageResolver: Send + Sync {
    fn authorize(&self, request: &DownloadRequest) -> Result<()>;
    fn connect(&self, request: &DownloadRequest) -> Result<Box<dyn StorageConnection>>;
}

/// One attempt owns the connection. Each operation must have a network deadline;
/// writes include server flush acknowledgement before SQLite progress commits.
/// Paths are relative to the resolved root; adapters reject links/reparse points.
pub trait StorageConnection: Send {
    fn length(&mut self, path: &str) -> Result<Option<u64>>;
    fn read(&mut self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>>;
    fn create(&mut self, path: &str) -> Result<()>;
    fn truncate(&mut self, path: &str, length: u64) -> Result<()>;
    fn write(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()>;
    /// Must fail rather than replace an existing destination.
    fn publish(&mut self, staging: &str, destination: &str) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}

impl Store {
    /// Import a local regular file into private content storage before enqueue.
    /// Concurrent source changes can only fail the pinned hash/length check.
    pub fn stage_storage_upload(&self, source: &Path, sha256: &str, size: u64) -> Result<()> {
        let request = DownloadRequest {
            id: "storage-import".into(),
            sources: vec!["https://invalid.invalid/".into()],
            peer_source: None,
            storage: None,
            sha256: sha256.into(),
            size,
        };
        request.validate()?;
        regular_or_absent(source)?;
        let object = self.artifacts.join("objects").join(sha256);
        if object.try_exists()? {
            return verify_file(&object, &request);
        }
        // A unique temporary inode avoids exposing partially imported objects.
        let mut temporary = tempfile::NamedTempFile::new_in(self.artifacts.join("staging"))?;
        let mut input = File::open(source)?.take(size.saturating_add(1));
        std::io::copy(&mut input, &mut temporary)?;
        temporary.as_file().sync_all()?;
        verify_file(temporary.path(), &request)?;
        match fs::hard_link(temporary.path(), &object) {
            Ok(()) => sync_directory(object.parent().unwrap())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                verify_file(&object, &request)?
            }
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }
}

// A dropped async attempt still closes its authenticated transport before the
// worker/OS lease can disappear. Normal completion reports close failures.
struct ClosingStorage(Option<Box<dyn StorageConnection>>);
impl Drop for ClosingStorage {
    fn drop(&mut self) {
        if let Some(mut remote) = self.0.take() {
            let _ = remote.close();
        }
    }
}

impl Worker {
    async fn storage_continue(&self, request: &DownloadRequest, stop: &AtomicBool) -> Result<bool> {
        if stop.load(Ordering::Acquire) || self.desired(&request.id).await? != "run" {
            return Ok(false);
        }
        self.storage
            .as_ref()
            .context("storage resolver unavailable")?
            .authorize(request)?;
        Ok(!stop.load(Ordering::Acquire) && self.desired(&request.id).await? == "run")
    }

    pub(super) async fn transfer_storage(
        &self,
        request: &DownloadRequest,
        stop: &AtomicBool,
    ) -> Result<()> {
        if !self.storage_continue(request, stop).await? {
            return self.settle_interruption(&request.id).await;
        }
        let mut remote = ClosingStorage(Some(
            self.storage
                .as_ref()
                .context("storage resolver unavailable")?
                .connect(request)?,
        ));
        // Always close the actual attempt before the worker releases its lease.
        let outcome = self
            .storage_attempt(
                request,
                remote.0.as_mut().expect("active transport").as_mut(),
                stop,
            )
            .await;
        let closed = remote.0.take().expect("active transport").close();
        outcome.and(closed)
    }

    async fn storage_attempt(
        &self,
        request: &DownloadRequest,
        remote: &mut dyn StorageConnection,
        stop: &AtomicBool,
    ) -> Result<()> {
        let storage = request
            .storage
            .as_ref()
            .context("storage request missing")?;
        let object = self.artifacts.join("objects").join(&request.sha256);
        let staging = self
            .artifacts
            .join("staging")
            .join(&request.id)
            .join("storage");
        private_directory(&staging)?;
        if storage.direction == StorageDirection::Download {
            ensure!(
                remote.length(&storage.relative_path)? == Some(request.size),
                "content identity mismatch"
            );
            // Current remote authority and source existence are checked even for cache reuse.
            if object.try_exists()? {
                verify_file(&object, request)?;
                if !self.storage_continue(request, stop).await? {
                    return self.settle_interruption(&request.id).await;
                }
                return self.publish_receipt(request, &object, stop).await;
            }
            let partial = staging.join("payload");
            regular_or_absent(&partial)?;
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(&partial)?;
            let mut offset = self.get(&request.id).await?.completed_bytes;
            ensure!(offset <= request.size, "invalid storage checkpoint");
            if file.metadata()?.len() < offset {
                offset = 0;
                self.checkpoint(&request.id, 0).await?;
            }
            file.set_len(offset)?;
            file.sync_all()?;
            sync_directory(&staging)?;
            while offset < request.size {
                if !self.storage_continue(request, stop).await? {
                    return self.settle_interruption(&request.id).await;
                }
                let length = (request.size - offset).min(CHUNK) as usize;
                let bytes = remote.read(&storage.relative_path, offset, length)?;
                ensure!(bytes.len() == length, "incomplete storage range");
                file.seek(SeekFrom::Start(offset))?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                offset += length as u64;
                self.checkpoint(&request.id, offset).await?;
            }
            drop(file);
            if verify_file(&partial, request).is_err() {
                quarantine_partial(&staging)?;
                self.checkpoint(&request.id, 0).await?;
                bail!("content identity mismatch");
            }
            if !self.storage_continue(request, stop).await? {
                return self.settle_interruption(&request.id).await;
            }
            return self.publish_partial(request, &partial, &object, stop).await;
        }

        verify_file(&object, request)?;
        // Recover publication-before-receipt crashes without ever overwriting a target.
        if remote.length(&storage.relative_path)?.is_some() {
            if !self
                .verify_storage_remote(request, remote, &storage.relative_path, stop)
                .await?
            {
                return self.settle_interruption(&request.id).await;
            }
            return self.storage_receipt(request, stop).await;
        }
        let encoded = serde_json::to_vec(request)?;
        let temporary = format!(".ctox-transfer-{:x}.part", Sha256::digest(encoded));
        let mut offset = self.get(&request.id).await?.completed_bytes;
        ensure!(offset <= request.size, "invalid storage checkpoint");
        match remote.length(&temporary)? {
            None => {
                remote.create(&temporary)?;
                offset = 0;
            }
            Some(length) => {
                offset = offset.min(length);
            }
        }
        let mut input = File::open(&object)?;
        // A saved offset alone cannot bless a replaced/truncated remote prefix.
        let mut checked = 0;
        while checked < offset {
            if !self.storage_continue(request, stop).await? {
                return self.settle_interruption(&request.id).await;
            }
            let n = (offset - checked).min(CHUNK) as usize;
            let mut expected = vec![0; n];
            input.read_exact(&mut expected)?;
            ensure!(
                remote.read(&temporary, checked, n)? == expected,
                "remote resume prefix changed"
            );
            checked += n as u64;
        }
        if !self.storage_continue(request, stop).await? {
            return self.settle_interruption(&request.id).await;
        }
        remote.truncate(&temporary, offset)?;
        self.checkpoint(&request.id, offset).await?;
        while offset < request.size {
            if !self.storage_continue(request, stop).await? {
                return self.settle_interruption(&request.id).await;
            }
            let n = (request.size - offset).min(CHUNK) as usize;
            let mut bytes = vec![0; n];
            input.read_exact(&mut bytes)?;
            remote.write(&temporary, offset, &bytes)?;
            offset += n as u64;
            self.checkpoint(&request.id, offset).await?;
        }
        if !self
            .verify_storage_remote(request, remote, &temporary, stop)
            .await?
        {
            return self.settle_interruption(&request.id).await;
        }
        if !self.storage_continue(request, stop).await? {
            return self.settle_interruption(&request.id).await;
        }
        remote.publish(&temporary, &storage.relative_path)?;
        if !self
            .verify_storage_remote(request, remote, &storage.relative_path, stop)
            .await?
        {
            return self.settle_interruption(&request.id).await;
        }
        self.storage_receipt(request, stop).await
    }

    async fn verify_storage_remote(
        &self,
        request: &DownloadRequest,
        remote: &mut dyn StorageConnection,
        path: &str,
        stop: &AtomicBool,
    ) -> Result<bool> {
        if !self.storage_continue(request, stop).await? {
            return Ok(false);
        }
        ensure!(
            remote.length(path)? == Some(request.size),
            "content identity mismatch"
        );
        let mut digest = Sha256::new();
        let mut offset = 0;
        while offset < request.size {
            if !self.storage_continue(request, stop).await? {
                return Ok(false);
            }
            let n = (request.size - offset).min(CHUNK) as usize;
            let bytes = remote.read(path, offset, n)?;
            ensure!(bytes.len() == n, "incomplete storage verification range");
            digest.update(bytes);
            offset += n as u64;
        }
        ensure!(
            format!("{:x}", digest.finalize()) == request.sha256,
            "content identity mismatch"
        );
        self.storage_continue(request, stop).await
    }

    async fn storage_receipt(&self, request: &DownloadRequest, stop: &AtomicBool) -> Result<()> {
        if !self.storage_continue(request, stop).await? {
            return self.settle_interruption(&request.id).await;
        }
        let storage = request.storage.as_ref().unwrap();
        self.commit_receipt(
            request,
            Receipt {
                transfer_id: request.id.clone(),
                sha256: request.sha256.clone(),
                size: request.size,
                artifact: format!("storage:{}/{}", storage.endpoint_ref, storage.relative_path),
                engine_revision: None,
                transport: "ctox-storage-v1".into(),
            },
            stop,
        )
        .await
    }
}
