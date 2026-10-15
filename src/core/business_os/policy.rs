// Origin: CTOX
// License: AGPL-3.0-only

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessOsRole {
    Chef,
    Admin,
    Founder,
    User,
}

impl BusinessOsRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chef => "chef",
            Self::Admin => "admin",
            Self::Founder => "founder",
            Self::User => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BusinessOsPermission {
    WorkspaceManage,
    WorkspaceBrandingManage,
    UsersManage,
    RolesManage,
    RuntimeManage,
    IntegrationsManage,
    SecretsManage,
    McpManage,
    AppsInstall,
    AppsUninstall,
    AppsAssignOwner,
    AppsView,
    AppsModify,
    AppsRelease,
    AppsRollback,
    AppsSourceView,
    DataRead,
    DataWrite,
    CtoxTaskCreate,
    CtoxTaskManage,
    CrewManage,
    ExternalApprove,
    SupportRead,
    SupportTriage,
    SupportAssign,
    SupportReply,
    SupportResolve,
    SupportManageInboxes,
    SupportManageMacros,
    SupportManageSla,
    SupportAgentRequest,
    SupportAgentApply,
    /// Exact-grant-only session-handoff permissions. No role, workspace manage,
    /// data permission or sync membership implies them (issue183).
    SessionHandoffDisclose,
    SessionHandoffReceive,
    SessionHandoffExecute,
}

pub const BUSINESS_OS_PERMISSIONS: [BusinessOsPermission; 35] = [
    BusinessOsPermission::WorkspaceManage,
    BusinessOsPermission::WorkspaceBrandingManage,
    BusinessOsPermission::UsersManage,
    BusinessOsPermission::RolesManage,
    BusinessOsPermission::RuntimeManage,
    BusinessOsPermission::IntegrationsManage,
    BusinessOsPermission::SecretsManage,
    BusinessOsPermission::McpManage,
    BusinessOsPermission::AppsInstall,
    BusinessOsPermission::AppsUninstall,
    BusinessOsPermission::AppsAssignOwner,
    BusinessOsPermission::AppsView,
    BusinessOsPermission::AppsModify,
    BusinessOsPermission::AppsRelease,
    BusinessOsPermission::AppsRollback,
    BusinessOsPermission::AppsSourceView,
    BusinessOsPermission::DataRead,
    BusinessOsPermission::DataWrite,
    BusinessOsPermission::CtoxTaskCreate,
    BusinessOsPermission::CtoxTaskManage,
    BusinessOsPermission::CrewManage,
    BusinessOsPermission::ExternalApprove,
    BusinessOsPermission::SupportRead,
    BusinessOsPermission::SupportTriage,
    BusinessOsPermission::SupportAssign,
    BusinessOsPermission::SupportReply,
    BusinessOsPermission::SupportResolve,
    BusinessOsPermission::SupportManageInboxes,
    BusinessOsPermission::SupportManageMacros,
    BusinessOsPermission::SupportManageSla,
    BusinessOsPermission::SupportAgentRequest,
    BusinessOsPermission::SupportAgentApply,
    BusinessOsPermission::SessionHandoffDisclose,
    BusinessOsPermission::SessionHandoffReceive,
    BusinessOsPermission::SessionHandoffExecute,
];

