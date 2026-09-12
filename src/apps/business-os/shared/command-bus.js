import { withTimeout } from './async-timeout.js';
import { CTOX_COMMAND_AUTHORIZATION } from './command-lifecycle.generated.js';
import {
  DATA_PLANE_NO_PROGRESS_CODE,
  evaluateDataPlaneProgress,
  expectDataPlaneProgress,
  getDefaultDataPlaneProgressMonitor,
  noteDataPlaneProgress,
  releaseDataPlaneProgressExpectation,
  tryDataPlaneStallRepair,
} from '../rxdb/src/v1_5_status.mjs';

const COMMAND_ACCEPT_TIMEOUT_MS = 45000;
const COMMAND_SYNC_READY_TIMEOUT_MS = 45000;
const COMMAND_SYNC_FLUSH_TIMEOUT_MS = 15000;
const COMMAND_SYNC_FLUSH_MAX_TIMEOUT_MS = 120000;
// A follower first gives the elected tab a short chance to flush and then
// opens its own bounded WebRTC bridge when that leader is frozen or blocked
// by a native browser dialog. That failover legitimately takes longer than a
// normal leader-side push, so it needs a separate deadline.
const COMMAND_FOLLOWER_SYNC_FLUSH_TIMEOUT_MS = 45000;
// A cold native runtime may still be materializing the exact collection grant
// baseline when the shell asks for its first capability. That work can exceed
// the normal control-plane request budget on a large Business OS store. Keep a
// bounded cold-start window and, below, share one request across all callers so
// bootstrap/maintenance retries cannot create a server-side request storm.
const COMMAND_CAPABILITY_TIMEOUT_MS = 120000;
const COMMAND_CAPABILITY_TERMINAL_NEGATIVE_CACHE_MS = 10000;
// Transient failures only need enough suppression to avoid an immediate request
// storm. The submit retry waits slightly longer, so it always performs a fresh
// acquisition instead of re-reading this short-lived failure.
const COMMAND_CAPABILITY_TRANSIENT_ANTI_STORM_MS = 100;
const COMMAND_CAPABILITY_REFRESH_RETRY_BACKOFF_MS = 125;
const MAX_COMMAND_DOCUMENT_BYTES = 6 * 1024 * 1024;
const MAX_SIMULTANEOUS_COMMAND_WATCHERS = 128;
const MAX_COMMAND_TIMING_PROBES = 64;
const COMMAND_ROUNDTRIP_MARK_NAMES = Object.freeze([
  'browser_dispatch_started',
  'browser_local_inserted',
  'browser_push_confirmed',
  'native_dispatch_entered',
  'native_handler_completed',
  'native_rxdb_projection_committed',
  'browser_terminal_observed',
]);
const COMMAND_LIFECYCLE_TIMING_MARKS = Object.freeze({
  dispatch_started: 'browser_dispatch_started',
  local_inserted: 'browser_local_inserted',
  push_confirmed: 'browser_push_confirmed',
});
let activeCommandWatcherCount = 0;
const commandTimingProbes = new Map();
// Finite exact-id retries cover normal native handlers even when a multiplexed
// master-change frame is lost. The cumulative 11.175 s window stays below the
// default 15 s command deadline and never performs a collection-wide pull.
const COMMAND_TERMINAL_REVALIDATE_DELAYS_MS = Object.freeze([
  25, 50, 100, 200, 400, 800, 1600, 3000, 5000,
]);

function commandProgressToken(command) {
  if (!command) return '';
  return [
    command.id || '',
    command.status || '',
    command.execution_phase || '',
    command.replication_phase || '',
    command.terminal_status || '',
    command.updated_at_ms || '',
    command.execution_task_id || command.task_id || '',
    command.error_code || '',
  ].join('|');
}

function commandDataPlaneMonitor(sync) {
  return sync?.dataPlaneMonitor || getDefaultDataPlaneProgressMonitor();
}

function expectCommandDataPlaneProgress(sync, collection, token, atMs) {
  return expectDataPlaneProgress(commandDataPlaneMonitor(sync), {
    collection: collection || 'business_commands',
    token,
    atMs,
  });
}

function releaseCommandDataPlaneProgress(sync, collection) {
  return releaseDataPlaneProgressExpectation(
    commandDataPlaneMonitor(sync),
    collection || 'business_commands',
  );
}

function noteCommandDataPlaneProgress(sync, collection, token, atMs) {
  return noteDataPlaneProgress(commandDataPlaneMonitor(sync), {
    collection: collection || 'business_commands',
    token,
    atMs,
  });
}

function evaluateCommandDataPlaneProgress(sync) {
  return evaluateDataPlaneProgress(commandDataPlaneMonitor(sync));
}

async function repairCommandDataPlaneStall(sync, collection, repairFn) {
  return tryDataPlaneStallRepair(
    commandDataPlaneMonitor(sync),
    collection || 'business_commands',
    repairFn,
  );
}

const DEMAND_ONLY_SYNC_COLLECTIONS = new Set([
  'desktop_file_chunks',
  'document_blob_chunks',
  'spreadsheet_blob_chunks',
]);

export function createCommandBus({ db, sync = null, session = null } = {}) {
  return {
    async submit(command) {
      return submitRxdbCommand({ db, sync, session, command });
    },
    async waitForAccepted(commandId, options = {}) {
      return waitForCommandState({ db, sync, commandId, until: 'accepted', options });
    },
    async waitForTerminal(commandId, options = {}) {
      return waitForCommandState({ db, sync, commandId, until: 'terminal', options });
    },
    async resumeTracking(commandId, options = {}) {
      return waitForCommandState({ db, sync, commandId, until: options.until || 'terminal', options });
    },
    activeCommandIds() {
      return readActiveCommandIds();
    },
    async getStatus(commandId) {
      const currentDb = await resolveCommandDb(db);
      return findDoc(currentDb?.raw?.business_commands, commandId, { swallowErrors: false });
    },
    async getStatusesByRecordIds(recordIds, { commandType = '' } = {}) {
      const ids = [...new Set((Array.isArray(recordIds) ? recordIds : [])
        .map(cleanContextText)
        .filter(Boolean))];
      if (!ids.length) return [];
      if (ids.length > MAX_SIMULTANEOUS_COMMAND_WATCHERS) {
        throw commandError('', `Too many command record ids: ${ids.length}`, {
          code: 'query_limit',
          retryable: false,
        });
      }
      const currentDb = await resolveCommandDb(db);
      const collection = currentDb?.raw?.business_commands;
      if (!collection?.find) return [];
      const normalizedType = cleanContextText(commandType);
      if (!normalizedType) {
        throw commandError('', 'commandType is required for record status lookup.', {
          code: 'invalid_query',
          retryable: false,
        });
      }
      const selector = { command_type: { $eq: normalizedType } };
      const docs = await collection.find({
        selector,
        limit: 512,
        requireRevision: `command-record-status:${Date.now()}:${ids.length}`,
      }).exec();
      const recordIdsSet = new Set(ids);
      return docs
        .map((doc) => doc?.toJSON?.() || doc)
        .filter((doc) => doc && recordIdsSet.has(cleanContextText(doc.record_id)));
    },
    subscribe(commandId, observer) {
      return subscribeToCommand({ db, sync, commandId, observer });
    },
    async cancel(commandId, { reason = 'cancelled by user', until = 'terminal' } = {}) {
      const targetCommandId = cleanContextText(commandId);
      if (!targetCommandId) throw commandError('', 'command_id is required for cancellation.', {
        code: 'invalid_transition',
        retryable: false,
      });
      const cancellation = {
        id: `cmd_cancel_${crypto.randomUUID()}`,
        module: 'ctox',
        command_type: 'ctox.command.cancel',
        type: 'ctox.command.cancel',
        record_id: targetCommandId,
        payload: {
          target_command_id: targetCommandId,
          reason: cleanContextText(reason) || 'cancelled by user',
        },
      };
      const receipt = await submitRxdbCommand({ db, sync, session, command: cancellation });
      if (until === 'local') return receipt;
      return receipt.resumeTracking({
        until: until === 'accepted' ? 'accepted' : 'terminal',
      });
    },
    async dispatch(command, options = {}) {
      const commandId = command.id || '';
      const dispatchStartedAt = Date.now();
      if (commandTimingProbeEnabled(command)) {
        rememberCommandTimingProbe(commandId, dispatchStartedAt);
      }
      emitCommandLifecycle(commandId, command.command_type || command.type, 'dispatch_started');
      // `sync_queue_tasks: false` was honoured only on the command; passed as
      // a dispatch option it was ignored and every control command first
      // waited for the queue-task collection (field report 11.09.2026).
      const submitted = options?.sync_queue_tasks === false && command?.sync_queue_tasks !== false
        ? { ...command, sync_queue_tasks: false }
        : command;
      const receipt = await submitRxdbCommand({
        db,
        sync,
        session,
        command: submitted,
        dispatchStartedAt,
      });
      emitCommandLifecycle(receipt.command_id, command.command_type || command.type, 'local_receipt', dispatchStartedAt);
      const until = options.until || command?.until || 'accepted';
      if (until === 'local') return receipt;
      if (until === 'terminal') {
        return receipt.tracking.waitForTerminal({ ...command, ...options });
      }
      if (until !== 'accepted') {
        throw commandError(receipt.command_id, `Unknown command wait target: ${until}`, {
          code: 'invalid_transition',
          retryable: false,
        });
      }
      const accepted = await receipt.tracking.waitForAccepted({ ...command, ...options });
      emitCommandLifecycle(receipt.command_id, command.command_type || command.type, 'accepted', dispatchStartedAt);
      return accepted;
    },
  };
}

