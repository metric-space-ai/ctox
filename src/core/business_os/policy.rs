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
}

pub const BUSINESS_OS_PERMISSIONS: [BusinessOsPermission; 30] = [
    BusinessOsPermission::WorkspaceManage,
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
];

impl BusinessOsPermission {
    pub fn all() -> &'static [BusinessOsPermission] {
        &BUSINESS_OS_PERMISSIONS
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceManage => "workspace.manage",
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
            requires_approval: false,
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
            requires_approval: false,
            audit_level: "decision",
        }
    }
}

pub fn allow_decision(permission: BusinessOsPermission, scope: &BusinessOsScope) -> PolicyDecision {
    PolicyDecision::allow(permission, scope)
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
    matches!(
        parse_role(role),
        BusinessOsRole::Chef | BusinessOsRole::Admin
    )
}

pub fn evaluate(
    actor: &BusinessOsActor,
    permission: BusinessOsPermission,
    scope: &BusinessOsScope,
) -> PolicyDecision {
    let allowed = match permission {
        BusinessOsPermission::WorkspaceManage => actor.role == BusinessOsRole::Chef,
        BusinessOsPermission::UsersManage
        | BusinessOsPermission::RolesManage
        | BusinessOsPermission::RuntimeManage
        | BusinessOsPermission::IntegrationsManage
        | BusinessOsPermission::SecretsManage
        | BusinessOsPermission::McpManage
        | BusinessOsPermission::AppsInstall
        | BusinessOsPermission::AppsUninstall
        | BusinessOsPermission::AppsAssignOwner => {
            matches!(actor.role, BusinessOsRole::Chef | BusinessOsRole::Admin)
        }
        BusinessOsPermission::AppsView
        | BusinessOsPermission::AppsModify
        | BusinessOsPermission::AppsRelease
        | BusinessOsPermission::AppsRollback
        | BusinessOsPermission::AppsSourceView => match actor.role {
            BusinessOsRole::Chef | BusinessOsRole::Admin => true,
            BusinessOsRole::Founder => {
                scope.scope_type == BusinessOsScopeType::Module && scope.assigned_to_actor
            }
            BusinessOsRole::User => false,
        },
        BusinessOsPermission::DataRead | BusinessOsPermission::DataWrite => {
            matches!(actor.role, BusinessOsRole::Chef | BusinessOsRole::Admin)
                || scope.assigned_to_actor
        }
        BusinessOsPermission::CtoxTaskCreate => true,
        BusinessOsPermission::CtoxTaskManage => {
            matches!(actor.role, BusinessOsRole::Chef | BusinessOsRole::Admin)
                || scope.owned_by_actor
                || scope.assigned_to_actor
        }
        BusinessOsPermission::ExternalApprove => {
            matches!(actor.role, BusinessOsRole::Chef | BusinessOsRole::Admin)
        }
        BusinessOsPermission::SupportManageInboxes
        | BusinessOsPermission::SupportManageMacros
        | BusinessOsPermission::SupportManageSla => {
            matches!(actor.role, BusinessOsRole::Chef | BusinessOsRole::Admin)
        }
        BusinessOsPermission::SupportRead
        | BusinessOsPermission::SupportTriage
        | BusinessOsPermission::SupportAssign
        | BusinessOsPermission::SupportReply
        | BusinessOsPermission::SupportResolve
        | BusinessOsPermission::SupportAgentRequest
        | BusinessOsPermission::SupportAgentApply => {
            matches!(actor.role, BusinessOsRole::Chef | BusinessOsRole::Admin)
                || scope.assigned_to_actor
        }
    };

    if allowed {
        PolicyDecision::allow(permission, scope)
    } else {
        PolicyDecision::deny(
            permission,
            scope,
            "role_or_scope_denied",
            "This role is not allowed to perform this action for the selected scope.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_os_policy_normalizes_current_role_aliases() {
        assert_eq!(normalize_role("chef"), "chef");
        assert_eq!(normalize_role("owner"), "chef");
        assert_eq!(normalize_role("business_os_admin"), "admin");
        assert_eq!(normalize_role("founder"), "founder");
        assert_eq!(normalize_role("user"), "user");
        assert_eq!(normalize_role("business_os_user"), "user");
        assert_eq!(normalize_role("team"), "user");
        assert_eq!(normalize_role("business_os_team"), "user");
        assert_eq!(normalize_role("unknown"), "user");
    }

    #[test]
    fn business_os_policy_default_role_matrix_covers_major_permissions() {
        let owner = BusinessOsActor::new(Some("owner".to_owned()), "chef");
        let admin = BusinessOsActor::new(Some("admin".to_owned()), "admin");
        let founder = BusinessOsActor::new(Some("founder".to_owned()), "founder");
        let team = BusinessOsActor::new(Some("team".to_owned()), "team");
        let workspace = BusinessOsScope::workspace();
        let assigned_module = BusinessOsScope::module("crm", true);
        let unassigned_module = BusinessOsScope::module("crm", false);

        let cases = [
            (
                &owner,
                BusinessOsPermission::WorkspaceManage,
                &workspace,
                true,
            ),
            (
                &admin,
                BusinessOsPermission::WorkspaceManage,
                &workspace,
                false,
            ),
            (&owner, BusinessOsPermission::UsersManage, &workspace, true),
            (&admin, BusinessOsPermission::UsersManage, &workspace, true),
            (
                &founder,
                BusinessOsPermission::UsersManage,
                &workspace,
                false,
            ),
            (&team, BusinessOsPermission::UsersManage, &workspace, false),
            (&owner, BusinessOsPermission::AppsInstall, &workspace, true),
            (&admin, BusinessOsPermission::AppsInstall, &workspace, true),
            (
                &founder,
                BusinessOsPermission::AppsInstall,
                &workspace,
                false,
            ),
            (&team, BusinessOsPermission::AppsInstall, &workspace, false),
            (
                &founder,
                BusinessOsPermission::AppsView,
                &assigned_module,
                true,
            ),
            (
                &founder,
                BusinessOsPermission::AppsView,
                &unassigned_module,
                false,
            ),
            (
                &team,
                BusinessOsPermission::AppsView,
                &assigned_module,
                false,
            ),
            (
                &founder,
                BusinessOsPermission::AppsModify,
                &assigned_module,
                true,
            ),
            (
                &founder,
                BusinessOsPermission::AppsModify,
                &unassigned_module,
                false,
            ),
            (
                &team,
                BusinessOsPermission::AppsModify,
                &assigned_module,
                false,
            ),
            (
                &team,
                BusinessOsPermission::CtoxTaskCreate,
                &workspace,
                true,
            ),
            (
                &founder,
                BusinessOsPermission::ExternalApprove,
                &workspace,
                false,
            ),
            (
                &founder,
                BusinessOsPermission::SupportReply,
                &assigned_module,
                true,
            ),
            (
                &founder,
                BusinessOsPermission::SupportManageSla,
                &assigned_module,
                false,
            ),
            (
                &admin,
                BusinessOsPermission::SupportManageInboxes,
                &workspace,
                true,
            ),
            (
                &team,
                BusinessOsPermission::SupportAgentRequest,
                &assigned_module,
                true,
            ),
            (
                &team,
                BusinessOsPermission::ExternalApprove,
                &assigned_module,
                false,
            ),
            (
                &owner,
                BusinessOsPermission::SecretsManage,
                &workspace,
                true,
            ),
            (
                &admin,
                BusinessOsPermission::SecretsManage,
                &workspace,
                true,
            ),
            (
                &founder,
                BusinessOsPermission::SecretsManage,
                &workspace,
                false,
            ),
            (
                &team,
                BusinessOsPermission::SecretsManage,
                &workspace,
                false,
            ),
        ];

        for (actor, permission, scope, expected) in cases {
            assert_eq!(
                evaluate(actor, permission, scope).allowed,
                expected,
                "role={} permission={} scope={} assigned={} owned={}",
                actor.role.as_str(),
                permission.as_str(),
                scope.scope_type.as_str(),
                scope.assigned_to_actor,
                scope.owned_by_actor
            );
        }
    }

    #[test]
    fn business_os_policy_denials_include_stable_decision_shape() {
        let actor = BusinessOsActor::new(Some("team".to_owned()), "user");
        let scope = BusinessOsScope::module("crm", false);
        let decision = evaluate(&actor, BusinessOsPermission::AppsModify, &scope);

        assert!(!decision.allowed);
        assert_eq!(decision.permission, "apps.modify");
        assert_eq!(decision.scope_type, "module");
        assert_eq!(decision.scope_id.as_deref(), Some("crm"));
        assert_eq!(decision.reason_code, "role_or_scope_denied");
        assert!(!decision.display_reason.is_empty());
        assert!(!decision.requires_approval);
        assert_eq!(decision.audit_level, "decision");
    }
}
