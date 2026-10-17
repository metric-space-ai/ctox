import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// v4: index.js importiert `ctox:shared/…` (SW-Umschreibung); der Loader bildet sie in Node nach.
import './tests/ctox-shared-loader.mjs';

const { __appStoreTestHooks: hooks } = await import('./index.js');

const source = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), 'index.js'), 'utf8');
const moduleDir = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(resolve(moduleDir, 'index.css'), 'utf8');
const html = readFileSync(resolve(moduleDir, 'index.html'), 'utf8');
const presentationSource = `${css}\n${html}`;
const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

// v4 (R9): Rechte-Spiegel aus api.actor {id, role, permissions}. Was v3 als Governance-Grant oder Founder-Zuweisung
// las, kommt in 4.0 als Rechte-Name in api.actor.permissions (ohne Modul-Scope, V4-PORT.md OFFEN-6).
const permissionState = ({ role = 'user', userId = 'user-1', permissions = [] } = {}) => ({
  actor: { id: userId, role, permissions },
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

test('Zählerzeile trägt keinen Sync-Zustand (R11: Zustand nur in der Zustandszone)', () => {
  assert.equal(hooks.appCountLabel(0, 'marketplace', 'loading'), '0 Apps');
  assert.equal(hooks.appCountLabel(5), '5 Apps');
  assert.equal(hooks.marketplaceStateLabel, undefined);
});

test('Katalog kommt live aus den Plattform-Collections, ohne Nachladeknopf und ohne Browser-Discovery', () => {
  assert.match(source, /api\.query\(collection, \{ sort: \[\{ id: 'asc' \}\] \}\)\.subscribe/);
  assert.match(source, /live\('business_apps', 'platformApps'\)/);
  assert.match(source, /live\('business_app_versions', 'platformVersions'\)/);
  // §16.29: Marketplace und Vorlagen lesen den Release-Katalog app_catalog.
  assert.match(source, /live\('app_catalog', 'platformCatalog'\)/);
  assert.doesNotMatch(source, /data-refresh-marketplace|refreshMarketplace|business_module_catalog/);
  assert.doesNotMatch(html, /data-refresh-marketplace/);
  // Eine Katalogquelle: keine Browser-seitige GitHub-Discovery.
  assert.doesNotMatch(source, /api\.github\.com/);
  assert.doesNotMatch(source, /raw\.githubusercontent\.com/);
});

test('§16.29: Katalog-Befehle tragen app_id + base_app_ver, keine v3-Felder mehr', () => {
  // Installieren aus app_catalog = ctox.app.update mit base_app_ver null; Update/Rollback/Freigabe/Entfernen/Veröffentlichen
  // über ctox.app.* mit der vom Client gesehenen aktiven Version als Basis.
  assert.match(source, /payload: \{ app_id: appId, base_app_ver: base, \.\.\.extra \}/);
  for (const type of ['ctox.app.update', 'ctox.app.rollback', 'ctox.app.release', 'ctox.app.remove', 'ctox.app.publish']) {
    assert.ok(source.includes(`'${type}'`), type);
  }
  assert.doesNotMatch(source, /ctox\.module\.update'|ctox\.module\.rollback_version|ctox\.module\.release|ctox\.app_store\.uninstall|expected_baseline_sha256/);
  // GitHub-/Zip-Quellen bleiben eigene Installationswege (externe Quelle, nicht app_catalog).
  assert.match(source, /source_kind: 'github',\s*repo,/);
});

test('Updates nur bei höherer main-Version und nicht lokal angepasster App', () => {
  const rows = [
    { app_id: 'x', app_ver: '5', source: 'instance' },
    { app_id: 'x', app_ver: '4', source: 'main' },
  ];
  assert.deepEqual(hooks.newerMainRelease(rows, '3'), { app_ver: '4', source: 'main', published_at: '' });
  assert.equal(hooks.newerMainRelease(rows, '4'), null, 'gleiche Version ist kein Update');
  assert.equal(hooks.newerMainRelease([{ app_id: 'x', app_ver: '9', source: 'instance' }], '3'), null, 'instance ist kein Update');
  const installed = { id: 'x', kind: 'installed', status: 'installed', installed_version: 'Installiert: 1.0.0', available_version: 'Katalog: lokal' };
  assert.equal(hooks.itemHasUpdate(installed), false, 'abweichende Anzeigezeilen allein sind kein Update (v3-Fehler)');
  assert.equal(hooks.itemHasUpdate({ ...installed, update_available: true, modification_status: 'clean' }), true);
  const customized = { ...installed, title: 'X', update_available: true, modification_status: 'modified', raw: { catalog_release: { app_ver: '4' } } };
  assert.equal(hooks.itemHasUpdate(customized), false, 'angepasst → nicht im Band „Updates“');
  assert.match(hooks.customizedUpdateBadgeHtml(customized), /data-update-customized="4"[^>]*>Update 4 · angepasst</);
  assert.equal(hooks.customizedUpdateBadgeHtml({ ...customized, modification_status: 'clean', raw: {} }), '');
  assert.equal(hooks.isTemplateEntry({ layout: { category: 'Vorlagen' } }), true);
  assert.equal(hooks.isTemplateEntry({ layout: { template: true } }), true);
  assert.equal(hooks.isTemplateEntry({ layout: { category: 'Sales' } }), false);
});

test('scope matching keeps card badges and category counters aligned', () => {
  assert.equal(hooks.itemMatchesScope({ kind: 'marketplace', status: 'installed' }, 'installed'), true);
  assert.equal(hooks.itemMatchesScope({ kind: 'local', status: 'local' }, 'installed'), true);
  assert.equal(hooks.itemMatchesScope({ kind: 'template', status: 'template', id: 'create-scratch' }, 'installed'), false);
  assert.equal(hooks.itemMatchesScope({ kind: 'local', status: 'local' }, 'local'), true);
});

test('reduced motion keeps the cards view on the actionable DOM grid', () => {
  assert.equal(hooks.shouldUseShelf(true, false, false), true, 'ordinary cards use the shelf');
  assert.equal(hooks.shouldUseShelf(true, false, true), false, 'reduced-motion cards use DOM cards');
  assert.equal(hooks.shouldUseShelf(true, true, false), false, 'shelf failure uses DOM cards');
  assert.equal(hooks.shouldUseShelf(false, false, false), false, 'list view never uses the shelf');
});

test('JSON export serializes the visible catalog or installed list', () => {
  const payload = hooks.buildAppStoreExport({
    items: [{
      id: 'inventory',
      title: 'Inventory',
      description: 'Stock control',
      category: 'Operations',
      kind: 'installed',
      status: 'installed',
      installed_version: '1.0.0',
      available_version: '1.1.0',
      update_available: true,
      developer: 'CTOX',
      license: 'AGPL-3.0-only',
      source: 'catalog',
      repo: 'metric-space-ai/ctox',
      source_path: 'modules/inventory',
      install_scope: 'installed',
      launch_kind: 'desktop-app',
    }],
    exportedAt: '2026-07-22T12:00:00.000Z',
    scope: 'installed',
    query: 'stock',
    categoryFilter: 'Operations',
    sortKey: 'category',
    centerBand: 'updates',
  });
  assert.equal(payload.format, 'ctox-app-store-export');
  assert.equal(payload.scope, 'installed');
  assert.equal(payload.view, 'updates');
  assert.equal(payload.filters.search, 'stock');
  assert.equal(payload.count, 1);
  assert.equal(payload.apps[0].id, 'inventory');
  assert.equal(payload.apps[0].installed, true);
  assert.equal(payload.apps[0].updateAvailable, true);
  assert.equal(payload.apps[0].sourcePath, 'modules/inventory');
});

test('canonical catalog item prefers installed local records over duplicate GitHub records', () => {
  const marketplace = { id: 'buchhaltung', kind: 'marketplace', status: 'available', title: 'Buchhaltung' };
  const local = { id: 'buchhaltung', kind: 'local', status: 'local', title: 'Buchhaltung' };
  assert.equal(hooks.chooseCanonicalCatalogItem(marketplace, local), local);
});

test('leere Liste rendert keinen Zustandstext (R11)', () => {
  assert.equal(hooks.emptyCatalogTitle, undefined);
  assert.equal(hooks.emptyCatalogBody, undefined);
  assert.doesNotMatch(html, /data-loading-spinner|data-store-message|store-empty-state/);
});

test('external GitHub action exposes an explicit external-link marker', () => {
  assert.match(hooks.externalLinkIcon(), /external-link-icon/);
  assert.match(hooks.externalLinkIcon(), /aria-hidden="true"/);
});

test('Zip-Upload geht über api.files.put, nicht über Chunk-Collections', () => {
  assert.match(source, /state\.api\.files\.put\(file, \{ id: fileId/);
  assert.match(source, /source_kind: 'zip',\s*file_id: fileId,\s*file_ref: fileRef/);
  assert.doesNotMatch(source, /desktop_file_chunks|leaseCollection|bulkUpsert|startCollection/);
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
    permissions: ['apps.install', 'apps.modify', 'apps.uninstall'],
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
    permissions: ['apps.install', 'apps.modify', 'apps.uninstall'],
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
    permissions: ['apps.release'],
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
  // 4.0 kennt keine Founder-Zuweisung je Modul im Actor-Spiegel (OFFEN-6): Recht `apps.source.edit` steht für die App,
  // ohne Recht bleibt die Änderung gesperrt. Ziel des Befehls bleibt die ausgewählte App.
  const assignedFounder = permissionState({ role: 'founder', userId: 'founder-a', permissions: ['apps.source.edit'] });
  const plainFounder = permissionState({ role: 'founder', userId: 'founder-a' });
  assert.equal(hooks.canModifyAppStoreAppForModule(assignedFounder, { id: 'inventory' }), true);
  assert.equal(hooks.canModifyAppStoreAppForModule(plainFounder, { id: 'billing' }), false);

  // SKL-D: der tote Test-Haken appStoreContextChatDetail ist entfernt; „App ändern“ sendet jetzt der Baustein
  // ctox:shared/context-chat.js (ctox.app.modify.request, Rechteprüfung im ctox-Handler). Geprüft bleibt der
  // Sichtbereich, den der App Store für die ausgewählte App zeigt.
  const scope = hooks.buildAppStoreAgentScopeView(
    assignedFounder,
    {
      app_id: 'inventory',
      record_id: 'inventory',
      record_type: 'app',
      label: 'Inventory',
      app_title: 'Inventory',
      data_access: { summary: 'Freigegeben: Inventory Items', granted_collections: ['inventory_items'] },
      column: 'grid',
      active_scope: 'installed',
    },
    'app'
  );
  assert.equal(scope.actor.id, 'founder-a');
  assert.equal(scope.app.module_id, 'inventory');
  assert.equal(scope.app.can_modify, true);
  assert.equal(scope.data.summary, 'Freigegeben: Inventory Items');
  assert.deepEqual(scope.rows.map((row) => row.label), ['Nutzer', 'App', 'Auswahl', 'Daten', 'Externe Aktionen']);
  const deniedScope = hooks.buildAppStoreAgentScopeView(plainFounder, { app_id: 'billing', record_id: 'billing', record_type: 'app', label: 'Billing', column: 'grid' }, 'app');
  assert.equal(deniedScope.app.module_id, 'billing');
  assert.equal(deniedScope.app.can_modify, false);
});

test('app store lifecycle keeps 0.x apps private and 1.0.0 apps team-visible', () => {
  // v3: Founder-Zuweisung; 4.0: Recht `apps.view` im Actor-Spiegel (OFFEN-6).
  const team = permissionState({ role: 'user', userId: 'team-a' });
  const founder = permissionState({ role: 'founder', userId: 'founder-a', permissions: ['apps.view'] });
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
    { text: 'App privat', version: 'v0.1.0', state: 'private' }
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

test('server-projected catalog diff offers an in-repo update', () => {
  const catalog = hooks.updateStateFor(
    { version: '1.0.0', lifecycle: { update_available: true, catalog_version: 'v2', installed_version: 'v1' } },
    null,
    'installed',
    'maintained'
  );
  assert.equal(catalog.available, true);
  assert.match(catalog.reason, /Katalog v2/);

  const current = hooks.updateStateFor(
    { version: '1.0.0', lifecycle: { update_available: false } },
    null,
    'installed',
    'maintained'
  );
  assert.equal(current.available, false);
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
  assert.match(progress, /ctox-button ctox-button--sm is-primary is-progress/);
  assert.match(progress, /card-btn-progress-track/);
  assert.match(progress, /disabled/);

  // Endmeldungen stehen nicht mehr in der Karte, sondern in der Zustandszone (R5/R11).
  assert.equal(hooks.operationMessageHtml({ kind: 'success', text: 'Outbound installed.' }), '');
});

test('origin labels are humanized for the timeline', () => {
  assert.equal(hooks.originLabel('install'), 'Installation');
  assert.equal(hooks.originLabel('rollback'), 'Rollback');
  assert.equal(hooks.originLabel('manual_release'), 'Release');
  assert.equal(hooks.originLabel('edit'), 'Bearbeitung');
});

test('presentation layer stays compact and shell-native', () => {
  assert.doesNotMatch(presentationSource, forbiddenSurfacePattern);
  assert.doesNotMatch(presentationSource, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
  assert.doesNotMatch(presentationSource, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
  assert.doesNotMatch(presentationSource, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
  // The frame is the shared kit workspace with a declarative shell-wired
  // column resizer (setupModuleResizers in app.js) — no module-owned grid or
  // resizer wiring.
  assert.match(html, /class="ctox-workspace ctox-workspace--two-pane app-store-module"[^>]*data-resize-frame/);
  assert.match(html, /class="ctox-column-resizer"[^>]*data-resizer="left"[^>]*data-resizer-var="--ctox-left-width"/);
  assert.doesNotMatch(source, /CtoxResizer/);
  assert.doesNotMatch(css, /grid-template-columns: var\(--app-store-left-width/);
  assert.match(css, /@container business-app-window \(max-width: 768px\)/);
  assert.match(css, /@container business-app-window \(max-width: 768px\)/);
  assert.match(css, /@container business-app-window \(max-width: 768px\)/);
  assert.match(css, /\.app-version-row[\s\S]*border-radius: var\(--panel-radius\)/);
  assert.doesNotMatch(css, /\.store-loading-overlay|\.store-message|\.store-empty-state/);
  assert.match(css, /\.app-release-data-row[\s\S]*border-radius: var\(--panel-radius\)/);
});

test('center column carries the canonical grammar and the shelf contract', () => {
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const css = readFileSync(new URL('./index.css', import.meta.url), 'utf8');
  const js = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
  // Filter section: search + ONE view toggle IN the filterbar + collapsed tray
  // with reset; active-dot rule present. The views are cards|list — the
  // retail-box shelf IS this app's cards rendering.
  assert.match(html, /store-filterbar/);
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-tray[^>]*hidden/);
  assert.match(html, /data-pg-reset/);
  assert.match(html, /data-pg-filter data-pg-name="category" data-pg-default="all"/);
  assert.match(html, /data-pg-filter data-pg-name="sort" data-pg-default="title"/);
  // ONE view control, not two (Betreiber-Direktive 31.08.2026): a single
  // action button, no aria-pressed, no [data-pg-view] radio pair. The glyph
  // that is shown names the view the click switches TO.
  assert.equal((html.match(/data-store-view-toggle/g) || []).length, 1);
  assert.doesNotMatch(html, /data-pg-view=/);
  const toggleTag = html.slice(html.indexOf('<button type="button" class="ctox-pane-icon store-view-toggle'));
  assert.doesNotMatch(toggleTag.slice(0, toggleTag.indexOf('>')), /aria-pressed/);
  assert.match(html, /data-view-glyph="list"/);
  assert.match(html, /data-view-glyph="cards" hidden/);
  assert.match(js, /els\.viewToggle\?\.addEventListener\('click'/);
  assert.match(js, /state\.viewMode = state\.viewMode === 'list' \? 'cards' : 'list'/);
  assert.match(js, /removeAttribute\('aria-pressed'\)/);
  assert.match(css, /\.store-filter-toggle\.has-active-filters::after/);
  // Counted view band with two real views (zeros included via JS counts).
  assert.match(html, /data-pg-band="catalog"/);
  assert.match(html, /data-pg-band="updates"/);
  assert.match(html, /data-pg-count="catalog"/);
  assert.match(html, /data-pg-count="updates"/);
  // Recessed well + per-pane one-line footers.
  assert.match(css, /\.store-well/);
  assert.equal((html.match(/data-pg-footer/g) || []).length, 2);
  // No standing count badge in the pane actions (counts live in footer/band).
  assert.doesNotMatch(html, /data-apps-count/);
  // Shelf: vendored three-based library loaded lazily with a list fallback;
  // scroll rides the pane well, never the window.
  assert.match(js, /import\('\.\/vendor\/store-shelf\/store-shelf\.mjs'\)/);
  assert.doesNotMatch(js, new RegExp('store-shelf\\.mjs\\?' + 'v='));
  assert.match(js, /shelfUnavailable/);
  assert.match(js, /const REDUCED_MOTION_QUERY = '\(prefers-reduced-motion: reduce\)'/);
  assert.match(js, /const shelfMode = shouldUseShelf\(cardsMode, state\.shelfUnavailable, prefersReducedMotion\(\)\)/);
  assert.match(html, /data-shelf-canvas/);
  assert.match(html, /data-shelf-track/);
  // Detail panel keeps real store actions (shared builder with the cards).
  assert.match(js, /cardActionsHtml\(item, operation, statusForCard\(item, operation\), \{ includeDetails: false \}\)/);
  assert.match(html, /data-detail-capture/);

  // The two views are genuinely different shapes (Betreiber-Direktive
  // 31.08.2026): a shard carries a bold title, two meta lines and the action
  // row with roomy padding; a list entry is ONE dense line — title plus a
  // single short meta on the right, no actions, no description.
  assert.match(js, /function renderCard\(item, \{ compact = false \} = \{\}\)/);
  assert.match(js, /function shardMetaFor\(item\)/);
  assert.match(js, /function rowMetaFor\(item, cardStatus, operation\)/);
  assert.match(js, /class="app-card-rowmeta"/);
  assert.match(css, /\.app-card--shard\s*\{[^}]*padding:\s*14px/);
  assert.match(css, /\.app-card--row\s*\{[^}]*min-height:\s*28px/);
  const rowBranch = js.slice(js.indexOf('  if (compact) {'), js.indexOf('  const actionsHtml = cardActionsHtml'));
  assert.ok(rowBranch.length > 0);
  assert.doesNotMatch(rowBranch, /app-card-actions|app-card-desc|app-card-version-row/);

  // Responsive stages are the shell's two breakpoints and nothing else
  // (shell-v2 contract §7.5 and §7.6): container queries, never @media
  // max-width, and only 1024px / 768px as column boundaries.
  assert.doesNotMatch(css, /@media[^{]*max-width/);
  const bounds = [...css.matchAll(/@container business-app-window \(max-width: (\d+)px\)/g)].map((m) => m[1]);
  assert.ok(bounds.length > 0);
  assert.deepEqual([...new Set(bounds)].sort(), ['1024', '768']);
});

test('chrome is shell-grammar plus header icons, never module-owned wiring', () => {
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const css = readFileSync(new URL('./index.css', import.meta.url), 'utf8');
  const js = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
  // Standing actions (create + both import paths + export + GitHub discovery)
  // are collected .ctox-pane-icon buttons in the catalog pane header — never
  // page-level text buttons in the content flow.
  assert.match(html, /class="ctox-pane-icon" data-action="create-scratch"/);
  assert.match(html, /class="ctox-pane-icon" data-action="install-github"/);
  assert.match(html, /class="ctox-pane-icon" data-action="install-zip"/);
  assert.match(html, /class="ctox-pane-icon" data-export-catalog[^>]*aria-label="[^"]+"[^>]*title="[^"]+"/);
  // Kein Nachladeknopf mehr (Live-Abo, R4/R12); Icons app-lokal (4.0 hat keine Icon-API, OFFEN-D).
  assert.doesNotMatch(html, /data-refresh-marketplace/);
  assert.match(js, /function actionIcon\(name\)/);
  assert.doesNotMatch(js, /getActionIcon/);
  assert.match(js, /new Blob\(\[JSON\.stringify\(payload, null, 2\)\]/);
  assert.doesNotMatch(html, /btn-create-scratch|btn-install-github|btn-install-zip/);
  assert.doesNotMatch(html, /store-action-btn|store-refresh-btn/);
  assert.doesNotMatch(css, /\.store-action-btn|\.store-refresh-btn/);
  // Keine stehende Sync-Zeile und kein Sync-Tooltip (R11).
  assert.doesNotMatch(html, /data-marketplace-state/);
  assert.doesNotMatch(css, /\.store-sync-state/);
  assert.doesNotMatch(js, /marketplaceStateLabel/);
  // Scope shards are pure selectors: title + ONE meta line with the
  // parenthesized count (zeros included) — no badge-pill chrome in the nav.
  const nav = html.slice(html.indexOf('data-scope-list'), html.indexOf('</nav>'));
  assert.ok(nav.length > 0);
  assert.doesNotMatch(nav, /ctox-badge/);
  assert.match(nav, /class="cat-meta" data-scope-count>\(0\)/);
  assert.match(js, /count\.textContent = `\(\$\{counts\[scope\] \|\| 0\}\)`/);
  // The shell wires the data-pg-* chrome; the module only listens and mirrors.
  assert.match(js, /addEventListener\('ctox-pane-grammar-change', onCenterGrammarChange\)/);
  assert.match(js, /__ctoxPaneGrammar/);
  // No hand-rolled grammar wiring left (the shell owns search/view/tray/band).
  assert.doesNotMatch(js, /syncStoreFilterIndicator/);
  assert.doesNotMatch(js, /els\.filterToggle|els\.filterTray|els\.filterReset|els\.bandTabs/);
  assert.doesNotMatch(html, /data-toggle-store-filters|data-store-filter-advanced|data-reset-store-filters|data-center-band|data-search|data-view-toggle/);
  // Selection is an in-place class flip on existing cards (never a rebuild);
  // data re-renders preserve the well's scroll offset (resetScroll only for
  // intentional grammar/scope changes).
  assert.match(js, /function applyAppStoreSelection\(\)/);
  assert.match(js, /aria-selected/);
  assert.match(js, /render\(\{ resetScroll = false \} = \{\}\)/);
  // Cards carry the context-record trio for "Chat to CTOX".
  assert.match(js, /contextRecordId/);
  assert.match(js, /contextRecordType/);
  assert.match(js, /contextLabel/);
  // Markup relativ ohne Cache-Buster (R10, SW liefert versioniert); CSS lädt die Shell aus dem Manifest.
  assert.match(js, /async function loadModuleMarkup\(\)/);
  assert.match(js, /fetch\(new URL\('\.\/index\.html', import\.meta\.url\)\)/);
  assert.doesNotMatch(js, new RegExp('\\?' + 'v=\\$\\{|appStoreStyle|ensureStyles'));
  // No localStorage; the sessionStorage creator-return handoff is the
  // established inter-app channel (knowledge uses the same for cross-module
  // focus handoffs) and stays.
  assert.doesNotMatch(js, /localStorage/);
});

test('v2 overlays stay inside the app window and icons stay app-local', () => {
  assert.match(source, /els\.root\?\.append\(overlay\)/);
  assert.doesNotMatch(source, /state\.ctx/);
  assert.doesNotMatch(source, /document\.body\.append\(overlay\)/);
  assert.match(source, /function actionIcon\(name\)/);
  assert.match(source, /const paths = \{[\s\S]*close:/);
  assert.match(css, /\.app-store-module > \.ctox-modal \{ position: absolute; inset: 0;/);
});
