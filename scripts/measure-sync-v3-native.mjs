#!/usr/bin/env node
// Run only under the normal lane, against a supplied existing native artifact.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createWriteStream, createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyzeCase, compareCases } from './sync-v3/phase-analysis.mjs';

const values = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  const name = process.argv[index];
  if (!['--binary', '--playwright', '--output', '--soak-seconds', '--soak-status'].includes(name) || !process.argv[index + 1] || values.has(name)) throw Error('Supply binary/playwright/output, optional soak-seconds/status');
  values.set(name, name === '--soak-seconds' ? process.argv[index + 1] : resolve(process.argv[index + 1]));
}
if (!['--binary', '--playwright', '--output'].every(key => values.has(key)) || !process.env.TMPDIR) throw Error('A supplied native binary, pinned Playwright and admitted TMPDIR are required');
const soakSeconds = Number(values.get('--soak-seconds') || 0);
if (![0,20,86400].includes(soakSeconds) || (soakSeconds && !values.has('--soak-status'))) throw Error('Bounded soak duration and status path required');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = values.get('--output');
if (!output.startsWith(resolve(process.env.TMPDIR) + '/')) throw Error('Evidence must be in the admitted TMPDIR');
await mkdir(output, { mode: 0o700 });
const hash = createHash('sha256');
for await (const chunk of createReadStream(values.get('--binary'))) hash.update(chunk);
const report = { version: 1, stage: 'sync-v3:S0', sourceHead: process.env.BUILD_LANE_HEAD || null,
  binarySha256: hash.digest('hex'), installedAcceptance: false,
  relayDefinition: 'real loopback UDP datagram relay; requested RTT split equally by direction; not a TURN deployment',
  repetitions: 'one fresh isolated browser/native prefix per RTT, five sequential write samples; exploratory baseline, not statistical acceptance',
  cases: [], pass: false };
const percentile = (numbers, quantile) => [...numbers].sort((a, b) => a - b)[Math.ceil(numbers.length * quantile) - 1];
try {
  for (const [index, rtt] of (soakSeconds === 86400 ? [0] : [0, 300, 600]).entries()) {
    const runtimeRoot = join(output, `rtt-${rtt}`);
    await mkdir(runtimeRoot, { mode: 0o700 });
    const log = createWriteStream(join(output, `rtt-${rtt}.log`), { flags: 'wx', mode: 0o600 });
    const start = Date.now();
    const soakArgs = soakSeconds && rtt === 0 ? [`--sync-v3-soak-seconds=${soakSeconds}`, `--sync-v3-soak-status=${values.get('--soak-status')}`] : [];
    const child = spawn(process.execPath, [join(root, 'src/core/rxdb/tools/browser_rust_smoke.js'), `--sync-v3-relay-rtt=${rtt}`, ...soakArgs], {
      cwd: root, detached: true, env: { ...process.env, CTOX_BIN: values.get('--binary'),
        PLAYWRIGHT_MODULE_PATH: values.get('--playwright'), CTOX_SMOKE_ROOT: runtimeRoot,
        SMOKE_MODE: 'sync-v3-scale-relay', BUSINESS_PORT: String(61931 + index * 2),
        SIGNALING_PORT: String(61932 + index * 2), SMOKE_PROCESS_LIFECYCLE_PATH: join(runtimeRoot, 'process-lifecycle.json') },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    await writeFile(join(runtimeRoot, 'runner-owner.json'), JSON.stringify({ owner: '01a0879f-d2e0-72c3-9353-4ef802b2998b',
      pidPgid: child.pid, purpose: `isolated S0 ${rtt}ms RTT`, output: runtimeRoot, stopCondition: 'terminal exit or 240s deadline, own group TERM then KILL after10s' }));
    child.stdout.pipe(log, { end: false }); child.stderr.pipe(log, { end: false });
    const signal = value => { try { process.kill(-child.pid, value); } catch (error) { if (error.code !== 'ESRCH') throw error; } };
    let killTimer;
    const deadline = setTimeout(() => { signal('SIGTERM'); killTimer = setTimeout(() => signal('SIGKILL'), 10000); }, 240000 + (rtt === 0 ? soakSeconds * 1000 : 0));
    const exit = await new Promise((resolveExit, reject) => { child.once('error', reject); child.once('close', (code, signal) => resolveExit({ code, signal })); });
    clearTimeout(deadline); clearTimeout(killTimer);
    await new Promise(resolveLog => log.end(resolveLog));
    if (exit.code !== 0) { signal('SIGKILL'); console.error((await readFile(join(output, `rtt-${rtt}.log`), 'utf8')).slice(-8000)); throw Error(`RTT ${rtt} native harness failed: ${JSON.stringify(exit)}; ${join(output, `rtt-${rtt}.log`)}`); }
    const measurement = JSON.parse(await readFile(join(runtimeRoot, 'sync-v3-scale-result.json')));
    const relay = JSON.parse(await readFile(join(runtimeRoot, 'sync-v3-relay.json')));
    if (relay.errors.length || !relay.pairs.length || relay.requestedRttMs !== rtt
      || relay.pairs.some(pair => !pair.endpointsKnown || !pair.forwarded.native || !pair.forwarded.browser)) throw Error('Relay evidence incomplete or failed');
    const ports = new Set(relay.pairs.map(pair => pair.relayPort));
    if (measurement.selectedCandidatePairs.some(pair => !ports.has(pair.remotePort))) throw Error('Selected ICE pair bypassed impairment relay');
    if (rtt && relay.pairs.some(pair => pair.holdMinMs < rtt / 2 - 2)) throw Error('Datagram delay oracle below requested floor');
    report.cases.push({ ...measurement, nativeHarnessElapsedMs: Date.now() - start, relay,
      phaseAnalysis: analyzeCase(measurement),
      nativeAckMedianMs: percentile(measurement.writes.map(sample => sample.nativeAckMs), .5),
      nativeAckP95Ms: percentile(measurement.writes.map(sample => sample.nativeAckMs), .95),
      criterion: 'complete visible native rows, five accepted ACKs with SQLite readback, real relay path and delay validated; RFC product budget is measured separately' });
  }
  report.pass = true;
  if (soakSeconds !== 86400) report.phaseComparison = compareCases(report.cases);
} catch (error) {
  report.error = error.message;
  process.exitCode = 1;
} finally {
  await writeFile(join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  console.log(JSON.stringify(report));
}