// §9.1 capability token: the native side authorizes commands from a signed
// token rather than the (spoofable) browser-asserted actor. The generated v2
// contract currently requires this capability for every mutation and forbids
// an unauthorised offline intent. Cached until just before expiry.
let capabilityTokenCache = {
  token: null,
  expiresAtMs: 0,
  failureUntilMs: 0,
  failureCode: '',
  failureTransient: false,
};
let capabilityTokenRequestInFlight = null;

export async function getBusinessOsCapabilityToken({
  timeoutMs = COMMAND_CAPABILITY_TIMEOUT_MS,
} = {}) {
  const result = await acquireBusinessOsCapabilityToken({ timeoutMs });
  return result.token;
}

async function acquireBusinessOsCapabilityToken({
  timeoutMs = COMMAND_CAPABILITY_TIMEOUT_MS,
} = {}) {
  const now = Date.now();
  if (capabilityTokenCache.token && now < capabilityTokenCache.expiresAtMs - 60_000) {
    return capabilityAcquisitionResult({ token: capabilityTokenCache.token });
  }
  if (now < capabilityTokenCache.failureUntilMs) {
    return capabilityAcquisitionResult({
      code: capabilityTokenCache.failureCode,
      transient: capabilityTokenCache.failureTransient,
    });
  }
  const injected = injectedBusinessOsCapabilityToken(now);
  if (injected?.token) {
    capabilityTokenCache = {
      ...injected,
      failureUntilMs: 0,
      failureCode: '',
      failureTransient: false,
    };
    return capabilityAcquisitionResult({ token: capabilityTokenCache.token });
  }
  if (!capabilityTokenRequestInFlight) {
    capabilityTokenRequestInFlight = requestBusinessOsCapabilityToken(timeoutMs);
  }
  const inFlight = capabilityTokenRequestInFlight;
  try {
    return await inFlight;
  } finally {
    if (capabilityTokenRequestInFlight === inFlight) {
      capabilityTokenRequestInFlight = null;
    }
  }
}

async function requestBusinessOsCapabilityToken(timeoutMs) {
  const abortController = typeof AbortController === 'function' ? new AbortController() : null;
  try {
    const res = await withTimeout(
      fetch('/api/business-os/auth/capability', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        cache: 'no-store',
        signal: abortController?.signal,
      }),
      timeoutMs,
      {
        code: 'native_unavailable',
        message: 'Business OS capability request timed out.',
        onTimeout: () => abortController?.abort(),
      },
    );
    if (!res.ok) {
      const status = Number(res.status || 0);
      const transient = !isTerminalCapabilityHttpStatus(status);
      const code = `capability_http_${status || 'unknown'}`;
      rememberCapabilityFailure(code, { transient });
      return capabilityAcquisitionResult({ code, transient });
    }
    const data = await res.json();
    if (data && data.capability_token) {
      capabilityTokenCache = {
        token: data.capability_token,
        expiresAtMs: Number(data.expires_at_ms) || Date.now() + 11 * 60 * 60 * 1000,
        failureUntilMs: 0,
        failureCode: '',
        failureTransient: false,
      };
      return capabilityAcquisitionResult({ token: capabilityTokenCache.token });
    }
    // A successful HTTP response without a token is not an authorization
    // rejection signal. Stay fail-closed, but classify the malformed control-
    // plane response as transient rather than pinning all submits for 10 s.
    rememberCapabilityFailure('capability_missing', { transient: true });
    return capabilityAcquisitionResult({ code: 'capability_missing', transient: true });
  } catch (error) {
    // fetch rejects for timeout/abort and network failures. HTTP authorization
    // rejection is handled above from its concrete 4xx status.
    const code = String(error?.code || error?.name || 'capability_unavailable');
    rememberCapabilityFailure(code, { transient: true });
    return capabilityAcquisitionResult({ code, transient: true });
  }
}

export function resetBusinessOsCapabilityTokenCacheForTests() {
  capabilityTokenCache = {
    token: null,
    expiresAtMs: 0,
    failureUntilMs: 0,
    failureCode: '',
    failureTransient: false,
  };
  capabilityTokenRequestInFlight = null;
  resetCommandRoundtripTimingForTests();
}

export function resetCommandRoundtripTimingForTests() {
  commandTimingProbes.clear();
}

export function consumeCommandRoundtripTiming(commandId) {
  const key = String(commandId || '');
  const sample = commandTimingProbes.get(key);
  if (!sample) return null;
  commandTimingProbes.delete(key);
  return cloneCommandTimingSample(sample);
}

export function peekCommandRoundtripTiming(commandId) {
  const sample = commandTimingProbes.get(String(commandId || ''));
  return sample ? cloneCommandTimingSample(sample) : null;
}

function capabilityAcquisitionResult({ token = null, code = '', transient = false } = {}) {
  return {
    token: token || null,
    code: String(code || ''),
    transient: Boolean(transient),
  };
}

function isTerminalCapabilityHttpStatus(status) {
  return status >= 400 && status < 500;
}

function rememberCapabilityFailure(code, { transient }) {
  const cacheMs = transient
    ? COMMAND_CAPABILITY_TRANSIENT_ANTI_STORM_MS
    : COMMAND_CAPABILITY_TERMINAL_NEGATIVE_CACHE_MS;
  capabilityTokenCache = {
    token: null,
    expiresAtMs: 0,
    failureUntilMs: Date.now() + cacheMs,
    failureCode: String(code || 'capability_unavailable'),
    failureTransient: Boolean(transient),
  };
}

export const getCapabilityToken = getBusinessOsCapabilityToken;

function injectedBusinessOsCapabilityToken(now = Date.now()) {
  const candidates = [
    globalThis.CTOX_BUSINESS_OS_SESSION,
    globalThis.ctoxBusinessOsSession,
    globalThis.ctoxBusinessOsLaunch?.session,
    globalThis.CTOX_DESKTOP_SESSION,
    globalThis.ctoxDesktop?.session,
  ].filter((item) => item && typeof item === 'object');
  for (const candidate of candidates) {
    const token = String(candidate.capability_token || candidate.capabilityToken || '').trim();
    if (!token) continue;
    const expiresAtMs = Number(candidate.capability_expires_at_ms || candidate.capabilityExpiresAtMs || 0)
      || now + 11 * 60 * 60 * 1000;
    if (expiresAtMs <= now + 60_000) continue;
    return { token, expiresAtMs };
  }
  return null;
}

async function acquireCapabilityTokenForSubmit() {
  let result = await acquireBusinessOsCapabilityToken();
  if (result.token || !result.transient) return result;
  await delay(COMMAND_CAPABILITY_REFRESH_RETRY_BACKOFF_MS);
  result = await acquireBusinessOsCapabilityToken();
  return result;
}

