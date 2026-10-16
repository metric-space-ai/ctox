import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

// esbuild aus dem Klon oder (wie Playwright in den v4-Tests) aus dem Haupt-Checkout.
const require = createRequire(import.meta.url);
function loadEsbuild() {
  try { return require('esbuild'); } catch {}
  return require(process.env.ESBUILD_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/esbuild'));
}
const { build } = loadEsbuild();

// Wie der Service Worker (§16.15, PORTING §4 Falle 2): ctox:shared/x.js → ../../shared/x.js.
const ctoxShared = {
  name: 'ctox-shared',
  setup(b) {
    b.onResolve({ filter: /^ctox:shared\// }, (args) => ({
      path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)),
    }));
  },
};

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
  plugins: [ctoxShared],
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __conversationsTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

const t = (key, fallback) => fallback || key;

test('v4: no sync state machine, no readiness/diagnostics hooks, route params parse 1:1', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  assert.equal(hooks.conversationEmptyState, undefined);
  assert.equal(hooks.buildConversationDataDiagnostics, undefined);
  assert.doesNotMatch(js, /collectionReadiness|syncDiagnostics|ctox-syncing|commandBus|location\.hash|ctx\./);
  const link = hooks.parseConversationDeepLink({
    campaign_id: 'c1',
    engagement_id: 'e1',
    message_id: 'om1',
    thread_key: 't1',
    message_key: 'm1',
    account_key: 'a1',
    channel: 'physical_letter',
  });
  assert.equal(link.campaign_id, 'c1');
  assert.equal(link.engagement_id, 'e1');
  assert.equal(link.outbound_message_id, 'om1');
  assert.equal(link.thread_key, 't1');
  assert.equal(link.message_key, 'm1');
  assert.equal(link.account_key, 'a1');
  assert.equal(link.channel, 'email');
  assert.equal(hooks.parseConversationDeepLink(new URLSearchParams('channel=bogus')).channel, '');
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
  // R16: every selector is scoped under the app root (no shell ancestors).
  assert.doesNotMatch(css.replace(/\/\*[\s\S]*?\*\//g, ''), /\.shell-window/);
});

test('left column follows the canonical shell-wired column grammar', async () => {
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');

  // Filterbar: search + shard/list toggle + collapsed filter tray with reset.
  assert.match(html, /data-pg-search/);
  // Operator directive 31.08.2026: the shard/list switch is ONE button that
  // toggles, not a two-button segmented pair. It is therefore module-wired
  // (data-conv-view-toggle) instead of a pair of [data-pg-view] controls, and
  // the pane carries the current view for the shell grammar to read back.
  const viewToggles = html.match(/data-conv-view-toggle/g) || [];
  assert.equal(viewToggles.length, 1, `card/list switch must be ONE button, saw ${viewToggles.length}`);
  assert.doesNotMatch(html, /data-pg-view=/);
  assert.match(html, /data-pg-default-view="cards"/);
  // An action, not a state: no aria-pressed anywhere on that control.
  const toggleTag = html.match(/<button[^>]*data-conv-view-toggle[^>]*>/)?.[0] || '';
  assert.doesNotMatch(toggleTag, /aria-pressed/);
  assert.match(toggleTag, /aria-label=/);
  assert.match(toggleTag, /title=/);
  // Both glyphs live on the one button; JS shows the target view's icon.
  assert.match(html, /data-conv-view-icon="cards"/);
  assert.match(html, /data-conv-view-icon="list"/);
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

test('nothing outside the app window: no fixed overlays, no document-level mounts', async () => {
  // v2 bounded its CTOX chat form to the window (position:absolute in the module root). That form and its context
  // menu had no caller and were removed with the v4 port; the rule behind the old guard stays.
  const [js, css] = await Promise.all([
    readFile(new URL('./index.js', import.meta.url), 'utf8'),
    readFile(new URL('./index.css', import.meta.url), 'utf8'),
  ]);
  assert.doesNotMatch(js, /document\.body\.append|position = 'fixed'/);
  assert.doesNotMatch(css, /position:\s*fixed/);
  assert.match(css, /\[data-app="conversations"\] \.conv-module \{ position: relative;/);
});
