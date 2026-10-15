#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawn } from 'node:child_process';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const arg = (name, fallback) => { const i = process.argv.indexOf(name); return i < 0 ? fallback : process.argv[i + 1]; };
const out = path.resolve(arg('--out', 'runtime/transport-bench-' + new Date().toISOString().replace(/[:.]/g, '-')));
const scenario = JSON.parse(fs.readFileSync(path.join(root, 'tests/sync-v4-bench/scenarios.json'), 'utf8')).baseline;
const durationSeconds = Number(arg('--duration', String(scenario.durationSeconds))), peers = Number(arg('--peers', String(scenario.browserPeers)));

if (!(durationSeconds >= 10 && durationSeconds <= 600 && peers >= 1 && peers <= 3)) throw new Error('Invalid bounded scenario size');
if (durationSeconds < 600 && !process.argv.includes('--smoke')) throw new Error('Short run requires --smoke; never a 10min baseline');
const lossPercent = Number(arg('--loss', String(scenario.oneWayPacketLossPercent)));
const dropMode = arg('--drop-mode', 'factory');
if (!['factory', 'channel'].includes(dropMode)) throw new Error('drop-mode must be factory or channel');
if (!Number.isFinite(lossPercent) || lossPercent < 0 || lossPercent > 5) throw new Error('Loss must be in [0,5] percent');
fs.mkdirSync(out, { recursive: true });

const git = (...args) => fs.existsSync(path.join(root, 'source-manifest.json'))
  ? JSON.parse(fs.readFileSync(path.join(root, 'source-manifest.json'), 'utf8')).sourceHead
  : execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' }).trim();
const manifest = { schema: 'ctox.transport.bench.matrix.v1', startedAt: new Date().toISOString(), owner: arg('--owner', 'unspecified'),
  pid: process.pid, sourceHead: git('rev-parse', 'HEAD'), durationSeconds, peers,
  terminal: false, profiles: [], noCustomerContact: true };
const save = () => fs.writeFileSync(path.join(out, 'matrix.json'), JSON.stringify(manifest, null, 2));
save();
let active;
process.on('SIGTERM', () => { if (active?.pid) execFileSync('sudo', ['-n', 'kill', '-TERM', String(active.pid)]); });
try {
  let turnserver = arg('--turnserver', ''); let turnLibs = arg('--turn-libs', '');
  if (!turnserver) {
    const prefix = path.join(out, 'coturn'), debs = path.join(out, 'debs'); fs.mkdirSync(debs, { recursive: true });
    // Official configured apt repositories; extract into owned prefix, no apt install/system service.
    execFileSync('apt-get', ['download', 'coturn', 'libhiredis1.1.0', 'libmysqlclient24', 'libpq5',
      'libevent-extra-2.1-7t64', 'libevent-openssl-2.1-7t64', 'libevent-pthreads-2.1-7t64'], { cwd: debs, stdio: 'inherit' });
    for (const file of fs.readdirSync(debs).filter(x => x.endsWith('.deb'))) execFileSync('dpkg-deb', ['-x', path.join(debs, file), prefix]);
    turnserver = path.join(prefix, 'usr/bin/turnserver'); turnLibs = path.join(prefix, 'usr/lib/x86_64-linux-gnu');
    manifest.turnPackageFiles = fs.readdirSync(debs).filter(x => x.endsWith('.deb'));
  }
  manifest.turnVersion = execFileSync(turnserver, ['--version'], { env: { ...process.env, LD_LIBRARY_PATH: turnLibs }, encoding: 'utf8' }).trim();
  const profiles = arg('--profiles', scenario.requestedRttMs.join(',')).split(',').map(Number);
  if (profiles.some(x => ![20, 150, 600].includes(x))) throw new Error('Only declared RTT profiles are allowed');
  for (const rttMs of profiles) {
    const profileOut = path.join(out, `rtt-${rttMs}`); fs.mkdirSync(profileOut, { recursive: true });
    const config = { out: profileOut, durationSeconds, peers, rttMs, lossPercent, dropMode,
      uid: process.getuid(), gid: process.getgid(), hostNetworkNamespace: fs.readlinkSync('/proc/self/ns/net'),
      nativeBin: path.resolve(arg('--native-bin', path.join(process.env.CARGO_TARGET_DIR || '', 'release/ctox-transport-bench-peer'))),
      nativeRevision: arg('--native-revision', manifest.sourceHead),
      nativeBuildProfileDeclared: arg('--native-profile', 'release'),

      browserRoot: path.resolve(arg('--browser-root', path.join(root, 'src/apps/business-os/rxdb/src'))),
      browserRevision: arg('--browser-revision', manifest.sourceHead),
      browserAdapter: path.resolve(arg('--browser-adapter', path.join(root, 'tests/sync-v4-bench/browser.mjs'))),
      modulesRoot: path.resolve(arg('--modules-root', '/home/metricspace/build-lane/src/miltonticket-pr589-acceptance/node_modules')),
      chrome: arg('--chrome', '/opt/google/chrome/chrome'), turnPort: 34789, turnserver, turnLibs,
      runner: path.join(root, 'tests/sync-v4-bench/runner.mjs') };
    const configFile = path.join(profileOut, 'config.json'); fs.writeFileSync(configFile, JSON.stringify(config, null, 2));
    const row = { rttMs, startedAt: new Date().toISOString(), output: profileOut }; manifest.profiles.push(row); save();
    active = spawn('sudo', ['-n', 'unshare', '--net', '--', process.execPath, path.join(root, 'scripts/sync-v4/bench-netns.mjs'), configFile], { stdio: 'inherit' });
    row.pid = active.pid; save();
    console.log(JSON.stringify({ phase: 'profile-start', rttMs, pid: active.pid, output: profileOut }));

    row.exitCode = await new Promise((resolve, reject) => { active.on('error', reject); active.on('exit', code => resolve(code)); }); active = null;
    row.finishedAt = new Date().toISOString(); save();
    console.log(JSON.stringify({ phase: 'profile-end', rttMs, exitCode: row.exitCode }));

    if (row.exitCode !== 0) { process.exitCode = 1; break; }
  }
} catch (error) { manifest.error = String(error.message || error).slice(-5000); process.exitCode = 1; }
finally { manifest.finishedAt = new Date().toISOString(); manifest.terminal = true; save(); }
