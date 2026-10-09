import test from 'node:test';
import assert from 'node:assert/strict';
import { readWorkjetCalendar, CALENDAR_READ_METHOD } from './workjet-calendar-native.mjs';
import { CALENDAR_SCHEMA } from './workjet-calendar-contract.generated.mjs';
const accountsRequest = { action: 'project.calendar.accounts.read', commandId: 'read-1' };
const page = { ok: true, truncated: false, accounts: [{ id: 'mine@example.test', calendar_id: 'account:mine', label: 'Calendar', supported: true }] };
const receipt = data => ({ schema: CALENDAR_SCHEMA, request_id: 'read-1', action: 'accounts', data });
const current = () => {};
test('calendar uses the existing authenticated account lane and correlated receipt', async () => {
  let calls = 0;
  const sync = { requestNative: async (method, params, options) => {
    calls++; assert.equal(method, CALENDAR_READ_METHOD);
    assert.deepEqual(params, { action: 'accounts', request_id: 'read-1' });
    assert.deepEqual(options, { collection: 'communication_accounts', timeoutMs: 29_000 });
    return receipt(page);
  } };
  const result = await readWorkjetCalendar(sync, accountsRequest, current);
  assert.deepEqual(result, { ...accountsRequest, calendar: page });
  assert.equal(calls, 1);
});
test('caller identity, credentials, extra fields and invalid ranges never reach transport', async () => {
  let calls = 0;
  const sync = { requestNative: async () => { calls++; } };
  for (const request of [{ ...accountsRequest, actor: 'other' }, { ...accountsRequest, token: 'secret' },
    { ...accountsRequest, commandId: '' }, { action: 'project.calendar.events.read', commandId: 'read-1', accountId: 'mine', startMs: 2, endMs: 1 },
    { action: 'project.calendar.events.read', commandId: 'read-1', accountId: 'mine', startMs: 0, endMs: 401 * 86_400_000 }]) {
    await assert.rejects(readWorkjetCalendar(sync, request, current));
  }
  assert.equal(calls, 0);
});
test('stale session results and malformed receipts are rejected', async () => {
  for (const response of [{ ...receipt(page), request_id: 'other' }, { ...receipt(page), schema: 'other' },
    receipt({ ...page, ok: false }), receipt({ ...page, accounts: [...page.accounts, ...page.accounts] }), receipt({ ...page, token: 'secret' })]) {
    await assert.rejects(readWorkjetCalendar({ requestNative: async () => response }, accountsRequest, current));
  }
  let checks = 0;
  await assert.rejects(readWorkjetCalendar({ requestNative: async () => receipt(page) }, accountsRequest, () => {
    if (++checks === 2) throw new Error('Session changed');
  }), /Session changed/);
});
test('events preserve range, account and truncation without accepting foreign account data', async () => {
  const request = { action: 'project.calendar.events.read', commandId: 'events-1', accountId: 'mine', startMs: 1, endMs: 2 };
  const event = { id: 'event:1', calendar_id: 'account:mine', kind: 'synced', account_id: 'mine', title: 'Meeting', start_ms: 1, end_ms: 2, all_day: false, timezone: 'UTC', revision: 1 };
  let data = { ok: true, events: [event], truncated: true, synced_at_ms: 3 };
  const sync = { requestNative: async (_method, params) => {
    assert.deepEqual(params, { action: 'events', request_id: 'events-1', account_id: 'mine', start_ms: 1, end_ms: 2 });
    return { schema: CALENDAR_SCHEMA, request_id: 'events-1', action: 'events', data };
  } };
  assert.equal((await readWorkjetCalendar(sync, request, current)).calendar.truncated, true);
  data = { ...data, events: [{ ...event, account_id: 'foreign' }] };
  await assert.rejects(readWorkjetCalendar(sync, request, current), /another account/);
});
