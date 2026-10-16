// Shell v4 – Workjet-Brücke, Aktionsgruppe D (instance.grok.*, instance.providers.*). Eigentum Strang D.
// Beide Gruppen hängen an workjetProjectControl (V2 app.js:14008-14037). Rumpf wörtlich V2:
//   shared/workjet-grok-native.mjs (Pin e35a116ae) und shared/workjet-provider-native.mjs (Pin e35a116ae),
//   die Weiche aus app.js mit `state.` → `env.state.`, `actorContext` → `env.actorContext`.
// Die Exporte requestWorkjetGrok/GROK_METHOD/GROK_CAPABILITY und projectNativeProviderRegistry/
// requestNativeProviders tragen dieselben Namen und Signaturen wie die V2-Module (die V2-Orakel importieren sie).
//
// v4-Zusatz (keine V2-Rumpfänderung): fehlt dem Kern die Route (unknown_command → v2-compat {code:'unsupported'}),
// fangen die V2-Rümpfe das als „Grok control unavailable“ / PROVIDER_CONTROL_UNAVAILABLE (Workjet: core_outdated).
// Die Weiche merkt sich deshalb den unsupported-Fehler des Transports und meldet danach „nicht unterstützt“ in der
// Form, die Workjet für workjetProjectControl erkennt (Nachricht „Unsupported Workjet project control action: …“).
import { boundedWorkjetProjectText } from '../v2-helpers.js';
import { isUnsupported, unsupportedError } from '../v2-compat.js';

