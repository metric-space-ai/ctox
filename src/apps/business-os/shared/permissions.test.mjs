import test from 'node:test';
import assert from 'node:assert/strict';

import {
  BusinessOsPermissions,
  businessActorFromSession,
  canModifyBusinessModule,
  canUninstallBusinessApp,
  canUseBusinessPermission,
  canViewBusinessModuleSource,
} from './permissions.js';

const userSession = (id, role = 'user') => ({
  user: { id, role },
});

function governanceWithPermissionModel(overrides = {}) {
  return {
    founders: {},
    permission_model: {
      version: 1,
      deny_supported: false,
      role_defaults: {
        chef: {
          workspace: Object.values(BusinessOsPermissions),
          module: Object.values(BusinessOsPermissions),
          assigned_module: Object.values(BusinessOsPermissions),
        },
        admin: {
          workspace: [
            BusinessOsPermissions.UsersManage,
            BusinessOsPermissions.AppsInstall,
            BusinessOsPermissions.AppsUninstall,
            BusinessOsPermissions.AppsModify,
          ],
          module: [
            BusinessOsPermissions.AppsInstall,
            BusinessOsPermissions.AppsUninstall,
            BusinessOsPermissions.AppsModify,
          ],
        },
        founder: {
          workspace: [BusinessOsPermissions.CtoxTaskCreate],
          module: [],
          assigned_module: [
            BusinessOsPermissions.AppsModify,
            BusinessOsPermissions.AppsRelease,
          ],
        },
        user: {
          workspace: [BusinessOsPermissions.CtoxTaskCreate],
          module: [],
        },
      },
      module_assignments: {},
      explicit_grants: [],
      ...overrides,
    },
  };
}

test('business actor resolves browser session role aliases', () => {
  assert.deepEqual(businessActorFromSession(userSession('owner-1', 'owner')), {
    id: 'owner-1',
    role: 'chef',
  });
  assert.deepEqual(businessActorFromSession({ user: { id: 'u1', role: 'team' } }), {
    id: 'u1',
    role: 'user',
  });
  assert.deepEqual(businessActorFromSession({ user: { id: 'legacy', is_admin: true } }), {
    id: 'legacy',
    role: 'admin',
  });
  assert.deepEqual(businessActorFromSession({}, { user_id: 'ctox-system', role: 'admin' }), {
    id: 'ctox-system',
    role: 'admin',
  });
  assert.deepEqual(businessActorFromSession({}, { can_manage_all: true }), {
    id: '',
    role: 'admin',
  });
  assert.deepEqual(
    businessActorFromSession(userSession('viewer', 'user'), { user_id: 'ctox-system', role: 'admin' }),
    {
      id: 'viewer',
      role: 'user',
    }
  );
});

test('module assignments and founder fallback allow only assigned app modification', () => {
  const governance = governanceWithPermissionModel({
      module_assignments: {
        inventory: {
          founder_a: [
            BusinessOsPermissions.AppsModify,
            BusinessOsPermissions.AppsSourceView,
          ],
        },
      },
  });

  assert.equal(
    canModifyBusinessModule({ id: 'inventory' }, {
      session: userSession('founder_a', 'founder'),
      governance,
    }),
    true
  );
  assert.equal(
    canModifyBusinessModule({ id: 'billing' }, {
      session: userSession('founder_a', 'founder'),
      governance,
    }),
    false
  );

  const legacyGovernance = {
    founders: {
      inventory: [{ user_id: 'founder_b', active: true }],
    },
  };
  assert.equal(
    canModifyBusinessModule({ id: 'inventory' }, {
      session: userSession('founder_b', 'founder'),
      governance: legacyGovernance,
    }),
    true
  );
});

