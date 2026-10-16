import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): esbuild aus dem Klon, sonst ESBUILD_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie PLAYWRIGHT_MODULE in den v4-api-Tests). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const esbuild = loadDev('esbuild', 'ESBUILD_MODULE');
import { existsSync } from 'node:fs';
import {
  MAIL_GROUP_SCHEMA,
  applyContentRevisionToPending,
  appendMailContentRevision,
  buildMailContentRevision,
  buildMailGroupRecord,
  deriveMailGroupProgress,
  emailGroupDescriptor,
} from '../mail-group-model.mjs';

const moduleRoot = new URL('../', import.meta.url);
const [bootstrapPackage, bootstrapLock] = await Promise.all([
  readFile(new URL('../../../package.json', import.meta.url), 'utf8').then(JSON.parse),
  readFile(new URL('../../../package-lock.json', import.meta.url), 'utf8').then(JSON.parse),
]);
const declaredEsbuildVersion = bootstrapPackage.devDependencies.esbuild;
assert.match(declaredEsbuildVersion, /^\d+\.\d+\.\d+$/, 'Business OS must declare an exact esbuild version');
assert.equal(bootstrapLock.packages['node_modules/esbuild'].version, declaredEsbuildVersion, 'The locked esbuild version must match the Business OS declaration');
assert.equal(esbuild.version, declaredEsbuildVersion, 'Mail regression requires the declared and locked real esbuild; a re-export shim does not verify browser bundling');

