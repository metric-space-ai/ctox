#!/usr/bin/env node
/*
 * Guard the manual RxDB WebRTC soak workflow against drift from the runner.
 */
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '../../../..');
const runnerPath = path.join(__dirname, 'browser_rust_soak.js');
const workflowPath = path.join(root, '.github/workflows/rxdb-soak.yml');
const readinessWorkflowPath = path.join(root, '.github/workflows/rxdb-production-readiness.yml');

const runner = fs.readFileSync(runnerPath, 'utf8');
const workflow = fs.readFileSync(workflowPath, 'utf8');
const readinessWorkflow = fs.readFileSync(readinessWorkflowPath, 'utf8');
const {
  businessOsProductionSmokeModes,
} = require('./business_os_production_smoke_registry');
const runnerModes = extractRunnerModes(runner);
const matrix = fs.readFileSync(path.join(__dirname, 'browser_rust_smoke_matrix.js'), 'utf8');
const matrixModes = extractMatrixModes(matrix);
const evidenceModes = extractMatrixEvidenceModes(matrix);
const smokeModes = extractSmokeModes(fs.readFileSync(path.join(__dirname, 'browser_rust_smoke.js'), 'utf8'));
const workflowDefaultModes = extractWorkflowDefaultModes(workflow);
const workflowRequiredModes = extractWorkflowRequiredModes(workflow);

assertAtLeast('runner default modes', runnerModes, 33);
assertUnique('runner default modes', runnerModes);
assertUnique('smoke matrix default modes', matrixModes);
assertUnique('smoke matrix evidence modes', evidenceModes);
assertUnique('smoke harness supported modes', smokeModes);
assertUnique('workflow default modes', workflowDefaultModes);
assertUnique('workflow required modes', workflowRequiredModes);
assertSameList('workflow default modes', workflowDefaultModes, runnerModes);
assertSameList('workflow required modes', workflowRequiredModes, runnerModes);
assertContainsAll('smoke matrix default modes', matrixModes, runnerModes);
assertContainsAll('smoke harness supported modes', smokeModes, runnerModes);
assertContainsAll('smoke harness supported modes', smokeModes, matrixModes);
assertContainsAll('smoke matrix evidence requirements', evidenceModes, matrixModes);
assertContainsAll('smoke harness supported modes', smokeModes, evidenceModes);
assertIncludes(
  workflow,
  "SOAK_MIN_CYCLES: ${{ (github.event_name == 'schedule' || inputs.require_release_coverage == 'true') && '3' || '' }}",
  'workflow must require at least three cycles when release coverage is enabled',
);
assertIncludes(
  workflow,
  "SOAK_FAIL_ON_RETRY: ${{ (github.event_name == 'schedule' || inputs.require_release_coverage == 'true' || inputs.fail_on_retry == 'true') && '1' || '0' }}",
  'workflow must force fail-on-retry when release coverage is enabled',
);
assertIncludes(workflow, 'workflow_call:', 'soak workflow must be reusable by the release workflow');
assertIncludes(workflow, 'schedule:', 'soak workflow must run nightly');
assertIncludes(workflow, 'timeout-minutes: 360', 'nightly nine-cycle soak needs a production-readiness timeout budget');
assertIncludes(workflow, "github.event_name == 'schedule' && '9'", 'nightly soak must run nine cycles');
assertIncludes(
  workflow,
  "github.event_name == 'schedule' && 'runtime/build/ctox-sync-production-readiness-95-nightly-soak.json' || 'rxdb-soak-summary.json'",
  'nightly soak must write the production-readiness artifact path while release soak keeps rxdb-soak-summary.json',
);
assertIncludes(
  workflow,
  'require_release_coverage:\n        description: "Fail unless all release smoke modes are included"\n        required: true\n        default: "true"',
  'workflow must default release coverage enforcement to true',
);
assertIncludes(
  workflow,
  '- name: Print soak evidence summary\n        if: always()',
  'workflow must always print the soak evidence summary',
);
assertIncludes(
  workflow,
  '- name: Upload soak summary\n        if: always()',
  'workflow must always upload the soak summary artifact',
);

console.log(`rxdb soak workflow guard OK: modes=${runnerModes.length}`);

