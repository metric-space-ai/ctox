// Origin: CTOX
// License: AGPL-3.0-only

//! Explicit local-operator provider assignments, independent from guest enrollment.
//! A current credential or sole account is not an owner/profile entitlement.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderAssignmentInput {
    pub owner_user_id: String,
    pub worker_profile_id: String,
    pub gateway_account_id: String,
    pub model_id: String,
}

/// Called only by the trusted local operator's configure-guests CLI, never by
/// the enrollment socket, a renderer or a command-session request.
pub(crate) fn configure_provider_assignments(
    root: &Path,
    computer: &str,
    assignments: &[ProviderAssignmentInput],
) -> Result<()> {
    ensure!(
        identifier(computer) && assignments.len() <= 64,
        "invalid provider assignment configuration"
    );
    let mut conn = super::super::store::open_store(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    configure_in_transaction(&tx, computer, assignments)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn configure_in_transaction(
    tx: &Connection,
    computer: &str,
    assignments: &[ProviderAssignmentInput],
) -> Result<()> {
    ensure!(
        identifier(computer) && assignments.len() <= 64,
        "invalid provider assignment configuration"
    );
    tx.execute("UPDATE business_native_guest_provider_assignments SET state='revoked',revision=revision+1 WHERE computer_id=?1", [computer])?;
    let mut seen = BTreeSet::new();
    for assignment in assignments {
        ensure!(
            identifier(&assignment.owner_user_id)
                && identifier(&assignment.worker_profile_id)
                && !assignment.gateway_account_id.is_empty()
                && assignment.gateway_account_id.len() <= 256
                && assignment.gateway_account_id.trim() == assignment.gateway_account_id
                && !assignment.gateway_account_id.chars().any(char::is_control)
                && identifier(&assignment.model_id)
                && seen.insert((&assignment.owner_user_id, &assignment.worker_profile_id)),
            "invalid or duplicate provider assignment"
        );
        let profile = super::super::worker_profile_bindings::require_active(
            &tx,
            &assignment.owner_user_id,
            &assignment.worker_profile_id,
        )?;
        ensure!(
            profile["computer_id"] == computer,
            "provider assignment is for another computer"
        );
        let epoch: i64 = tx.query_row(
            "SELECT capability_epoch FROM business_users WHERE user_id=?1 AND active=1",
            [&assignment.owner_user_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO business_native_guest_provider_assignments
            (owner_user_id,worker_profile_id,computer_id,gateway_account_id,model_id,
             model_route_id,harness,harness_version,principal_epoch,state,revision,updated_at_ms)
             VALUES (?1,?2,?3,?4,?5,'openai',?6,?7,?8,'active',1,?9)
             ON CONFLICT(owner_user_id,worker_profile_id) DO UPDATE SET
             computer_id=excluded.computer_id,gateway_account_id=excluded.gateway_account_id,
             model_id=excluded.model_id,model_route_id=excluded.model_route_id,
             harness=excluded.harness,harness_version=excluded.harness_version,
             principal_epoch=excluded.principal_epoch,state='active',
             revision=business_native_guest_provider_assignments.revision+1,
             updated_at_ms=excluded.updated_at_ms",
            rusqlite::params![
                assignment.owner_user_id,
                assignment.worker_profile_id,
                computer,
                assignment.gateway_account_id,
                assignment.model_id,
                ctox_core::native_harness_name(),
                ctox_core::native_harness_version(),
                epoch,
                super::super::store::now_ms() as i64,
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn revoke_provider_assignment(root: &Path, owner: &str, profile: &str) -> Result<()> {
    ensure!(
        identifier(owner) && identifier(profile),
        "invalid provider revocation"
    );
    let mut conn = super::super::store::open_store(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE business_native_guest_provider_assignments SET state='revoked',revision=revision+1
        WHERE owner_user_id=?1 AND worker_profile_id=?2",
        rusqlite::params![owner, profile],
    )?;
    tx.commit()?;
    Ok(())
}

/// Optional during provisioning; absence cannot authorize a producer.
pub(super) fn snapshot(policy: &Connection, d: &GuestRestoreDestination) -> Result<Option<Value>> {
    snapshot_scope(policy, &d.human_owner_id, &d.worker_profile_id)
}

pub(super) fn snapshot_scope(
    policy: &Connection,
    owner: &str,
    profile: &str,
) -> Result<Option<Value>> {
    let exists: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table'
        AND name='business_native_guest_provider_assignments')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    Ok(policy.query_row(
        "SELECT a.computer_id,a.gateway_account_id,a.model_id,a.model_route_id,
        a.harness,a.harness_version,a.principal_epoch,a.updated_at_ms,u.active,u.capability_epoch,u.role,a.state,a.revision
        FROM business_native_guest_provider_assignments a
        JOIN business_users u ON u.user_id=a.owner_user_id
        WHERE a.owner_user_id=?1 AND a.worker_profile_id=?2",
        rusqlite::params![owner,profile],
        |row| Ok(json!({
            "computer_id":row.get::<_,String>(0)?,
            "gateway_account_id":row.get::<_,String>(1)?,
            "model_id":row.get::<_,String>(2)?,
            "model_route_id":row.get::<_,String>(3)?,
            "harness":row.get::<_,String>(4)?,
            "harness_version":row.get::<_,String>(5)?,
            "principal_epoch":row.get::<_,i64>(6)?,
            "updated_at_ms":row.get::<_,i64>(7)?,
            "principal_active":row.get::<_,i64>(8)?,
            "current_epoch":row.get::<_,i64>(9)?,
            "current_role":row.get::<_,String>(10)?,
            "state":row.get::<_,String>(11)?,
            "revision":row.get::<_,i64>(12)?,
        })),
    ).optional()?)
}

pub(super) fn require_assignment(
    policy: &Connection,
    d: &GuestRestoreDestination,
) -> Result<Value> {
    require_scope(policy, &d.human_owner_id, &d.worker_profile_id)
}

pub(super) fn require_scope(policy: &Connection, owner: &str, profile_id: &str) -> Result<Value> {
    let assigned = snapshot_scope(policy, owner, profile_id)?
        .context("native guest provider account is not explicitly assigned")?;
    let profile =
        super::super::worker_profile_bindings::require_active(policy, &owner, &profile_id)?;
    ensure!(
        assigned["state"] == "active"
            && assigned["principal_active"] == 1
            && assigned["principal_epoch"] == assigned["current_epoch"]
            && assigned["computer_id"] == profile["computer_id"],
        "native guest provider owner/epoch/computer assignment changed"
    );
    Ok(assigned)
}

pub(super) fn validate_provider(
    policy: &Connection,
    d: &GuestRestoreDestination,
    model: &str,
    contract: &crate::channels::NativeProviderCheckpointContract,
) -> Result<()> {
    let assigned = require_assignment(policy, d)?;
    ensure!(
        assigned["gateway_account_id"] == contract.gateway_account_id
            && assigned["model_id"] == model
            && assigned["model_route_id"] == contract.model_route_id
            && assigned["harness"] == contract.harness
            && assigned["harness_version"] == contract.harness_version,
        "actual native provider differs from the explicitly assigned account/model"
    );
    Ok(())
}

impl NativeGuestRegistry {
    pub(crate) fn authorize_provider_start(
        &self,
        guest: &str,
        context: &Value,
        model: &str,
        contract: &crate::channels::NativeProviderCheckpointContract,
    ) -> Result<()> {
        self.require_live_transport()?;
        let registration = self.registration(guest)?;
        self.with_policy(|policy| {
            let entry = registration
                .lock()
                .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
            let d = &entry.assignment.destination;
            ensure!(
                context["actor"].as_str() == Some(d.human_owner_id.as_str())
                    && context["expires_at_ms"]
                        .as_u64()
                        .is_some_and(|expiry| u128::from(expiry) > super::super::store::now_ms()),
                "native provider owner command changed or expired"
            );
            self.admission_destination(policy, &entry)?;
            validate_provider(policy, d, model, contract)
        })
    }
}
