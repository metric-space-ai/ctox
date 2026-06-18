import { normalizeRole, roleCanManage } from './roles.js';

export const BusinessOsPermissions = Object.freeze({
  WorkspaceManage: 'workspace.manage',
  UsersManage: 'users.manage',
  RolesManage: 'roles.manage',
  RuntimeManage: 'runtime.manage',
  IntegrationsManage: 'integrations.manage',
  SecretsManage: 'secrets.manage',
  McpManage: 'mcp.manage',
  AppsInstall: 'apps.install',
  AppsUninstall: 'apps.uninstall',
  AppsAssignOwner: 'apps.assign_owner',
  AppsView: 'apps.view',
  AppsModify: 'apps.modify',
  AppsRelease: 'apps.release',
  AppsRollback: 'apps.rollback',
  AppsSourceView: 'apps.source.view',
  DataRead: 'data.read',
  DataWrite: 'data.write',
  CtoxTaskCreate: 'ctox.task.create',
  CtoxTaskManage: 'ctox.task.manage',
  ExternalApprove: 'external.approve',
  SupportRead: 'support.read',
  SupportTriage: 'support.triage',
  SupportAssign: 'support.assign',
  SupportReply: 'support.reply',
  SupportResolve: 'support.resolve',
  SupportManageInboxes: 'support.manage_inboxes',
  SupportManageMacros: 'support.manage_macros',
  SupportManageSla: 'support.manage_sla',
  SupportAgentRequest: 'support.agent_request',
  SupportAgentApply: 'support.agent_apply',
});

const OWNER_ADMIN_PERMISSIONS = new Set(Object.values(BusinessOsPermissions));
const ASSIGNED_MODULE_PERMISSIONS = new Set([
  BusinessOsPermissions.AppsView,
  BusinessOsPermissions.AppsModify,
  BusinessOsPermissions.AppsRelease,
  BusinessOsPermissions.AppsRollback,
  BusinessOsPermissions.AppsSourceView,
  BusinessOsPermissions.DataRead,
  BusinessOsPermissions.DataWrite,
  BusinessOsPermissions.SupportRead,
  BusinessOsPermissions.SupportTriage,
  BusinessOsPermissions.SupportAssign,
  BusinessOsPermissions.SupportReply,
  BusinessOsPermissions.SupportResolve,
  BusinessOsPermissions.SupportAgentRequest,
  BusinessOsPermissions.SupportAgentApply,
]);

export function businessActorFromSession(session = null) {
  const user = session?.user || {};
  const role = normalizeRole(user.role || (user.is_admin ? 'admin' : 'user'));
  return {
    id: String(user.id || user.user_id || '').trim(),
    role,
  };
}

export function permissionModelFromGovernance(governance = null) {
  return governance?.permission_model || governance?.governance?.permission_model || null;
}

export function canUseBusinessPermission({
  session = null,
  governance = null,
  permission,
  scopeType = 'workspace',
  scopeId = '',
  assigned = false,
  owned = false,
} = {}) {
  if (!permission) return false;
  const actor = businessActorFromSession(session);
  const normalizedScopeId = String(scopeId || '').trim();
  const model = permissionModelFromGovernance(governance);
  const moduleAssigned = scopeType === 'module'
    && isModuleAssignedToActor(governance, normalizedScopeId, actor.id);
  const effectiveAssigned = Boolean(assigned || moduleAssigned);

  if (explicitGrantAllows(model, actor, permission, scopeType, normalizedScopeId)) {
    return true;
  }
  if (moduleAssignmentAllows(model, actor.id, normalizedScopeId, permission)) {
    return true;
  }
  if (roleDefaultAllows(model, actor.role, permission, scopeType, {
    assigned: effectiveAssigned,
    owned,
  })) {
    return true;
  }
  return fallbackRoleAllows(actor.role, permission, scopeType, {
    assigned: effectiveAssigned,
    owned,
  });
}

export function canModifyBusinessModule(moduleLike, options = {}) {
  const moduleId = String(moduleLike?.id || moduleLike?.module_id || '').trim();
  if (!moduleId) return false;
  return canUseBusinessPermission({
    ...options,
    permission: BusinessOsPermissions.AppsModify,
    scopeType: 'module',
    scopeId: moduleId,
  });
}

