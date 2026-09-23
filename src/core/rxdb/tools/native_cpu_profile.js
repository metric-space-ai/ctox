'use strict';

const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

function parseProcStat(text) {
  const end = text.lastIndexOf(')');
  const start = text.indexOf('(');
  if (start < 0 || end <= start) throw new Error('invalid proc stat name');
  const fields = text.slice(end + 1).trim().split(/\s+/);
  const userTicks = Number(fields[11]), systemTicks = Number(fields[12]), startedTicks = Number(fields[19]);
  if (![userTicks, systemTicks, startedTicks].every(Number.isSafeInteger)
    || [userTicks, systemTicks, startedTicks].some(value => value < 0)) throw new Error('invalid proc stat counters');
  return { name: text.slice(start + 1, end), ticks: userTicks + systemTicks, startedTicks };
}

function cpuDelta(previous, current, elapsedMs, ticksPerSecond) {
  if (!previous || current.startedTicks !== previous.startedTicks
    || current.ticks < previous.ticks || elapsedMs <= 0 || ticksPerSecond <= 0) return null;
  const cpuMs = (current.ticks - previous.ticks) * 1000 / ticksPerSecond;
  return { cpuMs, percentOfOneCore: cpuMs / elapsedMs * 100 };
}

// Test-harness observation only. Never discovers or signals unrelated processes.
function startNativeCpuProfile(child, { outputPath, phase = () => 'unknown', intervalMs = 1000, maxSamples = 900, maxThreads = 512 } = {}) {
  if (!outputPath || !child?.pid) return () => {};
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  const write = value => fs.appendFileSync(outputPath, JSON.stringify(value) + '\n');
  const clock = () => Number(process.hrtime.bigint()) / 1e6;
  const started = clock();
  let observerMs = 0, samples = 0, stopped = false, timer;
  const header = { schema: 'ctox.native_cpu_profile.v1', kind: 'header', pid: child.pid, platform: process.platform,
    startedAtMs: Date.now(), intervalMs, maxSamples, maxThreads, cpuPercentBasis: 'one-core',
    scope: 'native-process-and-its-threads-only', capturesStacks: false, childProcessesIncluded: false };
  if (process.platform !== 'linux') {
    write({ ...header, available: false, reason: 'procfs-requires-linux' });
    return () => {};
  }
  const hz = spawnSync('getconf', ['CLK_TCK'], { encoding: 'utf8', timeout: 2000 });
  const ticksPerSecond = Number(String(hz.stdout || '').trim());
  if (hz.status !== 0 || !Number.isSafeInteger(ticksPerSecond) || ticksPerSecond <= 0) {
    write({ ...header, available: false, reason: 'clock-tick-rate-unavailable' });
    return () => {};
  }
  write({ ...header, available: true, ticksPerSecond });
  observerMs = clock() - started;
  const processPath = '/proc/' + child.pid;
  const readStat = filename => parseProcStat(fs.readFileSync(filename, 'utf8'));
  let previous = null, previousThreads = new Map(), previousAt = clock(), processStartedTicks = null;
  const stop = (reason = 'stopped') => {
    if (stopped) return;
    stopped = true;
    clearInterval(timer);
    child.removeListener('exit', exited);
    write({ kind: 'summary', reason, pid: child.pid, samples, elapsedMs: clock() - started,
      observerMs, lastSampleGapMs: clock() - previousAt });
  };
  const sample = () => {
    if (stopped) return;
    const sampleStarted = clock();
    let stopReason = null;
    try {
      const current = readStat(processPath + '/stat');
      if (processStartedTicks !== null && current.startedTicks !== processStartedTicks) {
        stopReason = 'pid-reused'; return;
      }
      processStartedTicks = current.startedTicks;
      const at = clock(), elapsedMs = at - previousAt;
      const ids = fs.readdirSync(processPath + '/task').filter(id => /^\d+$/.test(id));
      const nextThreads = new Map(), threads = [];
      let vanishedThreads = 0, threadReadErrors = 0;
      for (const id of ids.slice(0, maxThreads)) {
        try {
          const stat = readStat(processPath + '/task/' + id + '/stat');
          nextThreads.set(id, stat);
          const delta = cpuDelta(previousThreads.get(id), stat, elapsedMs, ticksPerSecond);
          threads.push({ tid: Number(id), name: stat.name, startedTicks: stat.startedTicks, ...delta,
            measuredInterval: delta !== null });
        } catch (error) {
          if (error.code === 'ENOENT' || error.code === 'ESRCH') vanishedThreads++;
          else threadReadErrors++;
        }
      }
      const total = cpuDelta(previous, current, elapsedMs, ticksPerSecond);
      write({ kind: 'sample', atMs: Date.now(), phase: phase(), elapsedMs, pid: child.pid, processStartedTicks,
        total, threads, observedThreadCount: ids.length, omittedThreads: Math.max(0, ids.length - maxThreads),
        vanishedThreads, threadReadErrors, collectionMs: clock() - sampleStarted });
      previous = current; previousThreads = nextThreads; previousAt = at; samples++;
      if (samples >= maxSamples) stopReason = 'sample-limit';
    } catch (error) {
      write({ kind: 'unavailable', atMs: Date.now(), code: error.code || error.message });
      stopReason = error.code === 'ENOENT' || error.code === 'ESRCH' ? 'process-exited' : 'sample-failed';
    } finally {
      observerMs += clock() - sampleStarted;
      if (stopReason) stop(stopReason);
    }
  };
  const exited = () => stop('child-exited');
  child.once('exit', exited);
  sample();
  if (!stopped) { timer = setInterval(sample, intervalMs); timer.unref(); }
  return stop;
}

module.exports = { parseProcStat, cpuDelta, startNativeCpuProfile };
