// Origin: CTOX
// License: AGPL-3.0-only
//! Protected import under the actual target controller, before a Core/provider exists.
//! The completed import is preparation, never permission to activate the guest.
use super::target_enrollment::ProtectedEnrollment;
use super::target_import::NativeGuestImportFence;
use super::*;
use ctox_sync::{authority::auth::SigningIdentity, checkpoint::CheckpointStore};

pub(super) struct ProtectedImport<'a> {
    pub(super) registry: Arc<NativeGuestRegistry>,
    pub(super) guest_id: String,
    pub(super) protected: ProtectedEnrollment,
    pub(super) fence: &'a dyn NativeGuestImportFence,
}

impl ProtectedImport<'_> {
    fn request(&self, guest: &str, spec: &ExecutionSpec, ownership: &Ownership) -> Result<()> {
        ensure!(
            guest == self.guest_id
                && *spec == self.protected.spec
                && *ownership == self.protected.ownership,
            "foreign protected guest import"
        );
        Ok(())
    }

    fn with_current<T>(
        &self,
        action: impl FnOnce(&mut Registration, &dyn Fn() -> Result<()>) -> Result<T>,
    ) -> Result<T> {
        self.with_current_machine(false, action)
    }

    pub(super) fn with_current_machine<T>(
        &self,
        restoring: bool,
        action: impl FnOnce(&mut Registration, &dyn Fn() -> Result<()>) -> Result<T>,
    ) -> Result<T> {
        self.registry
            .verify_runtime_root(&self.registry.runtime_root)?;
        crate::sync_host::with_current_signing_identity(&self.registry.runtime_root, |identity| {
            self.registry.with_policy(|policy| {
                let registration = self.registry.registration(&self.guest_id)?;
                let mut entry = registration
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                ensure!(
                    !entry.revoked
                        && entry.restoration.as_ref() == Some(&self.protected)
                        && entry.provider.is_none()
                        && entry.execution.is_none(),
                    "protected target controller changed or execution already exists"
                );
                if restoring {
                    let imported = entry
                        .imported
                        .as_ref()
                        .context("target has no registered import")?;
                    ensure!(
                        entry.publication == PublicationState::Published
                            && imported.spec == self.protected.spec
                            && imported.ownership == self.protected.ownership
                            && imported.checkpoint_digest == self.protected.checkpoint_digest
                            && imported.destination == entry.assignment.destination
                            && entry.imported_identity.as_ref()
                                == Some(&private_directory(&imported.imported_directory)?),
                        "protected target import identity changed"
                    );
                } else {
                    ensure!(
                        entry.process_effect.is_none() && entry.registered_process.is_none(),
                        "protected target process already exists"
                    );
                    #[cfg(target_os = "linux")]
                    ensure!(
                        entry.desktop.is_none()
                            && entry.source_machine.is_none()
                            && entry.target_machine.is_none(),
                        "protected target already has a retained process"
                    );
                }
                let destination = entry.assignment.destination.clone();
                let import_identity = entry.import_identity.clone();
                let scope = &self.protected.scope;
                let verify = || {
                    self.registry.require_live_transport()?;
                    ensure!(
                        self.registry.authority.node_id() == self.protected.ownership.node_id
                            && self.registry.authority.scope_id() == self.protected.spec.scope_id,
                        "protected target authority changed"
                    );
                    ensure!(
                        destination.human_owner_id == scope.owner_user_id
                            && destination.project_id == scope.project_id
                            && destination.thread_id == scope.thread_id
                            && destination.worker_profile_id == scope.worker_profile_id
                            && private_directory(&destination.import_parent)? == import_identity,
                        "protected target assignment/import parent changed"
                    );
                    validate_policy(policy, &destination)?;
                    let current = target_handoff::resolve(
                        &self.registry.runtime_root,
                        policy,
                        scope,
                        &self.protected.spec,
                    )?;
                    ensure!(
                        current == *scope,
                        "protected target native entitlement changed"
                    );
                    Ok(())
                };
                verify()?;
                let mut action = Some(action);
                let mut result = None;
                self.fence.with_current_checkpoint(
                    policy,
                    identity,
                    &destination,
                    &self.protected.binding_digest,
                    &self.protected.checkpoint_digest,
                    &self.protected.spec,
                    &self.protected.ownership,
                    &mut || {
                        let action = action.take().ok_or_else(|| {
                            io::Error::other("protected import fence invoked twice")
                        })?;
                        result = Some(action(&mut entry, &verify).map_err(io_error)?);
                        verify().map_err(io_error)?;
                        Ok(())
                    },
                )?;
                result.context("protected import fence did not publish")
            })
        })
    }
}

impl GuestRestoreOwner for ProtectedImport<'_> {
    fn resolve_destination(
        &self,
        guest: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> io::Result<GuestRestoreDestination> {
        self.request(guest, spec, ownership).map_err(io_error)?;
        self.with_current(|entry, verify| {
            verify()?;
            Ok(entry.assignment.destination.clone())
        })
        .map_err(io_error)
    }

    fn with_current_fence(
        &self,
        expected: &GuestRestoreDestination,
        spec: &ExecutionSpec,
        ownership: &Ownership,
        publish: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.request(&expected.guest_id, spec, ownership)
            .map_err(io_error)?;
        self.with_current(|entry, verify| {
            ensure!(
                entry.assignment.destination == *expected
                    && entry.publication == PublicationState::Virgin
                    && entry.imported.is_none(),
                "protected target import already attempted or destination changed; reconcile"
            );
            entry.publication = PublicationState::Uncertain;
            verify()?;
            publish()?;
            entry.publication = PublicationState::Published;
            Ok(())
        })
        .map_err(io_error)
    }
}

impl NativeGuestRegistry {
    pub(super) async fn import_protected_checkpoint(
        self: &Arc<Self>,
        guest_id: &str,
        spec: &ExecutionSpec,
        source: &Ownership,
        store: &CheckpointStore,
        digest: &str,
        protected: ProtectedEnrollment,
        fence: &dyn NativeGuestImportFence,
    ) -> Result<GuestImportReceipt> {
        ensure!(
            *spec == protected.spec
                && digest == protected.checkpoint_digest
                && protected.ownership.node_id == self.authority.node_id()
                && source.node_id != protected.ownership.node_id
                && source.generation.checked_add(1) == Some(protected.ownership.generation),
            "protected import differs from the actual completed target takeover"
        );
        let owner = ProtectedImport {
            registry: Arc::clone(self),
            guest_id: guest_id.into(),
            protected,
            fence,
        };
        let staged = ctox_sync::guest_restore::stage_guest_restore(
            store,
            self.authority.as_ref(),
            &owner,
            guest_id,
            &spec.job_id,
            owner.protected.ownership.clone(),
            digest,
        )
        .await?;
        let receipt =
            ctox_sync::guest_restore::commit_guest_restore(self.authority.as_ref(), &owner, staged)
                .await?;
        NativeGuestExecution::validate_import_completion(
            self.authority.as_ref(),
            spec,
            &owner.protected.ownership,
            &receipt,
        )
        .await?;
        owner.with_current(|entry, verify| {
            NativeGuestExecution::register_import_current(
                entry,
                verify,
                receipt.clone(),
                spec,
                &owner.protected.ownership,
            )
        })?;
        Ok(receipt)
    }
}
