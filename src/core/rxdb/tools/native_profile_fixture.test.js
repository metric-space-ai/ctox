'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { EventEmitter } = require('node:events');

const source = fs.readFileSync(path.join(__dirname, 'browser_rust_smoke.js'), 'utf8');
const start = source.indexOf('async function stopChild(child) {');
const end = source.indexOf('\nfunction startReadyCtoxSymbolProfile(', start);
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

test('symbol attachment waits for native peer worker readiness and captures the aligned CPU boundary', async () => {
  const readySource = source.match(/function startReadyCtoxSymbolProfile\(child\) \{[\s\S]*?\n\}/)?.[0];
  assert.ok(readySource);
  const sequenceStart = source.indexOf('    const ctoxServerWaitStartedAt = Date.now();');
  const sequenceEnd = source.indexOf("    if (smokeMode === 'business-os-sellify-scale-ui')", sequenceStart);
  assert.ok(sequenceStart >= 0 && sequenceEnd > sequenceStart);
  const startupSource = source.slice(sequenceStart, sequenceEnd);
  const events = [];
  let listening, peerReady;
  const native = Object.assign(new EventEmitter(), { pid: 4242, exitCode: null });
  native.__ctoxNativeCpuProfile = { sample() { events.push('cpu-boundary'); } };
  const context = {
    ctox: native, nativeSymbolPerf: '/usr/bin/perf',
    smokeProcessLifecyclePath: '/isolated/context-processes.json',
    serverReadyTimeoutMs: 100, syncConfigWaitMs: 100, outerPhaseTimings: {},
    waitForCtoxServerListening: () => new Promise(resolve => { listening = resolve; }),
    waitForNativePeerSyncConfig: () => new Promise(resolve => { peerReady = resolve; }),
    startNativeSymbolProfile(target, options) {
      assert.equal(target, native);
      assert.equal(options.delayMs, 0);
      assert.deepEqual(events, ['cpu-boundary']);
      events.push('perf-attached');
      return {};
    },
    console: { log() {} },
  };
  const run = vm.runInNewContext(`${readySource}\n(async () => { ${startupSource} })`, context);
  const completion = run();
  assert.deepEqual(events, []);
  listening();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(events, [], 'listening alone does not imply worker readiness');
  peerReady({ native_rxdb_peer_available: true });
  await completion;
  assert.deepEqual(events, ['cpu-boundary', 'perf-attached']);

  events.length = 0;
  const rejected = run();
  listening();
  await new Promise(resolve => setImmediate(resolve));
  peerReady({ native_rxdb_peer_available: false });
  await assert.rejects(rejected, /native peer unavailable/);
  assert.deepEqual(events, [], 'an unavailable peer must not start a misleading recording');
});
