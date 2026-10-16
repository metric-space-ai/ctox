// Origin: CTOX
// License: Apache-2.0
// node --test suite for the browser v4 handlers. cap.browser is a mock that
// records the engine op + params so the lifecycle→op translation can be
// asserted, and the controller-lease handlers are exercised against the
// in-memory browser_sessions store.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';

import navigate from '../browser.navigate.js';
import tabOpen from '../browser.tab.open.js';
import dialogRespond from '../browser.dialog.respond.js';
import clipboardPaste from '../browser.clipboard.paste.js';
import credentialFill from '../browser.credential.fill.js';
import acquire from '../browser.controller.acquire.js';
import renew from '../browser.controller.renew.js';
import release from '../browser.controller.release.js';
import sessionStart from '../browser.session.start.js';
import authComplete from '../web_stack.auth_assist.complete.js';

const NOW = 1_760_000_000_000;
const browserCaps = () => ({ 'browser.op': async (req) => ({ ok: true, ...req }) });

function sessionData({ controller = 'owner_1', lease = 'lease_1', expires = NOW + 60_000 } = {}) {
  return {
    browser_sessions: {
      sess_1: {
        id: 'sess_1', owner_user_id: 'owner_1', current_url: 'https://old.example',
        controller_user_id: controller, controller_lease_id: lease, controller_lease_expires_at_ms: expires,
      },
    },
  };
}

test('browser.navigate maps to the navigate op with the payload url when the lease holds', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const result = await navigate(cmd('browser.navigate', { session_id: 'sess_1', lease_id: 'lease_1', url: 'https://new.example' }), env.ctx);
  assert.equal(result.op, 'navigate');
  assert.equal(result.params.url, 'https://new.example');
  assert.equal(result.params.timeoutMs, 30000);
});

test('browser.navigate falls back to the session current_url when no url is given', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const result = await navigate(cmd('browser.navigate', { session_id: 'sess_1', lease_id: 'lease_1' }), env.ctx);
  assert.equal(result.params.url, 'https://old.example');
});

test('browser.navigate is forbidden when the controller lease is expired', async () => {
  const env = createTestCtx({ data: sessionData({ expires: NOW - 1 }), now: NOW, caps: browserCaps() });
  const err = await rejects(navigate(cmd('browser.navigate', { session_id: 'sess_1', lease_id: 'lease_1', url: 'https://x' }), env.ctx));
  assert.equal(err.code, 'forbidden');
});

test('browser.navigate is forbidden when the lease id does not match', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const err = await rejects(navigate(cmd('browser.navigate', { session_id: 'sess_1', lease_id: 'other' }), env.ctx));
  assert.equal(err.code, 'forbidden');
});

test('browser.session.start needs no lease (first contact)', async () => {
  const env = createTestCtx({ data: { browser_sessions: {} }, now: NOW, caps: browserCaps() });
  const result = await sessionStart(cmd('browser.session.start', { session_id: 'sess_new', url: 'https://start' }), env.ctx);
  assert.equal(result.op, 'navigate');
  assert.equal(result.params.url, 'https://start');
});

test('browser.tab.open passes tabId and url to tab_open', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const result = await tabOpen(cmd('browser.tab.open', { session_id: 'sess_1', lease_id: 'lease_1', tab_id: 'tab_9', url: 'https://t' }), env.ctx);
  assert.equal(result.op, 'tab_open');
  assert.equal(result.params.tabId, 'tab_9');
  assert.equal(result.params.url, 'https://t');
});

test('browser.dialog.respond forwards accept and value', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const result = await dialogRespond(cmd('browser.dialog.respond', { session_id: 'sess_1', lease_id: 'lease_1', accept: true, value: 'hi' }), env.ctx);
  assert.equal(result.op, 'dialog_respond');
  assert.equal(result.params.accept, true);
  assert.equal(result.params.value, 'hi');
});

