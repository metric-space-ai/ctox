// Origin: CTOX
// License: AGPL-3.0-only

//! Native guest assignments and the shared controller guard. No renderer intake
//! registers a guest. Provider, policy and controller guards remain held through
//! synchronous publication; a receipt or cached readiness is never authority.

#[cfg(test)]
#[path = "guest_registry_tests.rs"]
mod tests;
// Scoped native resolver retained by the actual provider admission factory.
// Renderer/model fields cannot construct or replace this registration.
struct NativeGuestAdmissionResolver {
    registry: Arc<NativeGuestRegistry>,
    guest_id: String,
}
impl NativeGuestRegistry {
    pub(crate) fn admission(
        self: &Arc<Self>,
        guest_id: &str,
    ) -> Result<Arc<crate::channels::NativeGuestAdmission>> {
        self.registration(guest_id)?;
        let owner = Arc::new(NativeGuestAdmissionResolver {
            registry: Arc::clone(self),
            guest_id: guest_id.into(),
        });
        Ok(Arc::new(crate::channels::NativeGuestAdmission::new(
            Arc::clone(&self.authority),
            owner,
        )?))
    }

    fn admission_destination(
        &self,
        conn: &Connection,
        entry: &Registration,
    ) -> Result<NativeGuestAdmissionDestination> {
        let d = &entry.assignment.destination;
        ensure!(!entry.revoked, "native controller revoked");
        ensure!(
            private_directory(&d.import_parent)? == entry.import_identity,
            "native import parent replaced"
        );
        Ok(NativeGuestAdmissionDestination {
            instance_id: d.instance_id.clone(),
            project_id: d.project_id.clone(),
            human_owner_id: d.human_owner_id.clone(),
            guest_id: d.guest_id.clone(),
            worker_profile_id: d.worker_profile_id.clone(),
            controller_id: d.controller_id.clone(),
            controller_generation: d.controller_generation,
            policy_revision: validate_policy(conn, d)?,
            scope_id: entry.assignment.scope_id.clone(),
            required_capabilities: self.required_capabilities.clone(),
        })
    }
}
fn validate_provider(
    facts: &NativeProviderFacts,
    destination: &GuestRestoreDestination,
) -> Result<()> {
    let provenance = facts
        .command_provenance
        .as_ref()
        .context("native command provenance missing")?;
    ensure!(
        provenance.get("actor").and_then(serde_json::Value::as_str)
            == Some(destination.human_owner_id.as_str())
            && provenance
                .get("expires_at_ms")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|expiry| u128::from(expiry) > super::store::now_ms()),
        "native guest principal or command lifetime is not current"
    );
    ensure!(
        facts.checkpoint_contract.is_some(),
        "guest provider has no native account/harness"
    );
    ensure!(
        provenance
            .get("crew_binding")
            .and_then(|binding| binding.get("attempt_id"))
            .and_then(serde_json::Value::as_str)
            == Some(facts.attempt_id.as_str()),
        "native guest command does not belong to actual worker attempt"
    );
}
impl NativeGuestAdmissionOwner for NativeGuestAdmissionResolver {
    fn with_current_destination(
        &self,
        tx: &rusqlite::Transaction<'_>,
        runtime_root: &Path,
        facts: &NativeProviderFacts,
        expected: Option<&NativeGuestAdmissionDestination>,
        publish: &mut dyn FnMut(&NativeGuestAdmissionDestination) -> Result<()>,
    ) -> Result<()> {
        verify_worker_current(tx, facts)?;
        self.registry.verify_runtime_root(runtime_root)?;
        let registration = self.registry.registration(&self.guest_id)?;
        self.registry.with_policy(|policy| {
            let entry = registration
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            validate_provider(facts, &entry.assignment.destination)?;
            let destination = self.registry.admission_destination(policy, &entry)?;
            ensure!(
                expected.is_none_or(|expected| expected == &destination),
                "native policy/controller changed during quorum admission"
            );
            publish(&destination)?;
            verify_worker_current(tx, facts)?;
            Ok(())
        })
    }
}
fn verify_worker_current(tx: &Connection, facts: &NativeProviderFacts) -> Result<()> {
    ensure!(
        !facts.routing_attempts.is_empty(),
        "native worker has no routing attempts"
    );
    for (key, expected) in &facts.routing_attempts {
        let (attempt, expiry): (i64, String) = tx.query_row(
            "SELECT attempt, lease_expires_at FROM communication_routing_state
             WHERE message_key=?1 AND route_status='leased'
             AND lease_owner='ctox-service' AND lease_worker_id=?2",
            rusqlite::params![key, facts.worker_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(
            attempt == *expected
                && attempt > 0
                && chrono::DateTime::parse_from_rfc3339(&expiry)? > chrono::Utc::now(),
            "native guest worker lease expired or was replaced"
        );
    }
    Ok(())
}
use super::guest_runtime::identifier;
use super::session::{session_user_id, BusinessOsSession};
use super::store::{business_os_store_path, outbound_load_record, stable_instance_id};
use crate::channels::{
    NativeGuestAdmissionDestination, NativeGuestAdmissionOwner, NativeProviderBinding,
    NativeProviderFacts,
};
use anyhow::{ensure, Context, Result};
use ctox_sync::authority::{client::ExecutionAuthority, Ownership};
use ctox_sync::contracts::ExecutionSpec;
use ctox_sync::guest_restore::{
    GuestImportReceipt, GuestLiveEndpoint, GuestProcessEffect, GuestReadinessOwner,
    GuestReadyObservation, GuestRestoreDestination, GuestRestoreOwner,
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, PartialEq, Eq)]
struct FileIdentity(u64, u64);
fn identity(path: &Path) -> Result<FileIdentity> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "native policy store is not a regular file"
    );
    // SAFETY: geteuid reads only this native process's effective user ID.
    ensure!(
        metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o022 == 0,
        "native store identity is foreign or writable by other users"
    );
    Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

