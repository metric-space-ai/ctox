import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  evaluateConsentGate,
  hasValidConsent,
  isConsentValid,
  retentionDue,
  selectExpiredForRetention,
} from './consent.js';

const NOW = Date.UTC(2026, 5, 1);
const DAY = 24 * 60 * 60 * 1000;

test('isConsentValid honours grant, withdrawal and expiry', () => {
  assert.ok(isConsentValid({ legal_basis: 'consent', granted_at_ms: NOW - DAY }, NOW));
  assert.ok(!isConsentValid({ legal_basis: 'consent', granted_at_ms: NOW - 10 * DAY, withdrawn_at_ms: NOW - DAY }, NOW));
  assert.ok(!isConsentValid({ legal_basis: 'consent', granted_at_ms: NOW - 10 * DAY, expires_at_ms: NOW - DAY }, NOW));
  assert.ok(!isConsentValid({ legal_basis: 'consent' }, NOW), 'no grant timestamp');
});

test('contract/legal bases are valid without a withdrawal check', () => {
  assert.ok(isConsentValid({ legal_basis: 'contract' }, NOW));
  assert.ok(!isConsentValid({ legal_basis: 'bogus' }, NOW));
});

test('hasValidConsent matches by purpose', () => {
  const ledger = [
    { purpose: 'present_to_client', legal_basis: 'consent', granted_at_ms: NOW - DAY },
    { purpose: 'talent_pool', legal_basis: 'consent', granted_at_ms: NOW - 10 * DAY, withdrawn_at_ms: NOW - DAY },
  ];
  assert.ok(hasValidConsent(ledger, 'present_to_client', NOW));
  assert.ok(!hasValidConsent(ledger, 'talent_pool', NOW));
  assert.ok(!hasValidConsent(ledger, 'unknown', NOW));
});

test('evaluateConsentGate allows when no purpose, denies when missing', () => {
  assert.equal(evaluateConsentGate({}, [], NOW).allowed, true);
  const denied = evaluateConsentGate({ purpose: 'present_to_client' }, [], NOW);
  assert.equal(denied.allowed, false);
  assert.equal(denied.reason, 'consent_missing');
});

test('retention selects records past their window', () => {
  assert.ok(retentionDue({ reference_ms: NOW - 400 * DAY }, { retentionDays: 365, nowMs: NOW }));
  assert.ok(!retentionDue({ reference_ms: NOW - 10 * DAY }, { retentionDays: 365, nowMs: NOW }));
  const expired = selectExpiredForRetention(
    [{ id: 'a', reference_ms: NOW - 400 * DAY }, { id: 'b', reference_ms: NOW - 10 * DAY }],
    { retentionDays: 365, nowMs: NOW },
  );
  assert.deepEqual(expired.map((r) => r.id), ['a']);
});
