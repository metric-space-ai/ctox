import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __appStoreTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

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
    editable: true,
    deletable: false,
    update_available: false,
  });
  assert.match(editable, /Bearbeiten/);
  assert.doesNotMatch(editable, /Upgrade/);
  assert.doesNotMatch(editable, /Aktualisieren/);

  const update = hooks.actionButtonsForManagedItem({
    title: 'Buchhaltung',
    editable: true,
    deletable: false,
    update_available: true,
    download_url: 'https://example.test/archive.zip',
  });
  assert.match(update, /Aktualisieren/);
  assert.match(update, /Bearbeiten/);
  assert.doesNotMatch(update, /Upgrade/);
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

test('origin labels are humanized for the timeline', () => {
  assert.equal(hooks.originLabel('install'), 'Installation');
  assert.equal(hooks.originLabel('rollback'), 'Rollback');
  assert.equal(hooks.originLabel('manual_release'), 'Release');
  assert.equal(hooks.originLabel('edit'), 'Bearbeitung');
});
