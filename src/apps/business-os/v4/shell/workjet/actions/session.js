// Shell v4 – Workjet-Brücke, Aktionsgruppe D: Sitzung (globalThis.workjetSessionControl) und Sitzungsereignisse
// (globalThis.workjetSessionEvents). Eigentum Strang D.
// Rumpf wörtlich V2 (Pin e35a116ae): app.js:15575-15926 (Sitzungssteuerung) und app.js:472-648 (Ereignisse), mit den
// Ersetzungen aus PORT-PLAN Regel 2. Weitere, nötige Ersetzungen (Begründung in WJB-D.md):
//   - Hilfen mit V2-`state` bekommen `env` als ersten Parameter; `waitForSyncBridgeReady(env, …)` aus computer.js.
//   - `globalThis.workjetHostBridge` → `hostBridge(env)` (Orakel-Kontext ≠ Modul-Realm; Produktion: globalThis).
//   - die V2-Hilfe „Datenebene bereit“ (app.js, außerhalb des Abschnitts) → `kernelReady(env)`: der v4-Kernel ist beim
//     Einhängen bereit; env darf sie ersetzen (Orakel: die V2-Testattrappe).
//   - `dispatchWorkjetSessionCommand` / `readTerminalCommandOutcome` heißen im V2-Rumpf …V2; die gleichnamigen
//     Hüllen unten merken sich die Kern-Quittung und lesen das Endergebnis über die Kern-Befehls-ID (K-WJ-4: der
//     v4-Kernel vergibt die business_commands-ID selbst, V2 liest das Dokument unter der eigenen commandId).
// Workjet kennt für Sitzung/Ereignisse nur „Global fehlt“ als „nicht unterstützt“: index.js hängt die Brücke nur
// vollständig (alle 6 Aktionen) ein.
import { waitForSyncBridgeReady, awaitLocalReadView, WORKJET_LOCAL_READ_BUDGET_MS } from './computer.js';
import { unsupportedError } from '../v2-compat.js'; // E32

const hostBridge = (env) => ('hostBridge' in env ? env.hostBridge : globalThis.workjetHostBridge);
const kernelReady = (env) => (typeof env.kernelReady === 'function' ? env.kernelReady() : Promise.resolve());

// ---- ref: app.js:472-648 (Sitzungsereignisse) -------------------------------------------------------------------
const WORKJET_SESSION_EVENT_BUFFER_LIMIT = 64;
const WORKJET_SESSION_EVENT_DEDUPE_LIMIT = 1024;
const WORKJET_SESSION_TRANSFER_TERMINAL_STATES = new Set(['completed', 'rolled_back', 'failed']);

