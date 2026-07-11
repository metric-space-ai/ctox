#!/usr/bin/env node
/*
 * Repeatable Browser/Rust RxDB WebRTC soak runner.
 *
 * This wraps the full-app smoke matrix with stable port spacing so short CI
 * runs and longer manual runs exercise reconnect behavior without reusing
 * ports or SQLite runtime roots.
 *
 * Examples:
 *   node src/core/rxdb/tools/browser_rust_soak.js
 *   SOAK_CYCLES=12 SOAK_MODES=browser-to-rust,tickets-browser-to-rust,workspace-agent-artifacts-rust-to-browser,workspace-agent-artifacts-stress-rust-to-browser,workspace-agent-artifacts-churn-rust-to-browser,workspace-update-rust-to-browser,workspace-large-materialize-rust-to-browser,workspace-large-file-viewer-rust-to-browser,workspace-large-file-viewer-restart-rust-to-browser,migration-version-browser-to-rust,command-burst-browser-to-rust,restart-browser-to-rust,restart-signaling-browser-to-rust,rollover-native-peer-browser-to-rust,tab-freeze-browser-to-rust,network-flap-browser-to-rust,command-midflight-restart-browser-to-rust,signaling-error-browser-status,peer-lifecycle-browser-status,checkpoint-error-browser-status,schema-error-browser-status,replication-error-browser-status,replication-push-contract-error-browser-status,file-chunk-metadata-error-browser-status,file-chunk-tombstone-error-browser-status,file-chunk-stale-generation-error-browser-status node src/core/rxdb/tools/browser_rust_soak.js
 *   SOAK_FAIL_ON_RETRY=1 SOAK_RESULT_PATH=/tmp/rxdb-soak.json node src/core/rxdb/tools/browser_rust_soak.js
 */