async function submitRxdbCommand({ db, sync, session, command, dispatchStartedAt = 0 }) {
  const submitStartedAt = Date.now();
  const commandId = command.id || `cmd_${crypto.randomUUID()}`;
  if (commandTimingProbeEnabled(command)) {
    rememberCommandTimingProbe(commandId, Number(dispatchStartedAt) || submitStartedAt);
    transferCommandTimingProbe('', commandId);
    recordCommandTimingMark(
      commandId,
      'browser_dispatch_started',
      Number(dispatchStartedAt) || submitStartedAt,
    );
  }
  const capability = await acquireCapabilityTokenForSubmit();
  const capabilityToken = capability.token;
  emitCommandLifecycle(commandId, command.command_type || command.type, 'capability_resolved', submitStartedAt);
  // Every Business OS command mutates native/domain state. Offline reads stay
  // local-first, but mutation intent without a current server-issued actor
  // capability would be immutable and can never become authorized later.
  // Fail before insertion instead of creating a command that is guaranteed to
  // be rejected after replication.
  if (
    CTOX_COMMAND_AUTHORIZATION.defaultRequirement === 'capability'
    && !CTOX_COMMAND_AUTHORIZATION.offlineIntentAllowed
    && !capabilityToken
  ) {
    throw commandError(commandId, 'Business OS authorization is currently unavailable.', {
      code: 'auth_required',
      transient: capability.transient,
      retryable: true,
    });
  }
  const doc = await commandDocument(
    command,
    commandId,
    resolveActorContext(command, session),
    capabilityToken,
  );
  assertCommandDocumentTransportBudget(doc, commandId);
  const currentDb = await resolveCommandDb(db);
  emitCommandLifecycle(commandId, command.command_type || command.type, 'database_resolved', submitStartedAt);
  const collection = currentDb?.raw?.business_commands;
  if (!collection) throw commandError(commandId, 'business_commands collection is required.');

  const syncPlan = await prepareCommandSync({ db: currentDb, sync, command });
  emitCommandLifecycle(commandId, command.command_type || command.type, 'sync_ready', submitStartedAt);
  try {
    try {
      await flushSyncBridges(syncPlan.beforeCommand, [], syncPlan.flushTimeoutMs);
    } catch (error) {
      if (!commandAllowsDependencyDeliveryLag(command, error)) throw error;
      // Imported file snapshots are immutable and the native command handler
      // already has a durable waiting_dependencies state. A WebRTC flush can
      // deliver every chunk yet miss its final acknowledgement (notably after
      // leader failover). Preserve the authorized command locally so the
      // normal command bridge can deliver it and native intake can verify the
      // referenced generations instead of losing the job before insertion.
      emitCommandLifecycle(
        commandId,
        command.command_type || command.type,
        'dependency_push_unconfirmed',
        submitStartedAt,
      );
    }
    const localWriteStartedAt = Date.now();
    await insertOrPatchCommandDocument(collection, commandId, doc);
    emitCommandLifecycle(commandId, command.command_type || command.type, 'local_inserted', submitStartedAt);
    recordCommandMetric(sync, 'local_submit', commandId, Date.now() - localWriteStartedAt);

    // A small set of local-first intake surfaces (currently the global
    // bug/feature reporter) must retain an already-authorized immutable intent
    // even while the native peer is recovering. The live replication bridge
    // observes this local insert and delivers it when the peer returns. This
    // opt-in never bypasses capability acquisition above and never applies to
    // commands with data dependencies, which still have to flush first.
    if (command?.allow_local_intent_without_peer === true) {
      rememberActiveCommandId(commandId);
      emitCommandLifecycle(commandId, command.command_type || command.type, 'push_unconfirmed', submitStartedAt);
      recordCommandMetric(sync, 'submit_receipt', commandId, Date.now() - submitStartedAt);
      return localCommandReceipt({ db, sync, commandId, pushConfirmed: false });
    }

    let pushConfirmed = false;
    try {
      const confirmations = await flushSyncBridges(
        syncPlan.submitBridges,
        [doc],
        syncPlan.flushTimeoutMs,
      );
      pushConfirmed = confirmations.length > 0 && confirmations.every(Boolean);
    } catch (error) {
      // The local insert is the submit atomicity boundary. A transient push
      // failure after this point is a delivery state, not evidence that the
      // command was lost; periodic replication keeps retrying the same doc.
      if (!pushConfirmationRemainsPending(error, syncPlan.submitBridges)) {
        rememberActiveCommandId(commandId);
        error.command_id ||= commandId;
        error.receipt ||= localCommandReceipt({ db, sync, commandId, pushConfirmed: false });
        throw error;
      }
    }

    if (pushConfirmed) {
      emitCommandLifecycle(commandId, command.command_type || command.type, 'push_confirmed', submitStartedAt);
    } else {
      rememberActiveCommandId(commandId);
      emitCommandLifecycle(commandId, command.command_type || command.type, 'push_unconfirmed', submitStartedAt);
    }
    recordCommandMetric(sync, 'submit_receipt', commandId, Date.now() - submitStartedAt);
    return localCommandReceipt({ db, sync, commandId, pushConfirmed });
  } finally {
    await releaseSyncPlan(syncPlan);
  }
}

function commandAllowsDependencyDeliveryLag(command, error) {
  if (command?.allow_dependency_delivery_lag !== true) return false;
  return ['sync_unavailable', 'native_unavailable', 'projection_delayed']
    .includes(cleanContextText(error?.code));
}

function localCommandReceipt({ db, sync, commandId, pushConfirmed }) {
  const tracking = commandTrackingHandle({ db, sync, commandId });
  return {
    ok: true,
    command_id: commandId,
    status: 'local',
    code: pushConfirmed ? 'push_confirmed' : 'push_unconfirmed',
    transient: !pushConfirmed,
    retryable: false,
    pushConfirmed: Boolean(pushConfirmed),
    transport: 'rxdb-command-bus',
    tracking,
    resumeTracking: tracking.resumeTracking,
  };
}

function commandTrackingHandle({ db, sync, commandId }) {
  return {
    command_id: commandId,
    waitForAccepted(options = {}) {
      return waitForCommandState({ db, sync, commandId, until: 'accepted', options });
    },
    waitForTerminal(options = {}) {
      return waitForCommandState({ db, sync, commandId, until: 'terminal', options });
    },
    resumeTracking(options = {}) {
      return waitForCommandState({
        db,
        sync,
        commandId,
        until: options.until || 'terminal',
        options,
      });
    },
    subscribe(observer) {
      return subscribeToCommand({ db, sync, commandId, observer });
    },
    async getStatus() {
      const currentDb = await resolveCommandDb(db);
      return findDoc(currentDb?.raw?.business_commands, commandId, { swallowErrors: false });
    },
  };
}

function pushConfirmationRemainsPending(error, bridges) {
  // Multi-tab follower failover has its own contract and deadline (G4); keep
  // its existing typed failure until that coordinator can acknowledge the
  // specific command id rather than only the leader flush attempt.
  if ((bridges || []).some((bridge) => syncBridgeFromHandle(bridge)?.mode === 'follower')) return false;
  const code = cleanContextText(error?.code);
  return ![
    'ctox_rxdb_schema_hash_mismatch',
    'idempotency_conflict',
    'invalid_command_contract',
    'command_payload_too_large',
    'auth_required',
  ].includes(code);
}

export function assertCommandDocumentTransportBudget(doc, commandId = '') {
  const serializedBytes = new TextEncoder().encode(JSON.stringify(doc)).byteLength;
  if (serializedBytes <= MAX_COMMAND_DOCUMENT_BYTES) return serializedBytes;
  const error = commandError(
    commandId,
    `Der Auftrag ist mit ${serializedBytes} Bytes zu gross fuer den CTOX Sync. Uebergib grosse Daten als Files oder persistierte Datensaetze und referenziere nur deren IDs.`,
    { code: 'command_payload_too_large', retryable: false },
  );
  error.size_bytes = serializedBytes;
  error.max_bytes = MAX_COMMAND_DOCUMENT_BYTES;
  throw error;
}

function emitCommandLifecycle(commandId, commandType, phase, startedAt = 0) {
  const detail = {
    command_id: String(commandId || '').slice(0, 120),
    command_type: String(commandType || '').slice(0, 120),
    phase: String(phase || '').slice(0, 80),
    elapsed_ms: startedAt ? Math.max(0, Date.now() - Number(startedAt)) : 0,
  };
  recordCommandTimingFromLifecycle(detail.command_id, detail.phase);
  globalThis.dispatchEvent?.(new CustomEvent('ctox-business-command-lifecycle', { detail }));
  console.info('[command-bus]', JSON.stringify(detail));
}

async function commandDocument(command, commandId, actor, capabilityToken = null) {
  const now = Date.now();
  const commandClientContext = command.client_context && typeof command.client_context === 'object'
    ? command.client_context
    : {};
  const moduleId = String(
    command.module
      || commandClientContext.module
      || commandClientContext.module_id
      || commandClientContext.app_id
      || commandClientContext.source_module
      || 'ctox',
  ).trim() || 'ctox';
  const canonicalCommandType = String(command.command_type || '').trim();
  const legacyCommandType = String(command.type || '').trim();
  if (canonicalCommandType && legacyCommandType && canonicalCommandType !== legacyCommandType) {
    throw commandError(commandId, 'type and command_type must identify the same command.', {
      code: 'invalid_command_contract',
      retryable: false,
    });
  }
  const commandType = canonicalCommandType || legacyCommandType || 'business_os.chat.task';
  const inboundChannel = String(command.inbound_channel || command.client_context?.inbound_channel || moduleId).trim();
  if (!commandType) throw commandError(commandId, 'command_type is required.');
  const recordId = command.record_id || '';
  const dependencies = commandDependencyManifest(command);
  const commandDeadlineAtMs = Number(
    command.deadline_at_ms
      || command.command_deadline_at_ms
      || command.payload?.command_deadline_at_ms
      || 0,
  );
  const clientContext = normalizeCommandClientContext({
    command,
    moduleId,
    commandType,
    recordId,
    inboundChannel,
    actor,
  });
  if (capabilityToken) {
    clientContext.capability_token = capabilityToken;
  }
  const doc = {
    id: commandId,
    command_id: commandId,
    contract_version: 2,
    idempotency_key: String(command.idempotency_key || commandId),
    module: moduleId,
    command_type: commandType,
    record_id: recordId,
    status: 'pending_sync',
    inbound_channel: inboundChannel,
    payload: {
      ...(command.payload || {}),
      inbound_channel: inboundChannel,
      ...(dependencies.length > 0 ? { dependencies } : {}),
      ...(Number.isFinite(commandDeadlineAtMs) && commandDeadlineAtMs > 0
        ? { command_deadline_at_ms: Math.floor(commandDeadlineAtMs) }
        : {}),
    },
    client_context: clientContext,
    created_at_ms: now,
    updated_at_ms: now,
  };
  doc.payload_hash = await payloadHashForCommandDocument(doc);
  return doc;
}

