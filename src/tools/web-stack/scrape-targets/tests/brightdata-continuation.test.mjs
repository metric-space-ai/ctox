import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { collectionBinding, advanceCollection, DATASET } = require('../linkedin.com/scripts/brightdata-core.cjs');
const { projectProviderWait } = require('../linkedin.com/scripts/brightdata-continuation.cjs');

const input = { company: 'Fixture GmbH', country: 'DE', source_id: 'linkedin.com', research_operation_id: 'research-v1-' + 'a'.repeat(64) };
const binding = collectionBinding(input, 'https://www.linkedin.com/company/fixture/', ['https://www.linkedin.com/in/fixture-person/']);
const context = () => ({ rawInput: JSON.stringify(input), runDirectory: '/native/runs/scrape_run-fixture', targetKey: 'linkedin-com' });
const pending = () => ({ records: [], error_code: 'collection_pending', failure_mode: 'temporary_unreachable',
  continuation: { phase: 'pending', snapshot_id: 'sd_fixture', query_hash: binding.query_hash, binding, submission_attempt: 1 } });

test('actual core POST acceptance projects a current native wait, never failure fallback', async () => {
  let saved;
  const result = await advanceCollection(binding, null, {
    loadSecret: async () => 'secret-canary', claimSubmission: async () => true,
    saveState: async state => { saved = state; },
    fetch: async () => new Response(JSON.stringify({ snapshot_id: 'sd_fixture' }), { status: 202 }),
  });
  const output = projectProviderWait(result, context());
  assert.equal(saved.phase, 'pending');
  assert.equal(output.failure_mode, 'awaiting_provider');
  assert.deepEqual(output.records, []);
  assert.equal(output.continuation.run_id, 'scrape_run-fixture');
  assert.equal(output.continuation.operation_id, input.research_operation_id);
  assert.equal(output.continuation.dataset_id, DATASET);
  assert.equal(output.continuation.input_sha256, createHash('sha256').update(context().rawInput).digest('hex'));
  assert.equal(output.continuation.binding, undefined);
  assert(!JSON.stringify(output).includes('secret-canary'));
  assert(!JSON.stringify(output).includes('/native/'));
});

test('wait projection keeps operation and snapshot across native attempts', () => {
  const first = projectProviderWait(pending(), context());
  const next = projectProviderWait(pending(), { ...context(), runDirectory: '/native/runs/scrape_run-next' });
  assert.notEqual(first.continuation.run_id, next.continuation.run_id);
  assert.equal(first.continuation.operation_id, next.continuation.operation_id);
  assert.equal(first.continuation.snapshot_id, next.continuation.snapshot_id);
  const ready = pending(); ready.continuation.phase = 'ready';
  assert.equal(projectProviderWait(ready, context()).continuation.phase, 'ready');
});

test('wait projection rejects changed query, missing operation and foreign target', () => {
  for (const ctx of [
    { ...context(), rawInput: JSON.stringify({ ...input, company: 'Other GmbH' }) },
    { ...context(), rawInput: JSON.stringify({ ...input, country: 'AT' }) },
    { ...context(), rawInput: JSON.stringify({ ...input, research_operation_id: null }) },
    { ...context(), rawInput: JSON.stringify({ ...input, source_id: 'xing.com' }) },
    { ...context(), targetKey: 'other-target' }, { ...context(), runDirectory: 'relative/run' },
  ]) assert.equal(projectProviderWait(pending(), ctx).error_code, 'invalid_provider_continuation');
});

test('wait projection cannot mask completed, uncertain, invalid or partial results', () => {
  for (const mutate of [
    value => value.failure_mode = 'blocked',
    value => value.partial_output = true,
    value => value.query_completion = {},
    value => value.error = 'api_forbidden',
    value => value.error_code = 'submission_outcome_unknown',
    value => value.records.push({ field: 'person_name', value: 'Not complete' }),
    value => value.continuation.phase = 'completed',
    value => value.continuation.snapshot_id = '../../wrong',
    value => value.continuation.query_hash = 'c'.repeat(64),
    value => value.continuation.submission_attempt = 3,
  ]) {
    const value = pending(); mutate(value);
    const result = projectProviderWait(value, context());
    assert.equal(result.failure_mode, 'portal_drift');
    assert.equal(result.continuation, undefined);
    assert.deepEqual(result.records, []);
  }
});