#[derive(Clone)]
pub(crate) struct NativeGuestAssignment {
    pub(crate) destination: GuestRestoreDestination,
    /// Scope of the actual already-running native authority, never a client claim.
    pub(crate) scope_id: String,
}
impl NativeGuestAssignment {
    /// Separate mutable reconstruction/process directory, never signed import.
    pub(crate) fn runtime_parent(&self) -> PathBuf {
        self.destination
            .import_parent
            .join(format!("runtime-{}", self.destination.guest_id))
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ExecutionBinding {
    spec: ExecutionSpec,
    ownership: Ownership,
    provider_binding_id: String,
    admission: NativeGuestAdmissionDestination,
}

struct Registration {
    assignment: NativeGuestAssignment,
    import_identity: FileIdentity,
    revoked: bool,
    execution: Option<ExecutionBinding>,
    provider: Option<NativeProviderBinding>,
    imported: Option<GuestImportReceipt>,
    imported_identity: Option<FileIdentity>,
    publication: PublicationState,
    process_effect: Option<String>,
    registered_process: Option<GuestProcessEffect>,
    #[cfg(target_os = "linux")]
    stopped_status: Option<std::process::ExitStatus>,
    #[cfg(target_os = "linux")]
    desktop: Option<super::guest_runtime::RetainedQemuDesktop>,
}

/// One lifecycle owner retains this registry. Restart does not revive live
/// controllers/processes from persisted claims; reconciliation must re-enroll.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PublicationState {
    Virgin,
    Uncertain,
    Published,
}

pub(crate) struct NativeGuestRegistry {
    runtime_root: PathBuf,
    root_directory: std::fs::File,
    instance_id: String,
    authority: Arc<dyn ExecutionAuthority>,
    required_capabilities: BTreeSet<String>,
    policy_path: PathBuf,
    policy_identity: FileIdentity,
    instance_path: PathBuf,
    instance_file_identity: FileIdentity,
    guests: Mutex<HashMap<String, Arc<Mutex<Registration>>>>,
}

pub(crate) struct NativeGuestExecution {
    registry: Arc<NativeGuestRegistry>,
    provider: NativeProviderBinding,
    guest_id: String,
    binding: ExecutionBinding,
}

fn private_directory(path: &Path) -> Result<FileIdentity> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && std::fs::canonicalize(path)? == path
            && metadata.mode() & 0o077 == 0
            && metadata.uid() == unsafe { libc::geteuid() },
        "guest import parent is not a canonical private native directory"
    );
    Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