impl BusinessOsPermission {
    pub fn all() -> &'static [BusinessOsPermission] {
        &BUSINESS_OS_PERMISSIONS
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceManage => "workspace.manage",
            Self::WorkspaceBrandingManage => "workspace.branding.manage",
            Self::UsersManage => "users.manage",
            Self::RolesManage => "roles.manage",
            Self::RuntimeManage => "runtime.manage",
            Self::IntegrationsManage => "integrations.manage",
            Self::SecretsManage => "secrets.manage",
            Self::McpManage => "mcp.manage",
            Self::AppsInstall => "apps.install",
            Self::AppsUninstall => "apps.uninstall",
            Self::AppsAssignOwner => "apps.assign_owner",
            Self::AppsView => "apps.view",
            Self::AppsModify => "apps.modify",
            Self::AppsRelease => "apps.release",
            Self::AppsRollback => "apps.rollback",
            Self::AppsSourceView => "apps.source.view",
            Self::DataRead => "data.read",
            Self::DataWrite => "data.write",
            Self::CtoxTaskCreate => "ctox.task.create",
            Self::CtoxTaskManage => "ctox.task.manage",
            Self::CrewManage => "ctox.crew.manage",
            Self::ExternalApprove => "external.approve",
            Self::SupportRead => "support.read",
            Self::SupportTriage => "support.triage",
            Self::SupportAssign => "support.assign",
            Self::SupportReply => "support.reply",
            Self::SupportResolve => "support.resolve",
            Self::SupportManageInboxes => "support.manage_inboxes",
            Self::SupportManageMacros => "support.manage_macros",
            Self::SupportManageSla => "support.manage_sla",
            Self::SupportAgentRequest => "support.agent_request",
            Self::SupportAgentApply => "support.agent_apply",
            Self::SessionHandoffDisclose => "ctox.session_handoff.disclose",
            Self::SessionHandoffReceive => "ctox.session_handoff.receive",
            Self::SessionHandoffExecute => "ctox.session_handoff.execute",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BusinessOsScopeType {
    Workspace,
    Module,
    Collection,
    Record,
    Task,
    Approval,
    Mcp,
    /// Exact binding-scoped grants for native session handoff (issue183).
    SessionHandoff,
}

impl BusinessOsScopeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Module => "module",
            Self::Collection => "collection",
            Self::Record => "record",
            Self::Task => "task",
            Self::Approval => "approval",
            Self::Mcp => "mcp",
            Self::SessionHandoff => "session_handoff",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessOsActor {
    pub id: Option<String>,
    pub role: BusinessOsRole,
}

impl BusinessOsActor {
    pub fn new(id: Option<String>, role: impl AsRef<str>) -> Self {
        Self {
            id,
            role: parse_role(role.as_ref()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessOsScope {
    pub scope_type: BusinessOsScopeType,
    pub scope_id: Option<String>,
    pub assigned_to_actor: bool,
    pub owned_by_actor: bool,
}

impl BusinessOsScope {
    pub fn workspace() -> Self {
        Self {
            scope_type: BusinessOsScopeType::Workspace,
            scope_id: None,
            assigned_to_actor: false,
            owned_by_actor: false,
        }
    }

    pub fn module(module_id: impl Into<String>, assigned_to_actor: bool) -> Self {
        Self {
            scope_type: BusinessOsScopeType::Module,
            scope_id: Some(module_id.into()),
            assigned_to_actor,
            owned_by_actor: false,
        }
    }

    pub fn collection(collection: impl Into<String>) -> Self {
        Self {
            scope_type: BusinessOsScopeType::Collection,
            scope_id: Some(collection.into()),
            assigned_to_actor: false,
            owned_by_actor: false,
        }
    }

    pub fn record(record_id: impl Into<String>) -> Self {
        Self {
            scope_type: BusinessOsScopeType::Record,
            scope_id: Some(record_id.into()),
            assigned_to_actor: false,
            owned_by_actor: false,
        }
    }

    #[allow(dead_code)]
    pub fn task(task_id: impl Into<String>, owned_by_actor: bool, assigned_to_actor: bool) -> Self {
        Self {
            scope_type: BusinessOsScopeType::Task,
            scope_id: Some(task_id.into()),
            assigned_to_actor,
            owned_by_actor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub permission: &'static str,
    pub scope_type: &'static str,
    pub scope_id: Option<String>,
    pub reason_code: &'static str,
    pub display_reason: &'static str,
    pub requires_approval: bool,
    pub audit_level: &'static str,
}

impl PolicyDecision {
    fn allow(permission: BusinessOsPermission, scope: &BusinessOsScope) -> Self {
        Self {
            allowed: true,
            permission: permission.as_str(),
            scope_type: scope.scope_type.as_str(),
            scope_id: scope.scope_id.clone(),
            reason_code: "allowed",
            display_reason: "Allowed.",
            requires_approval: matches!(
                permission,
                BusinessOsPermission::DataWrite | BusinessOsPermission::AppsModify
            ),
            audit_level: "decision",
        }
    }

    fn deny(
        permission: BusinessOsPermission,
        scope: &BusinessOsScope,
        reason_code: &'static str,
        display_reason: &'static str,
    ) -> Self {
        Self {
            allowed: false,
            permission: permission.as_str(),
            scope_type: scope.scope_type.as_str(),
            scope_id: scope.scope_id.clone(),
            reason_code,
            display_reason,
            // A role/scope denial for an otherwise delegable mutation is not
            // terminal: the actor may ask an authorized reviewer to execute
            // the frozen target through Threads. Read denials and privileged
            // workspace controls stay non-delegable.
            requires_approval: matches!(
                permission,
                BusinessOsPermission::DataWrite | BusinessOsPermission::AppsModify
            ),
            audit_level: "decision",
        }
    }
}

/// Actor-less V3 grants cannot bypass the Store decision. Import explicit grants
/// into _rights with the base release and use evaluate with the actual actor.
pub fn allow_decision(permission: BusinessOsPermission, scope: &BusinessOsScope) -> PolicyDecision {
    PolicyDecision::deny(
        permission,
        scope,
        "legacy_grant_requires_store_import",
        "Legacy grant must be imported into current Store rights.",
    )
}

pub fn normalize_role(role: &str) -> String {
    parse_role(role).as_str().to_owned()
}

pub fn parse_role(role: &str) -> BusinessOsRole {
    match role.trim().to_ascii_lowercase().as_str() {
        "owner" | "chef" => BusinessOsRole::Chef,
        "admin" | "business_os_admin" => BusinessOsRole::Admin,
        "founder" => BusinessOsRole::Founder,
        "user" | "business_os_user" | "team" | "business_os_team" => BusinessOsRole::User,
        _ => BusinessOsRole::User,
    }
}

pub fn role_can_manage(role: &str) -> bool {
    evaluate(
        &BusinessOsActor::new(None, role),
        BusinessOsPermission::WorkspaceBrandingManage,
        &BusinessOsScope::workspace(),
    )
    .allowed
}

pub(crate) fn role_sees_private_crew_fields(role: &str) -> bool {
    with_policy_rights(|rights| {
        Ok(rights
            .resolve_collection(
                parse_role(role).as_str(),
                "__role_summary__",
                "ctox_crew_members",
            )?
            .projection("ctox_crew_members")
            .is_some_and(|p| p.field_mask == sync_rights::scope::FieldMask::All))
    })
    .unwrap_or(false)
}

pub(crate) fn crew_fields_for_role(role: &str) -> Option<Vec<String>> {
    if role_sees_private_crew_fields(role) {
        None
    } else {
        Some(
            crate::crew::PUBLIC_MEMBER_FIELDS
                .iter()
                .map(|field| field.to_string())
                .collect(),
        )
    }
}

/// Legacy sensitivity classifications used only by the base-release import.
/// Runtime access is decided from its committed _rights rows, never this list.
pub const ADMIN_ONLY_COLLECTIONS: &[&str] = &[
    "business_users",
    "business_module_acl",
    "business_credentials",
    "business_module_source_files",
    "business_module_commits",
    "business_module_source_blob_chunks",
    "ctox_runtime_settings",
];

/// Shared classification for native permission checks, grants and audit output.
pub fn is_cockpit_projection(collection: &str) -> bool {
    matches!(
        collection,
        "ctox_harness_events"
            | "ctox_harness_status"
            | "ctox_runs"
            | "ctox_crew_members"
            | "ctox_crew_learnings"
    )
}

/// Owner decision 08.09.2026: a user sees who the crew is and whether the
/// harness runs (names, expressions, pause, capacity) — never runs, event
/// streams or learnings, which carry costs and other people's work.
pub fn user_readable_cockpit_projection(collection: &str) -> bool {
    matches!(collection, "ctox_crew_members" | "ctox_harness_status")
}

/// Base-release import is the only producer of the former default matrix.
/// Persist its result through StoreRightsAuthority::replace; never call it as
/// a runtime fallback. Registry supplies every real collection explicitly.
pub fn base_release_rights(collections: &[String]) -> Vec<sync_rights::Right> {
    use sync_rights::{
        scope::{FieldMask, FilterValue, RowFilter},
        Operation, Right,
    };
    fn eq(path: &str, value: Value) -> RowFilter {
        RowFilter::Eq {
            path: path.into(),
            value: FilterValue::Literal { value },
        }
    }
    fn or(filters: Vec<RowFilter>) -> RowFilter {
        RowFilter::Or { filters }
    }
    fn and(filters: Vec<RowFilter>) -> RowFilter {
        RowFilter::And { filters }
    }
    let mut rows = Vec::new();
    for role in [
        BusinessOsRole::Chef,
        BusinessOsRole::Admin,
        BusinessOsRole::Founder,
        BusinessOsRole::User,
    ] {
        let admin = matches!(role, BusinessOsRole::Chef | BusinessOsRole::Admin);
        for permission in BusinessOsPermission::all() {
            let assigned = eq("/assigned", Value::Bool(true));
            let owned = eq("/owned", Value::Bool(true));
            let record = eq("/scope_type", Value::String("record".into()));
            let yes = if admin {
                RowFilter::All
            } else {
                RowFilter::None
            };
            let filter = match permission {
                BusinessOsPermission::WorkspaceManage => {
                    if role == BusinessOsRole::Chef {
                        RowFilter::All
                    } else {
                        RowFilter::None
                    }
                }
                BusinessOsPermission::WorkspaceBrandingManage
                | BusinessOsPermission::UsersManage
                | BusinessOsPermission::RolesManage
                | BusinessOsPermission::RuntimeManage
                | BusinessOsPermission::IntegrationsManage
                | BusinessOsPermission::SecretsManage
                | BusinessOsPermission::McpManage
                | BusinessOsPermission::AppsInstall
                | BusinessOsPermission::AppsUninstall
                | BusinessOsPermission::AppsAssignOwner
                | BusinessOsPermission::SupportManageInboxes
                | BusinessOsPermission::SupportManageMacros
                | BusinessOsPermission::SupportManageSla => yes,
                BusinessOsPermission::AppsView
                | BusinessOsPermission::AppsModify
                | BusinessOsPermission::AppsRelease
                | BusinessOsPermission::AppsRollback
                | BusinessOsPermission::AppsSourceView => {
                    if admin {
                        RowFilter::All
                    } else if role == BusinessOsRole::Founder {
                        and(vec![
                            eq("/scope_type", Value::String("module".into())),
                            assigned,
                        ])
                    } else {
                        RowFilter::None
                    }
                }
                BusinessOsPermission::DataRead => or(vec![yes, assigned]),
                BusinessOsPermission::DataWrite => {
                    or(vec![yes, assigned, and(vec![record, owned])])
                }
                BusinessOsPermission::CtoxTaskCreate => RowFilter::All,
                BusinessOsPermission::CtoxTaskManage => or(vec![yes, owned, assigned]),
                BusinessOsPermission::CrewManage => {
                    if admin {
                        RowFilter::All
                    } else if role == BusinessOsRole::Founder {
                        and(vec![
                            record,
                            or(vec![
                                eq(
                                    "/scope_id",
                                    Value::String("ctox.crew.learning.confirm".into()),
                                ),
                                eq(
                                    "/scope_id",
                                    Value::String("ctox.crew.learning.update".into()),
                                ),
                            ]),
                        ])
                    } else {
                        RowFilter::None
                    }
                }
                BusinessOsPermission::ExternalApprove
                | BusinessOsPermission::SupportRead
                | BusinessOsPermission::SupportTriage
                | BusinessOsPermission::SupportAssign
                | BusinessOsPermission::SupportReply
                | BusinessOsPermission::SupportResolve
                | BusinessOsPermission::SupportAgentRequest
                | BusinessOsPermission::SupportAgentApply => or(vec![yes, assigned]),
                BusinessOsPermission::SessionHandoffDisclose
                | BusinessOsPermission::SessionHandoffReceive
                | BusinessOsPermission::SessionHandoffExecute => RowFilter::None,
            };
            rows.push(Right {
                role: role.as_str().into(),
                collection: format!("policy:{}", permission.as_str()),
                row_filter: filter,
                field_mask: FieldMask::All,
                ops: [Operation::Command].into(),
            });
        }
        for collection in collections {
            let visible = admin
                || (!ADMIN_ONLY_COLLECTIONS.contains(&collection.as_str())
                    && (role != BusinessOsRole::User
                        || !is_cockpit_projection(collection)
                        || user_readable_cockpit_projection(collection)));
            if !visible {
                continue;
            }
            let field_mask = if collection == "ctox_crew_members" && role == BusinessOsRole::User {
                FieldMask::Fields {
                    paths: crate::crew::PUBLIC_MEMBER_FIELDS
                        .iter()
                        .map(|f| format!("/{}", f.replace('~', "~0").replace('/', "~1")))
                        .collect(),
                }
            } else {
                FieldMask::All
            };
            let mut ops = [Operation::Read]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            if admin && !is_cockpit_projection(collection) {
                ops.extend([Operation::Write, Operation::Delete]);
            }
            rows.push(Right {
                role: role.as_str().into(),
                collection: collection.clone(),
                row_filter: RowFilter::All,
                field_mask,
                ops,
            });
        }
    }
    rows
}

static POLICY_AUTHORITY: std::sync::RwLock<
    Option<std::sync::Weak<sync_rights::StoreRightsAuthority>>,
> = std::sync::RwLock::new(None);
/// Retain for the daemon lifetime. Drop removes only this exact registration.
/// No credential, Store connection or duplicate rights snapshot is retained.
pub struct PolicyAuthorityRegistration(std::sync::Arc<sync_rights::StoreRightsAuthority>);
pub fn bind_policy_authority(
    authority: std::sync::Arc<sync_rights::StoreRightsAuthority>,
) -> sync_rights::scope::Result<PolicyAuthorityRegistration> {
    let mut current = POLICY_AUTHORITY
        .write()
        .map_err(|_| "policy authority poisoned")?;
    if current
        .as_ref()
        .and_then(std::sync::Weak::upgrade)
        .is_some()
    {
        return Err("policy authority already bound".into());
    }
    *current = Some(std::sync::Arc::downgrade(&authority));
    Ok(PolicyAuthorityRegistration(authority))
}
impl Drop for PolicyAuthorityRegistration {
    fn drop(&mut self) {
        if let Ok(mut current) = POLICY_AUTHORITY.write() {
            if current
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|a| std::sync::Arc::ptr_eq(&a, &self.0))
            {
                *current = None;
            }
        }
    }
}
fn with_policy_rights<T>(
    f: impl FnOnce(&sync_rights::RightsSnapshot) -> sync_rights::scope::Result<T>,
) -> sync_rights::scope::Result<T> {
    let authority = POLICY_AUTHORITY
        .read()
        .map_err(|_| "policy authority poisoned")?
        .as_ref()
        .and_then(std::sync::Weak::upgrade)
        .ok_or("policy authority unavailable")?;
    authority.router.with_current_rights(f)
}
pub fn role_may_read_collection(role: BusinessOsRole, collection: &str) -> bool {
    with_policy_rights(|rights| {
        Ok(rights
            .resolve_collection(role.as_str(), "__role_summary__", collection)?
            .projection(collection)
            .is_some_and(|p| p.row_filter != sync_rights::scope::RowFilter::None))
    })
    .unwrap_or(false)
}

/// All former role defaults are now seq-bearing Store rows. Unknown authority,
/// missing grants and stopped/pending writers deny without seeding on demand.
pub fn evaluate(
    actor: &BusinessOsActor,
    permission: BusinessOsPermission,
    scope: &BusinessOsScope,
) -> PolicyDecision {
    with_policy_rights(|rights| Ok(evaluate_with_rights(rights, actor, permission, scope)))
        .unwrap_or_else(|_| {
            PolicyDecision::deny(
                permission,
                scope,
                "rights_unavailable",
                "Current Store rights are unavailable.",
            )
        })
}
pub fn evaluate_with_rights(
    rights: &sync_rights::RightsSnapshot,
    actor: &BusinessOsActor,
    permission: BusinessOsPermission,
    scope: &BusinessOsScope,
) -> PolicyDecision {
    use sync_rights::Operation;
    let context = serde_json::json!({"scope_type":scope.scope_type.as_str(), "scope_id":scope.scope_id,
        "assigned":scope.assigned_to_actor, "owned":scope.owned_by_actor, "actor_id":actor.id});
    if scope.scope_type == BusinessOsScopeType::Collection
        && matches!(
            permission,
            BusinessOsPermission::DataRead | BusinessOsPermission::DataWrite
        )
    {
        let op = if permission == BusinessOsPermission::DataRead {
            Operation::Read
        } else {
            Operation::Write
        };
        let allowed = scope.scope_id.as_ref().is_some_and(|name| {
            rights.allows_unrestricted_collection(
                actor.role.as_str(),
                actor.id.as_deref().unwrap_or(""),
                name,
                op,
            )
        });
        return if allowed {
            PolicyDecision::allow(permission, scope)
        } else {
            PolicyDecision::deny(
                permission,
                scope,
                "role_or_scope_denied",
                "Current Store rights deny this collection.",
            )
        };
    }
    let allowed = rights
        .rules
        .iter()
        .find(|r| {
            r.role == actor.role.as_str()
                && r.collection == format!("policy:{}", permission.as_str())
                && r.ops.contains(&Operation::Command)
        })
        .and_then(|r| r.row_filter.resolve(actor.id.as_deref().unwrap_or("")).ok())
        .is_some_and(|filter| filter.matches(&context));
    if allowed {
        PolicyDecision::allow(permission, scope)
    } else {
        PolicyDecision::deny(
            permission,
            scope,
            "role_or_scope_denied",
            "Current Store rights deny this action.",
        )
    }
}

#[cfg(test)]
#[path = "../store/rights_matrix_tests.rs"]
mod rights_matrix_tests;

// ---------------------------------------------------------------------------
// Session mapping stays below Store orchestration. Permission decisions use
// the registered StoreRightsAuthority; these helpers never open SQLite or
// manufacture a default grant when the authority is absent.
//
// The overlays that stayed in store.rs are thin wrappers over store-bound
// lookups (scoped_policy_decision and friends); moving those would relocate
// the seam, not remove it.

use super::session::{session_role, session_user_id, BusinessOsSession};
use serde_json::Value;

pub(super) fn policy_actor_from_session(session: &BusinessOsSession) -> BusinessOsActor {
    BusinessOsActor::new(
        session_user_id(session).map(str::to_owned),
        session_role(session),
    )
}

pub(super) fn policy_decision_payload(decision: &PolicyDecision) -> Value {
    serde_json::json!({
        "allowed": decision.allowed,
        "permission": decision.permission,
        "scope_type": decision.scope_type,
        "scope_id": decision.scope_id,
        "reason_code": decision.reason_code,
        "display_reason": decision.display_reason,
        "requires_approval": decision.requires_approval,
        "audit_level": decision.audit_level
    })
}

pub(super) fn owner_transfer_policy_decision(session: &BusinessOsSession) -> PolicyDecision {
    let actor = policy_actor_from_session(session);
    evaluate(
        &actor,
        BusinessOsPermission::WorkspaceManage,
        &BusinessOsScope::workspace(),
    )
}

// V4 mapping only: current Store rows are authoritative; static V3 permission
// defaults never widen a resolved collection projection.
#[path = "../store/rights.rs"]
pub mod sync_rights;

pub fn resolve_sync_scope(
    actor: &BusinessOsActor,
    rights: &sync_rights::RightsSnapshot,
) -> sync_rights::scope::Result<sync_rights::scope::ResolvedScope> {
    let actor_id = actor
        .id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or("actor identity required")?;
    rights.resolve(actor.role.as_str(), actor_id)
}

/// Actual Store document read: the app boundary, row predicate and field mask
/// are enforced together; an unrestricted legacy boolean is never reused.
pub fn read_sync_document(
    actor: &BusinessOsActor,
    rights: &sync_rights::RightsSnapshot,
    app: &str,
    collection: &str,
    doc: &Value,
) -> sync_rights::scope::Result<Value> {
    let id = actor
        .id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or("forbidden")?;
    rights.authorize_app_read(app, actor.role.as_str(), id, collection, doc)
}
pub fn write_sync_document(
    actor: &BusinessOsActor,
    rights: &sync_rights::RightsSnapshot,
    app: &str,
    collection: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    permit: Option<&sync_rights::manifest_rules::CommandPermit>,
) -> sync_rights::scope::Result<()> {
    let id = actor
        .id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or("forbidden")?;
    rights.authorize_app_write(
        app,
        actor.role.as_str(),
        id,
        collection,
        before,
        after,
        permit,
    )
}
pub fn authorize_sync_write(
    actor: &BusinessOsActor,
    rights: &sync_rights::RightsSnapshot,
    collection: &str,
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
) -> sync_rights::scope::Result<()> {
    let actor_id = actor
        .id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or("revoked")?;
    rights.authorize_write(actor.role.as_str(), actor_id, collection, before, after)
}

/// Official Core credential-provider bridge. The existing secret is canonical
/// base64url PKCS#8 under scope "instance"; provisioning/rotation is a separate
/// owner action. This path never initializes, replaces or writes any secret.
pub struct SyncInstanceSigningKeyStore {
    pub root: std::path::PathBuf,
}
impl sync_rights::scope::InstanceSigningKeyStore for SyncInstanceSigningKeyStore {
    fn with_instance_signing_key<T>(
        &self,
        use_key: impl FnOnce(&[u8]) -> sync_rights::scope::Result<T>,
    ) -> sync_rights::scope::Result<T> {
        use base64::Engine as _;
        crate::secrets::with_current_secret_value(
            &self.root,
            "instance",
            sync_rights::scope::INSTANCE_SIGNING_KEY,
            |encoded| {
                let key = zeroize::Zeroizing::new(
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(encoded)
                        .map_err(|_| anyhow::anyhow!("invalid instance signing key encoding"))?,
                );
                use_key(&key).map_err(anyhow::Error::msg)
            },
        )
        .map_err(|_| "instance signing authority unavailable".into())
    }
}

/// P4 AuthenticatePeer extension: this callback is supplied by the trusted
/// pairing/identity owner after checking nonce + signature + current device.
pub struct VerifiedSyncPeer {
    pub actor: BusinessOsActor,
    pub instance_id: String,
    pub device_id: String,
}
pub type VerifySyncPeer = std::sync::Arc<
    dyn Fn(
            String,
            rxdb::plugins::replication_webrtc::index_mod::hello::Hello,
        )
            -> futures_util::future::BoxFuture<'static, sync_rights::scope::Result<VerifiedSyncPeer>>
        + Send
        + Sync,
>;

pub fn sync_peer_authenticator<
    S: sync_rights::scope::InstanceSigningKeyStore + Send + Sync + 'static,
>(
    authority: std::sync::Arc<sync_rights::StoreRightsAuthority>,
    keys: std::sync::Arc<S>,
    verify: VerifySyncPeer,
    clock: sync_rights::scope::scope_filter::CoreClock,
) -> rxdb::plugins::replication_webrtc::index_mod::connection_lifecycle::AuthenticatePeer {
    use rxdb::plugins::replication_webrtc::index_mod::{
        connection_lifecycle::AuthorizedSession,
        hello::{HelloReject, RejectReason, WireSeq},
    };
    std::sync::Arc::new(move |peer, hello| {
        let (authority, keys, verify, clock) = (
            authority.clone(),
            keys.clone(),
            verify.clone(),
            clock.clone(),
        );
        Box::pin(async move {
            let failure = || {
                HelloReject::new(
                    RejectReason::AuthenticationFailed,
                    "current device and scope authority",
                )
            };
            let (device, instance, wanted) = (
                hello.device_proof.device_id.clone(),
                hello.instance_id.clone(),
                hello.scopes_wanted.clone(),
            );
            if wanted.len() > 128 {
                return Err(failure());
            }
            let verified = verify(peer, hello).await.map_err(|_| failure())?;
            if verified.device_id != device || verified.instance_id != instance {
                return Err(failure());
            }
            let actor = verified
                .actor
                .id
                .as_deref()
                .filter(|id| !id.is_empty())
                .ok_or_else(failure)?;
            let role = verified.actor.role.as_str();
            let (head, rights) = authority.hello_snapshot().await.map_err(|_| failure())?;
            let now = clock();
            let mut scopes = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for collection in wanted {
                if !seen.insert(collection.clone()) {
                    return Err(failure());
                }
                let scope = rights
                    .resolve_collection(role, actor, &collection)
                    .map_err(|_| failure())?;
                if scope.projections.is_empty() {
                    return Err(failure());
                }
                scopes.push(
                    sync_rights::scope::SignedScope::issue(&*keys, &instance, &device, &scope, now)
                        .and_then(|s| s.to_wire())
                        .map_err(|_| failure())?,
                );
            }
            // Signing occurs outside the router/issuer lock order. A concurrent
            // revocation invalidates this result before it can complete hello.
            authority
                .router
                .with_snapshot(rights.rights_ver, &head.epoch, |current, _| {
                    if current != instance {
                        return Err("instance mismatch".into());
                    }
                    Ok(AuthorizedSession {
                        instance_id: instance,
                        device_id: device,
                        epoch: head.epoch.clone(),
                        head_seq: WireSeq(head.head_seq),
                        scope_tokens: scopes,
                        rights_ver: WireSeq(rights.rights_ver),
                    })
                })
                .map_err(|_| failure())
        })
    })
}
/// Before-handler mapping; callers pass an assignment result from Core policy,
/// never a flag copied from the command payload.
pub fn dispatch_sync_command<T>(
    router: &sync_rights::RightsRouter,
    actor: &BusinessOsActor,
    app_id: &str,
    command: &str,
    assigned_by_core: bool,
    start: impl FnOnce(sync_rights::manifest_rules::CommandPermit) -> sync_rights::scope::Result<T>,
) -> sync_rights::scope::Result<T> {
    let id = actor
        .id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or("forbidden")?;
    router.with_command_dispatch(
        app_id,
        id,
        actor.role.as_str(),
        command,
        assigned_by_core,
        start,
    )
}

/// Concrete, owned official provider for the long-lived P4 callback. The
/// factory itself performs no credential access; signing stays in the provider.
pub fn sync_peer_authenticator_from_secret_store(
    authority: std::sync::Arc<sync_rights::StoreRightsAuthority>,
    root: std::path::PathBuf,
    verify: VerifySyncPeer,
    clock: sync_rights::scope::scope_filter::CoreClock,
) -> rxdb::plugins::replication_webrtc::index_mod::connection_lifecycle::AuthenticatePeer {
    sync_peer_authenticator(
        authority,
        std::sync::Arc::new(SyncInstanceSigningKeyStore { root }),
        verify,
        clock,
    )
}

/// Distinguish a committed rule denial from an unavailable Store or uncertain
/// authority. Only a rule denial may become a terminal outbox rejection.
#[derive(Debug, PartialEq, Eq)]
pub enum NativeWriteAdmission<T> {
    Accepted(T),
    /// A committed policy denial. P8 persists rejected{revoked} with the
    /// original op_id in its existing writer transaction; it is not an SQL error.
    Rejected,
}

/// Adapter for P8's single cursor reader and owning write transaction. Identity
/// comes from Core authentication. ReadAuthority delegates to authorize_snapshot
/// and project_change; WriteAuthority uses admit_write on the final document.
#[derive(Clone)]
pub struct NativeStorePolicy {
    authority: std::sync::Arc<sync_rights::StoreRightsAuthority>,
    actor: BusinessOsActor,
    device_id: String,
    app_id: String,
}
impl NativeStorePolicy {
    pub fn from_verified_peer(
        authority: std::sync::Arc<sync_rights::StoreRightsAuthority>,
        peer: VerifiedSyncPeer,
        app_id: String,
    ) -> sync_rights::scope::Result<Self> {
        if peer.actor.id.as_deref().is_none_or(str::is_empty)
            || peer.device_id.is_empty()
            || app_id.is_empty()
        {
            return Err("authenticated identity required".into());
        }
        authority.router.with_identity(|instance, _| {
            if instance != peer.instance_id {
                return Err("instance mismatch".into());
            }
            Ok(())
        })?;
        Ok(Self {
            authority,
            actor: peer.actor,
            device_id: peer.device_id,
            app_id,
        })
    }
    fn snapshot(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
    ) -> rusqlite::Result<sync_rights::RightsSnapshot> {
        fn invalid(s: impl Into<String>) -> rusqlite::Error {
            rusqlite::Error::InvalidParameterName(s.into())
        }
        // A valid role from another peer or another Store cannot be substituted.
        if participant != self.device_id {
            return Err(invalid("participant mismatch"));
        }
        let file: String = tx.query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )?;
        let actual =
            std::fs::canonicalize(file).map_err(|_| invalid("store identity unavailable"))?;
        if actual != self.authority.writer().db_id().path() {
            return Err(invalid("foreign store transaction"));
        }
        // Rights and document come from this same WAL snapshot. A current router
        // rejects an older reader snapshot and all suspended/stopped authorities.
        let rights = sync_rights::read(tx)?;
        self.authority
            .router
            .with_current_rights(|current| {
                if current.rights_ver != rights.rights_ver {
                    return Err("revoked".into());
                }
                Ok(())
            })
            .map_err(invalid)?;
        Ok(rights)
    }
    pub fn authorize_snapshot(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        collection: &str,
        generation: u64,
        scope_hash: &str,
    ) -> rusqlite::Result<sync_rights::scope::ResolvedScope> {
        let invalid = |s: String| rusqlite::Error::InvalidParameterName(s);
        let rights = self.snapshot(tx, participant)?;
        let active = ctox_store::schema::active_collection(tx, collection)?
            .ok_or_else(|| invalid("collection unavailable".into()))?;
        if active.gen != generation {
            return Err(invalid("generation changed".into()));
        }
        let scope = rights
            .resolve_app_collection(
                &self.app_id,
                self.actor.role.as_str(),
                self.actor
                    .id
                    .as_deref()
                    .ok_or_else(|| invalid("actor missing".into()))?,
                collection,
            )
            .map_err(invalid)?;
        if scope.projection(collection).is_none() || scope.hash().map_err(invalid)? != scope_hash {
            return Err(invalid("scope changed: resnapshot required".into()));
        }
        Ok(scope)
    }
    /// Same transaction and authority as authorize_snapshot. None is P8's
    /// op=unproject, including leaving a row filter; masks never leave Core.
    pub fn project_change(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        collection: &str,
        scope_hash: &str,
        change: &ctox_store::change_log::ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        let scope = self.authorize_snapshot(tx, participant, collection, change.gen, scope_hash)?;
        let projection = scope.projection(collection);
        match sync_rights::scope::scope_filter::project(
            projection,
            collection,
            &change.id,
            change.seq,
            change.body.as_ref(),
        )
        .map_err(rusqlite::Error::InvalidParameterName)?
        {
            sync_rights::scope::scope_filter::ProjectedChange::Put { doc, .. } => Ok(Some(doc)),
            sync_rights::scope::scope_filter::ProjectedChange::Unproject { .. } => Ok(None),
        }
    }
    /// Final publication fence for the snapshot above: no prepared page may
    /// bypass a concurrent rights commit while waiting for the transport.
    pub fn with_page_frame<T>(
        &self,
        rights_ver: u64,
        epoch: &str,
        send_one_frame: impl FnOnce() -> sync_rights::scope::Result<T>,
    ) -> sync_rights::scope::Result<T> {
        self.authority
            .router
            .with_snapshot(rights_ver, epoch, |_, _| send_one_frame())
    }
    /// Final current-schema document supplied by P8, never a client-side base.
    /// Before is fetched here from the active generation in the same writer tx.
    /// The trusted mutation callback is synchronous and must not enqueue/await.
    pub fn admit_write<T>(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        collection: &str,
        id: &str,
        after: Option<&Value>,
        permit: Option<&sync_rights::manifest_rules::CommandPermit>,
        commit: impl FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<NativeWriteAdmission<T>> {
        use rusqlite::OptionalExtension;
        let invalid = |s: String| rusqlite::Error::InvalidParameterName(s);
        let rights = self.snapshot(tx, participant)?;
        let active = ctox_store::schema::active_collection(tx, collection)?
            .ok_or_else(|| invalid("collection unavailable".into()))?;
        // Table identifiers are generated by the Store registry, not by input.
        if !active
            .table
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(invalid("invalid physical binding".into()));
        }
        let raw: Option<String> = tx
            .query_row(
                &format!(
                    "SELECT data FROM \"{}\" WHERE id=?1 AND deleted=0",
                    active.table
                ),
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let before: Option<Value> = raw
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|_| invalid("invalid stored document".into()))?;
        self.authority
            .router
            .with_current_rights(|current| {
                if current.rights_ver != rights.rights_ver {
                    return Err("revoked".into());
                }
                if write_sync_document(
                    &self.actor,
                    &rights,
                    &self.app_id,
                    collection,
                    before.as_ref(),
                    after,
                    permit,
                )
                .is_err()
                {
                    return Ok(Ok(NativeWriteAdmission::Rejected));
                }
                // Preserve the original SQLite failure; it rolls back the owning job.
                Ok(commit(tx).map(NativeWriteAdmission::Accepted))
            })
            .map_err(invalid)?
    }
    /// Direct writers roll back a denied mutation. The outbox bridge instead
    /// consumes admit_write's typed rejection to persist its rejection receipt.
    pub fn with_write<T>(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        collection: &str,
        id: &str,
        after: Option<&Value>,
        permit: Option<&sync_rights::manifest_rules::CommandPermit>,
        commit: impl FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        match self.admit_write(tx, participant, collection, id, after, permit, commit)? {
            NativeWriteAdmission::Accepted(value) => Ok(value),
            NativeWriteAdmission::Rejected => {
                Err(rusqlite::Error::InvalidParameterName("revoked".into()))
            }
        }
    }
    /// Core issues/renews peer authority from committed _rights and the same
    /// authenticated app/actor/device binding. No permission/availability fallback.
    pub async fn issue_peer_lease(
        &self,
        keys: &impl sync_rights::scope::InstanceSigningKeyStore,
        collection: &str,
        now: u64,
    ) -> sync_rights::scope::Result<sync_rights::scope::PeerLease> {
        let (head, rights) = self.authority.hello_snapshot().await?;
        let scope = rights.resolve_app_collection(
            &self.app_id,
            self.actor.role.as_str(),
            self.actor.id.as_deref().ok_or("actor missing")?,
            collection,
        )?;
        if scope.projection(collection).is_none() {
            return Err("revoked".into());
        }
        let instance = self
            .authority
            .router
            .with_identity(|id, _| Ok(id.to_string()))?;
        let lease =
            sync_rights::scope::PeerLease::issue(keys, &instance, &self.device_id, &scope, now)?;
        self.authority
            .router
            .with_snapshot(rights.rights_ver, &head.epoch, |id, _| {
                if id != instance {
                    return Err("instance changed".into());
                }
                Ok(lease)
            })
    }
}
