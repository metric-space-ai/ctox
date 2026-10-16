// Shell v4 – Workjet-Brücke, Aktionsgruppe C (Jour fixe, Kalender, Sprache). Eigentum Strang C (WJB-C).
//
// Muster wie actions/project.js: jede Aktion ist `async (env, request, diagnostics) => response`; env liefert die
// V2-Abhängigkeiten (env.state, env.actorContext, env.newId, env.now) – in Produktion aus v2-compat.js, im Orakel aus
// dem V2-Testaufbau. Rümpfe wörtlich V2 (ref-Anker), nur `state.` → `env.state.`, `Date.now()` → `env.now()`.
// Die V2-Module shared/{speech-dictation,speech-settings-control,workjet-calendar-native,jour-fixe-speech}.mjs sind
// hier wörtlich übernommen (Orakel: ihre V2-Testdateien laufen gegen diese Kopie, tests/oracle.test.mjs Strang C).
//
// Nativkanal (E25): env.state.sync.requestNative(method, …) → api.command(method, …) im Kern.
// Sprachsitzungen (E26 = K-WJ-3, Kernentscheid 16.10.): speech.dictation und project.jour_fixe.speech laufen als
// langlebiger Kernbefehl. `open` startet ihn (seine command_id ist die Sitzung), `write`/`finish`/`cancel` sind eigene
// Befehle mit {session_command_id}, `read` liefert den letzten `running`-Zwischenstand des open-Befehls ohne eigenen
// Befehl. Die Abbildung steckt in sessionSync() unten; der V2-Rumpf sieht weiter genau einen requestNative-Aufruf je
// Operation (die V2-Orakel prüfen genau diese Aufrufe). Annahmen: WJB-C.md, Abschnitt „E26“.
import { JOUR_FIXE_SCHEMA, validateJourFixeValue } from '../../../../shared/workjet-jour-fixe-contract.generated.mjs';
import { CALENDAR_SCHEMA, validateCalendarValue } from '../../../../shared/workjet-calendar-contract.generated.mjs';
import { WORKJET_PROJECT_CONTROL_TIMEOUT_MS, boundedWorkjetProjectText, assertWorkjetProjectPayloadKeys } from '../v2-helpers.js';
// Strang R: narration.read (V2-Modul unverändert, byte-gleich zum Pin e35a116ae; V2-Leser der Bereichs-Hilfen)
import { awaitWorkjetProjectListStep, readWorkjetProjectListRows } from '../v2-helpers.js';
import { readJourFixeNarration, validateNarrationRead } from '../../../../shared/jour-fixe-narration.mjs';
import { unsupportedError, isUnsupported } from '../v2-compat.js';

// =====================================================================================================================
// V2-Module (wörtlich)
// =====================================================================================================================

// ---- ref: shared/speech-dictation.mjs:1-65 -------------------------------------------------------------------------
export const DICTATION_METHOD = 'ctox.workjet.speech.dictation.v1';
export const DICTATION_CAPABILITY = 'ctox-workjet-speech-dictation-v1';
const uuid = value => typeof value === 'string'
  && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
const integer = value => Number.isSafeInteger(value) && value >= 0;
const states = new Set(['open', 'finishing', 'finished', 'canceled', 'failed']);
const errors = new Set(['missing_credential', 'configuration_unavailable', 'backend_unavailable',
  'timeout', 'credentials_rejected', 'access_denied', 'rate_limit', 'quota', 'provider_rejected',
  'backpressure', 'invalid_response', 'transport', 'retired', 'canceled',
  'invalid_sequence_or_audio', 'invalid_finish', 'missing_final']);