// ---- ref: shared/workjet-grok-native.mjs:1-24 (wörtlich) -----------------------------------------------------
// Origin: CTOX
// License: AGPL-3.0-only
export const GROK_METHOD = 'ctox.workjet.grok.v1';
export const GROK_CAPABILITY = 'ctox-workjet-grok-v1';
// v4: Name `actions` → `GROK_ACTIONS` (beide V2-Module stehen in einer Datei; der Provider-Teil hat ein eigenes `actions`).
const GROK_ACTIONS = new Set(['instance.grok.read','instance.grok.start','instance.grok.poll','instance.grok.cancel','instance.grok.check','instance.grok.remove']);
export async function requestWorkjetGrok(sync, request, assertCurrent = () => {}) {
  if (!sync || typeof sync.requestNative !== 'function') throw new Error('Grok native control unavailable'); // v4-allow: debt L-42 · v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  if (!request || typeof request !== 'object' || Array.isArray(request) || !GROK_ACTIONS.has(request.action)
    || request.version !== 1 || typeof request.operationId !== 'string'
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(request.operationId)) throw new Error('Invalid Grok request');
  const allowed = new Set(['version','action','operationId','loginId','modelId']);
  if (Object.keys(request).some(key => !allowed.has(key))) throw new Error('Invalid Grok fields');
  assertCurrent();
  let response;
  try {
    response = await sync.requestNative(GROK_METHOD, request, {requiredCapability:GROK_CAPABILITY,timeoutMs:24000}); // v4-allow: debt L-42
  } catch {
    throw new Error('Grok control unavailable. Check connection, Owner/Admin access and the installed CTOX version.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  assertCurrent();
  if (!response || response.version !== 1 || response.action !== request.action || response.operationId !== request.operationId) throw new Error('Grok response correlation failed');
  return response;
}

// ---- ref: shared/workjet-provider-native.mjs:1-174 (wörtlich) ------------------------------------------------
// Origin: CTOX
// License: AGPL-3.0-only
// Provider metadata and model choices use the existing admitted RxDB command
// plane. This public result is never a credential or an inference permit.
const schema = 'ctox.provider-federation-registry.v1';
const timeoutMs = 25_000;
const actions = new Map([
  ['instance.providers.read', 'ctox.workjet.providers.list'],
  ['instance.providers.adopt', 'ctox.workjet.providers.adopt_native'],
  ['instance.providers.observe', 'ctox.workjet.providers.observe_native'],
  ['instance.providers.models.select', 'ctox.workjet.providers.models.select'],
  ['instance.providers.models.exclude', 'ctox.workjet.providers.models.exclude'],
  ['instance.providers.account.enable', 'ctox.workjet.providers.account.enable'],
  ['instance.providers.account.remove', 'ctox.workjet.providers.account.remove'],
]);

function invalid() { throw new TypeError('Invalid native provider metadata or request.'); }
function object(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid();
  return value;
}
function text(value, max = 256) {
  if (typeof value !== 'string' || !value || value.length > max
    || value.trim() !== value || /[\u0000-\u001f\u007f]/.test(value)) invalid();
  return value;
}
function integer(value, minimum = 0, maximum = Number.MAX_SAFE_INTEGER - 1) {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) invalid();
  return value;
}
function boolean(value) { if (typeof value !== 'boolean') invalid(); return value; }
function models(value, maximum = 256) {
  if (!Array.isArray(value) || value.length > maximum) invalid();
  const result = value.map(value => text(value));
  if (new Set(result).size !== result.length) invalid();
  return result;
}
function nullableInteger(value, min = 0, max = Number.MAX_SAFE_INTEGER - 1) {
  return value === null ? null : integer(value, min, max);
}
function catalog(value) {
  object(value);
  let lastAttempt = null;
  if (value.lastAttempt !== null) {
    const attempt = object(value.lastAttempt);
    const failure = attempt.failure === null ? null : text(attempt.failure, 64);
    if (failure !== null && !/^[a-z][a-z0-9_]*$/.test(failure)) invalid();
    lastAttempt = {
      checkedAtMs: integer(attempt.checkedAtMs, 1),
      httpStatus: nullableInteger(attempt.httpStatus, 100, 599),
      elapsedMs: integer(attempt.elapsedMs),
      retryAfterSeconds: nullableInteger(attempt.retryAfterSeconds),
      failure, success: boolean(attempt.success),
    };
    if (lastAttempt.success && (lastAttempt.httpStatus !== 200 || failure !== null)) invalid();
  }
  const result = {
    observed: boolean(value.observed), fresh: boolean(value.fresh),
    models: models(value.models, 1024),
    lastSuccessAtMs: nullableInteger(value.lastSuccessAtMs, 1), lastAttempt,
  };
  if (result.fresh && (!result.observed || result.lastSuccessAtMs === null || !lastAttempt?.success)) invalid();
  return result;
}
export function projectNativeProviderRegistry(value) {
  object(value);
  if (value.ok !== true || value.schema !== schema || !Array.isArray(value.accounts)
    || value.accounts.length > 256 || !Array.isArray(value.providers) || value.providers.length > 256) invalid();
  const ids = new Set();
  const accounts = value.accounts.map(account => {
    object(account);
    const id = text(account.id);
    const holder = object(account.holder);
    if (holder.kind !== 'ctox_instance' || ids.has(id)) invalid();
    ids.add(id);
    const holderId = text(holder.id);
    const revision = integer(account.revision, 1);
    const reference = object(account.nativeAccountReference);
    if (reference.accountId !== id || reference.holderInstanceId !== holderId
      || reference.accountRevision !== revision) invalid();
    // Do not pass unknown fields, holder-local selectors or credentials through.
    return {
      id, holder: { kind: 'ctox_instance', id: holderId },
      provider: text(account.provider), enabled: boolean(account.enabled),
      credentialReady: boolean(account.credentialReady), revision,
      observedAtMs: integer(account.observedAtMs, 1),
      nativeAccountReference: { accountId: id, holderInstanceId: holderId, accountRevision: revision },
      ...(account.controls === undefined ? {} : { controls: {
        canEnable: boolean(object(account.controls).canEnable),
        canRemove: boolean(account.controls.canRemove),
      } }),
      modelCatalogObserved: boolean(account.modelCatalogObserved),
      modelCatalog: catalog(account.modelCatalog),
      excludedModels: models(account.excludedModels),
      effectiveModels: models(account.effectiveModels, 1024),
      // A GET /models result cannot attest a successful inference request.
      inferenceVerified: false,
    };
  });
  const providersSeen = new Set();
  const providers = value.providers.map(provider => {
    object(provider);
    const name = text(provider.provider);
    if (providersSeen.has(name)) invalid();
    providersSeen.add(name);
    return { provider: name, selection: provider.selection === null ? null : models(provider.selection) };
  });
  if (accounts.some(account => !providersSeen.has(account.provider))) invalid();
  return { ok: true, schema, revision: integer(value.revision), accounts, providers };
}
function commandFor(request) {
  object(request);
  const commandType = actions.get(request.action);
  if (!commandType || request.version !== 1) invalid();
  const operationId = text(request.operationId, 128);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(operationId)) invalid();
  const allowed = new Set(['version', 'action', 'operationId']);
  const payload = {};
  if (request.action === 'instance.providers.observe' || request.action === 'instance.providers.models.exclude'
    || request.action.startsWith('instance.providers.account.')) {
    allowed.add('accountId'); allowed.add('expectedAccountRevision');
    payload.account_id = text(request.accountId);
    payload.expected_account_revision = integer(request.expectedAccountRevision, 1);
  }
  if (request.action.startsWith('instance.providers.account.')) {
    allowed.add('expectedRevision');
    payload.expected_revision = integer(request.expectedRevision);
    if (request.action === 'instance.providers.account.enable') {
      allowed.add('enabled'); payload.enabled = boolean(request.enabled);
    }
  }
  if (request.action === 'instance.providers.models.select') {
    allowed.add('provider'); payload.provider = text(request.provider);
  }
  if (request.action.startsWith('instance.providers.models.')) {
    allowed.add('models'); allowed.add('expectedRevision');
    payload.models = models(request.models);
    payload.expected_revision = integer(request.expectedRevision);
  }
  if (Object.keys(request).some(key => !allowed.has(key))) invalid();
  return { id: operationId, command_id: operationId, module: 'ctox',
    command_type: commandType, record_id: null, payload };
}
export async function requestNativeProviders(request, { dispatch, assertCurrent }) {
  const command = commandFor(request);
  if (typeof dispatch !== 'function' || typeof assertCurrent !== 'function') invalid();
  assertCurrent();
  let timer;
  try {
    const receipt = await Promise.race([
      dispatch(command, { until: 'terminal', sync_queue_tasks: false, timeoutMs }), // v4-allow: debt L-42
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(Object.assign(new Error('Provider request timed out. Refresh to read its current state.'), // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
          { code: 'PROVIDER_CONTROL_TIMEOUT' })), timeoutMs);
      }),
    ]);
    assertCurrent();
    if (!receipt || receipt.command_id !== command.id || receipt.ok !== true
      || receipt.status !== 'completed' || receipt.transport !== 'rxdb-command-bus'
      || (receipt.target_record_id != null && receipt.target_record_id !== '')) {
      throw Object.assign(new Error('The native provider command did not complete. Check Owner/Admin access and the connection.'),
        { code: 'PROVIDER_CONTROL_UNCONFIRMED' });
    }
    const payload = object(receipt.payload);
    if (Object.keys(payload).some(key => key !== 'inbound_channel' && !Object.hasOwn(command.payload, key))
      || Object.entries(command.payload).some(([key, value]) => JSON.stringify(payload[key]) !== JSON.stringify(value))) invalid();
    return { version: 1, action: request.action, operationId: request.operationId,
      registry: projectNativeProviderRegistry(receipt.result) };
  } catch (error) {
    if (['PROVIDER_CONTROL_TIMEOUT', 'PROVIDER_CONTROL_UNCONFIRMED', 'PROVIDER_SCOPE_CHANGED'].includes(error?.code)) throw error;
    throw Object.assign(new Error('Native provider control is unavailable. Check the connection, Owner/Admin access and installed CTOX version.'), // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
      { code: 'PROVIDER_CONTROL_UNAVAILABLE' });
  } finally { clearTimeout(timer); }
}

