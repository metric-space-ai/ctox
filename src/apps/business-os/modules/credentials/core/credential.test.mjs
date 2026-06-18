import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  credentialStatus,
  daysUntilExpiry,
  evaluateDeploymentReadiness,
  isDeploymentBlocking,
  isDeploymentBlockingType,
} from './credential.js';

const NOW = Date.UTC(2026, 5, 1);
const DAY = 24 * 60 * 60 * 1000;

test('credentialStatus classifies validity window', () => {
  assert.equal(credentialStatus({ verified: true, valid_until_ms: NOW + 200 * DAY }, NOW), 'valid');
  assert.equal(credentialStatus({ verified: true, valid_until_ms: NOW + 10 * DAY }, NOW), 'expiring');
  assert.equal(credentialStatus({ verified: true, valid_until_ms: NOW - DAY }, NOW), 'expired');
  assert.equal(credentialStatus({ verified: false, valid_until_ms: NOW + 200 * DAY }, NOW), 'unverified');
  assert.equal(credentialStatus({ verified: true, valid_from_ms: NOW + 10 * DAY }, NOW), 'not_yet_valid');
});

test('daysUntilExpiry returns Infinity when no expiry set', () => {
  assert.equal(daysUntilExpiry({ verified: true }, NOW), Infinity);
});

test('isDeploymentBlocking only for blocking types that are not valid', () => {
  assert.ok(isDeploymentBlockingType('staplerschein'));
  assert.ok(!isDeploymentBlockingType('g37'));
  assert.ok(isDeploymentBlocking({ credential_type: 'staplerschein', verified: true, valid_until_ms: NOW - DAY }, NOW));
  assert.ok(!isDeploymentBlocking({ credential_type: 'staplerschein', verified: true, valid_until_ms: NOW + 100 * DAY }, NOW));
  assert.ok(!isDeploymentBlocking({ credential_type: 'g37', verified: false }, NOW), 'non-blocking type never blocks');
});

test('evaluateDeploymentReadiness flags missing and expired required credentials', () => {
  const creds = [
    { id: 'c1', credential_type: 'staplerschein', verified: true, valid_until_ms: NOW + 100 * DAY },
    { id: 'c2', credential_type: 'aufenthaltstitel', verified: true, valid_until_ms: NOW - DAY },
  ];
  const result = evaluateDeploymentReadiness(creds, { requiredTypes: ['staplerschein', 'fuehrerschein', 'aufenthaltstitel'], nowMs: NOW });
  assert.equal(result.ready, false);
  assert.ok(result.blockers.some((b) => b.credential_type === 'fuehrerschein' && b.reason === 'missing'));
  assert.ok(result.blockers.some((b) => b.credential_type === 'aufenthaltstitel' && b.reason === 'expired'));

  const ok = evaluateDeploymentReadiness(
    [{ id: 'c1', credential_type: 'staplerschein', verified: true, valid_until_ms: NOW + 100 * DAY }],
    { requiredTypes: ['staplerschein'], nowMs: NOW },
  );
  assert.equal(ok.ready, true);
});
