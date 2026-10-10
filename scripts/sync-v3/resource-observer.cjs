const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const { parseProcStat, cpuDelta } = require('../../src/core/rxdb/tools/native_cpu_profile.js');
const peerName = 'business-os-rxdb-peer'.slice(0, 15);
function cpuSnapshot(pid) {
  const base = `/proc/${pid}`;
  const process = parseProcStat(fs.readFileSync(`${base}/stat`, 'utf8'));
  const ids = fs.readdirSync(`${base}/task`);
  if (ids.length > 512) throw Error('CPU thread observation bound exceeded');
  const threads = [];
  for (const tid of ids) {
    try {
      const stat = parseProcStat(fs.readFileSync(`${base}/task/${tid}/stat`, 'utf8'));
      if (stat.name === peerName) threads.push({ tid: Number(tid), ...stat });
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  if (!threads.length) throw Error('Actual native peer threads missing');
  return { pid, process, threads, atMs: Number(globalThis.performance.now()) };
}
function cpuInterval(a, b, hz) {
  const elapsedMs = b.atMs - a.atMs;
  if (a.pid !== b.pid) throw Error('CPU process identity changed');
  const total = cpuDelta(a.process, b.process, elapsedMs, hz);
  if (!total) throw Error('CPU process counter reset or identity changed');
  const old = new Map(a.threads.map(t => [`${t.tid}:${t.startedTicks}`, t]));
  const oldIds = new Map(a.threads.map(t => [t.tid, t]));
  const stable = b.threads.filter(t => old.has(`${t.tid}:${t.startedTicks}`));
  if (b.threads.some(t => oldIds.has(t.tid) && oldIds.get(t.tid).startedTicks !== t.startedTicks)) throw Error('Native peer thread set identity replaced');
  const intervals = stable.map(t => cpuDelta(old.get(`${t.tid}:${t.startedTicks}`), t, elapsedMs, hz));
  if (intervals.some(x => !x)) throw Error('Native peer thread counter reset');
  const lower = intervals.reduce((n, x) => n + x.cpuMs, 0);
  const newThreads = b.threads.length - stable.length, vanishedThreads = a.threads.length - stable.length;
  const exact = newThreads === 0 && vanishedThreads === 0;
  // Unseen thread lifetimes cannot be reconstructed from two procfs snapshots.
  // Bound missing peer work by all process work; snapshots are not atomic.
  const upper = exact ? lower : Math.max(lower, total.cpuMs);
  return { elapsedMs, processCpuMs: total.cpuMs, peerCpuMs: exact ? lower : null,
    peerCpuLowerMs: lower, peerCpuUpperMs: upper, peerPercentOfOneCore: exact ? lower / elapsedMs * 100 : null,
    peerCoverage: exact ? 'stable-thread-set' : 'bounded-thread-churn', newThreads, vanishedThreads };
}
function summarize(samples, hz) {
  if (samples.length < 3) throw Error('Resource samples incomplete');
  const phases = samples.slice(1).map((sample, i) => ({ from: samples[i].name, to: sample.name,
    browserElapsedMs: sample.browserAt - samples[i].browserAt, ...cpuInterval(samples[i].cpu, sample.cpu, hz) }));
  const first = samples[0], last = samples.at(-1);
  return { ticksPerSecond: hz, samples, phases,
    growth: { observedMs: last.cpu.atMs - first.cpu.atMs,
      heapUsedStartBytes: first.heap.usedSize, heapUsedEndBytes: last.heap.usedSize,
      heapUsedPeakBytes: Math.max(...samples.map(x => x.heap.usedSize)),
      heapUsedDeltaBytes: last.heap.usedSize - first.heap.usedSize,
      indexedDbStartBytes: first.indexedDbBytes, indexedDbEndBytes: last.indexedDbBytes,
      indexedDbPeakBytes: Math.max(...samples.map(x => x.indexedDbBytes)),
      indexedDbDeltaBytes: last.indexedDbBytes - first.indexedDbBytes },
    definition: 'CDP Runtime.getHeapUsage and origin Storage.getUsageAndQuota indexeddb; no forced GC, quota estimates not logical row bytes. CPU procfs process + exactly named native peer threads; counters include sys/user and 1-core basis; callback reception boundaries and CDP sampling overhead recorded, not stack attribution.' };
}
async function attach(page, pid, origin) {
  const hz = Number(execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }).trim());
  if (!Number.isSafeInteger(hz) || hz <= 0) throw Error('CPU tick frequency unavailable');
  const cdp = await page.context().newCDPSession(page), samples = [], jobs = new Set(), errors = [];
  async function sample(name, browserAt) {
    const cpu = cpuSnapshot(pid), started = performance.now();
    const [heap, storage] = await Promise.all([cdp.send('Runtime.getHeapUsage'), cdp.send('Storage.getUsageAndQuota', { origin })]);
    const indexed = storage.usageBreakdown.find(x => x.storageType === 'indexeddb');
    const indexedDbBytes = indexed?.usage ?? (storage.usage === 0 ? 0 : NaN);
    if (![heap.usedSize, heap.totalSize, indexedDbBytes].every(x => Number.isFinite(x) && x >= 0)) throw Error('Heap/IndexedDB meter unavailable');
    samples.push({ name, browserAt, cpu, heap, indexedDbBytes, collectionMs: performance.now() - started });
    if (samples.length > 96) throw Error('Resource sample bound exceeded');
  }
  await page.exposeFunction('__syncV3ResourcePoint', (name, at) => {
    const job = sample(name, at); jobs.add(job);
    job.catch(error => errors.push(error.message)).finally(() => jobs.delete(job));
    return job;
  });
  await sample('pre-navigation', 0);
  return { async finish() {
    await Promise.allSettled([...jobs]); if (errors.length) throw Error(`Resource observation failed: ${errors.join(',')}`);
    samples.sort((a, b) => a.cpu.atMs - b.cpu.atMs); const result = summarize(samples, hz); await cdp.detach(); return result;
  } };
}
module.exports = { cpuInterval, summarize, attach };