const path = require('path');
const fs = require('fs');
const os = require('os');
const crypto = require('crypto');
const { spawn, execFileSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const matrixPath = path.join(__dirname, 'browser_rust_smoke_matrix.js');
const smokeBinaryPath = process.env.CTOX_BIN
  || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox');
const defaultSoakModes = [
  'browser-to-rust',
  'command-browser-to-rust',
  'tickets-browser-to-rust',
  'migration-version-browser-to-rust',
  'command-burst-browser-to-rust',
  'command-reload-browser-to-rust',
  'workspace-rust-to-browser',
  'workspace-agent-artifacts-rust-to-browser',
  'workspace-agent-artifacts-stress-rust-to-browser',
  'workspace-agent-artifacts-churn-rust-to-browser',
  'workspace-agent-artifacts-background-rust-to-browser',
  'workspace-update-rust-to-browser',
  'workspace-large-materialize-rust-to-browser',
  'workspace-large-file-viewer-rust-to-browser',
  'workspace-large-file-viewer-restart-rust-to-browser',
  'restart-browser-to-rust',
  'restart-signaling-browser-to-rust',
  'rollover-native-peer-browser-to-rust',
  'tab-freeze-browser-to-rust',
  'network-flap-browser-to-rust',
  'command-midflight-restart-browser-to-rust',
  'signaling-error-browser-status',
  'peer-lifecycle-browser-status',
  'checkpoint-error-browser-status',
  'rxdb-protocol-error-browser-status',
  'schema-error-browser-status',
  'replication-error-browser-status',
  'replication-push-contract-error-browser-status',
  'file-chunk-metadata-error-browser-status',
  'file-chunk-tombstone-error-browser-status',
  'file-chunk-stale-generation-error-browser-status',
];
const cyclesInput = process.env.SOAK_CYCLES || '3';
const minCyclesInput = process.env.SOAK_MIN_CYCLES || '';
const modes = process.env.SOAK_MODES || defaultSoakModes.join(',');
const modeList = parseModeList(modes);
const requiredModeList = parseModeList(process.env.SOAK_REQUIRED_MODES || '');
const pagePath = process.env.SMOKE_PAGE_PATH || '/index.html';
const businessPortBaseInput = process.env.BUSINESS_PORT || '9000';
const signalingPortBaseInput = process.env.SIGNALING_PORT || '19000';
const portStrideInput = process.env.SOAK_PORT_STRIDE || '20';
const resultPath = process.env.SOAK_RESULT_PATH || '';
const failOnRetry = process.env.SOAK_FAIL_ON_RETRY === '1';
const runSummary = {
  cycles: null,
  minCycles: null,
  modes,
  requiredModes: requiredModeList.join(','),
  pagePath,
  failOnRetry,
  startedAt: new Date().toISOString(),
  endedAt: null,
  ok: false,
  cycleResults: [],
  source: sourceEvidence(),
};
let runnerLock = null;
const cycles = parsePositiveIntegerConfig('SOAK_CYCLES', cyclesInput, { max: 100 });
const minCycles = minCyclesInput
  ? parsePositiveIntegerConfig('SOAK_MIN_CYCLES', minCyclesInput, { max: 100 })
  : 0;
const businessPortBase = parsePositiveIntegerConfig('BUSINESS_PORT', businessPortBaseInput, { max: 65535 });
const signalingPortBase = parsePositiveIntegerConfig('SIGNALING_PORT', signalingPortBaseInput, { max: 65535 });
const portStride = parsePositiveIntegerConfig('SOAK_PORT_STRIDE', portStrideInput, { min: 20, max: 1000 });
runSummary.cycles = cycles;
runSummary.minCycles = minCycles || null;

if (minCycles && cycles < minCycles) {
  failConfiguration(`SOAK_CYCLES must be at least ${minCycles} for this soak profile; got ${cycles}`);
}
const missingRequiredModes = requiredModeList.filter((mode) => !modeList.includes(mode));
if (missingRequiredModes.length) {
  failConfiguration(`SOAK_MODES is missing required release mode(s): ${missingRequiredModes.join(', ')}`);
}
if (requiredModeList.length && !failOnRetry) {
  failConfiguration('SOAK_FAIL_ON_RETRY=1 is required when SOAK_REQUIRED_MODES is set');
}
if (requiredModeList.length && runSummary.source.dirty) {
  failConfiguration('a dirty working tree cannot qualify a release soak');
}
const maxPortOffset = (cycles - 1) * portStride;
if (businessPortBase + maxPortOffset > 65535) {
  failConfiguration(`BUSINESS_PORT plus soak port range exceeds 65535: ${businessPortBase}+${maxPortOffset}`);
}
if (signalingPortBase + maxPortOffset > 65535) {
  failConfiguration(`SIGNALING_PORT plus soak port range exceeds 65535: ${signalingPortBase}+${maxPortOffset}`);
}

runnerLock = acquireRunnerLock('soak');
let activeMatrixChild = null;
let terminationSignal = null;
let forceExitTimer = null;

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.once(signal, () => handleTerminationSignal(signal));
}

main().catch((error) => {
  runSummary.error = error?.stack || String(error);
  console.error(runSummary.error);
  writeRunSummary(false);
  process.exit(1);
});

