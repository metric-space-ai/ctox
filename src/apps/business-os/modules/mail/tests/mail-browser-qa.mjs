#!/usr/bin/env node
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, readdir, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../../../../..');
const businessOsRoot = join(repoRoot, 'src/apps/business-os');
const browserPackage = join(businessOsRoot, 'package.json');
const require = createRequire(browserPackage);
const { chromium } = require('playwright');
const iconProviderMode = process.env.CTOX_MAIL_ICON_PROVIDER_MODE || 'shell';
if (!['shell', 'absent', 'empty'].includes(iconProviderMode)) {
  throw new Error('CTOX_MAIL_ICON_PROVIDER_MODE must be shell, absent, or empty');
}
const mailIconOnly = process.env.CTOX_MAIL_ICON_QA_ONLY === '1';

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url || '/', 'http://127.0.0.1');
    if (url.pathname === '/' || url.pathname === '/mail-qa.html') {
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      response.end(`<!doctype html><html lang="de" data-theme="light"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><link rel="icon" href="data:,"><link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css"><style>html,body,#host{width:100%;height:100%;margin:0}body{overflow:hidden}#host{display:grid}</style></head><body><div class="shell-window-content"><div id="host"></div></div></body></html>`);
      return;
    }
    const relative = normalize(decodeURIComponent(url.pathname)).replace(/^[/\\]+/, '');
    const filePath = resolve(businessOsRoot, relative);
    const fileStats = await stat(filePath).catch(() => null);
    if (!filePath.startsWith(`${businessOsRoot}/`) || !fileStats?.isFile()) {
      response.writeHead(404).end('not found');
      return;
    }
    response.writeHead(200, { 'content-type': mimeType(filePath) });
    response.end(await readFile(filePath));
  } catch (error) {
    response.writeHead(500).end(String(error?.message || error));
  }
});

await new Promise((resolveListen) => server.listen(0, '127.0.0.1', resolveListen));
const address = server.address();
const baseUrl = `http://127.0.0.1:${address.port}`;
const browser = await chromium.launch({ headless: true, executablePath: findChromiumExecutable() });
const context = await browser.newContext({ viewport: { width: 1440, height: 940 } });
const page = await context.newPage();
const browserErrors = [];
page.on('pageerror', (error) => browserErrors.push(String(error?.stack || error)));
page.on('console', (message) => {
  if (message.type() === 'error') browserErrors.push(message.text());
});

