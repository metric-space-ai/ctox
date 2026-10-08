// Origin: CTOX
// License: AGPL-3.0-only
//! Private, unverified ranges survive a bounded copy operation. Only the CAS
//! full-hash ingestion can turn them into a verified artifact.
use super::*;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

pub(super) struct StagedPart {
    file: File,
    path: PathBuf,
    pub(super) offset: u64,
}
impl StagedPart {
    // The caller holds Target::current for every filesystem operation here.
    pub(super) fn open(directory: &Path, artifact: Option<&ArtifactRef>) -> anyhow::Result<Self> {
        private_dir(directory)?;
        let name = artifact.map_or("manifest", |a| a.sha256.as_str());
        anyhow::ensure!(
            name == "manifest"
                || (name.len() == 64
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
            "invalid checkpoint staging identity"
        );
        let path = directory.join(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)?;
        let offset = file.metadata()?.len();
        anyhow::ensure!(
            offset <= artifact.map_or(MANIFEST_LIMIT, |a| a.size_bytes),
            "checkpoint staged range too large"
        );
        let part = Self { file, path, offset };
        part.check()?;
        File::open(directory)?.sync_all()?;
        Ok(part)
    }
    fn check(&self) -> anyhow::Result<()> {
        let opened = self.file.metadata()?;
        let current = std::fs::symlink_metadata(&self.path)?;
        anyhow::ensure!(
            opened.is_file()
                && current.is_file()
                && !current.file_type().is_symlink()
                && opened.dev() == current.dev()
                && opened.ino() == current.ino()
                && opened.uid() == unsafe { libc::geteuid() }
                && opened.nlink() == 1
                && opened.permissions().mode() & 0o777 == 0o600
                && opened.len() == self.offset,
            "checkpoint staged range changed"
        );
        Ok(())
    }
    pub(super) fn append(&mut self, bytes: &[u8], size: u64) -> anyhow::Result<()> {
        self.check()?;
        anyhow::ensure!(
            self.offset
                .checked_add(bytes.len() as u64)
                .is_some_and(|end| end <= size),
            "checkpoint range exceeds declared size"
        );
        self.file.seek(SeekFrom::Start(self.offset))?;
        self.file.write_all(bytes)?;
        self.file.sync_data()?;
        self.offset += bytes.len() as u64;
        Ok(())
    }
    pub(super) fn finish(
        &mut self,
        store: &CheckpointStore,
        artifact: Option<&ArtifactRef>,
        digest: &str,
    ) -> anyhow::Result<Vec<u8>> {
        self.check()?;
        self.file.seek(SeekFrom::Start(0))?;
        let result = if let Some(a) = artifact {
            store.ingest_blob(a, &mut self.file).map(|()| Vec::new())
        } else {
            use sha2::{Digest, Sha256};
            let mut bytes = Vec::new();
            self.file.read_to_end(&mut bytes)?;
            if format!("{:x}", Sha256::digest(&bytes)) == digest {
                Ok(bytes)
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "checkpoint manifest hash mismatch",
                ))
            }
        };
        // A corrupt incomplete prefix must not poison every future operation.
        // No CAS blob is removed, overwritten, or trusted by this cleanup.
        if (artifact.is_some() && result.is_ok())
            || result
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::InvalidData)
        {
            self.check()?;
            std::fs::remove_file(&self.path)?;
            File::open(self.path.parent().unwrap())?.sync_all()?;
        }
        result.map_err(Into::into)
    }
}
pub(super) fn verified(store: &CheckpointStore, artifact: &ArtifactRef) -> anyhow::Result<bool> {
    match store.verify_blob(artifact) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(bytes: &[u8]) -> ArtifactRef {
        use sha2::{Digest, Sha256};
        ArtifactRef {
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
        }
    }
    #[test]
    fn checkpoint_copy_progress_is_not_a_copy_receipt_or_longer_operation() {
        let request: CopyRequest = serde_json::from_value(serde_json::json!({
            "bindingDigest": "aa".repeat(32), "sourceRoute": "source"
        }))
        .unwrap();
        assert_eq!(
            request.operation_timeout(),
            std::time::Duration::from_secs(60)
        );
        let pending = serde_json::to_value(CopyResponse::CopyPending {
            checkpoint_digest: "aa".repeat(32),
            verified_bytes: 3,
            partial_bytes: 2,
            total_bytes: Some(10),
        })
        .unwrap();
        assert_eq!(pending["status"], "copy_pending");
        assert!(pending.get("receipt").is_none());
    }
    #[test]
    fn checkpoint_copy_progress_survives_reopen_without_acknowledging_partial_bytes() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        create_private(&staging).unwrap();
        let store = CheckpointStore::open(root.path().join("cas"), BLOB_LIMIT).unwrap();
        let bytes = b"original checkpoint artifact";
        let a = reference(bytes);
        let mut first = StagedPart::open(&staging, Some(&a)).unwrap();
        first.append(&bytes[..9], a.size_bytes).unwrap();
        drop(first);
        assert!(!verified(&store, &a).unwrap());
        let mut resumed = StagedPart::open(&staging, Some(&a)).unwrap();
        assert_eq!(resumed.offset, 9);
        resumed.append(&bytes[9..], a.size_bytes).unwrap();
        resumed.finish(&store, Some(&a), "unused").unwrap();
        assert!(verified(&store, &a).unwrap());
        assert!(!staging.join(&a.sha256).exists());
        // A later operation reuses only the full verified CAS artifact.
        std::fs::write(root.path().join("cas/blobs").join(&a.sha256), b"changed").unwrap();
        assert!(verified(&store, &a).is_err());
    }
    #[test]
    fn checkpoint_copy_progress_completes_an_eight_mib_vm_chunk_across_restarts() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        create_private(&staging).unwrap();
        let store = CheckpointStore::open(root.path().join("cas"), BLOB_LIMIT).unwrap();
        let bytes: Vec<u8> = (0..8 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let a = reference(&bytes);
        for end in [3 * 1024 * 1024, 6 * 1024 * 1024, bytes.len()] {
            let mut part = StagedPart::open(&staging, Some(&a)).unwrap();
            let start = part.offset as usize;
            assert!(start < end);
            for chunk in bytes[start..end].chunks(CHUNK) {
                part.append(chunk, a.size_bytes).unwrap();
            }
            assert_eq!(part.offset as usize, end);
            assert!(
                !verified(&store, &a).unwrap(),
                "partial files are never CAS copies"
            );
        }
        let mut part = StagedPart::open(&staging, Some(&a)).unwrap();
        assert_eq!(part.offset, a.size_bytes);
        part.finish(&store, Some(&a), "unused").unwrap();
        let mut restored = Vec::new();
        store
            .open_blob(&a)
            .unwrap()
            .read_to_end(&mut restored)
            .unwrap();
        assert_eq!(restored, bytes);
    }
    #[test]
    fn checkpoint_copy_progress_rejects_aliases_changes_and_corrupt_prefixes() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        create_private(&staging).unwrap();
        let store = CheckpointStore::open(root.path().join("cas"), BLOB_LIMIT).unwrap();
        let a = reference(b"abcd");
        let mut part = StagedPart::open(&staging, Some(&a)).unwrap();
        part.append(b"xx", a.size_bytes).unwrap();
        drop(part);
        let mut part = StagedPart::open(&staging, Some(&a)).unwrap();
        part.append(b"cd", a.size_bytes).unwrap();
        assert!(part.finish(&store, Some(&a), "unused").is_err());
        assert!(!staging.join(&a.sha256).exists());
        assert!(!verified(&store, &a).unwrap());
        let mut part = StagedPart::open(&staging, Some(&a)).unwrap();
        std::fs::hard_link(staging.join(&a.sha256), root.path().join("alias")).unwrap();
        assert!(part.append(b"a", a.size_bytes).is_err());
        drop(part);
        std::fs::remove_file(staging.join(&a.sha256)).unwrap();
        std::os::unix::fs::symlink(root.path().join("alias"), staging.join(&a.sha256)).unwrap();
        assert!(StagedPart::open(&staging, Some(&a)).is_err());
    }
    #[test]
    fn checkpoint_copy_progress_isolates_checkpoint_directories_and_verifies_manifest() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        create_private(&first).unwrap();
        let second = root.path().join("second");
        create_private(&second).unwrap();
        let store = CheckpointStore::open(root.path().join("cas"), BLOB_LIMIT).unwrap();
        let bytes = b"manifest bytes";
        let a = reference(bytes);
        let mut part = StagedPart::open(&first, None).unwrap();
        part.append(&bytes[..4], a.size_bytes).unwrap();
        drop(part);
        assert_eq!(StagedPart::open(&second, None).unwrap().offset, 0);
        let mut part = StagedPart::open(&first, None).unwrap();
        assert_eq!(part.offset, 4);
        part.append(&bytes[4..], a.size_bytes).unwrap();
        assert_eq!(part.finish(&store, None, &a.sha256).unwrap(), bytes);
        // Retain a complete manifest too: the next native request probes its
        // end offset under fresh source authority and rehashes these bytes.
        // A large manifest must not consume the fetch budget again per call.
        let mut cached = StagedPart::open(&first, None).unwrap();
        assert_eq!(cached.offset, bytes.len() as u64);
        assert_eq!(cached.finish(&store, None, &a.sha256).unwrap(), bytes);
    }
}