async function main() {
  const startedAt = Date.now();
  for (let cycle = 0; cycle < cycles; cycle++) {
    const matrixResultPath = path.join(os.tmpdir(), `ctox-rxdb-smoke-matrix-${process.pid}-${cycle}.json`);
    const env = {
      ...process.env,
      SMOKE_PAGE_PATH: pagePath,
      SMOKE_MODES: modes,
      SMOKE_REQUIRE_EVIDENCE: requiredModeList.length ? '1' : process.env.SMOKE_REQUIRE_EVIDENCE,
      BUSINESS_PORT: String(businessPortBase + cycle * portStride),
      SIGNALING_PORT: String(signalingPortBase + cycle * portStride),
      SMOKE_MATRIX_RESULT_PATH: matrixResultPath,
    };
    console.log(`\n=== rxdb soak cycle ${cycle + 1}/${cycles}: ${modes} ===`);
    console.log(`matrix_result_path=${matrixResultPath}`);
    const result = await runMatrixCycle(env);
    const cycleResult = readJsonFile(matrixResultPath) || {
      ok: false,
      modes: [],
      error: 'matrix result was not written',
    };
    cycleResult.matrixResultPath = matrixResultPath;
    if (result.status !== 0 || result.signal) {
      cycleResult.process = {
        status: result.status,
        signal: result.signal,
        stdoutTail: 'streamed to parent stdout',
        stderrTail: 'streamed to parent stderr',
      };
    }
    cycleResult.cycle = cycle + 1;
    runSummary.cycleResults.push(cycleResult);
    if (result.signal || terminationSignal) {
      const signal = result.signal || terminationSignal;
      runSummary.terminationSignal = signal;
      console.error(`rxdb soak cycle ${cycle + 1} terminated by signal ${signal}`);
      writeRunSummary(false);
      process.exit(1);
    }
    if (result.status !== 0) {
      console.error(`rxdb soak cycle ${cycle + 1} failed with status ${result.status}`);
      writeRunSummary(false);
      process.exit(result.status || 1);
    }
  }

  const elapsedSeconds = Math.round((Date.now() - startedAt) / 1000);
  const retryCount = runSummary.cycleResults
    .flatMap((cycle) => cycle.modes || [])
    .reduce((total, mode) => total + Math.max(0, (mode.attempts?.length || 1) - 1), 0);
  runSummary.retryCount = retryCount;
  if (failOnRetry && retryCount > 0) {
    console.error(`\nrxdb soak had ${retryCount} retried mode attempt(s); failing because SOAK_FAIL_ON_RETRY=1`);
    writeRunSummary(false);
    process.exit(1);
  }
  writeRunSummary(true);
  console.log(`\nrxdb soak OK: cycles=${cycles} modes=${modes} retries=${retryCount} elapsed=${elapsedSeconds}s`);
}

function runMatrixCycle(env) {
  activeMatrixChild = spawn(process.execPath, [matrixPath], {
    cwd: root,
    env,
    detached: true,
    stdio: ['ignore', 'inherit', 'inherit'],
  });
  return new Promise((resolve, reject) => {
    activeMatrixChild.once('error', reject);
    activeMatrixChild.once('close', (status, signal) => {
      if (forceExitTimer) {
        clearTimeout(forceExitTimer);
        forceExitTimer = null;
      }
      activeMatrixChild = null;
      resolve({ status, signal });
    });
  });
}

function handleTerminationSignal(signal) {
  terminationSignal = signal;
  runSummary.terminationSignal = signal;
  console.error(`rxdb soak received ${signal}; forwarding to active matrix child`);
  if (!activeMatrixChild?.pid) {
    writeRunSummary(false);
    process.exit(1);
  }
  terminateProcessTree(activeMatrixChild.pid, signal);
  forceExitTimer = setTimeout(() => {
    if (activeMatrixChild?.pid) {
      terminateProcessTree(activeMatrixChild.pid, 'SIGKILL');
    }
    writeRunSummary(false);
    process.exit(1);
  }, 30_000);
  forceExitTimer.unref();
}

function terminateProcessTree(pid, signal) {
  const descendants = collectDescendantPids(pid);
  try {
    process.kill(-pid, signal);
  } catch {}
  for (const childPid of descendants.reverse()) {
    try {
      process.kill(childPid, signal);
    } catch {}
  }
  try {
    process.kill(pid, signal);
  } catch {}
}

