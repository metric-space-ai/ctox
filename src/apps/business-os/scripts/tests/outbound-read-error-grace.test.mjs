import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  isTransientReadError, readErrorEntry,
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

test('a continuing failure keeps its first timestamp', () => {
  const start = 1_000_000;
  const first = readErrorEntry(undefined, coded('LEAD_QUERY_TIMEOUT'), start);
  assert.equal(readErrorEntry(first, coded('LEAD_QUERY_TIMEOUT'), start + 30_000).since, start);
});

test('any failure of a never-loaded collection is a startup failure', () => {
  const entry = readErrorEntry(undefined, coded('LEAD_DETAIL_INVALID'), 10, { neverLoaded: true });
  assert.equal(entry.transient, true);
});
