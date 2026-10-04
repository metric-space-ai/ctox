import assert from 'node:assert/strict';
import { createShellPerformanceTrace } from './shell-performance-trace.js';

let tick = 10;
let usedHeap = 100;
let observerCallback;
let stopped = false;
class FakeObserver {
  static supportedEntryTypes = ['longtask'];
  constructor(callback) { observerCallback = callback; }
  observe(options) { assert.deepEqual(options, { type: 'longtask', buffered: true }); }
  disconnect() { stopped = true; }
}
const trace = createShellPerformanceTrace({ capacity: 2, Observer: FakeObserver, clock: {
  now: () => tick,
  get memory() { return { usedJSHeapSize: usedHeap }; },
  getEntriesByType: type => type === 'navigation'
    ? [{ type: 'reload', startTime: 0, responseStart: 2, responseEnd: 8, domInteractive: 9, name: 'secret-url' }]
    : [{ initiatorType: 'script', transferSize: 16, duration: 5, name: 'secret-asset' },
      { initiatorType: 'fetch', transferSize: 400, duration: 300, name: 'private-data' }],
} });
assert.equal(await trace.measure('local-db-open', () => { tick += 5; usedHeap = 200; return 'opened'; }), 'opened');
const failure = new Error('private error text');
await assert.rejects(trace.measure('local-db-read', () => { tick += 2; throw failure; }), error => error === failure);
let snapshot = trace.snapshot();
assert.equal(snapshot.phases[0].durationMs, 5);
assert.equal(snapshot.phases[1].completed, false);
assert.equal(snapshot.jsHeap.observedPeakBytes, 200);
await trace.measure('first-module-mount', () => { tick += 3; usedHeap = 150; });
observerCallback({ getEntries: () => [{ duration: 60 }, { duration: 80 }] });
snapshot = trace.snapshot();
assert.equal(snapshot.phases.length, 2);
assert.equal(snapshot.droppedPhases, 1);
assert.equal(snapshot.jsHeap.currentUsedBytes, 150);
assert.equal(snapshot.jsHeap.observedPeakBytes, 200);
assert.deepEqual(snapshot.longTasks, { count: 2, totalMs: 140, longestMs: 80 });
assert.equal(snapshot.assets.observedCount, 1);
assert.equal(snapshot.assets.transferBytes, 16);
assert.equal(snapshot.rendererRssBytes, null);
assert.doesNotMatch(JSON.stringify(snapshot), /secret|private/);
snapshot.phases[0].name = 'mutated';
assert.notEqual(trace.snapshot().phases[0].name, 'mutated');
trace.stop();
assert.equal(stopped, true);
const unsupported = createShellPerformanceTrace({ clock: {}, Observer: undefined });
assert.equal(unsupported.snapshot().longTasks, null);
assert.equal(unsupported.snapshot().jsHeap.currentUsedBytes, null);
console.log('shell-performance-trace: bounded phases, failures, heap, long tasks, redaction and unsupported metrics PASS');
