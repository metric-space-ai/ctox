import test from 'node:test';
import assert from 'node:assert/strict';

import {
  BusinessOsPermissions,
  canModifyBusinessModule,
  canViewBusinessModuleSource,
} from './permissions.js';
import {
  buildBusinessUserPickerOptions,
  buildGlobalCtoxAgentScopeView,
  buildGlobalCtoxContextModes,
  buildLifecyclePermissionView,
  buildModuleWhyDiagnosticsView,
  buildModuleTargetContextItems,
  renderBusinessUserDatalistOptions,
  renderGlobalCtoxAgentScopeHtml,
  renderGlobalCtoxContextModeHtml,
  renderModuleWhyDiagnosticsHtml,
  shouldRenderModuleSourceAction,
} from './shell-permissions-ui.js';

const inventoryModule = { id: 'inventory', title: 'Inventory' };
const billingModule = { id: 'billing', title: 'Billing' };

const labels = {
  openApp: 'Öffnen',
  pinToTaskbar: 'An Bar anheften',
  unpinFromTaskbar: 'Von Bar lösen',
  openSource: 'Source öffnen',
  modifyApp: 'App ändern',
  workData: 'Mit Daten arbeiten',
  answer: 'Frage beantworten',
  note: 'Notiz an User',
  mention: 'User erwähnen',
  approval: 'Freigabe anfragen',
};

const governance = {
  permission_model: {
    version: 1,
    deny_supported: false,
    role_defaults: {
      chef: {
        workspace: [BusinessOsPermissions.AppsModify],
        module: [BusinessOsPermissions.AppsModify],
        assigned_module: [BusinessOsPermissions.AppsModify],
      },
      admin: {
        workspace: [BusinessOsPermissions.AppsModify],
        module: [BusinessOsPermissions.AppsModify],
        assigned_module: [BusinessOsPermissions.AppsModify],
      },
      founder: {
        workspace: [],
        module: [],
        assigned_module: [
          BusinessOsPermissions.AppsModify,
          BusinessOsPermissions.AppsSourceView,
        ],
      },
      user: {
        workspace: [],
        module: [],
        assigned_module: [],
      },
    },
    module_assignments: {
      inventory: {
        founder_a: [
          BusinessOsPermissions.AppsModify,
          BusinessOsPermissions.AppsSourceView,
        ],
      },
    },
    explicit_grants: [
      {
        grant_id: 'team_inventory_modify',
        subject_type: 'user',
        subject_id: 'granted_team_member',
        permission: BusinessOsPermissions.AppsModify,
        scope_type: 'module',
        scope_id: 'inventory',
        active: true,
      },
      {
        grant_id: 'team_inventory_source',
        subject_type: 'user',
        subject_id: 'source_viewer',
        permission: BusinessOsPermissions.AppsSourceView,
        scope_type: 'module',
        scope_id: 'inventory',
        active: true,
      },
    ],
  },
};

function userSession(id, role) {
  return { user: { id, role } };
}

function shellContextLabelsFor(module, session) {
  const canModify = canModifyBusinessModule(module, { session, governance });
  const canOpenSource = canViewBusinessModuleSource(module, { session, governance });
  return buildModuleTargetContextItems({
    target: {
      id: module.id,
      kind: 'module',
      title: module.title,
      glyph: '□',
      module,
    },
    pinned: false,
    canModify,
    canOpenSource,
    labels,
  })
    .filter((item) => item.label)
    .map((item) => item.label);
}

test('shell module context menu renders App aendern only for actors allowed to modify that app', () => {
  const cases = [
    ['owner', inventoryModule, userSession('owner', 'chef'), true],
    ['admin', inventoryModule, userSession('admin', 'admin'), true],
    ['assigned app owner', inventoryModule, userSession('founder_a', 'founder'), true],
    ['unassigned app owner', billingModule, userSession('founder_a', 'founder'), false],
    ['team member', inventoryModule, userSession('team_member', 'user'), false],
    ['team member with explicit grant', inventoryModule, userSession('granted_team_member', 'user'), true],
  ];

  for (const [name, module, session, expected] of cases) {
    const menuLabels = shellContextLabelsFor(module, session);
    assert.equal(
      menuLabels.includes('App ändern'),
      expected,
      `${name} App ändern visibility`
    );
    assert.equal(menuLabels.includes('Modul bearbeiten'), false, `${name} legacy module wording`);
    assert.equal(menuLabels.includes('App modifizieren'), false, `${name} legacy modify wording`);
  }
});

