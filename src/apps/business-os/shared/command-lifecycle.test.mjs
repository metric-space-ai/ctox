import test from 'node:test';
import assert from 'node:assert/strict';

import {
  validateCommandLifecycleDocument,
  validateCommandLifecycleTransition,
} from './command-lifecycle.js';
import {
  CTOX_COMMAND_CONTRACT_VERSION,
  CTOX_COMMAND_LIFECYCLE_CAPABILITY,
} from './command-lifecycle.generated.js';

function document(executionPhase, terminalStatus, projectionVersion, attempt = 0) {
  return {
    contract_version: CTOX_COMMAND_CONTRACT_VERSION,
    command_id: 'cmd-contract',
    idempotency_key: 'cmd-contract',
    payload_hash: 'sha256:fixture',
    module: 'ctox',
    command_type: 'business_os.chat.task',
    record_id: '',
    payload: { instruction: 'test' },
    client_context: {},
    created_at_ms: 1,
    replication_phase: 'native_observed',
    execution_mode: 'queue',
    execution_phase: executionPhase,
    terminal_status: terminalStatus,
    projection_version: projectionVersion,
    attempt,
  };
}

test('lifecycle v2 contract exposes the negotiated capability', () => {
  assert.equal(CTOX_COMMAND_CONTRACT_VERSION, 2);
  assert.equal(CTOX_COMMAND_LIFECYCLE_CAPABILITY, 'ctox-command-lifecycle-v2');
});

test('lifecycle accepts review and bounded rework transitions', () => {
  const running = document('running', 'none', 3);
  const review = document('awaiting_review', 'none', 4);
  const retry = document('retry_wait', 'none', 5, 1);
  const queued = document('queued', 'none', 6, 1);
  assert.equal(validateCommandLifecycleTransition(running, review), review);
  assert.equal(validateCommandLifecycleTransition(review, retry), retry);
  assert.equal(validateCommandLifecycleTransition(retry, queued), queued);
});

test('lifecycle rejects terminal regression and immutable payload changes', () => {
  const terminal = document('terminal', 'completed', 7, 1);
  assert.throws(
    () => validateCommandLifecycleTransition(terminal, document('running', 'none', 8, 2)),
    (error) => error.code === 'invalid_transition',
  );

  const changed = document('queued', 'none', 2);
  changed.payload = { instruction: 'different' };
  assert.throws(
    () => validateCommandLifecycleTransition(document('accepted', 'none', 1), changed),
    (error) => error.code === 'idempotency_conflict',
  );
});

test('lifecycle requires terminal phase and status to agree', () => {
  assert.throws(
    () => validateCommandLifecycleDocument(document('terminal', 'none', 1)),
    (error) => error.code === 'invalid_transition',
  );
  assert.throws(
    () => validateCommandLifecycleDocument(document('running', 'failed', 1)),
    (error) => error.code === 'invalid_transition',
  );
});