const bundledModule = await esbuild.build({
  entryPoints: [fileURLToPath(new URL('../index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
  metafile: true,
  // Wie der Service Worker (§16.15): ctox:shared/x.js → modules/../shared/x.js (PORTING §4 Falle 2).
  plugins: [{
    name: 'ctox-shared',
    setup(build) {
      build.onResolve({ filter: /^ctox:shared\// }, (args) => ({
        path: fileURLToPath(new URL(`../../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)),
      }));
    },
  }],
});
assert.ok(
  Object.keys(bundledModule.metafile?.inputs || {}).some(path => path.endsWith('/mail/lib/mail-body-renderer.mjs')),
  'The Mail bundle must include its actual body renderer dependency',
);
const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __mailTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

// App-API 4.0: keine Readiness mehr (PORTING R4); Zähler gelten, sobald der lokale Live-Stand gelesen ist.
test('mail counts stay unknown until the local live read completed without error', () => {
  assert.equal(hooks.mailCountsKnown({ loading: true, mailReadComplete: false }), false);
  assert.equal(hooks.mailCountsKnown({ loading: false, mailReadComplete: false }), false);
  assert.equal(hooks.mailCountsKnown({ loading: false, mailReadComplete: true, mailReadError: 'denied' }), false);
  assert.equal(hooks.mailCountsKnown({ loading: false, mailReadComplete: true }), true);
});

// App-API 4.0 (§16.13): einzige Schemaquelle ist collections.schema.json; schema.js ist gelöscht. Die geteilten
// Collections bleiben feldgleich mit ihren Quell-Apps (keine zweite Mail-Wahrheit).
test('mail declares the canonical communication and outbound collections in collections.schema.json', async () => {
  const declared = JSON.parse(await readFile(new URL('collections.schema.json', moduleRoot), 'utf8')).collections;
  assert.deepEqual(Object.keys(declared).sort(), [
    'business_commands',
    'business_module_catalog',
    'business_users',
    'communication_accounts',
    'communication_messages',
    'communication_threads',
    'document_blob_chunks',
    'document_versions',
    'documents',
    'outbound_approvals',
    'outbound_campaigns',
    'outbound_engagements',
    'outbound_messages',
    'outbound_pipeline_items',
  ]);
  assert.equal(existsSync(new URL('schema.js', moduleRoot)), false, 'schema.js is removed in v4');
  const sources = {
    conversations: ['communication_accounts', 'communication_messages', 'communication_threads', 'outbound_approvals',
      'outbound_campaigns', 'outbound_engagements', 'outbound_messages', 'outbound_pipeline_items'],
    documents: ['document_blob_chunks', 'document_versions', 'documents'],
  };
  for (const [app, names] of Object.entries(sources)) {
    const source = JSON.parse(await readFile(new URL(`../${app}/collections.schema.json`, moduleRoot), 'utf8')).collections;
    for (const name of names) {
      assert.deepEqual(Object.keys(declared[name].properties).sort(), Object.keys(source[name].properties).sort(), name);
      assert.deepEqual([...(declared[name].required || [])].sort(), [...(source[name].required || [])].sort(), name);
      assert.equal(declared[name].version ?? 0, source[name].version ?? 0, name);
    }
  }
});

test('ordinary users only see assigned or shared email accounts', () => {
  const accounts = [
    { id: 'email:alice@example.test', account_key: 'email:alice@example.test', channel: 'email', address: 'alice@example.test', profile_json: { owner_user_id: 'alice' } },
    { id: 'email:bob@example.test', account_key: 'email:bob@example.test', channel: 'email', address: 'bob@example.test', profile_json: { owner_user_id: 'bob' } },
    { id: 'email:team@example.test', id: 'email:team@example.test', account_key: 'email:team@example.test', channel: 'email', address: 'team@example.test', profile_json: { owner_user_id: 'ops', shared_user_ids: ['alice'] } },
    { id: 'email:legacy@example.test', account_key: 'email:legacy@example.test', channel: 'email', address: 'legacy@example.test', profile_json: {} },
    { id: 'email:shared@example.test', account_key: 'email:shared@example.test', channel: 'email', address: 'shared@example.test', profile_json: { shared_user_ids: ['alice'] } },
    { id: 'whatsapp:alice', account_key: 'whatsapp:alice', channel: 'whatsapp', address: '+49123', profile_json: { owner_user_id: 'alice' } },
  ];
  assert.deepEqual(
    hooks.visibleEmailAccounts(accounts, { id: 'alice', role: 'member' }).map((account) => account.account_key),
    ['email:alice@example.test', 'email:shared@example.test', 'email:team@example.test'],
  );
  assert.equal(hooks.visibleEmailAccounts(accounts, { id: 'root', role: 'admin' }).length, 5);
  assert.deepEqual(
    hooks.visibleEmailAccounts(accounts, { id: 'other', email: 'alice', login: 'alice', role: 'user' }),
    [],
    'mail account access follows the native user ID, not email or login aliases',
  );
  assert.equal(hooks.isGlobalMailAdmin({ role: 'founder' }), false);
  assert.equal(hooks.isGlobalMailAdmin({ role: 'member' }), false);
});

// App-API 4.0: der native Gegenlesepfad (readNativeCollectionDocument) entfällt. Der Core entzieht Zugriff,
// indem er die Projektion in communication_accounts ändert; die App folgt dem lokalen Live-Stand.
test('account access follows the live communication_accounts projection', () => {
  const user = { id: 'alice', role: 'member' };
  const shared = {
    account_key: 'email:team@example.test', channel: 'email', address: 'team@example.test',
    profile_json: { owner_user_id: 'bob', shared_user_ids: ['alice'] },
  };
  assert.equal(hooks.visibleEmailAccounts([shared], user).length, 1);
  const revoked = { ...shared, profile_json: { owner_user_id: 'bob', shared_user_ids: [] } };
  assert.deepEqual(hooks.visibleEmailAccounts([revoked], user), []);
  assert.deepEqual(hooks.visibleEmailAccounts([{ ...shared, _deleted: true }], user), []);
  assert.equal(hooks.authoritativeVisibleEmailAccounts, undefined);
});

test('mailserver configuration values are normalized without exposing secrets', () => {
  assert.equal(hooks.commandOutcome, undefined, 'no command read path in v4');
  assert.equal(hooks.mailserverDomain({ domain_name: 'example.test' }), 'example.test');
  assert.equal(hooks.mailserverUsername({ username: 'alice@example.test' }), 'alice@example.test');
  assert.equal(hooks.isEmailAddress('alice@example.test'), true);
  assert.equal(hooks.isEmailAddress('alice'), false);
});

test('mail status distinguishes queue, SMTP delivery, opens and clicks', () => {
  const queued = { send_status: 'queued_for_provider', payload: { provider_queued_at_ms: 100 } };
  const delivered = { send_status: 'sent', payload: { provider_dispatch_status: 'delivered', delivered_at_ms: 200 } };
  const opened = { ...delivered, payload: { ...delivered.payload, tracking_enabled: true, open_count: 2, last_opened_at_ms: 300 } };
  const clicked = { ...opened, payload: { ...opened.payload, click_count: 1, last_clicked_at_ms: 400 } };
  assert.equal(hooks.messageDeliveryStatus(queued), 'queued');
  assert.equal(hooks.messageDeliveryStatus(delivered), 'delivered');
  assert.equal(hooks.messageEventTimeline(opened).some((event) => event.label.includes('Öffnung erfasst')), true);
  assert.equal(hooks.messageEventTimeline(clicked).some((event) => event.label.includes('Link geklickt')), true);
  assert.match(hooks.plainTextToEmailHtml('Hallo\nWelt'), /Hallo<br>Welt/);
});

test('campaigns are scoped by ownership, sharing, or an assigned sender account', () => {
  const campaigns = [
    { id: 'owned', owner_id: 'alice', updated_at_ms: 4 },
    { id: 'foreign', owner_id: 'bob', updated_at_ms: 3 },
    { id: 'shared', owner_id: 'bob', payload: { shared_user_ids: ['alice'] }, updated_at_ms: 2 },
    { id: 'account-linked', owner_id: 'bob', updated_at_ms: 1 },
  ];
  const accounts = [{ id: 'email:alice@example.test', account_key: 'email:alice@example.test' }];
  const messages = [{ campaign_id: 'account-linked', sender_account_id: 'email:alice@example.test' }];
  assert.deepEqual(
    hooks.visibleMailCampaigns(campaigns, { id: 'alice', role: 'member' }, accounts, messages).map((campaign) => campaign.id),
    ['owned', 'shared', 'account-linked'],
  );
});

test('email group templates create finite work orders instead of mailbox folders', () => {
  const support = buildMailGroupRecord({
    id: 'group-support',
    templateId: 'support',
    name: 'Support August',
    ownerId: 'ops',
    accountKey: 'email:support@example.test',
  }, 1234);
  const descriptor = emailGroupDescriptor(support);
  assert.equal(support.payload.mail_group.schema, MAIL_GROUP_SCHEMA);
  assert.equal(descriptor.kind, 'support');
  assert.equal(descriptor.intakeMode, 'dynamic');
  assert.equal(descriptor.accountKey, 'email:support@example.test');
  assert.equal(support.payload.folder, undefined);
});

test('a one-email group is the degenerate form and progress is derived per item', () => {
  assert.deepEqual(deriveMailGroupProgress('single', [{
    id: 'mail-1', campaign_id: 'single', send_status: 'sent', payload: {},
  }]), { total: 1, open: 0, working: 0, done: 1, percent: 100, complete: true });

  assert.deepEqual(deriveMailGroupProgress('mixed', [
    { id: 'done', campaign_id: 'mixed', send_status: 'accepted', payload: {} },
    { id: 'working', campaign_id: 'mixed', approval_status: 'awaiting_approval', payload: {} },
    { id: 'failed', campaign_id: 'mixed', send_status: 'provider_failed', payload: {} },
  ]), { total: 3, open: 1, working: 1, done: 1, percent: 33, complete: false });
});

test('content revisions update pending emails but never rewrite sent evidence', () => {
  const revision = buildMailContentRevision({
    id: 'rev-2', groupId: 'group-1', editorKind: 'email_visual',
    sourceRef: { blob_id: 'easy-email-json-2', port_version: '1' },
  }, 1234);
  const rows = applyContentRevisionToPending([
    { id: 'sent', campaign_id: 'group-1', send_status: 'sent', payload: { mail_content_revision_id: 'rev-1' } },
    { id: 'draft', campaign_id: 'group-1', send_status: 'draft', payload: { mail_content_revision_id: 'rev-1' } },
    { id: 'foreign', campaign_id: 'group-2', send_status: 'draft', payload: { mail_content_revision_id: 'foreign-rev' } },
  ], revision);
  assert.equal(rows[0].payload.mail_content_revision_id, 'rev-1');
  assert.equal(rows[1].payload.mail_content_revision_id, 'rev-2');
  assert.equal(rows[2].payload.mail_content_revision_id, 'foreign-rev');
});

test('content revisions form an immutable history with one active revision', () => {
  const first = buildMailContentRevision({
    id: 'rev-1', groupId: 'group-1', editorKind: 'email_visual',
    sourceRef: { blob_id: 'design-1' },
  }, 1000);
  const second = buildMailContentRevision({
    id: 'rev-2', groupId: 'group-1', editorKind: 'email_visual',
    sourceRef: { blob_id: 'design-2' },
  }, 2000);
  const original = { mode: 'email_visual' };
  const afterFirst = appendMailContentRevision(original, first);
  const afterSecond = appendMailContentRevision(afterFirst, second);

  assert.equal(original.active_revision_id, undefined);
  assert.equal(afterSecond.active_revision_id, 'rev-2');
  assert.deepEqual(afterSecond.revisions.map((revision) => revision.id), ['rev-1', 'rev-2']);
  assert.equal(afterSecond.revisions[0].source_ref.blob_id, 'design-1');
});

test('content revision hashes are stable across object key order', async () => {
  const first = hooks.stableJson({ subject: 'Hallo', design: { b: 2, a: 1 } });
  const second = hooks.stableJson({ design: { a: 1, b: 2 }, subject: 'Hallo' });
  assert.equal(first, second);
  assert.equal(await hooks.sha256Text(first), await hooks.sha256Text(second));
  assert.match(await hooks.sha256Text(first), /^[0-9a-f]{64}$/);
});

test('mail folders derive from canonical thread and message direction', () => {
  const threads = [
    { id: 'inbox', thread_key: 'inbox', account_key: 'email:alice@example.test', unread_count: 2, last_message_at: '2026-08-06T08:00:00Z' },
    { id: 'sent', thread_key: 'sent', account_key: 'email:alice@example.test', unread_count: 0, last_message_at: '2026-08-06T09:00:00Z' },
  ];
  const messages = [
    { thread_key: 'inbox', account_key: 'email:alice@example.test', direction: 'inbound', folder_hint: 'inbox' },
    { thread_key: 'sent', account_key: 'email:alice@example.test', direction: 'outbound', folder_hint: 'sent' },
  ];
  assert.deepEqual(hooks.filterThreadsForFolder(threads, 'inbox', messages).map((row) => row.thread_key), ['inbox']);
  assert.deepEqual(hooks.filterThreadsForFolder(threads, 'unread', messages).map((row) => row.thread_key), ['inbox']);
  assert.deepEqual(hooks.filterThreadsForFolder(threads, 'sent', messages).map((row) => row.thread_key), ['sent']);
});

test('folder identity keeps delegated Sent mail and self-authored Inbox mail separate', () => {
  const account_key = 'email:owner@example.test';
  const sent = { __kind: 'thread', id: 'delegated', thread_key: 'delegated', account_key, last_message_at: '2026-08-06T09:00:00Z' };
  const inbox = { __kind: 'thread', id: 'self', thread_key: 'self', account_key, last_message_at: '2026-08-06T08:00:00Z' };
  const messages = [
    { thread_key: 'delegated', account_key, direction: 'inbound', folder_hint: 'sent' },
    { thread_key: 'self', account_key, direction: 'outbound', folder_hint: 'inbox' },
  ];
  assert.deepEqual(hooks.filterThreadsForFolder([sent, inbox], 'sent', messages).map((row) => row.thread_key), ['delegated']);
  assert.deepEqual(hooks.filterThreadsForFolder([sent, inbox], 'inbox', messages).map((row) => row.thread_key), ['self']);
  assert.equal(hooks.listBandCounts([sent, inbox], [], messages).outbound, 1);
  assert.equal(hooks.listBandCounts([sent, inbox], [], messages).inbound, 1);
});

test('a shared provider thread ID never mixes messages from different accounts', () => {
  const alice = { id: 'shared-id', thread_key: 'shared-id', account_key: 'email:alice@example.test' };
  const bob = { id: 'shared-id', thread_key: 'shared-id', account_key: 'email:bob@example.test' };
  const messages = [
    { thread_key: 'shared-id', account_key: bob.account_key, direction: 'outbound', folder_hint: 'sent', body_text: 'Bob private body', external_created_at: '2026-08-06T10:00:00Z' },
    { thread_key: 'shared-id', account_key: alice.account_key, direction: 'inbound', folder_hint: 'inbox', body_text: 'Alice private body', external_created_at: '2026-08-06T09:00:00Z' },
  ];
  assert.equal(hooks.messageBelongsToThread(messages[0], alice), false);
  assert.equal(hooks.latestMessageForThread(alice, messages)?.body_text, 'Alice private body');
  assert.equal(hooks.latestMessageForThread(bob, messages)?.body_text, 'Bob private body');
  assert.deepEqual(hooks.filterThreadsForFolder([alice], 'sent', messages), []);
  assert.deepEqual(hooks.filterThreadsForFolder([bob], 'inbox', messages), []);
});

test('route status for a shared thread ID stays with its source account', () => {
  const alice = { __kind: 'thread', id: 'shared-id', thread_key: 'shared-id', account_key: 'email:alice@example.test' };
  const bob = { __kind: 'thread', id: 'shared-id', thread_key: 'shared-id', account_key: 'email:bob@example.test' };
  const [route] = hooks.buildMailRouteCommands({
    batchId: 'batch-1', destinationModule: 'support', records: [bob], actor: { id: 'admin' },
  });
  assert.equal(hooks.routeCommandForRecord(alice, [route]), null);
  assert.equal(hooks.routeCommandForRecord(bob, [route])?.id, route.id);
});

test('composer builds ONE outbound.message.compose payload (MAIL2)', () => {
  const payload = hooks.buildComposePayload({
    accountKey: 'email:alice@example.test',
    campaignId: 'campaign-1',
    subject: 'Hallo',
    body: 'Guten Tag',
    contentRevisionId: 'rev-1',
    mode: 'approval',
  }, [{ recipient_email: 'kunde@example.test', engagement_id: 'eng-1', message_id: 'msg-1' }], 1234);

  assert.equal(payload.mode, 'approval');
  assert.equal(payload.sender_account_id, 'email:alice@example.test');
  assert.equal(payload.campaign_id, 'campaign-1');
  assert.equal(payload.body_text, 'Guten Tag');
  assert.match(payload.body_html, /Guten Tag/);
  assert.equal(payload.content_revision_id, 'rev-1');
  assert.equal(payload.created_at_ms, 1234);
  assert.deepEqual(payload.recipients, [{ recipient_email: 'kunde@example.test', engagement_id: 'eng-1', message_id: 'msg-1' }]);
});

test('mail_server_state rows map onto the mailbox admin view (MAIL2)', () => {
  const state = hooks.mailserverStateFromRows([
    { id: 'runtime', kind: 'runtime', status: 'running', data: { config: { enabled: true, smtp_port: 2525 }, health: { smtp_reachable: true, imap_reachable: false } } },
    { id: 'domain:b.test', kind: 'domain', name: 'b.test', data: { dkim_selector: 'default', spf_record: 'v=spf1 -all' } },
    { id: 'domain:a.test', kind: 'domain', name: 'a.test', data: {} },
    { id: 'user:ada@a.test', kind: 'user', name: 'ada@a.test', data: { owner_user_id: 'u1' } },
    { id: 'domain:gone.test', kind: 'domain', name: 'gone.test', _deleted: true },
  ]);
  assert.deepEqual(state.runtimeConfig, { enabled: true, smtp_port: 2525 });
  assert.equal(state.runtimeStatus.state, 'running');
  assert.equal(state.health.imap_reachable, false);
  assert.deepEqual(state.domains.map((d) => d.domain_name), ['a.test', 'b.test']);
  assert.equal(state.domains[1].spf_record, 'v=spf1 -all');
  assert.deepEqual(state.users.map((u) => [u.username, u.owner_user_id]), [['ada@a.test', 'u1']]);
  assert.deepEqual(hooks.mailserverStateFromRows([]), { runtimeConfig: {}, runtimeStatus: {}, health: {}, domains: [], users: [] });
});

test('composer validation and approval actions are deterministic', () => {
  assert.equal(hooks.validateComposeInput({ accountKey: '', recipient: 'a@example.test', subject: 'x', body: '' }), 'missingSender');
  assert.equal(hooks.validateComposeInput({ accountKey: 'email:a@example.test', recipient: 'invalid', subject: 'x', body: '' }), 'missingRecipient');
  assert.equal(hooks.validateComposeInput({ accountKey: 'email:a@example.test', recipient: 'b@example.test', subject: '', body: '' }), 'missingContent');
  assert.equal(hooks.validateComposeInput({ accountKey: 'email:a@example.test', recipient: 'b@example.test', subject: 'x', body: '' }), '');

  const t = (_key, fallback) => fallback;
  assert.equal(hooks.messageActions({ approval_status: 'draft', send_status: 'draft' }, t)[0].id, 'request-approval');
  assert.equal(hooks.messageActions({ approval_status: 'awaiting_approval', send_status: 'not_scheduled' }, t)[0].id, 'approve');
  assert.equal(hooks.messageActions({ approval_status: 'approved', send_status: 'approved_not_sent' }, t)[0].id, 'send');
});

test('outbound message progress is a six-step status model rendered in the row', () => {
  const draft = hooks.messageProgressModel({ approval_status: 'draft', send_status: 'draft', payload: {} });
  assert.equal(draft.steps.length, 6);
  assert.equal(draft.steps[0].state, 'active');
  assert.equal(draft.percent, 0);

  const awaiting = hooks.messageProgressModel({ approval_status: 'awaiting_approval', send_status: 'not_scheduled', payload: {} });
  assert.equal(awaiting.steps[0].state, 'done');
  assert.equal(awaiting.steps[1].state, 'active');
  assert.equal(awaiting.percent, 20);

  const queued = hooks.messageProgressModel({ approval_status: 'approved', send_status: 'queued_for_provider', payload: {} });
  assert.equal(queued.steps[1].state, 'done');
  assert.equal(queued.steps[2].state, 'active');
  assert.equal(queued.percent, 40);

  const delivered = hooks.messageProgressModel({ approval_status: 'approved', send_status: 'delivered', payload: { provider_dispatch_status: 'delivered' } });
  assert.equal(delivered.steps[3].state, 'done');
  assert.equal(delivered.percent, 60);

  const opened = hooks.messageProgressModel({ approval_status: 'approved', send_status: 'delivered', payload: { provider_dispatch_status: 'delivered', open_count: 2, tracking_enabled: true } });
  assert.equal(opened.steps[4].state, 'done');
  assert.equal(opened.percent, 80);

  const clicked = hooks.messageProgressModel({ approval_status: 'approved', send_status: 'delivered', payload: { provider_dispatch_status: 'delivered', open_count: 2, click_count: 1, tracking_enabled: true } });
  assert.ok(clicked.steps.every((step) => step.state === 'done'));
  assert.equal(clicked.percent, 100);

  const failed = hooks.messageProgressModel({ approval_status: 'approved', send_status: 'failed', payload: { provider_dispatch_status: 'failed' } });
  assert.equal(failed.steps[3].state, 'failed');
  assert.equal(failed.percent, 60);
});

// App-API 4.0 hat keine Icon-API (app-api.d.ts OFFEN-D): die App-Glyphen sind die einzige Quelle.
test('mail actions render app-local glyphs for every requested control', () => {
  const requested = [
    'add', 'check', 'chevronLeft', 'chevronRight', 'clock', 'close', 'columns',
    'download', 'edit', 'export', 'eye', 'filter', 'grid', 'link', 'list',
    'refresh', 'send', 'settings', 'warning',
  ];
  for (const name of requested) {
    const svg = hooks.mailActionIcon(name, 11, 1.9);
    assert.match(svg, new RegExp('class="mail-action-icon mail-action-' + name + '"'));
    assert.match(svg, /width="11" height="11"/);
    assert.match(svg, /stroke-width="1\.9"/);
    assert.doesNotMatch(svg, /class="mail-action-icon mail-action-more"/);
  }
  const expectedPaths = {
    send: 'M4 12 20 4l-4 16-4.5-6.5L4 12ZM11.5 13.5 20 4',
    warning: 'M12 4 2.8 19.5h18.4L12 4ZM12 10v4M12 17h.01',
    eye: 'M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6ZM12 9.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z',
    link: 'M10 14a4 4 0 0 0 6 .4l3-3a4 4 0 0 0-5.6-5.6L12 7.2M14 10a4 4 0 0 0-6-.4l-3 3a4 4 0 0 0 5.6 5.6L12 16.8',
    list: 'M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01',
    grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
  };
  for (const [name, expected] of Object.entries(expectedPaths)) {
    const actual = hooks.mailActionIcon(name).match(/ d="([^"]+)"/)?.[1];
    assert.equal(actual, expected);
  }
  const unlisted = hooks.mailActionIcon('unlisted-icon');
  assert.match(unlisted, /class="mail-action-icon mail-action-unlisted-icon"/);
  assert.match(unlisted, /d="M6 12h\.01M12 12h\.01M18 12h\.01"/);
});

test('mail surface provides a progressive inspector workbench and responsive composer', async () => {
  const [html, css, manifest, source, browserQaSource] = await Promise.all([
    readFile(new URL('index.html', moduleRoot), 'utf8'),
    readFile(new URL('index.css', moduleRoot), 'utf8'),
    readFile(new URL('module.json', moduleRoot), 'utf8').then(JSON.parse),
    readFile(new URL('index.js', moduleRoot), 'utf8'),
    readFile(new URL('tests/mail-browser-qa.mjs', moduleRoot), 'utf8'),
  ]);
  assert.match(html, /data-mail-account/);
  assert.match(html, /data-mail-group-list/);
  assert.match(html, /data-mail-record-list/);
  assert.match(html, /class="mail-table-head"/);
  assert.match(html, /data-mail-detail/);
  assert.match(html, /data-mail-composer/);
  assert.match(html, /data-mail-mailbox-admin/);
  assert.match(html, /data-mail-settings/);
  assert.match(html, /data-mail-import/);
  assert.match(html, /data-mail-export/);
  // One-button view switch (operator directive, 31.08.2026): a single toggle
  // that carries the CURRENT view in data-pg-view and no aria-pressed.
  assert.doesNotMatch(html, /ctox-view-toggle/, 'no two-button toggle group');
  assert.match(html, /data-mail-view-toggle data-pg-view="cards"/);
  assert.doesNotMatch(html, /data-mail-view-toggle[^>]*aria-pressed/);
  assert.match(html, /data-mail-view-toggle[^>]*aria-label="Als Liste anzeigen"/);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-band="campaigns"/);
  assert.match(html, /data-mail-mailbox-password[^>]+type="password"|type="password"[^>]+data-mail-mailbox-password/);
  assert.match(css, /@container business-app-window \(max-width: 768px\)/);
  assert.match(css, /\.mail-module\.is-inspector-open/);
  assert.match(source, /function mailActionIcon\(/);
  assert.match(source, /const MAIL_ICON_FALLBACK_PATHS = Object\.freeze/);
  assert.match(source, /mailActionIcon\(step\.icon, 11, 1\.9\)/);
  assert.match(source, /const icon = \(name\) => mailActionIcon\(name\)/);
  assert.match(source, /button\.innerHTML = mailActionIcon\(cards \? 'list' : 'grid'\)/);
  // App-API 4.0: nur api.*, keine Schleifen, kein nativer/Befehls-Leseweg, keine Zustands-Platzhalter.
  assert.match(source, /export async function mount\(host, api, \{ restored \} = \{\}\)/);
  assert.doesNotMatch(source.replace(/\/\/.*$/gm, ''), /setInterval|readNativeCollectionDocument|commandBus|waitForCommand|\bctx\./);
  assert.doesNotMatch(html, /data-mail-list-empty|data-mail-detail-empty|data-mail-read-error|role="status"/);
  const iconOnlyBranch = browserQaSource.match(/if \(mailIconOnly\) \{[\s\S]*?break mailQa;/)?.[0] || '';
  const browserErrorCheck = iconOnlyBranch.indexOf('assert.deepEqual(browserErrors, []');
  const iconSuccessLog = iconOnlyBranch.indexOf('console.log(`Mail icon QA OK');
  assert.ok(browserErrorCheck >= 0, 'icon-only QA must check browser errors');
  assert.ok(iconSuccessLog > browserErrorCheck, 'icon-only success must follow the browser-error check');
  const iconNavigation = browserQaSource.slice(browserQaSource.indexOf('async function assertMailIconAcceptance'));
  assert.match(iconNavigation, /\[data-mail-close-nav\]'\)\.click\(\);\s*await sidebar\.waitFor\(\{ state: 'hidden' \}\)/);
  assert.equal(manifest.layout.shell_contract, 'v2');
  assert.equal(manifest.install_scope, 'store');
  assert.equal(manifest.default_installed, false);
  assert.equal(manifest.core, false);
});

test('Sellify handoff opens the canonical Mail series-email contract', () => {
  const hash = hooks.buildSeriesEmailTransferHash({
    campaignId: 'sellify-campaign-1',
    recipients: ['a@example.test', 'b@example.test'],
    subject: 'Thesen',
  });
  assert.match(hash, /^mail\?action=series-email&source_module=sellify/);
  assert.match(hash, /campaign_id=sellify-campaign-1/);
  assert.deepEqual(hooks.parseRecipientAddresses('A <A@example.test>; b@example.test\na@example.test'), ['a@example.test', 'b@example.test']);
});

test('bulk routing uses native Support handoff and chunked app tasks', () => {
  const records = [
    { __kind: 'thread', id: 'thread-1', thread_key: 'thread-1', account_key: 'email:team@example.test', subject: 'Hilfe', unread_count: 2 },
    { __kind: 'outbound', id: 'message-1', recipient_email: 'a@example.test', subject: 'Follow-up' },
  ];
  const supportCommands = hooks.buildMailRouteCommands({
    batchId: 'mail_route_1', destinationModule: 'support', mode: 'handoff', records, actor: { id: 'alice', role: 'admin' },
  });
  assert.equal(supportCommands[0].command_type, 'support.conversation.open_from_thread');
  assert.deepEqual(supportCommands[0].payload.source_record_ids, ['thread-1']);
  assert.equal(supportCommands[0].payload.thread_key, 'thread-1');
  assert.equal(supportCommands[0].type, undefined);
  assert.equal(supportCommands[1].command_type, 'ctox.chat.task'); // SKL-D: Standardbefehl der Chat-App ctox
  assert.equal(supportCommands[1].module, 'support');
  assert.deepEqual(supportCommands[1].payload.source_record_ids, ['message-1']);
  assert.equal(supportCommands[1].client_context.surface, 'mail.bulk.route');

  const generic = hooks.buildMailRouteCommands({ batchId: 'mail_route_2', destinationModule: 'customers', records });
  assert.equal(generic.length, 1);
  assert.equal(generic[0].command_type, 'ctox.chat.task');
  assert.equal(generic[0].payload.source_app, 'mail');
  assert.equal(generic[0].payload.context.record_snapshot.schema, 'ctox.mail.route-batch.v1');
  assert.equal(hooks.routeCommandForRecord(records[0], supportCommands)?.module, 'support');
});

test('mail queues expose operational volume and routed evidence', () => {
  const thread = { id: 'thread-1', thread_key: 'thread-1', account_key: 'email:alice@example.test', unread_count: 3, last_message_at: '2026-08-06T09:00:00Z' };
  const outbound = { id: 'message-1', approval_status: 'awaiting_approval', send_status: 'not_scheduled', updated_at_ms: 2 };
  const commands = hooks.buildMailRouteCommands({ batchId: 'route', destinationModule: 'support', records: [{ ...thread, __kind: 'thread' }] });
  const queues = hooks.mailQueueDefinitions({ threads: [thread], sentThreads: [thread], outboundMessages: [outbound], commands });
  assert.equal(queues.find((queue) => queue.id === 'all').count, 2);
  assert.equal(queues.find((queue) => queue.id === 'outbound').count, 1);
  assert.equal(queues.find((queue) => queue.id === 'outbound').title, 'Gesendet');
  assert.equal(queues.find((queue) => queue.id === 'approval').count, 1);
  assert.equal(queues.find((queue) => queue.id === 'routed').count, 1);
});

test('sent mail from the communication store appears in the sent band', () => {
  const received = { __kind: 'thread', id: 'inbox', thread_key: 'inbox' };
  const sent = { __kind: 'thread', id: 'sent', thread_key: 'sent' };
  const messages = [
    { thread_key: 'inbox', direction: 'inbound', folder_hint: 'inbox' },
    { thread_key: 'sent', direction: 'outbound', folder_hint: 'sent' },
  ];
  const counts = hooks.listBandCounts([received, sent], [], messages);
  assert.equal(counts.inbound, 1);
  assert.equal(counts.outbound, 1);
});
