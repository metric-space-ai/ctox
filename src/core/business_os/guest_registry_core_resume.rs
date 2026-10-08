// Origin: CTOX
// License: AGPL-3.0-only
//! Original Core construction from the retained protected import owner.
use super::protected_import::ProtectedImport;
use super::target_enrollment::ProtectedEnrollment;
use super::target_import::NativeGuestImportFence;
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Retained only by the actual protected receiver. A guest ID or imported JSON
/// cannot create this capability, and returning from its IPC must not retire it.
pub(in crate::business_os) trait NativeGuestCoreOwner: Send + Sync {
    fn fence(&self) -> &dyn NativeGuestImportFence;
    fn read_state(
        &self,
        imported: &GuestImportReceipt,
    ) -> Result<(
        ctox_core::NativeSessionState,
        PathBuf,
        ctox_sync::contracts::ArtifactRef,
    )>;
}

/// One consumed native constructor attempt; never deserialized from a request.
pub(crate) struct NativeGuestCoreResume {
    registry: Arc<NativeGuestRegistry>,
    guest_id: String,
    protected: ProtectedEnrollment,
    owner: Arc<dyn NativeGuestCoreOwner>,
    state: ctox_core::NativeSessionState,
    journal: PathBuf,
    working_journal: PathBuf,
    working_identity: FileIdentity,
    workspace: PathBuf,
}

impl NativeGuestCoreResume {
    fn current(&self, job: Option<&ctox_sync::authority::Job>) -> Result<()> {
        let owner = ProtectedImport {
            registry: self.registry.clone(),
            guest_id: self.guest_id.clone(),
            protected: self.protected.clone(),
            fence: self.owner.fence(),
        };
        owner.with_current_machine(true, |entry, verify| {
            ensure!(
                entry.core_start_attempted,
                "original Core constructor was not reserved"
            );
            let imported = entry
                .imported
                .as_ref()
                .context("original import disappeared")?;
            if let Some(job) = job {
                validate_original_job(entry, &self.protected, job)?;
            }
            let (state, journal, _) = self.owner.read_state(imported)?;
            ensure!(
                entry.core_journal.as_ref() == Some(&self.working_journal)
                    && identity(&self.working_journal)? == self.working_identity,
                "target Core working journal changed"
            );
            ensure!(
                state.as_bytes() == self.state.as_bytes() && journal == self.journal,
                "protected original Core input changed"
            );
            verify()
        })
    }

    pub(crate) async fn load(
        self,
        manager: &ctox_core::ThreadManager,
        config: ctox_core::config::Config,
        auth: Arc<ctox_core::AuthManager>,
    ) -> Result<ctox_core::NewThread> {
        ensure!(
            config.cwd.as_path() == self.workspace
                && auth.runtime_account_binding()
                    == Some(self.protected.spec.gateway_account_id.as_str()),
            "target Core workspace or independently configured account differs"
        );
        self.current(None)?;
        let before = self
            .registry
            .authority
            .validate_ownership(&self.protected.spec.job_id, &self.protected.ownership)
            .await?;
        ensure!(
            before.spec == self.protected.spec
                && before.ownership == self.protected.ownership
                && !before.stopped,
            "original target ownership is stale"
        );
        self.current(Some(&before))?;
        let state_digest = Sha256::digest(self.state.as_bytes());
        // No native/SQLite/account lock survives Core startup. Only this actual
        // target manager can load the original UUID; there is no fresh-thread fallback.
        let loaded = manager
            .resume_thread_from_native_checkpoint(
                config,
                self.working_journal.clone(),
                auth.clone(),
                self.state,
            )
            .await?;
        let after = self
            .registry
            .authority
            .validate_ownership(&self.protected.spec.job_id, &self.protected.ownership)
            .await;
        let validation = (|| -> Result<()> {
            let _account = auth.current_runtime_account_guard()?;
            ensure!(
                after? == before,
                "original target quorum changed during Core startup"
            );
            let owner = ProtectedImport {
                registry: self.registry.clone(),
                guest_id: self.guest_id.clone(),
                protected: self.protected.clone(),
                fence: self.owner.fence(),
            };
            owner.with_current_machine(true, |entry, verify| {
                ensure!(
                    entry.core_start_attempted
                        && loaded.thread_id.to_string() == self.protected.spec.session_id,
                    "target did not load the original Core Session"
                );
                validate_original_job(entry, &self.protected, &before)?;
                let (state, journal, _) = self.owner.read_state(
                    entry
                        .imported
                        .as_ref()
                        .context("original import disappeared after Core startup")?,
                )?;
                ensure!(
                    Sha256::digest(state.as_bytes()) == state_digest && journal == self.journal,
                    "protected original Core input changed during startup"
                );
                ensure!(
                    entry.core_journal.as_ref() == Some(&self.working_journal)
                        && identity(&self.working_journal)? == self.working_identity,
                    "target Core working journal changed during startup"
                );
                verify()
            })
        })();
        if let Err(error) = validation {
            // This constructor owns only this actual Core, never another worker
            // or host service. Failed validation publishes no model turn.
            let _ = loaded
                .thread
                .submit(ctox_protocol::protocol::Op::Shutdown)
                .await;
            return Err(error);
        }
        Ok(loaded)
    }
}