// ---- Weiche aus workjetProjectControl --------------------------------------------------------------------------
// Gemeinsamer Vorspann (ref: app.js:13906-13915).
function scope(env) {
  const { state } = env;
  return {
    ownerUserId: boundedWorkjetProjectText(env.actorContext(state.session).id, 'owner_user_id', 256),
    requestSession: state.session,
    requestDb: state.db,
  };
}

// v4: merkt sich, ob der Transport „Kern-Route fehlt“ meldete (v2-compat: unknown_command → code 'unsupported').
function unsupportedProbe() {
  let seen = null;
  return {
    get seen() { return seen; },
    async wrap(promise) {
      try { return await promise; } catch (error) {
        if (isUnsupported(error)) seen = error;
        throw error;
      }
    },
  };
}

// v4: v2-compat.dispatch schickt record_id:null als 'null' (String) und setzt kein `transport`. Beides prüft der
// Provider-Rumpf (target_record_id leer, transport 'rxdb-command-bus'). Kern-Befehle laufen in v4 über die
// RxDB/WebRTC-Befehlsebene (kernel.command → business_commands); die Quittung aus v2-compat trägt dafür
// kernel_command_id. Nur solche Quittungen bekommen das V2-Merkmal. Gehört nach v2-compat (siehe WJB-D.md).
function kernelProviderDispatch(dispatch) {
  return async (command, options) => {
    const sent = { ...command };
    if (sent.record_id === null) delete sent.record_id;
    const receipt = await dispatch(sent, options); // v4-allow: debt L-42
    if (receipt && typeof receipt === 'object' && receipt.kernel_command_id && receipt.transport === undefined) {
      return { ...receipt, transport: 'rxdb-command-bus', target_record_id: receipt.target_record_id ?? null };
    }
    return receipt;
  };
}

