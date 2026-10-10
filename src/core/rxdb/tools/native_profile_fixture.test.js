'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { EventEmitter } = require('node:events');

const source = fs.readFileSync(path.join(__dirname, 'browser_rust_smoke.js'), 'utf8');
const start = source.indexOf('async function stopChild(child) {');
const end = source.indexOf('\nfunction startCtoxServer()', start);
assert.ok(start >= 0 && end > start);
const stopSource = source.slice(start, end);

test('fixture finalizer observes the owned native child after perf closes and before signaling', async () => {
  const events = [];
  const child = Object.assign(new EventEmitter(), {
    exitCode: null,
    __ctoxNativeSymbolProfile: {
      async stop(reason) {
        assert.equal(reason, 'fixture-finalizer');
        await Promise.resolve();
        events.push('perf-closed');
      },
    },
    __ctoxNativeCpuProfile: {
      sample() {
        assert.equal(child.exitCode, null, 'native process must still be alive for boundary observation');
        events.push('cpu-sampled');
      },
    },
  });
  const stopChild = vm.runInNewContext(`(${stopSource})`, {
    setTimeout, clearTimeout,
    terminateOwnedSmokeChild(target, signal) {
      assert.equal(target, child);
      assert.equal(signal, 'SIGINT');
      events.push('native-signaled');
      setImmediate(() => { child.exitCode = 0; child.emit('exit', 0); });
    },
  });
  await stopChild(child);
  assert.deepEqual(events, ['perf-closed', 'cpu-sampled', 'native-signaled']);
});

test('isolated fixture starts its symbol recorder before a short UI reproduction can finish', () => {
  const profileStart = source.indexOf('child.__ctoxNativeSymbolProfile = startNativeSymbolProfile(');
  const optionsEnd = source.indexOf('}, {', profileStart);
  assert.ok(profileStart >= 0 && optionsEnd > profileStart);
  assert.match(source.slice(profileStart, optionsEnd), /delayMs:\s*0\s*,/);
});
