//! Native pre-turn admission. Pending evidence never grants execution.
#[cfg(test)]
#[path = "native_guest_admission_tests.rs"]
mod regression_tests;
use super::queue_provider_binding::{
    NativeProviderAdmission, NativeProviderBinding, NativeProviderFacts,
};
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    authority::{client::ExecutionAuthority, Command, Job, Receipt, Request},
    contracts::ExecutionSpec,
};
use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

/// Native registry/policy/controller facts, not values from a model payload.
/// The lifecycle owner resolves them under its actual publication guard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeGuestAdmissionDestination {
    pub(crate) instance_id: String,
    pub(crate) project_id: String,
    pub(crate) human_owner_id: String,
    pub(crate) guest_id: String,
    pub(crate) worker_profile_id: String,
    pub(crate) controller_id: String,
    pub(crate) controller_generation: u64,
    pub(crate) policy_revision: String,
    pub(crate) scope_id: String,
    pub(crate) required_capabilities: BTreeSet<String>,
}

/// Implemented by the real VM registry/controller owner. There is no default.
/// The borrowed transaction is the already-held canonical worker transaction;
/// do not reopen it or await. Policy/controller revoke and publication must use
/// this same native guard. A persisted row or a preflight boolean is insufficient.
pub(crate) trait NativeGuestAdmissionOwner: Send + Sync {
    fn with_current_destination(
        &self,
        tx: &Transaction<'_>,
        facts: &NativeProviderFacts,
        expected: Option<&NativeGuestAdmissionDestination>,
        publish: &mut dyn FnMut(&NativeGuestAdmissionDestination) -> Result<()>,
    ) -> Result<()>;
}

/// Uses the existing running host authority, never a second node/store or HTTP.
/// Registration remains the native lifecycle owner's responsibility.
pub(crate) struct NativeGuestAdmission {
    authority: Arc<dyn ExecutionAuthority>,
    owner: Arc<dyn NativeGuestAdmissionOwner>,
}
impl NativeGuestAdmission {
    pub(crate) fn new(
        authority: Arc<dyn ExecutionAuthority>,
        owner: Arc<dyn NativeGuestAdmissionOwner>,
    ) -> Result<Self> {
        ensure!(
            authority.node_id() != 0 && valid_id(authority.scope_id()),
            "native guest authority is not configured"
        );
        Ok(Self { authority, owner })
    }

    async fn admit_current(&self, provider: NativeProviderBinding) -> Result<()> {
        let job_id = uuid::Uuid::new_v4().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();
        let (destination, spec) = provider.with_live_provider_transaction(|tx, facts, turn| {
            ensure!(turn.is_none(), "guest admission must precede provider turn");
            let contract = facts
                .checkpoint_contract
                .as_ref()
                .context("native guest has no actual provider account contract")?;
            let mut resolved = None;
            self.owner
                .with_current_destination(tx, facts, None, &mut |destination| {
                    ensure!(
                        resolved.is_none(),
                        "native resolver published more than once"
                    );
                    validate_destination(destination, facts, self.authority.scope_id())?;
                    let spec = ExecutionSpec {
                        job_id: job_id.clone(),
                        session_id: facts.provider_session_id.clone(),
                        scope_id: destination.scope_id.clone(),
                        harness: contract.harness.clone(),
                        harness_version: contract.harness_version.clone(),
                        model_route_id: contract.model_route_id.clone(),
                        gateway_account_id: contract.gateway_account_id.clone(),
                        model_id: facts.model_id.clone(),
                        required_capabilities: destination.required_capabilities.clone(),
                    };
                    tx.execute_batch(
                        "CREATE TABLE IF NOT EXISTS native_guest_provider_admissions (
                        binding_id TEXT PRIMARY KEY,
                        worker_id TEXT NOT NULL,
                        attempt_id TEXT NOT NULL UNIQUE,
                        request_id TEXT NOT NULL UNIQUE,
                        destination_json TEXT NOT NULL,
                        spec_json TEXT NOT NULL,
                        phase TEXT NOT NULL CHECK(phase IN ('PendingCreate','Admitted')),
                        ownership_json TEXT,
                        CHECK((phase='PendingCreate' AND ownership_json IS NULL)
                           OR (phase='Admitted' AND ownership_json IS NOT NULL))
                    )",
                    )?;
                    tx.execute(
                        "INSERT INTO native_guest_provider_admissions
                    (binding_id,worker_id,attempt_id,request_id,destination_json,spec_json,phase)
                    VALUES (?1,?2,?3,?4,?5,?6,'PendingCreate')",
                        params![
                            facts.binding_id,
                            facts.worker_id,
                            facts.attempt_id,
                            request_id,
                            serde_json::to_string(destination)?,
                            serde_json::to_string(&spec)?
                        ],
                    )?;
                    resolved = Some((destination.clone(), spec));
                    Ok(())
                })?;
            resolved.context("native owner did not resolve a destination")
        })?;