// ref: app.js:14008-14027 (instance.providers.*)
async function instanceProviders(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  const sync = state.sync;
  const commandBus = state.commandBus; // v4-allow: debt L-42
  const instance = boundedWorkjetProjectText(state.syncConfig?.instance_id || sync?.config?.instance_id, 'native instanceId', 256);
  const assertCurrent = () => {
    if (state.session !== requestSession || state.db !== requestDb || state.sync !== sync
      || state.commandBus !== commandBus || actorContext(state.session).id !== ownerUserId // v4-allow: debt L-42
      || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance) {
      throw Object.assign(new Error('Provider instance or authority changed.'), { code: 'PROVIDER_SCOPE_CHANGED' });
    }
  };
  if (typeof commandBus?.dispatch !== 'function') throw new Error('Provider command transport is unavailable.'); // v4-allow: debt L-42 · v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  const probe = unsupportedProbe();
  const dispatch = kernelProviderDispatch((command, options) => commandBus.dispatch(command, options)); // v4-allow: debt L-42
  try {
    return await requestNativeProviders(request, {
      assertCurrent,
      dispatch: (command, options) => probe.wrap(dispatch({
        ...command,
        client_context: { source: 'workjet-provider-control', actor: actorContext(requestSession) },
      }, options)),
    });
  } catch (error) {
    if (probe.seen) throw unsupportedError('project', request.action, probe.seen.detail || probe.seen.message);
    throw error;
  }
}

// ref: app.js:14028-14037 (instance.grok.*)
async function instanceGrok(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  const sync = state.sync;
  const instance = boundedWorkjetProjectText(state.syncConfig?.instance_id || sync?.config?.instance_id, 'native instanceId', 256);
  const probe = unsupportedProbe();
  const probed = sync && typeof sync.requestNative === 'function' // v4-allow: debt L-42
    ? { requestNative: (...args) => probe.wrap(sync.requestNative(...args)) } // v4-allow: debt L-42
    : sync;
  try {
    return await requestWorkjetGrok(probed, request, () => {
      if (state.session !== requestSession || state.db !== requestDb || state.sync !== sync
        || actorContext(state.session).id !== ownerUserId
        || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance) {
        throw new Error('Grok instance or authority changed.');
      }
    });
  } catch (error) {
    if (probe.seen) throw unsupportedError('project', request.action, probe.seen.detail || probe.seen.message);
    throw error;
  }
}

// Freigeschaltete Aktionen (Orakel: workjet-grok-native.test.mjs, workjet-provider-native.test.mjs).
// Nicht freigeschaltet (unsupported, siehe WJB-D.md): instance.providers.secret (Workjet-Vertrag verbietet die Aktion,
// V2 lehnt sie ab), instance.providers.models.check (V2-Shell e35a116ae kennt die Aktion nicht, kein Rumpf).
export const INSTANCE_ACTIONS = Object.freeze({
  ...Object.fromEntries([...GROK_ACTIONS].map((action) => [action, instanceGrok])),
  ...Object.fromEntries([...actions.keys()].map((action) => [action, instanceProviders])),
});
