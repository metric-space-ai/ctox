#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const ciPath = path.join(repoRoot, '.github/workflows/ci.yml');
const releasePath = path.join(repoRoot, '.github/workflows/release.yml');
const offenders = [];

const requiredEvidence = [
  'runtime/build/ctox-rxdb-js/ctox-rxdb-js.mjs',
  'runtime/build/ctox-rxdb-js/ctox-rxdb-js.provenance.json',
  'src/apps/business-os/rxdb/manifest.json',
  'src/apps/business-os/rxdb/README.md',
  'src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs',
  'src/apps/business-os/rxdb/src/index.mjs',
  'src/apps/business-os/rxdb/src/replication-webrtc.mjs',
  'src/apps/business-os/rxdb/src/webrtc-native.mjs',
  'src/apps/business-os/rxdb/src/storage-indexeddb.mjs',
  'src/apps/business-os/rxdb/tests/no-package-manager-import-smoke.mjs',
  'src/apps/business-os/rxdb/tests/query-api-smoke.mjs',
  'src/apps/business-os/rxdb/tests/storage-index-smoke.mjs',
];

const forbiddenLegacyEvidence = [
  'src/core/rxdb/js-fork',
  'src/apps/business-os/vendor/rxdb-bundle.mjs',
  'src/apps/business-os/vendor/rxdb-bundle.provenance.json',
  'src/core/rxdb/js-fork/source/package-lock.json',
  'npm --prefix src/core/rxdb/js-fork/source ci',
];

const unixReleaseEvidence = [
  'src/apps/business-os/rxdb/manifest.json bundle/rxdb-js/app-local/',
  'src/apps/business-os/rxdb/README.md bundle/rxdb-js/app-local/',
  'src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs bundle/rxdb-js/app-local/dist/',
  'src/apps/business-os/rxdb/src/*.mjs bundle/rxdb-js/app-local/src/',
  'src/apps/business-os/rxdb/tests/*.mjs bundle/rxdb-js/app-local/tests/',
];

const windowsReleaseEvidence = [
  '"src\\apps\\business-os\\rxdb\\manifest.json" -Destination "bundle\\rxdb-js\\app-local\\"',
  '"src\\apps\\business-os\\rxdb\\README.md" -Destination "bundle\\rxdb-js\\app-local\\"',
  '"src\\apps\\business-os\\rxdb\\dist\\ctox-rxdb-js.mjs" -Destination "bundle\\rxdb-js\\app-local\\dist\\"',
  '"src\\apps\\business-os\\rxdb\\src\\*.mjs" -Destination "bundle\\rxdb-js\\app-local\\src\\"',
  '"src\\apps\\business-os\\rxdb\\tests\\*.mjs" -Destination "bundle\\rxdb-js\\app-local\\tests\\"',
];

const ci = read(ciPath);
const release = read(releasePath);

if (ci) {
  assertContains(ciPath, ci, 'ctox-db app-local evidence build', 'node src/scripts/vendor-builds/build-ctox-rxdb-js.mjs --artifact-dir runtime/build/ctox-rxdb-js --write-provenance');
  assertContains(ciPath, ci, 'release evidence guard syntax check', 'node --check src/core/rxdb/tools/assert_js_fork_release_evidence.js');
  assertContains(ciPath, ci, 'release evidence guard execution', 'node src/core/rxdb/tools/assert_js_fork_release_evidence.js');
  for (const item of requiredEvidence) {
    assertContains(ciPath, ci, `uploaded CTOX DB evidence ${item}`, item);
  }
  for (const item of forbiddenLegacyEvidence) {
    assertNotContains(ciPath, ci, `legacy RxDB fork evidence ${item}`, item);
  }
}

if (release) {
  assertContains(releasePath, release, 'Unix app-local rxdb-js evidence directory', 'mkdir -p bundle/rxdb-js/app-local/dist');
  assertContains(releasePath, release, 'Unix app-local rxdb-js source directory', 'mkdir -p bundle/rxdb-js/app-local/src');
  assertContains(releasePath, release, 'Unix app-local rxdb-js test directory', 'mkdir -p bundle/rxdb-js/app-local/tests');
  assertContains(releasePath, release, 'Windows app-local rxdb-js evidence directory', 'New-Item -ItemType Directory -Force bundle\\rxdb-js\\app-local\\dist');
  assertContains(releasePath, release, 'Windows app-local rxdb-js source directory', 'New-Item -ItemType Directory -Force bundle\\rxdb-js\\app-local\\src');
  assertContains(releasePath, release, 'Windows app-local rxdb-js test directory', 'New-Item -ItemType Directory -Force bundle\\rxdb-js\\app-local\\tests');
  for (const item of unixReleaseEvidence) {
    assertContains(releasePath, release, `Unix release evidence ${item}`, item);
  }
  for (const item of windowsReleaseEvidence) {
    assertContains(releasePath, release, `Windows release evidence ${item}`, item);
  }
  for (const item of forbiddenLegacyEvidence) {
    assertNotContains(releasePath, release, `legacy RxDB fork evidence ${item}`, item);
  }
}

for (const item of requiredEvidence.slice(2)) {
  const file = path.join(repoRoot, item);
  if (!fs.existsSync(file)) {
    offenders.push(`${relative(file)}: release evidence source file missing`);
  }
}

if (offenders.length) {
  console.error(`ctox-db release evidence guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-db release evidence guard OK');

function read(file) {
  try {
    return fs.readFileSync(file, 'utf8');
  } catch (error) {
    offenders.push(`${relative(file)}: ${error.message}`);
    return '';
  }
}

function assertContains(file, content, label, needle) {
  if (!content.includes(needle)) {
    offenders.push(`${relative(file)}: missing ${label}`);
  }
}

function assertNotContains(file, content, label, needle) {
  if (content.includes(needle)) {
    offenders.push(`${relative(file)}: must not contain ${label}`);
  }
}

function relative(file) {
  return path.relative(repoRoot, file);
}