impl NativeGuestRegistry {
    pub(crate) fn new(
        root: &Path,
        authority: Arc<dyn ExecutionAuthority>,
        required_capabilities: BTreeSet<String>,
    ) -> Result<Arc<Self>> {
        ensure!(
            authority.node_id() != 0
                && identifier(authority.scope_id())
                && !required_capabilities.is_empty()
                && required_capabilities
                    .iter()
                    .all(|capability| identifier(capability)),
            "native host authority/capability requirements are missing"
        );
        let root = std::fs::canonicalize(root)?;
        use std::os::unix::fs::OpenOptionsExt;
        let root_directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&root)?;
        let instance_id = stable_instance_id(&root)?;
        let policy_path = business_os_store_path(&root);
        let policy_identity = identity(&policy_path)?;
        let instance_path = root.join("runtime/business-os-instance-id");
        let instance_file_identity = identity(&instance_path)?;
        Ok(Arc::new(Self {
            runtime_root: root,
            root_directory,
            instance_id,
            authority,
            required_capabilities,
            policy_path,
            policy_identity,
            instance_path,
            instance_file_identity,
            guests: Mutex::new(HashMap::new()),
        }))
    }

    fn verify_runtime_root(&self, runtime_root: &Path) -> Result<()> {
        ensure!(
            std::fs::canonicalize(runtime_root)? == self.runtime_root,
            "native provider belongs to another runtime root"
        );
        let current = std::fs::symlink_metadata(&self.runtime_root)?;
        let retained = self.root_directory.metadata()?;
        ensure!(
            current.is_dir()
                && !current.file_type().is_symlink()
                && retained.is_dir()
                && current.dev() == retained.dev()
                && current.ino() == retained.ino(),
            "native runtime root was replaced"
        );
        Ok(())
    }

    /// The actual policy store is separate from the channel worker database.
    /// Lock it before the controller, without opening/creating/migrating state.
    /// Cross-store commit failure is uncertain, never an atomic-success claim.
    fn with_policy<T>(
        &self,
        apply: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        self.verify_runtime_root(&self.runtime_root)?;
        ensure!(
            identity(&self.policy_path)? == self.policy_identity,
            "native policy store was replaced"
        );
        ensure!(
            identity(&self.instance_path)? == self.instance_file_identity
                && std::fs::read_to_string(&self.instance_path)?.trim() == self.instance_id,
            "native instance changed"
        );
        let mut conn =
            Connection::open_with_flags(&self.policy_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        conn.busy_timeout(Duration::from_millis(100))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            identity(&self.policy_path)? == self.policy_identity,
            "native policy store changed before publication"
        );
        let result = apply(&tx)?;
        self.verify_runtime_root(&self.runtime_root)?;
        ensure!(
            identity(&self.policy_path)? == self.policy_identity,
            "policy store changed during effect; reconcile"
        );
        ensure!(
            identity(&self.instance_path)? == self.instance_file_identity
                && std::fs::read_to_string(&self.instance_path)?.trim() == self.instance_id,
            "instance changed during effect; reconcile"
        );
        tx.commit()?;
        Ok(result)
    }

    fn registration(&self, guest_id: &str) -> Result<Arc<Mutex<Registration>>> {
        self.guests
            .lock()
            .map_err(|_| anyhow::anyhow!("native guest registry poisoned"))?
            .get(guest_id)
            .cloned()
            .context("guest is not enrolled")
    }

    /// Called by the native provisioning/control owner after authenticated
    /// human approval. The computer, profile, project and chat are canonical
    /// records, not host paths or copied model settings.
    pub(crate) fn enroll(
        &self,
        session: &BusinessOsSession,
        project_id: &str,
        thread_id: &str,
        worker_profile_id: &str,
        import_parent: &Path,
    ) -> Result<NativeGuestAssignment> {
        ensure!(
            session.ok && session.authenticated,
            "guest enrollment requires an authenticated human"
        );
        let human_owner_id =
            session_user_id(session).context("guest enrollment has no human principal")?;
        ensure!(
            [human_owner_id, project_id, thread_id, worker_profile_id]
                .iter()
                .all(|id| identifier(id)),
            "invalid native guest assignment"
        );
        let import_identity = private_directory(import_parent)?;
        let assignment = NativeGuestAssignment {
            destination: GuestRestoreDestination {
                instance_id: self.instance_id.clone(),
                guest_id: format!("guest_{}", uuid::Uuid::new_v4()),
                human_owner_id: human_owner_id.into(),
                project_id: project_id.into(),
                thread_id: thread_id.into(),
                worker_profile_id: worker_profile_id.into(),
                controller_id: format!("controller_{}", uuid::Uuid::new_v4()),
                controller_generation: 1,
                import_parent: import_parent.into(),
            },
            scope_id: self.authority.scope_id().to_owned(),
        };
        self.with_policy(|tx| {
            validate_policy(tx, &assignment.destination)?;
            let mut entries = self
                .guests
                .lock()
                .map_err(|_| anyhow::anyhow!("native guest registry poisoned"))?;
            ensure!(entries.len() < 64, "native guest registry capacity reached");
            // Never silently rebind an old process/import or existing assignment.
            for entry in entries.values() {
                let entry = entry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                let d = &entry.assignment.destination;
                ensure!(
                    d.human_owner_id != human_owner_id
                        || d.project_id != project_id
                        || d.thread_id != thread_id
                        || d.worker_profile_id != worker_profile_id,
                    "guest assignment already retained; reconcile it before enrolling another"
                );
                ensure!(
                    d.import_parent != import_parent,
                    "guest import parent is already retained"
                );
            }
            entries.insert(
                assignment.destination.guest_id.clone(),
                Arc::new(Mutex::new(Registration {
                    assignment: assignment.clone(),
                    import_identity,
                    revoked: false,
                    execution: None,
                    provider: None,
                    imported: None,
                    imported_identity: None,
                    publication: PublicationState::Virgin,
                    process_effect: None,
                    registered_process: None,
                    #[cfg(target_os = "linux")]
                    stopped_status: None,
                    #[cfg(target_os = "linux")]
                    desktop: None,
                })),
            );
            Ok(())
        })?;
        Ok(assignment)
    }

    /// Actual provider guard -> actual policy transaction -> shared controller.
    /// Architecture uses the generated scope/destination before quorum Create.
    /// This observation is not permission to publish after the callback returns.
    pub(crate) fn with_admission_destination<T>(
        &self,
        provider: &NativeProviderBinding,
        guest_id: &str,
        apply: impl FnOnce(&rusqlite::Transaction<'_>, &NativeGuestAssignment) -> Result<T>,
    ) -> Result<T> {
        self.verify_runtime_root(provider.runtime_root())?;
        let entry = self.registration(guest_id)?;
        provider.with_live_provider_transaction(|worker_tx, facts, _| {
            self.with_policy(|policy_tx| {
                let entry = entry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                ensure!(!entry.revoked, "native controller revoked");
                validate_policy(policy_tx, &entry.assignment.destination)?;
                ensure!(
                    private_directory(&entry.assignment.destination.import_parent)?
                        == entry.import_identity,
                    "native import parent replaced"
                );
                let provenance = facts
                    .command_provenance
                    .as_ref()
                    .context("provider has no verified native command")?;
                ensure!(
                    provenance
                        .get("expires_at_ms")
                        .and_then(serde_json::Value::as_u64)
                        .is_some_and(|expiry| u128::from(expiry) > super::store::now_ms()),
                    "native guest command authority expired or has no lifetime"
                );
                ensure!(
                    provenance.get("actor").and_then(serde_json::Value::as_str)
                        == Some(entry.assignment.destination.human_owner_id.as_str()),
                    "provider principal differs from guest owner"
                );
                ensure!(
                    facts.checkpoint_contract.is_some(),
                    "provider has no native account/harness contract"
                );
                apply(worker_tx, &entry.assignment)
            })
        })
    }

    /// Native lifecycle owner obtains the actual producer binding, never one
    /// reconstructed from a serialized job or a guest ID alone.
    pub(crate) fn bound_execution(
        self: &Arc<Self>,
        session: &BusinessOsSession,
        guest_id: &str,
    ) -> Result<NativeGuestExecution> {
        ensure!(
            session.ok && session.authenticated,
            "guest requires authenticated human"
        );
        let entry = self.registration(guest_id)?;
        let (provider, binding) = {
            let entry = entry
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            ensure!(
                session_user_id(session)
                    == Some(entry.assignment.destination.human_owner_id.as_str()),
                "foreign guest execution"
            );
            (
                entry
                    .provider
                    .clone()
                    .context("guest has no actual producer binding")?,
                entry
                    .execution
                    .clone()
                    .context("guest has no bound execution")?,
            )
        };
        let execution = NativeGuestExecution {
            registry: Arc::clone(self),
            provider,
            guest_id: guest_id.into(),
            binding,
        };
        // Reacquire in the real worker -> policy -> controller order and check
        // expiry, current native policy and provider identity before returning.
        execution.with_current(|_, verify| verify())?;
        Ok(execution)
    }

    /// Producer consumes its own admitted row as an observation, then the
    /// existing bind path independently revalidates quorum/provider/policy.
    pub(crate) async fn bind_admitted_execution(
        self: &Arc<Self>,
        provider: NativeProviderBinding,
        guest_id: &str,
    ) -> Result<NativeGuestExecution> {
        let (spec, ownership) = provider.with_live_provider_transaction(|tx, facts, _| {
            let (spec_json, ownership_json): (String, String) = tx.query_row(
                "SELECT spec_json, ownership_json FROM native_guest_provider_admissions
                 WHERE binding_id=?1 AND worker_id=?2 AND attempt_id=?3 AND phase='Admitted'",
                rusqlite::params![facts.binding_id, facts.worker_id, facts.attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok((
                serde_json::from_str::<ExecutionSpec>(&spec_json)?,
                serde_json::from_str::<Ownership>(&ownership_json)?,
            ))
        })?;
        self.bind_execution(provider, guest_id, spec, ownership)
            .await
    }

    /// Bind only the actual quorum result, then revalidate native destination and
    /// provider. Neither a client job string nor a serialized Ownership is enough.
    pub(crate) async fn bind_execution(
        self: &Arc<Self>,
        provider: NativeProviderBinding,
        guest_id: &str,
        spec: ExecutionSpec,
        ownership: Ownership,
    ) -> Result<NativeGuestExecution> {
        let job = self
            .authority
            .validate_ownership(&spec.job_id, &ownership)
            .await?;
        ensure!(
            job.spec == spec
                && job.ownership == ownership
                && !job.stopped
                && job.pending_effects.is_empty(),
            "quorum guest admission is not current or quiescent"
        );
        let (binding_id, admission) = provider.with_live_provider_transaction(|tx, facts, _| {
            let contract = facts.checkpoint_contract.as_ref().context("native checkpoint contract missing")?;
            ensure!(
                spec.session_id == facts.provider_session_id && spec.model_id == facts.model_id
                    && spec.harness == contract.harness && spec.harness_version == contract.harness_version
                    && spec.model_route_id == contract.model_route_id
                    && spec.gateway_account_id == contract.gateway_account_id,
                "guest execution differs from actual provider/account/harness"
            );
            let (spec_json, ownership_json, destination_json): (String, String, String) = tx.query_row(
                "SELECT spec_json, ownership_json, destination_json FROM native_guest_provider_admissions
                 WHERE binding_id=?1 AND worker_id=?2 AND attempt_id=?3 AND phase='Admitted'",
                rusqlite::params![facts.binding_id, facts.worker_id, facts.attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            ensure!(serde_json::from_str::<ExecutionSpec>(&spec_json)? == spec
                && serde_json::from_str::<Ownership>(&ownership_json)? == ownership,
                "guest execution is not the actual admitted provider job");
            Ok((facts.binding_id.clone(), serde_json::from_str::<NativeGuestAdmissionDestination>(&destination_json)?))
        })?;
        let binding = ExecutionBinding {
            spec,
            ownership,
            provider_binding_id: binding_id,
            admission,
        };
        self.with_admission_destination(&provider, guest_id, |_, assignment| {
            ensure!(
                binding.spec.scope_id == assignment.scope_id,
                "execution scope differs from canonical guest assignment"
            );
            Ok(())
        })?;
        // A fresh complete guard, not reuse of either preceding observation.
        let execution = NativeGuestExecution {
            registry: Arc::clone(self),
            provider,
            guest_id: guest_id.into(),
            binding,
        };
        execution.install_binding()?;
        Ok(execution)
    }

    /// Stops only the actual retained child. Revocation is immediate, while
    /// timeout/failure retains ownership and denies takeover.
    #[cfg(target_os = "linux")]
    pub(crate) fn stop_owned(
        &self,
        session: &BusinessOsSession,
        guest_id: &str,
    ) -> Result<std::process::ExitStatus> {
        ensure!(
            session.ok && session.authenticated,
            "guest stop requires authenticated human"
        );
        let entry = self.registration(guest_id)?;
        self.with_policy(|_| {
            let mut entry = entry
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            ensure!(
                session_user_id(session)
                    == Some(entry.assignment.destination.human_owner_id.as_str()),
                "foreign guest stop"
            );
            if !entry.revoked {
                entry.assignment.destination.controller_generation = entry
                    .assignment
                    .destination
                    .controller_generation
                    .checked_add(1)
                    .context("controller generation exhausted")?;
                entry.revoked = true;
            }
            let desktop = entry
                .desktop
                .as_mut()
                .context("guest has no retained process to stop")?;
            // Worker expiry cannot prevent the human from stopping this child.
            let status = super::guest_commands::block_on_guest(desktop.stop())?;
            entry.stopped_status = Some(status);
            // Preserve the pending quorum effect. A stop observation is not an
            // automatic CompleteEffect or new-controller admission.
            Ok(status)
        })
    }

    /// Human revoke shares the exact policy/controller locks used by every
    /// publication and probe. It invalidates rights immediately; it is NOT a
    /// process stop or application-consistent checkpoint witness.
    pub(crate) fn revoke(&self, session: &BusinessOsSession, guest_id: &str) -> Result<()> {
        ensure!(
            session.ok && session.authenticated,
            "controller revoke requires authenticated human"
        );
        let entry = self.registration(guest_id)?;
        self.with_policy(|_| {
            let mut entry = entry
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            ensure!(
                session_user_id(session)
                    == Some(entry.assignment.destination.human_owner_id.as_str()),
                "foreign controller revoke"
            );
            if !entry.revoked {
                entry.assignment.destination.controller_generation = entry
                    .assignment
                    .destination
                    .controller_generation
                    .checked_add(1)
                    .context("controller generation exhausted")?;
                entry.revoked = true;
            }
            Ok(())
        })
    }
}

fn validate_policy(conn: &Connection, destination: &GuestRestoreDestination) -> Result<String> {
    let project = super::project_chats::owned_project(
        conn,
        &destination.project_id,
        &destination.human_owner_id,
        true,
    )?;
    let profile = super::worker_profile_bindings::require_active(
        conn,
        &destination.human_owner_id,
        &destination.worker_profile_id,
    )?;
    let computer_id = profile["computer_id"]
        .as_str()
        .context("worker computer missing")?;
    let computer = super::store_workjet_computers::require_assigned_workjet_computer(
        conn,
        computer_id,
        &destination.human_owner_id,
    )?;
    ensure!(computer["is_deleted"] != true, "guest computer is deleted");
    let member_id = super::project_chats::stable_id(
        "workjet_member",
        &[
            &destination.human_owner_id,
            &destination.project_id,
            &destination.worker_profile_id,
        ],
    );
    let member = outbound_load_record(conn, super::project_chats::MEMBERS, &member_id)?
        .context("guest worker is not a project member")?;
    ensure!(
        member["owner_user_id"] == destination.human_owner_id
            && member["project_id"] == destination.project_id
            && member["worker_profile_id"] == destination.worker_profile_id
            && member["status"] == "active"
            && member["is_deleted"] != true,
        "guest project worker is unavailable"
    );
    let chat = outbound_load_record(conn, super::project_chats::CHATS, &destination.thread_id)?
        .context("guest chat is not registered")?;
    ensure!(
        chat["owner_user_id"] == destination.human_owner_id
            && chat["project_id"] == destination.project_id
            && chat["thread_id"] == destination.thread_id
            && chat["is_deleted"] != true
            && (chat["worker_profile_id"] == destination.worker_profile_id
                || (chat["kind"] == "group" && member["group_chat_id"] == destination.thread_id)),
        "guest chat is outside the approved worker assignment"
    );
    let thread = outbound_load_record(conn, super::project_chats::THREADS, &destination.thread_id)?
        .context("guest thread is unavailable")?;
    ensure!(
        thread["owner_user_id"] == destination.human_owner_id
            && thread["status"] == "open"
            && thread["is_deleted"] != true
            && thread["archived_at_ms"].as_i64().unwrap_or(0) == 0,
        "guest thread is closed, archived or foreign"
    );
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            project, profile, computer, member, chat, thread
        ))?)
    ))
}

