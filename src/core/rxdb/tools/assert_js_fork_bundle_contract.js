#!/usr/bin/env node
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const contractPath = path.join(rxdbRoot, 'js-fork/bundle-contract.json');
const offenders = [];

const contract = readJson(contractPath, 'bundle contract');
if (contract) assertContract(contract);

if (offenders.length) {
  console.error(`ctox-rxdb-js bundle contract guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-rxdb-js bundle contract guard OK');

function assertContract(parsed) {
  const sourcePackagePath = resolveRxdb(parsed.source_package);
  const sourceLockfilePath = resolveRxdb(parsed.source_lockfile);
  const bundlePath = resolveRxdb(parsed.output_path);
  const provenancePath = resolveRxdb(parsed.provenance_path);
  const sourcePackage = readJson(sourcePackagePath, 'source package');
  const sourceLockfile = readJson(sourceLockfilePath, 'source lockfile');
  const provenance = readJson(provenancePath, 'bundle provenance');

  if (parsed.name !== 'ctox-rxdb-js-browser-bundle') {
    offenders.push(`${relative(contractPath)}: unexpected contract name`);
  }
  if (parsed.fork_package !== 'ctox-rxdb-js') {
    offenders.push(`${relative(contractPath)}: fork_package must be ctox-rxdb-js`);
  }
  if (parsed.upstream_version !== '16.20.0') {
    offenders.push(`${relative(contractPath)}: upstream_version must remain pinned to 16.20.0`);
  }
  if (parsed.protocol !== 'ctox-rxdb-protocol-v1') {
    offenders.push(`${relative(contractPath)}: protocol must be ctox-rxdb-protocol-v1`);
  }
  if (parsed.package_manager !== 'npm-ci') {
    offenders.push(`${relative(contractPath)}: package_manager must be npm-ci`);
  }
  if (parsed.publish_policy !== 'private-package-only') {
    offenders.push(`${relative(contractPath)}: publish_policy must be private-package-only`);
  }
  if (parsed.version_discipline !== 'upstream-version-pinned-with-ctox-provenance') {
    offenders.push(`${relative(contractPath)}: version_discipline must pin upstream version with CTOX provenance`);
  }

  if (sourcePackage) {
    if (sourcePackage.name !== parsed.fork_package) {
      offenders.push(`${relative(sourcePackagePath)}: package name must be ${parsed.fork_package}`);
    }
    if (sourcePackage.version !== parsed.upstream_version) {
      offenders.push(`${relative(sourcePackagePath)}: package version must match ${parsed.upstream_version}`);
    }
    if (sourcePackage.private !== true) {
      offenders.push(`${relative(sourcePackagePath)}: package must be private to prevent accidental npm publish`);
    }
    if (sourcePackage.publishConfig?.access !== 'restricted') {
      offenders.push(`${relative(sourcePackagePath)}: publishConfig.access must be restricted`);
    }
    if (!/^https:\/\/github\.com\/metric-space-ai\/ctox\.git$/.test(sourcePackage.repository?.url || '')) {
      offenders.push(`${relative(sourcePackagePath)}: repository must point at the CTOX fork repository`);
    }
    if (sourcePackage.repository?.directory !== 'src/core/rxdb/js-fork/source') {
      offenders.push(`${relative(sourcePackagePath)}: repository.directory must point at the hard-fork source path`);
    }
    if (sourcePackage.homepage !== 'https://ctox.dev/') {
      offenders.push(`${relative(sourcePackagePath)}: homepage must point at ctox.dev, not upstream RxDB`);
    }
    if (sourcePackage.ctoxHardFork?.upstream?.version !== parsed.upstream_version) {
      offenders.push(`${relative(sourcePackagePath)}: missing ctoxHardFork upstream version provenance`);
    }
    if (sourcePackage.ctoxHardFork?.publishPolicy?.npm !== parsed.publish_policy) {
      offenders.push(`${relative(sourcePackagePath)}: ctoxHardFork.publishPolicy.npm must match bundle contract`);
    }
    if (sourcePackage.scripts?.postinstall !== 'node -e "process.exit(0)"') {
      offenders.push(`${relative(sourcePackagePath)}: postinstall must be a deterministic no-op for npm ci`);
    }
  }
  if (sourceLockfile) {
    if (sourceLockfile.name !== parsed.fork_package) {
      offenders.push(`${relative(sourceLockfilePath)}: lockfile name must be ${parsed.fork_package}`);
    }
    if (sourceLockfile.lockfileVersion !== 3) {
      offenders.push(`${relative(sourceLockfilePath)}: lockfileVersion must be 3 for reproducible npm ci installs`);
    }
    if (sourceLockfile.packages?.['']?.name !== parsed.fork_package) {
      offenders.push(`${relative(sourceLockfilePath)}: root package metadata must name ${parsed.fork_package}`);
    }
    if (sourceLockfile.packages?.['']?.version !== parsed.upstream_version) {
      offenders.push(`${relative(sourceLockfilePath)}: root package metadata must preserve upstream baseline ${parsed.upstream_version}`);
    }
  }

  const bundle = readText(bundlePath, 'browser bundle');
  if (bundle) {
    const exportLine = bundle.match(/export\s+\{[^}]+\}/s)?.[0] || '';
    for (const name of parsed.expected_exports || []) {
      if (!exportLine.includes(name)) {
        offenders.push(`${relative(bundlePath)}: missing expected export ${name}`);
      }
    }
  }

  if (provenance) {
    if (provenance.name !== parsed.name) {
      offenders.push(`${relative(provenancePath)}: provenance name must match contract`);
    }
    if (provenance.source_name !== parsed.fork_package) {
      offenders.push(`${relative(provenancePath)}: source_name must be ${parsed.fork_package}`);
    }
    if (provenance.source_version !== parsed.upstream_version) {
      offenders.push(`${relative(provenancePath)}: source_version must be ${parsed.upstream_version}`);
    }
    if (provenance.protocol !== parsed.protocol) {
      offenders.push(`${relative(provenancePath)}: protocol must match contract`);
    }
    if (provenance.package_manager !== parsed.package_manager) {
      offenders.push(`${relative(provenancePath)}: package_manager must match contract`);
    }
    if (provenance.publish_policy !== parsed.publish_policy) {
      offenders.push(`${relative(provenancePath)}: publish_policy must match contract`);
    }
    if (provenance.version_discipline !== parsed.version_discipline) {
      offenders.push(`${relative(provenancePath)}: version_discipline must match contract`);
    }
    if (provenance.source_lockfile !== path.relative(repoRoot, sourceLockfilePath)) {
      offenders.push(`${relative(provenancePath)}: source_lockfile must point at the fork package lockfile`);
    }
    if (provenance.source_lock_sha256 !== sha256(sourceLockfilePath)) {
      offenders.push(`${relative(provenancePath)}: source_lock_sha256 is stale; expected ${sha256(sourceLockfilePath)}`);
    }
    const hash = sha256(bundlePath);
    if (provenance.bundle_sha256 !== hash) {
      offenders.push(`${relative(provenancePath)}: bundle_sha256 is stale; expected ${hash}`);
    }
    for (const name of parsed.expected_exports || []) {
      if (!provenance.expected_exports?.includes(name)) {
        offenders.push(`${relative(provenancePath)}: missing expected export ${name}`);
      }
    }
  }

  for (const importer of parsed.expected_importers || []) {
    const importerPath = path.join(repoRoot, importer);
    const importerSource = readText(importerPath, 'expected importer');
    if (!importerSource) continue;
    if (!/vendor\/rxdb-bundle\.mjs/.test(importerSource)) {
      offenders.push(`${relative(importerPath)}: does not import the guarded rxdb-bundle.mjs`);
    }
  }
}

function resolveRxdb(relativePath) {
  return path.resolve(rxdbRoot, relativePath);
}

function readJson(file, label) {
  if (!fs.existsSync(file)) {
    offenders.push(`${relative(file)}: missing ${label}`);
    return null;
  }
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    offenders.push(`${relative(file)}: invalid JSON: ${error.message}`);
    return null;
  }
}

function readText(file, label) {
  if (!fs.existsSync(file)) {
    offenders.push(`${relative(file)}: missing ${label}`);
    return null;
  }
  return fs.readFileSync(file, 'utf8');
}

function sha256(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function relative(file) {
  return path.relative(repoRoot, file);
}