function createWorkjetSessionEvents(env) {
  let registeredComputerIds = new Set();
  let transferCollection = null;
  let transferSubscription = null;
  let startPromise = null;
  let generation = 0;
  const bufferedEvents = [];
  const emittedKeys = new Set();
  const emittedKeyOrder = [];

  const matchesRegistration = (event) => registeredComputerIds.has(event.sourceComputerId)
    || registeredComputerIds.has(event.targetComputerId);

  const bufferEvent = (event) => {
    bufferedEvents.push(event);
    while (bufferedEvents.length > WORKJET_SESSION_EVENT_BUFFER_LIMIT) bufferedEvents.shift();
  };

  const postEvent = (event) => {
    const post = hostBridge(env)?.postSessionTransferEvent;
    if (typeof post !== 'function') {
      bufferEvent(event);
      return;
    }
    try {
      const result = post.call(hostBridge(env), event);
      result?.catch?.(() => bufferEvent(event));
    } catch (error) {
      bufferEvent(event);
    }
  };

  const flushBufferedEvents = () => {
    if (!bufferedEvents.length) return;
    const pending = bufferedEvents.splice(0, bufferedEvents.length);
    for (const event of pending) {
      if (matchesRegistration(event)) postEvent(event);
    }
  };

  const rememberEvent = (event) => {
    const key = `${event.transferId}\0${event.state}\0${event.updatedAtMs}`;
    if (emittedKeys.has(key)) return false;
    emittedKeys.add(key);
    emittedKeyOrder.push(key);
    while (emittedKeyOrder.length > WORKJET_SESSION_EVENT_DEDUPE_LIMIT) {
      emittedKeys.delete(emittedKeyOrder.shift());
    }
    return true;
  };

  const emitRecord = (value) => {
    let event = null;
    try {
      event = boundedWorkjetSessionTransferEvent(value?.toJSON?.() || value);
    } catch (error) {
      return;
    }
    if (!event || !matchesRegistration(event) || !rememberEvent(event)) return;
    flushBufferedEvents();
    postEvent(event);
  };

  const ensureStarted = () => {
    if (transferSubscription || startPromise || registeredComputerIds.size === 0) {
      return startPromise || Promise.resolve();
    }
    const expectedGeneration = generation;
    const startup = (async () => {
      await kernelReady(env);
      if (expectedGeneration !== generation || registeredComputerIds.size === 0) return;
      const collection = env.state.db?.collection?.('workjet_session_transfers');
      if (!collection?.$?.subscribe) {
        throw new Error('workjet_session_transfers collection is not registered.');
      }
      const transferBridge = await env.state.sync?.startCollection?.('workjet_session_transfers'); // v4-allow: debt L-42
      await waitForSyncBridgeReady(env, transferBridge, 15_000);
      if (expectedGeneration !== generation || registeredComputerIds.size === 0) return;
      transferCollection = collection;
      transferSubscription = collection.$.subscribe((change) => {
        emitRecord(change?.documentData || change?.document || change);
      });
    })();
    startPromise = startup;
    startup.catch(() => {
      if (startPromise === startup) startPromise = null;
    });
    return startup;
  };

  return Object.freeze({
    register(request = {}) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) {
        throw new TypeError('Workjet session event registration must be an object.');
      }
      const unexpected = Object.keys(request).filter((key) => key !== 'computerIds');
      if (unexpected.length) {
        throw new TypeError(`Unsupported Workjet session event registration field: ${unexpected[0]}`);
      }
      if (!Array.isArray(request.computerIds) || request.computerIds.length > 256) {
        throw new TypeError('Invalid Workjet session event computerIds.');
      }
      registeredComputerIds = new Set(request.computerIds.map((computerId) => (
        boundedWorkjetSessionText(computerId, 'event.computerId', 256)
      )));
      flushBufferedEvents();
      ensureStarted().catch((error) => {
        console.warn('[business-os] Workjet session transfer event stream unavailable', error);
      });
      return Object.freeze({ registered: registeredComputerIds.size });
    },
    unregister() {
      generation += 1;
      registeredComputerIds.clear();
      bufferedEvents.splice(0, bufferedEvents.length);
      emittedKeys.clear();
      emittedKeyOrder.splice(0, emittedKeyOrder.length);
      try { transferSubscription?.unsubscribe?.(); } catch (error) {}
      transferSubscription = null;
      transferCollection = null;
      startPromise = null;
    },
    async snapshot() {
      if (registeredComputerIds.size === 0) return [];
      await ensureStarted();
      const collection = transferCollection || env.state.db?.collection?.('workjet_session_transfers');
      if (!collection?.find) return [];
      const docs = await collection.find().exec();
      return docs
        .map((doc) => {
          try { return boundedWorkjetSessionTransferEvent(doc?.toJSON?.() || doc); } catch (error) { return null; }
        })
        .filter((event) => event
          && !WORKJET_SESSION_TRANSFER_TERMINAL_STATES.has(event.state)
          && matchesRegistration(event))
        .sort((left, right) => left.updatedAtMs - right.updatedAtMs
          || left.transferId.localeCompare(right.transferId));
    },
  });
}

