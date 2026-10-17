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

// (8) Field observation 17.10.2026: leads showed "running"/"queued" although
// their task was handled/failed/cancelled — a late start patch replaced the
// native final status.
import {
  nativeResearchFingerprint,
  patchClaimsResearchStart,
  terminalResearchStatusWins,
} from '../../customer-modules/outbound-lead-generation/research-start-outcome.mjs';

const runningLead = {
  id: 'lead-a', research_status: 'running', command_id: '', field_status: { name: { value: 'A' } },
};

test('a native final status written after the start wins over the start patch', () => {
  const start = { sinceMs: 1_000, commandId: 'cmd-1', baseFingerprint: nativeResearchFingerprint(runningLead) };
  // Same run finished natively (writeback carries the command id).
  assert.equal(terminalResearchStatusWins({ ...runningLead, research_status: 'needs_review', command_id: 'cmd-1' }, start), true);
  // Native writeback stamped field revisions after the start.
  assert.equal(terminalResearchStatusWins({
    ...runningLead,
    research_status: 'completed',
    field_status: { name: { value: 'A', revision: 'r1' } },
  }, start), true);
  // Final status written after the start (clock evidence only).
  assert.equal(terminalResearchStatusWins(
    { id: 'lead-a', research_status: 'failed', research_updated_at_ms: 2_000 },
    { sinceMs: 1_000, commandId: 'cmd-1' },
  ), true);
});

test('a re-research of an older finished lead may claim running', () => {
  const finished = {
    id: 'lead-b', research_status: 'completed', command_id: 'cmd-old',
    research_updated_at_ms: 500, field_status: { name: { value: 'B', revision: 'r0' } },
  };
  const start = { sinceMs: 1_000, commandId: 'cmd-new', baseFingerprint: nativeResearchFingerprint(finished) };
  assert.equal(terminalResearchStatusWins(finished, start), false);
  assert.equal(terminalResearchStatusWins({ ...finished, research_status: 'running' }, start), false,
    'non-terminal current status never blocks');
});

test('only running/queued patches are start claims', () => {
  assert.equal(patchClaimsResearchStart({ research_status: 'running' }), true);
  assert.equal(patchClaimsResearchStart({ research_status: 'queued' }), true);
  assert.equal(patchClaimsResearchStart({ research_status: 'failed' }), false);
  assert.equal(patchClaimsResearchStart({ payload: {} }), false);
});

test('index.js routes every start patch and the pending overlay through the guard', async () => {
  const { readFile } = await import('node:fs/promises');
  const source = await readFile(new URL('../../customer-modules/outbound-lead-generation/index.js', import.meta.url), 'utf8');
  assert.match(source, /researchStart && patchClaimsResearchStart\(patch\) && terminalResearchStatusWins\(current, researchStart\)/);
  assert.match(source, /pending\.researchStart && terminalResearchStatusWins\(lead, pending\.researchStart\)/);
  assert.match(source, /research_started_at_ms: Date\.now\(\),\s*\},\s*\}, \{ researchStart:/);
  assert.match(source, /research_wait_since_ms:[^\n]*\n\s*\},\s*\}, \{ researchStart \}\)/);
  assert.match(source, /const outcome = await patchLead\(id, patch, \{ researchStart \}\)/);
});
