// Workjet „Google Kalender verbinden“ (Workjet #784, Kern ctox#875) in der v4-Brücke.
// ref: shared/workjet-calendar-connect.mjs (V2, Branch shell/plan-step-retry 3b8675aad) – Rumpf wörtlich; v4 schickt
// requestNative als Kern-Befehl gleichen Namens (E25, v2-compat). Einhängen wie V2 app.js (Zweig vor calendar.read).
import { boundedWorkjetProjectText } from '../v2-helpers.js';
// Workjet „Google Kalender verbinden“ (Workjet #784, Kern ctox#875): Start und Abschluss der nativen OAuth-Verbindung
// über die authentifizierte WebRTC-Spur des Gastes. Der Kern parst die Parameter mit deny_unknown_fields; deshalb geht
// nur {action, provider} bzw. {action, attemptId, callbackUrl} hinaus, nie commandId oder Aufruferidentität.
// Fehler kommen als {action, commandId, error:'<code>'} zurück statt geworfen zu werden: geworfene Fehler sieht
// Workjet nur als guest_failed.

export const CALENDAR_CONNECT_METHOD = 'ctox.workjet.calendar.connect.v1';
export const CALENDAR_CONNECT_ACTIONS = Object.freeze([
  'project.calendar.account.connect.start',
  'project.calendar.account.connect.complete',
]);
const START = CALENDAR_CONNECT_ACTIONS[0];
const TIMEOUT_MS = 29_000;
const CODE = /^[a-z][a-z0-9_]{0,63}$/;

const boundedText = (value, max) => typeof value === 'string' && value.length > 0 && value.length <= max;

function errorCode(error) {
  // Der native Peer verwirft mit dem rohen payload.error (Text oder Objekt), nicht immer mit einem Error.
  const message = typeof error === 'string' ? error : String(error?.message || error?.error || '');
  if (CODE.test(message)) return message; // Kern liefert den Code als Fehlertext (permission_denied, rate_limited, …)
  const prefixed = /^([a-z][a-z0-9_]{0,63}):/.exec(message); // „permission_denied: Detail“
  if (prefixed) return prefixed[1];
  const code = String(error?.code || '');
  if (CODE.test(code) && code !== 'native_failed') return code; // v4: Kern-Code ohne Präfix im Fehlerobjekt
  if (code === 'CTOX_WEBRTC_CAPABILITY_MISSING' || /unknown (native )?method|not supported|unsupported/i.test(message)) {
    return 'unsupported';
  }
  if (code === 'REQUEST_TIMEOUT' || /timed out|timeout/i.test(message)) return 'timeout';
  if (/not connected|cancelled/i.test(message)) return 'unavailable';
  return 'connect_failed';
}

/**
 * @param {{requestNative?: Function}} sync  WebRTC-Sync des Gastes
 * @param {object} request                    {action, commandId, provider} | {action, commandId, attemptId, callbackUrl}
 * @param {() => void} assertCurrent          wirft, wenn Sitzung, Datenbank oder Instanz gewechselt hat
 */
export async function connectWorkjetCalendar(sync, request, assertCurrent) {
  const action = request?.action;
  const commandId = request?.commandId;
  if (!CALENDAR_CONNECT_ACTIONS.includes(action) || !boundedText(commandId, 128)) {
    throw new TypeError('Invalid calendar connect request.');
  }
  const fail = (error) => ({ action, commandId, error });
  const start = action === START;
  const keys = start ? ['action', 'commandId', 'provider'] : ['action', 'commandId', 'attemptId', 'callbackUrl'];
  if (Object.keys(request).some((key) => !keys.includes(key))) return fail('invalid_request');
  if (start ? !boundedText(request.provider, 64) : (!boundedText(request.attemptId, 128) || !boundedText(request.callbackUrl, 8192))) {
    return fail('invalid_request');
  }
  // provider unverändert, auch 'capability-probe' (Workjet prüft damit die Verfügbarkeit).
  const params = start
    ? { action, provider: request.provider }
    : { action, attemptId: request.attemptId, callbackUrl: request.callbackUrl };
  if (typeof sync?.requestNative !== 'function') return fail('unavailable'); // v4-allow: debt L-42
  try { assertCurrent(); } catch { return fail('scope_changed'); }
  let timer;
  let receipt;
  try {
    receipt = await Promise.race([
      sync.requestNative(CALENDAR_CONNECT_METHOD, params, { collection: 'communication_accounts', timeoutMs: TIMEOUT_MS }), // v4-allow: debt L-42
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Calendar connect timed out.')), TIMEOUT_MS); }), // v4-allow: code (interner Text, wird zu error:"timeout")
    ]);
  } catch (error) {
    return fail(errorCode(error));
  } finally { clearTimeout(timer); }
  try { assertCurrent(); } catch { return fail('scope_changed'); }
  if (!receipt || typeof receipt !== 'object' || Array.isArray(receipt)) return fail('invalid_receipt');
  const { action: _a, commandId: _c, error: _e, ...rest } = receipt;
  return { action, commandId, ...rest };
}

// ref: app.js (3b8675aad, Zweig 'project.calendar.account.connect.*' in workjetProjectControl)
function scopeOf(env) {
  const { state } = env;
  // wie V2 workjetProjectControl-Kopf (und meeting.js scope): ohne Sitzung wirft die Brücke vor jedem Zweig
  return { ownerUserId: boundedWorkjetProjectText(env.actorContext(state.session).id, 'owner_user_id', 256), requestSession: state.session, requestDb: state.db };
}
async function calendarConnect(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scopeOf(env);
  const sync = state.sync;
  const instance = state.syncConfig?.instance_id || sync?.config?.instance_id || null;
  return connectWorkjetCalendar(sync, request, () => {
    if (state.session !== requestSession || state.db !== requestDb || state.sync !== sync
      || actorContext(state.session).id !== ownerUserId
      || (state.syncConfig?.instance_id || state.sync?.config?.instance_id || null) !== instance) {
      throw new Error('Calendar scope changed.');
    }
  });
}

export const CALENDAR_CONNECT_BRIDGE_ACTIONS = Object.freeze({
  'project.calendar.account.connect.start': calendarConnect,
  'project.calendar.account.connect.complete': calendarConnect,
});
