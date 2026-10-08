// Origin: CTOX
// License: AGPL-3.0-only
//! Original Owned-job admission from the actual protected receiver.
//! No new Create, guest, Core UUID or transfer-job authority is introduced.
use super::core_resume::NativeGuestCoreOwner;
use super::target_enrollment::ProtectedEnrollment;
use super::*;
use crate::channels::{NativeProviderAdmission, NativeProviderCommand};
use ctox_sync::authority::{auth::SigningIdentity, Job};
use std::{future::Future, pin::Pin};

pub(super) struct TargetAdmission {
    registry: Arc<NativeGuestRegistry>,
    guest_id: String,
    protected: ProtectedEnrollment,
    owner: Arc<dyn NativeGuestCoreOwner>,
}
impl TargetAdmission {
    pub(super) fn for_guest(
        registry: &Arc<NativeGuestRegistry>,
        guest_id: &str,
    ) -> Result<Option<Self>> {
        let registration = registry.registration(guest_id)?;
        let entry = registration
            .lock()
            .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
        let Some(protected) = entry.restoration.clone() else {
            return Ok(None);
        };
        ensure!(
            entry.core_ready && entry.core_start_attempted && !entry.revoked,
            "protected target requires verified original Core construction, never fresh Create"
        );
        let owner = entry
            .core_owner
            .clone()
            .context("original receiver owner missing")?;
        Ok(Some(Self {
            registry: registry.clone(),
            guest_id: guest_id.into(),
            protected,
            owner,
        }))
    }

