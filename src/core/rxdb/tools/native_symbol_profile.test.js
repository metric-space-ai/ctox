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
    stdout: new PassThrough(), stderr: new PassThrough(), signals: [] });
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
      assert.deepEqual(options.stdio, ['ignore', 'pipe', 'pipe']);
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

test('recording failures retain bounded stdout and stderr without accepting data', async t => {
  const outputPrefix = temporary(t);
  const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord() {
      const recorder = fakeRecorder(outputPrefix + '.perf.data');
      setImmediate(() => {
        recorder.stdout.write('stdout diagnostic\n' + 'x'.repeat(20000));
        recorder.stdout.write('must not grow the retained buffer');
        recorder.stderr.write('stderr diagnostic\n' + 'y'.repeat(20000));
        recorder.exitCode = 255;
        recorder.emit('close', 255, null);
      });
      return recorder;
    },
    async runReport() { assert.fail('diagnostic output cannot validate a failed recording'); },
  });
  const result = await profile.completion;
  assert.equal(result.reason, 'perf-record-failed');
  assert.equal(result.available, false);
  assert.equal(result.recordCode, 255);
  assert.match(result.recordStdout, /^stdout diagnostic/);
  assert.match(result.recordStderr, /^stderr diagnostic/);
  assert.equal(result.recordStdout.length, 16384);
  assert.equal(result.recordStderr.length, 16384);
  assert.equal(result.recordStdoutTruncated, true);
  assert.equal(result.recordStderrTruncated, true);
});

test('report failures preserve exit, signal, timing and bounded diagnostics', async t => {
  for (const timedOut of [false, true]) {
    const outputPrefix = temporary(t);
    const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0, durationMs: 5 }, {
      platform: 'linux', readStart: () => 10,
      spawnRecord: () => fakeRecorder(outputPrefix + '.perf.data'),
      async runReport(executable, args, options) {
        assert.equal(options.timeout, 5000);
        if (timedOut) {
          // Exercise Node's actual execFile timeout error shape, without perf
          // or a native host. The owned child is terminated by execFile.
          const { execFile } = require('node:child_process');
          const { promisify } = require('node:util');
          return promisify(execFile)(process.execPath,
            ['-e', 'setInterval(() => {}, 1000)'], { timeout: 40 });
        }
        throw Object.assign(new Error('report failed'), {
          code: 2, signal: null, killed: false,
          stdout: '# Samples: 12\n' + 'x'.repeat(20000),
          stderr: 'y'.repeat(20000) + 'report terminal error',
        });
      },
    });
    const result = await profile.completion;
    assert.equal(result.reason, 'perf-report-failed');
    assert.equal(result.available, false);
    assert.equal(result.reportTimeoutMs, 5000);
    assert.ok(result.reportStartedAtMs >= result.recordStoppedAtMs);
    assert.ok(result.reportFinishedAtMs >= result.reportStartedAtMs);
    assert.equal(result.reportDurationMs, result.reportFinishedAtMs - result.reportStartedAtMs);
    assert.equal(result.reportKilled, timedOut);
    assert.equal(result.reportSignal, timedOut ? 'SIGTERM' : null);
    assert.equal(result.reportCode, timedOut ? null : 2);
    if (!timedOut) {
      assert.equal(result.reportStdout.length, 16384);
      assert.equal(result.reportStderr.length, 16384);
      assert.equal(result.reportStderrTail.length, 16384);
      assert.ok(result.reportStderrTail.endsWith('report terminal error'));
      assert.equal(result.reportStdoutTruncated, true);
      assert.equal(result.reportStderrTruncated, true);
    }
    assert.deepEqual(JSON.parse(fs.readFileSync(outputPrefix + '.json', 'utf8')), result);
    assert.equal(fs.existsSync(outputPrefix + '.report.txt'), false);
  }
});