export function normalizeCommandClientContext({
  command = {},
  moduleId = '',
  commandType = '',
  recordId = '',
  inboundChannel = '',
  actor = null,
} = {}) {
  const context = command.client_context && typeof command.client_context === 'object'
    ? { ...command.client_context }
    : {};
  const payload = command.payload && typeof command.payload === 'object'
    ? command.payload
    : {};
  const payloadContext = payload.context && typeof payload.context === 'object'
    ? payload.context
    : {};
  const normalizedModule = cleanContextText(
    context.module || context.module_id || context.app_id || context.source_module || moduleId || command.module || 'ctox',
  ) || 'ctox';
  const normalizedCommandType = cleanContextText(commandType || command.type || command.command_type || 'business_os.chat.task');
  const normalizedRecordId = cleanContextText(context.record_id || recordId || command.record_id || payload.record_id || payloadContext.record_id);
  const normalizedRecordType = cleanContextText(context.record_type || payloadContext.record_type);
  const normalizedMode = cleanContextText(context.mode || payload.mode);
  const normalizedTarget = cleanContextText(context.target || payload.target);
  const normalizedAction = cleanContextText(context.action || normalizedCommandType);

  context.module = cleanContextText(context.module || normalizedModule) || normalizedModule;
  context.module_id = cleanContextText(context.module_id || normalizedModule) || normalizedModule;
  context.source_module = cleanContextText(context.source_module || normalizedModule) || normalizedModule;
  context.app_id = cleanContextText(context.app_id || normalizedModule) || normalizedModule;
  context.command_type = cleanContextText(context.command_type || normalizedCommandType) || normalizedCommandType;
  context.action = normalizedAction;
  if (normalizedMode) context.mode = normalizedMode;
  if (normalizedTarget) context.target = normalizedTarget;
  if (normalizedRecordId) context.record_id = normalizedRecordId;
  if (normalizedRecordType) context.record_type = normalizedRecordType;
  context.inbound_channel = cleanContextText(inboundChannel || context.inbound_channel || normalizedModule) || normalizedModule;
  context.dispatch_transport = 'rxdb-command-bus';
  if (actor) {
    if (!context.actor) {
      context.actor = actor;
    } else if (typeof context.actor === 'object' && !actorIdentity(context.actor)) {
      context.actor = {
        ...context.actor,
        id: actor.id,
        display_name: context.actor.display_name || actor.display_name,
        role: context.actor.role || actor.role,
        is_admin: context.actor.is_admin ?? actor.is_admin,
      };
    }
  }
  const attributedOwner = cleanContextText(context.owner_user_id)
    || (typeof context.owner === 'string' ? cleanContextText(context.owner) : '')
    || actorIdentity(context.actor)
    || actorIdentity(actor);
  if (attributedOwner && (!context.owner_user_id || context.owner_user_id === 'local-dev')) {
    context.owner_user_id = attributedOwner;
  }
  context.scope = normalizeCommandScope({
    context,
    payloadContext,
    moduleId: normalizedModule,
    commandType: normalizedCommandType,
    recordId: normalizedRecordId,
    recordType: normalizedRecordType,
    mode: normalizedMode,
    target: normalizedTarget,
    action: normalizedAction,
  });
  return context;
}

function normalizeCommandScope({
  context,
  payloadContext,
  moduleId,
  commandType,
  recordId,
  recordType,
  mode,
  target,
  action,
}) {
  const current = context.scope && typeof context.scope === 'object'
    ? { ...context.scope }
    : {};
  if (!current.app || typeof current.app !== 'object') {
    current.app = {};
  }
  current.app.module_id = cleanContextText(current.app.module_id || context.module_id || moduleId);
  current.app.app_id = cleanContextText(current.app.app_id || context.app_id || moduleId);
  if (!current.command || typeof current.command !== 'object') {
    current.command = {};
  }
  current.command.type = cleanContextText(current.command.type || commandType);
  current.command.action = cleanContextText(current.command.action || action || commandType);
  if (mode) current.command.mode = mode;
  if (target) current.command.target = target;

  if (!current.selection || typeof current.selection !== 'object') {
    current.selection = {};
  }
  current.selection.module_id = cleanContextText(current.selection.module_id || context.module_id || moduleId);
  current.selection.column = cleanContextText(current.selection.column || context.column || payloadContext.column || '');
  current.selection.record_type = cleanContextText(current.selection.record_type || recordType || payloadContext.record_type || '');
  current.selection.record_id = cleanContextText(current.selection.record_id || recordId || payloadContext.record_id || '');
  current.selection.label = cleanContextText(current.selection.label || context.label || payloadContext.label || '');

  if (context.visible_scope && typeof context.visible_scope === 'object') {
    current.visible_scope = context.visible_scope;
    current.app = {
      ...current.app,
      ...(context.visible_scope.app && typeof context.visible_scope.app === 'object' ? context.visible_scope.app : {}),
    };
    current.data = context.visible_scope.data && typeof context.visible_scope.data === 'object'
      ? context.visible_scope.data
      : current.data;
    current.external_actions = context.visible_scope.external_actions && typeof context.visible_scope.external_actions === 'object'
      ? context.visible_scope.external_actions
      : current.external_actions;
    current.selection = {
      ...current.selection,
      ...(context.visible_scope.selection && typeof context.visible_scope.selection === 'object'
        ? context.visible_scope.selection
        : {}),
    };
  }
  return current;
}

function actorIdentity(actor) {
  if (!actor) return '';
  if (typeof actor !== 'object') return String(actor).trim();
  return String(actor.id || actor.user_id || '').trim();
}

function resolveActorContext(command, session) {
  if (actorIdentity(command?.client_context?.actor)) return null;
  const currentSession = typeof session === 'function' ? session() : session;
  const user = currentSession?.user || {};
  const id = String(user.id || '').trim();
  if (!id) return null;
  return {
    id,
    display_name: user.display_name || user.name || id,
    role: user.role || (user.is_admin ? 'admin' : 'user'),
    is_admin: Boolean(user.is_admin),
  };
}

function cleanContextText(value) {
  return String(value ?? '').trim();
}

async function resolveCommandDb(db, timeoutMs = COMMAND_SYNC_READY_TIMEOUT_MS) {
  if (typeof db !== 'function') return db;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const current = db();
    if (current?.raw?.business_commands) return current;
    await delay(100);
  }
  return db();
}

async function resolveCommandSync(sync) {
  return typeof sync === 'function' ? sync() : sync;
}

async function prepareCommandSync({ db, sync, command = null }) {
  const currentSync = await resolveCommandSync(sync);
  const dependencyCollections = commandDependencySyncCollections(command);
  const queueProjectionRequired = command?.sync_queue_tasks !== false;
  const readyTimeoutMs = commandSyncReadyTimeoutMs(command);
  const leases = [];
  try {
    const dependencyBridges = await Promise.all(
      dependencyCollections.map((collection) => startScopedSyncCollection(
        currentSync,
        collection,
        `command-dependency:${command?.type || command?.command_type || 'unknown'}`,
        leases,
      )),
    );
    const commandBridge = await startScopedSyncCollection(
      currentSync,
      'business_commands',
      `command-core:${command?.type || command?.command_type || 'unknown'}`,
      leases,
    );
    const queueBridge = queueProjectionRequired
      ? await startScopedSyncCollection(
        currentSync,
        'ctox_queue_tasks',
        `command-queue:${command?.type || command?.command_type || 'unknown'}`,
        leases,
      )
      : null;
    const submitBridges = [commandBridge].filter(Boolean);
    const afterCommand = [commandBridge, queueBridge].filter(Boolean);
    const bridgesRequiredBeforeInsert = command?.allow_local_intent_without_peer === true
      ? dependencyBridges
      : [...dependencyBridges, ...submitBridges];
    await Promise.all(
      bridgesRequiredBeforeInsert.map((bridge) => (
        waitForSyncBridgeReady(bridge, readyTimeoutMs)
      )),
    );
    return {
      beforeCommand: dependencyBridges,
      submitBridges,
      afterCommand,
      leases,
      sync: currentSync,
      flushTimeoutMs: commandSyncFlushTimeoutMs(command),
    };
  } catch (error) {
    await releaseSyncLeases(leases);
    throw error;
  }
}

function commandSyncFlushTimeoutMs(command) {
  const explicit = Number(command?.sync_flush_timeout_ms || 0);
  if (!Number.isFinite(explicit) || explicit <= 0) return COMMAND_SYNC_FLUSH_TIMEOUT_MS;
  return Math.max(100, Math.min(COMMAND_SYNC_FLUSH_MAX_TIMEOUT_MS, explicit));
}

function commandSyncReadyTimeoutMs(command) {
  const explicit = Number(command?.sync_ready_timeout_ms || 0);
  if (Number.isFinite(explicit) && explicit > 0) {
    return Math.max(25, Math.min(COMMAND_SYNC_READY_TIMEOUT_MS, explicit));
  }
  const deadlineAtMs = Number(
    command?.deadline_at_ms
      || command?.command_deadline_at_ms
      || command?.payload?.command_deadline_at_ms
      || 0,
  );
  if (Number.isFinite(deadlineAtMs) && deadlineAtMs > Date.now()) {
    return Math.max(25, Math.min(COMMAND_SYNC_READY_TIMEOUT_MS, deadlineAtMs - Date.now()));
  }
  return COMMAND_SYNC_READY_TIMEOUT_MS;
}

async function startScopedSyncCollection(sync, collection, reason, leases) {
  if (typeof sync?.leaseCollection === 'function') {
    const lease = await sync.leaseCollection(collection, reason);
    leases.push(lease);
    return lease;
  }
  if (DEMAND_ONLY_SYNC_COLLECTIONS.has(collection)) {
    throw new Error(`${collection} requires sync.leaseCollection().`);
  }
  return sync?.startCollection?.(collection);
}