impl NativeGuestExecution {
    fn install_binding(&self) -> Result<()> {
        self.registry
            .verify_runtime_root(self.provider.runtime_root())?;
        let entry = self.registry.registration(&self.guest_id)?;
        self.provider.with_live_provider_transaction(|_, facts, _| {
            self.registry.with_policy(|tx| {
                let mut entry = entry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                ensure!(
                    !entry.revoked && entry.execution.is_none(),
                    "native guest execution already bound or revoked"
                );
                validate_provider(facts, &entry.assignment.destination)?;
                ensure!(
                    self.registry.admission_destination(tx, &entry)? == self.binding.admission,
                    "native policy/controller differs from actual admitted job"
                );
                ensure!(
                    facts.binding_id == self.binding.provider_binding_id
                        && self.binding.spec.scope_id == entry.assignment.scope_id,
                    "native binding changed during admission"
                );
                ensure!(
                    private_directory(&entry.assignment.destination.import_parent)?
                        == entry.import_identity,
                    "native import parent replaced"
                );
                entry.execution = Some(self.binding.clone());
                entry.provider = Some(self.provider.clone());
                Ok(())
            })
        })
    }

    fn with_current<T>(
        &self,
        apply: impl FnOnce(&mut Registration, &dyn Fn() -> Result<()>) -> Result<T>,
    ) -> Result<T> {
        self.registry
            .verify_runtime_root(self.provider.runtime_root())?;
        let entry = self.registry.registration(&self.guest_id)?;
        self.provider
            .with_live_provider_transaction(|worker_tx, facts, _| {
                self.registry.with_policy(|tx| {
                    let mut entry = entry
                        .lock()
                        .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                    ensure!(
                        !entry.revoked
                            && entry.execution.as_ref() == Some(&self.binding)
                            && facts.binding_id == self.binding.provider_binding_id,
                        "guest controller/execution revoked or replaced"
                    );
                    ensure!(
                        self.registry.admission_destination(tx, &entry)? == self.binding.admission,
                        "native guest policy/controller changed since admission"
                    );
                    let provenance = facts
                        .command_provenance
                        .as_ref()
                        .context("native command authority missing")?;
                    ensure!(
                        provenance.get("actor").and_then(serde_json::Value::as_str)
                            == Some(entry.assignment.destination.human_owner_id.as_str())
                            && provenance
                                .get("expires_at_ms")
                                .and_then(serde_json::Value::as_u64)
                                .is_some_and(|expiry| u128::from(expiry) > super::store::now_ms()),
                        "native guest principal/lifetime changed"
                    );
                    ensure!(
                        private_directory(&entry.assignment.destination.import_parent)?
                            == entry.import_identity,
                        "native import parent replaced"
                    );
                    let verify = || {
                        verify_worker_current(worker_tx, facts)?;
                        validate_provider(facts, &entry.assignment.destination)
                    };
                    verify()?;
                    // Capture a destination clone so the checker can be borrowed
                    // while the retained process changes under this controller.
                    if let Some(imported) = &entry.imported {
                        ensure!(
                            Some(private_directory(&imported.imported_directory)?)
                                == entry.imported_identity,
                            "registered import directory was replaced"
                        );
                    }
                    let destination = entry.assignment.destination.clone();
                    let verify = || {
                        verify_worker_current(worker_tx, facts)?;
                        validate_provider(facts, &destination)
                    };
                    let result = apply(&mut entry, &verify)?;
                    verify()?;
                    Ok(result)
                })
            })
    }

