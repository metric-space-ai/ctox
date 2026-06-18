import test from 'node:test';
import assert from 'node:assert/strict';

import { __appStoreTestHooks as hooks } from './index.js';

const permissionState = ({ role = 'user', userId = 'user-1', governance = {} } = {}) => ({
  ctx: {
    session: {
      user: {
        id: userId,
        role,
      },
    },
  },
  catalog: {
    governance,
  },
});

test('creator navigation carries App Store return context', () => {
  assert.equal(
    hooks.creatorHashFromStore({ mode: 'scratch' }),
    'creator?source=app-store&return=app-store&mode=scratch'
  );
  assert.equal(
    hooks.creatorHashFromStore({ mode: 'upgrade', upgrade: 'buchhaltung' }),
    'creator?source=app-store&return=app-store&mode=upgrade&upgrade=buchhaltung'
  );
});

test('marketplace sync labels explain loading and stale counts', () => {
  assert.equal(hooks.appCountLabel(0, 'marketplace', 'loading'), '0 Apps · Sync');
  assert.equal(
    hooks.marketplaceStateLabel({
      status: 'ready',
      message: '',
      discoveredCount: 19,
      availableCount: 1,
      installedCount: 20,
    }),
    '19 GitHub Module gefunden. 1 noch nicht lokal vorhanden. 20 installierte Apps lokal gezählt.'
  );
  assert.match(
    hooks.marketplaceStateLabel({
      status: 'stale',
      message: 'rate limited',
      marketplaceCount: 18,
      installedCount: 17,
    }),
    /Zeige letzten Stand: rate limited/
  );
});

test('scope matching keeps card badges and category counters aligned', () => {
  assert.equal(hooks.itemMatchesScope({ kind: 'marketplace', status: 'installed' }, 'installed'), true);
  assert.equal(hooks.itemMatchesScope({ kind: 'local', status: 'local' }, 'installed'), true);
  assert.equal(hooks.itemMatchesScope({ kind: 'template', status: 'template', id: 'create-scratch' }, 'installed'), false);
  assert.equal(hooks.itemMatchesScope({ kind: 'local', status: 'local' }, 'local'), true);
});

test('canonical catalog item prefers installed local records over duplicate GitHub records', () => {
  const marketplace = { id: 'buchhaltung', kind: 'marketplace', status: 'available', title: 'Buchhaltung' };
  const local = { id: 'buchhaltung', kind: 'local', status: 'local', title: 'Buchhaltung' };
  assert.equal(hooks.chooseCanonicalCatalogItem(marketplace, local), local);
});

test('empty states distinguish sync loading from search misses', () => {
  assert.equal(hooks.emptyCatalogTitle('marketplace', '', 'loading'), 'GitHub Discovery läuft');
  assert.equal(hooks.emptyCatalogTitle('all', 'zz-no-hit', 'ready'), 'Keine Apps gefunden');
  assert.match(
    hooks.emptyCatalogBody('marketplace', '', 'loading'),
    /Katalog wird gerade mit GitHub synchronisiert/
  );
  assert.match(hooks.emptyCatalogBody('all', 'zz-no-hit', 'ready'), /zz-no-hit/);
});

test('external GitHub action exposes an explicit external-link marker', () => {
  assert.match(hooks.externalLinkIcon(), /external-link-icon/);
  assert.match(hooks.externalLinkIcon(), /aria-hidden="true"/);
});

test('managed app actions do not expose fake upgrades', () => {
  const editable = hooks.actionButtonsForManagedItem({
    title: 'Browser',
    id: 'browser',
    editable: true,
    deletable: false,
    update_available: false,
  }, permissionState({ role: 'chef' }));
  assert.match(editable, /Bearbeiten/);
  assert.doesNotMatch(editable, /Upgrade/);
  assert.doesNotMatch(editable, /Aktualisieren/);

  const update = hooks.actionButtonsForManagedItem({
    title: 'Buchhaltung',
    id: 'buchhaltung',
    editable: true,
    deletable: false,
    update_available: true,
    download_url: 'https://example.test/archive.zip',
  }, permissionState({ role: 'admin' }));
  assert.match(update, /Aktualisieren/);
  assert.match(update, /Bearbeiten/);
  assert.doesNotMatch(update, /Upgrade/);
});

