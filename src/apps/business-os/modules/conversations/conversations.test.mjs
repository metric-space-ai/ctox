import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __conversationsTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

const t = (key, fallback) => fallback || key;

test('conversation empty state distinguishes filter misses from sync failures', () => {
  const noResults = hooks.conversationEmptyState({
    totalBuckets: 2,
    filteredBuckets: 0,
    hasActiveFilters: true,
    diagnostics: { hasFailure: false, isStarting: false },
    t,
  });

  assert.equal(noResults.kind, 'no-results');

  const syncFailure = hooks.conversationEmptyState({
    totalBuckets: 0,
    filteredBuckets: 0,
    hasActiveFilters: false,
    diagnostics: {
      hasFailure: true,
      isStarting: false,
      problemCollections: ['communication_messages'],
      detail: 'communication_messages: WebRTC replication failed',
    },
    t,
  });

  assert.equal(syncFailure.kind, 'sync-failure');
  assert.doesNotMatch(syncFailure.body, /communication_messages|WebRTC|replication/i);
  assert.match(syncFailure.body, /Konversationen/);
});

test('communication diagnostics include accounts and messages sync errors', () => {
  const diagnostics = hooks.buildConversationDataDiagnostics({
    missingCollections: new Set(['communication_accounts']),
    collectionErrors: new Map([
      ['communication_messages', new Error('load failed')],
    ]),
    syncDiagnostics: {
      collections: {
        communication_messages: {
          status: 'failed',
          lastError: { message: 'WebRTC replication failed' },
        },
      },
    },
  });

  assert.equal(diagnostics.hasFailure, true);
  assert.deepEqual(diagnostics.problemCollections.sort(), [
    'communication_accounts',
    'communication_messages',
  ]);
  assert.match(diagnostics.detail, /communication_accounts/);
  assert.match(diagnostics.detail, /communication_messages/);
});

test('communication diagnostics treats failed sync status as failure without lastError', () => {
  const diagnostics = hooks.buildConversationDataDiagnostics({
    syncDiagnostics: {
      collections: {
        communication_accounts: { status: 'connected' },
        communication_messages: { status: 'failed' },
      },
    },
  });

  assert.equal(diagnostics.hasFailure, true);
  assert.deepEqual(diagnostics.problemCollections, ['communication_messages']);
});

test('communication diagnostics treats running but connecting peers as sync starting', () => {
  const diagnostics = hooks.buildConversationDataDiagnostics({
    syncDiagnostics: {
      collections: {
        communication_accounts: { status: 'running', connectionStatus: 'connected' },
        communication_threads: { status: 'running', connectionStatus: 'connecting' },
        communication_messages: { status: 'running', connectionStatus: 'connected' },
      },
    },
  });

  assert.equal(diagnostics.hasFailure, false);
  assert.equal(diagnostics.isStarting, true);
  assert.deepEqual(diagnostics.startingCollections, ['communication_threads']);
});

test('communication diagnostics includes thread projection peer timeouts', () => {
  const diagnostics = hooks.buildConversationDataDiagnostics({
    syncDiagnostics: {
      collections: {
        communication_accounts: { status: 'running', connectionStatus: 'connected' },
        communication_threads: {
          status: 'running',
          connectionStatus: 'connecting',
          lastLifecycleEvent: {
            code: 'peer_connect_timeout',
            message: 'WebRTC native peer did not open for communication_threads within 30000ms',
            severity: 'recoverable',
          },
        },
        communication_messages: { status: 'running', connectionStatus: 'connected' },
      },
    },
  });

  assert.equal(diagnostics.hasFailure, true);
  assert.deepEqual(diagnostics.problemCollections, ['communication_threads']);
  assert.match(diagnostics.detail, /communication_threads/);
  assert.match(diagnostics.detail, /native peer did not open/);
});

test('bucket filters report active account, channel, direction, date, and search state', () => {
  assert.equal(hooks.hasActiveListFilters({
    channel: 'all',
    account: '',
    direction: 'any',
    dateRange: 'any',
    search: '',
  }), false);

  assert.equal(hooks.hasActiveListFilters({
    channel: 'email',
    account: '',
    direction: 'any',
    dateRange: 'any',
    search: '',
  }), true);
});

test('conversations presentation follows compact Business OS contract', async () => {
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');

  assert.doesNotMatch(html, /ctox-pane--glass/);
  assert.doesNotMatch(css, /border-(left|right)\s*:\s*(?:[2-9]|\d{2,})px/i);
  // Inspector cards ride on the kit .ctox-card — the module must not
  // re-declare their border/background/radius.
  assert.match(html, /class="ctox-card conv-card"/);
  assert.doesNotMatch(css, /\.conv-card\s*\{[^}]*(border|background|border-radius)/s);
  // The chat timeline stays a custom view and keeps its compact radius.
  assert.match(css, /\.conv-message\s*\{[^}]*border-radius:\s*var\(--control-radius\)/s);
});
