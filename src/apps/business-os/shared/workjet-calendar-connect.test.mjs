import test from 'node:test';
import assert from 'node:assert/strict';
import { connectWorkjetCalendar, CALENDAR_CONNECT_METHOD } from './workjet-calendar-connect.mjs';

const current = () => {};
const start = { action: 'project.calendar.account.connect.start', commandId: 'c-1', provider: 'google' };
const complete = { action: 'project.calendar.account.connect.complete', commandId: 'c-2', attemptId: 'att-1', callbackUrl: 'https://calendar.example.test/callback?code=x&state=y' };

test('start leitet genau {action, provider} an den Kern weiter, ohne commandId', async () => {
  const calls = [];
  const sync = { requestNative: async (method, params, options) => { calls.push({ method, params, options }); return { authorizeUrl: 'https://accounts.google.com/o', attemptId: 'att-1' }; } };
  const res = await connectWorkjetCalendar(sync, start, current);
  assert.deepEqual(calls, [{ method: CALENDAR_CONNECT_METHOD, params: { action: start.action, provider: 'google' }, options: { collection: 'communication_accounts', timeoutMs: 29_000 } }]);
  assert.deepEqual(res, { action: start.action, commandId: 'c-1', authorizeUrl: 'https://accounts.google.com/o', attemptId: 'att-1' });
});

test('complete leitet {action, attemptId, callbackUrl} weiter; Quittung überschreibt action/commandId nicht', async () => {
  let seen;
  const sync = { requestNative: async (_m, params) => { seen = params; return { action: 'x', commandId: 'y', accountId: 'me@example.test' }; } };
  const res = await connectWorkjetCalendar(sync, complete, current);
  assert.deepEqual(seen, { action: complete.action, attemptId: 'att-1', callbackUrl: complete.callbackUrl });
  assert.deepEqual(res, { action: complete.action, commandId: 'c-2', accountId: 'me@example.test' });
});

test('provider capability-probe geht unverändert durch', async () => {
  let seen;
  const sync = { requestNative: async (_m, params) => { seen = params; throw 'provider_unsupported'; } };
  const res = await connectWorkjetCalendar(sync, { ...start, provider: 'capability-probe' }, current);
  assert.equal(seen.provider, 'capability-probe');
  assert.deepEqual(res, { action: start.action, commandId: 'c-1', error: 'provider_unsupported' });
});

test('Fehler kommen als {action, commandId, error} zurück, nie geworfen', async () => {
  const cases = [
    ['permission_denied', 'permission_denied'],
    [new Error('rate_limited'), 'rate_limited'],
    [{ message: 'Unknown native method ctox.workjet.calendar.connect.v1' }, 'unsupported'],
    [Object.assign(new Error('Native WebRTC peer lacks x'), { code: 'CTOX_WEBRTC_CAPABILITY_MISSING' }), 'unsupported'],
    [new Error('Timed out waiting for remote WebRTC request ctox.workjet.calendar.connect.v1'), 'timeout'],
    [new Error('Native WebRTC peer is not connected'), 'unavailable'],
    [new Error('Something odd happened.'), 'connect_failed'],
  ];
  for (const [err, code] of cases) {
    const res = await connectWorkjetCalendar({ requestNative: async () => { throw err; } }, start, current);
    assert.deepEqual(res, { action: start.action, commandId: 'c-1', error: code }, String(err?.message || err));
  }
  assert.deepEqual(await connectWorkjetCalendar(null, start, current), { action: start.action, commandId: 'c-1', error: 'unavailable' });
  assert.deepEqual(await connectWorkjetCalendar({ requestNative: async () => null }, start, current), { action: start.action, commandId: 'c-1', error: 'invalid_receipt' });
});

test('Fremdfelder, Identität und leere Werte erreichen den Kern nie', async () => {
  let calls = 0;
  const sync = { requestNative: async () => { calls++; return {}; } };
  for (const req of [{ ...start, actor: 'other' }, { ...start, token: 's' }, { ...start, provider: '' }, { ...complete, provider: 'google' }, { ...complete, callbackUrl: '' }, { ...complete, attemptId: 'a'.repeat(129) }]) {
    const res = await connectWorkjetCalendar(sync, req, current);
    assert.equal(res.error, 'invalid_request');
  }
  assert.equal(calls, 0);
  await assert.rejects(connectWorkjetCalendar(sync, { ...start, commandId: '' }, current), TypeError);
  await assert.rejects(connectWorkjetCalendar(sync, { action: 'project.calendar.account.connect.other', commandId: 'c' }, current), TypeError);
});

test('Sitzungswechsel vor oder nach dem Kernaufruf → scope_changed statt Ergebnis', async () => {
  let checks = 0;
  const sync = { requestNative: async () => ({ authorizeUrl: 'u' }) };
  const res = await connectWorkjetCalendar(sync, start, () => { if (++checks === 2) throw new Error('changed'); });
  assert.deepEqual(res, { action: start.action, commandId: 'c-1', error: 'scope_changed' });
});