function boundedWorkjetSessionTransferEvent(value) {
  if (!value || typeof value !== 'object' || value._deleted === true || value.is_deleted === true) {
    return null;
  }
  const fenceEpoch = value.fence_epoch;
  const deadlineAtMs = value.deadline_at_ms;
  const updatedAtMs = value.updated_at_ms;
  if (!Number.isInteger(fenceEpoch) || fenceEpoch < 0
    || !Number.isInteger(deadlineAtMs) || deadlineAtMs < 0
    || !Number.isInteger(updatedAtMs) || updatedAtMs < 0) {
    throw new TypeError('Invalid Workjet session transfer event numbers.');
  }
  return Object.freeze({
    type: 'workjet.session.transfer',
    transferId: boundedWorkjetSessionText(value.id, 'event.transferId', 160),
    sessionId: boundedWorkjetSessionText(value.session_id, 'event.sessionId', 160),
    state: boundedWorkjetSessionText(value.state, 'event.state', 64),
    fenceEpoch,
    sourceComputerId: boundedWorkjetSessionText(
      value.source_computer_id,
      'event.sourceComputerId',
      256,
    ),
    targetComputerId: boundedWorkjetSessionText(
      value.target_computer_id,
      'event.targetComputerId',
      256,
    ),
    deadlineAtMs,
    updatedAtMs,
  });
}

// ---- ref: app.js:15575-15926 (Sitzungssteuerung) ----------------------------------------------------------------
const WORKJET_SESSION_CONTROL_MAX_RESULTS = 100;
const WORKJET_SESSION_CONTROL_TIMEOUT_MS = 30_000;