async function releaseSyncPlan(syncPlan) {
  await releaseSyncLeases(syncPlan?.leases || []);
}

async function releaseSyncLeases(leases) {
  await Promise.all((leases || []).map((lease) => lease?.release?.().catch(() => null)));
}

function commandDependencySyncCollections(command) {
  const collections = new Set();
  for (const dependency of commandDependencyManifest(command)) {
    const normalized = cleanContextText(dependency.collection);
    if (normalized) collections.add(normalized);
  }
  for (const collection of command?.sync_collections || []) {
    const normalized = cleanContextText(collection);
    if (normalized && normalized !== 'business_commands' && normalized !== 'ctox_queue_tasks') {
      collections.add(normalized);
    }
  }
  if (commandUsesDesktopFileMetadata(command)) {
    collections.add('desktop_files');
  }
  if (commandUsesDesktopFileChunks(command)) {
    collections.add('desktop_file_chunks');
  }
  return [...collections];
}

function commandDependencyManifest(command) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  const explicit = Array.isArray(command?.dependencies)
    ? command.dependencies
    : (Array.isArray(payload.dependencies) ? payload.dependencies : []);
  const dependencies = explicit.map((dependency) => normalizeCommandDependency(dependency)).filter(Boolean);
  const known = new Set(dependencies.map((dependency) => `${dependency.collection}:${dependency.record_id}`));
  const add = (dependency) => {
    const normalized = normalizeCommandDependency(dependency);
    if (!normalized) return;
    const key = `${normalized.collection}:${normalized.record_id}`;
    if (known.has(key)) return;
    known.add(key);
    dependencies.push(normalized);
  };
  const sourceFileId = cleanContextText(payload.source_file_id || payload.file_id);
  if (sourceFileId) {
    add({
      collection: 'desktop_files',
      record_id: sourceFileId,
      generation_id: payload.generation_id,
      content_hash: payload.content_hash || payload.sha256,
      required: true,
    });
  }
  for (const attachment of desktopFileAttachmentRefs(payload)) {
    const fileId = cleanContextText(attachment.file_id || attachment.fileId);
    if (fileId) {
      add({
        collection: 'desktop_files',
        record_id: fileId,
        generation_id: attachment.generation_id || attachment.generationId,
        content_hash: attachment.content_hash || attachment.contentHash || attachment.sha256,
        required: attachment.required !== false,
      });
    }
    const chunkId = cleanContextText(attachment.chunk_id || attachment.chunkId);
    if (chunkId) {
      add({
        collection: cleanContextText(attachment.chunk_collection || attachment.chunkCollection) || 'desktop_file_chunks',
        record_id: chunkId,
        generation_id: attachment.generation_id || attachment.generationId,
        content_hash: attachment.content_hash || attachment.contentHash || attachment.sha256,
        required: attachment.required !== false,
      });
    }
  }
  return dependencies;
}

function normalizeCommandDependency(dependency) {
  if (!dependency || typeof dependency !== 'object') return null;
  const collection = cleanContextText(dependency.collection);
  const recordId = cleanContextText(dependency.record_id || dependency.recordId || dependency.id);
  if (!collection || !recordId) return null;
  const normalized = {
    collection,
    record_id: recordId,
    required: dependency.required !== false,
  };
  const generationId = cleanContextText(dependency.generation_id || dependency.generationId);
  const contentHash = cleanContextText(dependency.content_hash || dependency.contentHash || dependency.sha256);
  if (generationId) normalized.generation_id = generationId;
  if (contentHash) normalized.content_hash = contentHash;
  return normalized;
}

function commandUsesDesktopFileMetadata(command) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  if (desktopFileAttachmentRefs(payload).some((item) => cleanContextText(item.file_id || item.fileId))) {
    return true;
  }
  return Boolean(cleanContextText(payload.source_file_id || payload.file_id));
}

function commandUsesDesktopFileChunks(command) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  const commandType = cleanContextText(command?.type || command?.command_type || '');
  const fileId = cleanContextText(payload.source_file_id || payload.file_id);
  if (fileId && (
    cleanContextText(payload.source_kind) === 'zip'
    || cleanContextText(payload.generation_id)
    || commandType.includes('install')
    || commandType.includes('parse')
  )) {
    return true;
  }
  return desktopFileAttachmentRefs(payload).some((item) => (
    cleanContextText(item.chunk_collection || item.chunkCollection) === 'desktop_file_chunks'
    || cleanContextText(item.storage_collection || item.storageCollection) === 'desktop_file_chunks'
    || cleanContextText(item.chunk_id || item.chunkId)
    || Number(item.chunk_count || item.chunkCount || 0) > 0
  ));
}

function desktopFileAttachmentRefs(payload) {
  return [
    ...(Array.isArray(payload.attachments) ? payload.attachments : []),
    ...(Array.isArray(payload.attachment_refs) ? payload.attachment_refs : []),
  ].filter((item) => (
    item
    && typeof item === 'object'
    && cleanContextText(item.kind || 'desktop_file') === 'desktop_file'
  ));
}

async function insertOrPatchCommandDocument(collection, commandId, doc) {
  try {
    await collection.insert(doc);
    return;
  } catch (error) {
    if (!isRxDbConflictError(error)) throw error;
  }
  const existingDoc = await collection.findOne(commandId).exec();
  const existing = existingDoc?.toJSON?.() || existingDoc || null;
  if (!existing) {
    await collection.insert(doc);
    return;
  }
  const existingHash = String(existing.payload_hash || await payloadHashForCommandDocument(existing));
  if (existingHash !== doc.payload_hash) {
    throw commandError(commandId, 'The command id is already bound to a different immutable payload.', {
      code: 'idempotency_conflict',
      retryable: false,
    });
  }
}

