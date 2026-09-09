'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { EventEmitter } = require('node:events');
const { PassThrough } = require('node:stream');
const { spawn } = require('node:child_process');
const { startNativeSymbolProfile } = require('./native_symbol_profile.js');

function child(pid = 4242) {
  return Object.assign(new EventEmitter(), { pid, exitCode: null, signalCode: null,
    kill() { throw new Error('profiler must not signal the native child'); } });
}
function temporary(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-symbol-profile-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  return path.join(directory, 'native');
}
function fakeRecorder(outputPath, { exitCode = 0, exitSignal = null, ignoreInterrupt = false } = {}) {
  const recorder = Object.assign(new EventEmitter(), { exitCode: null, signalCode: null,
    stderr: new PassThrough(), signals: [] });
  recorder.kill = signal => {
    recorder.signals.push(signal);
    if (signal === 'SIGINT' && ignoreInterrupt) return true;
    fs.writeFileSync(outputPath, 'fixture-perf-data');
    recorder.exitCode = exitSignal ? null : exitCode;
    recorder.signalCode = exitSignal;
    setImmediate(() => recorder.emit('close', recorder.exitCode, recorder.signalCode));
    return true;
  };
  return recorder;
}

test('unsupported platform records unavailable and never attaches', async t => {
  const native = child(), outputPrefix = temporary(t);
  const { completion } = startNativeSymbolProfile(native, { outputPrefix }, {
    platform: 'darwin', spawnRecord() { assert.fail('must not attach'); },
  });
  assert.equal((await completion).reason, 'linux-perf-required');
  assert.equal(native.listenerCount('exit'), 0);
});

test('delayed attachment rejects PID reuse and a stopped fixture', async t => {
  for (const reused of [true, false]) {
    const native = child(), outputPrefix = temporary(t);
    let reads = 0;
    const profile = startNativeSymbolProfile(native, { outputPrefix, delayMs: 5 }, {
      platform: 'linux', readStart: () => (++reads === 1 || !reused ? 10 : 20),
      spawnRecord() { assert.fail('stale child attachment'); },
    });
    if (!reused) native.exitCode = 0;
    assert.equal((await profile.completion).reason, 'native-identity-changed');
  }
});

test('explicit stop before attachment is idempotent and removes listeners', async t => {
  const native = child(), outputPrefix = temporary(t);
  const profile = startNativeSymbolProfile(native, { outputPrefix }, {
    platform: 'linux', readStart: () => 10, spawnRecord() { assert.fail('must not attach'); },
  });
  await profile.stop('fixture-ended');
  assert.equal((await profile.stop()).reason, 'fixture-ended');
  assert.equal(native.listenerCount('exit'), 0);
});

test('bounded flat sampling targets only the supplied native PID without stack dumps', async t => {
  const native = child(), outputPrefix = temporary(t);
  let recorder, recordArgs, reportArgs;
  const terminationRequests = [];
  const profile = startNativeSymbolProfile(native, { outputPrefix, delayMs: 0, durationMs: 10 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord(executable, args, options) {
      assert.equal(executable, 'perf'); recordArgs = args;
      assert.deepEqual(options.stdio, ['ignore', 'ignore', 'pipe']);
      assert.equal(options.env.PERF_CONFIG, '/dev/null');
      recorder = fakeRecorder(outputPrefix + '.perf.data');
      return recorder;
    },
    async runReport(executable, args) {
      reportArgs = args; return { stdout: '# Samples: 12 of event cpu-clock:u\nctox::bounded_cpu_work\n' };
    },
    signalRecord(target, signal, reason) {
      assert.equal(target, recorder);
      terminationRequests.push({ signal, reason });
      return target.kill(signal);
    },
  });
  const result = await profile.completion;
  assert.equal(result.available, true); assert.equal(result.reason, 'sampled');
  assert.equal(result.reportedSamples, '12');
  assert.equal(recordArgs[recordArgs.indexOf('--pid') + 1], '4242');
  assert.ok(recordArgs.includes('--no-inherit'));
  assert.ok(recordArgs.includes('--no-buildid-cache'));
  assert.equal(recordArgs[recordArgs.indexOf('--max-size') + 1], '32M');
  assert.ok(!recordArgs.some(arg => ['-a', '--all-cpus', '-g', '--call-graph', '--inherit'].includes(arg)));
  assert.ok(reportArgs.includes('--no-children'));
  assert.deepEqual(recorder.signals, ['SIGINT']);
  assert.deepEqual(terminationRequests, [{ signal: 'SIGINT', reason: 'duration-limit' }]);
  assert.equal(native.listenerCount('exit'), 0);
});

