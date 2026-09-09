// Origin: CTOX
// License: Apache-2.0

use super::policy::{
    self, owner_transfer_policy_decision, policy_actor_from_session, BusinessOsActor,
    BusinessOsPermission, BusinessOsScope, BusinessOsScopeType, PolicyDecision,
};
use super::session::{session_user_id, BusinessOsSession};
use super::store::{
    founder_owns_module, normalize_business_role, open_store, queue_command_policy_target,
    record_business_policy_decision_event, rxdb_authenticated_session,
    seed_configured_business_users, trusted_mcp_actor_with_conn,
    write_rxdb_policy_denied_command_outcome, BusinessCommand, WORKSPACE_AUTHORITY_ROLES,
};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;

fn should_record_allowed_policy_decision(command: &BusinessCommand) -> bool {
    !matches!(command.command_type.as_str(), "ctox.business_os.audit.list")
}

pub(super) fn module_policy_decision(
    root: &Path,
    session: &BusinessOsSession,
    permission: BusinessOsPermission,
    module_id: &str,
) -> anyhow::Result<PolicyDecision> {
    let actor = policy_actor_from_session(session);
    let conn = open_store(root)?;
    let assigned_to_actor = if actor.role.as_str() == "founder" {
        if let Some(user_id) = session_user_id(session) {
            founder_owns_module(&conn, module_id, user_id)?
        } else {
            false
        }
    } else {
        false
    };
    let scope = BusinessOsScope::module(module_id.trim(), assigned_to_actor);
    evaluate_policy_with_explicit_grants(&conn, &actor, permission, &scope)
}

pub(super) fn scoped_policy_decision(
    root: &Path,
    session: &BusinessOsSession,
    permission: BusinessOsPermission,
    scope: BusinessOsScope,
) -> anyhow::Result<PolicyDecision> {
    let actor = policy_actor_from_session(session);
    let conn = open_store(root)?;
    evaluate_policy_with_explicit_grants(&conn, &actor, permission, &scope)
}

pub fn trusted_mcp_actor_policy_decision(
    root: &Path,
    actor_id: &str,
    actor_display_name: &str,
    permission: BusinessOsPermission,
    scope_type: BusinessOsScopeType,
    scope_id: Option<&str>,
) -> anyhow::Result<PolicyDecision> {
    let conn = open_store(root)?;
    let actor = trusted_mcp_actor_with_conn(&conn, actor_id, actor_display_name)?;
    trusted_actor_policy_decision_with_conn(
        &conn,
        &actor.id,
        &actor.role,
        permission,
        scope_type,
        scope_id,
    )
}

pub fn trusted_mcp_actor_policy_decision_with_role(
    root: &Path,
    actor_id: &str,
    actor_role: &str,
    permission: BusinessOsPermission,
    scope_type: BusinessOsScopeType,
    scope_id: Option<&str>,
) -> anyhow::Result<PolicyDecision> {
    let conn = open_store(root)?;
    seed_configured_business_users(&conn)?;
    let actor_id = actor_id.trim();
    let actor_id = if actor_id.is_empty() {
        "mcp:local"
    } else {
        actor_id
    };
    trusted_actor_policy_decision_with_conn(
        &conn,
        actor_id,
        &normalize_business_role(actor_role),
        permission,
        scope_type,
        scope_id,
    )
}

pub(super) fn trusted_actor_policy_decision_with_conn(
    conn: &Connection,
    actor_id: &str,
    actor_role: &str,
    permission: BusinessOsPermission,
    scope_type: BusinessOsScopeType,
    scope_id: Option<&str>,
) -> anyhow::Result<PolicyDecision> {
    let actor = BusinessOsActor::new(Some(actor_id.to_owned()), actor_role);
    let scope = match scope_type {
        BusinessOsScopeType::Workspace => BusinessOsScope::workspace(),
        BusinessOsScopeType::Module => {
            let module_id = scope_id.unwrap_or("").trim();
            let assigned_to_actor =
                actor.role.as_str() == "founder" && founder_owns_module(conn, module_id, actor_id)?;
            BusinessOsScope::module(module_id, assigned_to_actor)
        }
        BusinessOsScopeType::Task => {
            BusinessOsScope::task(scope_id.unwrap_or("").trim(), false, false)
        }
        other => BusinessOsScope {
            scope_type: other,
            scope_id: scope_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            assigned_to_actor: false,
            owned_by_actor: false,
        },
    };
    evaluate_policy_with_explicit_grants(conn, &actor, permission, &scope)
}