    /// Receipt registration revalidates completed quorum effect; presence of an
    /// imported directory can never fabricate successful import completion.
    pub(crate) async fn register_import(&self, receipt: GuestImportReceipt) -> Result<()> {
        let job = self
            .registry
            .authority
            .validate_ownership(&self.binding.spec.job_id, &self.binding.ownership)
            .await?;
        ensure!(
            job.spec == self.binding.spec
                && !job.stopped
                && job.pending_effects.is_empty()
                && job.completed_effects.contains(&receipt.effect_id),
            "guest import effect is not completed/quiescent"
        );
        ensure!(
            job.checkpoint
                .as_ref()
                .is_some_and(|checkpoint| checkpoint.digest == receipt.checkpoint_digest
                    && checkpoint.sequence == receipt.sequence),
            "guest import is not the protected checkpoint"
        );
        self.with_current(|entry, verify| {
            ensure!(
                receipt.destination == entry.assignment.destination
                    && receipt.spec == self.binding.spec
                    && receipt.ownership == self.binding.ownership,
                "foreign guest import receipt"
            );
            ensure!(
                entry.imported.is_none(),
                "guest import already registered; reconcile before replacing"
            );
            ensure!(
                entry.publication == PublicationState::Published,
                "native owner did not successfully publish this import"
            );
            ensure!(
                receipt.imported_directory.parent()
                    == Some(entry.assignment.destination.import_parent.as_path()),
                "guest import outside native parent"
            );
            let effect_bytes = format!(
                "{}\0{}\0{}\0{}\0{}\0{}\0{}",
                receipt.spec.job_id,
                receipt.destination.instance_id,
                receipt.destination.guest_id,
                receipt.destination.controller_id,
                receipt.destination.controller_generation,
                receipt.ownership.generation,
                receipt.checkpoint_digest
            );
            // ref: src/core/sync/src/guest_restore.rs:287-304,324-329
            let expected_effect =
                format!("guest-import:{:x}", Sha256::digest(effect_bytes.as_bytes()));
            ensure!(
                receipt.effect_id == expected_effect
                    && receipt.imported_directory
                        == receipt.destination.import_parent.join(format!(
                            "import-{:x}",
                            Sha256::digest(receipt.effect_id.as_bytes())
                        )),
                "import receipt is not the exact native destination effect"
            );
            let imported_identity = private_directory(&receipt.imported_directory)?;
            verify()?;
            entry.imported_identity = Some(imported_identity);
            entry.imported = Some(receipt);
            Ok(())
        })
    }