mailQa: try {
  await page.goto(`${baseUrl}/mail-qa.html`, { waitUntil: 'domcontentloaded' });
  await page.evaluate(async (iconProviderMode) => {
    const rows = {
      business_commands: [],
      business_module_catalog: [{
        id: 'runtime', ok: true, updated_at_ms: 1,
        modules: [
          { id: 'mail', title: 'Mail', installed: true },
          { id: 'support', title: 'Support', installed: true },
          { id: 'customers', title: 'Kunden', installed: true },
        ],
      }],
      business_users: [
        { id: 'alice', user_id: 'alice', display_name: 'Alice Admin', role: 'admin', active: true, updated_at_ms: 1 },
        { id: 'bob', user_id: 'bob', display_name: 'Bob Beispiel', role: 'member', active: true, updated_at_ms: 1 },
      ],
      communication_accounts: [{
        account_key: 'email:alice@example.test',
        channel: 'email',
        address: 'alice@example.test',
        provider: 'ctox-mailserver',
        profile_json: { owner_user_id: 'alice' },
        created_at: '2026-08-06T08:00:00Z',
        updated_at: '2026-08-06T08:00:00Z',
      }],
      communication_threads: [{
        thread_key: 'thread-1',
        channel: 'email',
        account_key: 'email:alice@example.test',
        subject: 'Projektstatus August',
        participant_keys_json: ['kunde@example.test'],
        last_message_at: '2026-08-06T09:00:00Z',
        unread_count: 1,
        updated_at: '2026-08-06T09:00:00Z',
      }, {
        thread_key: 'thread-2', channel: 'email', account_key: 'email:alice@example.test',
        subject: 'Rückfrage zur Rechnung', participant_keys_json: ['finance@example.test'],
        last_message_at: '2026-08-06T08:52:00Z', unread_count: 1, updated_at: '2026-08-06T08:52:00Z',
      }, {
        thread_key: 'thread-3', channel: 'email', account_key: 'email:alice@example.test',
        subject: 'Neuer Supportfall', participant_keys_json: ['help@example.test'],
        last_message_at: '2026-08-06T08:45:00Z', unread_count: 2, updated_at: '2026-08-06T08:45:00Z',
      }],
      communication_messages: [{
        message_key: 'mail-1',
        thread_key: 'thread-1',
        channel: 'email',
        account_key: 'email:alice@example.test',
        direction: 'inbound',
        folder_hint: 'inbox',
        sender_display: 'Kunde GmbH',
        sender_address: 'kunde@example.test',
        subject: 'Projektstatus August',
        body_text: 'Können Sie uns den aktuellen Stand schicken?',
        external_created_at: '2026-08-06T09:00:00Z',
        observed_at: '2026-08-06T09:00:01Z',
      }, {
        message_key: 'mail-2', thread_key: 'thread-2', channel: 'email', account_key: 'email:alice@example.test', direction: 'inbound', folder_hint: 'inbox',
        sender_display: 'Finance Team', sender_address: 'finance@example.test', subject: 'Rückfrage zur Rechnung', body_text: 'Bitte ordnen Sie die Rechnung unserem Vorgang zu.', external_created_at: '2026-08-06T08:52:00Z', observed_at: '2026-08-06T08:52:01Z',
      }, {
        message_key: 'mail-3', thread_key: 'thread-3', channel: 'email', account_key: 'email:alice@example.test', direction: 'inbound', folder_hint: 'inbox',
        sender_display: 'Help Desk', sender_address: 'help@example.test', subject: 'Neuer Supportfall', body_text: 'Wir benötigen kurzfristig Unterstützung.', external_created_at: '2026-08-06T08:45:00Z', observed_at: '2026-08-06T08:45:01Z',
      }],
      outbound_campaigns: [{
        id: 'campaign-1',
        name: 'August Bestandskunden',
        objective: 'Status-Updates bündeln',
        status: 'active',
        owner_id: 'alice',
        payload: {
          mail_group_kind: 'newsletter',
          mail_group: {
            schema: 'ctox.mail-group.v1',
            kind: 'newsletter',
            intake_mode: 'fixed',
            lifecycle_status: 'active',
            account_key: 'email:alice@example.test',
            content: { mode: 'email_visual', active_revision_id: '' },
          },
        },
        created_at_ms: 1,
        updated_at_ms: 1,
      }],
      outbound_pipeline_items: [],
      outbound_engagements: [],
      outbound_messages: [{
        id: 'message-delivered-1',
        engagement_id: 'engagement-delivered-1',
        campaign_id: 'campaign-1',
        channel: 'email',
        direction: 'outbound',
        sender_account_id: 'email:alice@example.test',
        recipient_email: 'kunde@example.test',
        subject: 'Projektstatus August',
        body_text: 'Der aktuelle Projektstand ist jetzt verfügbar.',
        approval_status: 'approved',
        send_status: 'sent',
        sent_at_ms: Date.now() - 120000,
        payload: {
          provider_dispatch_status: 'delivered',
          provider_queued_at_ms: Date.now() - 180000,
          provider_completed_at_ms: Date.now() - 120000,
          delivered_at_ms: Date.now() - 120000,
          tracking_enabled: true,
          open_count: 2,
          first_opened_at_ms: Date.now() - 90000,
          last_opened_at_ms: Date.now() - 60000,
          click_count: 1,
          first_clicked_at_ms: Date.now() - 45000,
          last_clicked_at_ms: Date.now() - 45000,
        },
        created_at_ms: Date.now() - 240000,
        updated_at_ms: Date.now() - 45000,
      }],
      outbound_approvals: [],
    };
    const listeners = new Map();
    const mailserver = {
      domains: [{
        domain_name: 'example.test',
        dkim_selector: 'default',
        dkim_public_key: 'TESTPUBLICKEY',
        spf_record: 'v=spf1 mx a ~all',
        dmarc_record: 'v=DMARC1; p=none; rua=mailto:dmarc@example.test',
      }],
      users: [{ username: 'alice@example.test' }],
      runtime_config: {
        enabled: true,
        hostname: 'mail.example.test',
        bind_host: '127.0.0.1',
        smtp_port: 2525,
        imap_port: 1143,
        outbound_throttle_per_min: 120,
        max_connections: 10,
        tracking_base_url: 'https://mail.example.test',
      },
      runtime_status: { state: 'running', enabled: true, generation: 1 },
      health: { smtp_reachable: true, imap_reachable: true, queue_pending: 2, delivery_success: 18, delivery_failed: 1 },
    };
    function notify(name) {
      for (const listener of listeners.get(name) || []) listener();
    }
    function upsert(name, record) {
      const primary = name === 'communication_accounts' ? 'account_key'
        : name === 'communication_threads' ? 'thread_key'
          : name === 'communication_messages' ? 'message_key' : 'id';
      const index = rows[name].findIndex((item) => item[primary] === record[primary]);
      if (index >= 0) rows[name][index] = { ...rows[name][index], ...record };
      else rows[name].push(record);
      notify(name);
      return record;
    }
    function collection(name) {
      return {
        find: () => ({ exec: async () => rows[name].map((record) => ({ toJSON: () => ({ ...record }) })) }),
        findOne: (id) => ({ exec: async () => {
          const record = rows[name].find((item) => item.id === id || item.command_id === id);
          return record ? { toJSON: () => ({ ...record }) } : null;
        } }),
        insert: async (record) => upsert(name, record),
        incrementalUpsert: async (record) => upsert(name, record),
        $: { subscribe: (listener) => {
          if (!listeners.has(name)) listeners.set(name, new Set());
          listeners.get(name).add(listener);
          return { unsubscribe: () => listeners.get(name)?.delete(listener) };
        } },
      };
    }
    const [{ mount }, { getActionIcon }] = await Promise.all([
      import('/modules/mail/index.js'),
      import('/shared/icons.js'),
    ]);
    window.__mailRows = rows;
    window.__mailserver = mailserver;
    window.__dispatchedCommands = [];
    window.__unmountMail = await mount({
      host: document.querySelector('#host'),
      locale: 'de',
      session: { user: { id: 'alice', email: 'alice@example.test', role: 'admin' } },
      db: { collection },
      sync: {
        startCollection: async () => {},
        collectionReadiness: () => ({ ready: true, state: 'live' }),
        subscribeCollectionReadiness: (_name, listener) => { listener({ ready: true, state: 'live' }); return () => {}; },
      },
      storageScope: { get: () => null, set: () => {} },
      permissions: { canReadCollection: () => true, canWriteCollection: () => true },
      ...(iconProviderMode === 'empty' ? { getActionIcon: () => '' } : iconProviderMode === 'shell' ? { getActionIcon } : {}),
      commandBus: { dispatch: async (command) => {
        window.__dispatchedCommands.push(structuredClone(command));
        const now = Date.now();
        if (command.command_type === 'ctox.mailserver.get_config') {
          return { id: command.id, status: 'completed', result: {
            domains: mailserver.domains.map((domain) => ({ ...domain })),
            users: mailserver.users.map((user) => ({ ...user })),
            runtime_config: { ...mailserver.runtime_config },
            runtime_status: { ...mailserver.runtime_status },
            health: { ...mailserver.health },
          } };
        }
        if (command.command_type === 'ctox.mailserver.save_runtime') {
          mailserver.runtime_config = { ...mailserver.runtime_config, ...command.payload };
          mailserver.runtime_status = { ...mailserver.runtime_status, state: command.payload.enabled === false ? 'stopped' : 'running' };
          mailserver.health = { ...mailserver.health, smtp_reachable: command.payload.enabled !== false, imap_reachable: command.payload.enabled !== false };
          return { id: command.id, status: 'completed', result: { runtime_config: mailserver.runtime_config } };
        }
        if (command.command_type === 'ctox.mailserver.save_domain') {
          const existing = mailserver.domains.findIndex((domain) => domain.domain_name === command.payload.domain_name);
          const domain = { ...command.payload, dkim_public_key: 'GENERATEDPUBLICKEY' };
          if (existing >= 0) mailserver.domains[existing] = domain;
          else mailserver.domains.push(domain);
          return { id: command.id, status: 'completed', result: domain };
        }
        if (command.command_type === 'ctox.mailserver.delete_domain') {
          const index = mailserver.domains.findIndex((domain) => domain.domain_name === command.payload.domain_name);
          if (index >= 0) mailserver.domains.splice(index, 1);
          return { id: command.id, status: 'completed', result: { deleted: true } };
        }
        if (command.command_type === 'ctox.mailserver.save_user') {
          const username = command.payload.username;
          if (!mailserver.users.some((user) => user.username === username)) mailserver.users.push({ username });
          upsert('communication_accounts', {
            account_key: `email:${username}`,
            channel: 'email',
            address: username,
            provider: 'ctox-mailserver',
            profile_json: {
              owner_user_id: command.payload.owner_user_id,
              mailbox_address: username,
              mailbox_status: 'ready',
              mailserver_managed: true,
            },
            created_at: new Date(now).toISOString(),
            updated_at: new Date(now).toISOString(),
          });
          return { id: command.id, status: 'completed', result: { username } };
        }
        if (command.command_type === 'ctox.mailserver.delete_user') {
          const username = command.payload.username;
          const userIndex = mailserver.users.findIndex((user) => user.username === username);
          if (userIndex >= 0) mailserver.users.splice(userIndex, 1);
          const accountIndex = rows.communication_accounts.findIndex((account) => account.account_key === `email:${username}`);
          if (accountIndex >= 0) rows.communication_accounts.splice(accountIndex, 1);
          notify('communication_accounts');
          return { id: command.id, status: 'completed', result: { username } };
        }
        if (command.command_type === 'outbound.engagement.create') {
          return { id: command.id, status: 'completed', result: { engagement: {
            id: command.payload.engagement_id,
            campaign_id: command.payload.campaign_id,
            sender_account_id: command.payload.sender_account_id,
            status: 'assigned',
            payload: command.payload.payload || {},
            created_at_ms: now,
            updated_at_ms: now,
          } } };
        }
        if (command.command_type === 'outbound.message.prepare') {
          return { id: command.id, status: 'completed', result: { message: {
            ...command.payload,
            id: command.payload.message_id,
            draft_status: 'prepared',
            approval_status: 'draft',
            send_status: 'not_scheduled',
            payload: command.payload.payload || {},
            created_at_ms: now,
            updated_at_ms: now,
          } } };
        }
        if (command.command_type === 'outbound.message.request_approval') {
          const message = rows.outbound_messages.find((item) => item.id === command.payload.message_id);
          if (message) {
            message.approval_status = 'awaiting_approval';
            message.updated_at_ms = now;
            notify('outbound_messages');
          }
          return { id: command.id, status: 'completed', result: { message: message ? { ...message } : null } };
        }
        if (command.command_type === 'outbound.message.approve') {
          const message = rows.outbound_messages.find((item) => item.id === command.payload.message_id);
          if (message) {
            message.approval_status = 'approved';
            message.send_status = 'approved_not_sent';
            message.updated_at_ms = now;
            notify('outbound_messages');
          }
          return { id: command.id, status: 'completed', result: { message: message ? { ...message } : null } };
        }
        if (command.command_type === 'outbound.message.send_approved') {
          const message = rows.outbound_messages.find((item) => item.id === command.payload.message_id);
          if (message) {
            message.send_status = 'queued_for_provider';
            message.payload = { ...message.payload, provider_dispatch_status: 'queued_in_mailserver', provider_queued_at_ms: now };
            message.updated_at_ms = now;
            notify('outbound_messages');
          }
          return { id: command.id, status: 'completed', result: { message: message ? { ...message } : null } };
        }
        return { id: command.id, status: 'completed', result: {} };
      } },
    });
    const { wirePaneGrammar } = await import('/shared/pane-grammar.js');
    for (const pane of document.querySelectorAll('[data-mail-left-pane], [data-mail-list-pane]')) {
      pane.__ctoxPaneGrammar = wirePaneGrammar(pane);
    }
  }, iconProviderMode);

  await page.locator('[data-mail-root]').waitFor({ state: 'visible' });
  assert.equal(await page.locator('[data-mail-navigation-title]').textContent(), 'E-Mail-Queues');
  assert.equal(await page.locator('[data-mail-account]').inputValue(), 'all');
  assert.match(await page.locator('[data-mail-account]').textContent(), /alice@example\.test/);
  await assertVisibleText(page, 'Projektstatus August');
  await assertMailIconAcceptance(page, iconProviderMode);
  if (mailIconOnly) {
    assert.deepEqual(browserErrors, [], `Mail icon QA must not emit page or console errors (${iconProviderMode} provider)`);
    console.log(`Mail icon QA OK (${iconProviderMode} provider): toolbar, progress, navigation/close, and both toggle directions`);
    break mailQa;
  }

  await page.locator('[data-mail-select-record="thread:thread-1"]').check();
  await page.locator('[data-mail-select-record="thread:thread-2"]').check();
  await page.locator('[data-mail-bulkbar]').waitFor({ state: 'visible' });
  await page.locator('[data-mail-bulk-route]').click();
  await page.locator('[data-mail-route-destination]').selectOption('support');
  await page.locator('[data-mail-route-note]').fill('Als Supportfälle übernehmen und priorisieren.');
  await page.locator('[data-mail-confirm-route]').click();
  await page.getByText(/2 E-Mails geroutet an Support/).waitFor({ state: 'visible' });
  const routedCommands = await page.evaluate(() => window.__dispatchedCommands.filter((command) => command.client_context?.surface === 'mail.bulk.route'));
  assert.equal(routedCommands.length, 2);
  assert.ok(routedCommands.every((command) => command.command_type === 'support.conversation.open_from_thread'));
  assert.ok(routedCommands.every((command) => command.type === undefined));
  if (process.env.CTOX_MAIL_ROUTE_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_ROUTE_QA_SCREENSHOT), fullPage: true });
  }
  await page.locator('[data-mail-close-route]').click();
  await assertVisibleText(page, 'Geroutete E-Mails');

  await page.locator('[data-mail-record-id="thread-1"]').click();
  await assertVisibleText(page, 'Können Sie uns den aktuellen Stand schicken?');
  if (process.env.CTOX_MAIL_MAIN_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_MAIN_QA_SCREENSHOT), fullPage: true });
  }

  await page.locator('[data-mail-left-pane] [data-pg-band="campaigns"]').click();
  assert.equal(await page.locator('[data-mail-navigation-title]').textContent(), 'E-Mail-Gruppen');
  await page.locator('[data-mail-scope-id="campaign-1"]').click();
  await assertVisibleText(page, 'Status-Updates bündeln');

  await page.locator('[data-mail-close-detail]').click();
  const inlineProgress = page.locator('[data-mail-record-id="message-delivered-1"] .mail-record-progress');
  await inlineProgress.waitFor({ state: 'visible' });
  assert.equal(await inlineProgress.locator('.mail-progress-step').count(), 6);
  assert.equal(await inlineProgress.locator('.mail-progress-step.is-done').count(), 6);
  assert.equal(await inlineProgress.locator('.mail-progress-track > i').getAttribute('style'), 'width:100%');
  if (process.env.CTOX_MAIL_INLINE_STATUS_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_INLINE_STATUS_QA_SCREENSHOT), fullPage: true });
  }

  await page.locator('[data-mail-record-id="message-delivered-1"]').click();
  await assertVisibleText(page, 'Öffnung erfasst · 2×');
  await assertVisibleText(page, 'Link geklickt · 1×');
  if (process.env.CTOX_MAIL_STATUS_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_STATUS_QA_SCREENSHOT), fullPage: true });
  }
  await page.locator('[data-mail-scope-id="campaign-1"]').click();

  await page.locator('[data-mail-action="edit-group-content"]').click();
  await page.locator('[data-mail-content-surface]').waitFor({ state: 'visible' });
  const editorFrame = page.frameLocator('[data-mail-easy-email-host] iframe');
  const editable = editorFrame.locator('[contenteditable="true"]').first();
  await editable.waitFor({ state: 'visible' });
  await editable.dblclick();
  await page.keyboard.press('Meta+A');
  await page.keyboard.insertText('Echter visueller Kampagneninhalt');
  await page.locator('[data-mail-content-title]').click();
  await page.waitForTimeout(500);
  await page.locator('[data-mail-editor-history="undo"]').click();
  await page.waitForTimeout(300);
  await page.locator('[data-mail-editor-history="redo"]').click();
  await page.waitForTimeout(500);
  await page.locator('[data-mail-editor-viewport="desktop"]').click();
  assert.equal(await page.locator('[data-mail-editor-viewport="desktop"]').getAttribute('aria-pressed'), 'true');
  await page.locator('[data-mail-editor-viewport="mobile"]').click();
  assert.equal(await page.locator('[data-mail-editor-viewport="mobile"]').getAttribute('aria-pressed'), 'true');
  await page.locator('[data-mail-editor-viewport="edit"]').click();
  await editable.click();
  assert.equal(await editorFrame.locator('.ctox-frame-tools').count(), 0);
  await page.locator('[data-mail-editor-open-panel="blocks"]').click();
  await editorFrame.getByText('Baustein in die E-Mail ziehen', { exact: true }).waitFor({ state: 'visible' });
  const blockCountBeforeDrag = await editorFrame.locator('.email-block').count();
  const buttonBlock = editorFrame.locator('.ctox-frame-block-item')
    .filter({ hasText: 'Button' })
    .locator('[draggable="true"]');
  await buttonBlock.dragTo(editorFrame.locator('.email-block').last());
  await editorFrame.locator('.email-block').nth(blockCountBeforeDrag).waitFor({ state: 'attached' });
  await page.locator('[data-mail-editor-open-panel="design"]').click();
  await editorFrame.getByRole('tab', { name: 'Element', exact: true }).waitFor({ state: 'visible' });
  assert.equal(await editorFrame.locator('#easy-email-rich-text-bar').isVisible(), false);
  if (process.env.CTOX_MAIL_EDITOR_DESIGN_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_EDITOR_DESIGN_QA_SCREENSHOT), fullPage: true });
  }
  await page.locator('[data-mail-editor-open-panel="source"]').click();
  await editorFrame.getByText('HTML & MJML', { exact: true }).waitFor({ state: 'visible' });
  await page.locator('[data-mail-editor-open-panel="logic"]').click();
  await page.locator('[data-mail-logic-editor]').waitFor({ state: 'visible' });
  assert.equal(await editorFrame.locator('#easy-email-rich-text-bar').isVisible(), false);
  await page.locator('[data-logic-action="add-rule"]').first().click();
  const firstLogicRule = page.locator('.mail-logic-rule').first();
  await firstLogicRule.locator('[data-logic-field="field"]').fill('contact.segment');
  await firstLogicRule.locator('[data-logic-field="field"]').dispatchEvent('change');
  await firstLogicRule.locator('[data-logic-field="operator"]').selectOption('equals');
  await firstLogicRule.locator('[data-logic-field="valueType"]').selectOption('string');
  await firstLogicRule.locator('[data-logic-field="value"]').fill('kunde');
  await firstLogicRule.locator('[data-logic-field="value"]').dispatchEvent('change');
  await page.locator('[data-logic-test-data]').fill('{"contact":{"segment":"kunde"}}');
  await page.locator('[data-logic-test-data]').dispatchEvent('input');
  await page.getByText('Block wird angezeigt', { exact: true }).waitFor({ state: 'visible' });
  await page.locator('[data-logic-test-data]').fill('{"contact":{"segment":"lead"}}');
  await page.locator('[data-logic-test-data]').dispatchEvent('input');
  await page.getByText('Block wird für diese Testdaten ausgeblendet', { exact: true }).waitFor({ state: 'visible' });
  assert.equal(await page.locator('[data-mail-save-content]').getAttribute('class'), 'ctox-pane-icon');
  assert.equal(await page.locator('[data-mail-editor-open-panel="blocks"]').isVisible(), true);
  assert.equal(await page.locator('[data-mail-editor-open-panel="design"]').isVisible(), true);
  assert.equal(await page.locator('[data-mail-editor-open-panel="source"]').isVisible(), true);
  assert.equal(await page.locator('[data-mail-editor-open-panel="logic"]').isVisible(), true);
  await assertEditorThemeBridge(page, editorFrame);
  await assertEditorViewports(page);
  if (process.env.CTOX_MAIL_EDITOR_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_EDITOR_QA_SCREENSHOT), fullPage: true });
  }
  if (process.env.CTOX_MAIL_EDITOR_DARK_QA_SCREENSHOT) {
    await page.evaluate(() => document.documentElement.dataset.theme = 'dark');
    await page.waitForTimeout(200);
    await assertEditorThemeBridge(page, editorFrame);
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_EDITOR_DARK_QA_SCREENSHOT), fullPage: true });
  }
  if (process.env.CTOX_MAIL_EDITOR_BRAND_QA_SCREENSHOT) {
    await page.evaluate(() => {
      const root = document.documentElement;
      root.dataset.theme = 'light';
      root.style.setProperty('--accent', '#6d28d9');
      root.style.setProperty('--accent-soft', '#ede9fe');
      root.style.setProperty('--focus-ring', '#7c3aed');
    });
    await page.waitForTimeout(200);
    await assertEditorThemeBridge(page, editorFrame);
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_EDITOR_BRAND_QA_SCREENSHOT), fullPage: true });
    await page.evaluate(() => {
      const root = document.documentElement;
      root.style.removeProperty('--accent');
      root.style.removeProperty('--accent-soft');
      root.style.removeProperty('--focus-ring');
    });
  }
  await page.evaluate(() => document.documentElement.dataset.theme = 'light');
  await page.waitForTimeout(100);
  await page.locator('[data-mail-save-content]').click();
  await page.waitForFunction(() => {
    const campaign = window.__mailRows.outbound_campaigns.find((item) => item.id === 'campaign-1');
    return Boolean(campaign?.payload?.mail_group?.content?.active_revision_id);
  });
  const savedContent = await page.evaluate(() => window.__mailRows.outbound_campaigns
    .find((campaign) => campaign.id === 'campaign-1').payload.mail_group.content);
  assert.equal(savedContent.revisions.length, 1);
  assert.equal(savedContent.active_revision_id, savedContent.revisions[0].id);
  assert.match(JSON.stringify(savedContent.editor_envelope.htmlDocument), /Echter visueller Kampagneninhalt/);
  assert.match(JSON.stringify(savedContent.editor_envelope.htmlDocument), /contact\.segment/);
  assert.ok(savedContent.active_revision.compiled_html_ref.content.length > 1000);
  await page.locator('[data-mail-close-content-editor]').click();
  await page.locator('[data-mail-content-surface]').waitFor({ state: 'hidden' });
  await page.locator('[data-mail-action="edit-group-content"]').click();
  await editorFrame.locator('[contenteditable="true"]').first().waitFor({ state: 'visible' });
  await assertVisibleText(editorFrame, 'Echter visueller Kampagneninhalt');
  await page.locator('[data-mail-close-content-editor]').click();

  await page.locator('[data-mail-compose]').click();
  await page.locator('[data-mail-compose-to]').fill('kontakt@example.test');
  await page.locator('[data-mail-compose-campaign]').selectOption('campaign-1');
  await page.locator('[data-mail-compose-subject]').fill('August Update');
  await page.locator('[data-mail-compose-body]').fill('Hier ist der aktuelle Projektstand.');
  const commandsBeforeSend = await page.evaluate(() => window.__dispatchedCommands.length);
  await page.locator('[data-mail-send-now]').click();
  await page.locator('[data-mail-composer]').waitFor({ state: 'hidden' });
  await assertVisibleText(page, 'August Update');
  assert.equal(await page.evaluate(() => window.__mailRows.outbound_messages.length), 2);
  const sendChain = await page.evaluate((fromIndex) => window.__dispatchedCommands.slice(fromIndex).map((command) => command.command_type), commandsBeforeSend);
  assert.deepEqual(sendChain, [
    'outbound.engagement.create',
    'outbound.message.prepare',
    'outbound.message.request_approval',
    'outbound.message.approve',
    'outbound.message.send_approved',
  ]);

  await page.locator('[data-mail-new-group]').click();
  await page.locator('[data-mail-group-name]').fill('Messekontakte August');
  await page.locator('[data-mail-group-description]').fill('Nachfassgruppe für die August-Messe');
  await page.locator('[data-mail-create-group]').click();
  await assertVisibleText(page, 'Messekontakte August');

  await page.locator('[data-mail-settings]').click();
  await page.locator('[data-mail-mailbox-admin]').waitFor({ state: 'visible' });
  await assertVisibleText(page, 'Mailserver läuft');
  await assertVisibleText(page, 'SMTP 127.0.0.1:2525 · IMAP 127.0.0.1:1143');
  assert.equal(await page.locator('[data-mail-queue-count]').textContent(), '2');
  await page.locator('[data-mail-server-throttle]').fill('90');
  await page.locator('[data-mail-save-server]').click();
  await page.waitForFunction(() => window.__mailserver.runtime_config.outbound_throttle_per_min === 90);
  await page.locator('[data-mail-domain-form] [data-mail-domain-name]').fill('campaign.example.test');
  await page.locator('[data-mail-dmarc-record]').fill('v=DMARC1; p=quarantine');
  await page.locator('[data-mail-save-domain]').click();
  await page.locator('[data-mail-domain-list]').getByText('campaign.example.test', { exact: true }).waitFor({ state: 'visible' });
  await page.locator('[data-mail-domain-list]').getByText('example.test', { exact: true }).waitFor({ state: 'visible' });
  if (process.env.CTOX_MAIL_SERVER_QA_SCREENSHOT) {
    await page.locator('.mail-mailbox-admin-body').evaluate((node) => { node.scrollTop = 0; });
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_SERVER_QA_SCREENSHOT), fullPage: true });
  }
  await page.locator('[data-mail-mailbox-username]').fill('bob@example.test');
  await page.locator('[data-mail-mailbox-owner]').selectOption('bob');
  await page.locator('[data-mail-mailbox-password]').fill('only-for-command');
  await page.locator('[data-mail-create-mailbox]').click();
  await assertVisibleText(page, 'Zugeordnet zu: Bob Beispiel');
  assert.equal(await page.locator('[data-mail-mailbox-password]').inputValue(), '');
  const createdAccount = await page.evaluate(() => window.__mailRows.communication_accounts.find((account) => account.account_key === 'email:bob@example.test'));
  assert.equal(createdAccount.profile_json.owner_user_id, 'bob');
  assert.equal(JSON.stringify(createdAccount).includes('only-for-command'), false);
  if (process.env.CTOX_MAIL_ADMIN_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_ADMIN_QA_SCREENSHOT), fullPage: true });
  }

  await page.locator('[data-mailbox-action="request-delete"][data-mailbox-username="bob@example.test"]').click();
  await page.locator('[data-mailbox-action="confirm-delete"][data-mailbox-username="bob@example.test"]').click();
  await page.waitForFunction(() => !window.__mailserver.users.some((user) => user.username === 'bob@example.test'));
  assert.equal(await page.evaluate(() => window.__mailRows.communication_accounts.some((account) => account.account_key === 'email:bob@example.test')), false);
  await page.locator('[data-mail-close-mailbox-admin]').click();

  await page.setViewportSize({ width: 700, height: 820 });
  await page.locator('[data-mail-close-detail]').click();
  await page.locator('[data-mail-open-nav]').click();
  await page.locator('.mail-sidebar').waitFor({ state: 'visible' });
  await page.locator('[data-mail-settings]').click();
  await page.locator('[data-mail-mailbox-admin]').waitFor({ state: 'visible' });
  await page.locator('[data-mail-close-mailbox-admin]').click();
  await page.locator('[data-mail-open-nav]').click();
  await page.locator('.mail-sidebar').waitFor({ state: 'visible' });
  await page.locator('[data-mail-close-nav]').click();
  await page.evaluate(() => {
    location.hash = '#mail?action=series-email&source_module=sellify&campaign_id=campaign-1&recipients=kontakt%40example.test%2Ceinkauf%40example.test&subject=August%20Update';
  });
  await page.locator('[data-mail-composer]').waitFor({ state: 'visible' });
  await page.getByText('Serien-E-Mail', { exact: true }).waitFor({ state: 'visible' });
  assert.equal(await page.locator('[data-mail-compose-to]').inputValue(), 'kontakt@example.test\neinkauf@example.test');
  await page.waitForTimeout(250);
  const composerBox = await page.locator('[data-mail-composer]').boundingBox();
  assert.ok(composerBox && composerBox.width <= 700 && composerBox.height <= 820);

  if (process.env.CTOX_MAIL_QA_SCREENSHOT) {
    await page.screenshot({ path: resolve(process.env.CTOX_MAIL_QA_SCREENSHOT), fullPage: true });
  }
  await page.locator('[data-mail-compose-body]').fill('Serienmail aus Sellify.');
  await page.locator('[data-mail-save-draft]').click();
  await page.locator('[data-mail-composer]').waitFor({ state: 'hidden' });
  assert.deepEqual(
    await page.evaluate(() => window.__mailRows.outbound_messages.map((message) => message.recipient_email).sort()),
    ['einkauf@example.test', 'kontakt@example.test', 'kontakt@example.test', 'kunde@example.test'],
  );
  assert.deepEqual(browserErrors, []);
  console.log('Mail browser QA OK: inbox, thread, campaign, draft, group, mailbox administration, Sellify series-email handoff, and responsive composer');
} finally {
  await context.close();
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}

