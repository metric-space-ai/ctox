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
    ) -> Result<(ctox_core::NativeSessionState, PathBuf)>;
}

/// One consumed native constructor attempt; never deserialized from a request.
pub(crate) struct NativeGuestCoreResume {
    registry: Arc<NativeGuestRegistry>,
    guest_id: String,
    protected: ProtectedEnrollment,
    owner: Arc<dyn NativeGuestCoreOwner>,
    state: ctox_core::NativeSessionState,
    journal: PathBuf,
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
            let (state, journal) = self.owner.read_state(imported)?;
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
                self.journal.clone(),
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
                let (state, journal) = self.owner.read_state(
                    entry
                        .imported
                        .as_ref()
                        .context("original import disappeared after Core startup")?,
                )?;
                ensure!(
                    Sha256::digest(state.as_bytes()) == state_digest && journal == self.journal,
                    "protected original Core input changed during startup"
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
            if entry.restoration.is_none() {
                return Ok(None);
            }
            let d = &entry.assignment.destination;
            ensure!(
                !entry.revoked && context["actor"].as_str() == Some(d.human_owner_id.as_str()),
                "original Core workspace belongs to another principal"
            );
            validate_policy(policy, d)?;
            let assignment =
                workspaces::snapshot(policy, d)?.context("target workspace is not assigned")?;
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
        let (state, journal) = ProtectedImport {
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
            owner.read_state(entry.imported.as_ref().context("original import absent")?)
        })?;
        Ok(Some(NativeGuestCoreResume {
            registry: self.clone(),
            guest_id: guest.into(),
            protected,
            owner,
            state,
            journal,
            workspace: workspace.to_path_buf(),
        }))
    }
}