test('report failure Unicode diagnostics and null-code summary obey byte limits', async t => {
  const outputPrefix = temporary(t);
  const long = '😀€'.repeat(10000);
  const profile = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0, durationMs: 5 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord: () => fakeRecorder(outputPrefix + '.perf.data'),
    async runReport() {
      throw Object.assign(new Error(long), {
        code: null, signal: 'SIGTERM', killed: true,
        stdout: long, stderr: long + 'terminal-😀',
      });
    },
  });
  const result = await profile.completion;
  for (const key of ['error', 'reportStdout', 'reportStderr', 'reportStderrTail']) {
    assert.ok(Buffer.byteLength(result[key]) <= 16384, key);
    assert.ok(!result[key].includes('\uFFFD'), key + ' preserves complete code points');
  }
  assert.equal(result.reportCode, null);
  assert.equal(result.reportStdoutTruncated, true);
  assert.equal(result.reportStderrTruncated, true);
  assert.ok(result.reportStderrTail.endsWith('terminal-😀'));
  assert.equal(result.available, false);
});

test('filesystem failures have distinct phases and final metadata failure still settles', { timeout: 2000 }, async t => {
  for (const phase of ['data-stat', 'before-report', 'report-file', 'final-metadata']) {
    const native = child(), outputPrefix = temporary(t);
    let reportCalls = 0;
    const profile = startNativeSymbolProfile(native, { outputPrefix, delayMs: 0, durationMs: 5 }, {
      platform: 'linux', readStart: () => 10,
      spawnRecord() {
        const recorder = fakeRecorder(outputPrefix + '.perf.data');
        if (phase === 'data-stat') {
          const kill = recorder.kill;
          recorder.kill = signal => {
            const result = kill(signal);
            fs.unlinkSync(outputPrefix + '.perf.data');
            return result;
          };
        }
        return recorder;
      },
      writeFile(file, value) {
        const metadata = file.endsWith('.json') ? JSON.parse(value) : null;
        if ((phase === 'report-file' && file.endsWith('.report.txt'))
          || (phase === 'before-report' && metadata?.reportTimeoutMs && metadata.state !== 'complete')
          || (phase === 'final-metadata' && metadata?.state === 'complete')) {
          throw Object.assign(new Error('fixture disk full'), { code: 'ENOSPC' });
        }
        fs.writeFileSync(file, value);
      },
      async runReport() {
        reportCalls++;
        return { stdout: '# Samples: 12 of event cpu-clock:u\n' };
      },
    });
    const result = await profile.completion;
    assert.equal(result.available, false);
    assert.equal(result.state, 'complete');
    assert.equal(result.reportCode, undefined, 'filesystem errors are not subprocess exits');
    assert.equal(result.reportKilled, undefined);
    assert.equal(native.listenerCount('exit'), 0);
    if (phase === 'data-stat' || phase === 'before-report') {
      assert.equal(reportCalls, 0);
      assert.equal(result.reportStartedAtMs, undefined);
      assert.equal(result.failurePhase, phase);
      assert.equal(result.reason, phase === 'data-stat' ? 'profile-data-read-failed' : 'profile-metadata-write-failed');
    } else {
      assert.equal(reportCalls, 1);
      assert.ok(result.reportFinishedAtMs >= result.reportStartedAtMs);
      assert.equal(result.reportDurationMs, result.reportFinishedAtMs - result.reportStartedAtMs);
      if (phase === 'report-file') {
        assert.equal(result.reason, 'profile-report-write-failed');
        assert.equal(result.failurePhase, phase);
      } else {
        assert.equal(result.reason, 'profile-metadata-write-failed');
        assert.equal(result.completionReason, 'sampled');
        assert.equal(result.metadataWriteError, 'ENOSPC');
      }
    }
    assert.equal((await profile.stop()).reason, result.reason);
  }
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


test('verbose recorder diagnostics retain a bounded terminal error without accepting failure', async t => {
  const outputPrefix = temporary(t);
  let reports = 0;
  const { completion } = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0 }, {
    platform: 'linux', readStart: () => 10,
    spawnRecord(executable, args) {
      assert.ok(args.includes('--verbose'));
      const recorder = fakeRecorder(outputPrefix + '.perf.data');
      setImmediate(() => {
        recorder.stderr.write('x'.repeat(40000));
        recorder.stderr.write("couldn't open /proc/4343/status\n");
        recorder.exitCode = 255;
        recorder.emit('close', 255, null);
      });
      return recorder;
    },
    async runReport() { reports++; throw new Error('failed recording must not be accepted'); },
  });
  const result = await completion;
  assert.equal(result.available, false);
  assert.equal(result.reason, 'perf-record-failed');
  assert.equal(result.recordCode, 255);
  assert.equal(result.recordStderr.length, 16384);
  assert.equal(result.recordStderrTail.length, 16384);
  assert.ok(result.recordStderrTail.endsWith("couldn't open /proc/4343/status\n"));
  assert.equal(result.recordStderrTruncated, true);
  assert.equal(reports, 0);
});

