import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  READ_ERROR_GRACE_MS, isTransientReadError, readErrorEntry, visibleReadErrorKeys,
} from '../../customer-modules/outbound-lead-generation/read-error-grace.mjs';

const coded = (code, message = 'x') => Object.assign(new Error(message), { code });

test('startup read failures are transient, real failures are not', () => {
  assert.equal(isTransientReadError(coded('LEAD_QUERY_AUTHORITY_MISSING')), true);
  assert.equal(isTransientReadError(coded('LEAD_QUERY_TIMEOUT')), true);
  assert.equal(isTransientReadError(coded('LEAD_QUERY_GENERATION_CHANGED')), true);
  assert.equal(isTransientReadError(new Error('REMOTE_ERROR: secret master-key authority is unavailable')), true);
  assert.equal(isTransientReadError(coded('LEAD_LIST_PROJECTION_NOT_APPLIED')), false);
  assert.equal(isTransientReadError(new Error('permission denied')), false);
});

test('a transient failure stays hidden for the grace period, then shows', () => {
  const start = 1_000_000;
  const errors = new Map([['imports', readErrorEntry(undefined, coded('LEAD_QUERY_TIMEOUT'), start)]]);
  assert.deepEqual(visibleReadErrorKeys(errors, start + 20_000), []);
  // A continuing failure keeps its first timestamp.
  errors.set('imports', readErrorEntry(errors.get('imports'), coded('LEAD_QUERY_TIMEOUT'), start + 30_000));
  assert.equal(errors.get('imports').since, start);
  assert.deepEqual(visibleReadErrorKeys(errors, start + READ_ERROR_GRACE_MS), ['imports']);
});

test('a real failure shows at once', () => {
  const errors = new Map([['leads', readErrorEntry(undefined, coded('LEAD_DETAIL_INVALID'), 5)]]);
  assert.deepEqual(visibleReadErrorKeys(errors, 5), ['leads']);
});