function validate(request) {
  if (!request || Array.isArray(request) || request.action !== 'speech.dictation' || !uuid(request.commandId)) {
    throw new TypeError('Invalid dictation request.');
  }
  const fields = ['action', 'commandId', 'op'];
  let valid = false;
  if (request.op === 'open') valid = true;
  else if (['write', 'read', 'finish', 'cancel'].includes(request.op) && uuid(request.streamId)) {
    fields.push('streamId');
    if (request.op === 'write') {
      fields.push('sequence', 'pcmBase64');
      valid = integer(request.sequence) && request.sequence > 0
        && typeof request.pcmBase64 === 'string' && request.pcmBase64.length >= 4
        && request.pcmBase64.length <= 4268 && /^[A-Za-z0-9+/]+={0,2}$/.test(request.pcmBase64);
    } else if (request.op === 'read') {
      fields.push('afterSequence'); valid = integer(request.afterSequence);
    } else valid = true;
  }
  if (!valid || Object.keys(request).some(key => !fields.includes(key))) throw new TypeError('Invalid dictation request.');
}
function result(response, request) {
  const allowed = ['action', 'commandId', 'op', 'streamId', 'state', 'events', 'text', 'error'];
  if (!response || Array.isArray(response) || Object.keys(response).some(key => !allowed.includes(key))
    || response.action !== request.action || response.commandId !== request.commandId || response.op !== request.op
    || !uuid(response.streamId) || request.op !== 'open' && response.streamId !== request.streamId
    || !states.has(response.state) || !Array.isArray(response.events) || response.events.length > 1
    || response.events.some(event => !event || Object.keys(event).some(key => !['sequence', 'text'].includes(key))
      || !integer(event.sequence) || event.sequence < 1 || typeof event.text !== 'string' || event.text.length > 8192)
    || !(response.error === null || errors.has(response.error))
    || !(response.text === null || typeof response.text === 'string' && response.text.length <= 32768)
    || response.state === 'finished' && (typeof response.text !== 'string' || !response.text.trim() || response.error !== null)
    || response.state !== 'finished' && response.text !== null
    || response.state === 'failed' && response.error === null
    || ['failed', 'canceled'].includes(response.state) && response.events.length > 0) {
    throw new Error('Invalid or mismatched dictation response.');
  }
  // Allowlist only. These are transient draft strings, never a meeting receipt.
  return Object.fromEntries(allowed.map(key => [key, response[key]]));
}
/** General draft-only consumer. The selected instance comes from the trusted host. */
export async function requestDictation(sync, nativeInstanceId, request, assertCurrent) {
  validate(request);
  if (typeof sync?.requestNative !== 'function' || typeof nativeInstanceId !== 'string' || !nativeInstanceId) { // v4-allow: debt L-42
    throw new Error('Dictation requires a connected CTOX instance. Open Speech settings.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  assertCurrent();
  const { action, ...operation } = request;
  const response = await sync.requestNative(DICTATION_METHOD, { // v4-allow: debt L-42
    ...operation, scope: { instanceId: nativeInstanceId },
  }, { requiredCapability: DICTATION_CAPABILITY, timeoutMs: request.op === 'open' ? 20_000 : 5_000 });
  assertCurrent();
  return result(response, request);
}

// ---- ref: shared/speech-settings-control.mjs:1-51 ------------------------------------------------------------------
/** Transient typed speech control: no command projection, HTTP fallback or logging. */
export async function requestSpeechSettings(sync, request, assertCurrent) {
  if (typeof sync?.requestNative !== 'function') throw new Error('Speech settings require a connected CTOX instance.'); // v4-allow: debt L-42
  const actions = new Set(['speech.settings.read', 'speech.settings.configure', 'speech.settings.key',
    'speech.settings.voices', 'speech.settings.check', 'speech.settings.check.transcription', 'speech.settings.playback']);
  if (!request || !actions.has(request.action) || typeof request.commandId !== 'string'
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(request.commandId)) {
    throw new Error('Invalid speech settings request.');
  }
  assertCurrent();
  let result;
  try {
    result = await sync.requestNative('ctox.workjet.speech.settings.v1', request, { // v4-allow: debt L-42
      requiredCapability: 'ctox-workjet-speech-settings-v1', timeoutMs: request.action === 'speech.settings.check.transcription' ? 30000 : 25000,
    });
  } catch (error) {
    // v4: eine fehlende Kernroute (unknown_command → code 'unsupported', Nachricht „Unsupported Workjet project control
    // action: …“, enthält nie Eingaben) bleibt „nicht unterstützt“; sonst machte speechSettingsFailure daraus
    // SPEECH_SETTINGS_FAILED = „fehlgeschlagen“ (PORT-PLAN Regel 4).
    if (isUnsupported(error)) throw error;
    // Native/provider exception bodies must never echo an input secret:
    // only fixed messages and codes leave this boundary.
    throw speechSettingsFailure(error);
  }
  assertCurrent();
  if (!result || result.action !== request.action || result.commandId !== request.commandId) {
    throw new Error('Speech settings response belongs to another request.');
  }
  return result;
}

function fixedError(message, code) {
  const error = new Error(message);
  error.code = code;
  return error;
}

/** Maps a native failure to a fixed, secret-free message and code Workjet can explain. */
export function speechSettingsFailure(error) {
  const code = typeof error?.code === 'string' ? error.code : '';
  const message = error instanceof Error ? error.message : '';
  if (code === 'CTOX_WEBRTC_CAPABILITY_MISSING') {
    return fixedError('Speech settings require a newer CTOX core.', 'SPEECH_CORE_OUTDATED');
  }
  if (code === 'PEER_UNAVAILABLE' || message === 'Native WebRTC peer is not connected') { // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
    return fixedError('Native WebRTC peer is not connected', 'PEER_UNAVAILABLE'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  if (code === 'REQUEST_TIMEOUT' || /^Native request [a-zA-Z0-9._-]+ exceeded /u.test(message)) {
    return fixedError('Native request ctox.workjet.speech.settings.v1 exceeded its deadline.', 'REQUEST_TIMEOUT'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  if (/Owner\/Admin/u.test(message)) {
    return fixedError('Speech settings require Owner/Admin.', 'SPEECH_ACCESS_DENIED');
  }
  return fixedError('Speech settings unavailable. Check connection, Owner/Admin access and the installed CTOX version.', 'SPEECH_SETTINGS_FAILED'); // v4-allow: code (V2-Fehlertext, V2-Orakel prüft wörtlich)
}

// ---- ref: shared/workjet-calendar-native.mjs:1-56 ------------------------------------------------------------------
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
  // v4-allow: code (V2-Fehlertext, wörtlich aus shared/workjet-calendar-native.mjs)
  if (typeof sync?.requestNative !== 'function') throw new Error('Calendar transport unavailable.'); // v4-allow: debt L-42
  assertCurrent();
  let timer;
  let response;
  try {
    response = await Promise.race([sync.requestNative(CALENDAR_READ_METHOD, { action: accounts ? 'accounts' : 'events', ...params }, { // v4-allow: debt L-42
      collection: 'communication_accounts', timeoutMs: 29_000,
    }), new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('Calendar read timed out.')), 29_000); // v4-allow: code (V2-Fehlertext, V2-Orakel prüft /timed out/)
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

// ---- ref: shared/jour-fixe-speech-contract.mjs:1-2 + shared/jour-fixe-speech.mjs:1-18 ------------------------------
export const JOUR_FIXE_SPEECH_METHOD = 'ctox.workjet.jour_fixe.speech.v1';
export const JOUR_FIXE_SPEECH_CAPABILITY = 'ctox-workjet-jour-fixe-speech-v1';

/** Shared browser/desktop guest ingress; no Owner transcript append or HTTP. */
export async function requestJourFixeSpeech(sync, nativeInstanceId, request) {
  // v4-allow: code (V2-Fehlertext, wörtlich aus shared/jour-fixe-speech.mjs)
  if (!sync?.requestNative || !nativeInstanceId) throw new Error('Speech transport unavailable.'); // v4-allow: debt L-42
  const allowed = new Set(['action', 'projectId', 'meetingId', 'deckRevision', 'op', 'requestId', 'streamId', 'sequence', 'pcmBase64', 'afterSequence']);
  if (!request || Object.keys(request).some(key => !allowed.has(key))
    || request.action !== 'project.jour_fixe.speech') throw new TypeError('Invalid speech request.');
  const { action, projectId, meetingId, deckRevision, ...operation } = request;
  if (![projectId, meetingId].every(v => typeof v === 'string' && v.trim() === v && v.length > 0 && v.length <= 256)
    || !Number.isSafeInteger(deckRevision) || deckRevision < 1) throw new TypeError('Invalid speech scope.');
  const response = await sync.requestNative(JOUR_FIXE_SPEECH_METHOD, { // v4-allow: debt L-42
    ...operation, scope: { instanceId: nativeInstanceId, projectId, meetingId, deckRevision },
  }, { requiredCapability: JOUR_FIXE_SPEECH_CAPABILITY, timeoutMs: operation.op === 'open' ? 20_000 : 5_000 });
  return { action, projectId, meetingId, deckRevision, op: operation.op, ...response };
}

// =====================================================================================================================
// E26: Sprachsitzungen als langlebiger Kernbefehl (K-WJ-3). Gehört nach v2-compat (siehe WJB-C.md).
// =====================================================================================================================
// Pro App-API ein Sitzungsregister: streamId → {commandId (= Sitzung), latest (letzter Zwischen-/Endstand), done}.
const SESSION_LIMIT = 16;
const sessionRegistry = new WeakMap();
function sessionsOf(api) {
  let map = sessionRegistry.get(api);
  if (!map) { map = new Map(); sessionRegistry.set(api, map); }
  return map;
}

const isUnknownCommand = (text) => /^unknown_command\b/.test(String(text ?? ''));
function nativeFailure(name, s) {
  const text = String(s.error || s.status);
  const code = /^([a-z_]+):/.exec(text)?.[1] || (s.status === 'cancelled' ? 'cancelled' : 'native_failed');
  return Object.assign(new Error(`Native request ${name} failed: ${text}`), { code, status: s.status });
}

// Startet den open-Befehl und liefert den ersten Zwischenstand (running mit result) oder den Endstand.
// Die Frist (V2: 20 s) gilt bis zum ersten Zwischenstand, nicht für die Sitzung; das Ende bestimmt der Kern
// (finish/cancel oder eigener Abbruch). Danach hält das Register den letzten Stand für read.
function openNativeSession(api, method, params, { timeoutMs = 20_000, readResponse } = {}) {
  const handle = api.command(method, JSON.parse(JSON.stringify(params ?? {})), {});
  return new Promise((resolve, reject) => {
    let settled = false;
    let session = null;
    let unsub = null;
    const sessions = sessionsOf(api);
    const drop = () => {
      try { unsub?.(); } catch { /* still */ }
      if (session && sessions.get(session.streamId) === session) sessions.delete(session.streamId);
    };
    const settle = (fn, value) => { if (settled) return; settled = true; clearTimeout(timer); fn(value); };
    const timer = setTimeout(() => {
      settle(reject, Object.assign(new Error(`Native request ${method} exceeded ${timeoutMs} ms.`), { code: 'REQUEST_TIMEOUT', retryable: true })); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
      try { handle.cancel?.(); } catch { /* still */ }
      drop();
    }, Math.max(1, timeoutMs));
    Promise.resolve(handle.result).then((r) => {
      if (r?.status !== 'rejected') return;
      drop();
      if (isUnknownCommand(r.reason) || isUnknownCommand(r.code)) settle(reject, unsupportedError('project', method, r.reason));
      else settle(reject, Object.assign(new Error(`Native command ${method} rejected: ${r.reason || r.detail || 'rejected'}`), { code: /forbidden/.test(String(r.reason || '')) ? 'forbidden' : (r.code || 'rejected') }));
    }, () => {});
    unsub = handle.status.subscribe((s) => {
      if (!s) return;
      if (s.status === 'running' && s.result && typeof s.result === 'object') {
        if (!session) {
          const streamId = s.result.streamId;
          if (typeof streamId === 'string' && streamId) {
            session = { streamId, commandId: handle.id, latest: s.result, done: false, readResponse };
            if (sessions.size >= SESSION_LIMIT) sessions.delete(sessions.keys().next().value);
            sessions.set(streamId, session);
          }
          settle(resolve, s.result);
        } else {
          session.latest = s.result;
        }
        return;
      }
      if (s.status === 'done' || s.status === 'failed' || s.status === 'cancelled') {
        if (session) {
          session.done = true;
          if (s.status === 'done' && s.result && typeof s.result === 'object') session.latest = s.result;
        }
        try { unsub?.(); } catch { /* still */ }
        unsub = null;
        if (s.status === 'done') settle(resolve, s.result ?? null);
        else if (s.status === 'failed' && isUnknownCommand(s.error)) settle(reject, unsupportedError('project', method, s.error));
        else settle(reject, nativeFailure(method, s));
        // Abgeschlossene Sitzung: Endstand bleibt für ein letztes read, bis finish/cancel/Verdrängung sie entfernt.
      }
    });
  });
}

// read-Antworten aus dem letzten Zwischenstand. Annahme A3 (WJB-C.md): ein Zwischenstand hat die Form der
// V2-Nativantwort derselben Methode; events sind nach sequence aufsteigend. read gibt nur Ereignisse nach
// afterSequence zurück (Diktat höchstens 1 = der neueste Schnappschuss, Jour fixe höchstens 32).
function dictationRead(latest, params) {
  const events = (Array.isArray(latest.events) ? latest.events : []).filter((e) => e?.sequence > params.afterSequence).slice(-1);
  return { ...latest, action: 'speech.dictation', commandId: params.commandId, op: 'read', streamId: params.streamId, events };
}
function jourFixeRead(latest, params) {
  const { requestId: _openOnly, ...rest } = latest;
  const events = (Array.isArray(rest.events) ? rest.events : []).filter((e) => e?.sequence > params.afterSequence).slice(-32);
  return { ...rest, op: 'read', streamId: params.streamId, events };
}
const SESSION_METHODS = Object.freeze({ [DICTATION_METHOD]: dictationRead, [JOUR_FIXE_SPEECH_METHOD]: jourFixeRead });

/**
 * Hülle um env.state.sync für die Sprachmethoden. Ohne Kernel-App-API (V2-Orakel, V2-Testaufbau) bleibt sync
 * unverändert, d. h. der V2-Rumpf sieht genau die V2-Aufrufe. Mit App-API (Produktion, v2-compat) gilt E26.
 */
function sessionSync(env, sync) {
  const api = env.api;
  if (!sync || typeof api?.command !== 'function') return sync;
  return {
    config: sync.config,
    async requestNative(method, params = {}, opts = {}) { // v4-allow: debt L-42
      const readResponse = SESSION_METHODS[method];
      if (!readResponse) return sync.requestNative(method, params, opts); // v4-allow: debt L-42
      const sessions = sessionsOf(api);
      if (params.op === 'open') return openNativeSession(api, method, params, { timeoutMs: opts.timeoutMs, readResponse });
      const session = sessions.get(params.streamId);
      if (!session) {
        throw Object.assign(new Error('Speech stream is not open in this shell.'), { code: 'SPEECH_STREAM_CLOSED' });
      }
      if (params.op === 'read') return session.readResponse(session.latest, params);
      const response = await sync.requestNative(method, { ...params, session_command_id: session.commandId }, opts); // v4-allow: debt L-42
      if (params.op === 'finish' || params.op === 'cancel') sessions.delete(params.streamId);
      return response;
    },
  };
}

// =====================================================================================================================
// Aktionen (Rümpfe aus app.js workjetProjectControl)
// =====================================================================================================================

// Gemeinsamer Vorspann aus workjetProjectControl (ref: app.js:13906-13915).
function scope(env) {
  const { state } = env;
  return {
    ownerUserId: boundedWorkjetProjectText(env.actorContext(state.session).id, 'owner_user_id', 256),
    requestSession: state.session,
    requestDb: state.db,
  };
}

// ref: app.js:13918-13928
async function speechDictation(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  const sync = state.sync;
  const instance = boundedWorkjetProjectText(state.syncConfig?.instance_id || sync?.config?.instance_id, 'instanceId', 256);
  return requestDictation(sessionSync(env, sync), instance, request, () => {
    if (state.session !== requestSession || state.db !== requestDb || state.sync !== sync
      || actorContext(state.session).id !== ownerUserId
      || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance) {
      throw new Error('Dictation instance or authority changed.');
    }
  });
}

// ref: app.js:13929-13940
async function speechSettings(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  const sync = state.sync;
  if (typeof sync?.requestNative !== 'function') throw new Error('Speech settings require a connected CTOX instance.'); // v4-allow: debt L-42
  const instance = boundedWorkjetProjectText(state.syncConfig?.instance_id || sync.config?.instance_id, 'native instanceId', 256);
  return requestSpeechSettings(sync, request, () => {
    if (state.session !== requestSession || state.db !== requestDb || state.sync !== sync
      || actorContext(state.session).id !== ownerUserId
      || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance) {
      throw new Error('Speech settings instance or authority changed.');
    }
  });
}

// ref: app.js:13941-13951
async function calendarRead(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  const sync = state.sync;
  const instance = boundedWorkjetProjectText(state.syncConfig?.instance_id || sync?.config?.instance_id, 'native instanceId', 256);
  return readWorkjetCalendar(sync, request, () => {
    if (state.session !== requestSession || state.db !== requestDb || state.sync !== sync
      || actorContext(state.session).id !== ownerUserId
      || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance) {
      throw new Error('Calendar scope changed.');
    }
  });
}

// ref: app.js:14039-14045
async function jourFixeSpeech(env, request) {
  const { state } = env;
  const { requestSession, requestDb } = scope(env);
  const sync = state.sync;
  const instance = boundedWorkjetProjectText(state.syncConfig?.instance_id || sync?.config?.instance_id, 'native instanceId', 256);
  const result = await requestJourFixeSpeech(sessionSync(env, sync), instance, request);
  if (state.session !== requestSession || state.db !== requestDb || state.sync !== sync) throw new Error('Speech scope changed.');
  return result;
}

// ref: app.js:15002-15008 (requireWorkjetSupervisorDataPlane; gehört nach v2-helpers, Strang B braucht dieselbe Hilfe)
async function requireWorkjetSupervisorDataPlane(env) {
  const { state } = env;
  if (!state.commandBus?.dispatch || !state.db?.collection?.('business_commands')) { // v4-allow: debt L-42
    throw new Error('Workjet supervisor control is not ready.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  await state.sync?.startCollection?.('business_commands'); // v4-allow: debt L-42
  return {};
}

// ref: app.js:14078-14240 (Besprechungsänderungen). Ganzer V2-Rumpf inkl. der lokalen Zweige
// narration.local_publish / transcript.local_candidate (nicht im INVENTORY, daher nur PREPARED, nicht freigeschaltet).
const meetingMutationTypes = {
  'project.jour_fixe.meeting.start': 'MeetingTransitionRequest',
  'project.jour_fixe.meeting.end': 'MeetingTransitionRequest',
  'project.jour_fixe.transcript.append': 'AppendTranscriptRequest',
  'project.jour_fixe.narration.local_publish': 'LocalNarrationRequest',
  'project.jour_fixe.transcript.local_candidate': 'LocalTranscriptCandidateRequest',
  'project.jour_fixe.todos.revise': 'ProposeTodosRequest',
  'project.jour_fixe.todos.confirm': 'ConfirmTodosRequest',
  'project.jour_fixe.comment.add': 'AddCommentRequest',
};
async function meetingMutation(env, request) {
  const { state, actorContext } = env;
  const action = boundedWorkjetProjectText(request.action, 'action', 64);
  const { requestSession, requestDb, ownerUserId } = scope(env);
  await requireWorkjetSupervisorDataPlane(env);
  const appending = action === 'project.jour_fixe.transcript.append';
  const localNarration = action === 'project.jour_fixe.narration.local_publish';
  const localCandidate = action === 'project.jour_fixe.transcript.local_candidate';
  const nativeInstanceId = localNarration || localCandidate
    ? boundedWorkjetProjectText(state.syncConfig?.instance_id || state.sync?.config?.instance_id, 'native instanceId', localNarration ? 128 : 256) : null;
  const revising = action === 'project.jour_fixe.todos.revise';
  const confirming = action === 'project.jour_fixe.todos.confirm';
  const commenting = action === 'project.jour_fixe.comment.add';
  const allowedKeys = new Set(['action', 'commandId', 'projectId', 'operationId', 'meetingId', 'expectedRevision']);
  if (appending) allowedKeys.add('turn');
  if (localNarration) for (const key of ['slideId', 'fileId', 'generationId', 'deckRevision', 'audioSha256', 'narrationTextSha256']) allowedKeys.add(key);
  if (localCandidate) for (const key of ['requestId', 'deckRevision', 'text']) allowedKeys.add(key);
  if (revising) for (const key of ['proposalRevision', 'items']) allowedKeys.add(key);
  if (confirming) for (const key of ['proposalRevision', 'expectedGoalRevision']) allowedKeys.add(key);
  if (commenting) for (const key of ['commentId', 'slideId', 'deckRevision', 'x', 'y', 'text']) allowedKeys.add(key);
  assertWorkjetProjectPayloadKeys(request, allowedKeys);
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
  if (!Number.isSafeInteger(request.expectedRevision) || request.expectedRevision < 0
    || request.expectedRevision === Number.MAX_SAFE_INTEGER) {
    throw new TypeError('Workjet meeting expectedRevision must admit a safe next revision.');
  }
  const payload = {
    operation_id: boundedWorkjetProjectText(request.operationId, 'operationId', 128),
    meeting_id: boundedWorkjetProjectText(request.meetingId, 'meetingId', 128),
    expected_revision: request.expectedRevision,
    ...(appending ? { turn: request.turn } : {}),
    ...(localNarration ? { instance_id: nativeInstanceId, project_id: projectId, slide_id: request.slideId,
      file_id: request.fileId, generation_id: request.generationId, deck_revision: request.deckRevision,
      audio_sha256: request.audioSha256, narration_text_sha256: request.narrationTextSha256 } : {}),
    ...(localCandidate ? { request_id: request.requestId, instance_id: nativeInstanceId,
      project_id: projectId, deck_revision: request.deckRevision, text: request.text } : {}),
    ...(revising ? { proposal_revision: request.proposalRevision, items: request.items } : {}),
    ...(confirming ? { proposal_revision: request.proposalRevision, expected_goal_revision: request.expectedGoalRevision } : {}),
    ...(commenting ? { comment_id: request.commentId, slide_id: request.slideId,
      deck_revision: request.deckRevision, x: request.x, y: request.y, text: request.text } : {}),
  };
  const validation = validateJourFixeValue(meetingMutationTypes[action], payload);
  if (validation.ok !== true) throw new TypeError(validation.error);
  if (confirming && (!Number.isSafeInteger(payload.expected_goal_revision)
    || payload.expected_goal_revision < 0 || payload.expected_goal_revision === Number.MAX_SAFE_INTEGER)) {
    throw new TypeError('Workjet confirmation requires a safe next goal revision.');
  }
  if (appending && (payload.turn.meeting_id !== payload.meeting_id
    || payload.turn.speaker !== 'owner' || payload.turn.modality !== 'text'
    || ['audio', 'source_run_id', 'stream_id', 'sentence_end_latency_ms'].some(key => payload.turn[key] != null)
    || payload.turn.sequence < 1 || payload.turn.ended_at_ms < payload.turn.started_at_ms)) {
    throw new TypeError('Owner text cannot claim another speaker or speech provenance.');
  }
  if (localNarration && ![payload.audio_sha256, payload.narration_text_sha256].every(hash => /^[a-f0-9]{64}$/.test(hash))) {
    throw new TypeError('Local narration requires exact lowercase SHA256 hashes.');
  }
  if (localCandidate && (!payload.text.trim() || new TextEncoder().encode(payload.text).length > 4096)) {
    throw new TypeError('Local transcript candidate exceeds the 4096-byte UTF-8 text budget.');
  }
  const expectedPayload = JSON.parse(JSON.stringify(payload));
  const candidateTextHash = localCandidate
    ? [...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(expectedPayload.text)))]
      .map(byte => byte.toString(16).padStart(2, '0')).join('') : null;
  const sameValue = (left, right) => {
    if (left === right) return true;
    if (!left || !right || typeof left !== 'object' || typeof right !== 'object'
      || Array.isArray(left) !== Array.isArray(right)) return false;
    const keys = Object.keys(left);
    return keys.length === Object.keys(right).length
      && keys.every(key => Object.hasOwn(right, key) && sameValue(left[key], right[key]));
  };
  const assertCurrentIdentity = () => {
    if (state.session !== requestSession || state.db !== requestDb
      || actorContext(state.session).id !== ownerUserId
      || ((localNarration || localCandidate) && (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== nativeInstanceId)) {
      throw new Error('Workjet project session changed before the meeting mutation was delivered.');
    }
  };
  assertCurrentIdentity();
  const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId, command_id: commandId, module: 'ctox', record_id: projectId,
    command_type: `ctox.workjet.${action.slice('project.'.length)}`, inbound_channel: 'ctox',
    payload: JSON.parse(JSON.stringify(expectedPayload)),
    client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
  }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
  assertCurrentIdentity();
  if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
    || receipt.target_record_id !== projectId || receipt.result?.ok !== true
    || receipt.result.contract !== JOUR_FIXE_SCHEMA
    || !sameValue(receipt.payload, { ...expectedPayload, inbound_channel: 'ctox' })) {
    throw new Error('Für den Besprechungsauftrag liegt keine passende Erfolgsbestätigung vor.');
  }
  const mutation = receipt.result.mutation;
  const resultValidation = validateJourFixeValue('MeetingMutationReceipt', mutation);
  if (resultValidation.ok !== true) throw new TypeError(resultValidation.error);
  const confirmedGoal = confirming ? receipt.result.goal : null;
  if (confirming) {
    const goalValidation = validateJourFixeValue('GoalRef', confirmedGoal);
    if (goalValidation.ok !== true) throw new TypeError(goalValidation.error);
    if (!confirmedGoal.goal_id.trim() || confirmedGoal.revision !== expectedPayload.expected_goal_revision + 1) {
      throw new Error('Workjet confirmation did not return the requested next goal revision.');
    }
  }
  const expectedState = confirming ? 'confirmed' : action === 'project.jour_fixe.meeting.start' || localCandidate ? 'live'
    : action === 'project.jour_fixe.meeting.end' || revising ? 'review' : null;
  if (mutation.operation_id !== expectedPayload.operation_id || mutation.meeting_id !== expectedPayload.meeting_id
    || mutation.project_id !== projectId || mutation.revision !== expectedPayload.expected_revision + 1
    || (expectedState ? mutation.state !== expectedState : !(localNarration ? ['preparing', 'ready'] : ['live', 'review']).includes(mutation.state))
    || (confirming ? mutation.changed_id !== confirmedGoal.goal_id
      : localNarration ? mutation.changed_id !== expectedPayload.slide_id
      : localCandidate ? mutation.changed_id !== receipt.result.local_candidate?.turn_id
      : appending ? mutation.changed_id !== expectedPayload.turn.id
      : commenting ? mutation.changed_id !== expectedPayload.comment_id : mutation.changed_id != null)
    || (revising || confirming ? mutation.todos_revision !== expectedPayload.proposal_revision : mutation.todos_revision != null)) {
    throw new Error('Workjet meeting mutation receipt does not confirm the requested operation.');
  }
  if (localNarration) {
    const stored = receipt.result.local_narration;
    const checked = validateJourFixeValue('LocalNarrationReceipt', stored);
    if (checked.ok !== true) throw new TypeError(checked.error);
    const audio = stored.audio;
    if (stored.operation_id !== expectedPayload.operation_id || stored.instance_id !== nativeInstanceId
      || stored.project_id !== projectId || stored.meeting_id !== expectedPayload.meeting_id
      || stored.slide_id !== expectedPayload.slide_id || stored.deck_revision !== expectedPayload.deck_revision
      || stored.owner_user_id !== receipt.result.owner_user_id || !stored.owner_user_id.trim()
      || stored.revision !== mutation.revision || stored.provider_verified !== false
      || stored.provenance !== 'authenticated_owner_local_audio'
      || audio.provenance !== 'authenticated_owner_local_audio' || !audio.generation_id?.trim()
      || audio.file_id === expectedPayload.file_id || audio.sha256 !== expectedPayload.audio_sha256
      || audio.narration_text_sha256 !== expectedPayload.narration_text_sha256
      || audio.source_run_id !== expectedPayload.operation_id || audio.model !== 'owner-uploaded-local-audio'
      || audio.format !== 'wav' || audio.mime_type !== 'audio/wav' || audio.duration_ms < 1
      || audio.duration_ms > 300000 || audio.synthesis_duration_ms !== 0) {
      throw new Error('Workjet could not verify the saved narration audio.');
    }
  }
  if (localCandidate) {
    const stored = receipt.result.local_candidate;
    const shape = validateJourFixeValue('LocalTranscriptCandidateReceipt', stored);
    if (shape.ok !== true) throw new TypeError(shape.error);
    const nativeOwnerId = boundedWorkjetProjectText(receipt.result.owner_user_id, 'owner_user_id', 256);
    if (stored.operation_id !== expectedPayload.operation_id || stored.request_id !== expectedPayload.request_id
      || stored.instance_id !== nativeInstanceId || stored.project_id !== projectId
      || stored.meeting_id !== expectedPayload.meeting_id || stored.deck_revision !== expectedPayload.deck_revision
      || stored.owner_user_id !== nativeOwnerId || stored.revision !== mutation.revision
      || stored.turn_id !== mutation.changed_id || stored.text_sha256 !== candidateTextHash
      || stored.provider_verified !== false || stored.provenance !== 'authenticated_owner_local_candidate') {
      throw new Error('Local transcript receipt does not confirm the exact Owner candidate and live scope.');
    }
  }
  return { action, commandId, projectId, contract: JOUR_FIXE_SCHEMA,
    mutation: JSON.parse(JSON.stringify(mutation)),
    ...(confirming ? { goal: JSON.parse(JSON.stringify(confirmedGoal)) } : {}),
    ...(localNarration ? { localNarration: JSON.parse(JSON.stringify(receipt.result.local_narration)) } : {}),
    ...(localCandidate ? { localCandidate: JSON.parse(JSON.stringify(receipt.result.local_candidate)) } : {}) };
}

// ref: app.js:14242-14338, nur der Zweig meetingRead (KPI-Zweige gehören Strang A). Die Konstanten des gemeinsamen
// V2-Blocks sind hier ausgewertet: meetingRead = true, configuring = false, validate = validateJourFixeValue.
async function meetingRead(env, request) {
  const { state, actorContext } = env;
  const action = 'project.jour_fixe.meeting.read';
  const { requestSession, requestDb, ownerUserId } = scope(env);
  await requireWorkjetSupervisorDataPlane(env);
  const allowedKeys = new Set(['action', 'commandId', 'projectId']);
  allowedKeys.add('meetingId');
  assertWorkjetProjectPayloadKeys(request, allowedKeys);
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
  const payload = { project_id: projectId };
  if (request.meetingId !== undefined) {
    payload.meeting_id = boundedWorkjetProjectText(request.meetingId, 'meetingId', 128);
  }
  const requestValidation = validateJourFixeValue('ReadMeetingRequest', payload);
  if (requestValidation.ok !== true) throw new TypeError(requestValidation.error);
  const assertCurrentIdentity = () => {
    if (state.session !== requestSession || state.db !== requestDb
      || actorContext(state.session).id !== ownerUserId) {
      throw new Error('Die Projektsitzung hat sich vor der Antwort geändert.');
    }
  };
  assertCurrentIdentity();
  const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId, command_id: commandId, module: 'ctox', record_id: projectId,
    command_type: 'ctox.workjet.jour_fixe.meeting.read',
    payload, client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
  }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
  assertCurrentIdentity();
  if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
    || receipt.target_record_id !== projectId || receipt.result?.ok !== true
    || Object.entries(payload).some(([key, value]) => receipt.payload?.[key] !== value)) {
    throw new Error('Für den Projektauftrag liegt keine passende Erfolgsbestätigung vor.');
  }
  const meeting = receipt.result.meeting;
  if (meeting === null && payload.meeting_id !== undefined) {
    throw new Error('Workjet explicitly requested meeting was not confirmed.');
  }
  if (meeting !== null) {
    const meetingValidation = validateJourFixeValue('Meeting', meeting);
    if (meetingValidation.ok !== true) throw new TypeError(meetingValidation.error);
    if (meeting.project_id !== projectId || !Number.isSafeInteger(meeting.revision)
      || !Number.isSafeInteger(meeting.deck_revision)
      || (payload.meeting_id !== undefined && meeting.id !== payload.meeting_id)) {
      throw new Error('Workjet meeting result belongs to another project or meeting.');
    }
  }
  const result = { action, commandId, projectId, contract: JOUR_FIXE_SCHEMA, meeting: JSON.parse(JSON.stringify(meeting)) };
  if (receipt.result.preparation_task_id != null) {
    if (meeting === null || typeof receipt.result.preparation_task_id !== 'string') {
      throw new Error('Die Vorbereitung benötigt eine bestätigte Besprechung und einen zugeordneten Auftrag.');
    }
    result.preparationTaskId = boundedWorkjetProjectText(receipt.result.preparation_task_id, 'preparationTaskId', 256);
  }
  return result;
}

// project.jour_fixe.narration.read (freigeschaltet WJB-R, E26 / K-WJ-2). ref: app.js:13952-14004, wörtlich V2.
// Ersetzungen nach PORT-PLAN Regel 2: state → env.state, Date.now() → env.now(), crypto.randomUUID() → env.newId()
// (wie Strang A in projectPresentation); fehlt der Loader, „nicht unterstützt“ (siehe readRange). Der rekursive
// V2-Aufruf workjetProjectControl({action: 'project.jour_fixe.meeting.read', …}) geht direkt an meetingRead(env, …)
// (der Router davor validiert nur die Aktion).
// Das Bereichslesen bleibt V2: bridge.state.demandFileLoader.fetchFile(fileId, {range}); v2-compat.js startCollection
// bildet es auf api.files.get({id, range:{start, end}}) ab (WJB-R).
async function narrationRead(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env); // ref: app.js:13906-13915 (Vorspann)
  const now = typeof env.now === 'function' ? env.now : Date.now;
  validateNarrationRead(request);
  const requestSync = state.sync;
  const instance = boundedWorkjetProjectText(
    state.syncConfig?.instance_id || requestSync?.config?.instance_id, 'native instanceId', 256,
  );
  const deadline = env.now() + WORKJET_PROJECT_CONTROL_TIMEOUT_MS - 1000;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), Math.max(0, deadline - env.now()));
  const authority = {};
  let bridge;
  const assertCurrent = () => {
    if (controller.signal.aborted || env.now() >= deadline || state.session !== requestSession
      || state.db !== requestDb || state.sync !== requestSync
      || actorContext(state.session).id !== ownerUserId
      || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance
      || (authority.peer && (authority.peer.cancelled
        || authority.peer.collection.demandLoader !== authority.loader
        || authority.peer.collectionQueryGenerationToken?.(authority.peer.activeRemotePeerId) !== authority.generation))) {
      throw Object.assign(new Error('Narration scope or connection changed.'), { code: 'NARRATION_SCOPE_CHANGED' });
    }
  };
  try {
    return await readJourFixeNarration(request, {
      assertCurrent,
      readMeeting: async scope => {
        const response = await awaitWorkjetProjectListStep(meetingRead(env, {
          action: 'project.jour_fixe.meeting.read', commandId: env.newId(),
          projectId: scope.projectId, meetingId: scope.meetingId,
        }), deadline, 'meeting narration scope', now);
        return response.meeting;
      },
      readMetadata: async fileId => {
        bridge ??= await awaitWorkjetProjectListStep(
          requestSync?.startCollection?.('desktop_files', { pin: false, forceDirect: true }), deadline, 'narration file bridge', now, // v4-allow: debt L-42
        );
        if (!bridge?.state && bridge?.ready) {
          bridge = await awaitWorkjetProjectListStep(bridge.ready, deadline, 'narration file bridge readiness', now);
        }
        const rows = await readWorkjetProjectListRows(bridge, {
          selector: { id: { $eq: fileId } }, limit: 1,
        }, env.newId(), deadline, controller.signal, authority, now);
        return rows[0]?.toJSON?.() || rows[0] || null;
      },
      readRange: (fileId, range) => {
        const loader = bridge?.state?.demandFileLoader;
        // v4-Abweichung wie Strang A (projectPresentation): ohne Loader (Kernel ohne files.get) Kern-Lücke →
        // „nicht unterstützt“ statt V2 „Narration file transport is …“ (Workjet blendet aus statt „fehlgeschlagen“).
        if (!loader?.fetchFile) throw unsupportedError('project', 'project.jour_fixe.narration.read', 'K-WJ-2: files.get ohne range');
        return awaitWorkjetProjectListStep(loader.fetchFile(fileId, { range }), deadline, 'narration bytes', now);
      },
    });
  } finally { clearTimeout(timer); controller.abort(); }
}

// Freigeschaltete Aktionen dieser Gruppe (nur mit grünem Orakel und Prüfer, PORT-PLAN Regel 3).
export const MEETING_ACTIONS = Object.freeze({
  'project.jour_fixe.meeting.read': meetingRead,
  'project.jour_fixe.meeting.start': meetingMutation,
  'project.jour_fixe.meeting.end': meetingMutation,
  'project.jour_fixe.todos.revise': meetingMutation,
  'project.jour_fixe.todos.confirm': meetingMutation,
  'project.jour_fixe.comment.add': meetingMutation,
  'project.jour_fixe.transcript.append': meetingMutation,
  'project.jour_fixe.speech': jourFixeSpeech,
  'project.calendar.accounts.read': calendarRead,
  'project.calendar.events.read': calendarRead,
  'speech.dictation': speechDictation,
  'speech.settings.read': speechSettings,
  'speech.settings.configure': speechSettings,
  'speech.settings.key': speechSettings,
  'speech.settings.playback': speechSettings,
  'speech.settings.voices': speechSettings,
  'speech.settings.check': speechSettings,
  'speech.settings.check.transcription': speechSettings,
  'project.jour_fixe.narration.read': narrationRead, // Strang R (E26 / K-WJ-2)
});

// Portiert, aber nicht freigeschaltet: lokale Jour-fixe-Zweige (nicht im gemessenen INVENTORY, Workjet ruft sie heute
// nicht über die Brücke). narration.read ist seit WJB-R freigeschaltet (E26 / K-WJ-2).
export const PREPARED = Object.freeze({
  'project.jour_fixe.narration.local_publish': meetingMutation,
  'project.jour_fixe.transcript.local_candidate': meetingMutation,
});

// Für Tests der E26-Abbildung (bridge-nahe Prüfungen in tests/oracle.test.mjs, Strang C).
export const __e26 = Object.freeze({ sessionSync, sessionsOf });
