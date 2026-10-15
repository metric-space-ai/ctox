import { test } from 'node:test';
import assert from 'node:assert/strict';
import { quantiles, reconcile } from './counters.mjs';
test('nearest rank and absence never become zero latency', () => {
  assert.equal(quantiles([]).p99, null); assert.equal(quantiles([3, 1, 2]).p50, 2); assert.equal(quantiles([3, 1, 2]).p99, 3);
});
test('missing, duplicate and late-after-error deliveries remain visible', () => {
  const tx = [{ kind: 'attempt', class: 'control', id: 'a' }, { kind: 'send_result', class: 'control', id: 'a', ok: true },
    { kind: 'attempt', class: 'control', id: 'b' }, { kind: 'send_result', class: 'control', id: 'b', ok: false }];
  const rx = [{ kind: 'received', class: 'control', id: 'b', bytes: 4 }, { kind: 'received', class: 'control', id: 'b', bytes: 4 }];
  const result = reconcile(tx, rx, 1).control;
  assert.deepEqual(result.acceptedButNotReceived, ['a']); assert.deepEqual(result.receivedWithoutAcceptedSend, ['b']);
  assert.equal(result.duplicateDeliveries, 1); assert.equal(result.reliableCountersEqual, false);
});