async function workjetSessionControl(env, request = {}) {
  if (!request || typeof request !== 'object' || Array.isArray(request)) {
    throw new TypeError('Workjet session control request must be an object.');
  }
  const action = boundedWorkjetSessionText(request.action, 'action', 64);
  const ownerUserId = boundedWorkjetSessionText(env.actorContext(env.state.session).id, 'owner_user_id', 256);

  // E32: session.list ist eine lokale Kernel-Abfrage auf workjet_sessions, kein Kern-Befehl mehr.
  // V2 (app.js:15586-15597 @ e35a116ae) sandte ctox.workjet.session.list nur als Schranke und verwarf das Ergebnis;
  // die Workjet-Antwort entstand schon immer aus den projizierten Zeilen (listProjectedWorkjetSessions, unverändert V2).
  if (action === 'session.list') {
    assertWorkjetSessionPayloadKeys(request, new Set(['action']));
    const view = await awaitLocalReadView(env, 'workjet_sessions', WORKJET_LOCAL_READ_BUDGET_MS);
    if (!view.ready) throw unsupportedError('session', 'session.list', view.detail);
    return {
      action: 'session.list',
      sessions: await listProjectedWorkjetSessions(env, ownerUserId),
    };
  }

  const { sessionBridge } = await requireWorkjetSessionDataPlane(env);

  if (action === 'session.create') {
    assertWorkjetSessionPayloadKeys(request, new Set([
      'action', 'commandId', 'sessionId', 'projectId', 'workingCopyId', 'threadId', 'codingSessionId',
    ]));
    const commandId = boundedWorkjetSessionText(request.commandId, 'commandId', 128);
    const sessionId = boundedOptionalWorkjetSessionText(request.sessionId, 'sessionId', 160);
    const projectId = boundedWorkjetSessionText(request.projectId, 'projectId', 160);
    const workingCopyId = boundedWorkjetSessionText(request.workingCopyId, 'workingCopyId', 160);
    const threadId = boundedOptionalWorkjetSessionText(request.threadId, 'threadId', 160);
    const codingSessionId = boundedOptionalWorkjetSessionText(request.codingSessionId, 'codingSessionId', 160);
    const payload = { project_id: projectId, working_copy_id: workingCopyId };
    if (sessionId) payload.session_id = sessionId;
    if (threadId) payload.thread_id = threadId;
    if (codingSessionId) payload.coding_session_id = codingSessionId;
    await dispatchWorkjetSessionCommand(
      env,
      commandId,
      'ctox.workjet.session.create',
      sessionId || projectId,
      payload,
    );
    const outcome = await readTerminalCommandOutcome(env, commandId);
    const projectedSessionId = boundedOptionalWorkjetSessionText(
      outcome?.session?.id,
      'outcome.session.id',
      160,
    ) || sessionId;
    const session = await waitForProjectedWorkjetSession(
      env,
      projectedSessionId,
      ownerUserId,
      sessionBridge,
      WORKJET_SESSION_CONTROL_TIMEOUT_MS,
      { projectId, workingCopyId },
    );
    return { action: 'session.create', session };
  }

  if (action === 'session.transfer.start') {
    assertWorkjetSessionPayloadKeys(request, new Set([
      'action', 'commandId', 'sessionId', 'targetComputerId', 'targetPath', 'idempotencyKey',
    ]));
    const commandId = boundedWorkjetSessionText(request.commandId, 'commandId', 128);
    const sessionId = boundedWorkjetSessionText(request.sessionId, 'sessionId', 160);
    await dispatchWorkjetSessionCommand(
      env,
      commandId,
      'ctox.workjet.session.transfer.start',
      sessionId,
      {
        session_id: sessionId,
        target_computer_id: boundedWorkjetSessionText(request.targetComputerId, 'targetComputerId', 160),
        target_path: boundedWorkjetSessionText(request.targetPath, 'targetPath', 4096),
        idempotency_key: boundedWorkjetSessionText(request.idempotencyKey, 'idempotencyKey', 160),
      },
    );
    return {
      action: 'session.transfer.start',
      outcome: normalizeWorkjetTransferOutcome(await readTerminalCommandOutcome(env, commandId)),
    };
  }

  if (action === 'session.transfer.pause_ack') {
    assertWorkjetSessionPayloadKeys(request, new Set([
      'action', 'commandId', 'transferId', 'computerId', 'fenceEpoch',
      'lastTerminalTurnId', 'gitRepository', 'idempotencyKey',
    ]));
    const commandId = boundedWorkjetSessionText(request.commandId, 'commandId', 128);
    const transferId = boundedWorkjetSessionText(request.transferId, 'transferId', 160);
    const fenceEpoch = request.fenceEpoch;
    if (!Number.isInteger(fenceEpoch) || fenceEpoch < 0) {
      throw new TypeError('Invalid Workjet session fenceEpoch.');
    }
    if (typeof request.gitRepository !== 'boolean') {
      throw new TypeError('Invalid Workjet session gitRepository.');
    }
    await dispatchWorkjetSessionCommand(
      env,
      commandId,
      'ctox.workjet.session.transfer.pause_ack',
      transferId,
      {
        transfer_id: transferId,
        computer_id: boundedWorkjetSessionText(request.computerId, 'computerId', 256),
        fence_epoch: fenceEpoch,
        last_terminal_turn_id: request.lastTerminalTurnId === null || request.lastTerminalTurnId === undefined
          ? null
          : boundedWorkjetSessionText(request.lastTerminalTurnId, 'lastTerminalTurnId', 160),
        git_repository: request.gitRepository,
        idempotency_key: boundedWorkjetSessionText(request.idempotencyKey, 'idempotencyKey', 160),
      },
    );
    return {
      action: 'session.transfer.pause_ack',
      outcome: normalizeWorkjetTransferOutcome(await readTerminalCommandOutcome(env, commandId)),
    };
  }

  if (action === 'session.transfer.status') {
    assertWorkjetSessionPayloadKeys(request, new Set(['action', 'commandId', 'transferId', 'sessionId']));
    const commandId = boundedWorkjetSessionText(request.commandId, 'commandId', 128);
    const transferId = boundedOptionalWorkjetSessionText(request.transferId, 'transferId', 160);
    const sessionId = boundedOptionalWorkjetSessionText(request.sessionId, 'sessionId', 160);
    if (Boolean(transferId) === Boolean(sessionId)) {
      throw new TypeError('Workjet session transfer status requires exactly one of transferId or sessionId.');
    }
    const payload = transferId ? { transfer_id: transferId } : { session_id: sessionId };
    await dispatchWorkjetSessionCommand(
      env,
      commandId,
      'ctox.workjet.session.transfer.status',
      transferId || sessionId,
      payload,
    );
    return {
      action: 'session.transfer.status',
      outcome: normalizeWorkjetTransferOutcome(await readTerminalCommandOutcome(env, commandId)),
    };
  }

  if (action === 'session.transfer.abort') {
    assertWorkjetSessionPayloadKeys(request, new Set([
      'action', 'commandId', 'transferId', 'reason', 'idempotencyKey',
    ]));
    const commandId = boundedWorkjetSessionText(request.commandId, 'commandId', 128);
    const transferId = boundedWorkjetSessionText(request.transferId, 'transferId', 160);
    await dispatchWorkjetSessionCommand(
      env,
      commandId,
      'ctox.workjet.session.transfer.abort',
      transferId,
      {
        transfer_id: transferId,
        reason: boundedWorkjetSessionText(request.reason, 'reason', 512),
        idempotency_key: boundedWorkjetSessionText(request.idempotencyKey, 'idempotencyKey', 160),
      },
    );
    return {
      action: 'session.transfer.abort',
      outcome: normalizeWorkjetTransferOutcome(await readTerminalCommandOutcome(env, commandId)),
    };
  }

  throw new Error(`Unsupported Workjet session control action: ${action}`);
}

