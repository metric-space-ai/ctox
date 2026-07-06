#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const manifestPath = path.join(repoRoot, 'src/apps/business-os/rxdb/manifest.json');
const distPath = path.join(repoRoot, 'src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs');
const offenders = [];

const manifest = readJson(manifestPath);
if (manifest) {
  if (manifest.name !== 'ctox-rxdb-js') offenders.push(`${relative(manifestPath)}: name must be ctox-rxdb-js`);
  if (manifest.public_name !== 'CTOX Sync Engine') offenders.push(`${relative(manifestPath)}: public_name must be CTOX Sync Engine`);
  if (manifest.package_manager !== 'none') offenders.push(`${relative(manifestPath)}: package_manager must be none`);
  if (manifest.api_contract !== 'ctox-db-business-os-v1') offenders.push(`${relative(manifestPath)}: api_contract must be ctox-db-business-os-v1`);
  if (manifest.upstream_compatible !== false || manifest.upstream_compatibility !== 'not-upstream-rxdb') {
    offenders.push(`${relative(manifestPath)}: upstream compatibility marker is invalid`);
  }
  if (manifest.entry !== 'dist/ctox-rxdb-js.mjs') offenders.push(`${relative(manifestPath)}: entry must be dist/ctox-rxdb-js.mjs`);
}

const dist = readText(distPath);
if (dist) {
  for (const name of [
    'createRxDatabase',
    'getCtoxIndexedDbStorage',
    'replicateWebRTC',
    'getConnectionHandlerSimplePeer',
    'buildBusinessOsAdvancedStatus',
  ]) {
    if (!new RegExp(`\\b${name}\\b`).test(dist)) {
      offenders.push(`${relative(distPath)}: missing expected CTOX Sync Engine export ${name}`);
    }
  }
  for (const forbidden of ['simple-peer', 'Dexie', 'npm', 'premium access', 'rxdb-premium']) {
    if (dist.includes(forbidden)) offenders.push(`${relative(distPath)}: forbidden legacy token ${forbidden}`);
  }
}

for (const legacy of [
  path.join(rxdbRoot, 'js-fork'),
  path.join(repoRoot, 'src/apps/business-os/vendor/rxdb-bundle.mjs'),
  path.join(repoRoot, 'src/apps/business-os/vendor/rxdb-bundle.provenance.json'),
]) {
  if (fs.existsSync(legacy)) {
    offenders.push(`${relative(legacy)}: legacy generated bundle contract must not be active`);
  }
}

if (offenders.length) {
  console.error(`ctox-db bundle contract guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-db bundle contract guard OK');

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    offenders.push(`${relative(file)}: invalid JSON: ${error.message}`);
    return null;
  }
}

function readText(file) {
  try {
    return fs.readFileSync(file, 'utf8');
  } catch (error) {
    offenders.push(`${relative(file)}: ${error.message}`);
    return '';
  }
}

function relative(file) {
  return path.relative(repoRoot, file);
}
