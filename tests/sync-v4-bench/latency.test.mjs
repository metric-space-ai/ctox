import test from 'node:test';
import assert from 'node:assert/strict';
import { controlLatency, mergeLatency } from './latency.mjs';

test('offer time includes synchronous send and drain; no ACK never becomes zero', () => {
  const e = (kind, id, at, extra = {}) => ({ kind, id, at, class: 'control', ...extra });
  const row = controlLatency([
    { kind: 'measurement_start', at: 100 }, e('attempt', 'before', 90),
    e('attempt', 'a', 110), e('attempt', 'missing', 120), e('attempt', 'error', 130),
    { kind: 'measurement_stop', at: 200 }, e('ack', 'a', 250, { latencyMs: 1 }),
    e('ack', 'a', 260), e('ack', 'error', 270, { error: true }), e('ack', 'before', 280),
  ]);
  assert.deepEqual(row.samples, [140]);
  assert.equal(row.completedDuringDrain, 1);
  assert.equal(row.noSuccessfulAck, 2);
  assert.equal(row.errors, 1);
  assert.equal(mergeLatency([row]).pingMs.p99, 140);
});