test('shell source menu is hidden from team by default and visible by source-view grant', () => {
  const cases = [
    ['owner', inventoryModule, userSession('owner', 'chef'), true],
    ['admin', inventoryModule, userSession('admin', 'admin'), true],
    ['assigned app owner', inventoryModule, userSession('founder_a', 'founder'), true],
    ['team member', inventoryModule, userSession('team_member', 'user'), false],
    ['team source viewer', inventoryModule, userSession('source_viewer', 'user'), true],
  ];

  for (const [name, module, session, expected] of cases) {
    const menuLabels = shellContextLabelsFor(module, session);
    assert.equal(
      menuLabels.includes('Source öffnen'),
      expected,
      `${name} Source öffnen visibility`
    );
  }
});

test('module appbar source action follows the same source-view permission', () => {
  const teamCanOpen = canViewBusinessModuleSource(inventoryModule, {
    session: userSession('team_member', 'user'),
    governance,
  });
  const grantedCanOpen = canViewBusinessModuleSource(inventoryModule, {
    session: userSession('source_viewer', 'user'),
    governance,
  });

  assert.equal(
    shouldRenderModuleSourceAction({ module: inventoryModule, canOpenSource: teamCanOpen }),
    false
  );
  assert.equal(
    shouldRenderModuleSourceAction({ module: inventoryModule, canOpenSource: grantedCanOpen }),
    true
  );
  assert.equal(
    shouldRenderModuleSourceAction({ module: { id: 'desktop' }, canOpenSource: true }),
    false
  );
});

test('lifecycle drawer permission view uses business-facing manager and readonly copy', () => {
  const manager = buildLifecyclePermissionView({ canManage: true, canOpenSource: true });
  assert.equal(manager.state, 'manager');
  assert.equal(manager.canManage, true);
  assert.equal(manager.canOpenSource, true);
  assert.equal(manager.label, 'Verwalten erlaubt');
  assert.match(manager.description, /Sichtbarkeit, Verantwortliche und Releases/);
  assert.equal(manager.storeActionLabel, 'Im App Store verwalten');

  const readonly = buildLifecyclePermissionView({ canManage: false });
  assert.equal(readonly.state, 'readonly');
  assert.equal(readonly.canManage, false);
  assert.equal(readonly.canOpenSource, false);
  assert.equal(readonly.label, 'Nur Ansicht');
  assert.match(readonly.description, /App-Verantwortliche, Admins oder Owner/);
  assert.equal(readonly.storeActionLabel, 'Details im App Store ansehen');
});

test('module why diagnostics explains actor app release and data decisions', () => {
  const view = buildModuleWhyDiagnosticsView({
    actor: { id: 'team_member', display_name: 'Team Member', role: 'user' },
    module: { id: 'inventory', title: 'Inventory', version: '1.0.0' },
    lifecycle: {
      state: 'team',
      label: 'Team',
      versionLabel: 'v1.0.0',
      public: true,
      canAccessNonPublic: false,
      canManage: false,
      runtimeInstalled: true,
      reason: 'Diese App ist als Team-Version freigegeben.',
    },
    releaseProjection: {
      hasReleaseState: true,
      status: 'released',
      statusLabel: 'Freigegeben',
      releaseLine: 'Aktuell v1.0.0 · Freigegeben',
      rollbackLine: 'Rollback-Ziel v0.9.0',
    },
    dataAccess: {
      summary: 'Freigegeben: Inventory Items (inventory_items); Gesperrt: Supplier Prices (supplier_prices)',
      status: 'reviewed',
      statusLabel: 'Geprüft',
      declared: ['inventory_items', 'supplier_prices'],
      granted: ['inventory_items'],
      locked: ['supplier_prices'],
      areas: [
        { collection: 'inventory_items', read: 'granted', write: 'locked' },
        { collection: 'supplier_prices', read: 'locked', write: 'not_requested' },
      ],
      reviewNote: 'Review ist Nachweis; Datenrechte bleiben explizit.',
      grantsImplied: false,
    },
    permissionView: buildLifecyclePermissionView({ canManage: false, canOpenSource: false }),
    permissions: {
      canSee: true,
      canOpen: true,
      canModify: false,
      canOpenSource: false,
      canRelease: false,
      canRollback: false,
    },
    dataPermissions: [
      { collection: 'inventory_items', readAllowed: true, writeAllowed: false, readReviewState: 'granted', writeReviewState: 'locked' },
      { collection: 'supplier_prices', readAllowed: false, writeAllowed: false, readReviewState: 'locked', writeReviewState: 'not_requested' },
    ],
  });

  assert.deepEqual(
    view.rows.map((row) => row.key),
    ['actor', 'visibility', 'open', 'modify', 'source', 'release', 'rollback', 'data']
  );
  assert.equal(view.app.module_id, 'inventory');
  assert.equal(view.app.can_open, true);
  assert.equal(view.app.can_modify, false);
  assert.equal(view.app.can_release, false);
  assert.equal(view.release.line, 'Aktuell v1.0.0 · Freigegeben');
  assert.equal(view.data.decisions.length, 2);
  assert.equal(view.data.decisions[0].read.allowed, true);
  assert.equal(view.data.decisions[0].write.allowed, false);
  assert.match(view.data.decisions[0].write.reason, /gesperrt/);
  assert.match(view.rows.find((row) => row.key === 'data').reason, /Datenrechte bleiben explizit/);
});

