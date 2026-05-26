#!/usr/bin/env node
/*
 * Run a bounded shard of the Browser/Rust RxDB gate.
 *
 * This is intentionally smaller than browser_rust_soak.js: long foreground
 * runners can be externally terminated in Codex, so this tool runs one group
 * of isolated strict matrix executions and writes durable JSON after each
 * finished mode.
 */
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawn } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const matrixPath = path.join(__dirname, 'browser_rust_smoke_matrix.js');
const defaultModes = [
  'browser-to-rust',
  'command-browser-to-rust',
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

const resultDir = process.env.SPLIT_RESULT_DIR || path.join(os.tmpdir(), 'ctox-rxdb-split-gate');
const cycle = parsePositiveInteger('SPLIT_CYCLE', process.env.SPLIT_CYCLE || '1', { max: 100 });
const groupIndex = parsePositiveInteger('SPLIT_GROUP_INDEX', process.env.SPLIT_GROUP_INDEX || '0', { min: 0, max: 1000 });
const groupSize = parsePositiveInteger('SPLIT_GROUP_SIZE', process.env.SPLIT_GROUP_SIZE || '3', { max: 10 });
const portStride = parsePositiveInteger('SPLIT_PORT_STRIDE', process.env.SPLIT_PORT_STRIDE || '2', { max: 100 });
const businessPortBase = parsePositiveInteger('BUSINESS_PORT', process.env.BUSINESS_PORT || '9000', { max: 65535 });
const signalingPortBase = parsePositiveInteger('SIGNALING_PORT', process.env.SIGNALING_PORT || '19000', { max: 65535 });
const pagePath = process.env.SMOKE_PAGE_PATH || '/index.html';
const modes = (process.env.SPLIT_MODES || defaultModes.join(','))
  .split(',')
  .map((mode) => mode.trim())
  .filter(Boolean);
const selectedModes = modes.slice(groupIndex * groupSize, groupIndex * groupSize + groupSize);
const summary = {
  ok: false,
  cycle,
  groupIndex,
  groupSize,
  pagePath,
  modeCount: modes.length,
  selectedModes,
  startedAt: new Date().toISOString(),
  endedAt: null,
  results: [],
};

if (!selectedModes.length) {
  fail(`No modes selected for groupIndex=${groupIndex} groupSize=${groupSize}`);
}
if (businessPortBase + selectedModes.length * portStride > 65535) {
  fail('BUSINESS_PORT range exceeds 65535');
}
if (signalingPortBase + selectedModes.length * portStride > 65535) {
  fail('SIGNALING_PORT range exceeds 65535');
}

const runnerLock = acquireRunnerLock('split-gate');
fs.mkdirSync(resultDir, { recursive: true });
const summaryPath = path.join(resultDir, `cycle-${String(cycle).padStart(2, '0')}-group-${String(groupIndex).padStart(2, '0')}.json`);
writeSummary();

Promise.all(selectedModes.map((mode, index) => runMode(mode, index)))
  .then((results) => {
    summary.results = results;
    summary.ok = results.every((result) => result.ok);
    summary.endedAt = new Date().toISOString();
    writeSummary();
    printCompactSummary();
    runnerLock.release();
    process.exit(summary.ok ? 0 : 1);
  })
  .catch((error) => {
    summary.error = error?.stack || String(error);
    summary.endedAt = new Date().toISOString();
    writeSummary();
    console.error(summary.error);
    runnerLock.release();
    process.exit(1);
  });

function runMode(mode, index) {
  const id = `${String(cycle).padStart(2, '0')}-${String(groupIndex).padStart(2, '0')}-${String(index).padStart(2, '0')}-${slug(mode)}`;
  const matrixResultPath = path.join(resultDir, `${id}.matrix.json`);
  const logPath = path.join(resultDir, `${id}.log`);
  const out = fs.openSync(logPath, 'w');
  const err = fs.openSync(logPath, 'a');
  const env = {
    ...process.env,
    CTOX_SKIP_SMOKE_BUILD: '1',
    SMOKE_MODES: mode,
    SMOKE_MATRIX_ATTEMPTS: '1',
    SMOKE_MATRIX_RESULT_PATH: matrixResultPath,
    SMOKE_PAGE_PATH: pagePath,
    BUSINESS_PORT: String(businessPortBase + index * portStride),
    SIGNALING_PORT: String(signalingPortBase + index * portStride),
  };
  const startedAt = Date.now();
  const child = spawn(process.execPath, [matrixPath], {
    cwd: root,
    env,
    stdio: ['ignore', out, err],
  });
  return new Promise((resolve) => {
    child.once('close', (status, signal) => {
      fs.closeSync(out);
      fs.closeSync(err);
      const matrix = readJson(matrixResultPath);
      const modeResult = matrix?.modes?.[0] || null;
      const attempt = modeResult?.attempts?.[0] || null;
      const result = {
        mode,
        ok: status === 0 && !signal && modeResult?.ok === true,
        status,
        signal,
        durationMs: Date.now() - startedAt,
        matrixDurationMs: attempt?.durationMs || null,
        evidence: attempt?.evidence || {},
        evidenceProblems: attempt?.evidenceProblems || [],
        matrixResultPath,
        logPath,
      };
      summary.results = summary.results.filter((entry) => entry.mode !== mode).concat(result);
      writeSummary();
      resolve(result);
    });
  });
}

function printCompactSummary() {
  const attempts = summary.results;
  const failed = attempts.filter((attempt) => !attempt.ok);
  const maxDuration = Math.max(0, ...attempts.map((attempt) => Number(attempt.matrixDurationMs || attempt.durationMs || 0)));
  const maxSync = Math.max(0, ...attempts.map((attempt) => Number(attempt.evidence?.ctox_sync_config_wait_ms || 0)));
  const maxWarnings = Math.max(0, ...attempts.map((attempt) => Number(attempt.evidence?.browser_warning_count || 0)));
  const maxRequestFailures = Math.max(0, ...attempts.map((attempt) => Number(attempt.evidence?.browser_request_failure_count || 0)));
  const maxRepairs = Math.max(0, ...attempts.map((attempt) => Number(attempt.evidence?.browser_cache_repair_count || 0)));
  const maxReloads = Math.max(0, ...attempts.map((attempt) => Number(attempt.evidence?.startup_smoke_hook_reload_count || 0)));
  console.log(JSON.stringify({
    ok: summary.ok,
    cycle: summary.cycle,
    groupIndex: summary.groupIndex,
    modes: attempts.map((attempt) => attempt.mode),
    failed: failed.map((attempt) => ({
      mode: attempt.mode,
      status: attempt.status,
      signal: attempt.signal,
      evidenceProblems: attempt.evidenceProblems,
      logPath: attempt.logPath,
    })),
    maxDuration,
    maxSync,
    maxWarnings,
    maxRequestFailures,
    maxRepairs,
    maxReloads,
    summaryPath,
  }, null, 2));
}

function writeSummary() {
  fs.writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`);
}

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch {
    return null;
  }
}

function parsePositiveInteger(name, value, options = {}) {
  if (!/^(0|[1-9]\d*)$/.test(String(value))) fail(`${name} must be a non-negative integer; got ${JSON.stringify(value)}`);
  const parsed = Number(value);
  const min = options.min ?? 1;
  const max = options.max ?? Number.MAX_SAFE_INTEGER;
  if (parsed < min || parsed > max) fail(`${name} must be between ${min} and ${max}; got ${parsed}`);
  return parsed;
}

function slug(value) {
  return String(value).replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_+|_+$/g, '');
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
      process.once('SIGINT', () => {
        release();
        process.exit(130);
      });
      process.once('SIGTERM', () => {
        release();
        process.exit(143);
      });
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
          fail(`another CTOX RxDB runner is active: pid=${pid} owner=${current?.owner || 'unknown'} lock=${lockDir}`);
        } catch (killError) {
          if (killError?.code !== 'ESRCH') throw killError;
        }
      }
      try { fs.rmSync(lockDir, { recursive: true, force: true }); } catch {}
    }
  }
}

function fail(message) {
  console.error(`rxdb split gate configuration error: ${message}`);
  process.exit(2);
}
