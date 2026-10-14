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