        // PendingCreate is committed before this await. Cancellation, transport
        // uncertainty, replay or post-await revocation leaves it pending. This
        // implementation never retries a Create or invents a successful receipt.
        let receipt = self
            .authority
            .submit(Request {
                request_id: request_id.clone(),
                actor: self.authority.node_id(),
                command: Command::Create {
                    spec: spec.clone(),
                    owner: self.authority.node_id(),
                },
            })
            .await?;
        let job = match receipt {
            Receipt::Applied(job) => job,
            Receipt::Replayed(_) => anyhow::bail!("native Create replay requires reconciliation"),
            _ => anyhow::bail!("native quorum did not admit the guest provider"),
        };
        validate_job(&job, &spec, self.authority.node_id())?;
        let current = self
            .authority
            .validate_ownership(&spec.job_id, &job.ownership)
            .await?;
        validate_job(&current, &spec, self.authority.node_id())?;
        ensure!(
            current.ownership == job.ownership,
            "native quorum ownership changed after Create"
        );

        provider.with_live_provider_transaction(|tx, facts, turn| {
            ensure!(
                turn.is_none(),
                "provider turn started during quorum admission"
            );
            let expected_json = serde_json::to_string(&destination)?;
            let spec_json = serde_json::to_string(&spec)?;
            let mut published = false;
            self.owner
                .with_current_destination(tx, facts, Some(&destination), &mut |current| {
                    ensure!(
                        !published && current == &destination,
                        "native destination/policy/controller changed during admission"
                    );
                    validate_destination(current, facts, self.authority.scope_id())?;
                    let changed = tx.execute(
                        "UPDATE native_guest_provider_admissions
                    SET phase='Admitted',ownership_json=?1
                    WHERE binding_id=?2 AND worker_id=?3 AND attempt_id=?4
                    AND request_id=?5 AND destination_json=?6 AND spec_json=?7
                    AND phase='PendingCreate' AND ownership_json IS NULL",
                        params![
                            serde_json::to_string(&job.ownership)?,
                            facts.binding_id,
                            facts.worker_id,
                            facts.attempt_id,
                            request_id,
                            expected_json,
                            spec_json
                        ],
                    )?;
                    ensure!(
                        changed == 1,
                        "native pending admission changed or was replayed"
                    );
                    published = true;
                    Ok(())
                })?;
            ensure!(published, "native owner did not fence admitted publication");
            Ok(())
        })
    }
}
impl NativeProviderAdmission for NativeGuestAdmission {
    fn admit<'a>(
        &'a self,
        provider: NativeProviderBinding,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(self.admit_current(provider))
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= 256
        && !value.chars().any(char::is_control)
}
fn validate_destination(
    destination: &NativeGuestAdmissionDestination,
    facts: &NativeProviderFacts,
    scope: &str,
) -> Result<()> {
    ensure!(
        [
            &destination.instance_id,
            &destination.project_id,
            &destination.human_owner_id,
            &destination.guest_id,
            &destination.worker_profile_id,
            &destination.controller_id,
            &destination.policy_revision,
            &destination.scope_id,
        ]
        .into_iter()
        .all(|id| valid_id(id))
            && destination.controller_generation != 0
            && destination.scope_id == scope
            && !destination.required_capabilities.is_empty()
            && destination
                .required_capabilities
                .iter()
                .all(|id| valid_id(id)),
        "native destination is incomplete or belongs to another authority"
    );
    ensure!(
        uuid::Uuid::parse_str(&facts.provider_session_id).is_ok(),
        "native provider has no actual durable harness session"
    );
    let provenance = facts
        .command_provenance
        .as_ref()
        .context("native guest has no verified command principal")?;
    ensure!(
        provenance.get("actor").and_then(serde_json::Value::as_str)
            == Some(destination.human_owner_id.as_str()),
        "native guest owner differs from the verified command principal"
    );
    // Source and destination IDs are independently resolved. In particular,
    // workspace labels and queue attempts are not instance IDs/Raft generations.
    Ok(())
}
fn validate_job(job: &Job, spec: &ExecutionSpec, node: u64) -> Result<()> {
    ensure!(
        &job.spec == spec
            && job.ownership.node_id == node
            && job.ownership.generation != 0
            && !job.stopped
            && job.pending_effects.is_empty()
            && job.completed_effects.is_empty()
            && job.checkpoint.is_none(),
        "native Create does not match the actual provider execution"
    );
    Ok(())
}