function collectDescendantPids(rootPid) {
  let rows = [];
  try {
    rows = execFileSync('ps', ['-axo', 'pid=,ppid='], { encoding: 'utf8' })
      .split(/\r?\n/)
      .map((line) => line.trim().split(/\s+/).map(Number))
      .filter(([pid, ppid]) => Number.isFinite(pid) && Number.isFinite(ppid));
  } catch {
    return [];
  }
  const childrenByParent = new Map();
  for (const [pid, ppid] of rows) {
    if (!childrenByParent.has(ppid)) childrenByParent.set(ppid, []);
    childrenByParent.get(ppid).push(pid);
  }
  const result = [];
  const stack = [...(childrenByParent.get(rootPid) || [])];
  while (stack.length) {
    const pid = stack.pop();
    result.push(pid);
    stack.push(...(childrenByParent.get(pid) || []));
  }
  return result;
}

function readJsonFile(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return null;
  }
}

function tailLines(text, maxLines) {
  return String(text || '')
    .split(/\r?\n/)
    .slice(-maxLines)
    .join('\n');
}

function parseModeList(value) {
  return String(value || '')
    .split(/[,\s]+/)
    .map((mode) => mode.trim())
    .filter(Boolean);
}

function acquireRunnerLock(owner) {
  if (process.env.CTOX_RXDB_RUNNER_LOCK === '0') {
    return { release() {} };
  }
  const lockDir = process.env.CTOX_RXDB_RUNNER_LOCK_DIR || path.join(os.tmpdir(), 'ctox-rxdb-runner.lock');
  for (;;) {
    try {
      fs.mkdirSync(lockDir);
      fs.writeFileSync(path.join(lockDir, 'owner.json'), `${JSON.stringify({
        owner,
        pid: process.pid,
        startedAt: new Date().toISOString(),
        cwd: process.cwd(),
      }, null, 2)}\n`);
      let released = false;
      const release = () => {
        if (released) return;
        released = true;
        try { fs.rmSync(lockDir, { recursive: true, force: true }); } catch {}
      };
      process.once('exit', release);
      return { release };
    } catch (error) {
      if (error?.code !== 'EEXIST') throw error;
      const ownerPath = path.join(lockDir, 'owner.json');
      let current = null;
      try { current = JSON.parse(fs.readFileSync(ownerPath, 'utf8')); } catch {}
      const pid = Number(current?.pid || 0);
      if (pid > 0) {
        try {
          process.kill(pid, 0);
          failConfiguration(`another CTOX RxDB runner is active: pid=${pid} owner=${current?.owner || 'unknown'} lock=${lockDir}`);
        } catch (killError) {
          if (killError?.code !== 'ESRCH') throw killError;
        }
      }
      try { fs.rmSync(lockDir, { recursive: true, force: true }); } catch {}
    }
  }
}

function failConfiguration(message) {
  runnerLock?.release?.();
  runSummary.configurationError = message;
  console.error(`rxdb soak configuration error: ${message}`);
  writeRunSummary(false);
  process.exit(1);
}

function parsePositiveIntegerConfig(name, value, options = {}) {
  const parsed = Number(value);
  const min = options.min ?? 1;
  const max = options.max ?? Number.MAX_SAFE_INTEGER;
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    failConfiguration(`${name} must be an integer between ${min} and ${max}; got ${JSON.stringify(String(value))}`);
  }
  return parsed;
}

function writeRunSummary(ok) {
  runSummary.ok = ok;
  runSummary.endedAt = new Date().toISOString();
  if (!resultPath) return;
  fs.mkdirSync(path.dirname(resultPath), { recursive: true });
  fs.writeFileSync(resultPath, `${JSON.stringify(runSummary, null, 2)}\n`);
}

function sourceEvidence() {
  const git = (args, fallback = '') => {
    try { return execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim(); }
    catch { return fallback; }
  };
  const status = git(['status', '--porcelain=v1', '--untracked-files=all']);
  return {
    commit: git(['rev-parse', 'HEAD'], 'unknown'),
    dirty: Boolean(status),
    artifactHashes: {
      browserBundleSha256: sha256File(path.join(root, 'src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs')),
      smokeBinaryPath,
      smokeBinarySha256: sha256File(smokeBinaryPath),
    },
  };
}

function sha256File(filePath) {
  try { return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex'); }
  catch { return null; }
}
