'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawn, execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { parseProcStat } = require('./native_cpu_profile.js');
const execFileAsync = promisify(execFile);

// Diagnostic fixture only: a flat user-space CPU sample of this owned child.
// No system-wide target, child inheritance, stack/memory dump or native signal.
function startNativeSymbolProfile(child, {
  outputPrefix, perfExecutable = 'perf', delayMs = 30000, durationMs = 30000,
} = {}, dependencies = {}) {
  const platform = dependencies.platform || process.platform;
  const readStart = dependencies.readStart || (pid =>
    parseProcStat(fs.readFileSync('/proc/' + pid + '/stat', 'utf8')).startedTicks);
  const spawnRecord = dependencies.spawnRecord || spawn;
  const signalRecord = dependencies.signalRecord || ((recorder, signal) => recorder.kill(signal));
  const runReport = dependencies.runReport || execFileAsync;
  const perfEnvironment = { ...process.env, PERF_CONFIG: '/dev/null', PERF_CONFIG_NOSYSTEM: '1', PERF_CONFIG_NOGLOBAL: '1' };
  if (!outputPrefix || !Number.isSafeInteger(child?.pid) || child.pid <= 0)
    throw new Error('symbol profile requires an owned child and output prefix');
  if (!Number.isFinite(delayMs) || delayMs < 0 || delayMs > 60000
    || !Number.isFinite(durationMs) || durationMs <= 0 || durationMs > 30000)
    throw new Error('symbol profile timing is out of bounds');
  fs.mkdirSync(path.dirname(outputPrefix), { recursive: true });
  const dataPath = outputPrefix + '.perf.data';
  const reportPath = outputPrefix + '.report.txt';
  const metadataPath = outputPrefix + '.json';
  const metadata = {
    schema: 'ctox.native_symbol_profile.v1', pid: child.pid, platform,
    requestedAtMs: Date.now(), delayMs, durationMs, frequencyHz: 49,
    event: 'cpu-clock:u', scope: 'existing-native-process-threads',
    inheritsChildTasks: false, capturesStacks: false, capturesUserMemory: false,
    acceptanceTiming: false, available: false, state: 'scheduled',
  };
  let timer, durationTimer, killTimer, recorder, finished = false, stopping = false;
  let interruptRequested = false;
  let recordError = '', stderr = '', stderrTruncated = false, startTicks;
  let stdout = '', stdoutTruncated = false;
  let resolveCompletion;
  const completion = new Promise(resolve => { resolveCompletion = resolve; });
  const write = () => fs.writeFileSync(metadataPath, JSON.stringify(metadata, null, 2) + '\n');
  const finish = (reason, details = {}) => {
    if (finished) return;
    finished = true;
    clearTimeout(timer); clearTimeout(durationTimer); clearTimeout(killTimer);
    child.removeListener('exit', childExited);
    Object.assign(metadata, details, { state: 'complete', reason, finishedAtMs: Date.now() });
    write();
    resolveCompletion(metadata);
  };
  const stop = (reason = 'requested-stop') => {
    if (finished || stopping) return completion;
    stopping = true;
    metadata.stopReason = reason;
    clearTimeout(timer); clearTimeout(durationTimer);
    if (!recorder) { finish(reason); return completion; }
    if (recorder.exitCode === null && recorder.signalCode === null) {
      interruptRequested = signalRecord(recorder, 'SIGINT', reason) !== false;
      killTimer = setTimeout(() => {
        if (recorder.exitCode === null && recorder.signalCode === null) signalRecord(recorder, 'SIGKILL', 'interrupt-grace-expired');
      }, 3000);
    }
    return completion;
  };
  const childExited = () => { void stop('native-child-exited'); };
  child.once('exit', childExited);
  write();
  if (platform !== 'linux') {
    finish('linux-perf-required');
    return { stop, completion };
  }
  try { startTicks = readStart(child.pid); metadata.processStartedTicks = startTicks; }
  catch { finish('native-identity-unavailable'); return { stop, completion }; }

  timer = setTimeout(() => {
    if (finished || stopping) return;
    try {
      if (child.exitCode !== null || child.signalCode !== null || readStart(child.pid) !== startTicks) {
        finish('native-identity-changed'); return;
      }
      const args = ['record', '--event', 'cpu-clock:u', '--freq', '49',
        '--no-inherit', '--pid', String(child.pid), '--mmap-pages', '128',
        '--no-buildid-cache', '--max-size', '32M', '--output', dataPath];
      metadata.recordArgs = args;
      metadata.perfExecutable = perfExecutable;
      metadata.startedAtMs = Date.now(); metadata.state = 'recording'; write();
      recorder = spawnRecord(perfExecutable, args, { stdio: ['ignore', 'pipe', 'pipe'], env: perfEnvironment });
      recorder.stdout.on('data', chunk => {
        const text = chunk.toString();
        const room = Math.max(0, 16384 - stdout.length);
        stdout += text.slice(0, room);
        if (text.length > room) stdoutTruncated = true;
      });
      recorder.stderr.on('data', chunk => {
        const text = chunk.toString();
        const room = Math.max(0, 16384 - stderr.length);
        stderr += text.slice(0, room);
        if (text.length > room) stderrTruncated = true;
      });
      recorder.once('error', error => { recordError = error.code || error.message; });
      recorder.once('close', async (code, signal) => {
        clearTimeout(durationTimer); clearTimeout(killTimer);
        metadata.recordCode = code; metadata.recordSignal = signal;
        metadata.recordStdout = stdout; metadata.recordStdoutTruncated = stdoutTruncated;
        metadata.recordStderr = stderr; metadata.recordStderrTruncated = stderrTruncated;
        metadata.recordStoppedAtMs = Date.now();
        // Linux perf re-raises SIGINT after flushing a controlled recording.
        // Accept only our requested interrupt, then still validate the data and
        // require a successfully parsed report containing actual samples.
        const controlledInterrupt = interruptRequested && code === null && signal === 'SIGINT';
        metadata.controlledInterrupt = controlledInterrupt;
        if (recordError || (code !== 0 && !controlledInterrupt)) {
          finish('perf-record-failed', { error: recordError || 'exit-' + code }); return;
        }
        try {
          const size = fs.statSync(dataPath).size;
          if (size === 0 || size > 32 * 1024 * 1024) {
            finish('profile-size-out-of-bounds', { bytes: size }); return;
          }
          const report = await runReport(perfExecutable,
            ['report', '--stdio', '--no-children', '--percent-limit', '0.5', '--input', dataPath],
            { encoding: 'utf8', timeout: 5000, maxBuffer: 2 * 1024 * 1024, env: perfEnvironment });
          fs.writeFileSync(reportPath, report.stdout);
          // A readable report without samples is not a successful CPU profile.
          const samples = /# Samples:\s+([\d.,]+[KMG]?)/i.exec(report.stdout)?.[1] || null;
          const hasSamples = samples !== null && Number.parseFloat(samples.replaceAll(',', '')) > 0;
          finish(hasSamples ? 'sampled' : 'no-samples', {
            available: hasSamples, bytes: size, reportedSamples: samples,
            reportFile: path.basename(reportPath), dataFile: path.basename(dataPath),
          });
        } catch (error) {
          finish('perf-report-failed', { error: error.code || error.message });
        }
      });
      durationTimer = setTimeout(() => { void stop('duration-limit'); }, durationMs);
    } catch (error) { finish('profile-start-failed', { error: error.code || error.message }); }
  }, delayMs);
  return { stop, completion };
}

module.exports = { startNativeSymbolProfile };
