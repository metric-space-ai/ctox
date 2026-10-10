import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpuInterval, summarize } from './resource-observer.cjs';
import { loggedBusyObserver } from './lock-soak.cjs';
import { EventEmitter } from 'node:events';
const snapshot = (atMs, ticks) => ({ pid: 42, atMs, process: { startedTicks: 1, ticks },
  threads: [{ tid: 4, startedTicks: 2, ticks: ticks / 2 }] });
test('actual tick frequency and one-core basis, not wall time or whole-server CPU', () => {
 const delta = cpuInterval(snapshot(0, 100), snapshot(2000, 200), 100);
 assert.equal(delta.processCpuMs, 1000); assert.equal(delta.peerCpuMs, 500); assert.equal(delta.peerPercentOfOneCore, 25);
});
test('thread births and exits produce explicit bounds, never zero or invented exact CPU', () => {
 const a = snapshot(0, 100), b = snapshot(2000, 200);
 b.threads.push({ tid: 5, startedTicks: 5, ticks: 10 });
 const delta = cpuInterval(a, b, 100);
 assert.equal(delta.peerCpuMs, null); assert.equal(delta.peerCoverage, 'bounded-thread-churn');
 assert.equal(delta.peerCpuLowerMs, 500); assert.equal(delta.peerCpuUpperMs, 1000); assert.equal(delta.newThreads, 1);
 const exit = cpuInterval({ ...a, threads: [...a.threads, { tid: 6, startedTicks: 6, ticks: 5 }] }, snapshot(2000, 200), 100);
 assert.equal(exit.peerCpuMs, null); assert.equal(exit.vanishedThreads, 1);
});
test('PID reuse, thread replacement and counter reset cannot become zero CPU', () => {
 const first = snapshot(0, 100), last = snapshot(2000, 200);
 assert.throws(() => cpuInterval(first, { ...last, pid: 43 }, 100), /identity/);
 assert.throws(() => cpuInterval(first, snapshot(2000, 50), 100), /counter/);
 last.threads[0].startedTicks = 3; assert.throws(() => cpuInterval(first, last, 100), /thread set/);
});
test('heap growth retains signed deltas and high-water mark, not a leak assertion', () => {
 const samples = [100, 200, 120].map((used, index) => ({ name: `${index}`, browserAt: index * 1000,
  cpu: snapshot(index * 1000, 100 + index * 20), heap: { usedSize: used }, indexedDbBytes: index * 50 }));
 const r = summarize(samples, 100); assert.equal(r.growth.heapUsedPeakBytes, 200);
 assert.equal(r.growth.heapUsedDeltaBytes, 20); assert.equal(r.growth.indexedDbDeltaBytes, 100);
});
test('native lock messages are counted across stream splits without storing content', () => {
 const child = { stdout: new EventEmitter(), stderr: new EventEmitter() }, count = loggedBusyObserver(child);
 child.stderr.emit('data', Buffer.from('database is lo')); child.stderr.emit('data', Buffer.from('cked secret-value\n'));
 child.stdout.emit('data', Buffer.from('SQLITE_BUSY database is locked\nordinary line\n'));
 assert.equal(count(), 2);
});
