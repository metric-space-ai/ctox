// Actual Workjet framing/schema client, native private endpoints and signed,
// independently persisted checkpoint copies. No VM or harness is executed here.
import assert from 'node:assert/strict';

export async function exerciseHandoff(requestSyncAuthority, source, fixture) {
  const { target, spec, receipts } = fixture;
  const send = (endpoint, operation, requestId) => requestSyncAuthority(endpoint, {
    version: 1, requestId: `handoff-${requestId}`, operation,
  });
  const created = await send(source, { type: 'create', spec }, 'create');
  assert.equal(created.result.type, 'applied');
  const ownership = created.result.ownership;
  assert.deepEqual(ownership, { nodeId: 1, generation: 1 });
  const protect = { type: 'protectCheckpoint', jobId: spec.jobId, ownership, receipts };
  const protectStart = performance.now();
  assert.equal((await send(source, protect, 'protect')).result.type, 'applied');
  const protectMs = performance.now() - protectStart;
  assert.equal((await send(source, protect, 'protect')).result.type, 'replayed');
  const takeover = {
    type: 'takeOver', jobId: spec.jobId, expected: ownership,
    checkpointDigest: receipts[0].checkpointDigest,
  };
  const takeoverStart = performance.now();
  const taken = await send(target, takeover, 'takeover');
  const takeoverMs = performance.now() - takeoverStart;
  assert.equal(taken.result.type, 'applied');
  assert.deepEqual(taken.result.ownership, { nodeId: 2, generation: 2 });
  assert.deepEqual((await send(target, takeover, 'takeover')).result, {
    type: 'replayed', spec, ownership: taken.result.ownership,
  });
  assert.equal((await send(source, {
    type: 'validate', jobId: spec.jobId, ownership,
  }, 'old-owner')).result.type, 'rejected');
  const samples = [];
  for (let i = 0; i < 30; i++) {
    const started = performance.now();
    const validated = await send(target, {
      type: 'validate', jobId: spec.jobId, ownership: taken.result.ownership,
    }, `new-owner-${i}`);
    samples.push(performance.now() - started);
    assert.equal(validated.result.type, 'authorized');
    assert.deepEqual(validated.result.ownership, taken.result.ownership);
  }
  samples.sort((a, b) => a - b);
  console.error(JSON.stringify({
    scenario: 'workjet-native-checkpoint-ipc', protectSamples: 1, protectMs,
    takeoverSamples: 1, takeoverMs, authorizedReadSamples: samples.length,
    authorizedReadP50Ms: samples[14], authorizedReadP95Ms: samples[28],
    quantile: 'nearest-rank', includesVmOrHarness: false,
  }));
}