    pub(super) fn with_fenced_entry<T>(
        registry: &NativeGuestRegistry,
        policy: &Connection,
        identity: Option<&SigningIdentity>,
        entry: &mut Registration,
        apply: impl FnOnce(&mut Registration) -> Result<T>,
    ) -> Result<T> {
        let protected = entry
            .restoration
            .clone()
            .context("original target enrollment missing")?;
        let owner = entry
            .core_owner
            .clone()
            .context("original receiver owner missing")?;
        let identity = identity.context("target requires the held native issuer fence")?;
        ensure!(
            entry.core_ready
                && entry.core_start_attempted
                && !entry.revoked
                && entry.publication == PublicationState::Published,
            "original target Core/import is not current"
        );
        let imported = entry.imported.as_ref().context("original import missing")?;
        ensure!(
            imported.spec == protected.spec
                && imported.ownership == protected.ownership
                && imported.checkpoint_digest == protected.checkpoint_digest
                && imported.destination == entry.assignment.destination
                && entry.imported_identity.as_ref()
                    == Some(&private_directory(&imported.imported_directory)?),
            "original protected import changed"
        );
        let journal = entry
            .core_journal
            .as_ref()
            .context("original Core journal missing")?;
        let metadata = std::fs::symlink_metadata(journal)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == unsafe { libc::geteuid() }
                && metadata.mode() & 0o077 == 0
                && metadata.nlink() == 1
                && std::fs::canonicalize(journal)? == *journal
                && entry.core_journal_identity.as_ref() == Some(&identity_of_journal(journal)?),
            "original Core working journal was replaced or exposed"
        );
        let destination = entry.assignment.destination.clone();
        ensure!(
            registry.authority.node_id() == protected.ownership.node_id
                && registry.authority.scope_id() == protected.spec.scope_id
                && private_directory(&destination.import_parent)? == entry.import_identity,
            "original target native authority or import parent changed"
        );
        registry.require_live_transport()?;
        validate_policy(policy, &destination)?;
        ensure!(
            target_handoff::resolve(
                &registry.runtime_root,
                policy,
                &protected.scope,
                &protected.spec
            )? == protected.scope,
            "original target entitlement changed"
        );
        #[cfg(not(target_os = "linux"))]
        anyhow::bail!("original target admission requires a restored Linux guest");
        #[cfg(target_os = "linux")]
        {
            let machine = entry
                .target_machine
                .clone()
                .context("original machine is not restored")?;
            let imported = entry.imported.as_ref().context("original import missing")?;
            let process = entry
                .registered_process
                .as_ref()
                .context("original process missing")?;
            machine.verify_ready(
                imported,
                process,
                &protected.service_session,
                entry.desktop.as_mut(),
            )?;
        }
        let mut apply = Some(apply);
        let mut result = None;
        owner.fence().with_current_checkpoint(
            policy,
            identity,
            &destination,
            &protected.binding_digest,
            &protected.checkpoint_digest,
            &protected.spec,
            &protected.ownership,
            &mut || {
                let action = apply
                    .take()
                    .ok_or_else(|| io::Error::other("target fence invoked twice"))?;
                result = Some(action(entry).map_err(io_error)?);
                Ok(())
            },
        )?;
        result.context("original target receiver did not publish")
    }

    fn with_current<T>(
        &self,
        provider: &NativeProviderBinding,
        job: Option<&Job>,
        apply: impl FnOnce(
            &rusqlite::Transaction<'_>,
            &Connection,
            &NativeProviderFacts,
            &mut Registration,
            NativeGuestAdmissionDestination,
        ) -> Result<T>,
    ) -> Result<T> {
        self.registry.verify_runtime_root(provider.runtime_root())?;
        crate::sync_host::with_current_signing_identity(provider.runtime_root(), |identity| {
            provider.with_live_provider_transaction(|worker, facts, turn| {
                ensure!(
                    turn.is_none(),
                    "original job admission must precede the actual Core turn"
                );
                self.registry.with_policy(|policy| {
                    let registration = self.registry.registration(&self.guest_id)?;
                    let mut entry = registration
                        .lock()
                        .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                    ensure!(
                        entry.restoration.as_ref() == Some(&self.protected)
                            && entry
                                .core_owner
                                .as_ref()
                                .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner))
                            && entry.provider.is_none()
                            && entry.execution.is_none(),
                        "original target constructor/producer changed or already bound"
                    );
                    validate_provider(worker, policy, facts, &entry.assignment.destination)?;
                    validate_original_provider(&self.protected.spec, facts)?;
                    let destination = self.registry.admission_destination(policy, &entry)?;
                    Self::with_fenced_entry(
                        &self.registry,
                        policy,
                        Some(identity),
                        &mut entry,
                        |entry| {
                            if let Some(job) = job {
                                core_resume::validate_original_job(entry, &self.protected, job)?;
                            }
                            let result = apply(worker, policy, facts, entry, destination)?;
                            verify_worker_current(worker, facts)?;
                            validate_provider(
                                worker,
                                policy,
                                facts,
                                &entry.assignment.destination,
                            )?;
                            Ok(result)
                        },
                    )
                })
            })
        })
    }

    pub(super) async fn bind_original(
        &self,
        provider: NativeProviderBinding,
        spec: ExecutionSpec,
        ownership: Ownership,
    ) -> Result<NativeGuestExecution> {
        ensure!(
            spec == self.protected.spec && ownership == self.protected.ownership,
            "admitted row differs from the retained original job"
        );
        let job = self
            .registry
            .authority
            .validate_ownership(&spec.job_id, &ownership)
            .await?;
        let binding = self.with_current(
            &provider,
            Some(&job),
            |worker, policy, facts, entry, destination| {
                let (s, o, d): (String, String, String) = worker.query_row(
                    "SELECT spec_json,ownership_json,destination_json
                FROM native_guest_provider_admissions
                WHERE binding_id=?1 AND worker_id=?2 AND attempt_id=?3 AND phase='Admitted'",
                    rusqlite::params![facts.binding_id, facts.worker_id, facts.attempt_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
                ensure!(
                    serde_json::from_str::<ExecutionSpec>(&s)? == spec
                        && serde_json::from_str::<Ownership>(&o)? == ownership
                        && serde_json::from_str::<NativeGuestAdmissionDestination>(&d)?
                            == destination,
                    "original provider admission changed"
                );
                let lease = workspaces::acquire_lease(policy, &entry.assignment.destination)?;
                ensure!(
                    lease.is_some(),
                    "original target workspace requires a current writer lease"
                );
                #[cfg(not(target_os = "linux"))]
                anyhow::bail!("original target continuation requires the real Linux guest");
                #[cfg(target_os = "linux")]
                {
                    ensure!(
                        entry.desktop.is_none()
                            && entry.source_boot.is_none()
                            && entry.source_machine.is_none(),
                        "target cannot replace an existing machine"
                    );
                    let machine = entry
                        .target_machine
                        .clone()
                        .context("original target machine not restored")?;
                    let imported = entry.imported.as_ref().context("original import missing")?;
                    let process = entry
                        .registered_process
                        .as_ref()
                        .context("original target process missing")?;
                    let (desktop, io, _) = machine.take_ready_desktop(imported, process)?;
                    entry.desktop = Some(desktop);
                    entry.desktop_io = Some(io);
                }
                let binding = ExecutionBinding {
                    spec: spec.clone(),
                    ownership: ownership.clone(),
                    provider_binding_id: facts.binding_id.clone(),
                    admission: destination,
                };
                entry.workspace_lease = lease;
                entry.execution = Some(binding.clone());
                entry.provider = Some(provider.clone());
                Ok(binding)
            },
        )?;
        let execution = NativeGuestExecution {
            registry: self.registry.clone(),
            provider,
            guest_id: self.guest_id.clone(),
            binding,
        };
        execution.with_current(|_, verify| verify())?;
        Ok(execution)
    }

    async fn admit_original(&self, provider: NativeProviderBinding) -> Result<()> {
        let destination =
            self.with_current(&provider, None, |_, _, _, _, destination| Ok(destination))?;
        // Reads cannot grant a different job or turn a copied row into authority.
        // No native/worker/account/policy lock survives either await.
        let before = self
            .registry
            .authority
            .validate_ownership(&self.protected.spec.job_id, &self.protected.ownership)
            .await?;
        self.with_current(&provider, Some(&before), |_, _, _, _, current| {
            ensure!(
                current == destination,
                "target destination changed after quorum read"
            );
            Ok(())
        })?;
        let after = self
            .registry
            .authority
            .validate_ownership(&self.protected.spec.job_id, &self.protected.ownership)
            .await?;
        ensure!(
            after == before,
            "original target quorum changed during admission"
        );
        self.with_current(&provider, Some(&after), |worker, _, facts, _, current| {
            ensure!(current == destination, "target destination changed before admitted publication");
            crate::channels::NativeGuestAdmission::ensure_store(worker)?;
            // This is only the current producer's evidence. It never submits Create
            // or changes the already Owned job; replay/duplicates are not retried.
            worker.execute(
                "INSERT INTO native_guest_provider_admissions
                (binding_id,worker_id,attempt_id,request_id,destination_json,spec_json,phase,ownership_json)
                VALUES (?1,?2,?3,?4,?5,?6,'Admitted',?7)",
                rusqlite::params![
                    facts.binding_id, facts.worker_id, facts.attempt_id,
                    format!("original-resume:{}", facts.binding_id),
                    serde_json::to_string(&current)?,
                    serde_json::to_string(&self.protected.spec)?,
                    serde_json::to_string(&self.protected.ownership)?,
                ],
            )?;
            Ok(())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ctox_sync::authority::ProtectedCheckpoint;
    // Registry/worker SQL fixtures; these exercise production validators, not
    // installed Core, target QEMU or remote quorum acceptance.
    #[test]
    fn original_target_provider_cannot_change_session_model_route_or_account() {
        let (root, _, _) = super::super::tests::fixture();
        let (_, facts, _) = super::super::tests::worker_store(root.path());
        let contract = facts.checkpoint_contract.as_ref().unwrap();
        let spec = ExecutionSpec {
            job_id: "original-job".into(),
            session_id: facts.provider_session_id.clone(),
            scope_id: "scope".into(),
            harness: contract.harness.clone(),
            harness_version: contract.harness_version.clone(),
            model_route_id: contract.model_route_id.clone(),
            gateway_account_id: contract.gateway_account_id.clone(),
            model_id: facts.model_id.clone(),
            required_capabilities: BTreeSet::from(["actual-guest".into()]),
        };
        validate_original_provider(&spec, &facts).unwrap();
        for field in 0..6 {
            let mut foreign = spec.clone();
            match field {
                0 => foreign.session_id = uuid::Uuid::new_v4().to_string(),
                1 => foreign.model_id.push_str("-foreign"),
                2 => foreign.harness.push_str("-foreign"),
                3 => foreign.harness_version.push_str("-foreign"),
                4 => foreign.model_route_id.push_str("-foreign"),
                _ => foreign.gateway_account_id.push_str("-foreign"),
            }
            assert!(
                validate_original_provider(&foreign, &facts).is_err(),
                "{field}"
            );
        }
        let mut missing = facts.clone();
        missing.checkpoint_contract = None;
        assert!(validate_original_provider(&spec, &missing).is_err());
    }
    #[test]
    fn original_target_admission_rejects_unknown_effects_and_changed_checkpoint_generation() {
        let (_root, registry, assignment) = super::super::tests::fixture();
        let spec = ExecutionSpec {
            job_id: "original-job".into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            scope_id: "scope".into(),
            harness: "native".into(),
            harness_version: "version".into(),
            model_route_id: "openai".into(),
            gateway_account_id: "fixture".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::from(["actual-guest".into()]),
        };
        let ownership = Ownership {
            node_id: 4,
            generation: 9,
        };
        let protected = ProtectedEnrollment {
            binding_digest: "bc".repeat(32),
            checkpoint_digest: "ab".repeat(32),
            spec: spec.clone(),
            ownership: ownership.clone(),
            service_session: "original-service".into(),
            scope: target_handoff::TargetPolicyScope {
                owner_user_id: "owner".into(),
                worker_profile_id: "profile".into(),
                project_id: "project".into(),
                thread_id: "thread".into(),
                working_copy_id: "copy".into(),
                repository_id: "repository".into(),
                policy_revision: "revision".into(),
            },
        };
        let registration = registry
            .registration(&assignment.destination.guest_id)
            .unwrap();
        let mut entry = registration.lock().unwrap();
        let imported = GuestImportReceipt {
            destination: assignment.destination.clone(),
            spec: spec.clone(),
            ownership: ownership.clone(),
            checkpoint_digest: protected.checkpoint_digest.clone(),
            sequence: 7,
            imported_directory: assignment.destination.import_parent.clone(),
            effect_id: "actual-import".into(),
        };
        entry.imported = Some(imported.clone());
        let mut job = Job {
            spec: spec.clone(),
            ownership: ownership.clone(),
            checkpoint: Some(ProtectedCheckpoint {
                digest: imported.checkpoint_digest.clone(),
                sequence: imported.sequence,
                replicas: BTreeSet::from([2, 4]),
                receipts: vec![],
                disclosure: None,
            }),
            checkpoint_requires_refresh: true,
            pending_effects: BTreeSet::new(),
            completed_effects: BTreeSet::from([imported.effect_id]),
            stopped: false,
        };
        core_resume::validate_original_job(&entry, &protected, &job).unwrap();
        entry.process_effect = Some("actual-child".into());
        entry.registered_process = Some(GuestProcessEffect {
            effect_id: "actual-child".into(),
            job_id: spec.job_id.clone(),
            ownership: ownership.clone(),
            controller_id: assignment.destination.controller_id.clone(),
            controller_generation: assignment.destination.controller_generation,
            process_instance_id: "retained-child".into(),
        });
        job.pending_effects.insert("actual-child".into());
        core_resume::validate_original_job(&entry, &protected, &job).unwrap();
        for mutation in 0..9 {
            let mut bad = job.clone();
            match mutation {
                0 => {
                    bad.pending_effects.insert("unknown-external".into());
                }
                1 => {
                    bad.completed_effects.insert("actual-child".into());
                }
                2 => {
                    bad.pending_effects.clear();
                }
                3 => {
                    bad.completed_effects.clear();
                }
                4 => {
                    bad.ownership.generation += 1;
                }
                5 => {
                    bad.checkpoint.as_mut().unwrap().sequence += 1;
                }
                6 => {
                    bad.checkpoint.as_mut().unwrap().digest = "cd".repeat(32);
                }
                7 => {
                    bad.spec.session_id = uuid::Uuid::new_v4().to_string();
                }
                _ => {
                    bad.stopped = true;
                }
            };
            assert!(
                core_resume::validate_original_job(&entry, &protected, &bad).is_err(),
                "{mutation}"
            );
        }
        entry
            .registered_process
            .as_mut()
            .unwrap()
            .controller_generation += 1;
        assert!(core_resume::validate_original_job(&entry, &protected, &job).is_err());
    }
}

fn identity_of_journal(path: &Path) -> Result<FileIdentity> {
    super::identity(path)
}

fn validate_original_provider(spec: &ExecutionSpec, facts: &NativeProviderFacts) -> Result<()> {
    let contract = facts
        .checkpoint_contract
        .as_ref()
        .context("original provider contract missing")?;
    ensure!(
        spec.session_id == facts.provider_session_id
            && spec.model_id == facts.model_id
            && spec.harness == contract.harness
            && spec.harness_version == contract.harness_version
            && spec.model_route_id == contract.model_route_id
            && spec.gateway_account_id == contract.gateway_account_id,
        "actual producer does not own the original Core/account/harness"
    );
    Ok(())
}

impl NativeProviderAdmission for TargetAdmission {
    fn admit<'a>(
        &'a self,
        provider: NativeProviderBinding,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(self.admit_original(provider))
    }
    fn execute_guest_command(
        &self,
        command: &super::super::store::BusinessCommand,
        witness: NativeProviderCommand,
    ) -> Result<ctox_protocol::mcp::CallToolResult> {
        self.registry.verify_runtime_root(witness.runtime_root())?;
        crate::sync_host::with_current_signing_identity(witness.runtime_root(), |identity| {
            witness.with_current_command_transaction(command, |worker, facts, turn| {
                let request = super::super::guest_commands::parse_guest_command(command)?;
                ensure!(
                    request.guest_id == self.guest_id
                        && command
                            .record_id
                            .as_deref()
                            .is_none_or(|id| id == self.guest_id),
                    "native target command addresses another guest"
                );
                let registration = self.registry.registration(&self.guest_id)?;
                let (provider, binding) = {
                    let entry = registration
                        .lock()
                        .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                    (
                        entry
                            .provider
                            .clone()
                            .context("original target producer missing")?,
                        entry
                            .execution
                            .clone()
                            .context("original target execution missing")?,
                    )
                };
                ensure!(
                    binding.spec == self.protected.spec
                        && binding.ownership == self.protected.ownership,
                    "original target execution changed"
                );
                let execution = NativeGuestExecution {
                    registry: self.registry.clone(),
                    guest_id: self.guest_id.clone(),
                    provider,
                    binding,
                };
                execution.with_held_worker_policy_guarded(
                    worker,
                    facts,
                    Some(identity),
                    |entry, verify, _| {
                        let d = &entry.assignment.destination;
                        let scope = super::super::guest_runtime::GuestScope {
                            instance_id: d.instance_id.clone(),
                            user_id: d.human_owner_id.clone(),
                            project_id: d.project_id.clone(),
                            thread_id: d.thread_id.clone(),
                            worker_profile_id: d.worker_profile_id.clone(),
                            guest_id: d.guest_id.clone(),
                        };
                        super::super::guest_commands::apply_scope_claims(&scope, command)?;
                        verify()?;
                        execution.execute_frame_action(entry, verify, turn, request.action, command)
                    },
                )
            })
        })
    }
}
