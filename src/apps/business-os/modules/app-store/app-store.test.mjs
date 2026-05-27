import test from 'node:test';
import assert from 'node:assert/strict';

import { __appStoreTestHooks as hooks } from './index.js';

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
      marketplaceCount: 18,
      installedCount: 17,
    }),
    '18 GitHub Module gefunden. 17 installierte Apps lokal gezählt.'
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
  assert.equal(hooks.itemMatchesScope({ kind: 'local', status: 'local' }, 'installed'), false);
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