function extractRunnerModes(source) {
  const match = source.match(/const defaultSoakModes = \[([\s\S]*?)\];/);
  if (!match) fail('defaultSoakModes array not found in browser_rust_soak.js');
  const modes = [...match[1].matchAll(/'([^']+)'/g)].map((entry) => entry[1]);
  if (!modes.length) fail('defaultSoakModes array is empty');
  return modes;
}

function extractMatrixModes(source) {
  const match = source.match(/const defaultModes = \[([\s\S]*?)\];/);
  if (!match) fail('defaultModes array not found in browser_rust_smoke_matrix.js');
  const modes = [...match[1].matchAll(/'([^']+)'/g)].map((entry) => entry[1]);
  if (!modes.length) fail('defaultModes array is empty');
  return modes;
}

function extractMatrixEvidenceModes(source) {
  const match = source.match(/const modeEvidenceRequirements = \{([\s\S]*?)\};\nconst modes = /);
  if (!match) fail('modeEvidenceRequirements map not found in browser_rust_smoke_matrix.js');
  const modes = [...match[1].matchAll(/^\s+'([^']+)':\s*\{/gm)].map((entry) => entry[1]);
  if (match[1].includes('...businessOsProductionSmokeEvidenceRequirements')) {
    modes.push(...businessOsProductionSmokeModes);
  }
  if (!modes.length) fail('modeEvidenceRequirements map is empty');
  return modes;
}

function extractSmokeModes(source) {
  const match = source.match(/const supportedSmokeModes = \[([\s\S]*?)\];/);
  if (!match) fail('supportedSmokeModes array not found in browser_rust_smoke.js');
  const modes = [...match[1].matchAll(/'([^']+)'/g)].map((entry) => entry[1]);
  if (match[1].includes('...businessOsProductionSmokeModes')) {
    modes.push(...businessOsProductionSmokeModes);
  }
  if (!modes.length) fail('supported SMOKE_MODE list is empty');
  return modes;
}

function extractWorkflowDefaultModes(source) {
  const match = source.match(/modes:\n(?:.*\n){0,8}?\s+default:\s*"([^"]+)"/);
  if (!match) fail('workflow modes.default list not found');
  return splitModes(match[1]);
}

function extractWorkflowRequiredModes(source) {
  const match = source.match(/SOAK_REQUIRED_MODES:\s*\$\{\{[^\n]*&& '([^']+)' \|\| '' \}\}/);
  if (!match) fail('workflow SOAK_REQUIRED_MODES release list not found');
  return splitModes(match[1]);
}

function splitModes(value) {
  return String(value)
    .split(',')
    .map((mode) => mode.trim())
    .filter(Boolean);
}

function assertSameList(label, actual, expected) {
  const actualJoined = actual.join(',');
  const expectedJoined = expected.join(',');
  if (actualJoined !== expectedJoined) {
    fail(`${label} drifted\nexpected: ${expectedJoined}\nactual:   ${actualJoined}`);
  }
}

function assertContainsAll(label, actual, expected) {
  const actualSet = new Set(actual);
  const missing = expected.filter((entry) => !actualSet.has(entry));
  if (missing.length) {
    fail(`${label} is missing release soak mode(s): ${missing.join(', ')}`);
  }
}

function assertUnique(label, values) {
  const seen = new Set();
  const duplicates = [];
  for (const value of values) {
    if (seen.has(value)) duplicates.push(value);
    seen.add(value);
  }
  if (duplicates.length) {
    fail(`${label} contains duplicate mode(s): ${duplicates.join(', ')}`);
  }
}

function assertAtLeast(label, values, minimum) {
  if (values.length < minimum) {
    fail(`${label} coverage regressed below ${minimum}: got ${values.length}`);
  }
}

function assertIncludes(haystack, needle, message) {
  if (!haystack.includes(needle)) fail(message);
}

function assertPlaywrightInstallIsOutsideRepository(source, label) {
  assertIncludes(
    source,
    'mkdir -p /tmp/ctox-pw-smoke\n          cd /tmp/ctox-pw-smoke\n          npm init -y',
    `${label} must initialize its smoke-only npm project from /tmp`,
  );
  if (source.includes('npm --prefix /tmp/ctox-pw-smoke init')) {
    fail(`${label} must not use npm init --prefix because npm can write package.json in the repository`);
  }
}

function fail(message) {
  console.error(`rxdb soak workflow guard failed: ${message}`);
  process.exit(1);
}
