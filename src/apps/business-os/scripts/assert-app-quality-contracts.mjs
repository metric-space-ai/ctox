#!/usr/bin/env node
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadBusinessOsAppInventory } from './business-os-app-inventory.mjs';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const contract = JSON.parse(readFileSync(join(appRoot, 'qa/app-quality-contracts.json'), 'utf8'));
const inventory = loadBusinessOsAppInventory();
const expected = inventory.sourceApps.map((app) => app.id).sort();
const actual = contract.apps.map((app) => app.id).sort();
const allowedArchetypes = new Set([
  'record-workbench', 'queue-workflow', 'editor-document', 'automation', 'timeline-thread', 'shell-exception',
]);

assert.equal(contract.schema, 'ctox.business_os.app_quality_contracts.v1');
assert.deepEqual(actual, expected, 'per-app quality contracts must cover exactly the 34 source apps');
assert.equal(new Set(actual).size, 34, 'per-app quality contract ids must be unique');
assert.ok(Array.isArray(contract.common_required_evidence) && contract.common_required_evidence.length >= 9);

for (const app of contract.apps) {
  assert.ok(allowedArchetypes.has(app.archetype), `${app.id}: unknown archetype ${app.archetype}`);
  assert.ok(String(app.variant || '').trim(), `${app.id}: variant is required`);
  assert.ok(String(app.business_story || '').trim().length >= 24, `${app.id}: concrete business story is required`);
  assert.ok(Array.isArray(app.required_actions) && app.required_actions.length >= 3, `${app.id}: required actions are incomplete`);
  assert.equal(new Set(app.required_actions).size, app.required_actions.length, `${app.id}: duplicate required action`);
  const moduleDir = join(appRoot, 'modules', app.id);
  assert.ok(existsSync(join(moduleDir, 'module.json')), `${app.id}: module manifest is missing`);
  assert.ok(existsSync(join(moduleDir, 'index.html')), `${app.id}: index.html is missing`);
  assert.ok(existsSync(join(moduleDir, 'index.js')), `${app.id}: index.js is missing`);
  assert.ok(existsSync(join(moduleDir, 'index.css')), `${app.id}: index.css is missing`);
}

console.log(`Business OS per-app quality contracts OK: ${contract.apps.length}/34 apps with named archetype, variant, story and actions`);
