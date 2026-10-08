// Origin: CTOX
// License: AGPL-3.0-only
//! Retain the actual protected receiver for the target's original Core factory.
use super::super::super::super::guest_registry::{
    core_resume::NativeGuestCoreOwner, target_import::NativeGuestImportFence,
};
use super::*;
use ctox_sync::guest_restore::GuestImportReceipt;
use sha2::{Digest, Sha256};

pub(super) struct ReceiverCore<P> {
    pub(super) target: Arc<Target<P>>,
    /// Ownership moves out of the bounded import RPC into the retained native
    /// controller. Host/account/policy retirement still invalidates every use.
    pub(super) lifetime: CopyLifetime,
}

impl<P: Clone + Eq + Hash + Send + Sync + 'static> NativeGuestCoreOwner for ReceiverCore<P> {
    fn fence(&self) -> &dyn NativeGuestImportFence {
        self.target.as_ref()
    }

    fn read_state(
        &self,
        imported: &GuestImportReceipt,
    ) -> anyhow::Result<(ctox_core::NativeSessionState, PathBuf)> {
        let t = &self.target;
        let _retained_lifetime = &self.lifetime;
        anyhow::ensure!(
            imported.spec == t.request.spec
                && imported.checkpoint_digest == t.request.checkpoint_digest,
            "original Core import differs from the protected receiver"
        );
        let root = t
            .server
            .gate
            .root
            .join("runtime/ctox-sync/received-checkpoints");
        private_dir(&root)?;
        let store = CheckpointStore::open(root, BLOB_LIMIT)?;
        // The native registered import already verified the complete copy.
        // Recheck the immutable manifest and the Core inputs actually consumed
        // here; do not rehash multi-GiB VM RAM/disk under publication locks.
        let manifest = store.load_manifest(&t.request.checkpoint_digest)?;
        reconstruction::verify_manifest(&manifest, &t.request)?;
        let states: Vec<_> = manifest
            .provider_state
            .iter()
            .filter(|entry| {
                entry.path == "native-session-state.json"
                    && entry.kind == ctox_sync::contracts::WorkspaceEntryKind::File
            })
            .collect();
        anyhow::ensure!(
            states.len() == 1 && manifest.history.len() == 1,
            "original Core state/history absent or ambiguous"
        );
        let mut bytes = Vec::new();
        store
            .open_blob(&states[0].artifact)?
            .take(BLOB_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() as u64 <= BLOB_LIMIT,
            "original Core state exceeds its bound"
        );
        let state = ctox_core::NativeSessionState::from_checkpoint(
            &bytes,
            ctox_protocol::ThreadId::from_string(&t.request.spec.session_id)?,
            &t.request.spec.model_id,
            &t.request.spec.model_route_id,
        )?;
        let journal = imported
            .imported_directory
            .join("history")
            .join(&manifest.history[0].sha256);
        verify_journal_file(&journal, &manifest.history[0])?;
        Ok((state, journal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn original_journal_rejects_mutation_symlink_hardlink_and_public_permissions() {
        let root = tempfile::tempdir().unwrap();
        let journal = root.path().join("original");
        let bytes = b"protected original journal";
        std::fs::write(&journal, bytes).unwrap();
        std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o600)).unwrap();
        let artifact = ArtifactRef {
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
        };
        verify_journal_file(&journal, &artifact).unwrap();
        let link = root.path().join("link");
        symlink(&journal, &link).unwrap();
        assert!(verify_journal_file(&link, &artifact).is_err());
        let hard = root.path().join("hard");
        std::fs::hard_link(&journal, &hard).unwrap();
        assert!(verify_journal_file(&journal, &artifact).is_err());
        std::fs::remove_file(hard).unwrap();
        std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(verify_journal_file(&journal, &artifact).is_err());
        std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&journal, vec![b'x'; bytes.len()]).unwrap();
        assert!(verify_journal_file(&journal, &artifact).is_err());
        assert!(verify_journal_file(&root.path().join("missing"), &artifact).is_err());
    }
}

fn verify_journal_file(path: &Path, artifact: &ArtifactRef) -> anyhow::Result<()> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0
            && metadata.nlink() == 1
            && metadata.len() == artifact.size_bytes
            && artifact.size_bytes <= BLOB_LIMIT
            && std::fs::canonicalize(path)? == path,
        "original Core journal is not the private imported artifact"
    );
    let mut bytes = Vec::new();
    file.take(BLOB_LIMIT + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 == artifact.size_bytes
            && format!("{:x}", Sha256::digest(&bytes)) == artifact.sha256,
        "original Core journal differs from the protected checkpoint"
    );
    Ok(())
}
