import test from 'node:test';
import assert from 'node:assert/strict';

import { BusinessOsPermissions } from './permissions.js';
import {
  appDataAccessSummary,
  appLifecycleBadge,
  appLifecycleState,
  appReleaseProjection,
  businessDataAreaLabel,
  businessAppVersion,
  canSeeModuleForAppVersion,
  hasPublicAppVersion,
  parseBusinessAppSemver,
} from './app-lifecycle.js';

const session = (id, role = 'user') => ({ user: { id, role } });

const governance = {
  permission_model: {
    version: 1,
    role_defaults: {
      chef: {
        workspace: Object.values(BusinessOsPermissions),
        module: Object.values(BusinessOsPermissions),
        assigned_module: Object.values(BusinessOsPermissions),
      },
      admin: {
        workspace: Object.values(BusinessOsPermissions),
        module: Object.values(BusinessOsPermissions),
      },
      founder: {
        workspace: [],
        module: [],
        assigned_module: [
          BusinessOsPermissions.AppsView,
          BusinessOsPermissions.AppsModify,
          BusinessOsPermissions.AppsSourceView,
          BusinessOsPermissions.DataRead,
          BusinessOsPermissions.DataWrite,
        ],
      },
      user: {
        workspace: [],
        module: [],
      },
    },
    module_assignments: {
      draft_app: {
        builder: [
          BusinessOsPermissions.AppsView,
          BusinessOsPermissions.AppsModify,
          BusinessOsPermissions.AppsSourceView,
        ],
      },
    },
    explicit_grants: [
      {
        grant_id: 'qa_draft_view',
        subject_type: 'user',
        subject_id: 'qa',
        permission: BusinessOsPermissions.AppsView,
        scope_type: 'module',
        scope_id: 'draft_app',
        active: true,
      },
      {
        grant_id: 'modifier_draft_modify',
        subject_type: 'user',
        subject_id: 'modifier',
        permission: BusinessOsPermissions.AppsModify,
        scope_type: 'module',
        scope_id: 'draft_app',
        active: true,
      },
    ],
  },
};

const draftApp = {
  id: 'draft_app',
  title: 'Draft App',
  version: '0.1.0',
  install_scope: 'installed',
  entry: 'installed-modules/draft_app/index.html',
};

test('semver parser accepts plain Business OS app versions only', () => {
  assert.deepEqual(parseBusinessAppSemver('0.1.2'), { major: 0, minor: 1, patch: 2 });
  assert.deepEqual(parseBusinessAppSemver('1.0.0'), { major: 1, minor: 0, patch: 0 });
  assert.equal(parseBusinessAppSemver('v1.0.0'), null);
  assert.equal(parseBusinessAppSemver('1'), null);
});