pub fn session_can_manage_workspace_branding(
    root: &Path,
    session: &BusinessOsSession,
) -> anyhow::Result<bool> {
    Ok(
        workspace_policy_decision(root, session, BusinessOsPermission::WorkspaceBrandingManage)?
            .allowed,
    )
}

pub fn session_can_modify_module(
    root: &Path,
    session: &BusinessOsSession,
    module_id: &str,
) -> anyhow::Result<bool> {
    Ok(module_policy_decision(root, session, BusinessOsPermission::AppsModify, module_id)?.allowed)
}

pub(super) fn session_has_workspace_permission(
    root: &Path,
    session: &BusinessOsSession,
    permission: BusinessOsPermission,
) -> anyhow::Result<bool> {
    Ok(scoped_policy_decision(root, session, permission, BusinessOsScope::workspace())?.allowed)
}

pub(super) fn session_can_manage_task(
    root: &Path,
    session: &BusinessOsSession,
    task_id: &str,
) -> anyhow::Result<bool> {
    Ok(task_policy_decision(root, session, task_id)?.allowed)
}

pub(super) fn evaluate_policy_with_explicit_grants(
    conn: &Connection,
    actor: &BusinessOsActor,
    permission: BusinessOsPermission,
    scope: &BusinessOsScope,
) -> anyhow::Result<PolicyDecision> {
    let decision = policy::evaluate(actor, permission, scope);
    if scope
        .scope_id
        .as_deref()
        .is_some_and(policy::is_cockpit_projection)
        && matches!(
            permission,
            BusinessOsPermission::DataRead | BusinessOsPermission::DataWrite
        )
    {
        return Ok(decision);
    }
    if decision.allowed {
        return Ok(decision);
    }
    if permission == BusinessOsPermission::CrewManage {
        return Ok(decision);
    }
    if active_permission_grant_allows(conn, actor, permission, scope)? {
        return Ok(policy::allow_decision(permission, scope));
    }
    Ok(decision)
}