async function payloadHashForCommandDocument(document) {
  const clientContext = document?.client_context && typeof document.client_context === 'object'
    ? { ...document.client_context }
    : {};
  delete clientContext.capability_token;
  const immutable = {
    command_id: String(document?.command_id || document?.id || ''),
    idempotency_key: String(document?.idempotency_key || document?.command_id || document?.id || ''),
    module: String(document?.module || ''),
    command_type: String(document?.command_type || document?.type || ''),
    record_id: String(document?.record_id || ''),
    payload: document?.payload || {},
    client_context: clientContext,
  };
  const subtle = globalThis.crypto?.subtle;
  if (!subtle || typeof TextEncoder !== 'function') {
    throw commandError(immutable.command_id, 'SHA-256 is unavailable for command idempotency.', {
      code: 'sync_unavailable',
      retryable: false,
    });
  }
  const digest = await subtle.digest('SHA-256', new TextEncoder().encode(canonicalJson(immutable)));
  const hex = [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
  return `sha256:${hex}`;
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function commandWaitTimeoutMs(options) {
  const raw = options?.timeoutMs
    ?? options?.timeout_ms
    ?? options?.wait_timeout_ms
    ?? options?.client_context?.command_wait_timeout_ms
    ?? options?.client_context?.wait_timeout_ms;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed <= 0) return COMMAND_ACCEPT_TIMEOUT_MS;
  return Math.min(Math.max(parsed, 1000), 10 * 60 * 1000);
}

function subscribeToCommand({ db, sync, commandId, observer }) {
  let closed = false;
  let subscription = null;
  const releaseWatcher = reserveCommandWatcher(commandId);
  recordCommandMetric(sync, 'watcher_started', commandId);
  const ready = Promise.resolve()
    .then(() => resolveCommandDb(db))
    .then((currentDb) => {
      if (closed) return null;
      const collection = currentDb?.raw?.business_commands;
      if (!collection?.findOne) {
        throw commandError(commandId, 'business_commands collection is required.', {
          code: 'sync_unavailable',
          retryable: true,
        });
      }
      const stream = collection.findOne(commandId)?.$;
      if (!stream?.subscribe) {
        throw commandError(commandId, 'business_commands does not support reactive tracking.', {
          code: 'sync_unavailable',
          retryable: true,
        });
      }
      subscription = stream.subscribe((value) => {
        if (typeof observer === 'function') observer(value);
        else observer?.next?.(value);
        const command = value?.toJSON?.() || value;
        if (commandIsTerminal(command)) {
          closed = true;
          subscription?.unsubscribe?.();
          releaseWatcher();
        }
      });
      return subscription;
    })
    .catch((error) => {
      releaseWatcher();
      throw error;
    });
  return {
    ready,
    unsubscribe() {
      closed = true;
      subscription?.unsubscribe?.();
      releaseWatcher();
    },
  };
}

async function waitForCommandState({ db, sync, commandId, until, options = {} }) {
  const releaseWatcher = reserveCommandWatcher(commandId);
  const timeoutMs = commandWaitTimeoutMs(options);
  const progressCollection = 'business_commands';
  let currentDb = null;
  let syncPlan = null;
  let lastCommand = null;
  let subscription = null;
  let boundRawDb = null;
  let progressWatchHeld = false;
  const masterChangeSubscriptions = [];
  rememberActiveCommandId(commandId);
  try {
    currentDb = await resolveCommandDb(db);
    // Native acceptance is durable. Reopening the data channel must not turn
    // an already replicated receipt into a failed handoff after a reconnect.
    const localReceipt = async () => {
      let command;
      try {
        command = await findLocalDoc(currentDb?.raw?.business_commands, commandId);
      } catch {
        return null; // An unavailable local cache is not an acknowledgement.
      }
      if (command?.id !== commandId || command?._deleted
        || command?.replication_phase !== 'native_observed') return null;
      if (commandIsTerminal(command)) forgetActiveCommandId(commandId);
      if (commandIsFailed(command)) throw nativeCommandFailure(command, commandId);
      if (!commandHasReached(command, until)) return null;
      recordObservedCommandMetrics(sync, commandId, command);
      return commandReceipt(command, commandId);
    };
    const observed = await localReceipt();
    if (observed) return observed;
    try {
      syncPlan = await prepareCommandSync({ db: currentDb, sync, command: options });
    } catch (error) {
      // A native receipt can arrive while bridge readiness is waiting. Read
      // only this command id; a local intent alone never proves acceptance.
      const arrived = await localReceipt();
      if (arrived) return arrived;
      throw error;
    }
    return await new Promise((resolve, reject) => {
      let settled = false;
      let rebindInFlight = false;
      let authoritativeRebindPending = false;
      let authoritativeRevision = 0;
      let revalidationTimer = null;
      // A reactive RxDB stream may synchronously emit the current command from
      // subscribe(). Keep the timer binding initialized before bind() can call
      // settle(); otherwise terminal commands hit the temporal dead zone and
      // leave their watcher behind.
      let progressTimer = null;
      const settle = (handler, value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timeout);
        clearTimeout(revalidationTimer);
        clearInterval(progressTimer);
        subscription?.unsubscribe?.();
        masterChangeSubscriptions.splice(0).forEach((entry) => entry?.unsubscribe?.());
        if (commandIsTerminal(lastCommand)) forgetActiveCommandId(commandId);
        handler(value);
      };
      const observeProgress = (command) => {
        const token = commandProgressToken(command);
        if (!progressWatchHeld) {
          expectCommandDataPlaneProgress(sync, progressCollection, token);
          progressWatchHeld = true;
          return;
        }
        noteCommandDataPlaneProgress(sync, progressCollection, token);
      };
      const inspect = (value) => {
        if (settled || !value) return;
        lastCommand = value?.toJSON?.() || value;
        observeProgress(lastCommand);
        if (commandIsFailed(lastCommand)) {
          settle(reject, nativeCommandFailure(lastCommand, commandId));
          return;
        }
        if (!commandHasReached(lastCommand, until)) return;
        recordObservedCommandMetrics(sync, commandId, lastCommand);
        settle(resolve, commandReceipt(lastCommand, commandId));
      };
      const bind = async ({ authoritative = false } = {}) => {
        if (settled) return;
        if (rebindInFlight) {
          authoritativeRebindPending ||= authoritative;
          return;
        }
        rebindInFlight = true;
        try {
          currentDb = await resolveCommandDb(db);
          const commands = currentDb?.raw?.business_commands;
          if (!commands) throw commandError(commandId, 'business_commands collection is required.', {
            code: 'sync_unavailable', retryable: true,
          });
          if (boundRawDb !== currentDb.raw) {
            subscription?.unsubscribe?.();
            boundRawDb = currentDb.raw;
            const stream = commands.findOne(commandId)?.$;
            if (!stream?.subscribe) throw commandError(commandId, 'Reactive command tracking is unavailable.', {
              code: 'sync_unavailable', retryable: true,
            });
            subscription = stream.subscribe(inspect);
            recordCommandMetric(sync, 'watcher_started', commandId);
          }
          // Close the subscribe/read race and every data-plane rebind window.
          const requireRevision = authoritative
            ? `command-terminal:${commandId}:${++authoritativeRevision}`
            : '';
          inspect(await findDoc(commands, commandId, {
            swallowErrors: false,
            requireRevision,
          }));
        } catch (error) {
          if (isLocalFallbackCommandTrackingQueryError(error)) {
            try {
              inspect(await findLocalDoc(currentDb?.raw?.business_commands, commandId));
            } catch {}
            return;
          }
          settle(reject, error);
        } finally {
          rebindInFlight = false;
          if (!settled && authoritativeRebindPending) {
            authoritativeRebindPending = false;
            void bind({ authoritative: true });
          }
        }
      };
      const timeout = setTimeout(() => {
        recordCommandMetric(sync, 'wait_timeout', commandId, timeoutMs);
        settle(reject, commandError(
          commandId,
          'Die Rückmeldung steht noch aus. Du kannst den Vorgang weiter verfolgen.',
          {
            code: 'projection_delayed',
            status: 'projection_pending',
            transient: true,
            retryable: true,
            receipt: lastCommand ? commandReceipt(lastCommand, commandId) : { command_id: commandId },
          },
        ));
      }, timeoutMs);
      // Demand-only command projections do not receive an unsolicited full
      // collection pull. A native master-change hint immediately revalidates
      // this exact command id. Scheduled retries are only a finite safety net
      // for a lost hint; they never restart the room or full-pull history.
      const scheduleTerminalRevalidation = (index = 0) => {
        if (settled || index >= COMMAND_TERMINAL_REVALIDATE_DELAYS_MS.length) return;
        revalidationTimer = setTimeout(() => {
          if (settled) return;
          // Demand-only collections revalidate this exact id inside bind(). A
          // collection-wide pull first is both redundant and expensive: it
          // serializes the command query behind every bridge's pull cycle.
          // The broader pull remains part of the AP3 stall-repair path below.
          bind({ authoritative: true })
            .finally(() => scheduleTerminalRevalidation(index + 1));
        }, COMMAND_TERMINAL_REVALIDATE_DELAYS_MS[index]);
      };
      for (const bridge of syncPlan?.afterCommand || []) {
        let masterChangeSubscription;
        const bindMasterChanges = (currentBridge) => {
          masterChangeSubscription?.unsubscribe?.();
          masterChangeSubscription = null;
          if (settled) return;
          const state = currentBridge?.state;
          if (cleanContextText(state?.collection?.name) !== 'business_commands') return;
          masterChangeSubscription = state?.masterChange$?.subscribe?.((hint) => {
            const hinted = commandFromMasterChangeHint(hint, commandId);
            if (hinted.detailed) {
              if (hinted.command) inspect(hinted.command);
              return;
            }
            void bind({ authoritative: true });
          });
        };
        if (typeof bridge?.subscribeBridge === 'function') {
          masterChangeSubscriptions.push(bridge.subscribeBridge(bindMasterChanges));
        } else {
          bindMasterChanges(syncBridgeFromHandle(bridge));
        }
        masterChangeSubscriptions.push({ unsubscribe: () => masterChangeSubscription?.unsubscribe?.() });
      }
      scheduleTerminalRevalidation();
      progressTimer = setInterval(() => {
        if (settled) return;
        const evaluation = evaluateCommandDataPlaneProgress(sync);
        if (evaluation.ok !== false) return;
        recordCommandMetric(sync, DATA_PLANE_NO_PROGRESS_CODE, commandId, evaluation.stalledMs);
        repairCommandDataPlaneStall(sync, evaluation.collection, async () => {
          await refreshProjectionBridges(syncPlan?.afterCommand);
          await bind();
        }).catch(() => {});
      }, 250);
      bind();
    });
  } finally {
    if (progressWatchHeld) releaseCommandDataPlaneProgress(sync, progressCollection);
    subscription?.unsubscribe?.();
    releaseWatcher();
    await releaseSyncPlan(syncPlan);
  }
}

function reserveCommandWatcher(commandId) {
  if (activeCommandWatcherCount >= MAX_SIMULTANEOUS_COMMAND_WATCHERS) {
    throw commandError(commandId, 'Too many simultaneous command watchers.', {
      code: 'projection_delayed',
      transient: true,
      retryable: true,
    });
  }
  activeCommandWatcherCount += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    activeCommandWatcherCount = Math.max(0, activeCommandWatcherCount - 1);
  };
}

function recordObservedCommandMetrics(sync, commandId, command) {
  const createdAtMs = Number(command.created_at_ms || 0);
  if (command.replication_phase === 'native_observed' && createdAtMs > 0) {
    recordCommandMetric(sync, 'submit_to_native_observed', commandId, Math.max(0, Date.now() - createdAtMs));
  }
  if (commandIsTerminal(command)) {
    const terminalAtMs = Number(command.updated_at_ms || 0);
    if (terminalAtMs > 0) {
      recordCommandMetric(sync, 'terminal_to_browser_observed', commandId, Math.max(0, Date.now() - terminalAtMs));
    }
    if (commandTimingProbeEnabled(command) || commandTimingProbes.has(String(commandId || ''))) {
      mergeNativeCommandTimingMarks(commandId, command);
      recordCommandTimingMark(commandId, 'browser_terminal_observed');
      recordCommandRoundtripStageMetrics(sync, commandId);
    }
  }
}

const ACTIVE_COMMAND_STORAGE_KEY = 'ctox.businessOs.activeCommandIds.v1';
const MAX_ACTIVE_COMMAND_IDS = 128;

function readActiveCommandIds() {
  try {
    const ids = JSON.parse(globalThis.localStorage?.getItem?.(ACTIVE_COMMAND_STORAGE_KEY) || '[]');
    return Array.isArray(ids) ? ids.map(String).filter(Boolean).slice(-MAX_ACTIVE_COMMAND_IDS) : [];
  } catch {
    return [];
  }
}