test('runtime installed 0.x app is private unless actor can view that app', () => {
  assert.equal(
    canSeeModuleForAppVersion(draftApp, {
      session: session('team_member', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canSeeModuleForAppVersion(draftApp, {
      session: session('builder', 'founder'),
      governance,
    }),
    true
  );
  assert.equal(
    canSeeModuleForAppVersion(draftApp, {
      session: session('qa', 'user'),
      governance,
    }),
    true
  );
  assert.equal(
    canSeeModuleForAppVersion(draftApp, {
      session: session('modifier', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canSeeModuleForAppVersion(draftApp, {
      session: session('global_admin', 'admin'),
      governance,
    }),
    true
  );
});

test('runtime installed 1.0.0 app is team-visible by default', () => {
  const released = { ...draftApp, version: '1.0.0' };
  assert.equal(hasPublicAppVersion(released), true);
  assert.equal(
    canSeeModuleForAppVersion(released, {
      session: session('team_member', 'user'),
      governance,
    }),
    true
  );
  assert.equal(appLifecycleState(released).state, 'team');
});

test('projected preview audience display still requires exact app view access', () => {
  const preview = {
    ...draftApp,
    lifecycle: {
      runtime_installed: true,
      visibility_state: 'preview',
      audience: 'preview',
      preview_user_ids: ['qa'],
    },
  };
  assert.equal(appLifecycleState(preview).state, 'preview');
  assert.equal(
    canSeeModuleForAppVersion(preview, {
      session: session('team_member', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canSeeModuleForAppVersion(preview, {
      session: session('qa', 'user'),
      governance,
    }),
    true
  );
  assert.equal(
    canSeeModuleForAppVersion(preview, {
      session: session('modifier', 'user'),
      governance,
    }),
    false
  );
});

test('projected native lifecycle semver overrides stale manifest version', () => {
  const projected = {
    ...draftApp,
    version: '0.9.0',
    lifecycle: {
      runtime_installed: true,
      visibility_state: 'team',
      current_semver: '1.0.0',
    },
  };
  assert.equal(businessAppVersion(projected), '1.0.0');
  assert.equal(hasPublicAppVersion(projected), true);
  assert.equal(appLifecycleState(projected).state, 'team');
  assert.equal(appLifecycleBadge(projected).version, 'v1.0.0');
});

test('restricted released apps are not team-visible without app view permission', () => {
  const restricted = {
    ...draftApp,
    version: '1.0.0',
    lifecycle: { visibility_state: 'restricted' },
  };
  assert.equal(appLifecycleState(restricted).state, 'restricted');
  assert.equal(
    canSeeModuleForAppVersion(restricted, {
      session: session('team_member', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canSeeModuleForAppVersion(restricted, {
      session: session('builder', 'founder'),
      governance,
    }),
    true
  );
  assert.equal(
    canSeeModuleForAppVersion(restricted, {
      session: session('qa', 'user'),
      governance,
    }),
    true
  );
  assert.equal(
    canSeeModuleForAppVersion(restricted, {
      session: session('modifier', 'user'),
      governance,
    }),
    false
  );
  assert.equal(
    canSeeModuleForAppVersion(restricted, {
      session: session('global_admin', 'admin'),
      governance,
    }),
    true
  );
});

test('malformed runtime installed version remains private and warns maintainers', () => {
  const invalid = { ...draftApp, version: 'v1.0.0' };
  const state = appLifecycleState(invalid, {
    session: session('team_member', 'user'),
    governance,
  });
  assert.equal(state.state, 'private');
  assert.equal(state.warning, true);
  assert.equal(canSeeModuleForAppVersion(invalid, {
    session: session('team_member', 'user'),
    governance,
  }), false);
});

test('projected invalid native semver remains private even if manifest version is stale public', () => {
  const invalidProjected = {
    ...draftApp,
    version: '1.0.0',
    lifecycle: {
      runtime_installed: true,
      visibility_state: 'private',
      current_semver: null,
      warning_code: 'invalid_semver',
    },
  };
  const state = appLifecycleState(invalidProjected, {
    session: session('team_member', 'user'),
    governance,
  });
  assert.equal(businessAppVersion(invalidProjected), '');
  assert.equal(state.state, 'private');
  assert.equal(state.warning, true);
  assert.equal(state.warningCode, 'invalid_semver');
  assert.equal(canSeeModuleForAppVersion(invalidProjected, {
    session: session('team_member', 'user'),
    governance,
  }), false);
});

test('packaged apps remain visible outside runtime lifecycle gating', () => {
  const packaged = { id: 'ctox', version: '0.1.0', entry: 'modules/ctox/index.html' };
  assert.equal(canSeeModuleForAppVersion(packaged, {
    session: session('team_member', 'user'),
    governance,
  }), true);
  assert.equal(appLifecycleState(packaged).state, 'packaged');
});

test('lifecycle badge exposes version and business-facing state labels', () => {
  assert.deepEqual(
    {
      text: appLifecycleBadge(draftApp).text,
      version: appLifecycleBadge(draftApp).version,
      state: appLifecycleBadge(draftApp).state,
    },
    { text: 'Privat', version: 'v0.1.0', state: 'private' }
  );
  assert.deepEqual(
    {
      text: appLifecycleBadge({ ...draftApp, version: '1.0.0' }).text,
      version: appLifecycleBadge({ ...draftApp, version: '1.0.0' }).version,
      state: appLifecycleBadge({ ...draftApp, version: '1.0.0' }).state,
    },
    { text: 'Team', version: 'v1.0.0', state: 'team' }
  );
});

test('lifecycle state separates app visibility from app management permission', () => {
  const viewerState = appLifecycleState(draftApp, {
    session: session('qa', 'user'),
    governance,
  });
  assert.equal(viewerState.canAccessNonPublic, true);
  assert.equal(viewerState.canManage, false);

  const builderState = appLifecycleState(draftApp, {
    session: session('builder', 'founder'),
    governance,
  });
  assert.equal(builderState.canAccessNonPublic, true);
  assert.equal(builderState.canManage, true);

  const modifierState = appLifecycleState(draftApp, {
    session: session('modifier', 'user'),
    governance,
  });
  assert.equal(modifierState.canAccessNonPublic, false);
  assert.equal(modifierState.canManage, true);

  const adminState = appLifecycleState(draftApp, {
    session: session('global_admin', 'admin'),
    governance,
  });
  assert.equal(adminState.canAccessNonPublic, true);
  assert.equal(adminState.canManage, true);
});

test('release projection explains current release, rollback and data areas', () => {
  const projected = {
    ...draftApp,
    lifecycle: {
      runtime_installed: true,
      current_semver: '1.2.0',
      release_status: 'released',
      release_state: {
        status: 'released',
        current: {
          version_id: 'version-current',
          version: 4,
          target_version: '1.2.0',
        },
        history_count: 4,
      },
      rollback_target: {
        version_id: 'version-previous',
        version: 3,
        target_version: '1.1.0',
      },
      data_access: {
        status: 'reviewed',
        completed: true,
        areas: [
          { collection: 'inventory_items', read: 'granted', write: 'locked' },
          { collection: 'supplier_prices', read: 'locked', write: 'not_requested' },
        ],
        granted_collection_ids: ['inventory_items'],
        locked_collection_ids: ['inventory_items', 'supplier_prices'],
        review_is_evidence_only: true,
        grants_implied: false,
      },
    },
  };

  const projection = appReleaseProjection(projected);
  assert.equal(projection.hasReleaseState, true);
  assert.equal(projection.currentVersion, 'v1.2.0');
  assert.equal(projection.rollbackVersion, 'v1.1.0');
  assert.match(projection.releaseLine, /Aktuell v1\.2\.0/);
  assert.match(projection.rollbackLine, /Rollback-Ziel v1\.1\.0/);
  assert.match(projection.dataAccess.summary, /Inventory Items \(inventory_items\)/);
  assert.match(projection.dataAccess.summary, /Supplier Prices \(supplier_prices\)/);
  assert.match(projection.dataAccess.reviewNote, /Datenrechte bleiben explizit/);
});

test('data access summary falls back to declared business data areas', () => {
  const summary = appDataAccessSummary({
    collections: ['inventory_items', 'supplier_prices'],
  });
  assert.equal(summary.hasReview, false);
  assert.match(summary.summary, /Inventory Items \(inventory_items\)/);
  assert.match(summary.summary, /Supplier Prices \(supplier_prices\)/);
  assert.equal(businessDataAreaLabel('business_commands'), 'Business Commands (business_commands)');
});