test('app source view is hidden from team by default but can be explicitly granted', () => {
  const governance = governanceWithPermissionModel({
    module_assignments: {
      inventory: {
        founder_a: [BusinessOsPermissions.AppsSourceView],
      },
    },
    explicit_grants: [
      {
        grant_id: 'viewer_inventory_source',
        subject_type: 'user',
        subject_id: 'viewer',
        permission: BusinessOsPermissions.AppsSourceView,
        scope_type: 'module',
        scope_id: 'inventory',
        active: true,
      },
    ],
  });

  assert.equal(
    canViewBusinessModuleSource({ id: 'inventory' }, {
      session: userSession('team_member', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canViewBusinessModuleSource({ id: 'inventory' }, {
      session: userSession('viewer', 'user'),
      governance,
    }),
    true
  );
  assert.equal(
    canViewBusinessModuleSource({ id: 'billing' }, {
      session: userSession('viewer', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canViewBusinessModuleSource({ id: 'inventory' }, {
      session: userSession('founder_a', 'founder'),
      governance,
    }),
    true
  );
});

test('app visibility grants do not imply edit, source or data permissions', () => {
  const governance = governanceWithPermissionModel({
    explicit_grants: [
      {
        grant_id: 'viewer_inventory_visibility',
        subject_type: 'user',
        subject_id: 'viewer',
        permission: BusinessOsPermissions.AppsView,
        scope_type: 'module',
        scope_id: 'inventory',
        active: true,
      },
    ],
  });

  assert.equal(
    canUseBusinessPermission({
      session: userSession('viewer', 'user'),
      governance,
      permission: BusinessOsPermissions.AppsView,
      scopeType: 'module',
      scopeId: 'inventory',
    }),
    true
  );
  assert.equal(
    canModifyBusinessModule({ id: 'inventory' }, {
      session: userSession('viewer', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canViewBusinessModuleSource({ id: 'inventory' }, {
      session: userSession('viewer', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canUseBusinessPermission({
      session: userSession('viewer', 'user'),
      governance,
      permission: BusinessOsPermissions.DataRead,
      scopeType: 'collection',
      scopeId: 'business_commands',
    }),
    false
  );
});

test('explicit projected grants authorize users and roles without new role labels', () => {
  const governance = governanceWithPermissionModel({
    explicit_grants: [
      {
        grant_id: 'viewer_inventory_modify',
        subject_type: 'user',
        subject_id: 'viewer',
        permission: BusinessOsPermissions.AppsModify,
        scope_type: 'module',
        scope_id: 'inventory',
        active: true,
      },
      {
        grant_id: 'team_uninstall_billing',
        subject_type: 'role',
        subject_id: 'team',
        permission: BusinessOsPermissions.AppsUninstall,
        scope_type: 'module',
        scope_id: 'billing',
        active: true,
      },
      {
        grant_id: 'inactive_viewer_payroll_modify',
        subject_type: 'user',
        subject_id: 'viewer',
        permission: BusinessOsPermissions.AppsModify,
        scope_type: 'module',
        scope_id: 'payroll',
        active: false,
      },
    ],
  });

  assert.equal(
    canModifyBusinessModule({ id: 'inventory' }, {
      session: userSession('viewer', 'user'),
      governance,
    }),
    true
  );
  assert.equal(
    canModifyBusinessModule({ id: 'billing' }, {
      session: userSession('viewer', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canUninstallBusinessApp({ id: 'billing' }, {
      session: userSession('any-team-member', 'user'),
      governance,
    }),
    true
  );
  assert.equal(
    canModifyBusinessModule({ id: 'payroll' }, {
      session: userSession('viewer', 'user'),
      governance,
    }),
    false
  );
});

test('workspace grants authorize targeted workspace permissions only', () => {
  const governance = governanceWithPermissionModel({
    explicit_grants: [
      {
        grant_id: 'ops_users_manage',
        subject_type: 'user',
        subject_id: 'ops',
        permission: BusinessOsPermissions.UsersManage,
        scope_type: 'workspace',
        scope_id: '',
        active: true,
      },
    ],
  });

  assert.equal(
    canUseBusinessPermission({
      session: userSession('ops', 'user'),
      governance,
      permission: BusinessOsPermissions.UsersManage,
      scopeType: 'workspace',
    }),
    true
  );
  assert.equal(
    canUseBusinessPermission({
      session: userSession('ops', 'user'),
      governance,
      permission: BusinessOsPermissions.RuntimeManage,
      scopeType: 'workspace',
    }),
    false
  );
});

test('server-projected governance actor authorizes local admin shells with empty sessions', () => {
  const governance = governanceWithPermissionModel({
    role_defaults: {
      admin: {
        workspace: [BusinessOsPermissions.AppsView],
        module: [BusinessOsPermissions.AppsView, BusinessOsPermissions.AppsModify],
      },
      user: {
        workspace: [],
        module: [],
      },
    },
  });

  assert.equal(
    canUseBusinessPermission({
      session: {},
      governance: {
        ...governance,
        user_id: 'ctox-system',
        role: 'admin',
      },
      permission: BusinessOsPermissions.AppsView,
      scopeType: 'module',
      scopeId: 'private_app',
    }),
    true
  );
  assert.equal(
    canUseBusinessPermission({
      session: {},
      governance,
      permission: BusinessOsPermissions.AppsView,
      scopeType: 'module',
      scopeId: 'private_app',
    }),
    false
  );
});

test('support permissions separate workflow rights from management and external approval', () => {
  const assignedGovernance = {
    founders: {
      support: [{ user_id: 'founder_support', active: true }],
    },
  };

  assert.equal(
    canUseBusinessPermission({
      session: userSession('founder_support', 'founder'),
      governance: assignedGovernance,
      permission: BusinessOsPermissions.SupportReply,
      scopeType: 'module',
      scopeId: 'support',
    }),
    true
  );
  assert.equal(
    canUseBusinessPermission({
      session: userSession('founder_support', 'founder'),
      governance: assignedGovernance,
      permission: BusinessOsPermissions.SupportManageSla,
      scopeType: 'module',
      scopeId: 'support',
    }),
    false
  );
  assert.equal(
    canUseBusinessPermission({
      session: userSession('founder_support', 'founder'),
      governance: assignedGovernance,
      permission: BusinessOsPermissions.ExternalApprove,
      scopeType: 'module',
      scopeId: 'support',
    }),
    false
  );
  assert.equal(
    canUseBusinessPermission({
      session: userSession('ops_admin', 'admin'),
      governance: assignedGovernance,
      permission: BusinessOsPermissions.SupportManageInboxes,
      scopeType: 'module',
      scopeId: 'support',
    }),
    true
  );
});
