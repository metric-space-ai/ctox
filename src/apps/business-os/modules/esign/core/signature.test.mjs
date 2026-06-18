import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  applySignerEvent,
  canTransitionSigner,
  isComplete,
  signatureRequestStatus,
} from './signature.js';

const NOW = Date.UTC(2026, 5, 1);
const DAY = 24 * 60 * 60 * 1000;

test('signer transitions are guarded', () => {
  assert.ok(canTransitionSigner('pending', 'signed'));
  assert.ok(canTransitionSigner('viewed', 'declined'));
  assert.ok(!canTransitionSigner('signed', 'pending'));
});

test('signatureRequestStatus derives overall state', () => {
  const base = { sent_at_ms: NOW - DAY, signers: [{ id: 'a', state: 'signed' }, { id: 'b', state: 'pending' }] };
  assert.equal(signatureRequestStatus(base, NOW), 'partially_signed');
  assert.equal(signatureRequestStatus({ ...base, signers: [{ state: 'signed' }, { state: 'signed' }] }, NOW), 'completed');
  assert.equal(signatureRequestStatus({ signers: [{ state: 'declined' }] }, NOW), 'declined');
  assert.equal(signatureRequestStatus({ sent_at_ms: NOW - 2 * DAY, expires_at_ms: NOW - DAY, signers: [{ state: 'pending' }] }, NOW), 'expired');
  assert.equal(signatureRequestStatus({ signers: [{ state: 'pending' }] }, NOW), 'created');
});

test('applySignerEvent enforces legal transitions immutably', () => {
  const req = { signers: [{ id: 'a', state: 'pending' }, { id: 'b', state: 'pending' }] };
  const next = applySignerEvent(req, 'a', 'signed');
  assert.equal(next.signers[0].state, 'signed');
  assert.equal(req.signers[0].state, 'pending', 'input not mutated');
  assert.throws(() => applySignerEvent(next, 'a', 'pending'), /illegal/);
});

test('isComplete only when all signed', () => {
  assert.ok(isComplete({ signers: [{ state: 'signed' }] }, NOW));
  assert.ok(!isComplete({ signers: [{ state: 'signed' }, { state: 'pending' }] }, NOW));
});