function rememberActiveCommandId(commandId) {
  const ids = readActiveCommandIds().filter((id) => id !== commandId);
  ids.push(commandId);
  try { globalThis.localStorage?.setItem?.(ACTIVE_COMMAND_STORAGE_KEY, JSON.stringify(ids.slice(-MAX_ACTIVE_COMMAND_IDS))); } catch {}
}

function forgetActiveCommandId(commandId) {
  const ids = readActiveCommandIds().filter((id) => id !== commandId);
  try { globalThis.localStorage?.setItem?.(ACTIVE_COMMAND_STORAGE_KEY, JSON.stringify(ids)); } catch {}
}

function recordCommandMetric(sync, name, commandId, durationMs) {
  sync?.recordCommandMetric?.({ name, commandId, durationMs });
}

function commandHasReached(command, until) {
  if (until === 'terminal') return commandIsTerminal(command);
  return commandIsTerminal(command)
    || command.replication_phase === 'native_observed'
    || ['accepted', 'waiting_dependencies', 'queued', 'leased', 'running', 'awaiting_review', 'validating', 'retry_wait', 'blocked']
      .includes(String(command.execution_phase || command.status || ''));
}

function commandIsTerminal(command) {
  return command?.execution_phase === 'terminal'
    || ['completed', 'failed', 'cancelled'].includes(String(command?.terminal_status || command?.status || ''));
}

function nativeCommandFailure(command, commandId) {
  const outcome = command.result?.outcome || command.payload?.outcome || null;
  return commandError(
    commandId,
    command.error_message || command.error || outcome?.stderr || outcome?.error || 'Die Aufgabe konnte nicht ausgeführt werden.',
    {
      code: command.error_code || 'command_terminal_failure',
      retryable: Boolean(command.retryable),
    },
  );
}

function commandIsFailed(command) {
  if (!command) return false;
  if (command.terminal_status === 'failed' || command.status === 'failed') return true;
  const outcome = command.result?.outcome || command.payload?.outcome || null;
  return outcome?.ok === false || Number(outcome?.exit_code || 0) !== 0;
}

function commandReceipt(command, commandId) {
  const executionTaskId = String(command?.execution_task_id || '').trim();
  const compatibilityTaskId = String(command?.task_id || '').trim();
  const taskId = executionTaskId || (
    command?.execution_mode === 'control' || commandIsTerminal(command)
      ? ''
      : compatibilityTaskId
  );
  const status = commandReceiptStatus(command);
  const legacyTaskStatus = String(command?.task_status || '').trim();
  return {
    ok: !commandIsFailed(command),
    command_id: commandId,
    status,
    execution_mode: command?.execution_mode || null,
    execution_task_id: taskId,
    task_id: taskId,
    target_task_id: command?.target_task_id || (
      taskId ? '' : compatibilityTaskId
    ),
    target_record_id: command?.target_record_id || command?.record_id || '',
    task_status: legacyTaskStatus && legacyTaskStatus !== 'pending_sync' ? legacyTaskStatus : status,
    payload: command?.payload || null,
    result: command?.result || null,
    transport: 'rxdb-command-bus',
  };
}

function commandReceiptStatus(command) {
  const executionPhase = String(command?.execution_phase || '').trim();
  if (executionPhase === 'terminal') {
    return String(command?.terminal_status || command?.status || 'terminal');
  }
  if (executionPhase) return executionPhase;
  const legacyStatus = String(command?.status || '').trim();
  if (command?.replication_phase === 'native_observed' && legacyStatus === 'pending_sync') {
    return 'accepted';
  }
  return legacyStatus || 'accepted';
}

async function findDoc(collection, id, { swallowErrors = true, requireRevision = '' } = {}) {
  if (!collection?.findOne || !id) return null;
  let doc;
  try {
    doc = await collection.findOne(requireRevision
      ? { selector: { id }, requireRevision }
      : id).exec();
  } catch (error) {
    if (swallowErrors) return null;
    throw error;
  }
  return doc?.toJSON?.() || doc || null;
}

async function findLocalDoc(collection, id) {
  const storageCollection = collection?.storageCollection;
  if (!storageCollection?.findDocumentsById || !id) return null;
  const documents = await storageCollection.findDocumentsById([id]);
  const doc = documents?.[id] || documents?.[String(id)] || null;
  return doc?.toJSON?.() || doc;
}

function isLocalFallbackCommandTrackingQueryError(error) {
  const codes = [error?.code, error?.cause?.code, error?.data?.code]
    .map((code) => String(code || ''));
  const message = String(error?.message || error || '');
  return [
    'SQLITE_QUERY_STREAM_UNSUPPORTED',
    'QUERY_FETCH_STREAM_UNSUPPORTED',
    'QUERY_NOT_SUPPORTED',
    'QUERY_QUEUE_LIMIT',
    'STREAM_LIMIT_EXCEEDED',
  ]
    .some((code) => codes.includes(code) || message.includes(code));
}

function commandFromMasterChangeHint(hint, commandId) {
  const payload = hint?.result ?? hint;
  const documents = Array.isArray(payload?.documents) ? payload.documents : null;
  if (!documents) return { detailed: false, command: null };
  const expectedId = String(commandId || '');
  const command = documents.find((document) => (
    String(document?.id || document?.command_id || '') === expectedId
  )) || null;
  return { detailed: true, command };
}

async function waitForSyncBridgeReady(bridge, timeoutMs) {
  const resolvedBridge = syncBridgeFromHandle(bridge);
  const state = resolvedBridge?.state;
  const collection = cleanContextText(
    bridge?.collection || resolvedBridge?.collection || state?.collection?.name,
  ) || 'unknown';
  if (typeof bridge?.subscribeBridge === 'function') {
    await waitForConnectedSyncPeer(state, collection, timeoutMs, bridge);
    return;
  }
  if (!state) {
    if (resolvedBridge?.mode === 'pending' || resolvedBridge?.mode === 'paused') {
      throw commandError('', `CTOX Sync Engine collection "${collection}" is ${resolvedBridge.mode}.`, {
        code: 'sync_unavailable',
        retryable: true,
      });
    }
    return;
  }
  // Command submission needs an authenticated, open native peer so the new
  // local row can be pushed; it must not wait for the complete historical
  // pull of business_commands. A mature instance can contain many thousands
  // of immutable command records, making that cold pull much longer than the
  // command deadline even though the transport is already usable.
  if (syncBridgeHasPeerStatus(state)) {
    await waitForConnectedSyncPeer(state, collection, timeoutMs);
    return;
  }
  await withTimeout(
    () => state.awaitInSync?.() || state.awaitInitialReplication?.(),
    timeoutMs,
    {
      code: 'native_unavailable',
      message: `CTOX Sync Engine collection "${collection}" did not become ready before the command deadline.`,
    },
  );
}

function syncBridgeHasPeerStatus(state) {
  return typeof state?.getTransportStatus === 'function'
    || Boolean(state?.demandStatus)
    || Boolean(state?.peerStates$)
    || Boolean(state?.active$)
    || Boolean(state?.transportStatus$);
}

function waitForConnectedSyncPeer(state, collection, timeoutMs, lease = null) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let lastStatus = syncBridgeStatus(state);
    let observedState;
    let bridgeSubscription;
    const subscriptions = [];
    const clearStateSubscriptions = () => {
      for (const subscription of subscriptions.splice(0)) subscription?.unsubscribe?.();
    };
    const finish = (handler, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearInterval(pollTimer);
      clearStateSubscriptions();
      bridgeSubscription?.unsubscribe?.();
      handler(value);
    };
    const inspect = () => {
      if (settled) return;
      const bridge = lease ? syncBridgeFromHandle(lease) : { state };
      state = bridge?.state;
      if (observedState !== state) {
        observedState = state;
        clearStateSubscriptions();
        for (const observable of [state?.peerStates$, state?.active$, state?.transportStatus$, state?.canceled$]) {
          if (settled) break;
          const subscription = observable?.subscribe?.(inspect);
          if (!subscription) continue;
          if (settled) subscription.unsubscribe?.();
          else subscriptions.push(subscription);
        }
      }
      if (settled) return;
      lastStatus = syncBridgeStatus(state);
      if (['released', 'stopped', 'failed'].includes(bridge?.mode)) {
        finish(reject, commandError('', `CTOX Sync Engine collection "${collection}" is ${bridge.mode}.`, {
          code: 'sync_unavailable', retryable: bridge.mode === 'failed',
        }));
        return;
      }
      if (bridge?.mode === 'follower') { finish(resolve); return; }
      if (!state) return;
      if (state.cancelled || state.canceled$?.getValue?.() === true) {
        // Runtime-owned replacement may follow cancellation. Keep the same
        // deadline and await the lease's next generation; never restart here.
        if (lease) return;
        finish(reject, commandError('', `CTOX Sync Engine collection "${collection}" was cancelled.`, {
          code: 'sync_unavailable', retryable: true,
        }));
        return;
      }
      if (syncBridgePeerConnected(state, lastStatus)) finish(resolve);
    };
    const timer = setTimeout(() => {
      const summary = syncBridgeStatusSummary(lastStatus);
      finish(reject, commandError(
        '',
        `CTOX Sync Engine collection "${collection}" has no authenticated WebRTC peer after ${timeoutMs} ms (${summary}).`,
        { code: 'native_unavailable', retryable: true },
      ));
    }, timeoutMs);
    const pollTimer = setInterval(inspect, 50);
    if (lease) {
      bridgeSubscription = lease.subscribeBridge(inspect);
      if (settled) bridgeSubscription?.unsubscribe?.();
    }
    inspect();
  });
}

