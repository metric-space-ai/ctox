#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const appLocalRoot = path.join(repoRoot, 'src/apps/business-os/rxdb');
const offenders = [];

for (const legacy of [
  repoPath('src/core/rxdb/js-fork'),
  repoPath('src/apps/business-os/vendor/rxdb-bundle.mjs'),
  repoPath('src/apps/business-os/vendor/rxdb-bundle.provenance.json'),
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
    const file = childPath(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(file));
    else out.push(file);
  }
  return out;
}

function repoPath(relativePath) {
  return safeRelativePath(repoRoot, relativePath);
}

function safeRelativePath(base, relativePath) {
  const value = String(relativePath || '');
  if (!value || path.isAbsolute(value)) {
    throw new Error(`unsafe relative path: ${JSON.stringify(relativePath)}`);
  }
  const normalized = path.normalize(value);
  if (normalized === '..' || normalized.startsWith(`..${path.sep}`)) {
    throw new Error(`path escapes base: ${JSON.stringify(relativePath)}`);
  }
  return `${base}${path.sep}${normalized}`;
}

function childPath(dir, entryName) {
  const name = String(entryName || '');
  if (!name || name === '.' || name === '..' || name.includes('/') || name.includes('\\')) {
    throw new Error(`unsafe directory entry: ${JSON.stringify(entryName)}`);
  }
  return `${dir}${path.sep}${name}`;
}

function relative(file) {
  return path.relative(repoRoot, file);
}
