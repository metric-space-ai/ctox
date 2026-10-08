// Origin: CTOX
// License: AGPL-3.0-only
//! Consume the host's retained execution owner; payloads cannot mint one.
use super::*;
use ctox_sync::authority::auth::SigningIdentity;
use ctox_sync::checkpoint::CheckpointStore;

/// The protected copy owner holds account, handoff policy and host lifetime
/// through the callback. It borrows the existing native policy transaction:
/// reopening it here would deadlock or split the publication decision.
pub(crate) trait NativeGuestImportFence: Send + Sync {
    fn with_current(
        &self,
        policy: &Connection,
        identity: &SigningIdentity,
        destination: &GuestRestoreDestination,
        publish: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()>;
}

struct BoundImport<'a> {
    execution: &'a NativeGuestExecution,
    fence: &'a dyn NativeGuestImportFence,
}
impl BoundImport<'_> {
    fn with_current<T>(
        &self,
        action: impl FnOnce(&mut Registration, &dyn Fn() -> Result<()>) -> Result<T>,
    ) -> Result<T> {
        // Issuer -> worker/account -> policy -> controller. The existing
        // secret mutation fence must precede worker/policy locks; its callback
        // neither initializes credentials nor re-enters secret APIs.
        crate::sync_host::with_current_signing_identity(
            &self.execution.registry.runtime_root,
            |identity| {
                self.execution
                    .provider
                    .with_live_provider_transaction(|worker, facts, _| {
                        self.execution.with_held_worker_policy(
                            worker,
                            facts,
                            |entry, verify, policy| {
                                let destination = entry.assignment.destination.clone();
                                let mut action = Some(action);
                                let mut result = None;
                                self.fence.with_current(
                                    policy,
                                    identity,
                                    &destination,
                                    &mut || {
                                        let action = action.take().ok_or_else(|| {
                                            io::Error::other("guest import fence invoked twice")
                                        })?;
                                        result = Some(action(entry, verify).map_err(io_error)?);
                                        Ok(())
                                    },
                                )?;
                                result.context("guest import fence did not publish")
                            },
                        )
                    })
            },
        )
    }
}
impl GuestRestoreOwner for BoundImport<'_> {
    fn resolve_destination(
        &self,
        guest_id: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> io::Result<GuestRestoreDestination> {
        ensure_request(self.execution, guest_id, spec, ownership)?;
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
        ensure_request(self.execution, &expected.guest_id, spec, ownership)?;
        self.with_current(|entry, verify| {
            ensure!(
                entry.assignment.destination == *expected,
                "native guest destination changed"
            );
            ensure!(
                entry.publication == PublicationState::Virgin,
                "native import already attempted; reconcile"
            );
            // Admission failure/cancellation stays uncertain. File presence
            // cannot authorize another publication or complete another effect.
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
    /// An actual retained producer and binding, never renderer-owned claims.
    fn retained_execution(self: &Arc<Self>, guest_id: &str) -> Result<NativeGuestExecution> {
        let registration = self.registration(guest_id)?;
        let entry = registration
            .lock()
            .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
        ensure!(!entry.revoked, "native controller revoked");
        Ok(NativeGuestExecution {
            registry: self.clone(),
            guest_id: guest_id.into(),
            provider: entry
                .provider
                .clone()
                .context("guest has no retained provider")?,
            binding: entry
                .execution
                .clone()
                .context("guest has no admitted execution")?,
        })
    }

    pub(crate) async fn import_received_checkpoint(
        self: &Arc<Self>,
        guest_id: &str,
        spec: &ExecutionSpec,
        source_ownership: &Ownership,
        store: &CheckpointStore,
        digest: &str,
        fence: &dyn NativeGuestImportFence,
    ) -> Result<GuestImportReceipt> {
        let execution = self.retained_execution(guest_id)?;
        ensure!(
            execution.binding.spec == *spec,
            "guest differs from enrolled execution"
        );
        let ownership = &execution.binding.ownership;
        ensure!(
            ownership.node_id == self.authority.node_id()
                && ownership.node_id != source_ownership.node_id
                && source_ownership.generation.checked_add(1) == Some(ownership.generation),
            "guest import requires the actual next target ownership"
        );
        self.require_live_transport()?;
        let owner = BoundImport {
            execution: &execution,
            fence,
        };
        let staged = ctox_sync::guest_restore::stage_guest_restore(
            store,
            self.authority.as_ref(),
            &owner,
            guest_id,
            &spec.job_id,
            ownership.clone(),
            digest,
        )
        .await?;
        let receipt =
            ctox_sync::guest_restore::commit_guest_restore(self.authority.as_ref(), &owner, staged)
                .await?;
        execution.validate_import_completion(&receipt).await?;
        // Fresh combined fence after completion: revocation during the await
        // must deny registration, not merely the eventual response.
        owner.with_current(|entry, verify| {
            self.require_live_transport()?;
            execution.register_import_current(entry, verify, receipt.clone())
        })?;
        Ok(receipt)
    }
}