pub(super) fn active_permission_grant_allows(
    conn: &Connection,
    actor: &BusinessOsActor,
    permission: BusinessOsPermission,
    scope: &BusinessOsScope,
) -> anyhow::Result<bool> {
    let actor_id = actor.id.as_deref().unwrap_or("").trim();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM business_permission_grants
         WHERE active = 1
           AND permission = ?1
           AND scope_type = ?2
           AND scope_id = ?3
           AND (
                (subject_type = 'role' AND subject_id = ?4)
                OR (subject_type = 'user' AND subject_id = ?5)
           )",
        params![
            permission.as_str(),
            scope.scope_type.as_str(),
            scope.scope_id.as_deref().unwrap_or(""),
            actor.role.as_str(),
            actor_id,
        ],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub(super) fn queue_command_policy_decision(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<PolicyDecision> {
    let (permission, module_id, explicitly_scoped) = queue_command_policy_target(command);
    if explicitly_scoped {
        scoped_policy_decision(
            root,
            session,
            permission,
            BusinessOsScope::module(module_id, false),
        )
    } else {
        module_policy_decision(root, session, permission, &module_id)
    }
}

pub(super) fn reject_command_if_policy_denied(
    root: &Path,
    command: &BusinessCommand,
    decision: &PolicyDecision,
) -> anyhow::Result<Option<Value>> {
    if decision.allowed {
        if should_record_allowed_policy_decision(command) {
            record_business_policy_decision_event(root, command, decision)?;
        }
        return Ok(None);
    }
    {
        let conn = super::store::open_store(root)?;
        if super::domain_effect::contains(&conn, command.id.as_deref().unwrap_or_default())? {
            // A denied replay cannot fail an already committed effect.
            record_business_policy_decision_event(root, command, decision)?;
            anyhow::bail!("policy denied domain effect recovery");
        }
    }
    Ok(Some(write_rxdb_policy_denied_command_outcome(
        root, command, decision,
    )?))
}

pub(super) enum CommandPolicyRequirement {
    Module {
        permission: BusinessOsPermission,
        module_id: String,
    },
    Workspace {
        permission: BusinessOsPermission,
    },
    Scoped {
        permission: BusinessOsPermission,
        scope: BusinessOsScope,
    },
}

impl CommandPolicyRequirement {
    pub(super) fn module(permission: BusinessOsPermission, module_id: impl Into<String>) -> Self {
        Self::Module {
            permission,
            module_id: module_id.into(),
        }
    }

    pub(super) fn workspace(permission: BusinessOsPermission) -> Self {
        Self::Workspace { permission }
    }

    pub(super) fn scoped(permission: BusinessOsPermission, scope: BusinessOsScope) -> Self {
        Self::Scoped { permission, scope }
    }
}

/// The result of an allowed command continuation or the persisted denial outcome.
///
/// The inner result is deliberately opaque: callers must consume it with
/// `into_outcome`, and the continuation that can mutate state is never invoked
/// when policy denies the command.
#[must_use = "return the enforced command outcome; dropping it discards the command result"]
pub(super) struct EnforcedCommandOutcome(anyhow::Result<Value>);

impl EnforcedCommandOutcome {
    pub(super) fn into_outcome(self) -> anyhow::Result<Value> {
        self.0
    }
}

/// Resolve the authenticated session, choose and evaluate policy, persist a
/// denial when necessary, and invoke `on_allowed` only after all gates pass.
///
/// The outer `Result` contains session, requirement-resolution, and policy-
/// evaluation failures. The opaque inner outcome contains denial-persistence
/// failures, the persisted denial, or the allowed command's result. Keeping
/// those layers separate preserves callers that report authorization lookup
/// failures differently from command outcome failures.
#[must_use = "authorization must be consumed and returned to the command caller"]
pub(super) fn enforce_command_policy<Resolve, OnAllowed>(
    root: &Path,
    command: &BusinessCommand,
    resolve_requirement: Resolve,
    on_allowed: OnAllowed,
) -> anyhow::Result<EnforcedCommandOutcome>
where
    Resolve: FnOnce(&BusinessOsSession) -> anyhow::Result<CommandPolicyRequirement>,
    OnAllowed: FnOnce(&BusinessOsSession) -> anyhow::Result<Value>,
{
    let session = rxdb_authenticated_session(root, command)?;
    enforce_command_policy_with_session(root, command, &session, resolve_requirement, on_allowed)
}

/// The native receipt-recovery caller supplies an active user read from its
/// durable application identity. Network callers must use enforce_command_policy.
pub(super) fn enforce_command_policy_with_session<Resolve, OnAllowed>(
    root: &Path,
    command: &BusinessCommand,
    session: &BusinessOsSession,
    resolve_requirement: Resolve,
    on_allowed: OnAllowed,
) -> anyhow::Result<EnforcedCommandOutcome>
where
    Resolve: FnOnce(&BusinessOsSession) -> anyhow::Result<CommandPolicyRequirement>,
    OnAllowed: FnOnce(&BusinessOsSession) -> anyhow::Result<Value>,
{
    let decision = match resolve_requirement(session)? {
        CommandPolicyRequirement::Module {
            permission,
            module_id,
        } => module_policy_decision(root, session, permission, &module_id)?,
        CommandPolicyRequirement::Workspace { permission } => {
            workspace_policy_decision(root, session, permission)?
        }
        CommandPolicyRequirement::Scoped { permission, scope } => {
            scoped_policy_decision(root, session, permission, scope)?
        }
    };
    match reject_command_if_policy_denied(root, command, &decision) {
        Ok(Some(outcome)) => Ok(EnforcedCommandOutcome(Ok(outcome))),
        Ok(None) => Ok(EnforcedCommandOutcome(on_allowed(session))),
        Err(error) => Ok(EnforcedCommandOutcome(Err(error))),
    }
}

pub(super) fn workspace_policy_decision(
    root: &Path,
    session: &BusinessOsSession,
    permission: BusinessOsPermission,
) -> anyhow::Result<PolicyDecision> {
    scoped_policy_decision(root, session, permission, BusinessOsScope::workspace())
}

pub(super) fn user_upsert_policy_decision(
    root: &Path,
    session: &BusinessOsSession,
    target_role: &str,
) -> anyhow::Result<PolicyDecision> {
    let decision = workspace_policy_decision(root, session, BusinessOsPermission::UsersManage)?;
    if !decision.allowed || !WORKSPACE_AUTHORITY_ROLES.contains(&target_role) {
        return Ok(decision);
    }
    Ok(owner_transfer_policy_decision(session))
}

pub(super) fn task_policy_decision(
    root: &Path,
    session: &BusinessOsSession,
    task_id: &str,
) -> anyhow::Result<PolicyDecision> {
    scoped_policy_decision(
        root,
        session,
        BusinessOsPermission::CtoxTaskManage,
        BusinessOsScope::task(task_id.trim(), false, false),
    )
}

#[cfg(test)]
mod tests {
    use super::super::policy::{BusinessOsPermission, BusinessOsScopeType};
    use super::super::store::tests::{seed_business_user, test_session};
    use super::super::store::{
        accept_rxdb_business_command, now_ms, open_store, outbound_load_record,
        push_collection_records,
    };
    use super::{session_can_modify_module, trusted_mcp_actor_policy_decision};
    use rusqlite::params;
    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn control_command_policy_contract_denial_blocks_mutation() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "viewer", "user")?;

        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "policy-contract-denied",
                "command_id": "policy-contract-denied",
                "module": "customers",
                "command_type": "customers.account.create",
                "record_id": "account-must-not-be-created",
                "payload": {
                    "account_id": "account-must-not-be-created",
                    "name": "Denied GmbH"
                },
                "client_context": {
                    "actor": { "id": "viewer", "display_name": "Viewer" }
                }
            }),
        )?;

        assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            outcome
                .pointer("/result/policy_decision/permission")
                .and_then(Value::as_str),
            Some(BusinessOsPermission::DataWrite.as_str())
        );
        assert_eq!(
            outcome
                .pointer("/result/policy_decision/scope_id")
                .and_then(Value::as_str),
            Some("customers")
        );
        let conn = open_store(root)?;
        assert!(
            outbound_load_record(&conn, "customer_accounts", "account-must-not-be-created")?
                .is_none(),
            "the contract must not invoke the customer mutation after denial"
        );
        Ok(())
    }

    #[test]
    fn permission_grant_allows_scoped_module_action_without_new_role() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let now = now_ms() as i64;
        conn.execute(
            "INSERT INTO business_users
                (user_id, display_name, role, active, created_at_ms, updated_at_ms)
             VALUES ('viewer', 'Viewer', 'user', 1, ?1, ?1)",
            params![now],
        )?;
        conn.execute(
            "INSERT INTO business_permission_grants
                (grant_id, subject_type, subject_id, permission, scope_type, scope_id,
                 active, reason, created_by, created_at_ms, updated_at_ms)
             VALUES (?1, 'user', 'viewer', ?2, 'module', 'inventory', 1,
                 'temporary app editor', 'tester', ?3, ?3)",
            params![
                "grant_viewer_inventory_modify",
                BusinessOsPermission::AppsModify.as_str(),
                now
            ],
        )?;
        drop(conn);

        let session = test_session("viewer", "user");
        assert!(
            session_can_modify_module(root, &session, "inventory")?,
            "explicit module grant should allow a Teammitglied to modify that module"
        );
        assert!(
            !session_can_modify_module(root, &session, "billing")?,
            "the same grant must not leak to another module"
        );
        Ok(())
    }

    #[test]
    fn record_owner_payload_field_does_not_grant_native_policy_access() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "record-owner", "team")?;
        push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_opportunities",
                "documents": [{
                    "id": "opp_1",
                    "name": "Opportunity",
                    "owner_id": "record-owner",
                    "updated_at_ms": 10
                }]
            }),
        )?;

        let decision = trusted_mcp_actor_policy_decision(
            root,
            "record-owner",
            "Record Owner",
            BusinessOsPermission::DataRead,
            BusinessOsScopeType::Record,
            Some("customer_opportunities/opp_1"),
        )?;

        assert!(
            !decision.allowed,
            "owner-like payload fields must not become implicit record grants"
        );
        assert_eq!(decision.permission, BusinessOsPermission::DataRead.as_str());
        assert_eq!(decision.scope_type, BusinessOsScopeType::Record.as_str());
        assert_eq!(
            decision.scope_id.as_deref(),
            Some("customer_opportunities/opp_1")
        );
        assert_eq!(decision.reason_code, "role_or_scope_denied");
        Ok(())
    }
}
