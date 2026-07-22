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

test('left column follows the canonical shell-wired column grammar', async () => {
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');

  // Filterbar: search + shard/list toggle + collapsed filter tray with reset.
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-view="cards"/);
  assert.match(html, /data-pg-view="list"/);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-tray\b/);
  assert.match(html, /data-pg-reset/);

  // Tray filters carry stable state keys the shell reports back.
  assert.match(html, /data-pg-name="account"/);
  assert.match(html, /data-pg-name="direction"/);
  assert.match(html, /data-pg-name="dateRange"/);

  // Recessed well + one-line footer.
  assert.match(html, /class="[^"]*\bctox-well\b/);
  assert.match(html, /data-pg-footer/);

  // Header actions are collected import/export icons (honest JSON I/O).
  assert.match(html, /data-action="import"/);
  assert.match(html, /data-action="export"/);

  // Counted channel band: >= 2 real views incl. an "all" tab and count spans.
  const bandTabs = html.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bandTabs.length >= 2, `channel band needs >= 2 views, saw ${bandTabs.length}`);
  assert.match(html, /data-pg-band="all"/);
  assert.match(html, /data-pg-count="all"/);

  // The old permanent chip wall and manual toggles are gone.
  assert.doesNotMatch(html, /data-conv-channel-filters/);
  assert.doesNotMatch(html, /data-toggle-actions/);
  assert.doesNotMatch(html, /data-toggle-filters/);
});

test('channel band counts render zeros and reflect every bucket', () => {
  const counts = hooks.channelBandCounts([
    { channels: ['email'] },
    { channels: ['email', 'whatsapp'] },
    { channels: new Set(['whatsapp']) },
  ]);
  assert.equal(counts.all, 3);
  assert.equal(counts.email, 2);
  assert.equal(counts.whatsapp, 2);
  // A supported channel with no buckets is still counted (zero, never hidden).
  assert.equal(counts.jami, 0);
  assert.equal(hooks.channelBandCounts([]).all, 0);
});

test('context pane auto-reveals only with a selection and no user collapse', () => {
  assert.equal(hooks.conversationContextVisible(true, false), true);
  assert.equal(hooks.conversationContextVisible(true, true), false);
  assert.equal(hooks.conversationContextVisible(false, false), false);
  assert.equal(hooks.conversationContextVisible('', false), false);
});

test('conversation rows and message bubbles expose the agent context trio', () => {
  assert.deepEqual(hooks.conversationRecordContext({ key: 'alice|bob', displayName: 'Alice & Bob' }), {
    'data-context-record-id': 'alice|bob',
    'data-context-record-type': 'conversation',
    'data-context-label': 'Alice & Bob',
  });
  assert.deepEqual(hooks.messageRecordContext({
    message_key: 'msg-42',
    subject: 'Quarterly review',
    body_text: 'Fallback body',
  }), {
    'data-context-record-id': 'msg-42',
    'data-context-record-type': 'conversation_message',
    'data-context-label': 'Quarterly review',
  });
});

test('export builds an honest snapshot and import round-trips it into a local overlay', () => {
  const buckets = [{
    key: 'a|b',
    displayName: 'A, B',
    participants: ['a', 'b'],
    channels: new Set(['email']),
    accountKeys: new Set(['acct1']),
    threads: [{ thread_key: 't1', channel: 'email', account_key: 'acct1' }],
  }];
  const messages = [
    { thread_key: 't1', message_key: 'm1', body_text: 'hi' },
    { message_key: 'no-thread' },
  ];

  const payload = hooks.buildConversationsExport(buckets, messages, 123);
  assert.equal(payload.kind, 'ctox-conversations-export');
  assert.equal(payload.exported_at_ms, 123);
  assert.equal(payload.conversations.length, 1);
  assert.deepEqual(payload.conversations[0].channels, ['email']);
  assert.deepEqual(payload.conversations[0].account_keys, ['acct1']);
  assert.equal(payload.messages.t1.length, 1);
  assert.equal(payload.messages['no-thread'], undefined);

  const parsed = hooks.parseConversationsImport(payload);
  assert.equal(parsed.threads.length, 1);
  assert.equal(parsed.threads[0].thread_key, 't1');
  assert.equal(parsed.threads[0].__imported, true);
  assert.equal(parsed.messagesByThread.get('t1').length, 1);
  assert.equal(parsed.messagesByThread.get('t1')[0].__imported, true);

  // A bare conversations array is accepted; junk yields an empty overlay.
  assert.equal(hooks.parseConversationsImport([{ threads: [{ thread_key: 't2' }] }]).threads.length, 1);
  assert.equal(hooks.parseConversationsImport({}).threads.length, 0);
  assert.equal(hooks.parseConversationsImport(null).threads.length, 0);
});

test('selecting a conversation is an in-place class flip, never a list rebuild', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');

  const markFn = js.match(/function markActiveBucket\(\)\s*\{[\s\S]*?\n {2}\}/);
  assert.ok(markFn, 'markActiveBucket present');
  assert.match(markFn[0], /classList\.toggle\('is-selected'/);
  assert.match(markFn[0], /classList\.toggle\('is-active'/);

  const selectFn = js.match(/function selectBucket\(bucketKey\)\s*\{[\s\S]*?\n {2}\}/);
  assert.ok(selectFn, 'selectBucket present');
  assert.match(selectFn[0], /markActiveBucket\(\)/);
  // A rebuild (renderList) would reset the operator's scroll — selection must
  // not trigger it.
  assert.doesNotMatch(selectFn[0], /renderList\(/);
});
