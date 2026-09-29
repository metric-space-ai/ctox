'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawn, execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { parseProcStat } = require('./native_cpu_profile.js');
const execFileAsync = promisify(execFile);
const REPORT_TIMEOUT_MS = 20000;

function boundedDiagnostic(value, tail = false) {
  const bytes = Buffer.from(String(value ?? ''), 'utf8');
  const limit = 16384;
  if (bytes.length <= limit) return bytes.toString('utf8');
  if (tail) {
    let start = bytes.length - limit;
    while ((bytes[start] & 0xc0) === 0x80) start++;
    return bytes.subarray(start).toString('utf8');
  }
  let end = limit;
  while ((bytes[end] & 0xc0) === 0x80) end--;
  return bytes.subarray(0, end).toString('utf8');
}

function diagnosticError(error) {
  return boundedDiagnostic(error.code ?? error.message ?? String(error));
}

// Diagnostic fixture only: a flat user-space CPU sample of this owned child.
// No system-wide target, child inheritance, stack/memory dump or native signal.
function startNativeSymbolProfile(child, {
  outputPrefix, perfExecutable = 'perf', delayMs = 30000, durationMs = 30000,
} = {}, dependencies = {}) {
  const platform = dependencies.platform || process.platform;
  const readStart = dependencies.readStart || (pid =>
    parseProcStat(fs.readFileSync('/proc/' + pid + '/stat', 'utf8')).startedTicks);
  const readThreadStart = dependencies.readThreadStart || ((pid, tid) =>
    parseProcStat(fs.readFileSync(`/proc/${pid}/task/${tid}/stat`, 'utf8')).startedTicks);
  const readThreads = dependencies.readThreads || (pid => {
    const threads = [];
    for (const name of fs.readdirSync(`/proc/${pid}/task`).slice(0, 512)) {
      if (!/^\d+$/.test(name)) continue;
      const tid = Number(name);
      try { threads.push({ tid, startedTicks: readThreadStart(pid, tid) }); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
    }
    return threads;
  });
  const spawnRecord = dependencies.spawnRecord || spawn;
  const signalRecord = dependencies.signalRecord || ((recorder, signal) => recorder.kill(signal));
  const runReport = dependencies.runReport || execFileAsync;
  const writeFile = dependencies.writeFile || fs.writeFileSync;
  const perfEnvironment = { ...process.env, PERF_CONFIG: '/dev/null', PERF_CONFIG_NOSYSTEM: '1', PERF_CONFIG_NOGLOBAL: '1' };
  if (!outputPrefix || !Number.isSafeInteger(child?.pid) || child.pid <= 0)
    throw new Error('symbol profile requires an owned child and output prefix');
  if (!Number.isFinite(delayMs) || delayMs < 0 || delayMs > 60000
    || !Number.isFinite(durationMs) || durationMs <= 0 || durationMs > 30000)
    throw new Error('symbol profile timing is out of bounds');
  fs.mkdirSync(path.dirname(outputPrefix), { recursive: true });
  let dataPath = outputPrefix + '.perf.data';
  const maxBytes = 32 * 1024 * 1024;
  let retainedBytes = 0, recordingDeadline, recordingStarted;
  const reportPath = outputPrefix + '.report.txt';
  const metadataPath = outputPrefix + '.json';
  const metadata = {
    schema: 'ctox.native_symbol_profile.v1', pid: child.pid, platform,
    requestedAtMs: Date.now(), delayMs, durationMs, frequencyHz: 49,
    event: 'cpu-clock:u', scope: 'existing-native-process-threads',
    inheritsChildTasks: false, capturesStacks: false, capturesUserMemory: false,
    acceptanceTiming: false, available: false, state: 'scheduled',
    attempts: [], maxRecordAttempts: 3,
  };
  let timer, durationTimer, killTimer, recorder, finished = false, stopping = false;
  let interruptRequested = false;
  let recordError = '', stderr = '', stderrTail = '', stderrTruncated = false, startTicks;
  let stdout = '', stdoutTruncated = false;
  let resolveCompletion;
  const completion = new Promise(resolve => { resolveCompletion = resolve; });
  const write = () => writeFile(metadataPath, JSON.stringify(metadata, null, 2) + '\n');
  const finish = (reason, details = {}) => {
    if (finished) return;
    finished = true;
    clearTimeout(timer); clearTimeout(durationTimer); clearTimeout(killTimer);
    child.removeListener('exit', childExited);
    Object.assign(metadata, details, { state: 'complete', reason, finishedAtMs: Date.now() });
    try { write(); }
    catch (error) {
      metadata.completionReason = reason;
      metadata.reason = 'profile-metadata-write-failed';
      metadata.available = false;
      metadata.metadataWriteError = diagnosticError(error);
    } finally { resolveCompletion(metadata); }
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

  const attach = () => {
    if (finished || stopping) return;
    try {
      if (child.exitCode !== null || child.signalCode !== null || readStart(child.pid) !== startTicks) {
        finish('native-identity-changed'); return;
      }
      if (recordingDeadline !== undefined && performance.now() >= recordingDeadline) {
        finish('perf-record-failed', { error: 'recording-budget-exhausted' }); return;
      }
      const attempt = { number: metadata.attempts.length + 1, dataFile: path.basename(dataPath) };
      // Failure to observe a thread disables recovery; it does not silently
      // expand the sample to unrelated tasks or suppress a perf failure.
      try { attempt.threadsBefore = readThreads(child.pid); }
      catch (error) { attempt.threadSnapshotError = diagnosticError(error); }
      if (readStart(child.pid) !== startTicks || child.exitCode !== null || child.signalCode !== null) {
        finish('native-identity-changed'); return;
      }
      if (recordingDeadline === undefined) {
        recordingStarted = performance.now();
        recordingDeadline = recordingStarted + durationMs;
        metadata.startedAtMs = Date.now();
        metadata.recordingDeadlineAtMs = metadata.startedAtMs + durationMs;
      }
      const remainingMs = recordingDeadline - performance.now();
      if (remainingMs <= 0) { finish('perf-record-failed', { error: 'recording-budget-exhausted' }); return; }
      metadata.attempts.push(attempt);
      attempt.startedAtMs = Date.now();
      attempt.recordingBudgetMs = remainingMs;
      interruptRequested = false;
      recordError = ''; stderr = ''; stderrTail = ''; stderrTruncated = false;
      stdout = ''; stdoutTruncated = false;
      const args = ['record', '--verbose', '--event', 'cpu-clock:u', '--freq', '49',
        '--no-inherit', '--pid', String(child.pid), '--mmap-pages', '128',
        '--no-buildid-cache', '--max-size', retainedBytes ? `${maxBytes - retainedBytes}B` : '32M', '--output', dataPath];
      metadata.recordArgs = args;
      attempt.recordArgs = args;
      metadata.perfExecutable = perfExecutable;
      metadata.state = 'recording'; write();
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
        stderrTail = (stderrTail + text).slice(-16384);
        if (text.length > room) stderrTruncated = true;
      });
      recorder.once('error', error => { recordError = error.code || error.message; });
      recorder.once('close', async (code, signal) => {
        clearTimeout(durationTimer); clearTimeout(killTimer);
        metadata.recordCode = code; metadata.recordSignal = signal;
        metadata.recordStdout = stdout; metadata.recordStdoutTruncated = stdoutTruncated;
        metadata.recordStderr = stderr; metadata.recordStderrTruncated = stderrTruncated;
        metadata.recordStderrTail = stderrTruncated ? stderrTail : ''; // retain terminal debug failures too
        metadata.recordStoppedAtMs = Date.now();
        // Linux perf re-raises SIGINT after flushing a controlled recording.
        // Accept only our requested interrupt, then still validate the data and
        // require a successfully parsed report containing actual samples.
        const controlledInterrupt = interruptRequested && code === null && signal === 'SIGINT';
        metadata.controlledInterrupt = controlledInterrupt;
        Object.assign(attempt, {
          stoppedAtMs: metadata.recordStoppedAtMs, code, signal, controlledInterrupt,
          stdout, stdoutTruncated, stderr, stderrTruncated,
          stderrTail: stderrTruncated ? stderrTail : '',
        });
        if (recordError || (code !== 0 && !controlledInterrupt)) {
          // Linux perf 6.8 can abort initial metadata synthesis when a thread
          // in its task map exits. Recover at most twice, each time with fresh
          // before/after ownership evidence and the original recording budget.
          const missing = /^couldn't open \/proc\/(\d+)\/status\s*$/.exec(stderrTail.trim().split('\n').at(-1));
          const tid = missing ? Number(missing[1]) : null;
          const known = attempt.threadsBefore?.find(thread => thread.tid === tid
            && Number.isSafeInteger(thread.startedTicks) && thread.startedTicks >= 0);
          const permissionFailure = stderr.split('\n').some(line => /Permission denied|Operation not permitted/i.test(line)
            && !line.startsWith("perf_event__synthesize_bpf_events: can't get next program: Operation not permitted"));
          if (attempt.number < metadata.maxRecordAttempts && !recordError && code === 255 && signal === null
            && !stderrTruncated && !permissionFailure
            && !stopping && !interruptRequested && known && tid !== child.pid
            && performance.now() - recordingStarted <= 5000 && performance.now() < recordingDeadline) {
            try {
              let vanished = false;
              try { readThreadStart(child.pid, tid); }
              catch (error) { if (error.code === 'ENOENT') vanished = true; else throw error; }
              if (vanished && child.exitCode === null && child.signalCode === null && readStart(child.pid) === startTicks) {
                let bytes = 0;
                try { bytes = fs.statSync(dataPath).size; }
                catch (error) { if (error.code !== 'ENOENT') throw error; }
                if (retainedBytes + bytes < maxBytes) {
                  retainedBytes += bytes;
                  attempt.recovery = { reason: 'owned-thread-exited-during-perf-start',
                    tid, threadStartedTicks: known.startedTicks, processStartedTicks: startTicks,
                    attemptBytes: bytes, retainedBytes };
                  dataPath = outputPrefix + `.retry-${attempt.number}.perf.data`;
                  recorder = null;
                  metadata.state = 'recovering'; write();
                  timer = setTimeout(attach, 0);
                  return;
                }
              }
            } catch (error) { attempt.recoveryCheckError = diagnosticError(error); }
          }
          finish('perf-record-failed', { error: recordError || 'exit-' + code }); return;
        }
        let size;
        try { size = fs.statSync(dataPath).size; }
        catch (error) {
          finish('profile-data-read-failed', { error: diagnosticError(error), failurePhase: 'data-stat' }); return;
        }
        if (size === 0 || size + retainedBytes > maxBytes) {
          finish('profile-size-out-of-bounds', { bytes: size, retainedBytes }); return;
        }
        metadata.reportTimeoutMs = REPORT_TIMEOUT_MS;
        try { write(); }
        catch (error) {
          finish('profile-metadata-write-failed', { error: diagnosticError(error), failurePhase: 'before-report' }); return;
        }
        metadata.reportStartedAtMs = Date.now();
        let report;
        try {
          report = await runReport(perfExecutable,
            ['report', '--stdio', '--no-children', '--percent-limit', '0.5', '--input', dataPath],
            { encoding: 'utf8', timeout: metadata.reportTimeoutMs, maxBuffer: 2 * 1024 * 1024, env: perfEnvironment });
        } catch (error) {
          // Preserve the subprocess result instead of collapsing a timeout,
          // signal or report exit into the same unstructured command message.
          // Partial output is diagnostic only and can never establish samples.
          const reportStdout = String(error.stdout || '');
          const reportStderr = String(error.stderr || '');
          const reportFinishedAtMs = Date.now();
          finish('perf-report-failed', {
            error: diagnosticError(error), failurePhase: 'report-process',
            reportCode: error.code ?? null,
            reportSignal: error.signal ?? null,
            reportKilled: error.killed === true,
            reportFinishedAtMs,
            reportDurationMs: metadata.reportStartedAtMs === undefined ? null
              : reportFinishedAtMs - metadata.reportStartedAtMs,
            reportStdout: boundedDiagnostic(reportStdout),
            reportStdoutTruncated: Buffer.byteLength(reportStdout) > 16384,
            reportStderr: boundedDiagnostic(reportStderr),
            reportStderrTruncated: Buffer.byteLength(reportStderr) > 16384,
            reportStderrTail: Buffer.byteLength(reportStderr) > 16384 ? boundedDiagnostic(reportStderr, true) : '',
          });
          return;
        }
        metadata.reportFinishedAtMs = Date.now();
        metadata.reportDurationMs = metadata.reportFinishedAtMs - metadata.reportStartedAtMs;
        try { writeFile(reportPath, report.stdout); }
        catch (error) {
          finish('profile-report-write-failed', { error: diagnosticError(error), failurePhase: 'report-file' }); return;
        }
        // A readable report without samples is not a successful CPU profile.
        const samples = /# Samples:\s+([\d.,]+[KMG]?)/i.exec(report.stdout)?.[1] || null;
        const hasSamples = samples !== null && Number.parseFloat(samples.replaceAll(',', '')) > 0;
        finish(hasSamples ? 'sampled' : 'no-samples', {
          available: hasSamples, bytes: size, retainedBytes, reportedSamples: samples,
          reportFile: path.basename(reportPath), dataFile: path.basename(dataPath),
        });
      });
      durationTimer = setTimeout(() => { void stop('duration-limit'); }, Math.max(0, recordingDeadline - performance.now()));
    } catch (error) { finish('profile-start-failed', { error: error.code || error.message }); }
  };
  timer = setTimeout(attach, delayMs);
  return { stop, completion };
}

module.exports = { startNativeSymbolProfile };