    #[cfg(target_os = "linux")]
    pub(crate) async fn start_prepared(
        &self,
        config: &super::guest_runtime::PreparedQemuGuest,
    ) -> Result<GuestLiveEndpoint> {
        use ctox_sync::authority::{Command, Receipt, Request};
        let effect_id = format!("guest-process:{}", uuid::Uuid::new_v4());
        self.with_current(|entry, verify| {
            ensure!(
                entry.imported.is_some()
                    && entry.desktop.is_none()
                    && entry.process_effect.is_none(),
                "guest needs a registered import and fresh process attempt"
            );
            ensure!(
                config.runtime_parent == entry.assignment.runtime_parent()
                    && config.overlay_qcow2.parent() == Some(config.runtime_parent.as_path())
                    && std::fs::canonicalize(&config.overlay_qcow2)? == config.overlay_qcow2,
                "prepared QEMU runtime/overlay is outside this native guest assignment"
            );
            private_directory(&config.runtime_parent)?;
            verify()?;
            // Retain the attempt BEFORE the first quorum await. Cancellation or
            // uncertain admission never creates another process/effect attempt.
            entry.process_effect = Some(effect_id.clone());
            Ok(())
        })?;
        let receipt = self
            .registry
            .authority
            .submit(Request {
                request_id: format!("{effect_id}:begin"),
                actor: self.registry.authority.node_id(),
                command: Command::BeginEffect {
                    job_id: self.binding.spec.job_id.clone(),
                    ownership: self.binding.ownership.clone(),
                    effect_id: effect_id.clone(),
                },
            })
            .await?;
        match receipt {
            Receipt::Applied(job)
                if job.spec == self.binding.spec
                    && job.ownership == self.binding.ownership
                    && !job.stopped
                    && job.pending_effects.len() == 1
                    && job.pending_effects.contains(&effect_id) => {}
            _ => anyhow::bail!("guest process effect was not freshly admitted; reconcile"),
        }
        self.with_current(|entry, verify| {
            ensure!(
                entry.process_effect.as_deref() == Some(effect_id.as_str())
                    && entry.desktop.is_none(),
                "native process attempt changed during admission"
            );
            verify()?;
            // Keep the unresolved process effect until the exact old child has
            // stopped and explicit native reconciliation completes it.
            entry.desktop = Some(super::guest_runtime::RetainedQemuDesktop::spawn_paused(
                config,
                self.guest_id.clone(),
            )?);
            entry.registered_process = Some(GuestProcessEffect {
                effect_id: effect_id.clone(),
                job_id: self.binding.spec.job_id.clone(),
                ownership: self.binding.ownership.clone(),
                controller_id: entry.assignment.destination.controller_id.clone(),
                controller_generation: entry.assignment.destination.controller_generation,
                process_instance_id: entry.desktop.as_ref().unwrap().process_instance_id().into(),
            });
            let endpoint =
                super::guest_commands::block_on_guest(entry.desktop.as_mut().unwrap().boot())?;
            verify()?;
            Ok(endpoint)
        })
    }
}