test('one evidenced startup thread exit recovers within the original time and byte budgets', async t => {
  const native = child(), outputPrefix = temporary(t);
  let spawned = 0, reports = 0;
  const profile = startNativeSymbolProfile(native, { outputPrefix, delayMs: 0, durationMs: 120 }, {
    platform: 'linux', readStart: () => 10,
    readThreads: () => [{ tid: 4343, startedTicks: 11 }],
    readThreadStart(pid, tid) {
      assert.equal(pid, 4242); assert.equal(tid, 4343);
      throw Object.assign(new Error('gone'), { code: 'ENOENT' });
    },
    spawnRecord(executable, args) {
      spawned++;
      const output = args[args.indexOf('--output') + 1];
      const recorder = fakeRecorder(output, { exitSignal: 'SIGINT' });
      if (spawned === 1) setTimeout(() => {
        fs.writeFileSync(output, 'partial');
        recorder.stderr.write("perf_event__synthesize_bpf_events: can't get next program: Operation not permitted\n");
        recorder.stderr.write("couldn't open /proc/4343/status\n");
        recorder.exitCode = 255; recorder.emit('close', 255, null);
      }, 35);
      else {
        assert.equal(output, outputPrefix + '.retry-1.perf.data');
        assert.equal(args[args.indexOf('--max-size') + 1], String(32 * 1024 * 1024 - 7));
        assert.equal(args[args.indexOf('--pid') + 1], '4242');
      }
      return recorder;
    },
    async runReport(executable, args) {
      reports++;
      assert.equal(args.at(-1), outputPrefix + '.retry-1.perf.data');
      return { stdout: '# Samples: 9 of event cpu-clock:u\nctox::work\n' };
    },
  });
  const result = await profile.completion;
  assert.equal(result.available, true); assert.equal(spawned, 2); assert.equal(reports, 1);
  assert.equal(result.recordingDeadlineAtMs, result.startedAtMs + 120);
  assert.ok(result.attempts[1].recordingBudgetMs < 100, 'recovery must consume the original budget');
  assert.equal(result.attempts[0].code, 255);
  assert.equal(result.attempts[0].recovery.tid, 4343);
  assert.equal(result.attempts[0].recovery.threadStartedTicks, 11);
  assert.match(result.attempts[0].stderr, /couldn't open/);
  assert.equal(result.attempts[1].controlledInterrupt, true);
  assert.equal(result.dataFile, 'native.retry-1.perf.data');
  assert.equal(fs.readFileSync(outputPrefix + '.perf.data', 'utf8'), 'partial');
  assert.deepEqual(JSON.parse(fs.readFileSync(outputPrefix + '.json', 'utf8')), result);
  assert.equal(native.listenerCount('exit'), 0);
});

test('thread recovery rejects missing ownership, live or reused tasks, identity changes and inspection errors', async t => {
  for (const cause of ['unknown-thread', 'leader', 'live-thread', 'reused-thread', 'permission', 'native-reused', 'native-exited', 'wrong-exit', 'wrong-terminal', 'perf-permission', 'truncated', 'snapshot-error']) {
    const native = child(), outputPrefix = temporary(t);
    let failed = false, spawned = 0;
    const tid = cause === 'leader' ? 4242 : 4343;
    const profile = startNativeSymbolProfile(native, { outputPrefix, delayMs: 0, durationMs: 100 }, {
      platform: 'linux', readStart: () => failed && cause === 'native-reused' ? 20 : 10,
      readThreads() {
        if (cause === 'snapshot-error') throw Object.assign(new Error('denied'), { code: 'EACCES' });
        return cause === 'unknown-thread' ? [] : [{ tid, startedTicks: 11 }];
      },
      readThreadStart() {
        if (cause === 'live-thread') return 11;
        if (cause === 'reused-thread') return 22;
        throw Object.assign(new Error('inspection failed'), { code: cause === 'permission' ? 'EACCES' : 'ENOENT' });
      },
      spawnRecord(executable, args) {
        spawned++;
        const recorder = fakeRecorder(args.at(-1));
        setImmediate(() => {
          failed = true;
          if (cause === 'native-exited') native.exitCode = 0;
          if (cause === 'perf-permission') recorder.stderr.write('perf_event_open failed: Permission denied\n');
          if (cause === 'truncated') recorder.stderr.write('x'.repeat(20000) + '\n');
          recorder.stderr.write(`couldn't open /proc/${tid}/status\n`);
          if (cause === 'wrong-terminal') recorder.stderr.write('perf_event_open failed: Permission denied\n');
          const code = cause === 'wrong-exit' ? 1 : 255;
          recorder.exitCode = code; recorder.emit('close', code, null);
        });
        return recorder;
      },
      async runReport() { assert.fail('failed recording cannot become valid'); },
    });
    const result = await profile.completion;
    assert.equal(result.available, false, cause);
    assert.equal(result.reason, 'perf-record-failed', cause);
    assert.equal(spawned, 1, cause);
    assert.equal(result.attempts.length, 1, cause);
  }
});

test('a second vanished thread is terminal and preserves both failed attempts', async t => {
  const outputPrefix = temporary(t);
  let spawned = 0;
  const { completion } = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0, durationMs: 100 }, {
    platform: 'linux', readStart: () => 10,
    readThreads: () => [{ tid: 4343, startedTicks: 11 }],
    readThreadStart() { throw Object.assign(new Error('gone'), { code: 'ENOENT' }); },
    spawnRecord(executable, args) {
      spawned++;
      const recorder = fakeRecorder(args.at(-1));
      setImmediate(() => {
        fs.writeFileSync(args.at(-1), 'partial-' + spawned);
        recorder.stderr.write("couldn't open /proc/4343/status\n");
        recorder.exitCode = 255; recorder.emit('close', 255, null);
      });
      return recorder;
    },
    async runReport() { assert.fail('no valid recording'); },
  });
  const result = await completion;
  assert.equal(result.reason, 'perf-record-failed');
  assert.equal(result.available, false); assert.equal(spawned, 2);
  assert.deepEqual(result.attempts.map(attempt => attempt.code), [255, 255]);
  assert.equal(fs.readFileSync(outputPrefix + '.perf.data', 'utf8'), 'partial-1');
  assert.equal(fs.readFileSync(outputPrefix + '.retry-1.perf.data', 'utf8'), 'partial-2');
});

test('a requested stop cannot trigger thread-exit recovery', async t => {
  const outputPrefix = temporary(t);
  let spawned = 0;
  const { completion } = startNativeSymbolProfile(child(), { outputPrefix, delayMs: 0, durationMs: 5 }, {
    platform: 'linux', readStart: () => 10,
    readThreads: () => [{ tid: 4343, startedTicks: 11 }],
    readThreadStart() { assert.fail('must not inspect for recovery after stopping'); },
    spawnRecord(executable, args) {
      spawned++;
      const recorder = fakeRecorder(args.at(-1), { exitCode: 255 });
      recorder.stderr.write("couldn't open /proc/4343/status\n");
      return recorder;
    },
    async runReport() { assert.fail('stopped failure is not a sample'); },
  });
  const result = await completion;
  assert.equal(result.reason, 'perf-record-failed');
  assert.equal(result.available, false); assert.equal(spawned, 1);
  assert.equal(result.stopReason, 'duration-limit');
});
