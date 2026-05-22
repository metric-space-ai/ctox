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
 *   SOAK_CYCLES=12 SOAK_MODES=browser-to-rust,workspace-update-rust-to-browser,workspace-large-materialize-rust-to-browser,workspace-large-file-viewer-rust-to-browser,workspace-large-file-viewer-restart-rust-to-browser,command-burst-browser-to-rust,restart-browser-to-rust,restart-signaling-browser-to-rust,rollover-native-peer-browser-to-rust,tab-freeze-browser-to-rust,network-flap-browser-to-rust,command-midflight-restart-browser-to-rust,signaling-error-browser-status,checkpoint-error-browser-status,schema-error-browser-status node src/core/rxdb/tools/browser_rust_soak.js
 *   SOAK_FAIL_ON_RETRY=1 SOAK_RESULT_PATH=/tmp/rxdb-soak.json node src/core/rxdb/tools/browser_rust_soak.js
 */
const path = require('path');
const fs = require('fs');
const os = require('os');
const { spawnSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const matrixPath = path.join(__dirname, 'browser_rust_smoke_matrix.js');
const defaultSoakModes = [
  'browser-to-rust',
  'command-browser-to-rust',
  'command-burst-browser-to-rust',
  'workspace-rust-to-browser',
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
  'checkpoint-error-browser-status',
  'schema-error-browser-status',
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
};
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
const maxPortOffset = (cycles - 1) * portStride;
if (businessPortBase + maxPortOffset > 65535) {
  failConfiguration(`BUSINESS_PORT plus soak port range exceeds 65535: ${businessPortBase}+${maxPortOffset}`);
}
if (signalingPortBase + maxPortOffset > 65535) {
  failConfiguration(`SIGNALING_PORT plus soak port range exceeds 65535: ${signalingPortBase}+${maxPortOffset}`);
}

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
  const result = spawnSync(process.execPath, [matrixPath], {
    cwd: root,
    env,
    stdio: 'inherit',
  });
  const cycleResult = readJsonFile(matrixResultPath) || {
    ok: false,
    modes: [],
    error: 'matrix result was not written',
  };
  cycleResult.cycle = cycle + 1;
  runSummary.cycleResults.push(cycleResult);
  if (result.signal) {
    console.error(`rxdb soak cycle ${cycle + 1} terminated by signal ${result.signal}`);
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

function readJsonFile(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return null;
  }
}

function parseModeList(value) {
  return String(value || '')
    .split(/[,\s]+/)
    .map((mode) => mode.trim())
    .filter(Boolean);
}

function failConfiguration(message) {
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
