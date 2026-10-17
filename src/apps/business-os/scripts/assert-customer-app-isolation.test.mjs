// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { customerReleaseViolations } from './assert-customer-app-isolation.mjs';

test('customer runtime paths and private global manifests are rejected', () => {
  const manifests = new Map([
    ['src/apps/business-os/modules/tickets/module.json', JSON.stringify({ id: 'tickets' })],
    ['src/apps/business-os/modules/private/module.json', JSON.stringify({ id: 'private', audience: 'customer' })],
  ]);
  const violations = customerReleaseViolations([
    'src/apps/business-os/installed-modules/rem-private/index.js',
    'runtime/customer-work/outbound-lead-generation/index.js',
    ...manifests.keys(),
  ], (path) => manifests.get(path));
  assert.equal(violations.length, 3);
  assert.match(violations.join('\n'), /installed-modules/);
  assert.match(violations.join('\n'), /runtime\/customer-work/);
  assert.match(violations.join('\n'), /private\/module\.json/);
});

test('public system manifests remain allowed', () => {
  const violations = customerReleaseViolations([
    'src/apps/business-os/modules/tickets/module.json',
  ], () => JSON.stringify({ id: 'tickets', audience: 'public' }));
  assert.deepEqual(violations, []);
});

test('unknown and malformed distribution markers fail closed', () => {
  const paths = [
    'src/apps/business-os/modules/new-private/module.json',
    'src/apps/business-os/modules/malformed/module.json',
    'src/apps/business-os/modules/customer-id/module.json',
  ];
  const values = new Map([
    [paths[0], JSON.stringify({ id: 'new-private', distribution: 'customer-beta' })],
    [paths[1], JSON.stringify({ id: 'malformed', audience: ['public'] })],
    [paths[2], JSON.stringify({ id: 'customer-id', customerId: 42 })],
  ]);
  const violations = customerReleaseViolations(paths, (path) => values.get(path));
  assert.equal(violations.length, 3);
});

test('CLI checks direct and symlinked entrypoints against release data', (t) => {
  const base = realpathSync(mkdtempSync(join(tmpdir(), 'ctox-isolation-cli-')));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const physical = join(base, 'physical');
  const alias = join(base, 'alias');
  const project = join(physical, 'project');
  const relative = 'src/apps/business-os/installed-modules/fixture-app/module.json';
  mkdirSync(join(project, 'src/apps/business-os/installed-modules/fixture-app'), { recursive: true });
  writeFileSync(join(project, relative), JSON.stringify({ id: 'fixture-app' }));
  execFileSync('git', ['init', '-q', project]);
  execFileSync('git', ['-C', project, 'add', '-f', relative]);
  copyFileSync(fileURLToPath(new URL('./assert-customer-app-isolation.mjs', import.meta.url)), join(physical, 'guard.mjs'));
  symlinkSync(physical, alias, 'dir');

  const entries = [
    ['direct', join(physical, 'guard.mjs'), project],
    ['symlink', join(alias, 'guard.mjs'), join(alias, 'project')],
  ];
  for (const [route, entry, root] of entries) {
    const result = spawnSync(process.execPath, [entry, root], { encoding: 'utf8' });
    assert.notEqual(result.status, 0, `${route} invocation must reject runtime app data`);
    assert.match(result.stderr + result.stdout, /Customer app isolation failed/);
  }

  // A successful CLI exit must also prove the check ran.
  const safe = join(base, 'safe');
  mkdirSync(safe);
  execFileSync('git', ['init', '-q', safe]);
  for (const [route, entry] of entries) {
    const result = spawnSync(process.execPath, [entry, safe], { encoding: 'utf8' });
    assert.equal(result.status, 0, `${route}: ${result.stderr}`);
    assert.match(result.stdout, /customer app isolation: ok/);
  }
});