async function dispatchWorkjetSessionCommandV2(env, commandId, commandType, recordId, payload) {
  return env.state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId,
    command_id: commandId,
    module: 'ctox',
    command_type: commandType,
    record_id: recordId,
    payload,
    client_context: {
      source: 'workjet-session-control',
      actor: env.actorContext(env.state.session),
    },
  }, { until: 'terminal', timeoutMs: WORKJET_SESSION_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
}

async function requireWorkjetSessionDataPlane(env) {
  if (!env.state.commandBus?.dispatch || !env.state.db?.collection?.('business_commands')) { // v4-allow: debt L-42
    throw new Error('Workjet session control is not ready.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  if (!env.state.db?.collection?.('workjet_sessions')) {
    throw new Error('workjet_sessions collection is not registered.');
  }
  const commandBridge = await env.state.sync?.startCollection?.('business_commands'); // v4-allow: debt L-42
  await waitForSyncBridgeReady(env, commandBridge, 15_000);
  const sessionBridge = await env.state.sync?.startCollection?.('workjet_sessions'); // v4-allow: debt L-42
  await waitForSyncBridgeReady(env, sessionBridge, 15_000);
  return { commandBridge, sessionBridge };
}

async function listProjectedWorkjetSessions(env, ownerUserId) {
  const collection = env.state.db?.collection?.('workjet_sessions');
  const docs = await collection.find({
    selector: { owner_user_id: { $eq: ownerUserId } },
    limit: WORKJET_SESSION_CONTROL_MAX_RESULTS,
  }).exec();
  return docs
    .map((doc) => boundedWorkjetSessionResult(doc?.toJSON?.() || doc))
    .filter(Boolean)
    .sort((left, right) => right.updatedAtMs - left.updatedAtMs || left.id.localeCompare(right.id));
}

async function waitForProjectedWorkjetSession(
  env,
  sessionId,
  ownerUserId,
  bridge,
  timeoutMs,
  expected = {},
) {
  const collection = env.state.db?.collection?.('workjet_sessions');
  const deadline = env.now() + timeoutMs;
  let lastError = null;
  while (env.now() < deadline) {
    try {
      await waitForSyncBridgeReady(env, bridge, Math.max(1, deadline - env.now()));
      let rawSession = null;
      if (sessionId) {
        const doc = await collection.findOne(sessionId).exec();
        rawSession = doc?.toJSON?.() || doc;
      }
      if (!rawSession && expected.projectId && expected.workingCopyId) {
        const docs = await collection.find({
          selector: {
            owner_user_id: { $eq: ownerUserId },
            project_id: { $eq: expected.projectId },
            working_copy_id: { $eq: expected.workingCopyId },
          },
          limit: 2,
        }).exec();
        if (docs.length === 1) rawSession = docs[0]?.toJSON?.() || docs[0];
      }
      if (rawSession?.owner_user_id === ownerUserId
        && (!expected.projectId || rawSession.project_id === expected.projectId)
        && (!expected.workingCopyId || rawSession.working_copy_id === expected.workingCopyId)) {
        const session = boundedWorkjetSessionResult(rawSession);
        if (session) return session;
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const error = new Error(`Workjet session projection did not arrive${sessionId ? ` for ${sessionId}` : ''}.`);
  error.code = 'workjet_session_projection_timeout';
  if (lastError) error.cause = lastError;
  throw error;
}

const WORKJET_TRANSFER_OUTCOME_MAX_TRANSFER_BYTES = 64 * 1024;

// Transfer outcomes cross the Desktop/Mobile bridge as a typed contract:
// camelCase keys, bounded strings, and the raw transfer record only as an
// opaque, size-capped object. The native result keys stay snake_case.
function normalizeWorkjetTransferOutcome(outcome) {
  const text = (value, field, maxLength) => (
    typeof value === 'string' && value.length > 0
      ? boundedWorkjetSessionText(value, field, maxLength)
      : null
  );
  let transfer = null;
  if (outcome.transfer && typeof outcome.transfer === 'object' && !Array.isArray(outcome.transfer)) {
    const encoded = JSON.stringify(outcome.transfer);
    if (encoded.length <= WORKJET_TRANSFER_OUTCOME_MAX_TRANSFER_BYTES) transfer = Object.freeze({ ...outcome.transfer });
  }
  let session = null;
  if (outcome.session && typeof outcome.session === 'object') {
    try { session = boundedWorkjetSessionResult(outcome.session); } catch { session = null; }
  }
  const normalized = { ok: outcome.ok === true, retryable: outcome.retryable === true };
  const transferId = text(outcome.transfer_id, 'outcome.transferId', 160);
  const state = text(outcome.state, 'outcome.state', 64);
  const errorCode = text(outcome.error_code, 'outcome.errorCode', 128);
  const message = text(outcome.message ?? outcome.error, 'outcome.message', 512);
  // Optional keys are omitted, never null: the Desktop contract decodes with
  // excess-property errors and optional (not nullable) fields.
  if (transferId !== null) normalized.transferId = transferId;
  if (state !== null) normalized.state = state;
  if (errorCode !== null) normalized.errorCode = errorCode;
  if (message !== null) normalized.message = message;
  if (session !== null) normalized.session = session;
  if (transfer !== null) normalized.transfer = transfer;
  return Object.freeze(normalized);
}

async function readTerminalCommandOutcomeV2(env, commandId) {
  const collection = env.state.db?.collection?.('business_commands');
  const doc = await collection.findOne(commandId).exec();
  const command = doc?.toJSON?.() || doc;
  if (!command || !['completed', 'failed', 'cancelled'].includes(String(
    command.terminal_status || command.status || '',
  ))) {
    throw new Error(`Terminal Workjet session command document is unavailable for ${commandId}.`); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  const outcome = command.result?.outcome || command.result;
  if (!outcome || typeof outcome !== 'object' || Array.isArray(outcome)) {
    throw new Error(`Terminal Workjet session command outcome is unavailable for ${commandId}.`); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  return Object.freeze({ ...outcome });
}

function boundedWorkjetSessionResult(value) {
  if (!value || typeof value !== 'object' || value._deleted === true || value.is_deleted === true) {
    return null;
  }
  const fenceEpoch = Number(value.fence_epoch);
  const updatedAtMs = Number(value.updated_at_ms);
  if (!Number.isInteger(fenceEpoch) || fenceEpoch < 0 || !Number.isFinite(updatedAtMs) || updatedAtMs < 0) {
    return null;
  }
  return Object.freeze({
    id: boundedWorkjetSessionText(value.id, 'session.id', 160),
    projectId: boundedWorkjetSessionText(value.project_id, 'session.projectId', 160),
    workingCopyId: boundedWorkjetSessionText(value.working_copy_id, 'session.workingCopyId', 160),
    computerId: boundedWorkjetSessionText(value.computer_id, 'session.computerId', 256),
    threadId: boundedOptionalWorkjetSessionText(value.thread_id, 'session.threadId', 160),
    codingSessionId: boundedOptionalWorkjetSessionText(value.coding_session_id, 'session.codingSessionId', 160),
    runStatus: boundedWorkjetSessionText(value.run_status, 'session.runStatus', 64),
    fenceEpoch,
    activeTransferId: boundedOptionalWorkjetSessionText(value.active_transfer_id, 'session.activeTransferId', 160),
    updatedAtMs,
    ...(Number.isSafeInteger(value.created_at_ms) && value.created_at_ms >= 0
      ? { createdAtMs: value.created_at_ms } : {}),
  });
}

function boundedWorkjetSessionText(value, field, maxLength) {
  if (typeof value !== 'string') throw new TypeError(`Invalid Workjet session ${field}.`);
  const normalized = value.trim();
  const hasControlCharacter = Array.from(normalized).some((character) => {
    const code = character.charCodeAt(0);
    return code < 32 || code === 127;
  });
  if (!normalized || [...normalized].length > maxLength || hasControlCharacter) {
    throw new TypeError(`Invalid Workjet session ${field}.`);
  }
  return normalized;
}

function boundedOptionalWorkjetSessionText(value, field, maxLength) {
  if (value === undefined || value === null) return null;
  return boundedWorkjetSessionText(value, field, maxLength);
}

function assertWorkjetSessionPayloadKeys(payload, allowed) {
  const unexpected = Object.keys(payload).filter((key) => !allowed.has(key));
  if (unexpected.length) {
    throw new TypeError(`Unsupported Workjet session payload field: ${unexpected[0]}`);
  }
}

// ---- v4: Endergebnis über die Kern-Befehls-ID (gehört nach v2-compat, WJB-D.md) ----------------------------------
// v2-compat.dispatch liefert die V2-Quittung mit kernel_command_id. V2 liest das Enddokument unter der eigenen
// commandId aus business_commands; im v4-Kernel heißt es kernel_command_id und trägt den Kernstatus done|failed.
// Die Sicht unten bildet genau dieses eine Dokument in V2-Form ab (id = commandId, status completed|failed|cancelled);
// fehlt das Dokument in der lokalen Sicht, gilt der gepushte Endstatus der Quittung.
const KERNEL_RECEIPT_LIMIT = 256;
const kernelReceipts = new WeakMap();
function receiptsOf(env) {
  const key = env.state && typeof env.state === 'object' ? env.state : env;
  if (!kernelReceipts.has(key)) kernelReceipts.set(key, new Map());
  return kernelReceipts.get(key);
}
async function dispatchWorkjetSessionCommand(env, commandId, ...rest) {
  const receipt = await dispatchWorkjetSessionCommandV2(env, commandId, ...rest);
  if (receipt && typeof receipt === 'object' && receipt.kernel_command_id) {
    const map = receiptsOf(env);
    map.delete(commandId);
    map.set(commandId, receipt);
    while (map.size > KERNEL_RECEIPT_LIMIT) map.delete(map.keys().next().value);
  }
  return receipt;
}
const KERNEL_STATUS_V2 = { done: 'completed', completed: 'completed', failed: 'failed', cancelled: 'cancelled' };
function kernelCommandView(env, commandId, receipt) {
  return {
    findOne(id) {
      return {
        async exec() {
          if (id !== commandId) return null;
          const stored = await env.state.db?.collection?.('business_commands')?.findOne?.(receipt.kernel_command_id)?.exec?.();
          const doc = stored?.toJSON?.() || stored;
          const status = KERNEL_STATUS_V2[String(doc?.status || '')] || receipt.status;
          const result = doc && Object.hasOwn(doc, 'result') ? doc.result : receipt.result;
          return { ...(doc || {}), id: commandId, kernel_command_id: receipt.kernel_command_id, status, terminal_status: status, result };
        },
      };
    },
  };
}
async function readTerminalCommandOutcome(env, commandId) {
  const receipt = receiptsOf(env).get(commandId);
  if (!receipt) return readTerminalCommandOutcomeV2(env, commandId);
  const view = kernelCommandView(env, commandId, receipt);
  return readTerminalCommandOutcomeV2({ state: { db: { collection: (name) => (name === 'business_commands' ? view : env.state.db?.collection?.(name)) } } }, commandId);
}

// Freigeschaltete Aktionen (Orakel: workjet-session-control.test.mjs) und Ereignisstrom (workjet-session-events.test.mjs).
export const SESSION_ACTIONS = Object.freeze(Object.fromEntries([
  'session.list', 'session.create', 'session.transfer.start',
  'session.transfer.status', 'session.transfer.abort', 'session.transfer.pause_ack',
].map((action) => [action, (env, request) => workjetSessionControl(env, request)])));

export { createWorkjetSessionEvents };