test('browser.clipboard.paste delegates to the capability without a value in the payload', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const result = await clipboardPaste(cmd('browser.clipboard.paste', { session_id: 'sess_1', lease_id: 'lease_1' }), env.ctx);
  assert.equal(result.op, 'clipboard_paste');
});

test('browser.credential.fill forwards the credential reference, never a secret', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const result = await credentialFill(cmd('browser.credential.fill', { session_id: 'sess_1', lease_id: 'lease_1', credential_id: 'cred_7', selector: '#pw', field_role: 'password' }), env.ctx);
  assert.equal(result.op, 'credential_fill');
  assert.equal(result.params.credential_ref, 'cred_7');
  assert.equal(result.params.field_role, 'password');
  assert.equal(result.params.password, undefined);
});

test('browser.credential.fill requires a credential reference', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const err = await rejects(credentialFill(cmd('browser.credential.fill', { session_id: 'sess_1', lease_id: 'lease_1' }), env.ctx));
  assert.equal(err.code, 'invalid');
});

test('browser.controller.acquire takes the lease and sets a 120s window', async () => {
  const env = createTestCtx({
    data: { browser_sessions: { sess_1: { id: 'sess_1', owner_user_id: 'owner_1', controller_user_id: '', controller_lease_id: '', controller_lease_expires_at_ms: 0 } } },
    now: NOW,
  });
  const result = await acquire(cmd('browser.controller.acquire', { session_id: 'sess_1', lease_id: 'lease_new' }, { actor: { id: 'owner_1', role: 'admin' } }), env.ctx);
  assert.equal(result.controller_lease_id, 'lease_new');
  assert.equal(result.controller_lease_expires_at_ms, NOW + 120_000);
  const stored = env.doc('browser_sessions', 'sess_1');
  assert.equal(stored.controller_user_id, 'owner_1');
});

test('browser.controller.renew requires the current lease to match', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW });
  const ok = await renew(cmd('browser.controller.renew', { session_id: 'sess_1', lease_id: 'lease_1' }), env.ctx);
  assert.equal(ok.controller_lease_expires_at_ms, NOW + 120_000);
  const bad = await rejects(renew(cmd('browser.controller.renew', { session_id: 'sess_1', lease_id: 'nope' }), env.ctx));
  assert.equal(bad.code, 'conflict');
});

test('browser.controller.release clears the controller fields', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW });
  const result = await release(cmd('browser.controller.release', { session_id: 'sess_1', lease_id: 'lease_1' }), env.ctx);
  assert.equal(result.controller_user_id, '');
  assert.equal(result.controller_lease_expires_at_ms, 0);
  const stored = env.doc('browser_sessions', 'sess_1');
  assert.equal(stored.controller_lease_id, '');
});

test('controller ops fail not_found when the session is absent', async () => {
  const env = createTestCtx({ data: { browser_sessions: {} }, now: NOW });
  const err = await rejects(acquire(cmd('browser.controller.acquire', { session_id: 'ghost', lease_id: 'l' }), env.ctx));
  assert.equal(err.code, 'not_found');
});

test('web_stack.auth_assist.complete requires explicit confirmation', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: { 'browser.auth_assist_complete': async (r) => ({ ok: true, ...r }) } });
  const err = await rejects(authComplete(cmd('web_stack.auth_assist.complete', { session_id: 'sess_1', lease_id: 'lease_1' }), env.ctx));
  assert.equal(err.code, 'invalid');
  const ok = await authComplete(cmd('web_stack.auth_assist.complete', { session_id: 'sess_1', lease_id: 'lease_1', confirmed: true }), env.ctx);
  assert.equal(ok.ok, true);
});

test('a missing session_id is an invalid command', async () => {
  const env = createTestCtx({ data: sessionData(), now: NOW, caps: browserCaps() });
  const err = await rejects(navigate(cmd('browser.navigate', { url: 'https://x' }), env.ctx));
  assert.equal(err.code, 'invalid');
});