export function canUseBusinessExplicitOrAssignedPermission({
  session = null,
  governance = null,
  permission,
  scopeType = 'workspace',
  scopeId = '',
} = {}) {
  if (!permission) return false;
  const actor = businessActorFromSession(session);
  const normalizedScopeId = String(scopeId || '').trim();
  const model = permissionModelFromGovernance(governance);
  if (explicitGrantAllows(model, actor, permission, scopeType, normalizedScopeId)) return true;
  if (scopeType === 'module' && moduleAssignmentAllows(model, actor.id, normalizedScopeId, permission)) return true;
  if (scopeType === 'module' && isModuleAssignedToActor(governance, normalizedScopeId, actor.id)) return true;
  return false;
}

export function canViewBusinessModuleSource(moduleLike, options = {}) {
  const moduleId = String(moduleLike?.id || moduleLike?.module_id || '').trim();
  if (!moduleId) return false;
  return canUseBusinessPermission({
    ...options,
    permission: BusinessOsPermissions.AppsSourceView,
    scopeType: 'module',
    scopeId: moduleId,
  });
}

export function canInstallBusinessApps(options = {}) {
  return canUseBusinessPermission({
    ...options,
    permission: BusinessOsPermissions.AppsInstall,
    scopeType: options.scopeType || 'workspace',
    scopeId: options.scopeId || '',
  });
}

export function canUninstallBusinessApp(moduleLike = null, options = {}) {
  const moduleId = String(moduleLike?.id || moduleLike?.module_id || options.scopeId || '').trim();
  return canUseBusinessPermission({
    ...options,
    permission: BusinessOsPermissions.AppsUninstall,
    scopeType: moduleId ? 'module' : (options.scopeType || 'workspace'),
    scopeId: moduleId,
  });
}

export function canAssignBusinessAppOwner(moduleLike = null, options = {}) {
  const moduleId = String(moduleLike?.id || moduleLike?.module_id || options.scopeId || '').trim();
  return canUseBusinessPermission({
    ...options,
    permission: BusinessOsPermissions.AppsAssignOwner,
    scopeType: moduleId ? 'module' : (options.scopeType || 'workspace'),
    scopeId: moduleId,
  });
}

function roleDefaultAllows(model, role, permission, scopeType, { assigned = false, owned = false } = {}) {
  const defaults = model?.role_defaults?.[normalizeRole(role)];
  if (!defaults || typeof defaults !== 'object') return false;
  const keys = scopeKeysFor(scopeType, { assigned, owned });
  return keys.some((key) => permissionListIncludes(defaults[key], permission));
}

function moduleAssignmentAllows(model, userId, moduleId, permission) {
  if (!userId || !moduleId) return false;
  return permissionListIncludes(model?.module_assignments?.[moduleId]?.[userId], permission);
}

function explicitGrantAllows(model, actor, permission, scopeType, scopeId) {
  const grants = Array.isArray(model?.explicit_grants) ? model.explicit_grants : [];
  const normalizedScopeId = String(scopeId || '');
  return grants.some((grant) => {
    if (!grant || grant.active === false) return false;
    if (String(grant.permission || '') !== permission) return false;
    if (String(grant.scope_type || '') !== scopeType) return false;
    if (String(grant.scope_id || '') !== normalizedScopeId) return false;
    const subjectType = String(grant.subject_type || '').trim();
    const subjectId = String(grant.subject_id || '').trim();
    if (subjectType === 'role') return normalizeRole(subjectId) === actor.role;
    if (subjectType === 'user') return Boolean(actor.id) && subjectId === actor.id;
    return false;
  });
}

function scopeKeysFor(scopeType, { assigned = false, owned = false } = {}) {
  if (scopeType === 'workspace') return ['workspace'];
  if (scopeType === 'module') return assigned ? ['assigned_module', 'module'] : ['module'];
  if (scopeType === 'task') return owned ? ['owned_task', 'task'] : ['task'];
  return [scopeType];
}

function fallbackRoleAllows(role, permission, scopeType, { assigned = false, owned = false } = {}) {
  const normalizedRole = normalizeRole(role);
  if (roleCanManage(normalizedRole) && OWNER_ADMIN_PERMISSIONS.has(permission)) return true;
  if (permission === BusinessOsPermissions.CtoxTaskCreate) return true;
  if (scopeType === 'task' && owned && permission === BusinessOsPermissions.CtoxTaskManage) return true;
  if (scopeType === 'module' && assigned && normalizedRole === 'founder') {
    return ASSIGNED_MODULE_PERMISSIONS.has(permission);
  }
  return false;
}

function isModuleAssignedToActor(governance, moduleId, userId) {
  if (!moduleId || !userId) return false;
  const assignments = governance?.founders?.[moduleId] || [];
  return assignments.some((item) => item?.user_id === userId && item.active !== false);
}

function permissionListIncludes(value, permission) {
  return Array.isArray(value) && value.some((item) => String(item || '') === permission);
}