fn io_error(error: anyhow::Error) -> io::Error {
    io::Error::other(error.to_string())
}
impl GuestRestoreOwner for NativeGuestExecution {
    fn resolve_destination(
        &self,
        guest_id: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> io::Result<GuestRestoreDestination> {
        ensure_request(self, guest_id, spec, ownership)?;
        self.with_current(|entry, _| Ok(entry.assignment.destination.clone()))
            .map_err(io_error)
    }
    fn with_current_fence(
        &self,
        expected: &GuestRestoreDestination,
        spec: &ExecutionSpec,
        ownership: &Ownership,
        publish: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        ensure_request(self, &expected.guest_id, spec, ownership)?;
        self.with_current(|entry, verify| {
            ensure!(
                entry.assignment.destination == *expected,
                "native guest destination/controller changed"
            );
            ensure!(
                entry.publication == PublicationState::Virgin,
                "native import publication already attempted; reconcile uncertain effect"
            );
            entry.publication = PublicationState::Uncertain;
            verify()?;
            publish().map_err(anyhow::Error::from)?;
            entry.publication = PublicationState::Published;
            Ok(())
        })
        .map_err(io_error)
    }
}
fn ensure_request(
    owner: &NativeGuestExecution,
    guest_id: &str,
    spec: &ExecutionSpec,
    ownership: &Ownership,
) -> io::Result<()> {
    if owner.guest_id != guest_id
        || owner.binding.spec != *spec
        || owner.binding.ownership != *ownership
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "foreign native guest execution",
        ));
    }
    Ok(())
}
impl GuestReadinessOwner for NativeGuestExecution {
    fn with_live_guest(
        &self,
        imported: &GuestImportReceipt,
        publish: &mut dyn FnMut(GuestReadyObservation) -> io::Result<()>,
    ) -> io::Result<()> {
        ensure_request(
            self,
            &imported.destination.guest_id,
            &imported.spec,
            &imported.ownership,
        )?;
        self.with_current(|entry, verify| {
            ensure!(
                entry.process_effect.is_some(),
                "guest has no retained quorum process effect"
            );
            ensure!(
                entry.imported.as_ref() == Some(imported),
                "guest import is not canonically registered"
            );
            #[cfg(not(target_os = "linux"))]
            {
                let _ = publish;
                anyhow::bail!("real guest readiness requires retained Linux QEMU");
            }
            #[cfg(target_os = "linux")]
            {
                let process_effect = entry
                    .registered_process
                    .clone()
                    .context("guest has no exact registered child effect")?;
                ensure!(
                    entry.process_effect.as_deref() == Some(process_effect.effect_id.as_str())
                        && process_effect.job_id == self.binding.spec.job_id
                        && process_effect.ownership == self.binding.ownership
                        && process_effect.controller_id
                            == entry.assignment.destination.controller_id
                        && process_effect.controller_generation
                            == entry.assignment.destination.controller_generation,
                    "native child effect binding changed"
                );
                let desktop = entry
                    .desktop
                    .as_mut()
                    .context("guest has no retained process")?;
                ensure!(
                    desktop.process_instance_id() == process_effect.process_instance_id,
                    "registered native child was replaced"
                );
                let endpoint = super::guest_commands::block_on_guest(desktop.probe_live())?;
                verify()?;
                ensure!(
                    endpoint.process_instance_id == process_effect.process_instance_id,
                    "observed child differs from registered process effect"
                );
                publish(GuestReadyObservation {
                    endpoint,
                    process_effect,
                })
                .map_err(anyhow::Error::from)
            }
        })
        .map_err(io_error)
    }
}
