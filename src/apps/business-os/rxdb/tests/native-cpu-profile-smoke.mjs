import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { parseProcStat, cpuDelta, startNativeCpuProfile } = require('../../../../core/rxdb/tools/native_cpu_profile.js');

// Linux proc_pid_stat(5): utime=field14, stime=field15, starttime=field22.
// Parentheses and spaces inside comm must not shift the numeric fields.
const stat = parseProcStat('42 (worker ) one) S 1 2 3 4 5 6 7 8 9 10 23 7 0 0 0 0 1 0 100 0');
assert.deepEqual(stat, { name: 'worker ) one', ticks: 30, startedTicks: 100 });
assert.deepEqual(cpuDelta({ ...stat, ticks: 20 }, stat, 200, 100), { cpuMs: 100, percentOfOneCore: 50 });
assert.equal(cpuDelta({ ...stat, startedTicks: 99 }, stat, 200, 100), null, 'PID/TID reuse has no valid delta');
assert.equal(cpuDelta({ ...stat, ticks: 31 }, stat, 200, 100), null, 'counter rollback is not negative CPU');
assert.throws(() => parseProcStat('42 (worker) S'), /invalid proc stat counters/);
const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-native-cpu-proof-'));
const outputPath = path.join(directory, 'cpu.jsonl');
try {
  if (process.platform !== 'linux') {
    const child = new EventEmitter(); child.pid = process.pid;
    startNativeCpuProfile(child, { outputPath });
    const header = JSON.parse(fs.readFileSync(outputPath, 'utf8'));
    assert.equal(header.available, false);
    assert.equal(header.reason, 'procfs-requires-linux');
    console.log('native CPU parser/unsupported-platform PASS; live Linux sampling unavailable on ' + process.platform);
  } else {
    // Real owned process; work is bounded by consumed CPU, not a mocked tick
    // stream or an assumption about how fast this runner schedules the child.
    const child = spawn(process.execPath, ['-e', `
      process.once('message', () => {
        const start = process.cpuUsage();
        while (true) {
          const used = process.cpuUsage(start);
          if (used.user + used.system >= 250000) break;
        }
        setTimeout(() => process.exit(0), 150);
      });
    `], { stdio: ['ignore', 'ignore', 'ignore', 'ipc'] });
    const stop = startNativeCpuProfile(child, { outputPath, intervalMs: 50, maxSamples: 300, phase: () => 'controlled-cpu-work' });
    child.send('start');
    let timer;
    try {
      await Promise.race([
        new Promise((resolve, reject) => {
          child.once('error', reject);
          child.once('exit', code => code === 0 ? resolve() : reject(new Error('child exit ' + code)));
        }),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('CPU child timeout')), 12000); }),
      ]);
      const rows = fs.readFileSync(outputPath, 'utf8').trim().split('\n').map(line => JSON.parse(line));
      assert.equal(rows[0].available, true);
      const samples = rows.filter(row => row.kind === 'sample');
      assert.ok(samples.length >= 2);
      assert.ok(samples.reduce((total, row) => total + (row.total?.cpuMs || 0), 0) >= 100, 'actual CPU work was not measured');
      assert.ok(samples.some(row => row.threads.some(thread => thread.cpuMs > 0)));
      assert.ok(samples.every(row => row.phase === 'controlled-cpu-work' && row.pid === child.pid));
      const summary = rows.at(-1);
      assert.equal(summary.kind, 'summary');
      assert.equal(summary.reason, 'child-exited');
      assert.ok(summary.observerMs >= 0 && summary.observerMs < summary.elapsedMs);
      console.log('native CPU live Linux child sampling PASS: ' + samples.length + ' samples');
    } finally {
      clearTimeout(timer);
      stop();
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    }
  }
} finally { fs.rmSync(directory, { recursive: true, force: true }); }
