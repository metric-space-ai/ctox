import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  isTransientResearchWaitError,
  researchStartFailedBeforeWrite,
  researchStartFailureOutcome,
} from '../../customer-modules/outbound-lead-generation/research-start-outcome.mjs';

const failure = (fields, message = 'x') => Object.assign(new Error(message), fields);

// Field incident 17.10.2026: peerAuthorityError('native_unavailable') is
// transient, failed BEFORE the local write, and the lead was set to "running"
// with a command_id that never existed.
test('a transient pre-write failure is not a running research', () => {
  const error = failure({
    code: 'native_unavailable', transient: true, retryable: true, local_inserted: false, command_written: false,
  }, 'business_commands has no authenticated WebRTC peer after 45000 ms');
  assert.equal(researchStartFailedBeforeWrite(error), true);
  assert.equal(isTransientResearchWaitError(error), false);
  assert.equal(researchStartFailureOutcome(error), 'not_written');
});

test('a pre-write projection-style code still is not running', () => {
  const error = failure({ code: 'projection_delayed', local_inserted: false });
  assert.equal(researchStartFailureOutcome(error), 'not_written');
});

test('a transient failure after the local write keeps the lead trackable', () => {
  const error = failure({ code: 'projection_delayed', transient: true, local_inserted: true, command_written: true });
  assert.equal(researchStartFailedBeforeWrite(error), false);
  assert.equal(isTransientResearchWaitError(error), true);
  assert.equal(researchStartFailureOutcome(error), 'running');
});

test('unmarked legacy errors keep the previous classification', () => {
  assert.equal(researchStartFailureOutcome(failure({ transient: true })), 'running');
  assert.equal(researchStartFailureOutcome(new Error('Der Auftrag kann trotzdem angenommen worden sein')), 'running');
  assert.equal(researchStartFailureOutcome(new Error('permission denied')), 'other');
  assert.equal(researchStartFailureOutcome(null), 'other');
});
