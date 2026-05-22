#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const ciPath = path.join(repoRoot, '.github/workflows/ci.yml');
const releasePath = path.join(repoRoot, '.github/workflows/release.yml');
const offenders = [];

const requiredEvidence = [
  'runtime/build/ctox-rxdb-js/rxdb-bundle.mjs',
  'runtime/build/ctox-rxdb-js/rxdb-bundle.provenance.json',
  'src/core/rxdb/js-fork/bundle-contract.json',
  'src/core/rxdb/js-fork/dependency-audit-baseline.json',
  'src/core/rxdb/js-fork/ctox-rxdb-js.manifest.json',
  'src/core/rxdb/js-fork/source/package.json',
  'src/core/rxdb/js-fork/source/package-lock.json',
];

const unixReleaseEvidence = [
  'src/apps/business-os/vendor/rxdb-bundle.provenance.json bundle/rxdb-js/',
  'src/core/rxdb/js-fork/bundle-contract.json bundle/rxdb-js/',
  'src/core/rxdb/js-fork/dependency-audit-baseline.json bundle/rxdb-js/',
  'src/core/rxdb/js-fork/ctox-rxdb-js.manifest.json bundle/rxdb-js/',
  'src/core/rxdb/js-fork/source/package.json bundle/rxdb-js/source/',
  'src/core/rxdb/js-fork/source/package-lock.json bundle/rxdb-js/source/',
];

const windowsReleaseEvidence = [
  '"src\\apps\\business-os\\vendor\\rxdb-bundle.provenance.json" -Destination "bundle\\rxdb-js\\"',
  '"src\\core\\rxdb\\js-fork\\bundle-contract.json" -Destination "bundle\\rxdb-js\\"',
  '"src\\core\\rxdb\\js-fork\\dependency-audit-baseline.json" -Destination "bundle\\rxdb-js\\"',
  '"src\\core\\rxdb\\js-fork\\ctox-rxdb-js.manifest.json" -Destination "bundle\\rxdb-js\\"',
  '"src\\core\\rxdb\\js-fork\\source\\package.json" -Destination "bundle\\rxdb-js\\source\\"',
  '"src\\core\\rxdb\\js-fork\\source\\package-lock.json" -Destination "bundle\\rxdb-js\\source\\"',
];

const ci = read(ciPath);
const release = read(releasePath);

if (ci) {
  assertContains(ciPath, ci, 'ctox-rxdb-js bundle provenance', 'node src/scripts/vendor-builds/build-ctox-rxdb-js.mjs --artifact-dir runtime/build/ctox-rxdb-js --write-provenance');
  assertContains(ciPath, ci, 'release evidence guard syntax check', 'node --check src/core/rxdb/tools/assert_js_fork_release_evidence.js');
  assertContains(ciPath, ci, 'release evidence guard execution', 'node src/core/rxdb/tools/assert_js_fork_release_evidence.js');
  for (const item of requiredEvidence) {
    assertContains(ciPath, ci, `uploaded bundle evidence ${item}`, item);
  }
}

if (release) {
  assertContains(releasePath, release, 'Unix rxdb-js evidence directory', 'mkdir -p bundle/rxdb-js/source');
  assertContains(releasePath, release, 'Windows rxdb-js evidence directory', 'New-Item -ItemType Directory -Force bundle\\rxdb-js\\source');
  for (const item of unixReleaseEvidence) {
    assertContains(releasePath, release, `Unix release evidence ${item}`, item);
  }
  for (const item of windowsReleaseEvidence) {
    assertContains(releasePath, release, `Windows release evidence ${item}`, item);
  }
}

for (const item of requiredEvidence.slice(2)) {
  const file = path.join(repoRoot, item);
  if (!fs.existsSync(file)) {
    offenders.push(`${relative(file)}: release evidence source file missing`);
  }
}

if (offenders.length) {
  console.error(`ctox-rxdb-js release evidence guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-rxdb-js release evidence guard OK');

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

function relative(file) {
  return path.relative(repoRoot, file);
}
