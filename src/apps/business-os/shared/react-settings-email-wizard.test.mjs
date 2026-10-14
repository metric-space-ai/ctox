import test from 'node:test';
import assert from 'node:assert/strict';

import { __reactSettingsTestHooks as hooks } from './react-settings.js';

function withLang(lang, fn) {
  const previous = globalThis.document;
  globalThis.document = { documentElement: { lang, dataset: {} } };
  try { return fn(); } finally { globalThis.document = previous; }
}

test('Gmail preset sends IMAP transport with Google servers and compact app password', () => {
  const payload = hooks.emailConfigPayload({
    provider: 'gmail',
    data: { emailAddress: ' ctox@gmail.com ', emailPassword: 'abcd efgh ijkl mnop' },
  });
  assert.equal(payload.provider, 'imap');
  assert.equal(payload.address, 'ctox@gmail.com');
  assert.equal(payload.password, 'abcdefghijklmnop');
  assert.equal(payload.imap_host, 'imap.gmail.com');
  assert.equal(payload.imap_port, 993);
  assert.equal(payload.smtp_host, 'smtp.gmail.com');
  assert.equal(payload.smtp_port, 465);
});

test('custom provider maps to the core imap transport and keeps entered servers', () => {
  const payload = hooks.emailConfigPayload({
    provider: 'custom',
    data: { emailAddress: 'a@b.de', emailPassword: 'x', emailImapHost: 'imap.b.de', emailSmtpHost: 'smtp.b.de' },
  });
  assert.equal(payload.provider, 'imap');
  assert.equal(payload.imap_port, 993);
  assert.equal(payload.smtp_port, 587);
  assert.equal(hooks.emailCoreProvider('microsoft365'), 'graph');
  assert.equal(hooks.emailCoreProvider('owa'), 'owa');
});

test('email form keeps entered values and shows the test error', () => {
  const html = withLang('de', () => hooks.emailWizard({
    wizard: 'email',
    step: 'form',
    provider: 'custom',
    error: 'Verbindungstest fehlgeschlagen: login failed',
    data: { emailProvider: 'custom', emailAddress: 'a@b.de', emailPassword: 'pw', emailImapHost: 'imap.b.de', emailSmtpPort: '465' },
  }));
  assert.match(html, /value="a@b\.de"/);
  assert.match(html, /value="pw"/);
  assert.match(html, /value="imap\.b\.de"/);
  assert.match(html, /value="465"/);
  assert.match(html, /role="alert">Verbindungstest fehlgeschlagen: login failed/);
  assert.match(html, /STARTTLS/);
});

test('Gmail is offered as app-password preset in both languages', () => {
  const de = withLang('de', () => hooks.emailWizard({ wizard: 'email', step: 'form', provider: 'gmail', data: {} }));
  assert.match(de, /myaccount\.google\.com\/apppasswords/);
  assert.match(de, /App-Passwort/);
  assert.match(de, /value="imap\.gmail\.com"/);
  const en = withLang('en', () => hooks.emailWizard({ wizard: 'email', step: 'provider', data: {} }));
  assert.match(en, /Set up email/);
  assert.match(en, /IMAP \+ SMTP with an app password/);
  assert.match(en, /Other provider/);
  assert.doesNotMatch(en, /OAuth über Google/);
});

test('failed channel test surfaces the adapter error', () => {
  assert.equal(
    hooks.channelFailureMessage({ ok: false, adapter_result: { ok: false, error: 'IMAP login failed' } }),
    'IMAP login failed',
  );
});

test('one.com, IONOS, Strato and Outlook presets fill servers', () => {
  const one = hooks.emailConfigPayload({ provider: 'onecom', data: { emailAddress: 'm@metric-space.ai', emailPassword: 'x' } });
  assert.deepEqual([one.provider, one.imap_host, one.imap_port, one.smtp_host, one.smtp_port], ['imap', 'imap.one.com', 993, 'send.one.com', 465]);
  assert.equal(hooks.emailConfigPayload({ provider: 'ionos', data: {} }).smtp_host, 'smtp.ionos.de');
  assert.equal(hooks.emailConfigPayload({ provider: 'strato', data: {} }).imap_host, 'imap.strato.de');
  const outlook = hooks.emailConfigPayload({ provider: 'outlook', data: {} });
  assert.deepEqual([outlook.provider, outlook.smtp_host, outlook.smtp_port], ['imap', 'smtp.office365.com', 587]);
});

test('provider detection by domain and MX lookup', async () => {
  assert.deepEqual(await hooks.detectEmailProvider('x@gmail.com', null), { provider: 'gmail', source: 'domain' });
  const seen = [];
  const fakeFetch = async (url) => {
    seen.push(url);
    return { ok: true, json: async () => ({ Answer: [{ data: '10 mx4.pub.mailpod14-cph3.one.com.' }] }) };
  };
  assert.deepEqual(await hooks.detectEmailProvider('michael@metric-space.ai', fakeFetch), { provider: 'onecom', source: 'mx' });
  assert.match(seen[0], /name=metric-space\.ai&type=MX/);
  assert.doesNotMatch(seen[0], /michael/);
  assert.equal(hooks.emailProviderForMxHosts(['1 aspmx.l.google.com.']), 'gmail');
  assert.equal(hooks.emailProviderForMxHosts(['0 x-de.mail.protection.outlook.com']), 'outlook');
  assert.deepEqual(await hooks.detectEmailProvider('a@nowhere.example', async () => { throw new Error('offline'); }), { provider: '', source: '' });
});

test('email form marks settings as draft until the test succeeds', () => {
  const html = withLang('en', () => hooks.emailWizard({ wizard: 'email', step: 'form', provider: 'custom', data: {} }));
  assert.match(html, /Draft: these settings are only saved after a successful test/);
});

test('hub shows active mailbox, replace button and undo bar instead of a dialog', () => {
  const state = {
    accounts: [{ channel: 'email', account_key: 'email:a@b.de', address: 'a@b.de' }],
    pendingDisconnect: null,
  };
  const html = withLang('de', () => hooks.channelsHubPanel(state));
  assert.match(html, /Aktives Postfach/);
  assert.match(html, /data-channel-replace="email:a@b\.de"/);
  assert.match(html, /Postfach ersetzen/);
  assert.match(html, /data-channel-disconnect="email:a@b\.de">Trennen/);
  const pending = withLang('en', () => hooks.channelsHubPanel({ ...state, pendingDisconnect: { accountKey: 'email:a@b.de' } }));
  assert.doesNotMatch(pending, /data-channel-disconnect="email:a@b\.de"/);
  assert.match(pending, /data-channel-disconnect-undo>Undo/);
  assert.match(pending, /email:a@b\.de will be disconnected/);
});
