// Origin: CTOX
// License: AGPL-3.0-only

//! Private original-lease controller for a selected project Supervisor.
//! Capture is native-only; no JSON facts, session label or caller boolean can
//! construct execution authority. This kernel does not start an SDK or claim
//! a physical stop. The holding adapter remains responsible for those witnesses.
use super::*;
use crate::business_os::consumer_authority::{AdmittedConsumerAuthority, ConsumerFacts};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use zeroize::Zeroizing;

pub(crate) struct NativeSupervisorExecutionLease {
    root: PathBuf,
    token: Zeroizing<String>,
    trusted: Value,
    requested: RequestedRoute,
    selection: provider_federation::SupervisorModelEligibility,
    execution_key: String,
    lease_hash: String,
}
pub(super) fn from_sealed(
    root: &Path,
    token: &str,
    trusted: Value,
    requested: RequestedRoute,
    selection: provider_federation::SupervisorModelEligibility,
) -> anyhow::Result<NativeSupervisorExecutionLease> {
    let (execution_key, _, lease_hash) = lease_record(&trusted)?;
    Ok(NativeSupervisorExecutionLease {
        root: root.to_owned(),
        token: Zeroizing::new(token.to_owned()),
        trusted,
        requested,
        selection,
        execution_key,
        lease_hash,
    })
}
impl NativeSupervisorExecutionLease {
    /// The service's actual signed restricted command/confirmed-plan token.
    /// This cannot be built from requested-route DTOs or a consumer response.
    pub(crate) fn capture(root: &Path, token: &str) -> anyhow::Result<Self> {
        capture_lease(root, Some(token))?.context("selected Supervisor Luma required")
    }
    pub(crate) fn selection(&self) -> &provider_federation::SupervisorModelEligibility {
        &self.selection
    }
    pub(crate) fn harness(&self) -> &str {
        &self.requested.harness
    }
    fn verify_session(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            verify_internal_command_session_token(&self.root, &self.token)? == self.trusted,
            unavailable(
                "supervisor_execution_fenced",
                "native session changed or expired"
            )
        );
        Ok(())
    }
    fn current(
        &self,
        core: &Connection,
        policy: &Connection,
        facts: &ConsumerFacts,
    ) -> anyhow::Result<()> {
        let context = context_from_arguments_with_trusted_gateway_context(
            workjet_worker_dispatch::TOOL,
            &json!({}),
            Some(&self.trusted),
        )?;
        anyhow::ensure!(
            facts.owner_user_id == context.actor && facts.computer_id == self.requested.computer_id,
            unavailable(
                "supervisor_execution_fenced",
                "admitted holder is not the selected Owner/computer"
            )
        );
        let (project, thread, _) =
            workjet_jour_fixe::bound_project(core, policy, &context, &self.trusted).map_err(
                |_| {
                    unavailable(
                        "supervisor_execution_fenced",
                        "native lease or project authority retired",
                    )
                },
            )?;
        let route = resolve(policy, &context.actor, &project, &thread)
            .map_err(|_| {
                unavailable(
                    "supervisor_execution_fenced",
                    "native account, catalog or route changed",
                )
            })?
            .ok_or_else(|| {
                unavailable(
                    "supervisor_execution_fenced",
                    "selected Supervisor Luma was cleared",
                )
            })?;
        anyhow::ensure!(
            route == self.requested,
            unavailable(
                "supervisor_execution_fenced",
                "selected native route changed"
            )
        );
        self.selection
            .revalidate(policy, &context.actor)
            .map_err(|_| {
                unavailable(
                    "supervisor_execution_fenced",
                    "sealed native account or model eligibility changed",
                )
            })?;
        let sealed: Option<(String, String)> = core
            .query_row(
                "SELECT owner_user_id,requested_json FROM workjet_supervisor_route_attempts
             WHERE execution_key=?1 AND lease_hash=?2",
                params![self.execution_key, self.lease_hash],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        anyhow::ensure!(
            sealed == Some((context.actor, serde_json::to_string(&self.requested)?)),
            unavailable(
                "supervisor_execution_fenced",
                "native selection seal differs"
            )
        );
        Ok(())
    }
    /// Prepare private account/secret snapshots BEFORE this call. Lock order:
    /// source transport -> issuer -> Core -> Policy. Inside apply: no awaits,
    /// network, secrets, transport/account reentry or retained connections.
    /// Re-enter before dispatch and after every await, including stream/result
    /// publication. Both the original lease and current enrollment stay fenced.
    pub(crate) fn with_current<T>(
        &self,
        authority: &AdmittedConsumerAuthority,
        apply: impl FnOnce(&ConsumerFacts, &Connection, &Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        anyhow::ensure!(
            self.root == authority.native_host_root(),
            unavailable("supervisor_execution_fenced", "native holder root differs")
        );
        self.verify_session()?;
        authority.with_current_core(|facts, core, policy| {
            self.current(core, policy, facts)?;
            apply(facts, core, policy)
        })
    }
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_supervisor_execution_controllers (
    execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL, controller_id TEXT NOT NULL,
    consumer_json TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('active','cancelled','finished')),
    created_at_ms INTEGER NOT NULL, retired_at_ms INTEGER,
    PRIMARY KEY(execution_key,lease_hash), UNIQUE(controller_id));";

/// Owns the exact admitted peer/generation, not a reconstructed ConsumerFacts.
/// There can be one controller for an original lease, even after retirement.
pub(crate) struct NativeSupervisorHoldingController {
    lease: NativeSupervisorExecutionLease,
    authority: AdmittedConsumerAuthority,
    id: String,
    consumer_json: String,
    retired: AtomicBool,
}
fn claim_in_fence(
    lease: &NativeSupervisorExecutionLease,
    core: &Connection,
    policy: &Connection,
    facts: &ConsumerFacts,
) -> anyhow::Result<(String, String)> {
    lease.current(core, policy, facts)?;
    core.execute_batch(SCHEMA)?;
    let id = uuid::Uuid::new_v4().to_string();
    let consumer_json = serde_json::to_string(facts)?;
    let inserted = core.execute(
        "INSERT INTO workjet_supervisor_execution_controllers
         (execution_key,lease_hash,controller_id,consumer_json,state,created_at_ms)
         VALUES (?1,?2,?3,?4,'active',?5) ON CONFLICT(execution_key,lease_hash) DO NOTHING",
        params![
            lease.execution_key,
            lease.lease_hash,
            id,
            consumer_json,
            now_ms()
        ],
    )?;
    anyhow::ensure!(
        inserted == 1,
        unavailable(
            "supervisor_controller_already_claimed",
            "this native execution lease already has a controller"
        )
    );
    Ok((id, consumer_json))
}
fn current_controller(
    core: &Connection,
    lease: &NativeSupervisorExecutionLease,
    id: &str,
    consumer_json: &str,
) -> anyhow::Result<()> {
    let current: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM workjet_supervisor_execution_controllers
         WHERE execution_key=?1 AND lease_hash=?2 AND controller_id=?3 AND consumer_json=?4 AND state='active')",
        params![lease.execution_key,lease.lease_hash,id,consumer_json], |r| r.get(0),
    )?;
    anyhow::ensure!(
        current,
        unavailable(
            "supervisor_execution_fenced",
            "holding controller retired or replaced"
        )
    );
    Ok(())
}
impl NativeSupervisorHoldingController {
    /// Called only by the native guarded holding handler with its actual
    /// accepted connection. No client-supplied lease/controller is accepted.
    pub(crate) fn claim(
        lease: NativeSupervisorExecutionLease,
        authority: AdmittedConsumerAuthority,
    ) -> anyhow::Result<Self> {
        let (id, consumer_json) = lease.with_current(&authority, |facts, core, policy| {
            claim_in_fence(&lease, core, policy, facts)
        })?;
        Ok(Self {
            lease,
            authority,
            id,
            consumer_json,
            retired: AtomicBool::new(false),
        })
    }
    pub(crate) fn controller_id(&self) -> &str {
        &self.id
    }
    pub(crate) fn execution_key(&self) -> &str {
        &self.lease.execution_key
    }
    pub(crate) fn authority(&self) -> &AdmittedConsumerAuthority {
        &self.authority
    }
    pub(crate) fn selection(&self) -> &provider_federation::SupervisorModelEligibility {
        self.lease.selection()
    }
    pub(crate) fn with_current<T>(
        &self,
        apply: impl FnOnce(&ConsumerFacts, &Connection, &Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        anyhow::ensure!(
            !self.retired.load(Ordering::Acquire),
            unavailable("supervisor_execution_fenced", "holding controller retired")
        );
        self.lease
            .with_current(&self.authority, |facts, core, policy| {
                // Cancellation's atomic fence also wins while a DB reservation
                // is being admitted; the durable row fences other controllers.
                anyhow::ensure!(
                    !self.retired.load(Ordering::Acquire),
                    unavailable("supervisor_execution_fenced", "holding controller retired")
                );
                anyhow::ensure!(
                    serde_json::to_string(facts)? == self.consumer_json,
                    unavailable("supervisor_execution_fenced", "holder enrollment changed")
                );
                current_controller(core, &self.lease, &self.id, &self.consumer_json)?;
                apply(facts, core, policy)
            })
    }
    /// Retire only this private controller, also when the original peer/lease
    /// is gone. No other task/lease is stopped. The account proxy must retire
    /// its scoped capability and the SDK producer must separately prove stop.
    pub(crate) fn cancel(&self) -> anyhow::Result<()> {
        self.retired.store(true, Ordering::Release);
        let core = Connection::open(crate::paths::core_db(&self.lease.root))?;
        core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
        core.execute(
            "UPDATE workjet_supervisor_execution_controllers SET state='cancelled',retired_at_ms=?1
             WHERE execution_key=?2 AND lease_hash=?3 AND controller_id=?4 AND state='active'",
            params![
                now_ms(),
                self.lease.execution_key,
                self.lease.lease_hash,
                self.id
            ],
        )?;
        Ok(())
    }
}
#[cfg(test)]
#[path = "mcp_supervisor_holding_tests.rs"]
mod tests;