fn stage_core_journal(
    source: &Path,
    artifact: &ctox_sync::contracts::ArtifactRef,
    parent: &Path,
) -> Result<PathBuf> {
    use std::io::{Read, Write};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    private_directory(parent)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0
            && metadata.nlink() == 1
            && metadata.len() == artifact.size_bytes
            && artifact.size_bytes <= 64 * 1024 * 1024
            && std::fs::canonicalize(source)? == source,
        "original journal is not a private bounded file"
    );
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == artifact.size_bytes
            && format!("{:x}", Sha256::digest(&bytes)) == artifact.sha256,
        "original journal changed before target Core staging"
    );
    let directory = tempfile::Builder::new()
        .prefix("core-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)?;
    let path = directory.path().join("journal.jsonl");
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    std::fs::File::open(directory.path())?.sync_all()?;
    // Retain before Core startup can append. Failed or cancelled construction
    // leaves owned evidence for reconciliation instead of deleting a live writer.
    directory.keep();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_core_appends_only_to_its_own_verified_journal() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let parent = tempfile::tempdir().unwrap();
        std::fs::set_permissions(parent.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let original = parent.path().join("protected-history");
        let bytes = b"original protected journal\\n";
        std::fs::write(&original, bytes).unwrap();
        std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o600)).unwrap();
        let artifact = ctox_sync::contracts::ArtifactRef {
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
        };
        let working = stage_core_journal(&original, &artifact, parent.path()).unwrap();
        assert_ne!(original, working);
        assert_eq!(std::fs::read(&working).unwrap(), bytes);
        std::fs::OpenOptions::new()
            .append(true)
            .open(&working)
            .unwrap()
            .write_all(b"target Core event\\n")
            .unwrap();
        assert_eq!(std::fs::read(&original).unwrap(), bytes);
        assert_ne!(std::fs::read(&working).unwrap(), bytes);
        std::fs::write(&original, b"changed").unwrap();
        assert!(stage_core_journal(&original, &artifact, parent.path()).is_err());
    }
}

fn validate_original_job(
    entry: &Registration,
    protected: &ProtectedEnrollment,
    job: &ctox_sync::authority::Job,
) -> Result<()> {
    let imported = entry
        .imported
        .as_ref()
        .context("original import is absent")?;
    ensure!(
        job.spec == protected.spec
            && job.ownership == protected.ownership
            && !job.stopped
            && job.completed_effects.contains(&imported.effect_id)
            && job
                .checkpoint
                .as_ref()
                .is_some_and(|checkpoint| checkpoint.digest == protected.checkpoint_digest),
        "original target job/import/checkpoint is stale"
    );
    let pending = match (&entry.process_effect, &entry.registered_process) {
        (None, None) => BTreeSet::new(),
        (Some(effect), Some(process)) => {
            ensure!(
                process.effect_id == *effect
                    && process.job_id == job.spec.job_id
                    && process.ownership == job.ownership
                    && process.controller_id == entry.assignment.destination.controller_id
                    && process.controller_generation
                        == entry.assignment.destination.controller_generation,
                "original target process differs from current ownership"
            );
            BTreeSet::from([effect.clone()])
        }
        _ => anyhow::bail!("original target process effect is incomplete"),
    };
    ensure!(
        job.pending_effects == pending,
        "original target has unknown or foreign pending effects"
    );
    Ok(())
}

