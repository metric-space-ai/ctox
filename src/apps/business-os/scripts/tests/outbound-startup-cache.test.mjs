import assert from 'node:assert/strict';
import test from 'node:test';
import {
  actionAllowedOnStartupSnapshot,
  buildStartupSnapshot,
  isUsableStartupSnapshot,
  STARTUP_CACHE_MAX_AGE_MS,
  startupCacheScope,
} from '../../customer-modules/outbound-lead-generation/startup-cache.mjs';

const lists = {
  sources: [{ id: 'northdata.de', label: 'North Data' }],
  adapters: [],
  imports: [{ id: 'import_1', title: 'Chemie' }],
  researchPolicies: [{ id: 'policy' }, null],
  leads: [{ id: 'lead_1', _rev: '1-a', name: 'Acme', campaign: 'Chemie' }],
};

test('a snapshot belongs to one user on one host', () => {
  assert.equal(startupCacheScope({ host: 'thesen.ctox.dev', userId: 'u1' }), 'thesen.ctox.dev|u1');
  assert.notEqual(
    startupCacheScope({ host: 'thesen.ctox.dev', userId: 'u1' }),
    startupCacheScope({ host: 'thesen.ctox.dev', userId: 'u2' }),
  );
  // Without a known user nothing is stored or read.
  assert.equal(startupCacheScope({ host: 'thesen.ctox.dev', userId: '' }), '');
});

test('only a recent snapshot with leads is shown', () => {
  const now = 1_800_000_000_000;
  const snapshot = buildStartupSnapshot(lists, now);
  assert.deepEqual(snapshot.researchPolicies, [{ id: 'policy' }]);
  assert.equal(isUsableStartupSnapshot(snapshot, now + 1000), true);
  assert.equal(isUsableStartupSnapshot(snapshot, now + STARTUP_CACHE_MAX_AGE_MS + 1), false);
  assert.equal(isUsableStartupSnapshot(buildStartupSnapshot({ ...lists, leads: [] }, now), now), false);
  assert.equal(isUsableStartupSnapshot({ ...snapshot, schema: 99 }, now), false);
  assert.equal(isUsableStartupSnapshot(null, now), false);
});

test('a snapshot allows viewing but no action that writes to CTOX', () => {
  for (const action of ['select-lead', 'select-campaign', 'lead-sort', 'toggle-lead', 'retry-sync']) {
    assert.equal(actionAllowedOnStartupSnapshot(action), true, action);
  }
  for (const action of ['research-lead', 'save-policy', 'toggle-source', 'delete-campaign',
    'import-leads', 'save-lead-editor', 'export-campaign-xlsx', 'add-custom-field', '']) {
    assert.equal(actionAllowedOnStartupSnapshot(action), false, action);
  }
});
