#!/usr/bin/env node
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const source = readFileSync(fileURLToPath(new URL('./smoke-app-module.mjs', import.meta.url)), 'utf8');
const e2eSource = readFileSync(fileURLToPath(new URL('./e2e-app-module.mjs', import.meta.url)), 'utf8');

test('generic smoke treats a visible mounted non-CRUD app as valid', () => {
  assert.match(source, /No primary create action is required for a generic mount smoke/);
  assert.doesNotMatch(source, /result\.failures\.push\('no visible primary create action found under module root'\)/);
  assert.match(source, /timeout: Math\.min\(timeoutMs, 3000\)/);
  assert.match(source, /return null;/);
});

test('generic smoke waits for the replicated catalog and rejects the recovery surface', () => {
  assert.match(source, /app\.modules\?\.find\?\.\(\(module\) => module\.id === id\)/);
  assert.match(source, /root\?\.dataset\.moduleReady === 'true'/);
  assert.match(source, /root\?\.dataset\.moduleLoadFailed === 'true'/);
  assert.match(source, /qaModuleMountFailures/);
  assert.match(source, /ctoxBusinessOsSmoke\?\.state\?\.qaModuleMountFailures/);
  assert.match(source, /history\.replaceState/);
  assert.match(source, /searchParams\.set\('rxdbSmoke', '1'\)/);
  assert.match(source, /execution context was destroyed\|cannot find context/);
  assert.match(source, /Business OS shell did not expose module/);
  assert.match(source, /\.desktop-icon\[data-target=/);
  assert.match(source, /launcher\.click\(\)/);
});

test('declared-scenario E2E uses the same deterministic mount gate', () => {
  assert.match(e2eSource, /app\.modules\?\.find\?\.\(\(module\) => module\.id === id\)/);
  assert.match(e2eSource, /root\?\.dataset\.moduleReady === 'true'/);
  assert.match(e2eSource, /root\?\.dataset\.moduleLoadFailed === 'true'/);
  assert.match(e2eSource, /qaModuleMountFailures/);
  assert.match(e2eSource, /ctoxBusinessOsSmoke\?\.state\?\.qaModuleMountFailures/);
  assert.match(e2eSource, /history\.replaceState/);
  assert.match(e2eSource, /searchParams\.set\('rxdbSmoke', '1'\)/);
  assert.match(e2eSource, /execution context was destroyed\|cannot find context/);
  assert.match(e2eSource, /Business OS shell did not expose module/);
  assert.match(e2eSource, /\.desktop-icon\[data-target=/);
  assert.match(e2eSource, /launcher\.click\(\)/);
});

test('generic smoke fails same-origin HTTP request errors', () => {
  assert.match(source, /same-origin request failed/);
  assert.match(source, /url\.origin === pageOrigin/);
});