test('module why diagnostics html uses stable rows and does not leak raw grant json', () => {
  const html = renderModuleWhyDiagnosticsHtml({
    actor: { display_name: 'Admin <script>', role: 'admin' },
    module: { id: 'support', title: 'Support & Care' },
    lifecycle: {
      state: 'restricted',
      label: 'Eingeschränkt',
      versionLabel: 'v1.1.0',
      public: false,
      canAccessNonPublic: true,
      reason: 'Diese Team-App ist auf eine explizite Zielgruppe eingeschränkt.',
    },
    releaseProjection: { releaseLine: 'Aktuell v1.1.0 · Freigegeben' },
    dataAccess: {
      summary: 'Gesperrt: Support Tickets (support_tickets)',
      declared: ['support_tickets'],
      locked: ['support_tickets'],
      areas: [{ collection: 'support_tickets', read: 'locked', write: 'locked' }],
    },
    permissions: {
      canSee: true,
      canOpen: true,
      canModify: true,
      canOpenSource: true,
      canRelease: true,
      canRollback: false,
    },
    dataPermissions: [
      { collection: 'support_tickets', readAllowed: false, writeAllowed: false },
    ],
  });

  assert.match(html, /data-why-diagnostics="support"/);
  assert.match(html, /data-why-row="visibility"/);
  assert.match(html, /data-why-row="release"/);
  assert.match(html, /data-why-data-row="support_tickets"/);
  assert.match(html, /Admin &lt;script&gt;/);
  assert.match(html, /App ändern/);
  assert.doesNotMatch(html, /<script>/);
  assert.doesNotMatch(html, /explicit_grants|module_assignments|selected_text|clicked_text|prompt|secret|token/i);
});

test('global CTOX context modes render app mode only when app modification is allowed', () => {
  const deniedModes = buildGlobalCtoxContextModes({ canModify: false, labels });
  assert.deepEqual(deniedModes.map((mode) => mode.value), ['data', 'ask', 'note', 'mention', 'approval']);

  const allowedModes = buildGlobalCtoxContextModes({ canModify: true, labels });
  assert.deepEqual(allowedModes.map((mode) => mode.value), ['data', 'ask', 'app', 'note', 'mention', 'approval']);
  assert.equal(allowedModes.find((mode) => mode.value === 'app')?.label, 'App ändern');
  assert.equal(deniedModes.find((mode) => mode.value === 'approval')?.label, 'Freigabe anfragen');

  const deniedHtml = renderGlobalCtoxContextModeHtml({ canModify: false, labels });
  assert.doesNotMatch(deniedHtml, /value="app"/);
  assert.doesNotMatch(deniedHtml, /App ändern/);
  assert.doesNotMatch(deniedHtml, /App modifizieren|Modul bearbeiten/);
  assert.match(deniedHtml, /value="note"/);
  assert.match(deniedHtml, /value="mention"/);
  assert.match(deniedHtml, /value="approval"/);

  const allowedHtml = renderGlobalCtoxContextModeHtml({ canModify: true, labels });
  assert.match(allowedHtml, /value="data" checked/);
  assert.match(allowedHtml, /value="app"/);
  assert.match(allowedHtml, /App ändern/);
  assert.match(allowedHtml, /Freigabe anfragen/);
  assert.doesNotMatch(allowedHtml, /App modifizieren|Modul bearbeiten/);
});

