// Origin: CTOX
// License: AGPL-3.0-only
import { CALENDAR_SCHEMA, validateCalendarValue } from './workjet-calendar-contract.generated.mjs?v=20261009-shell-v2-computer-ssh-key';

export const CALENDAR_READ_METHOD = 'ctox.workjet.calendar.read.v1';
function requireCalendarValue(type, value) {
  const result = validateCalendarValue(type, value);
  if (!result.ok) throw new TypeError(result.error);
}


/** Read a registered account through the authenticated guest's WebRTC lane. */
export async function readWorkjetCalendar(sync, request, assertCurrent) {
  const accounts = request?.action === 'project.calendar.accounts.read';
  const events = request?.action === 'project.calendar.events.read';
  const keys = accounts ? ['action', 'commandId'] : ['action', 'commandId', 'accountId', 'startMs', 'endMs'];
  if ((!accounts && !events) || Object.keys(request).some(key => !keys.includes(key))) {
    throw new TypeError('Invalid calendar request.');
  }
  const params = accounts
    ? { request_id: request.commandId }
    : { request_id: request.commandId, account_id: request.accountId, start_ms: request.startMs, end_ms: request.endMs };
  requireCalendarValue(accounts ? 'CalendarAccountsReadRequest' : 'CalendarEventsReadRequest', params);
  if (events && request.endMs - request.startMs > 400 * 86_400_000) throw new TypeError('Calendar range exceeds 400 days.');
  if (typeof sync?.requestNative !== 'function') throw new Error('Calendar transport unavailable.');
  assertCurrent();
  let timer;
  let response;
  try {
    response = await Promise.race([sync.requestNative(CALENDAR_READ_METHOD, { action: accounts ? 'accounts' : 'events', ...params }, {
      collection: 'communication_accounts', timeoutMs: 29_000,
    }), new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('Calendar read timed out.')), 29_000);
    })]);
  } finally { clearTimeout(timer); }
  assertCurrent();
  if (!response || Object.keys(response).some(key => !['schema', 'request_id', 'action', 'data'].includes(key))
    || response.schema !== CALENDAR_SCHEMA || response.request_id !== request.commandId
    || response.action !== (accounts ? 'accounts' : 'events') || response.data?.ok !== true) {
    throw new Error('Invalid calendar receipt.');
  }
  requireCalendarValue(accounts ? 'CalendarAccountsPage' : 'CalendarEventsPage', response.data);
  if (accounts) {
    if (new Set(response.data.accounts.map(account => account.id)).size !== response.data.accounts.length
      || new Set(response.data.accounts.map(account => account.calendar_id)).size !== response.data.accounts.length) {
      throw new Error('Duplicate calendar account identity.');
    }
  } else if (response.data.events.some(event => event.kind !== 'synced' || event.account_id !== request.accountId)
    || new Set(response.data.events.map(event => event.id)).size !== response.data.events.length) {
    throw new Error('Calendar receipt belongs to another account.');
  }
  return {
    action: request.action, commandId: request.commandId, calendar: response.data,
    ...(events ? { accountId: request.accountId, startMs: request.startMs, endMs: request.endMs } : {}),
  };
}