impl NativeGuestRegistry {
    pub(in crate::business_os) fn retain_core_owner(
        self: &Arc<Self>,
        guest: &str,
        owner: Arc<dyn NativeGuestCoreOwner>,
    ) -> Result<()> {
        let protected = self
            .registration(guest)?
            .lock()
            .map_err(|_| anyhow::anyhow!("native controller poisoned"))?
            .restoration
            .clone()
            .context("guest is not a protected target")?;
        ProtectedImport {
            registry: self.clone(),
            guest_id: guest.into(),
            protected,
            fence: owner.fence(),
        }
        .with_current_machine(true, |entry, verify| {
            ensure!(
                entry.core_owner.is_none() && !entry.core_start_attempted,
                "original Core owner already retained or attempted"
            );
            verify()?;
            entry.core_owner = Some(owner.clone());
            Ok(())
        })
    }

    pub(crate) fn continuation_workspace(
        &self,
        guest: &str,
        context: &Value,
    ) -> Result<Option<PathBuf>> {
        let registration = self.registration(guest)?;
        self.with_policy(|policy| {
            let entry = registration
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            let d = &entry.assignment.destination;
            ensure!(
                !entry.revoked
                    && context["actor"].as_str() == Some(d.human_owner_id.as_str())
                    && context["expires_at_ms"]
                        .as_u64()
                        .is_some_and(|expiry| u128::from(expiry) > super::super::store::now_ms()),
                "native Core workspace principal or command lifetime is not current"
            );
            validate_policy(policy, d)?;
            let Some(assignment) = workspaces::snapshot(policy, d)? else {
                ensure!(
                    entry.restoration.is_none(),
                    "target workspace is not assigned"
                );
                // Ordinary, unassigned source chats retain their caller cwd;
                // they acquire no portable workspace/export authority.
                return Ok(None);
            };
            let path = PathBuf::from(
                assignment["nativeWorkspace"]
                    .as_str()
                    .context("target workspace absent")?,
            );
            workspaces::require(policy, d, &path)?.verify()?;
            Ok(Some(path))
        })
    }

    pub(crate) fn prepare_core_resume(
        self: &Arc<Self>,
        guest: &str,
        context: &Value,
        model: &str,
        contract: &crate::channels::NativeProviderCheckpointContract,
        workspace: &Path,
    ) -> Result<Option<NativeGuestCoreResume>> {
        self.authorize_provider_start(guest, context, model, contract)?;
        let registration = self.registration(guest)?;
        let (protected, owner) = {
            let entry = registration
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            let Some(protected) = &entry.restoration else {
                return Ok(None);
            };
            (
                protected.clone(),
                entry
                    .core_owner
                    .clone()
                    .context("original Core receiver owner absent")?,
            )
        };
        let (state, journal, working_journal, working_identity) = ProtectedImport {
            registry: self.clone(),
            guest_id: guest.into(),
            protected: protected.clone(),
            fence: owner.fence(),
        }
        .with_current_machine(true, |entry, verify| {
            ensure!(
                !entry.core_start_attempted
                    && protected.spec.model_id == model
                    && protected.spec.harness == contract.harness
                    && protected.spec.harness_version == contract.harness_version
                    && protected.spec.model_route_id == contract.model_route_id
                    && protected.spec.gateway_account_id == contract.gateway_account_id,
                "original Core constructor already attempted or provider differs"
            );
            // Burn the single attempt before reading protected input. Error,
            // cancellation and ambiguous startup require native reconciliation.
            entry.core_start_attempted = true;
            verify()?;
            let (state, journal, artifact) =
                owner.read_state(entry.imported.as_ref().context("original import absent")?)?;
            let working_journal = stage_core_journal(
                &journal,
                &artifact,
                &entry.assignment.destination.import_parent,
            )?;
            let working_identity = identity(&working_journal)?;
            entry.core_journal = Some(working_journal.clone());
            verify()?;
            Ok((state, journal, working_journal, working_identity))
        })?;
        Ok(Some(NativeGuestCoreResume {
            registry: self.clone(),
            guest_id: guest.into(),
            protected,
            owner,
            state,
            journal,
            working_journal,
            working_identity,
            workspace: workspace.to_path_buf(),
        }))
    }
}
