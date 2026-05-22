#!/usr/bin/env node
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const rxdbRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(rxdbRoot, '..', '..', '..');
const baselinePath = path.join(rxdbRoot, 'js-fork/dependency-audit-baseline.json');
const baseline = readJson(baselinePath);
const offenders = [];

const sourceLockfilePath = path.join(rxdbRoot, baseline.source_lockfile);
const sourcePackageDir = path.join(rxdbRoot, 'js-fork/source');

if (!fs.existsSync(sourceLockfilePath)) {
  offenders.push(`${relative(sourceLockfilePath)}: missing source lockfile`);
} else {
  const hash = sha256(sourceLockfilePath);
  if (hash !== baseline.source_lock_sha256) {
    offenders.push(`${relative(sourceLockfilePath)}: lockfile hash changed; update dependency-audit-baseline.json after audit triage (${hash})`);
  }
}

const audit = runAudit();
if (audit) {
  assertSeverityBudget(audit.metadata?.vulnerabilities || {});
  assertDependencyBudget(audit.metadata?.dependencies || {});
  assertDirectVulnerablePackages(audit.vulnerabilities || {});
}

if (offenders.length) {
  console.error(`ctox-rxdb-js dependency audit guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('ctox-rxdb-js dependency audit guard OK');

function runAudit() {
  const result = spawnSync('npm', ['audit', '--json'], {
    cwd: sourcePackageDir,
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  });
  const output = result.stdout || result.stderr;
  if (!output) {
    offenders.push('npm audit produced no JSON output');
    return null;
  }
  try {
    return JSON.parse(output);
  } catch (error) {
    offenders.push(`npm audit JSON parse failed: ${error.message}`);
    return null;
  }
}

function assertSeverityBudget(actual) {
  for (const [name, budget] of Object.entries(baseline.severity_budget || {})) {
    const value = Number(actual[name] || 0);
    if (value > budget) {
      offenders.push(`npm audit ${name} count grew from ${budget} to ${value}`);
    }
  }
}

function assertDependencyBudget(actual) {
  for (const [name, budget] of Object.entries(baseline.dependency_budget || {})) {
    const value = Number(actual[name] || 0);
    if (value > budget) {
      offenders.push(`npm dependency ${name} count grew from ${budget} to ${value}`);
    }
  }
}

function assertDirectVulnerablePackages(vulnerabilities) {
  const allowed = new Set(baseline.known_direct_vulnerable_packages || []);
  const actual = Object.values(vulnerabilities)
    .filter((entry) => entry?.isDirect)
    .map((entry) => entry.name)
    .sort();
  const unknown = actual.filter((name) => !allowed.has(name));
  if (unknown.length) {
    offenders.push(`new direct vulnerable package(s): ${unknown.join(', ')}`);
  }
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    console.error(`${relative(file)}: invalid JSON: ${error.message}`);
    process.exit(1);
  }
}

function sha256(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function relative(file) {
  return path.relative(repoRoot, file);
}