test('global CTOX context modes default to human-in-the-loop actions', () => {
  const modes = buildGlobalCtoxContextModes({ canModify: false });
  assert.deepEqual(modes.map((mode) => mode.value), ['data', 'ask', 'note', 'mention', 'approval']);
  assert.equal(modes.find((mode) => mode.value === 'note')?.label, 'Notiz an User');
  assert.equal(modes.find((mode) => mode.value === 'mention')?.label, 'User erwähnen');
  assert.equal(modes.find((mode) => mode.value === 'approval')?.label, 'Freigabe anfragen');
  assert.deepEqual(
    modes.map((mode) => mode.impact),
    ['data_mutation', 'read_only', 'human_note', 'human_mention', 'approval_required']
  );

  const html = renderGlobalCtoxContextModeHtml({ canModify: false });
  assert.match(html, /data-impact="read_only"/);
  assert.match(html, /Nur lesend/);
  assert.match(html, /data-impact="approval_required"/);
});

test('global CTOX user picker keeps active users and escapes datalist labels', () => {
  const users = buildBusinessUserPickerOptions([
    { id: 'inactive', display_name: 'Inactive', active: false },
    { id: 'deleted', display_name: 'Deleted', is_deleted: true },
    { user_id: 'reviewer', display_name: 'Reviewer <Lead>', role: 'admin', active: true },
    { id: 'note_target', display_name: 'Note Target', role: 'user', active: true },
  ], {
    session: { user: { id: 'current_user', display_name: 'Current User', role: 'user' } },
  });

  assert.deepEqual(
    users.map((user) => user.id),
    ['current_user', 'note_target', 'reviewer']
  );

  const html = renderBusinessUserDatalistOptions(users);
  assert.match(html, /value="reviewer"/);
  assert.match(html, /Reviewer &lt;Lead&gt; · admin/);
  assert.doesNotMatch(html, /inactive|deleted|<Lead>/);
});

test('global CTOX agent scope view exposes actor app data and external boundaries', () => {
  const view = buildGlobalCtoxAgentScopeView({
    actor: {
      id: 'team_member',
      display_name: 'Team Member',
      role: 'user',
    },
    module: {
      id: 'inventory',
      title: 'Inventory',
      version: '0.4.0',
    },
    lifecycle: {
      state: 'preview',
      label: 'Vorschau',
      versionLabel: 'v0.4.0',
      public: false,
      runtimeInstalled: true,
      canManage: false,
    },
    dataAccess: {
      summary: 'Freigegeben: Inventory Items (inventory_items)',
      status: 'reviewed',
      statusLabel: 'Geprüft',
      declared: ['inventory_items'],
      granted: ['inventory_items'],
      locked: [],
      grantsImplied: false,
    },
    context: {
      module: 'inventory',
      column: 'right',
      record_type: 'account',
      record_id: 'acc_1',
      label: 'Account A',
      selected_text: 'selected',
    },
    canModify: false,
    externalActions: 'none',
  });

  assert.deepEqual(view.rows.map((row) => row.key), ['actor', 'app', 'selection', 'data', 'external']);
  assert.equal(view.actor.id, 'team_member');
  assert.equal(view.app.module_id, 'inventory');
  assert.equal(view.app.version, 'v0.4.0');
  assert.equal(view.app.visibility, 'preview');
  assert.equal(view.app.can_modify, false);
  assert.equal(view.data.granted_collections[0], 'inventory_items');
  assert.equal(view.data.grants_implied, false);
  assert.equal(view.selection.record_id, 'acc_1');
  assert.equal(view.selection.has_selected_text, true);
  assert.equal(view.external_actions.label, 'In diesem Schritt aus');
});

test('global CTOX agent scope html uses business-facing labels and escapes values', () => {
  const html = renderGlobalCtoxAgentScopeHtml({
    actor: { display_name: 'Admin <script>', role: 'admin' },
    module: { id: 'support', title: 'Support & Care' },
    lifecycle: { state: 'team', label: 'Team', versionLabel: 'v1.0.0', public: true },
    dataAccess: { summary: 'Keine Datenbereiche deklariert' },
    context: { record_id: 'ticket_1', label: 'Ticket <1>' },
    externalActions: 'approval_required',
  });

  assert.match(html, /CTOX Zugriff/);
  assert.match(html, /Nutzer/);
  assert.match(html, /App/);
  assert.match(html, /Daten/);
  assert.match(html, /Externe Aktionen/);
  assert.match(html, /Nur mit Freigabe/);
  assert.match(html, /Admin &lt;script&gt;/);
  assert.match(html, /Ticket &lt;1&gt;/);
  assert.doesNotMatch(html, /<script>/);
});