test('managed app actions require projected permissions in addition to metadata', () => {
  const managed = {
    id: 'inventory',
    title: 'Inventory',
    editable: true,
    deletable: true,
    update_available: true,
    download_url: 'https://example.test/archive.zip',
  };
  const denied = hooks.actionButtonsForManagedItem(managed, permissionState({ role: 'user' }));
  assert.match(denied, /Aktualisieren/);
  assert.match(denied, /Deinstallieren/);
  assert.match(denied, /data-disabled-reason=/);
  assert.match(denied, /App-Installationsrecht/);
  assert.match(denied, /Entfernungsrecht/);
  assert.doesNotMatch(denied, /data-card-action="update"/);
  assert.doesNotMatch(denied, /Bearbeiten/);
  assert.doesNotMatch(denied, /data-card-action="uninstall"/);

  const granted = hooks.actionButtonsForManagedItem(managed, permissionState({
    role: 'user',
    userId: 'ops',
    governance: {
      permission_model: {
        explicit_grants: [
          {
            subject_type: 'user',
            subject_id: 'ops',
            permission: 'apps.install',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
          {
            subject_type: 'user',
            subject_id: 'ops',
            permission: 'apps.modify',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
          {
            subject_type: 'user',
            subject_id: 'ops',
            permission: 'apps.uninstall',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
        ],
      },
    },
  }));
  assert.match(granted, /Aktualisieren/);
  assert.match(granted, /Bearbeiten/);
  assert.match(granted, /Deinstallieren/);
  assert.match(granted, /data-card-action="update"/);
  assert.match(granted, /data-card-action="edit"/);
  assert.match(granted, /data-card-action="uninstall"/);
  assert.doesNotMatch(granted, /data-disabled-reason=/);
});

test('managed app actions recompute when governance grants refresh', () => {
  const managed = {
    id: 'inventory',
    title: 'Inventory',
    editable: true,
    deletable: true,
    update_available: true,
    download_url: 'https://example.test/archive.zip',
  };
  const beforeRefresh = hooks.actionButtonsForManagedItem(managed, permissionState({
    role: 'user',
    userId: 'ops',
  }));
  assert.match(beforeRefresh, /data-disabled-reason=/);
  assert.doesNotMatch(beforeRefresh, /data-card-action="edit"/);

  const afterRefresh = hooks.actionButtonsForManagedItem(managed, permissionState({
    role: 'user',
    userId: 'ops',
    governance: {
      permission_model: {
        explicit_grants: [
          {
            subject_type: 'user',
            subject_id: 'ops',
            permission: 'apps.install',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
          {
            subject_type: 'user',
            subject_id: 'ops',
            permission: 'apps.modify',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
          {
            subject_type: 'user',
            subject_id: 'ops',
            permission: 'apps.uninstall',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
        ],
      },
    },
  }));
  assert.match(afterRefresh, /data-card-action="update"/);
  assert.match(afterRefresh, /data-card-action="edit"/);
  assert.match(afterRefresh, /data-card-action="uninstall"/);
  assert.doesNotMatch(afterRefresh, /data-disabled-reason=/);
});

test('release wizard action requires apps.release and builds evidence-only payload', () => {
  const releaseApp = {
    id: 'inventory',
    title: 'Inventory',
    version: '0.8.0',
    editable: true,
    lifecycle: { runtimeInstalled: true },
    install_scope: 'installed',
    permissions: ['inventory_items', 'supplier_prices'],
    version_state: {
      version_count: 2,
      versions: [
        { version_id: 'version-current', seq: 2, origin: 'edit', label: 'Aktuelle Quelle' },
        { version_id: 'version-baseline', seq: 1, origin: 'install', label: 'Installation' },
      ],
    },
  };
  const denied = hooks.actionButtonsForManagedItem(releaseApp, permissionState({ role: 'user' }));
  assert.match(denied, /Freigeben/);
  assert.match(denied, /Freigaberecht/);
  assert.doesNotMatch(denied, /data-card-action="release"/);

  const grantedState = permissionState({
    role: 'user',
    userId: 'ops',
    governance: {
      permission_model: {
        explicit_grants: [
          {
            subject_type: 'user',
            subject_id: 'ops',
            permission: 'apps.release',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
        ],
      },
    },
  });
  const granted = hooks.actionButtonsForManagedItem(releaseApp, grantedState);
  assert.match(granted, /data-card-action="release"/);
  assert.equal(hooks.canReleaseAppStoreItem(grantedState, releaseApp), true);

  const model = hooks.releaseWizardModel(releaseApp, grantedState);
  assert.equal(model.targetVersion, '1.0.0');
  assert.equal(model.sourceVersionId, 'version-current');
  assert.equal(model.rollbackVersionId, 'version-baseline');
  assert.deepEqual(model.dataAreas.map((area) => area.collection), ['inventory_items', 'supplier_prices']);

  const payload = hooks.releasePayloadForWizard(releaseApp, {
    readCollections: ['inventory_items', 'unknown_collection'],
    writeCollections: ['supplier_prices'],
    responsibleUserIds: 'ops, founder-a',
    notes: 'Team release',
  }, grantedState);
  assert.equal(payload.module_id, 'inventory');
  assert.equal(payload.target_version, '1.0.0');
  assert.equal(payload.release_channel, 'team');
  assert.equal(payload.source_version_id, 'version-current');
  assert.equal(payload.rollback_version_id, 'version-baseline');
  assert.deepEqual(payload.responsible_user_ids, ['ops', 'founder-a']);
  assert.deepEqual(payload.data_access_review.collections, ['inventory_items', 'supplier_prices']);
  assert.deepEqual(payload.data_access_review.read_collections, ['inventory_items']);
  assert.deepEqual(payload.data_access_review.write_collections, ['supplier_prices']);
  assert.deepEqual(payload.data_access_review.locked_read_collections, ['supplier_prices']);
  assert.deepEqual(payload.data_access_review.locked_write_collections, ['inventory_items']);
  assert.equal(payload.data_access_review.review_is_evidence_only, true);
  assert.equal(payload.data_access_review.grants_implied, false);
  assert.match(payload.data_access_review.locked_state_behavior, /locked data state/);
});

test('app store context modify uses selected app permission and target', () => {
  const assignedFounder = permissionState({
    role: 'founder',
    userId: 'founder-a',
    governance: {
      founders: {
        inventory: [{ user_id: 'founder-a', active: true }],
      },
    },
  });
  assert.equal(hooks.canModifyAppStoreAppForModule(assignedFounder, { id: 'inventory' }), true);
  assert.equal(hooks.canModifyAppStoreAppForModule(assignedFounder, { id: 'billing' }), false);

  const detail = hooks.appStoreContextChatDetail(
    assignedFounder,
    {
      app_id: 'inventory',
      record_id: 'inventory',
      record_type: 'app',
      label: 'Inventory',
      app_title: 'Inventory',
      app_version: 'v0.9.0',
      app_visibility: 'private',
      app_visibility_label: 'Privat',
      data_access: {
        summary: 'Freigegeben: Inventory Items',
        granted_collections: ['inventory_items'],
      },
      column: 'grid',
      active_scope: 'installed',
    },
    'Bitte Formular verbessern',
    'app'
  );
  assert.equal(detail.command_type, 'ctox.business_os.app.modify');
  assert.equal(detail.record_id, 'inventory');
  assert.equal(detail.payload.module_id, 'inventory');
  assert.equal(detail.payload.app_id, 'inventory');
  assert.equal(detail.client_context.module_id, 'inventory');
  assert.equal(detail.client_context.app_id, 'inventory');
  assert.equal(detail.client_context.actor.id, 'founder-a');
  assert.equal(detail.client_context.visible_scope.app.module_id, 'inventory');
  assert.equal(detail.client_context.visible_scope.app.can_modify, true);
  assert.equal(detail.client_context.visible_scope.data.summary, 'Freigegeben: Inventory Items');
  assert.deepEqual(
    detail.client_context.visible_scope.rows.map((row) => row.label),
    ['Nutzer', 'App', 'Auswahl', 'Daten', 'Externe Aktionen']
  );
  assert.notEqual(detail.record_id, 'app-store');

  const deniedDetail = hooks.appStoreContextChatDetail(
    assignedFounder,
    {
      app_id: 'billing',
      record_id: 'billing',
      record_type: 'app',
      label: 'Billing',
      column: 'grid',
    },
    'Bitte fremde App ändern',
    'app'
  );
  assert.equal(deniedDetail.command_type, 'business_os.chat.task');
  assert.equal(deniedDetail.payload.mode, 'data');
  assert.equal(deniedDetail.client_context.module_id, 'billing');
  assert.equal(deniedDetail.client_context.visible_scope.app.can_modify, false);
});

test('app store lifecycle keeps 0.x apps private and 1.0.0 apps team-visible', () => {
  const governance = {
    founders: {
      inventory: [{ user_id: 'founder-a', active: true }],
    },
  };
  const team = permissionState({ role: 'user', userId: 'team-a', governance });
  const founder = permissionState({ role: 'founder', userId: 'founder-a', governance });
  const privateApp = {
    id: 'inventory',
    version: '0.1.0',
    install_scope: 'installed',
    entry: 'installed-modules/inventory/index.html',
  };
  const releasedApp = { ...privateApp, version: '1.0.0' };

  assert.equal(hooks.canSeeAppStoreModuleForAppVersion(team, privateApp), false);
  assert.equal(hooks.canSeeAppStoreModuleForAppVersion(founder, privateApp), true);
  assert.equal(hooks.canSeeAppStoreModuleForAppVersion(team, releasedApp), true);
  assert.deepEqual(
    {
      text: hooks.appLifecycleBadge(privateApp, {}).text,
      version: hooks.appLifecycleBadge(privateApp, {}).version,
      state: hooks.appLifecycleBadge(privateApp, {}).state,
    },
    { text: 'Privat', version: 'v0.1.0', state: 'private' }
  );
  assert.deepEqual(
    {
      text: hooks.appLifecycleBadge(releasedApp, {}).text,
      version: hooks.appLifecycleBadge(releasedApp, {}).version,
      state: hooks.appLifecycleBadge(releasedApp, {}).state,
    },
    { text: 'Team', version: 'v1.0.0', state: 'team' }
  );
});

test('version and modification states distinguish update and local edits', () => {
  assert.equal(hooks.compareVersions('v1.2.0', '1.1.9') > 0, true);
  assert.equal(hooks.compareVersions('1.0.0', 'v1') === 0, true);

  assert.deepEqual(
    hooks.updateStateFor(
      { version: '1.0.0' },
      { version: '1.2.0', download_url: 'https://example.test/archive.zip' },
      'installed'
    ),
    { available: true, reason: '1.2.0 ist verfügbar, lokal ist 1.0.0.' }
  );

  assert.equal(
    hooks.modificationStateFor(
      { manifest_sha256: 'local' },
      { manifest_sha256: 'release' },
      'installed'
    ).status,
    'modified'
  );
});

test('release projection facts use native lifecycle state and business data labels', () => {
  const raw = {
    id: 'inventory',
    version: '1.2.0',
    install_scope: 'installed',
    lifecycle: {
      runtime_installed: true,
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
  const projection = hooks.appReleaseProjection(raw);
  const lines = hooks.releaseFactLinesForItem({ raw, release_projection: projection });
  const text = lines.join('\n');
  assert.match(text, /Freigabe: Aktuell v1\.2\.0/);
  assert.match(text, /Rollback: Rollback-Ziel v1\.1\.0/);
  assert.match(text, /Inventory Items \(inventory_items\)/);
  assert.match(text, /Supplier Prices \(supplier_prices\)/);
  assert.match(text, /Datenrechte bleiben explizit/);

  const badge = hooks.releaseProjectionBadgeHtml({ release_projection: projection });
  assert.match(badge, /data-release-status="released"/);
  assert.match(badge, /Freigabe v1\.2\.0/);
});

test('fork-class apps do not offer destructive upstream updates', () => {
  const fork = hooks.updateStateFor(
    { version: '1.0.0' },
    { version: '1.2.0', download_url: 'https://example.test/archive.zip' },
    'installed',
    'fork'
  );
  assert.equal(fork.available, false);
  assert.match(fork.reason, /Fork-Apps/);

  const maintained = hooks.updateStateFor(
    { version: '1.0.0' },
    { version: '1.2.0', download_url: 'https://example.test/archive.zip' },
    'installed',
    'maintained'
  );
  assert.equal(maintained.available, true);
});

test('versions button reflects the recorded timeline', () => {
  assert.equal(hooks.versionsButtonHtml({ title: 'X' }), '');
  assert.equal(hooks.versionsButtonHtml({ title: 'X', version_state: { version_count: 0 } }), '');
  const html = hooks.versionsButtonHtml({ title: 'Buchhaltung', version_state: { version_count: 3 } });
  assert.match(html, /Versionen \(3\)/);
  assert.match(html, /data-card-action="versions"/);
});

test('install operations render card-local progress and terminal status', () => {
  assert.equal(hooks.statusForCard({ status: 'available' }, { kind: 'running' }), 'installing');
  assert.equal(hooks.statusForCard({ status: 'available' }, { kind: 'success' }), 'installed');
  assert.equal(hooks.statusForCard({ status: 'available' }, { kind: 'error' }), 'error');

  const progress = hooks.progressButtonHtml('Installing Outbound...');
  assert.match(progress, /card-btn primary is-progress/);
  assert.match(progress, /card-btn-progress-track/);
  assert.match(progress, /disabled/);

  const message = hooks.operationMessageHtml({ kind: 'success', text: 'Outbound installed.' });
  assert.match(message, /data-kind="success"/);
  assert.match(message, /Outbound installed\./);
});

test('origin labels are humanized for the timeline', () => {
  assert.equal(hooks.originLabel('install'), 'Installation');
  assert.equal(hooks.originLabel('rollback'), 'Rollback');
  assert.equal(hooks.originLabel('manual_release'), 'Release');
  assert.equal(hooks.originLabel('edit'), 'Bearbeitung');
});