async function iconState(locator) {
  await locator.waitFor({ state: 'visible' });
  return locator.evaluate((node) => {
    const box = node.getBoundingClientRect();
    return {
      html: node.innerHTML,
      svgCount: node.querySelectorAll('svg').length,
      width: box.width,
      height: box.height,
    };
  });
}

function assertIconState(state, className, label) {
  assert.equal(state.svgCount, 1, `${label} must contain exactly one SVG`);
  assert.match(state.html, new RegExp(className), `${label} must use ${className}`);
  assert.ok(state.width > 0 && state.height > 0, `${label} must be visible`);
}

async function assertMailIconAcceptance(page, providerMode) {
  const fallback = providerMode !== 'shell';
  const compose = await iconState(page.locator('[data-mail-compose]'));
  assertIconState(compose, fallback ? 'mail-action-edit' : 'ctox-action-edit', 'compose');

  const toggle = page.locator('[data-mail-left-pane] [data-mail-view-toggle]');
  let toggleState = await iconState(toggle);
  assertIconState(toggleState, fallback ? 'mail-action-list' : 'ctox-action-list', 'cards-to-list toggle');
  assert.equal(await toggle.getAttribute('data-pg-view'), 'cards');
  assert.equal(await toggle.getAttribute('aria-label'), 'Als Liste anzeigen');
  assert.equal(await toggle.getAttribute('title'), 'Als Liste anzeigen');
  assert.equal(await toggle.getAttribute('aria-pressed'), null);
  await toggle.click();
  toggleState = await iconState(toggle);
  assertIconState(toggleState, fallback ? 'mail-action-grid' : 'ctox-action-grid', 'list-to-cards toggle');
  assert.equal(await toggle.getAttribute('data-pg-view'), 'list');
  assert.equal(await toggle.getAttribute('aria-label'), 'Als Karten anzeigen');
  assert.equal(await toggle.getAttribute('title'), 'Als Karten anzeigen');
  assert.equal(await toggle.getAttribute('aria-pressed'), null);
  await toggle.click();
  toggleState = await iconState(toggle);
  assertIconState(toggleState, fallback ? 'mail-action-list' : 'ctox-action-list', 'toggle restored');

  await page.locator('[data-mail-left-pane] [data-pg-band="campaigns"]').click();
  await page.locator('[data-mail-scope-id="campaign-1"]').click();
  const progressSteps = page.locator('[data-mail-record-id="message-delivered-1"] .mail-progress-step');
  await progressSteps.first().waitFor({ state: 'visible' });
  const expectedProgressIcons = ['edit', 'check', 'send', 'check', 'eye', 'link'];
  for (const [index, name] of expectedProgressIcons.entries()) {
    const state = await iconState(progressSteps.nth(index));
    assertIconState(state, fallback ? `mail-action-${name}` : `ctox-action-${name}`, `progress ${name}`);
  }

  await page.locator('[data-mail-close-detail]').click();
  await page.setViewportSize({ width: 700, height: 820 });
  await page.waitForTimeout(100);
  const openNav = await iconState(page.locator('[data-mail-open-nav]'));
  assertIconState(openNav, fallback ? 'mail-action-columns' : 'ctox-action-columns', 'navigation open');
  await page.locator('[data-mail-open-nav]').click();
  const sidebar = page.locator('[data-mail-left-pane]');
  await sidebar.waitFor({ state: 'visible' });
  const closeNav = await iconState(page.locator('[data-mail-close-nav]'));
  assertIconState(closeNav, fallback ? 'mail-action-close' : 'ctox-action-close', 'navigation close');
  await page.locator('[data-mail-close-nav]').click();
  await sidebar.waitFor({ state: 'hidden' });
  await page.setViewportSize({ width: 1440, height: 940 });
  await page.waitForTimeout(100);
  await page.locator('[data-mail-left-pane] [data-pg-band="queues"]').click();
  await page.locator('[data-mail-scope-id="inbound"]').click();
  await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
}

