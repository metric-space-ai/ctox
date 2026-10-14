// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  formatShellCompatibility,
  formatShellTimestamp,
  normalizeShellHealth,
  normalizeShellUpdateStatus,
  normalizeShellVersion,
  readEmbeddedIdentity,
  shellChannel,
} from './shell-release-status.js';

test('loaded document identity is independent of a later active release', () => {
  const metadata = {
    'meta[name="ctox-shell-version"]': [{ content: '0.1.46-beta.7' }],
    'meta[name="ctox-shell-source-commit"]': [{ content: 'a'.repeat(40) }],
  };
  const documentRoot = { querySelectorAll: (selector) => metadata[selector] || [] };
  assert.deepEqual(readEmbeddedIdentity(documentRoot), {
    version: '0.1.46-beta.7', sourceCommit: 'a'.repeat(40), channel: 'beta', state: 'current',
  });
  metadata['meta[name="ctox-shell-version"]'].push({ content: '0.1.46-beta.8' });
  assert.throws(() => readEmbeddedIdentity(documentRoot), /ambiguous/);
  assert.throws(() => readEmbeddedIdentity({ querySelectorAll: () => [] }), /identity/);
});

test('shell identity stays short and channel-aware', () => {
  assert.equal(normalizeShellVersion('v1.2.3'), '1.2.3');
  assert.equal(normalizeShellVersion('business-os-shell-v1.2.3'), '');
  assert.equal(shellChannel('1.2.3'), 'stable');
  assert.equal(shellChannel('1.2.3-rc.1'), 'beta');
  assert.equal(shellChannel('1.2.3-nightly.9'), 'nightly');
});

test('all public update states are accepted and unknown values fail closed', () => {
  for (const state of ['current', 'checking', 'available', 'download', 'verify', 'ready', 'restart', 'failed', 'incompatible', 'blocked', 'rollback', 'recovery']) {
    assert.equal(normalizeShellUpdateStatus(state), state);
  }
  assert.equal(normalizeShellUpdateStatus('surprise'), 'failed');
});

test('release details render only validated bounded metadata', () => {
  assert.match(formatShellTimestamp('2026-08-26T05:29:12+02:00'), /2026/);
  assert.equal(formatShellTimestamp('not-a-date'), '—');
  assert.equal(
    formatShellCompatibility({
      workjetMinVersion: '0.0.33',
      workjetMaxVersion: null,
      ctoxMinVersion: '0.3.22',
      ctoxMaxVersion: '0.4.0',
    }),
    'Workjet ≥0.0.33 · CTOX ≥0.3.22 ≤0.4.0',
  );
  assert.equal(formatShellCompatibility({ workjetMinVersion: '<script>', ctoxMinVersion: '0.3.22' }), '—');
});

test('release identity helpers do not paint recovery or version chrome', () => {
  const source = readFileSync(new URL('./shell-release-status.js', import.meta.url), 'utf8');
  assert.match(source, /export function readEmbeddedIdentity/);
  assert.doesNotMatch(source, /data-shell-recovery|data-shell-release-panel|querySelector\('\[data-shell|textContent|fetch\s*\(/);
});

test('the running data plane overrides stale lifecycle health without inventing readiness', () => {
  assert.equal(normalizeShellHealth('degraded', 'ready'), 'healthy');
  assert.equal(normalizeShellHealth('healthy', 'failed'), 'degraded');
  assert.equal(normalizeShellHealth('healthy', 'pending'), 'healthy');
  assert.equal(normalizeShellHealth('surprise', 'pending'), 'unknown');
});