function syncBridgeStatus(state) {
  try {
    return state?.getTransportStatus?.() || state?.transportStatus$?.getValue?.() || {};
  } catch {
    return {};
  }
}

function syncBridgePeerConnected(state, status = syncBridgeStatus(state)) {
  if (state?.demandStatus?.peerConnected === true) return true;
  if (status?.demandLoading?.peerConnected === true || status?.peerConnected === true) return true;
  const peerStates = state?.peerStates$?.getValue?.();
  if (peerStates instanceof Map && peerStates.size > 0) return true;
  if (Array.isArray(peerStates) && peerStates.length > 0) return true;
  if (state?.active$?.getValue?.() === true && cleanContextText(state?.activeRemotePeerId)) return true;
  return Array.isArray(status?.connectionStates)
    && status.connectionStates.some((connection) => {
      const channelState = connection?.channelState || connection?.channelReadyState || '';
      const peerState = connection?.peerConnectionState || '';
      return connection?.open === true
        || (channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(peerState));
    });
}

function syncBridgeStatusSummary(status) {
  const activePeerCount = Number(status?.activePeerCount || 0);
  const connectionCount = Number(status?.connectionCount || 0);
  const demandPeer = status?.demandLoading?.peerConnected === true ? 'connected' : 'not-connected';
  return `active peers: ${activePeerCount}, connections: ${connectionCount}, collection peer: ${demandPeer}`;
}

async function flushSyncBridges(
  bridges,
  documents = [],
  timeoutMs = COMMAND_SYNC_FLUSH_TIMEOUT_MS,
) {
  return Promise.all(
    (bridges || []).map((bridge) => flushSyncBridge(bridge, documents, timeoutMs)),
  );
}

async function flushSyncBridge(bridge, documents = [], timeoutMs = COMMAND_SYNC_FLUSH_TIMEOUT_MS) {
  const resolvedBridge = syncBridgeFromHandle(bridge);
  if (resolvedBridge?.mode === 'follower') {
    if (typeof resolvedBridge.flush !== 'function') {
      throw commandError('', 'Multi-tab sync follower cannot confirm the leader push.', {
        code: 'sync_unavailable',
        retryable: true,
      });
    }
    const acknowledgement = await withTimeout(
      () => resolvedBridge.flush(documents),
      followerSyncFlushTimeoutMs(resolvedBridge),
      {
        code: 'sync_unavailable',
        message: 'CTOX Sync Engine could not complete multi-tab command failover before the deadline.',
      },
    );
    return acknowledgement?.ok === true;
  }
  const state = resolvedBridge?.state;
  if (!state) return false;
  let pushesCurrentDocuments = false;
  const acknowledgement = await withTimeout(
    () => {
      if (documents.length && typeof state.pushDocumentsToRemotePeers === 'function') {
        pushesCurrentDocuments = true;
        return state.pushDocumentsToRemotePeers(documents);
      }
      if (typeof state.pushToRemotePeers === 'function') {
        pushesCurrentDocuments = true;
        return state.pushToRemotePeers();
      }
      // awaitInSync is only a collection-wide readiness signal. It does not
      // confirm that this command document reached the master.
      return state.awaitInSync?.();
    },
    timeoutMs,
    {
      code: 'sync_unavailable',
      message: 'CTOX Sync Engine could not push command dependencies before the deadline.',
    },
  );
  return pushesCurrentDocuments && acknowledgement === true;
}

function followerSyncFlushTimeoutMs(bridge) {
  const requested = Number(bridge?.flushTimeoutMs);
  if (!Number.isFinite(requested) || requested <= 0) {
    return COMMAND_FOLLOWER_SYNC_FLUSH_TIMEOUT_MS;
  }
  return Math.max(100, Math.min(COMMAND_FOLLOWER_SYNC_FLUSH_TIMEOUT_MS, requested));
}

async function refreshProjectionBridges(bridges) {
  await Promise.all((bridges || []).map((bridge) => refreshProjectionBridge(bridge)));
}

async function refreshProjectionBridge(bridge) {
  const state = syncBridgeFromHandle(bridge)?.state;
  if (!state) return;
  await withTimeout(
    () => {
      if (typeof state.pullFromRemotePeers === 'function') return state.pullFromRemotePeers();
      return state.awaitInSync?.();
    },
    COMMAND_SYNC_FLUSH_TIMEOUT_MS,
    {
      code: 'projection_delayed',
      message: 'CTOX command projection refresh exceeded its deadline.',
    },
  );
}

function syncBridgeFromHandle(handle) {
  return handle?.bridge || handle;
}

function commandError(commandId, message, options = {}) {
  const error = new Error(message);
  error.command_id = commandId;
  error.code = options.code || 'command_terminal_failure';
  error.status = options.status || 'failed';
  error.transient = Boolean(options.transient);
  error.retryable = Boolean(options.retryable);
  if (options.receipt) error.receipt = options.receipt;
  return error;
}

function isRxDbConflictError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: CONFLICT')
    || message.includes('conflict')
    || message.includes('document already exists')
    || message.includes('Document update conflict');
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function commandTimingProbeEnabled(source) {
  const context = source?.client_context && typeof source.client_context === 'object'
    ? source.client_context
    : source;
  return context?.command_timing_probe === true;
}

function rememberCommandTimingProbe(commandId, startedAtMs) {
  const key = String(commandId || '');
  const existing = commandTimingProbes.get(key);
  if (existing) return existing;
  if (commandTimingProbes.size >= MAX_COMMAND_TIMING_PROBES) {
    const oldest = commandTimingProbes.keys().next().value;
    if (oldest !== undefined) commandTimingProbes.delete(oldest);
  }
  const sample = {
    command_id: key,
    started_at_ms: Number(startedAtMs) || Date.now(),
    marks: {},
  };
  commandTimingProbes.set(key, sample);
  return sample;
}

function transferCommandTimingProbe(fromId, toId) {
  const fromKey = String(fromId || '');
  const toKey = String(toId || '');
  if (!toKey || fromKey === toKey) return;
  const sample = commandTimingProbes.get(fromKey);
  if (!sample) return;
  commandTimingProbes.delete(fromKey);
  sample.command_id = toKey;
  commandTimingProbes.set(toKey, sample);
}

function recordCommandTimingMark(commandId, markName, atMs = Date.now()) {
  if (!COMMAND_ROUNDTRIP_MARK_NAMES.includes(markName)) return;
  const sample = commandTimingProbes.get(String(commandId || ''));
  if (!sample) return;
  const stamp = Number(atMs);
  if (!Number.isFinite(stamp)) return;
  if (!Number.isFinite(sample.marks[markName])) {
    sample.marks[markName] = stamp;
  }
}

function recordCommandTimingFromLifecycle(commandId, phase) {
  const markName = COMMAND_LIFECYCLE_TIMING_MARKS[String(phase || '')];
  if (!markName) return;
  recordCommandTimingMark(commandId, markName);
}

function mergeNativeCommandTimingMarks(commandId, command) {
  const timing = command?.result?.command_timing;
  if (!timing || typeof timing !== 'object') return;
  for (const markName of [
    'native_dispatch_entered',
    'native_handler_completed',
    'native_rxdb_projection_committed',
  ]) {
    const stamp = Number(timing[markName]);
    if (Number.isFinite(stamp) && stamp > 0) {
      recordCommandTimingMark(commandId, markName, stamp);
    }
  }
}

function recordCommandRoundtripStageMetrics(sync, commandId) {
  const sample = commandTimingProbes.get(String(commandId || ''));
  if (!sample) return;
  const stages = commandRoundtripStagesFromMarks(sample.marks);
  if (!stages) return;
  for (const [name, durationMs] of Object.entries(stages)) {
    recordCommandMetric(sync, `roundtrip_${name}`, commandId, durationMs);
  }
}

export function commandRoundtripStagesFromMarks(marks = {}) {
  if (!COMMAND_ROUNDTRIP_MARK_NAMES.every((name) => Number.isFinite(Number(marks[name])))) {
    return null;
  }
  return {
    browser_insert: marks.browser_local_inserted - marks.browser_dispatch_started,
    push: marks.browser_push_confirmed - marks.browser_local_inserted,
    push_to_native_intake: marks.native_dispatch_entered - marks.browser_push_confirmed,
    native_processing: marks.native_handler_completed - marks.native_dispatch_entered,
    projection_commit: marks.native_rxdb_projection_committed - marks.native_handler_completed,
    commit_to_browser_observed: marks.browser_terminal_observed - marks.native_rxdb_projection_committed,
    total: marks.browser_terminal_observed - marks.browser_dispatch_started,
  };
}

function cloneCommandTimingSample(sample) {
  return {
    command_id: String(sample.command_id || ''),
    started_at_ms: Number(sample.started_at_ms) || 0,
    marks: { ...sample.marks },
  };
}