test('requested SIGINT can finalize a valid Linux perf recording', async t => {
  const outputPrefix = temporary(t);
  const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0, durationMs: 5 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord: () => fakeRecorder(outputPrefix + '.perf.data', { exitSignal: 'SIGINT' }),
    runReport: async () => ({ stdout: '# Samples: 88 of event cpu-clock:u\nnode::work\n' }),
  });
  const result = await profile.completion;
  assert.equal(result.recordCode, null);
  assert.equal(result.recordSignal, 'SIGINT');
  assert.equal(result.controlledInterrupt, true);
  assert.equal(result.available, true);
});

test('an unsolicited SIGINT remains a failed recording', async t => {
  const outputPrefix = temporary(t);
  const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord() {
      const recorder = fakeRecorder(outputPrefix + '.perf.data', { exitSignal: 'SIGINT' });
      setImmediate(() => recorder.kill('SIGINT'));
      return recorder;
    },
    runReport: async () => assert.fail('unsolicited interruption is not acceptance'),
  });
  const result = await profile.completion;
  assert.equal(result.controlledInterrupt, false);
  assert.equal(result.available, false);
  assert.equal(result.reason, 'perf-record-failed');
});

test('native exit stops only its profiler and retains partial sample evidence', async t => {
  const native = child(), outputPrefix = temporary(t);
  let recorder;
  const profile = startNativeSymbolProfile(native, { outputPrefix, delayMs: 0 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord() {
      recorder = fakeRecorder(outputPrefix + '.perf.data');
      setImmediate(() => { native.exitCode = 0; native.emit('exit', 0); });
      return recorder;
    },
    async runReport() { return { stdout: '# Samples: 3 of event cpu-clock:u\n' }; },
  });
  const result = await profile.completion;
  assert.equal(result.stopReason, 'native-child-exited');
  assert.equal(result.available, true);
  assert.deepEqual(recorder.signals, ['SIGINT']);
});

test('perf permission failure preserves diagnostics without claiming a sample', async t => {
  const outputPrefix = temporary(t);
  const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord() {
      const recorder = fakeRecorder(outputPrefix + '.perf.data');
      setImmediate(() => {
        recorder.stderr.write('perf_event_open failed: Permission denied');
        recorder.exitCode = 255; recorder.emit('close', 255, null);
      });
      return recorder;
    },
    async runReport() { assert.fail('failed recording must not be reported as valid'); },
  });
  const result = await profile.completion;
  assert.equal(result.available, false);
  assert.equal(result.reason, 'perf-record-failed');
  assert.match(result.recordStderr, /Permission denied/);
});

test('empty sample reports are explicitly unavailable', async t => {
  const outputPrefix = temporary(t);
  const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0, durationMs: 5 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord: () => fakeRecorder(outputPrefix + '.perf.data', { exitSignal: 'SIGINT' }),
    runReport: async () => ({ stdout: '# Samples: 0 of event cpu-clock:u\n' }),
  });
  assert.equal((await profile.completion).reason, 'no-samples');
});

test('an unresponsive recorder is killed after the bounded interrupt grace', async t => {
  const outputPrefix = temporary(t);
  let recorder;
  const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0, durationMs: 5 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord: () => (recorder = fakeRecorder(outputPrefix + '.perf.data', { ignoreInterrupt: true, exitCode: 137 })),
  });
  assert.equal((await profile.completion).available, false);
  assert.deepEqual(recorder.signals, ['SIGINT', 'SIGKILL']);
});

const requirePerfAt = process.argv.indexOf('--require-perf');
test('real Linux owned-child CPU sampling resolves symbols', { skip: requirePerfAt < 0 }, async t => {
  assert.equal(process.platform, 'linux');
  const perfExecutable = process.argv[requirePerfAt + 1];
  assert.ok(perfExecutable);
  const outputPrefix = temporary(t);
  // An owned process with bounded CPU work; no unrelated process discovery.
  const native = spawn(process.execPath, ['-e', 'const end=Date.now()+8000; while(Date.now()<end) Math.sqrt(Math.random());'], {
    stdio: ['ignore', 'ignore', 'ignore'],
  });
  const profile = startNativeSymbolProfile(native, { outputPrefix, perfExecutable, delayMs: 100, durationMs: 2000 });
  try {
    const result = await profile.completion;
    assert.equal(result.available, true, JSON.stringify(result));
    assert.equal(result.reason, 'sampled');
    assert.match(fs.readFileSync(outputPrefix + '.report.txt', 'utf8'), /node|libc|libnode|v8/);
  } finally {
    await profile.stop();
    if (native.exitCode === null && native.signalCode === null) {
      const exited = new Promise(resolve => native.once('exit', resolve));
      native.kill('SIGKILL');
      await exited;
    }
  }
});
