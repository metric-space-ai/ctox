import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';
import { collections as conversationCollections } from '../../conversations/schema.js';
// Gleicher Query-String wie in ../schema.js — sonst erzeugt Node eine zweite
// Modulinstanz und die Referenzgleichheits-Pruefung unten schlaegt fehl.
import { collections as ctoxCollections } from '../../ctox/schema.js?v=20260816-browser-sync-guards-v141';
import { collections as appStoreCollections } from '../../app-store/schema.js';
import { collections as documentCollections } from '../../documents/schema.js';
import { collections as mailCollections } from '../schema.js';
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
const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('../index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});
const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __mailTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

test('mail reuses canonical communication and outbound schemas', () => {
  assert.deepEqual(Object.keys(mailCollections).sort(), [
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
  for (const [name, schema] of Object.entries(mailCollections)) {
    const canonical = name === 'business_users'
      ? ctoxCollections.business_users
      : ['documents', 'document_versions', 'document_blob_chunks'].includes(name)
        ? documentCollections[name]
      : name === 'business_module_catalog' || name === 'business_commands'
        ? appStoreCollections[name]
        : conversationCollections[name];
    assert.equal(schema, canonical);
  }
});

test('ordinary users only see assigned or shared email accounts', () => {
  const accounts = [
    { account_key: 'email:alice@example.test', channel: 'email', address: 'alice@example.test', profile_json: { owner_user_id: 'alice' } },
    { account_key: 'email:bob@example.test', channel: 'email', address: 'bob@example.test', profile_json: { owner_user_id: 'bob' } },
    { account_key: 'email:team@example.test', channel: 'email', address: 'team@example.test', profile_json: { owner_user_id: 'ops', shared_user_ids: ['alice'] } },
    { account_key: 'email:legacy@example.test', channel: 'email', address: 'legacy@example.test', profile_json: {} },
    { account_key: 'whatsapp:alice', channel: 'whatsapp', address: '+49123', profile_json: { owner_user_id: 'alice' } },
  ];
  assert.deepEqual(
    hooks.visibleEmailAccounts(accounts, { id: 'alice', role: 'member' }).map((account) => account.account_key),
    ['email:alice@example.test', 'email:legacy@example.test', 'email:team@example.test'],
  );
  assert.equal(hooks.visibleEmailAccounts(accounts, { id: 'root', role: 'admin' }).length, 4);
  assert.equal(hooks.isGlobalMailAdmin({ role: 'founder' }), true);
  assert.equal(hooks.isGlobalMailAdmin({ role: 'member' }), false);
});

test('mailserver configuration values are normalized without exposing secrets', () => {
  assert.deepEqual(hooks.commandOutcome({ result: { outcome: { users: [{ username: 'alice@example.test' }] } } }), {
    users: [{ username: 'alice@example.test' }],
  });
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
  const accounts = [{ account_key: 'email:alice@example.test' }];
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
    { thread_key: 'inbox', unread_count: 2, last_message_at: '2026-08-06T08:00:00Z' },
    { thread_key: 'sent', unread_count: 0, last_message_at: '2026-08-06T09:00:00Z' },
  ];
  const messages = [
    { thread_key: 'inbox', direction: 'inbound', folder_hint: 'inbox' },
    { thread_key: 'sent', direction: 'outbound', folder_hint: 'sent' },
  ];
  assert.deepEqual(hooks.filterThreadsForFolder(threads, 'inbox', messages).map((row) => row.thread_key), ['inbox']);
  assert.deepEqual(hooks.filterThreadsForFolder(threads, 'unread', messages).map((row) => row.thread_key), ['inbox']);
  assert.deepEqual(hooks.filterThreadsForFolder(threads, 'sent', messages).map((row) => row.thread_key), ['sent']);
});

test('composer builds the native Outbound command chain', () => {
  const bundle = hooks.buildComposeCommandBundle({
    accountKey: 'email:alice@example.test',
    recipient: 'kunde@example.test',
    campaignId: 'campaign-1',
    subject: 'Hallo',
    body: 'Guten Tag',
  }, { engagementId: 'eng-1', messageId: 'msg-1' }, 1234);

  assert.equal(bundle.engagement.commandType, 'outbound.engagement.create');
  assert.equal(bundle.message.commandType, 'outbound.message.prepare');
  assert.equal(bundle.approval.commandType, 'outbound.message.request_approval');
  assert.equal(bundle.message.payload.sender_account_id, 'email:alice@example.test');
  assert.equal(bundle.message.payload.recipient_email, 'kunde@example.test');
  assert.equal(bundle.message.payload.campaign_id, 'campaign-1');
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

test('mail surface provides a progressive inspector workbench and responsive composer', async () => {
  const [html, css, manifest, source] = await Promise.all([
    readFile(new URL('index.html', moduleRoot), 'utf8'),
    readFile(new URL('index.css', moduleRoot), 'utf8'),
    readFile(new URL('module.json', moduleRoot), 'utf8').then(JSON.parse),
    readFile(new URL('index.js', moduleRoot), 'utf8'),
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
  assert.match(source, /function renderActionIcons\(ctx, refs\)/);
  assert.match(source, /ctx\.getActionIcon\?\.\(name\)/);
  assert.doesNotMatch(source, /MAIL_ICON_FALLBACK_PATHS|function mailActionIcon\(/);
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
    { __kind: 'thread', thread_key: 'thread-1', account_key: 'email:team@example.test', subject: 'Hilfe', unread_count: 2 },
    { __kind: 'outbound', id: 'message-1', recipient_email: 'a@example.test', subject: 'Follow-up' },
  ];
  const supportCommands = hooks.buildMailRouteCommands({
    batchId: 'mail_route_1', destinationModule: 'support', mode: 'handoff', records, actor: { id: 'alice', role: 'admin' },
  });
  assert.equal(supportCommands[0].command_type, 'support.conversation.open_from_thread');
  assert.deepEqual(supportCommands[0].payload.source_record_ids, ['thread-1']);
  assert.equal(supportCommands[0].payload.thread_key, 'thread-1');
  assert.equal(supportCommands[0].type, undefined);
  assert.equal(supportCommands[1].command_type, 'business_os.chat.task');
  assert.equal(supportCommands[1].module, 'support');
  assert.deepEqual(supportCommands[1].payload.source_record_ids, ['message-1']);
  assert.equal(supportCommands[1].client_context.surface, 'mail.bulk.route');

  const generic = hooks.buildMailRouteCommands({ batchId: 'mail_route_2', destinationModule: 'customers', records });
  assert.equal(generic.length, 1);
  assert.equal(generic[0].command_type, 'business_os.chat.task');
  assert.equal(generic[0].payload.record_snapshot.schema, 'ctox.mail.route-batch.v1');
  assert.equal(hooks.routeCommandForRecord(records[0], supportCommands)?.module, 'support');
});

test('mail queues expose operational volume and routed evidence', () => {
  const thread = { thread_key: 'thread-1', unread_count: 3, last_message_at: '2026-08-06T09:00:00Z' };
  const outbound = { id: 'message-1', approval_status: 'awaiting_approval', send_status: 'not_scheduled', updated_at_ms: 2 };
  const commands = hooks.buildMailRouteCommands({ batchId: 'route', destinationModule: 'support', records: [{ ...thread, __kind: 'thread' }] });
  const queues = hooks.mailQueueDefinitions({ threads: [thread], outboundMessages: [outbound], commands });
  assert.equal(queues.find((queue) => queue.id === 'all').count, 2);
  assert.equal(queues.find((queue) => queue.id === 'approval').count, 1);
  assert.equal(queues.find((queue) => queue.id === 'routed').count, 1);
});
