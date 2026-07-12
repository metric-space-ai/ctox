#!/usr/bin/env node
'use strict';

/*
 * Run the CTOX Sync Engine 9.5 full Browser/Rust matrix:
 * - default matrix modes from browser_rust_smoke_matrix.js
 * - Business OS production modes from business_os_production_smoke_registry.js
 *
 * The underlying matrix runner writes the source commit, dirty flag and
 * browser-bundle/smoke-binary hashes that the 9.5 audit enforces.
 */

const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const selfTest = process.argv.includes('--self-test');
const skipRun = process.argv.includes('--skip-run');
const defaultOutput = path.resolve(flagValue('--default-output') || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-default-matrix.json'));
const businessOutput = path.resolve(flagValue('--business-output') || path.join(root, 'runtime/build/ctox-sync-production-readiness-95-business-os-matrix.json'));
const smokeBinaryPath = flagValue('--smoke-binary');
const matrixRunner = path.join(root, 'src/core/rxdb/tools/browser_rust_smoke_matrix.js');
const {
  businessOsProductionSmokeModes,
} = require('./business_os_production_smoke_registry');

if (selfTest) {
  runSelfTest();
  process.exit(0);
}

if (!skipRun) {
  runMatrix('default', {
    output: defaultOutput,
    modes: '',
    businessPort: '9200',
    signalingPort: '19200',
  });
  runMatrix('business-os', {
    output: businessOutput,
    modes: businessOsProductionSmokeModes.join(','),
    businessPort: '9300',
    signalingPort: '19300',
  });
}

validateOutputs();
console.log(`ctox_sync_production_readiness_95_full_matrix=1 default=${path.relative(root, defaultOutput)} business=${path.relative(root, businessOutput)}`);

function runMatrix(label, options) {
  const env = {
    ...process.env,
    SMOKE_MATRIX_ATTEMPTS: '1',
    SMOKE_PAGE_PATH: '/index.html',
    SMOKE_MODE_TIMEOUT_MS: '300000',
    SMOKE_BROWSER_WARNING_BUDGET: '0',
    SMOKE_BROWSER_ERROR_BUDGET: '0',
    SMOKE_BROWSER_REQUEST_FAILURE_BUDGET: '0',
    SMOKE_MATRIX_RESULT_PATH: options.output,
    BUSINESS_PORT: options.businessPort,
    SIGNALING_PORT: options.signalingPort,
    ...(options.modes ? { SMOKE_MODES: options.modes } : {}),
    ...(smokeBinaryPath ? { CTOX_BIN: path.resolve(smokeBinaryPath) } : {}),
  };
  const result = spawnSync(process.execPath, [matrixRunner], {
    cwd: root,
    env,
    encoding: 'utf8',
    stdio: 'inherit',
    timeout: 60 * 60 * 1000,
    killSignal: 'SIGTERM',
  });
  if (result.error) throw result.error;
  if (result.status !== 0 || result.signal) {
    throw new Error(`${label} matrix failed status=${result.status} signal=${result.signal || ''}`);
  }
}

function validateOutputs() {
  const defaultMatrix = readJson(defaultOutput);
  const businessMatrix = readJson(businessOutput);
  validateMatrix(defaultMatrix, 'default');
  validateMatrix(businessMatrix, 'business');
  const uniqueModes = new Set([
    ...defaultMatrix.requestedModes,
    ...businessMatrix.requestedModes,
  ]);
  if (uniqueModes.size < 40) {
    throw new Error(`full matrix unique mode count below 40: ${uniqueModes.size}`);
  }
  for (const mode of businessOsProductionSmokeModes) {
    if (!businessMatrix.requestedModes.includes(mode)) {
      throw new Error(`business matrix missing mode ${mode}`);
    }
  }
}

function validateMatrix(matrix, label) {
  if (!matrix || typeof matrix !== 'object') throw new Error(`${label} matrix missing`);
  if (matrix.schema !== 'ctox.business_os.smoke_matrix_summary.v1') throw new Error(`${label} matrix schema`);
  if (matrix.ok !== true) throw new Error(`${label} matrix ok`);
  if (matrix.configuration?.attempts !== 1) throw new Error(`${label} matrix attempts`);
  if (!Array.isArray(matrix.requestedModes) || matrix.requestedModes.length === 0) {
    throw new Error(`${label} matrix requestedModes`);
  }
  if (!matrix.source?.artifactHashes?.browserBundleSha256 || !matrix.source?.artifactHashes?.smokeBinarySha256) {
    throw new Error(`${label} matrix source hashes`);
  }
}

function runSelfTest() {
  const tmp = fs.mkdtempSync(path.join(require('os').tmpdir(), 'ctox-full-matrix-'));
  const defaultFixture = path.join(tmp, 'default.json');
  const businessFixture = path.join(tmp, 'business.json');
  fs.writeFileSync(defaultFixture, `${JSON.stringify(fixtureMatrix(['rust-to-browser', 'browser-to-rust', ...Array.from({ length: 32 }, (_, index) => `default-${index}`)]), null, 2)}\n`);
  fs.writeFileSync(businessFixture, `${JSON.stringify(fixtureMatrix([...businessOsProductionSmokeModes]), null, 2)}\n`);
  const previousDefault = defaultOutput;
  const previousBusiness = businessOutput;
  // Validate the same shape through a local copy of validateOutputs without
  // mutating the module-level const paths used by normal execution.
  validateMatrix(readJson(defaultFixture), 'default-self-test');
  validateMatrix(readJson(businessFixture), 'business-self-test');
  const uniqueModes = new Set([
    ...readJson(defaultFixture).requestedModes,
    ...readJson(businessFixture).requestedModes,
  ]);
  if (uniqueModes.size < 40) throw new Error('self-test full mode count');
  if (!previousDefault || !previousBusiness) throw new Error('output paths');
  const source = fs.readFileSync(__filename, 'utf8');
  for (const token of [
    'ctox_sync_production_readiness_95_full_matrix=1',
    'SMOKE_MATRIX_ATTEMPTS',
    'SMOKE_BROWSER_WARNING_BUDGET',
    'ctox-sync-production-readiness-95-default-matrix.json',
    'ctox-sync-production-readiness-95-business-os-matrix.json',
    'businessOsProductionSmokeModes',
    '--smoke-binary',
  ]) {
    if (!source.includes(token)) throw new Error(`missing token ${token}`);
  }
  console.log(`ctox_sync_production_readiness_95_full_matrix_self_test=1 business_modes=${businessOsProductionSmokeModes.length}`);
}

function fixtureMatrix(modes) {
  return {
    schema: 'ctox.business_os.smoke_matrix_summary.v1',
    ok: true,
    requestedModes: modes,
    configuration: { attempts: 1 },
    source: {
      artifactHashes: {
        browserBundleSha256: 'a'.repeat(64),
        smokeBinarySha256: 'b'.repeat(64),
      },
    },
  };
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function flagValue(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return '';
  return process.argv[index + 1] || '';
}
