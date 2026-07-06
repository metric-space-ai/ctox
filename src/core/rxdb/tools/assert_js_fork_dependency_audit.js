#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const appLocalRoot = path.join(repoRoot, 'src/apps/business-os/rxdb');
const offenders = [];

for (const legacy of [
  path.join(rxdbRoot, 'js-fork'),
  path.join(repoRoot, 'src/apps/business-os/vendor/rxdb-bundle.mjs'),
  path.join(repoRoot, 'src/apps/business-os/vendor/rxdb-bundle.provenance.json'),
]) {
  if (fs.existsSync(legacy)) {
    offenders.push(`${relative(legacy)}: legacy npm/TS RxDB fork artifact must be removed`);
  }
}

for (const file of walk(appLocalRoot)) {
  const basename = path.basename(file);
  if (['package.json', 'package-lock.json', 'npm-shrinkwrap.json', 'pnpm-lock.yaml', 'yarn.lock'].includes(basename)) {
    offenders.push(`${relative(file)}: package-manager file is not allowed in app-local CTOX Sync Engine`);
  }
  if (file.split(path.sep).includes('node_modules')) {
    offenders.push(`${relative(file)}: dependency tree is not allowed in app-local CTOX Sync Engine`);
  }
}

if (offenders.length) {
  console.error(`ctox-db dependency-surface guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-db dependency-surface guard OK');

function walk(dir) {
  if (!fs.existsSync(dir)) return [];
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(file));
    else out.push(file);
  }
  return out;
}

function relative(file) {
  return path.relative(repoRoot, file);
}