async function assertVisibleText(page, text) {
  await page.getByText(text, { exact: false }).first().waitFor({ state: 'visible' });
}

async function assertEditorThemeBridge(page, editorFrame) {
  const parentTokens = await page.evaluate(() => {
    const styles = getComputedStyle(document.documentElement);
    return ['--surface', '--surface-2', '--text', '--muted', '--line', '--accent']
      .map((name) => [name, styles.getPropertyValue(name).trim()]);
  });
  const frameTokens = await editorFrame.locator('html').first().evaluate((element) => {
    const styles = getComputedStyle(element);
    return ['--surface', '--surface-2', '--text', '--muted', '--line', '--accent']
      .map((name) => [name, styles.getPropertyValue(name).trim()]);
  });
  assert.deepEqual(frameTokens, parentTokens);
}

async function assertEditorViewports(page) {
  const viewports = [
    { width: 1180, height: 820 },
    { width: 960, height: 720 },
    { width: 640, height: 480 },
    { width: 390, height: 844 },
    { width: 360, height: 760 },
  ];
  for (const viewport of viewports) {
    await page.setViewportSize(viewport);
    const geometry = await page.locator('[data-mail-content-surface]').evaluate((surface) => {
      const drawer = surface.querySelector('.mail-content-editor-drawer');
      const commandbar = surface.querySelector('.mail-content-editor-commandbar');
      const controls = surface.querySelector('[data-mail-content-controls]');
      const surfaceBox = surface.getBoundingClientRect();
      const drawerBox = drawer?.getBoundingClientRect();
      return {
        surface: { left: surfaceBox.left, right: surfaceBox.right, width: surfaceBox.width },
        drawer: drawerBox ? { left: drawerBox.left, right: drawerBox.right, width: drawerBox.width } : null,
        commandbarOverflow: commandbar ? commandbar.scrollWidth - commandbar.clientWidth : 0,
        commandbarWidth: commandbar?.clientWidth || 0,
        controlsWidth: controls?.clientWidth || 0,
        modeWidth: commandbar?.querySelector('.mail-content-editor-modes')?.getBoundingClientRect().width || 0,
        toolsWidth: commandbar?.querySelector('.mail-content-editor-html-tools')?.getBoundingClientRect().width || 0,
        panelLabelDisplay: commandbar ? getComputedStyle(commandbar.querySelector('.mail-content-editor-panel-button > span:last-child')).display : '',
      };
    });
    assert.ok(geometry.surface.left >= 0 && geometry.surface.right <= viewport.width + 1);
    assert.ok(geometry.drawer && geometry.drawer.left >= geometry.surface.left - 1 && geometry.drawer.right <= viewport.width + 1);
    assert.ok(geometry.commandbarOverflow <= 1, `Editor header overflow at ${viewport.width}x${viewport.height}: ${JSON.stringify(geometry)}`);
  }
  await page.setViewportSize({ width: 1440, height: 940 });
  await page.waitForTimeout(100);
}

function findChromiumExecutable() {
  const candidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
  ];
  return candidates.find((candidate) => existsSync(candidate));
}

function mimeType(path) {
  return ({
    '.css': 'text/css; charset=utf-8',
    '.html': 'text/html; charset=utf-8',
    '.js': 'text/javascript; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.mjs': 'text/javascript; charset=utf-8',
    '.svg': 'image/svg+xml',
  })[extname(path)] || 'application/octet-stream';
}
