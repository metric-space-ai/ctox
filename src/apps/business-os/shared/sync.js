// =============================================================================
// AGENT GUARDRAILS — ctox-rxdb data plane (read docs/ctox-rxdb.md first)
// =============================================================================
// This file orchestrates CTOX Sync Engine, the WebRTC-ONLY data plane between Business OS
// and the CTOX daemon. Hard rules (each one has caused real regressions):
//   1. NO HTTP fallback/bridge for collection data — ever. WebRTC only.
//   2. NO npm/bare/node: imports — this runtime is package-manager-free.
//   3. After ANY src edit: rebuild dist with the pinned esbuild command and
//      bump the ?v= cache-buster (see docs/ctox-rxdb.md "Build & release").
//      Never patch dist/ctox-rxdb-js.mjs directly.
//   4. Wire-contract constants are GENERATED from fixtures — never hand-edit
//      *-contract.generated.mjs or the Rust twins.
//   5. Run `node src/apps/business-os/rxdb/tests/run-all.mjs` and keep it
//      green. Never delete or weaken a failing test to make it pass.
// =============================================================================

// Per-collection sync runtime on top of CTOX Sync Engine. Repair philosophy: the
// shared native peer self-heals its transport; this layer only classifies
// errors and schedules bounded restarts.
import {
  batchSizeFor,
  collectionTopic,
  nativeRxdbPeerReady,
  normalizeCollectionReadinessState,
} from './sync-contract.js?v=20260910-shell-v2-authoritative-pins-v373';
import { getBusinessOsCapabilityToken } from './command-bus.js?v=20260910-shell-v2-authoritative-pins-v373';
import { loadRxdbRuntime, RXDB_BUNDLE_URL } from './rxdb-runtime.js?v=20260910-shell-v2-authoritative-pins-v373';
import { CTOX_COMMAND_LIFECYCLE_CAPABILITY } from './command-lifecycle.generated.js';

const CTOX_RXDB_PROTOCOL = 'ctox-rxdb-protocol-v1';
// Multi-tab leadership may span a rolling Business OS release: an already
// open tab keeps its Web Lock and continues to heartbeat while a freshly
// loaded tab runs the new sync protocol. Sharing one coordinator room across
// those builds made the new tab follow the old, failed bridge forever. The
// release epoch isolates only the local BroadcastChannel/Web Lock; both builds
// still replicate through the same server-authoritative WebRTC room.
const MULTI_TAB_COORDINATOR_EPOCH = '20260910-shell-v2-authoritative-pins-v373';
const CTOX_BROWSER_CAPABILITIES = [
  'ctox-control-plane-v1',
  'ctox-role-bound-signaling-v1',
  'ctox-rxdb-browser-v1',
  'ctox-file-chunks-v1',
  'ctox-schema-hash-v1',
  'ctox-peer-session-v1',
  'ctox-device-proof-v1',
  'ctox-checkpoint-epoch-v1',
  'ctox-checkpoint-generation-v2',
  CTOX_COMMAND_LIFECYCLE_CAPABILITY,
];
const NATIVE_PEER_OPEN_WATCHDOG_MS = 30000;
// Native peer bring-up can legitimately include store recovery, projection
// refresh, and signaling-room rejoin. Keep this above the observed cold-start
// tail so restartCollections does not turn a healthy slow reconnect into a
// user-visible command failure.
const NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS = 60000;
const NATIVE_PEER_RESTART_STABLE_MS = 1000;
const COMMAND_FOLLOWER_DIRECT_OPEN_TIMEOUT_MS = 25_000;
const COMMAND_FOLLOWER_DIRECT_FLUSH_TIMEOUT_MS = 35_000;
const COMMAND_FOLLOWER_BRIDGE_TIMEOUT_MS = 40_000;
const SYNC_DIAGNOSTIC_EMIT_MIN_INTERVAL_MS = 250;
const DEMAND_ONLY_COLLECTION_START_ERROR = 'DEMAND_ONLY_COLLECTION_REQUIRES_LEASE';
const ROOM_CIRCUIT_FAILURE_THRESHOLD = 5;
const ROOM_CIRCUIT_OPEN_MS = 120_000;
const ROOM_RETRY_BASE_MS = 1_000;
const ROOM_RETRY_MAX_MS = 30_000;

// Workjet native-host bridge boundary for device proof. The native app owns
// the P-256 private key and injects this one callback; browser code receives
// only the public JWK and a raw IEEE-P1363 signature for the supplied nonce.
export async function getBusinessOsDeviceProof(nonce) {
  const provider = globalThis.ctoxWorkjetDeviceProofProvider;
  if (typeof provider !== 'function') return null;
  return provider(nonce);
}
// Pacing between collection starts. This exists so a multi-collection bootstrap
// stays under the send-queue budget that recycles a wedged peer — see
// enqueueSendFrame in rxdb/src/webrtc-native.mjs, which drops the connection at
// MAX_PEER_SEND_QUEUE_FRAMES = 1024 / MAX_PEER_SEND_QUEUE_BYTES = 16 MB.
//
// Measured: the boot starts 15 collections. At 500 ms the browser needed 15,5 s
// until all 15 were connected — 7,5 s of that was this gap doing nothing but
// waiting, on a queue sized for 1024 frames. The budget is not the binding
// constraint at this collection count; the gap was.
//
// 60 ms keeps the starts ordered and still spreads them over the event loop, so
// a slow peer is not handed fifteen simultaneous negotiations. If the boot set
// ever grows far beyond this, measure the queue depth before raising this again
// — the guard rail is the queue budget, not this constant.
const COLLECTION_START_GAP_MS = 60;
// How many collection registrations may mutate the shared room handshake at
// the same time. Four lanes looked faster in an isolated bootstrap benchmark,
// but a restored workspace starts several module leases concurrently. On the
// managed customer tenant instance that invalidated the multiplexed handshake while
// other lanes were still running: masterChangesSince timed out, the shared
// peer dropped, and even a one-row command insert waited 16.5 seconds behind
// recovery work. Serialize registration on the one shared transport. This is
// deliberately one lane, not one WebRTC peer per collection.
const COLLECTION_START_LANES = 1;
// These high-frequency Browser collections are a compatibility transport.
// The Browser module establishes them explicitly only if the authenticated
// browser_sessions direct-live capability is unavailable. Keeping them in the
// manifest is necessary for schema registration and scoped data access, but a
// module-window lease must not start them eagerly.
const MODULE_EXPLICIT_START_COLLECTIONS = new Set([
  'browser_frames',
  'browser_input_events',
]);
// Erster Durchgang kurz nach dem Start (die meisten Rennen sind bis dahin
// entschieden), danach traeger, damit ein dauerhaft fehlendes Schema nicht
// zum Dauerlauf wird.
const UNREGISTERED_SWEEP_DELAY_MS = 3000;
const UNREGISTERED_SWEEP_RETRY_MS = 15000;
const STALLED_RECONNECT_MIN_AGE_MS = 30000;
const COLLECTION_START_QUEUE_STEP_TIMEOUT_MS = 3_000;
const COLLECTION_RESTART_GAP_MS = 500;
// Feldmessung 04.09.2026 (Kundenmandant): sieben Kollektionen standen dauerhaft auf
// initialReplicationState 'pending' und durchliefen wiederholt 'restarting'.
// Ohne Herkunftsvermerk am Datensatz laesst sich nicht belegen, WELCHER Pfad
// den raumweiten Neustart ausloest. Der Zaehler ist reine Diagnose.
let restartCounter = 0;
const DESKTOP_ICON_SAFE_FIELDS = new Set([
  'id',
  'target_type',
  'target_module',
  'target_record_id',
  'label',
  'glyph',
  'x',
  'y',
  'pinned',
  'hidden',
  'sort_index',
  'updated_at_ms',
  '_deleted',
  '_rev',
  '_meta',
  '_attachments',
]);
const desktopIconRepairPromises = new WeakMap();
const desktopIconReplicationCollections = new WeakMap();
const RETRYABLE_CONTROL_PLANE_CODES = new Set([
  'control_plane_token_expired',
  'temporary_unavailable',
]);

const signalingErrorHandlers = new Set();
let signalingErrorObserverInstalled = false;

import { CollectionSyncRegistry } from './sync-collection-registry.js?v=1';

export function createSyncRuntime({
  db,
  config,
  onDiagnostic,
  capabilityTokenProvider = getBusinessOsCapabilityToken,
  onNativeQueryReady = null,
}) {
  const bridges = new CollectionSyncRegistry();
  const activeCollections = new Set();
  // Direct shell/service consumers pin a bridge until they explicitly stop
  // it. App windows use reference-counted leases instead, so closing the last
  // window can return the sync runtime to its pre-launch resource baseline.
  const pinnedCollections = new Set();
  const suspendedCollections = new Set();
  let globalRestartTimer = null;
  let unregisteredSweepTimer = null;
  let repairCycleInProgress = false;
  // Collection starts used to hang off ONE promise chain: every collection
  // waited for the previous one to finish its bridge before it even began. At a
  // measured 600–1000 ms per bridge that put the whole boot on a serial track —
  // 15 collections, ~15 s until all were connected.
  //
  // Now COLLECTION_START_LANES chains run side by side and each collection takes
  // the next lane round-robin. Start ORDER is preserved (lane n gets the n-th
  // collection), so priority collections still go first; only the waiting is
  // parallel. The guard rail stays the send-queue budget in
  // rxdb/src/webrtc-native.mjs (1024 frames / 16 MB) — four concurrent bridges
  // are nowhere near it.
  let collectionStartLanes = createCollectionStartLanes();
  let collectionStartLaneCursor = 0;
  let multiTabCoordinator = null;
  const multiTabUnsubscribers = [];
  let suspensionReason = '';
  let stopped = false;
  let nativeReadSequence = 0;
  const publishResourceBudget = () => {
    const root = globalThis.document?.documentElement;
    if (!root?.dataset) return;
    root.dataset.syncActiveCollectionCount = String(activeCollections.size);
    root.dataset.syncBridgeCount = String(bridges.size);
    root.dataset.syncPinnedCollectionCount = String(pinnedCollections.size);
    root.dataset.syncLeaseCount = String(
      bridges.leaseCounts().reduce((sum, [, count]) => sum + count, 0),
    );
  };
  publishResourceBudget();
  const useWebrtc = nativeRxdbPeerReady(config, db);
  if (!useWebrtc) {
    throw new Error('Business OS requires RxDB WebRTC sync; unsupported sync contract.');
  }
  const runtimeMode = 'webrtc';
  const diagnostics = createDiagnostics(config, runtimeMode);
  diagnostics.browserStorage = sanitizeBrowserStorageStatus(db?.storageHealth || null);
  const browserStorageListener = (event) => {
    if (event?.detail?.databaseName && event.detail.databaseName !== db?.name) return;
    diagnostics.browserStorage = sanitizeBrowserStorageStatus({
      ...(db?.storageHealth || {}),
      ...(event?.detail || {}),
      journalPendingWrites: event?.detail?.pendingWrites
        ?? event?.detail?.journalPendingWrites
        ?? db?.storageHealth?.journalPendingWrites,
      journalPendingBytes: event?.detail?.pendingBytes
        ?? event?.detail?.journalPendingBytes
        ?? db?.storageHealth?.journalPendingBytes,
      lastRecoveryExportAtMs: event?.detail?.lastExportAtMs
        ?? event?.detail?.lastRecoveryExportAtMs
        ?? db?.storageHealth?.lastRecoveryExportAtMs,
    });
    scheduleDiagnosticEmit?.();
  };
  globalThis.addEventListener?.('ctox-indexeddb-recovery-status', browserStorageListener);
  globalThis.addEventListener?.('ctox-indexeddb-storage-pressure', browserStorageListener);
  multiTabUnsubscribers.push(() => {
    globalThis.removeEventListener?.('ctox-indexeddb-recovery-status', browserStorageListener);
    globalThis.removeEventListener?.('ctox-indexeddb-storage-pressure', browserStorageListener);
  });
  const roomCircuit = diagnostics.roomCircuit;
  let diagnosticEmitTimer = null;
  const commandMetricSeen = new Set();
  let lastDiagnosticEmitAtMs = 0;
  const flushDiagnostic = () => {
    if (!onDiagnostic) return;
    lastDiagnosticEmitAtMs = Date.now();
    onDiagnostic(snapshotDiagnostics(diagnostics));
  };
  const scheduleDiagnosticEmit = ({ immediate = false } = {}) => {
    if (!onDiagnostic) return;
    if (immediate) {
      if (diagnosticEmitTimer) {
        clearTimeout(diagnosticEmitTimer);
        diagnosticEmitTimer = null;
      }
      flushDiagnostic();
      return;
    }
    if (diagnosticEmitTimer) return;
    diagnosticEmitTimer = setTimeout(() => {
      diagnosticEmitTimer = null;
      flushDiagnostic();
    }, SYNC_DIAGNOSTIC_EMIT_MIN_INTERVAL_MS);
  };
  const emitDiagnostic = (updates = {}, options = {}) => {
    if (updates.lastError !== undefined) diagnostics.lastError = updates.lastError;
    if (updates.lastLifecycleEvent !== undefined) diagnostics.lastLifecycleEvent = updates.lastLifecycleEvent;
    if (updates.phase) diagnostics.phase = updates.phase;
    if (updates.moduleId) diagnostics.moduleId = updates.moduleId;
    diagnostics.updatedAt = new Date().toISOString();
    scheduleDiagnosticEmit({
      immediate: options.immediate === true || isUrgentDiagnosticUpdate(updates),
    });
  };
  const flushFollowerDirectly = async (collection) => {
    await withRejectingTimeout(async () => {
      const current = await Promise.resolve(bridges.get(collection)).catch(() => null);
      if (current?.mode === 'follower') bridges.delete(collection);
      const direct = await syncRuntime.startCollection(collection, { pin: false, forceDirect: true });
      const state = direct?.state;
      if (!state) throw new Error(`Direct sync bridge for ${collection} is unavailable.`);
      await waitForNativePeerOpenState(state, collection, COMMAND_FOLLOWER_DIRECT_OPEN_TIMEOUT_MS);
      if (typeof state.pushToRemotePeers === 'function') await state.pushToRemotePeers();
      else await state.scheduleLocalWritePush?.();
    }, COMMAND_FOLLOWER_DIRECT_FLUSH_TIMEOUT_MS, (
      `Direct multi-tab failover for ${collection} did not reach the native WebRTC peer before the deadline.`
    ));
  };
  const recordCollection = (collection, update) => {
    const current = diagnostics.collections[collection] || {};
    const updatedAt = new Date().toISOString();
    const declaredSyncProfile = declaredCollectionSyncProfile(collection);
    const demandOnly = isDemandOnlyPullCollection(collection);
    const next = {
      ...current,
      collection,
      updatedAt,
      syncProfile: declaredSyncProfile || (demandOnly ? 'demand-only' : 'eager'),
      localCoverage: demandOnly ? 'windowed' : 'full',
      ...update,
    };
    const nextStatus = update.connectionStatus || update.status || current.connectionStatus || current.status || '';
    if (
      update.lastError === undefined
      && isHealthyCollectionStatus(nextStatus)
      && isTransientSignalingSocketError(current.lastError)
    ) {
      next.lastError = null;
    }
    if (update.queryReady === undefined) {
      next.queryReady = demandOnly
        ? isHealthyCollectionStatus(nextStatus) && !next.lastError
        : true;
    }
    const nextPeerSession = peerSessionKey(update.remotePeerSession);
    if (nextPeerSession) {
      const previousPeerSession = peerSessionKey(current.remotePeerSession);
      const changed = Boolean(previousPeerSession && previousPeerSession !== nextPeerSession);
      const currentGeneration = Number.isFinite(Number(current.peerGeneration))
        ? Number(current.peerGeneration)
        : 0;
      next.peerGeneration = changed ? currentGeneration + 1 : Math.max(1, currentGeneration || 1);
      next.previousPeerSession = changed ? previousPeerSession : current.previousPeerSession || null;
      next.peerGenerationChangedAt = changed || !current.peerGeneration
        ? updatedAt
        : current.peerGenerationChangedAt || updatedAt;
    }
    diagnostics.collections[collection] = {
      ...next,
    };
    emitDiagnostic({ phase: 'collection-sync' }, {
      immediate: isUrgentCollectionDiagnostic(update, nextStatus),
    });
  };
  const stopAllBridges = async () => {
    const bridgePromises = [...bridges.values()];
    bridges.revokeAllLeases();
    bridges.clear();
    activeCollections.clear();
    pinnedCollections.clear();
    publishResourceBudget();
    const states = await Promise.allSettled(bridgePromises);
    for (const state of states) {
      if (state.status === 'fulfilled') {
        try { await withTimeout(state.value?.stop?.(), 3000); } catch {}
      }
    }
  };
  const collectionNeedsRestart = (collection) => {
    const current = diagnostics.collections[collection] || {};
    const status = current.connectionStatus || current.status || '';
    // A collection that lost the startup race sits on 'pending' with
    // reason='collection-not-registered' — it was read from db.raw before it was
    // entered there. That is recoverable and must be retried, but 'pending' was
    // not in the restart set, so the repair cycle filtered it straight back out:
    // the trigger fired and did nothing. Measured on a customer instance, where
    // user_thread_states stayed pending for 75 s while the collection was
    // demonstrably present in db.raw and all 25 others were healthy.
    //
    // Deliberately NOT every 'pending': the 'startup-in-progress' stub means a
    // bridge is still being built and resolves on its own. Restarting that one
    // would cancel work in flight.
    if (status === 'pending' && current.reason === 'collection-not-registered') return true;
    if (!['reconnecting', 'failed', 'error', 'stopped'].includes(status)) return false;
    // Schema/auth/protocol rejection is terminal until configuration or
    // credentials change. Replaying it in a timer only hammers signaling.
    return current.lastError?.retryable !== false;
  };
  // Unconditional sweep for collections that lost the startup race.
  //
  // They sit on status 'pending' with reason 'collection-not-registered' and
  // emit NO event: db.raw simply did not have them yet, nothing threw. Every
  // event-driven path is therefore blind to them, and the repair-cycle trigger
  // additionally drops any request while another timer is pending (see
  // `if (globalRestartTimer || repairCycleInProgress) return` below). Measured
  // on a customer instance: user_thread_states stayed pending for 75 s with a
  // matching predicate, a closed circuit and the collection present in db.raw —
  // while a manual restartCollection() fixed it in under 6 s. The repair was
  // healthy the whole time; nobody knocked.
  //
  // This sweep owes nothing to any event and it KEEPS RUNNING while the runtime
  // lives. The first version stopped when a pass found nothing — and that is
  // exactly the bug it was written to fix, repeated one level up: during boot
  // the first collection arms the timer, all later calls are dropped because a
  // timer already exists, the pass fires three seconds later, finds nothing
  // because the losing collection is not on 'pending' yet, and nobody re-arms.
  // Measured on a customer instance: updatedAt of the stuck collection was
  // FROZEN across 138 s — the pass never touched it once.
  //
  // A pass is one filter over a short list, so a steady heartbeat is cheap; the
  // previous "stop when idle" was the expensive choice, because it cost the
  // whole repair.
  const scheduleUnregisteredCollectionSweep = (delayMs = UNREGISTERED_SWEEP_DELAY_MS) => {
    if (stopped || unregisteredSweepTimer) return;
    unregisteredSweepTimer = setTimeout(async () => {
      unregisteredSweepTimer = null;
      if (stopped) return;
      const stuck = [...activeCollections].filter((collection) => {
        const current = diagnostics.collections[collection] || {};
        // Key off the DEFECT MARKER, not the connection status. A collection
        // that lost the startup race reports two fields that disagree: status
        // 'pending' but connectionStatus 'connecting'. Reading
        // `connectionStatus || status` — as this filter did — sees 'connecting',
        // which looks healthy, and skips the very collection it exists for.
        // Measured on a customer instance: reason stayed
        // 'collection-not-registered' and nothing moved for over 50 s while the
        // sweep passed it by every 15 s.
        //
        // `reason` is the defect itself and does not depend on which phase the
        // bridge claims to be in. The health check below keeps a collection that
        // genuinely came up from being restarted for a stale reason string.
        if (current.reason !== 'collection-not-registered') return false;
        const healthy = isHealthyCollectionStatus(current.connectionStatus)
          || isHealthyCollectionStatus(current.status);
        return !healthy;
      });
      const repairCandidates = repairCandidateCollectionNames(
        activeCollections,
        diagnostics.collections,
      );
      const stalledReconnects = repairCandidates.filter((collection) => (
        isStalledReconnectingCollection(
          diagnostics.collections[collection],
          Date.now(),
          STALLED_RECONNECT_MIN_AGE_MS,
        )
      ));
      for (const collection of stuck) {
        if (stopped) return;
        try {
          await syncRuntime.restartCollection(collection);
        } catch (error) {
          recordCollection(collection, { lastError: serializeError(error) });
        }
      }
      // Event-driven recovery is still the fast path. This heartbeat is the
      // bounded safety net for a bridge whose peer disappeared while its
      // one-shot restart timer was already consumed. Measured after a native
      // peer restart: ctox_queue_tasks remained on the old peer session for
      // more than a minute while every other collection had reconnected; the
      // existing manual restart repaired it immediately. Route the stale
      // collection back through the shared room repair cycle so circuit
      // breaking, retry limits and healthy-collection isolation still apply.
      if (stalledReconnects.length) {
        scheduleRestartOfUnhealthyCollections(stalledReconnects[0], 0);
      }
      scheduleUnregisteredCollectionSweep(UNREGISTERED_SWEEP_RETRY_MS);
    }, delayMs);
    unregisteredSweepTimer.unref?.();
  };

  const scheduleRestartOfUnhealthyCollections = (triggerCollection, delayMs = 5000) => {
    if (stopped) return;
    if (roomCircuit.permanent) return;
    if (globalRestartTimer || repairCycleInProgress) return;
    const now = Date.now();
    if (roomCircuit.state === 'open' && roomCircuit.openUntilMs > now) {
      delayMs = Math.max(delayMs, roomCircuit.openUntilMs - now);
    }
    globalRestartTimer = setTimeout(async () => {
      if (stopped) return;
      globalRestartTimer = null;
      if (roomCircuit.permanent || repairCycleInProgress) return;
      if (roomCircuit.state === 'open') {
        const remaining = Number(roomCircuit.openUntilMs || 0) - Date.now();
        if (remaining > 0) {
          scheduleRestartOfUnhealthyCollections(triggerCollection, remaining);
          return;
        }
        roomCircuit.state = 'half_open';
        roomCircuit.nextProbeAtMs = 0;
        roomCircuit.updatedAtMs = Date.now();
      }
      const collections = repairCandidateCollectionNames(
        activeCollections,
        diagnostics.collections,
      ).filter(collectionNeedsRestart);
      let nextDelay = 0;
      repairCycleInProgress = true;
      try {
        if (collections.length) {
          const stableBridges = await syncRuntime.restartCollections(collections);
          applyRoomRepairCycleOutcome(roomCircuit, { stableCount: stableBridges.length });
        }
      } catch (restartError) {
        const restartSerialized = serializeError(restartError);
        // The circuit receives one verdict for the whole repair cycle. Per-
        // collection watchdogs only mark their own collection unhealthy and
        // cannot independently advance or reopen the room circuit.
        nextDelay = applyRoomRepairCycleOutcome(roomCircuit, { errors: [restartSerialized] }) || 0;
        emitDiagnostic({ phase: 'failed', lastError: restartSerialized });
      } finally {
        repairCycleInProgress = false;
        if (
          !stopped
          && repairCandidateCollectionNames(activeCollections, diagnostics.collections)
            .some(collectionNeedsRestart)
        ) {
          scheduleRestartOfUnhealthyCollections(triggerCollection, nextDelay || ROOM_RETRY_BASE_MS);
        }
      }
    }, delayMs);
  };
  const scheduleGlobalRestart = (triggerCollection, error) => {
    if (stopped) return;
    const serialized = serializeError(error);
    const lifecycleEvent = isLifecycleEvent(error) ? serialized : null;
    const retryable = serialized?.retryable !== false;
    const reconnectingSince = new Date().toISOString();
    recordCollection(triggerCollection, {
      status: retryable ? 'reconnecting' : 'error',
      connectionStatus: retryable ? 'reconnecting' : 'error',
      lastError: !lifecycleEvent ? serialized : diagnostics.collections[triggerCollection]?.lastError || null,
      lastLifecycleEvent: lifecycleEvent || diagnostics.collections[triggerCollection]?.lastLifecycleEvent || null,
      reconnectingSince: retryable ? reconnectingSince : null,
    });
    emitDiagnostic({
      phase: retryable ? 'reconnecting' : 'failed',
      lastError: lifecycleEvent ? null : serialized,
      lastLifecycleEvent: lifecycleEvent,
    });
    if (retryable) scheduleRestartOfUnhealthyCollections(triggerCollection, ROOM_RETRY_BASE_MS);
  };
  const onlineListener = () => scheduleRestartOfUnhealthyCollections(null, 250);
  if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
    window.addEventListener('online', onlineListener);
  }
  emitDiagnostic({ phase: 'ready' });
  const ensureMultiTabCoordinator = async () => {
    if (multiTabCoordinator) return multiTabCoordinator;
    const rxdb = db?.rxdb || await loadRxdbRuntime();
    if (typeof rxdb?.getMultiTabSyncCoordinator !== 'function') return null;
    multiTabCoordinator = rxdb.getMultiTabSyncCoordinator({
      databaseName: db?.name || db?.raw?.name || 'ctox_business_os_js_v1',
      room: multiTabCoordinatorRoom(config.sync_room),
    });
    // Serve follower tabs that ask the leader to run a native request for
    // them. Without this the Browser app is dead in every tab but one.
    if (typeof multiTabCoordinator.onNativeRequest === 'function') {
      multiTabUnsubscribers.push(multiTabCoordinator.onNativeRequest(
        (method, params, options) => requestNativeDirectly(method, params, options),
      ));
    }
    multiTabUnsubscribers.push(multiTabCoordinator.onRoleChange?.((status) => {
      diagnostics.multiTab = sanitizeMultiTabStatus(status);
      emitDiagnostic({ phase: diagnostics.phase || 'ready' });
      if (status.isLeader) {
        queueMicrotask(async () => {
          for (const collection of [...activeCollections]) {
            const current = await Promise.resolve(bridges.get(collection)).catch(() => null);
            if (current?.mode !== 'follower') continue;
            bridges.delete(collection);
            await syncRuntime.startCollection(collection).catch((error) => {
              recordCollection(collection, { status: 'error', connectionStatus: 'error', lastError: serializeError(error) });
            });
          }
        });
      } else {
        queueMicrotask(async () => {
          for (const [collection, bridgePromise] of [...bridges.entries()]) {
            const bridge = await Promise.resolve(bridgePromise).catch(() => null);
            if (!bridge || bridge.mode === 'follower') continue;
            try { await withTimeout(bridge.stop?.(), 3000); } catch {}
            bridges.set(collection, Promise.resolve(createFollowerBridge(
              collection,
              status,
              multiTabCoordinator,
              () => flushFollowerDirectly(collection),
            )));
            recordCollection(collection, {
              status: 'follower',
              connectionStatus: 'follower',
              multiTab: sanitizeMultiTabStatus(status),
              lastError: null,
            });
          }
        });
      }
    }) || (() => {}));
    multiTabUnsubscribers.push(multiTabCoordinator.onDirty?.(({ collection, ids }) => (
      Promise.resolve(bridges.get(normalizeCollectionName(collection)))
        .then((bridge) => flushLeaderDirtyCollection(bridge, collection, ids))
    )) || (() => {}));
    const storageListener = (event) => {
      const detail = event?.detail || {};
      if (detail.databaseName !== (db?.name || db?.raw?.name)) return;
      if (detail.replicationOriginRole) {
        if (multiTabCoordinator.isLeader()) multiTabCoordinator.notifyReplicatedChange(detail.collection, detail.ids);
      } else if (!multiTabCoordinator.isLeader()) {
        multiTabCoordinator.notifyDirty(detail.collection, detail.ids);
      }
    };
    globalThis.addEventListener?.('ctox-rxdb-storage-change', storageListener);
    multiTabUnsubscribers.push(() => globalThis.removeEventListener?.('ctox-rxdb-storage-change', storageListener));
    diagnostics.multiTab = sanitizeMultiTabStatus(await multiTabCoordinator.start());
    return multiTabCoordinator;
  };
  const syncRuntime = {
    db,
    config,
    mode: runtimeMode,
    diagnostics,
    recordCommandMetric(metric = {}) {
      const name = String(metric.name || '').trim();
      if (!name) return;
      const commandId = String(metric.commandId || '').trim();
      const dedupeKey = commandId ? `${name}:${commandId}` : '';
      if (dedupeKey && commandMetricSeen.has(dedupeKey)) return;
      if (dedupeKey) commandMetricSeen.add(dedupeKey);
      recordCommandPlaneMetric(diagnostics.commandPlane, name, metric.durationMs);
      emitDiagnostic({ phase: diagnostics.phase || 'ready' });
    },
    async requestNative(method, params = {}, options = {}) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      const coordinator = multiTabCoordinator;
      // Only the leader holds the WebRTC data channel. A follower that opens
      // its own direct bridge never connects, so ask the leader first and keep
      // the direct path as the fallback for when no leader answers.
      // Callers budget these calls (the Browser app renews its controller
      // lease on a 5s budget). Splitting that budget across the proxy hop and
      // the direct fallback keeps a slow leader from blowing the caller's
      // deadline, which is what silently starved the lease renewal.
      const budgetMs = Number(options?.timeoutMs) > 0 ? Number(options.timeoutMs) : 0;
      if (coordinator && !coordinator.isLeader?.() && typeof coordinator.requestNativeViaLeader === 'function') {
        try {
          // The proxy hop is an optimisation, not the contract: it must not eat
          // the caller's budget. Splitting the budget between hop and fallback
          // left the actual call ~1s of a 5s allowance, and the Browser app's
          // lease reacquisition failed with "exceeded 5000ms" every single time
          // — the surface then reports "Steuerung abgelaufen und konnte nicht
          // zurückgeholt werden" while the channel itself is perfectly healthy.
          // Bound the hop on its own short deadline and leave the caller's
          // budget to the direct call below.
          return await coordinator.requestNativeViaLeader(method, params, options, {
            timeoutMs: budgetMs ? Math.min(1500, Math.max(500, Math.round(budgetMs * 0.3))) : 1500,
          });
        } catch (error) {
          if (stopped) throw error;
          // The failed proxy call already dropped the unresponsive lease and
          // called for an election. Give that election a brief moment to land
          // so the direct bridge below is built as a leader rather than as a
          // follower that can never connect — brief, because the caller's
          // deadline is still running.
          await waitForLeadership(coordinator, 500);
        }
      }
      return requestNativeDirectly(method, params, options);
    },
    async startModule(moduleManifest) {
      const collections = moduleManifest?.collections || [];
      const results = [];
      emitDiagnostic({ phase: 'module-sync', moduleId: moduleManifest?.id || null });
      for (const collection of collections) {
        if (!moduleSyncCollections([collection]).length) {
          results.push({
            status: 'fulfilled',
            value: {
              mode: 'skipped',
              collection,
              reason: 'demand-only-module-collection',
              stop: async () => {},
            },
          });
          continue;
        }
        try {
          results.push({ status: 'fulfilled', value: await this.startCollection(collection) });
        } catch (reason) {
          results.push({ status: 'rejected', reason });
        }
        await delay(100);
      }
      return results;
    },
    async leaseModule(moduleManifest, reason = 'module-window') {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      const collections = moduleSyncCollections(moduleManifest?.collections || []);
      const leases = [];
      try {
        for (const collection of collections) {
          leases.push(await this.leaseCollection(
            collection,
            `${reason}:${moduleManifest?.id || 'unknown'}`,
          ));
          await delay(25);
        }
      } catch (error) {
        await Promise.allSettled(leases.map((lease) => lease.release()));
        throw error;
      }
      let released = false;
      return {
        mode: 'module-lease',
        moduleId: moduleManifest?.id || null,
        collections: leases.map((lease) => lease.collection),
        async release() {
          if (released) return false;
          released = true;
          await Promise.allSettled(leases.map((lease) => lease.release()));
          return true;
        },
      };
    },
    async leaseCollection(collection, reason = 'scoped-collection-lease') {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      const normalized = normalizeCollectionName(collection);
      if (!normalized) throw new Error('collection is required.');
      const lease = bridges.acquire(normalized, reason, async (remaining) => {
        publishResourceBudget();
        if (remaining <= 0 && !pinnedCollections.has(normalized)) {
          await syncRuntime.stopCollection(normalized, { preservePin: true }).catch(() => null);
          if (isModuleDemandOnlyCollection(normalized)) {
            recordCollection(normalized, {
              status: 'skipped',
              connectionStatus: 'demand-only',
              reason: 'demand-only-lease-released',
              active: false,
              frameTransport: null,
              lastError: null,
              reconnectingSince: null,
            });
          }
        }
      });
      publishResourceBudget();
      try {
        await this.startCollection(normalized, { pin: false });
        return lease;
      } catch (error) {
        await lease.release();
        throw error;
      }
    },
    async startCollection(collection, options = {}) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      collection = normalizeCollectionName(collection);
      if (!collection) throw new Error('collection is required.');
      const coordinator = await ensureMultiTabCoordinator();
      if (isModuleDemandOnlyCollection(collection) && !bridges.leaseCount(collection)) {
        const error = new Error(`${collection} is demand-only and must be started through leaseCollection().`);
        error.code = DEMAND_ONLY_COLLECTION_START_ERROR;
        recordCollection(collection, {
          status: 'skipped',
          connectionStatus: 'demand-only',
          reason: 'demand-only-requires-lease',
          // This is an expected API-contract rejection, not a transport or
          // replication failure. Keeping it in lastError poisoned Advanced
          // Status for the rest of the browser session even after the caller
          // recovered or used the correct scoped lease.
          lastError: null,
          reconnectingSince: null,
        });
        throw error;
      }
      if (options.pin !== false) pinnedCollections.add(collection);
      activeCollections.add(collection);
      scheduleUnregisteredCollectionSweep();
      publishResourceBudget();
      if (suspendedCollections.has(collection)) {
        recordCollection(collection, {
          status: 'paused',
          connectionStatus: 'paused',
          reason: suspensionReason || 'sync-suspended',
          reconnectingSince: null,
          lastError: null,
          lastLifecycleEvent: null,
        });
        return {
          mode: 'pending',
          collection,
          reason: suspensionReason || 'sync-suspended',
          state: null,
          stop: async () => {},
        };
      }
      if (bridges.has(collection)) {
        const current = diagnostics.collections[collection] || {};
        const currentBridgePromise = bridges.get(collection);
        // startCollection is an idempotent acquisition API. A transient
        // diagnostic such as active$=false must never make a caller cancel the
        // shared bridge: command tracking can call this while another command
        // is awaiting acknowledgement, and cancelling here aborts that live
        // command. The background repair loop owns reconnect/restart policy.
        // Only a replication state that is actually cancelled is replaced.
        const currentBridge = await withTimeout(currentBridgePromise, 3000);
        if (!currentBridge) {
          // Keep repeated acquisitions bounded while the authoritative bridge
          // is still opening. The ready promise remains available to callers,
          // but the app/window launch itself does not hang on WebRTC startup.
          const pendingBridge = createPendingCollectionBridge(collection, currentBridgePromise);
          recordCollection(collection, {
            status: 'pending',
            connectionStatus: 'connecting',
            reason: pendingBridge.reason,
            lastError: null,
            reconnectingSince: null,
            connectedAt: null,
          });
          return pendingBridge;
        }
        if (
          shouldReplaceCachedBridgeForStart(currentBridge, options)
          || currentBridge?.state?.cancelled === true
        ) {
          // Followers cannot serve requestNative(), pending stubs need a real
          // collection after schema registration, and actually-cancelled
          // states must be rebuilt. Transient diagnostics alone never enter
          // this branch.
          bridges.delete(collection);
        } else {
          recordCollection(collection, {
            status: 'reused',
            connectionStatus: current.connectionStatus || current.status || 'connecting',
          });
          return currentBridge;
        }
      }
      // An ordinary acquisition must reuse a direct bridge already serving
      // native requests in a follower tab. Overwriting it with a follower
      // stub loses its owner; the next forceDirect call replaces and cancels
      // the live collection registration. Role changes own actual demotion.
      if (coordinator && !coordinator.isLeader() && options.forceDirect !== true) {
        const follower = createFollowerBridge(
          collection,
          coordinator.snapshot(),
          coordinator,
          () => flushFollowerDirectly(collection),
        );
        bridges.set(collection, Promise.resolve(follower));
        publishResourceBudget();
        recordCollection(collection, {
          status: 'follower',
          connectionStatus: 'follower',
          multiTab: coordinator.snapshot(),
          lastError: null,
          reconnectingSince: null,
        });
        return follower;
      }
      recordCollection(collection, { status: 'starting' });
      const startLane = collectionStartLaneCursor % collectionStartLanes.length;
      collectionStartLaneCursor += 1;
      const startBridge = () => {
        if (stopped) throw new Error('Business OS sync runtime has been stopped');
        return startWebRtcReplication({
          db,
          config,
          collection,
          recordCollection,
          capabilityTokenProvider,
          onNativeQueryReady,
          onFatalPeerError: (error) => scheduleGlobalRestart(collection, error),
          // Passed down explicitly: startWebRtcReplication is a module-level
          // function, so it cannot see this closure. The previous direct call
          // threw a ReferenceError that the metric-subscription wrapper
          // swallowed — the primary "peer dropped → schedule repair" trigger
          // never ran.
          scheduleRestart: scheduleRestartOfUnhealthyCollections,
        });
      };
      const bridgePromise = collectionStartLanes[startLane].then(startBridge);
      // Every collection shares one bounded room send queue. Pacing initial
      // catch-up keeps a legitimate multi-collection bootstrap below the
      // wedged-peer recycle threshold while preserving deterministic order.
      collectionStartLanes[startLane] = boundedCollectionStartQueueStep(bridgePromise);
      bridges.set(collection, bridgePromise);
      publishResourceBudget();
      try {
        const bridge = await withTimeout(bridgePromise, 3000);
        if (!bridge) {
          const pendingBridge = createPendingCollectionBridge(collection, bridgePromise);
          recordCollection(collection, {
            status: 'pending',
            connectionStatus: 'connecting',
            reason: pendingBridge.reason,
            lastError: null,
            reconnectingSince: null,
            connectedAt: null,
          });
          return pendingBridge;
        }
        recordCollection(collection, {
          status: bridge.mode === 'pending' ? 'pending' : 'running',
          connectionStatus: bridge.mode === 'pending' ? 'pending' : 'connecting',
          topic: bridge.topic || null,
          reason: bridge.reason || null,
          lastError: null,
          reconnectingSince: null,
          connectedAt: null,
        });
        return bridge;
      } catch (error) {
        if (bridges.get(collection) === bridgePromise) bridges.delete(collection);
        publishResourceBudget();
        const serialized = serializeError(error);
        recordCollection(collection, { status: 'failed', lastError: serialized });
        emitDiagnostic({ phase: 'failed', lastError: serialized });
        throw error;
      }
    },
    async stopCollection(collection, options = {}) {
      collection = normalizeCollectionName(collection);
      activeCollections.delete(collection);
      if (!options?.preserveLeases) bridges.revokeLeases(collection);
      if (!options?.preservePin) pinnedCollections.delete(collection);
      const bridgePromise = bridges.get(collection);
      bridges.delete(collection);
      publishResourceBudget();
      if (!bridgePromise) return false;
      recordCollection(collection, {
        status: 'restarting',
        connectionStatus: 'reconnecting',
        lastError: null,
        reconnectingSince: new Date().toISOString(),
      });
      try {
        const bridge = await withTimeout(bridgePromise, 3000);
        await withTimeout(bridge?.stop?.(), 3000);
      } catch {
        // The old bridge is already unusable. Dropping it from the cache is enough.
      }
      return true;
    },
    async restartCollection(collection) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      collection = normalizeCollectionName(collection);
      if (!collection) throw new Error('collection is required.');
      const wasPinned = pinnedCollections.has(collection);
      activeCollections.add(collection);
      await this.stopCollection(collection, { preserveLeases: true, preservePin: true });
      return this.startCollection(collection, { pin: wasPinned });
    },
    async restartCollections(collections) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      if (globalRestartTimer) clearTimeout(globalRestartTimer);
      globalRestartTimer = null;
      const requested = [...new Set((collections || [])
        .filter((collection) => typeof collection === 'string')
        .map(normalizeCollectionName)
        .filter(Boolean))];
      const restartable = requested.filter((collection) => (
        !isModuleDemandOnlyCollection(collection) || bridges.leaseCount(collection) > 0
      ));
      for (const collection of requested) {
        if (restartable.includes(collection)) continue;
        activeCollections.delete(collection);
        recordCollection(collection, {
          status: 'skipped',
          connectionStatus: 'demand-only',
          reason: 'demand-only-requires-lease',
          lastError: null,
          reconnectingSince: null,
        });
      }
      const pinnedBeforeRestart = new Map(
        restartable.map((collection) => [collection, pinnedCollections.has(collection)]),
      );
      for (const collection of requested) suspendedCollections.delete(collection);
      if (!suspendedCollections.size) suspensionReason = '';
      for (const collection of restartable) activeCollections.add(collection);
      await Promise.all(restartable.map((collection) => this.stopCollection(collection, {
        preserveLeases: true,
        preservePin: true,
      })));
      const startBatch = async (batchCollections) => {
        collectionStartLanes = createCollectionStartLanes();
        collectionStartLaneCursor = 0;
        const starts = [];
        for (const collection of batchCollections) {
          starts.push(this.startCollection(collection, {
            pin: pinnedBeforeRestart.get(collection) === true,
          }).then(
            (bridge) => ({ collection, bridge }),
            (error) => ({ collection, error }),
          ));
          await delay(COLLECTION_RESTART_GAP_MS);
        }
        return Promise.all(starts);
      };
      const restarted = await repairRestartBatch(await startBatch(restartable), {
        waitForStable: async ({ collection, bridge, error }) => {
          if (error) throw error;
          let readyBridge = bridge;
          if (!readyBridge?.state && readyBridge?.ready) {
            readyBridge = await withRejectingTimeout(
              () => readyBridge.ready,
              NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS,
              `Native peer bridge did not start for ${collection} within ${NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS}ms.`,
            );
          }
          await waitForStableNativePeerOpenState(
            readyBridge?.state,
            collection,
            NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS,
            NATIVE_PEER_RESTART_STABLE_MS,
          );
          return readyBridge;
        },
        stopFailed: async ({ collection, error }) => {
          const lifecycleEvent = serializeError(error);
          recordCollection(collection, {
            status: 'reconnecting',
            connectionStatus: 'reconnecting',
            lastError: null,
            lastLifecycleEvent: lifecycleEvent,
            reconnectingSince: new Date().toISOString(),
          });
          await this.stopCollection(collection, { preserveLeases: true, preservePin: true });
        },
        restartFailed: (failed) => startBatch(failed.map(({ collection }) => collection)),
      });
      for (const { collection, bridge } of restarted.stable) {
        recordCollection(collection, {
          status: 'connected',
          connectionStatus: 'connected',
          reconnectingSince: null,
          lastError: null,
          lastLifecycleEvent: null,
        });
      }
      for (const { collection, error } of restarted.failed) {
        recordCollection(collection, {
          status: 'reconnecting',
          connectionStatus: 'reconnecting',
          lastError: null,
          lastLifecycleEvent: serializeError(error),
          reconnectingSince: new Date().toISOString(),
        });
      }
      if (!restarted.stable.length && restarted.failed.length) {
        const retryError = new AggregateError(
          restarted.failed.map(({ error }) => error),
          `Native peer did not open for any restarted collection after individual retry: ${restarted.failed.map(({ error }) => formatLifecycleError(error)).join('; ')}`,
        );
        retryError.code = 'peer_connect_timeout';
        retryError.retryable = true;
        throw retryError;
      }
      return restarted.stable.map(({ bridge }) => bridge);
    },
    async suspendCollections(collections, reason = 'sync-suspended') {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      if (globalRestartTimer) clearTimeout(globalRestartTimer);
      globalRestartTimer = null;
      const requested = [...new Set((collections || [])
        .filter((collection) => typeof collection === 'string')
        .map(normalizeCollectionName)
        .filter(Boolean))];
      suspensionReason = reason || 'sync-suspended';
      for (const collection of requested) {
        activeCollections.add(collection);
        suspendedCollections.add(collection);
      }
      for (const collection of requested) {
        await this.stopCollection(collection, { preserveLeases: true, preservePin: true });
        recordCollection(collection, {
          status: 'paused',
          connectionStatus: 'paused',
          reason: suspensionReason,
          reconnectingSince: null,
          lastError: null,
          lastLifecycleEvent: null,
        });
      }
      return requested;
    },
    async resumeCollections(collections) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      const requested = [...new Set((collections || [])
        .filter((collection) => typeof collection === 'string')
        .map(normalizeCollectionName)
        .filter(Boolean))];
      for (const collection of requested) suspendedCollections.delete(collection);
      if (!suspendedCollections.size) suspensionReason = '';
      return this.restartCollections(requested);
    },
    async readCollectionNativeDocument(collection, documentId, options = {}) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      const normalized = normalizeCollectionName(collection);
      if (!normalized) throw new Error('collection is required.');
      const id = String(documentId || '').trim();
      if (!id) throw new Error('documentId is required.');
      const budgetMs = Math.max(250, Number(options.timeoutMs) || 15_000);
      const startedAt = Date.now();
      const remainingMs = () => Math.max(1, budgetMs - (Date.now() - startedAt));
      const controller = typeof AbortController === 'function' ? new AbortController() : null;
      nativeReadSequence = (nativeReadSequence + 1) % Number.MAX_SAFE_INTEGER;
      const lease = await withRejectingTimeout(
        () => this.leaseCollection(normalized, 'authoritative-native-read'),
        remainingMs(),
        `Native read lease for ${normalized} exceeded ${budgetMs}ms.`,
      );
      let timer = null;
      try {
        let bridge = lease.bridge;
        if (!bridge?.state && bridge?.ready) {
          bridge = await withRejectingTimeout(
            () => bridge.ready,
            remainingMs(),
            `Native read bridge for ${normalized} exceeded ${budgetMs}ms.`,
          );
        }
        const state = bridge?.state;
        if (!state?.awaitQueryReady) {
          throw new Error(`Native query readiness is unavailable for ${normalized}.`);
        }
        await state.awaitQueryReady(remainingMs());
        const generation = state.collectionQueryGenerationToken?.(state.activeRemotePeerId);
        if (!generation) throw new Error(`Native query generation is unavailable for ${normalized}.`);
        const primaryPath = state.collection?.schema?.primaryPath || 'id';
        const requireRevision = `authority:${normalized}:${id}:${nativeReadSequence}`;
        const query = {
          selector: { [primaryPath]: id },
          requireRevision,
        };
        if (controller) query.signal = controller.signal;
        const read = state.collection.findOne(query).exec();
        timer = setTimeout(() => controller?.abort?.(), remainingMs());
        const document = await read;
        const currentBridge = lease.bridge;
        if (currentBridge?.state !== state || state.cancelled) {
          throw new Error(`Native read generation for ${normalized} was replaced.`);
        }
        const currentGeneration = state.collectionQueryGenerationToken?.(state.activeRemotePeerId);
        if (!currentGeneration || currentGeneration !== generation) {
          throw new Error(`Native read generation for ${normalized} changed.`);
        }
        return document;
      } finally {
        if (timer) clearTimeout(timer);
        controller?.abort?.();
        await lease.release().catch(() => {});
      }
    },

    async stop() {
      stopped = true;
      if (globalRestartTimer) clearTimeout(globalRestartTimer);
      globalRestartTimer = null;
      if (unregisteredSweepTimer) clearTimeout(unregisteredSweepTimer);
      unregisteredSweepTimer = null;
      if (diagnosticEmitTimer) {
        clearTimeout(diagnosticEmitTimer);
        diagnosticEmitTimer = null;
      }
      if (typeof window !== 'undefined' && typeof window.removeEventListener === 'function') {
        window.removeEventListener('online', onlineListener);
      }
      for (const unsubscribe of multiTabUnsubscribers.splice(0)) {
        try { unsubscribe?.(); } catch {}
      }
      await multiTabCoordinator?.close?.();
      await stopAllBridges();
      emitDiagnostic({ phase: 'stopped' }, { immediate: true });
    },
    resourceSnapshot() {
      return {
        activeCollections: [...activeCollections].sort(),
        bridgeCollections: [...bridges.keys()].sort(),
        pinnedCollections: [...pinnedCollections].sort(),
        leaseCounts: Object.fromEntries(bridges.leaseCounts().sort()),
      };
    },
  };
  async function waitForLeadership(coordinator, timeoutMs) {
    const deadline = Date.now() + Math.max(0, Number(timeoutMs) || 0);
    while (Date.now() < deadline) {
      if (stopped || coordinator.isLeader?.()) return coordinator.isLeader?.() === true;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    return coordinator.isLeader?.() === true;
  }

  async function requestNativeDirectly(method, params = {}, options = {}) {
    if (stopped) throw new Error('Business OS sync runtime has been stopped');
    const collection = normalizeCollectionName(options.collection || 'business_commands');
    if (!collection) throw new Error('A collection is required for native WebRTC requests.');
    let bridge = await syncRuntime.startCollection(collection, { pin: false, forceDirect: true });
    if (!bridge?.state && bridge?.ready) bridge = await bridge.ready;
    if (typeof bridge?.state?.requestNative !== 'function') {
      throw new Error(`Native WebRTC requests are unavailable for ${collection}.`);
    }
    const budgetMs = Number(options?.timeoutMs) > 0 ? Number(options.timeoutMs) : 0;
    const call = bridge.state.requestNative(method, params, options);
    if (!budgetMs) return call;
    // A peer that never finishes negotiating leaves the transport promise
    // pending forever. The caller's budget has to win, or its retry loop
    // never runs again.
    let timer = null;
    try {
      return await Promise.race([
        call,
        new Promise((_, reject) => {
          timer = setTimeout(
            () => reject(new Error(`Native request ${method} exceeded ${budgetMs}ms.`)),
            budgetMs,
          );
        }),
      ]);
    } finally {
      if (timer) clearTimeout(timer);
    }
  }

  return syncRuntime;
}

function createFollowerBridge(collection, status, coordinator = null, directFallback = null) {
  return {
    mode: 'follower',
    collection,
    state: null,
    multiTab: status,
    flushTimeoutMs: COMMAND_FOLLOWER_BRIDGE_TIMEOUT_MS,
    async flush(documents = []) {
      const ids = [...new Set((Array.isArray(documents) ? documents : [])
        .map((document) => String(document?.id || document?.command_id || '').trim())
        .filter(Boolean))];
      try {
        return await coordinator?.notifyDirtyAndWait?.(collection, ids, { timeoutMs: 1_000 });
      } catch (error) {
        if (typeof directFallback !== 'function') throw error;
        await directFallback(error);
        return { ok: true, mode: 'direct-fallback' };
      }
    },
    stop: async () => {},
  };
}

async function flushLeaderDirtyCollection(bridge, collection, ids = []) {
  const state = bridge?.state;
  if (!state) throw new Error(`Leader bridge for ${collection} is unavailable.`);
  const exactIds = [...new Set((Array.isArray(ids) ? ids : [])
    .map((id) => String(id || '').trim())
    .filter(Boolean))];
  const storage = state.collection?.storageCollection;
  if (
    exactIds.length
    && typeof storage?.findDocumentsById === 'function'
    && typeof state.pushDocumentsToRemotePeers === 'function'
  ) {
    for (let attempt = 0; attempt < 20; attempt += 1) {
      const found = await storage.findDocumentsById(exactIds);
      const documents = exactIds
        .map((id) => Array.isArray(found)
          ? found.find((document) => String(document?.id || document?.command_id || '') === id)
          : found?.[id])
        .filter(Boolean);
      if (documents.length === exactIds.length) {
        return state.pushDocumentsToRemotePeers(documents);
      }
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  }
  if (typeof state.pushToRemotePeers === 'function') return state.pushToRemotePeers();
  return state.scheduleLocalWritePush?.();
}

function createPendingCollectionBridge(collection, bridgePromise) {
  let stopRequested = false;
  let stopTask = null;
  const ready = Promise.resolve(bridgePromise);
  const stopResolvedBridge = (bridge) => {
    if (!bridge) return Promise.resolve(false);
    if (!stopTask) {
      stopTask = withTimeout(bridge.stop?.(), 3000)
        .then(() => true)
        .catch(() => false);
    }
    return stopTask;
  };
  ready.then(async (bridge) => {
    if (!stopRequested) return;
    await stopResolvedBridge(bridge);
  }).catch(() => null);
  return {
    mode: 'pending',
    collection,
    reason: 'startup-in-progress',
    state: null,
    ready,
    async stop() {
      stopRequested = true;
      const bridge = await withTimeout(ready.catch(() => null), 3000);
      if (!bridge) return false;
      return stopResolvedBridge(bridge);
    },
  };
}

function sanitizeMultiTabStatus(status) {
  if (!status || typeof status !== 'object') return null;
  return {
    schema: 'ctox.rxdb.multi-tab-sync.v1',
    databaseName: typeof status.databaseName === 'string' ? status.databaseName.slice(0, 120) : null,
    role: status.role === 'leader' ? 'leader' : 'follower',
    isLeader: status.isLeader === true,
    leaderLeaseAgeMs: Number.isFinite(Number(status.leaderLeaseAgeMs))
      ? Math.max(0, Number(status.leaderLeaseAgeMs))
      : null,
    updatedAtMs: Number.isFinite(Number(status.updatedAtMs)) ? Number(status.updatedAtMs) : Date.now(),
  };
}

function sanitizeBrowserStorageStatus(status) {
  if (!status || typeof status !== 'object') return null;
  return {
    persistent: typeof status.persistent === 'boolean' ? status.persistent : null,
    ephemeralLikely: status.ephemeralLikely === true,
    quota: Number.isFinite(Number(status.quota)) ? Number(status.quota) : null,
    usage: Number.isFinite(Number(status.usage)) ? Number(status.usage) : null,
    pressureRatio: Number.isFinite(Number(status.pressureRatio)) ? Number(status.pressureRatio) : null,
    journalPendingWrites: Number(status.journalPendingWrites || 0),
    journalPendingBytes: Number(status.journalPendingBytes || 0),
    oldestPendingAtMs: Number(status.oldestPendingAtMs || 0),
    unresolvedConflicts: Number(status.unresolvedConflicts || 0),
    lastRecoveryExportAtMs: Number(status.lastRecoveryExportAtMs || 0),
    capturedAtMs: Number(status.capturedAtMs || Date.now()),
  };
}

function peerSessionKey(value) {
  if (typeof value === 'string') return value;
  if (!value || typeof value !== 'object') return '';
  const role = typeof value.role === 'string' && value.role ? value.role : 'unknown';
  const sessionId = typeof value.sessionId === 'string' && value.sessionId ? value.sessionId : '';
  return sessionId ? `${role}:${sessionId}` : '';
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function boundedCollectionStartQueueStep(
  bridgePromise,
  timeoutMs = COLLECTION_START_QUEUE_STEP_TIMEOUT_MS,
  gapMs = COLLECTION_START_GAP_MS,
) {
  return withTimeout(Promise.resolve(bridgePromise).catch(() => undefined), timeoutMs)
    .then(() => delay(gapMs));
}

function createCollectionStartLanes(lanes = COLLECTION_START_LANES) {
  return Array.from({ length: Math.max(1, lanes) }, () => Promise.resolve());
}

async function settleRestartEntries(entries, waitForStable) {
  const outcomes = await Promise.all((entries || []).map(async (entry) => {
    try {
      const bridge = await waitForStable(entry);
      return { status: 'stable', collection: entry.collection, bridge };
    } catch (error) {
      return { status: 'failed', collection: entry.collection, error };
    }
  }));
  return {
    stable: outcomes.filter(({ status }) => status === 'stable'),
    failed: outcomes.filter(({ status }) => status === 'failed'),
  };
}

async function repairRestartBatch(entries, { waitForStable, stopFailed, restartFailed }) {
  const firstAttempt = await settleRestartEntries(entries, waitForStable);
  if (!firstAttempt.failed.length) return firstAttempt;

  // Each failed member is isolated. Stable peers remain untouched while the
  // failed subset is stopped and receives its one bounded retry.
  await Promise.all(firstAttempt.failed.map((failure) => stopFailed(failure)));
  let retryEntries;
  try {
    retryEntries = await restartFailed(firstAttempt.failed);
  } catch (error) {
    retryEntries = firstAttempt.failed.map(({ collection }) => ({ collection, error }));
  }
  const retryAttempt = await settleRestartEntries(retryEntries, waitForStable);
  return {
    stable: [...firstAttempt.stable, ...retryAttempt.stable],
    failed: retryAttempt.failed,
  };
}

function resetRoomCircuitState(roomCircuit, now = Date.now()) {
  roomCircuit.state = 'closed';
  roomCircuit.consecutiveFailures = 0;
  roomCircuit.openUntilMs = 0;
  roomCircuit.nextProbeAtMs = 0;
  roomCircuit.permanent = false;
  roomCircuit.lastError = null;
  roomCircuit.lastFailureKey = '';
  roomCircuit.lastFailureAtMs = 0;
  roomCircuit.updatedAtMs = now;
}

function registerRoomRepairCycleFailure(
  roomCircuit,
  error,
  now = Date.now(),
  random = Math.random,
) {
  const serialized = serializeError(error) || {};
  roomCircuit.lastFailureKey = `${serialized.code || ''}|${serialized.message || ''}`;
  roomCircuit.lastFailureAtMs = now;
  roomCircuit.lastError = serialized;
  roomCircuit.updatedAtMs = now;
  roomCircuit.consecutiveFailures += 1;
  if (
    roomCircuit.state === 'half_open'
    || roomCircuit.consecutiveFailures >= ROOM_CIRCUIT_FAILURE_THRESHOLD
  ) {
    roomCircuit.state = 'open';
    roomCircuit.permanent = false;
    roomCircuit.openUntilMs = now + ROOM_CIRCUIT_OPEN_MS;
    roomCircuit.nextProbeAtMs = roomCircuit.openUntilMs;
    return ROOM_CIRCUIT_OPEN_MS;
  }
  roomCircuit.state = 'closed';
  const exponent = Math.max(0, roomCircuit.consecutiveFailures - 1);
  const base = Math.min(ROOM_RETRY_MAX_MS, ROOM_RETRY_BASE_MS * (2 ** exponent));
  return base + Math.floor(random() * Math.max(1, Math.floor(base / 4)));
}

function applyRoomRepairCycleOutcome(roomCircuit, { stableCount = 0, errors = [] } = {}, options = {}) {
  if (stableCount > 0) {
    resetRoomCircuitState(roomCircuit, options.now);
    return 0;
  }
  if (!errors.length) return 0;
  const cycleError = errors.length === 1
    ? errors[0]
    : new AggregateError(errors, `${errors.length} collections failed in one room repair cycle.`);
  return registerRoomRepairCycleFailure(
    roomCircuit,
    cycleError,
    options.now,
    options.random,
  );
}

function withTimeout(value, ms) {
  return Promise.race([
    Promise.resolve(value),
    delay(ms),
  ]);
}

async function withRejectingTimeout(operation, ms, message) {
  let timer = null;
  try {
    return await Promise.race([
      Promise.resolve().then(operation),
      new Promise((_, reject) => {
        timer = setTimeout(() => {
          const error = new Error(message);
          error.code = 'native_unavailable';
          error.retryable = true;
          reject(error);
        }, ms);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function waitForNativePeerOpenState(state, collection, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (hasOpenNativePeerState(state)) return true;
    // A controlled daemon/peer restart cancels the old replication promises
    // before the replacement peer opens. Cancellation is an intermediate
    // lifecycle signal here, not a terminal result for this bounded poll.
    // Keep checking the actual native peer/channel state until the deadline;
    // non-recovery is still reported as a typed peer_connect_timeout below.
    await withTimeout(Promise.resolve(state?.awaitInitialReplication?.()).catch(() => undefined), 2000);
    await withTimeout(Promise.resolve(state?.awaitInSync?.()).catch(() => undefined), 3000);
    if (hasOpenNativePeerState(state)) return true;
    await delay(500);
  }
  throw createNativePeerOpenTimeoutEvent(collection, timeoutMs);
}

async function waitForStableNativePeerOpenState(state, collection, timeoutMs, stableMs) {
  await waitForNativePeerOpenState(state, collection, timeoutMs);
  await withTimeout(Promise.resolve(state?.awaitInSync?.()).catch(() => undefined), 3000);
  await delay(stableMs);
  if (hasOpenNativePeerState(state)) return true;
  // The peer DID open — it dropped again inside the stability window. Reporting
  // that as "did not open within 1000ms" sends the reader after a deadline that
  // is not the problem: measured on thesen 09.09.2026, the open timeout is 60 s
  // and was never reached, while the message read "within 1000ms" and cost an
  // afternoon of looking at the wrong number.
  throw createNativePeerUnstableEvent(collection, stableMs);
}

function hasOpenNativePeerState(state) {
  const peerStates = state?.peerStates$?.getValue?.();
  const entries = peerStates && typeof peerStates.entries === 'function'
    ? Array.from(peerStates.entries())
    : [];
  for (const [peerId, entry] of entries) {
    if (entry?.remoteProtocol?.peerSession?.role !== 'ctox_instance') continue;
    const connection = state?.peer?.connections?.get?.(peerId);
    const channelState = connection?.channel?.readyState || '';
    const pcState = connection?.peer?.connectionState || '';
    if (channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(pcState)) {
      return true;
    }
  }
  return false;
}

function createNativePeerUnstableEvent(collection, stableMs) {
  return {
    name: 'CtoxWebRtcPeerLifecycleEvent',
    code: 'peer_unstable_after_open',
    phase: 'peer-reconnect',
    severity: 'recoverable',
    retryable: true,
    lifecycle: true,
    collection,
    stableMs,
    message: `WebRTC native peer for ${collection} opened and closed again within ${stableMs}ms; reconnect repair is scheduled.`,
  };
}

function createNativePeerOpenTimeoutEvent(collection, timeoutMs) {
  return {
    name: 'CtoxWebRtcPeerLifecycleEvent',
    code: 'peer_connect_timeout',
    phase: 'peer-reconnect',
    severity: 'recoverable',
    retryable: true,
    lifecycle: true,
    collection,
    timeoutMs,
    message: `WebRTC native peer did not open for ${collection} within ${timeoutMs}ms; reconnect repair is scheduled.`,
  };
}

function registerSignalingErrorHandler(signalingServerUrl, onError) {
  installSignalingErrorObserver();
  const matchKey = signalingUrlMatchKey(signalingServerUrl);
  const handler = { matchKey, onError };
  signalingErrorHandlers.add(handler);
  return () => signalingErrorHandlers.delete(handler);
}

function installSignalingErrorObserver() {
  if (signalingErrorObserverInstalled || typeof globalThis.WebSocket !== 'function') return;
  const NativeWebSocket = globalThis.WebSocket;
  class ObservedWebSocket extends NativeWebSocket {
    constructor(url, protocols) {
      if (protocols === undefined) {
        super(url);
      } else {
        super(url, protocols);
      }
      const requestedUrl = String(url || '');
      this.addEventListener('message', (event) => {
        const error = parseSignalingControlPlaneError(event?.data, this.url || requestedUrl);
        if (!error) return;
        for (const handler of signalingErrorHandlers) {
          if (!handler?.matchKey || handler.matchKey !== signalingUrlMatchKey(error.url)) continue;
          try { handler.onError(error); } catch {}
        }
      });
    }
  }
  for (const key of ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED']) {
    try { ObservedWebSocket[key] = NativeWebSocket[key]; } catch {}
  }
  globalThis.WebSocket = ObservedWebSocket;
  signalingErrorObserverInstalled = true;
}

function parseSignalingControlPlaneError(raw, url) {
  if (typeof raw !== 'string' || !raw.includes('ctoxError')) return null;
  let payload;
  try {
    payload = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!payload || payload.type !== 'ctoxError' || payload.scope !== 'control-plane') return null;
  const code = typeof payload.code === 'string' ? payload.code.trim() : 'control_plane_rejected';
  const reason = typeof payload.reason === 'string' ? payload.reason.trim() : code;
  return {
    name: 'CtoxSignalingControlPlaneError',
    message: reason || code,
    code,
    phase: 'signaling-control-plane',
    severity: 'error',
    retryable: RETRYABLE_CONTROL_PLANE_CODES.has(code),
    url: redactUrlSecrets(url),
  };
}

function signalingUrlMatchKey(value) {
  try {
    const url = new URL(value, window.location.href);
    return `${url.protocol}//${url.host}${url.pathname}`;
  } catch {
    return String(value || '').split('?')[0];
  }
}

async function startWebRtcReplication({
  db,
  config,
  collection,
  recordCollection,
  capabilityTokenProvider,
  onNativeQueryReady,
  onFatalPeerError,
  scheduleRestart,
}) {
  const rxCollection = db?.raw?.[collection] || db?.collection?.(collection);
  if (!rxCollection) {
    recordCollection?.(collection, { status: 'pending', reason: 'collection-not-registered' });
    // Losing the startup race produces NO event — the collection simply is not
    // in db.raw yet and nothing fails. An event-driven repair cycle can never
    // wake for a state that never announced itself. The unconditional sweep in
    // the runtime closure (scheduleUnregisteredCollectionSweep) picks this up;
    // the call below only shortens the wait when a repair cycle is idle anyway.
    // This is a RACE, not a verdict: the collection may be registered moments
    // later. The caller already knows how to recover — it drops a cached
    // 'pending' stub and starts fresh (see the `currentBridge?.mode === 'pending'`
    // branch in startCollection). Nothing ever triggered that path, so a
    // collection that lost this race stayed unsynced until a page reload.
    // Observed on a customer instance: user_thread_states sat on
    // reason='collection-not-registered' while it was demonstrably present in
    // db.raw, right next to user_thread_links which had connected normally.
    scheduleRestart?.(collection);
    return { mode: 'pending', collection, reason: 'collection-not-registered' };
  }
  if (collection === 'desktop_icons' && typeof rxCollection.find === 'function') {
    await repairDesktopIconsBeforeReplication(rxCollection);
  }
  const replicationCollection = collectionForReplication(collection, rxCollection);
  const rxdb = db?.rxdb || await loadRxdbRuntime();
  if (typeof rxdb?.replicateWebRTC !== 'function' || typeof rxdb?.getConnectionHandlerSimplePeer !== 'function') {
    throw new Error('RxDB WebRTC bundle is missing replicateWebRTC/getConnectionHandlerSimplePeer');
  }

  ensureBrowserProcessNextTick();
  const signalingServerUrl = await signalingUrlWithBrowserMetadata(firstSignalingUrl(config), config);
  const iceServers = iceServersFromConfig(config);
  const iceServersHaveTurn = iceServersContainTurn(iceServers);
  const iceServersHaveCredentialedTurn = iceServersContainCredentialedTurn(iceServers);
  // Phase 3 (single multiplexed stream): the WebRTC room is now the BARE sync
  // room shared by every collection — one signaling socket + RTCPeerConnection
  // + DataChannel per browser. `collectionTopic(...)` is retained only as a
  // human-readable per-collection label for diagnostics, not as the room. The
  // collection a frame belongs to is now carried in-band on the wire.
  const room = config.sync_room;
  const topic = collectionTopic(config.sync_room, collection);
  const batchSize = batchSizeFor(collection);
  const initialReplicationStartedAt = new Date().toISOString();
  let nativePeerProtocolReady = false;
  recordCollection?.(collection, {
    status: 'connecting',
    topic,
    signalingUrl: redactUrlSecrets(signalingServerUrl),
    iceServersConfigured: iceServers.length,
    iceServersHaveTurn,
    iceServersHaveCredentialedTurn,
    batchSize,
    initialReplicationState: 'pending',
    initialReplicationStartedAt,
    initialReplicationAt: null,
  });
  let stopped = false;
  // SYNC-30: the native peer mints ephemeral coturn TURN credentials with a ~1h
  // TTL and advertises `ice_servers_refresh_url` (the control-plane sync-config
  // endpoint). Before a relay-dependent reconnect whose current credential is
  // near expiry, the WebRTC peer calls this callback to obtain freshly-minted
  // ICE servers WITHOUT a page reload. The fetch is a CONTROL-PLANE refresh
  // (like subscription-auth / release-check) — it returns bootstrap sync config,
  // never Business OS records — so it lives here in the shell, not inside the
  // WebRTC-only rxdb runtime (which the data-plane guard keeps fetch-free).
  const iceServersRefreshUrl = String(config?.ice_servers_refresh_url || config?.iceServersRefreshUrl || '').trim();
  const refreshIceServers = iceServersRefreshUrl
    ? () => refreshIceServersFromControlPlane(iceServersRefreshUrl)
    : null;
  const connectionHandlerCreator = rxdb.getConnectionHandlerSimplePeer({
    signalingServerUrl,
    config: (iceServers.length || refreshIceServers)
      ? { iceServers, iceServersRefreshUrl, refreshIceServers }
      : undefined,
  });
  const subscriptions = [];
  const unregisterSignalingErrorHandler = registerSignalingErrorHandler(signalingServerUrl, (error) => {
    if (stopped) return;
    recordCollection?.(collection, {
      status: 'error',
      connectionStatus: 'error',
      lastError: error,
    });
    onFatalPeerError?.(error);
  });
  subscriptions.push({ unsubscribe: unregisterSignalingErrorHandler });
  let nativePeerOpenWatchdog = null;
  const replicationState = await rxdb.replicateWebRTC({
    collection: replicationCollection,
    // Phase 3: pass the BARE sync room so every collection multiplexes onto a
    // single shared CtoxWebRtcNativePeer for this room.
    topic: room,
    connectionHandlerCreator,
    pull: isDemandOnlyPullCollection(collection) ? null : { batchSize },
    push: isReadOnlyProjectionCollection(collection) ? null : { batchSize },
    retryTime: 5000,
    ctox: {
      expectedNativePeerId: String(config?.native_peer_id || config?.nativePeerId || '').trim(),
      capabilityTokenProvider,
      deviceProofProvider: getBusinessOsDeviceProof,
      onPeerProtocol(info) {
        const remoteCapabilities = Array.isArray(info?.capabilities) ? info.capabilities : [];
        const remoteCheckpoint = sanitizeRemoteCheckpoint(info?.checkpoint || null);
        const checkpointError = classifyCheckpointProtocolError(collection, remoteCapabilities, remoteCheckpoint);
        nativePeerProtocolReady = !checkpointError && hasNativePeerProtocolEvidence(info, remoteCapabilities, remoteCheckpoint);
        recordCollection?.(collection, {
          remoteProtocol: info?.protocol || null,
          remoteCapabilities,
          remotePeerSession: info?.peerSession || null,
          remoteCheckpoint,
          peerSessionSeenAt: new Date().toISOString(),
          ...(checkpointError
            ? {
                status: 'error',
                connectionStatus: 'error',
                lastError: checkpointError,
              }
            : {
                status: 'connected',
                connectionStatus: 'connected',
                connectedAt: new Date().toISOString(),
                reconnectingSince: null,
                lastError: null,
                lastLifecycleEvent: null,
              }),
        });
        if (nativePeerProtocolReady && nativePeerOpenWatchdog) {
          clearTimeout(nativePeerOpenWatchdog);
          nativePeerOpenWatchdog = null;
        }
        if (checkpointError) onFatalPeerError?.(checkpointError);
      },
      onPeerCapabilityNegotiated(info) {
        const demandOnly = isDemandOnlyPullCollection(collection);
        const queryReady = info?.queryFetchCapable === true
          && info?.demandLoaderActive === true;
        if (demandOnly && !queryReady) {
          recordCollection?.(collection, {
            status: 'error',
            connectionStatus: 'error',
            reason: 'query-fetch-capability-required',
            queryReady: false,
            lastError: {
              code: 'ctox_query_fetch_capability_required',
              message: `${collection} requires the negotiated query-fetch capability.`,
              phase: 'query-capability',
              severity: 'error',
              retryable: false,
            },
          });
          return;
        }
        recordCollection?.(collection, {
          queryReady: demandOnly ? queryReady : true,
          reason: null,
        });
        if (queryReady && typeof onNativeQueryReady === 'function') {
          try { onNativeQueryReady(collection, info); } catch (error) { console.error(error); }
        }
      },
    },
  });
  // The protocol callback may fire while the replication bridge is still being
  // constructed. Backfill from its live peer state so diagnostics and policy
  // do not remain permanently unaware of an already-open native peer.
  const existingPeerStates = replicationState.peerStates$?.getValue?.();
  const existingRemoteProtocol = existingPeerStates && typeof existingPeerStates.values === 'function'
    ? Array.from(existingPeerStates.values()).map((entry) => entry?.remoteProtocol).find(Boolean)
    : null;
  if (existingRemoteProtocol) {
    const remoteCapabilities = Array.isArray(existingRemoteProtocol.capabilities)
      ? existingRemoteProtocol.capabilities
      : [];
    const remoteCheckpoint = sanitizeRemoteCheckpoint(existingRemoteProtocol.checkpoint || null);
    const checkpointError = classifyCheckpointProtocolError(collection, remoteCapabilities, remoteCheckpoint);
    nativePeerProtocolReady = !checkpointError
      && hasNativePeerProtocolEvidence(existingRemoteProtocol, remoteCapabilities, remoteCheckpoint);
    recordCollection?.(collection, {
      remoteProtocol: existingRemoteProtocol.protocol || null,
      remoteCapabilities,
      remotePeerSession: existingRemoteProtocol.peerSession || null,
      remoteCheckpoint,
      peerSessionSeenAt: new Date().toISOString(),
      status: checkpointError ? 'error' : 'connected',
      connectionStatus: checkpointError ? 'error' : 'connected',
      lastError: checkpointError || null,
    });
    if (checkpointError) onFatalPeerError?.(checkpointError);
  }
  const recordTransportStatus = (status) => {
    if (stopped) return;
    const frameTransport = sanitizeReplicationTransportStatus(status);
    if (!frameTransport) return;
    recordCollection?.(collection, {
      frameTransport,
      // OS-A3: checkpoint progress rides the transport-status events, so it
      // only re-records on real replication activity — no timers, idle stays
      // idle. Ages are derived at snapshot time (snapshotDiagnostics).
      ...checkpointDiagnosticFields(replicationState),
      // OS-C4: field-merge observability for merge-enabled collections.
      ...mergeDiagnosticFields(rxCollection),
    });
  };
  recordTransportStatus(replicationState.getTransportStatus?.());
  const transportStatusSubscription = replicationState.transportStatus$?.subscribe?.(recordTransportStatus);
  if (transportStatusSubscription) subscriptions.push(transportStatusSubscription);
  nativePeerOpenWatchdog = setTimeout(() => {
    nativePeerOpenWatchdog = null;
    if (stopped || hasOpenNativePeerState(replicationState)) return;
    const lifecycleEvent = createNativePeerOpenTimeoutEvent(collection, NATIVE_PEER_OPEN_WATCHDOG_MS);
    recordCollection?.(collection, {
      status: 'reconnecting',
      connectionStatus: 'reconnecting',
      lastError: null,
      lastLifecycleEvent: lifecycleEvent,
      reconnectingSince: new Date().toISOString(),
    });
    onFatalPeerError?.(lifecycleEvent);
  }, NATIVE_PEER_OPEN_WATCHDOG_MS);
  subscriptions.push({
    unsubscribe() {
      if (nativePeerOpenWatchdog) clearTimeout(nativePeerOpenWatchdog);
      nativePeerOpenWatchdog = null;
    },
  });
  let lastErrorLogAt = 0;
  // AGENT GUARDRAIL: the classification ORDER below is load-bearing —
  // control-plane (fatal) -> schema (fatal) -> replication IO (record only)
  // -> transient shutdown -> peer lifecycle -> signaling blip (reconnecting)
  // -> generic. Reordering it, or escalating IO/blip errors to fatal, brings
  // back the mass-restart churn. Extend at the END, with a test.
  const errorSubscription = replicationState.error$?.subscribe?.((error) => {
    if (stopped) return;
    const now = Date.now();
    const signalingControlPlaneError = classifySignalingControlPlaneError(error);
    if (signalingControlPlaneError) {
      recordCollection?.(collection, {
        status: 'error',
        connectionStatus: 'error',
        lastError: signalingControlPlaneError,
      });
      onFatalPeerError?.(signalingControlPlaneError);
      return;
    }
    const schemaProtocolError = classifySchemaProtocolError(collection, error);
    if (schemaProtocolError) {
      recordCollection?.(collection, {
        status: 'error',
        connectionStatus: 'error',
        lastError: schemaProtocolError,
      });
      onFatalPeerError?.(schemaProtocolError);
      return;
    }
    const replicationIoError = classifyReplicationIoError(collection, error);
    if (replicationIoError) {
      recordCollection?.(collection, {
        status: 'error',
        connectionStatus: 'error',
        lastError: replicationIoError,
      });
      return;
    }
    const transientShutdownEvent = classifyTransientShutdownEvent(error);
    if (transientShutdownEvent) {
      if (hasOpenNativePeerState(replicationState)) {
        recordCollection?.(collection, {
          status: 'connected',
          connectionStatus: 'connected',
          reconnectingSince: null,
          lastError: null,
          lastLifecycleEvent: null,
        });
        return;
      }
      recordCollection?.(collection, {
        status: 'reconnecting',
        connectionStatus: 'reconnecting',
        lastError: null,
        lastLifecycleEvent: transientShutdownEvent,
        reconnectingSince: new Date().toISOString(),
      });
      return;
    }
    const lifecycleEvent = classifyPeerLifecycleEvent(error);
    if (lifecycleEvent) {
      if (hasOpenNativePeerState(replicationState)) {
        recordCollection?.(collection, {
          status: 'connected',
          connectionStatus: 'connected',
          reconnectingSince: null,
          lastError: null,
          lastLifecycleEvent: null,
        });
        return;
      }
      recordCollection?.(collection, {
        status: 'reconnecting',
        connectionStatus: 'reconnecting',
        lastError: null,
        lastLifecycleEvent: lifecycleEvent,
        reconnectingSince: new Date().toISOString(),
      });
      onFatalPeerError?.(lifecycleEvent);
      return;
    }
    if (isTransientSignalingSocketError(error)) {
      // The shared native peer auto-reconnects its signaling socket with
      // backoff; a socket-level blip is not a per-collection failure. The
      // generic fallthrough below used to mark every collection `error` and
      // arm a mass hard-restart that raced the in-progress reconnect — every
      // Wi-Fi blip turned into stop/start churn across ~80 collections.
      // Record a reconnecting hint; the unhealthy-collection sweep repairs
      // it only if it stays down.
      recordCollection?.(collection, {
        status: 'reconnecting',
        connectionStatus: 'reconnecting',
        lastError: null,
        lastLifecycleEvent: serializeError(error),
        reconnectingSince: new Date().toISOString(),
        lastRestartReason: 'replication-error',
        lastRestartAt: new Date().toISOString(),
        restartCount: (restartCounter += 1),
      });
      scheduleRestart?.(collection, 15000);
      return;
    }
    if (now - lastErrorLogAt > 5000) {
      lastErrorLogAt = now;
      const serializedError = serializeError(error);
      console.error(
        `[business-os] WebRTC replication failed for ${collection}: ${JSON.stringify(serializedError)}`,
      );
    }
    recordCollection?.(collection, {
      status: 'error',
      connectionStatus: 'error',
      lastError: serializeError(error),
    });
    if (isFatalPeerStormError(error)) onFatalPeerError?.(error);
  });
  if (errorSubscription) subscriptions.push(errorSubscription);
  let observedActive = false;
  subscribeReplicationMetric(replicationState.active$, subscriptions, (active) => {
    if (stopped) return;
    const isActive = Boolean(active);
    const now = new Date().toISOString();
    if (isActive) {
      observedActive = true;
      recordCollection?.(collection, {
        active: true,
        status: 'connected',
        connectionStatus: 'connected',
        connectedAt: now,
        reconnectingSince: null,
        lastLifecycleEvent: null,
      });
      return;
    }
    const reconnectingSince = observedActive ? now : null;
    recordCollection?.(collection, {
      active: false,
      status: observedActive ? 'reconnecting' : 'connecting',
      connectionStatus: observedActive ? 'reconnecting' : 'connecting',
      reconnectingSince,
      ...(observedActive
        ? {
          lastRestartReason: 'became-inactive',
          lastRestartAt: new Date().toISOString(),
          restartCount: (restartCounter += 1),
        }
        : {}),
    });
    if (observedActive) scheduleRestart?.(collection, 750);
  });
  subscribeReplicationMetric(replicationState.canceled$, subscriptions, (canceled) => {
    if (stopped) return;
    if (canceled) recordCollection?.(collection, { status: 'stopped', connectionStatus: 'stopped' });
  });
  const stopInitialReplicationWatch = watchInitialReplication({
    replicationState,
    collection,
    recordCollection,
    isStopped: () => stopped,
    startedAt: initialReplicationStartedAt,
    canCompleteInitialReplication: () => nativePeerProtocolReady && hasOpenNativePeerState(replicationState),
    scheduleRestart,
  });
  subscriptions.push({ unsubscribe: stopInitialReplicationWatch });

  return {
    mode: 'webrtc',
    collection,
    topic,
    state: replicationState,
    pullNow: async () => {},
    flush: async () => {},
    async stop() {
      stopped = true;
      if (nativePeerOpenWatchdog) clearTimeout(nativePeerOpenWatchdog);
      nativePeerOpenWatchdog = null;
      for (const subscription of subscriptions) {
        try { subscription?.unsubscribe?.(); } catch {}
      }
      try { await withTimeout(replicationState.cancel?.(), 3000); } catch {}
    },
  };
}

// desktop_icons is a permissive, field-merging browser cache. Keep the
// destructive preflight repair above, and also project at the storage boundary
// so a row written while replication is live cannot leak browser-only fields.
function collectionForReplication(collectionName, collection) {
  if (collectionName !== 'desktop_icons' || !collection?.storageCollection) return collection;
  const cached = desktopIconReplicationCollections.get(collection);
  if (cached) return cached;

  const storage = collection.storageCollection;
  const replicationStorage = new Proxy(storage, {
    get(target, property) {
      if (property === 'getChangedDocumentsSince' && typeof target.getChangedDocumentsSince === 'function') {
        return async (...args) => {
          const result = await target.getChangedDocumentsSince(...args);
          if (!result || typeof result !== 'object') return result;
          return {
            ...result,
            documents: Array.isArray(result.documents)
              ? result.documents.map(projectDesktopIconForReplication)
              : [],
          };
        };
      }
      if (property === 'bulkWrite' && typeof target.bulkWrite === 'function') {
        return (rows, ...args) => target.bulkWrite(
          Array.isArray(rows) ? rows.map(projectDesktopIconStorageRow) : rows,
          ...args,
        );
      }
      return boundProxyMember(target, property);
    },
  });
  const replicationCollection = new Proxy(collection, {
    get(target, property) {
      if (property === 'storageCollection') return replicationStorage;
      return boundProxyMember(target, property);
    },
  });
  desktopIconReplicationCollections.set(collection, replicationCollection);
  return replicationCollection;
}

function boundProxyMember(target, property) {
  const value = Reflect.get(target, property, target);
  return typeof value === 'function' ? value.bind(target) : value;
}

function projectDesktopIconStorageRow(row) {
  if (row?.document && typeof row.document === 'object') {
    return { ...row, document: projectDesktopIconForReplication(row.document) };
  }
  return projectDesktopIconForReplication(row);
}

function projectDesktopIconForReplication(raw) {
  const source = raw && typeof raw === 'object' ? raw : {};
  const updatedAtMs = boundedPositiveNumber(source.updated_at_ms, source._meta?.lwt);
  const lwt = boundedPositiveNumber(source._meta?.lwt, updatedAtMs);
  const icon = {
    id: boundedString(source.id, 128),
    target_type: boundedString(source.target_type, 32) || 'app',
    target_module: boundedString(source.target_module, 128),
    target_record_id: boundedString(source.target_record_id, 256),
    label: boundedString(source.label, 256),
    glyph: isSafeDesktopIconGlyph(source.glyph) ? String(source.glyph).trim() : '◻︎',
    x: boundedNumber(source.x),
    y: boundedNumber(source.y),
    pinned: Boolean(source.pinned),
    hidden: Boolean(source.hidden),
    sort_index: boundedNumber(source.sort_index),
    updated_at_ms: updatedAtMs,
    _deleted: Boolean(source._deleted),
    _meta: { lwt },
  };
  const revision = boundedString(source._rev, 256);
  if (revision) icon._rev = revision;
  const hybridLogicalClock = boundedString(source._meta?.ctoxHlc, 256);
  if (hybridLogicalClock) icon._meta.ctoxHlc = hybridLogicalClock;
  return icon;
}

async function repairDesktopIconsBeforeReplication(collection) {
  let repairPromise = desktopIconRepairPromises.get(collection);
  if (repairPromise) return repairPromise;
  repairPromise = (async () => {
    const documents = await collection.find().exec();
    for (const document of documents || []) {
      const raw = typeof document?.toJSON === 'function' ? document.toJSON() : document;
      if (!desktopIconNeedsRepair(raw)) continue;
      const sanitized = sanitizeDesktopIconForReplication(raw);
      await removeDesktopIconAttachments(document);
      // The app-local storage upsert deliberately field-merges documents.
      // Remove the corrupted cache row first so unknown legacy fields cannot
      // survive the repair and permanently block WebRTC replication.
      if (typeof collection?.storageCollection?.hardDeleteByIds === 'function') {
        await collection.storageCollection.hardDeleteByIds([raw.id]);
      }
      await collection.upsert(sanitized);
      const repairedDocument = await collection.findOne(raw.id).exec();
      const repaired = typeof repairedDocument?.toJSON === 'function'
        ? repairedDocument.toJSON()
        : repairedDocument;
      if (desktopIconNeedsRepair(repaired)) {
        throw new Error(`Desktop icon ${boundedString(raw.id, 128)} remains unsafe after repair.`);
      }
    }
  })();
  desktopIconRepairPromises.set(collection, repairPromise);
  try {
    await repairPromise;
  } catch (error) {
    desktopIconRepairPromises.delete(collection);
    throw error;
  }
}

function desktopIconNeedsRepair(raw) {
  if (!raw || typeof raw !== 'object') return false;
  if (!isSafeDesktopIconGlyph(raw.glyph)) return true;
  if (raw._attachments && Object.keys(raw._attachments).length > 0) return true;
  if (encodedJsonSize(raw) > 64 * 1024) return true;
  return Object.keys(raw).some((key) => !DESKTOP_ICON_SAFE_FIELDS.has(key));
}

function sanitizeDesktopIconForReplication(raw) {
  const now = Date.now();
  const icon = {
    id: boundedString(raw.id, 128),
    target_type: boundedString(raw.target_type, 32) || 'app',
    target_module: boundedString(raw.target_module, 128),
    target_record_id: boundedString(raw.target_record_id, 256),
    label: boundedString(raw.label, 256),
    glyph: isSafeDesktopIconGlyph(raw.glyph) ? String(raw.glyph).trim() : '◻︎',
    x: boundedNumber(raw.x),
    y: boundedNumber(raw.y),
    pinned: Boolean(raw.pinned),
    hidden: Boolean(raw.hidden),
    sort_index: boundedNumber(raw.sort_index),
    updated_at_ms: now,
    _deleted: Boolean(raw._deleted),
  };
  return icon;
}

async function removeDesktopIconAttachments(document) {
  const attachments = typeof document?.allAttachments === 'function'
    ? document.allAttachments()
    : [];
  for (const attachment of attachments || []) {
    await attachment?.remove?.();
  }
}

function encodedJsonSize(value) {
  try {
    const json = JSON.stringify(value);
    return typeof TextEncoder === 'function'
      ? new TextEncoder().encode(json).byteLength
      : json.length;
  } catch {
    return Number.POSITIVE_INFINITY;
  }
}

function isSafeDesktopIconGlyph(value) {
  const glyph = String(value || '').trim();
  if (!glyph || glyph.length > 16) return false;
  return !/^(?:data:|https?:)/i.test(glyph) && !/[<>{}]/.test(glyph);
}

function boundedString(value, maxLength) {
  return String(value || '').trim().slice(0, maxLength);
}

function boundedNumber(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return 0;
  return Math.max(-100_000, Math.min(100_000, number));
}

function boundedPositiveNumber(value, fallback = 1) {
  const number = Number(value);
  if (Number.isFinite(number) && number > 0) return Math.min(number, 1_000_000_000_000_000);
  const fallbackNumber = Number(fallback);
  if (Number.isFinite(fallbackNumber) && fallbackNumber > 0) {
    return Math.min(fallbackNumber, 1_000_000_000_000_000);
  }
  return 1;
}

function formatLifecycleError(error) {
  if (!error) return '';
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

// How long the initial catch-up may run without completing before the
// collection is declared stalled and handed to the restart sweep. The
// awaiter promise can hang FOREVER (handshake done, pull stuck) — without
// this watchdog the collection showed 'connected'/'connecting' with no data
// until a page reload, invisible to every repair path.
const INITIAL_REPLICATION_STALL_MS = 45_000;

function watchInitialReplication({
  replicationState,
  collection,
  recordCollection,
  isStopped,
  startedAt,
  canCompleteInitialReplication,
  scheduleRestart,
}) {
  const awaitInitialReplication = initialReplicationAwaiter(replicationState);
  if (!awaitInitialReplication) {
    recordCollection?.(collection, {
      initialReplicationState: 'unsupported',
      initialReplicationStartedAt: startedAt || new Date().toISOString(),
    });
    return () => {};
  }
  let lastProgressSignature = initialReplicationProgressSignature(replicationState);
  let stallTimer = null;
  const armStallTimer = () => {
    if (stallTimer) clearTimeout(stallTimer);
    stallTimer = setTimeout(() => {
      stallTimer = null;
      if (isStopped?.()) return;
      const progressSignature = initialReplicationProgressSignature(replicationState);
      if (progressSignature && progressSignature !== lastProgressSignature) {
        lastProgressSignature = progressSignature;
        recordCollection?.(collection, {
          status: hasOpenNativePeerState(replicationState) ? 'connected' : 'connecting',
          connectionStatus: hasOpenNativePeerState(replicationState) ? 'connected' : 'connecting',
          initialReplicationState: 'pending',
          lastError: null,
          lastLifecycleEvent: null,
        });
        armStallTimer();
        return;
      }
      recordCollection?.(collection, {
        status: 'reconnecting',
        connectionStatus: 'reconnecting',
        initialReplicationState: 'stalled',
        reconnectingSince: new Date().toISOString(),
        // Ohne diesen Vermerk laesst sich im Feld nicht unterscheiden, ob eine
        // Kollektion vom Stillstandswaechter, vom Fehlerpfad oder vom
        // Heartbeat-Sweep neu gestartet wurde - der Neustart trifft ueber
        // scheduleRestartOfUnhealthyCollections immer den ganzen Raum.
        lastRestartReason: 'initial-replication-stalled',
        lastRestartAt: new Date().toISOString(),
        restartCount: (restartCounter += 1),
      });
      scheduleRestart?.(collection, 1000);
    }, INITIAL_REPLICATION_STALL_MS);
  };
  armStallTimer();
  recordCollection?.(collection, {
    initialReplicationState: 'pending',
    initialReplicationSource: awaitInitialReplication.source,
    initialReplicationStartedAt: startedAt || new Date().toISOString(),
  });
  Promise.resolve()
    .then(() => awaitInitialReplication.fn.call(awaitInitialReplication.receiver || replicationState))
    .then(async () => {
      if (isStopped?.()) return;
      if (canCompleteInitialReplication && !canCompleteInitialReplication()) {
        recordCollection?.(collection, {
          status: 'connecting',
          connectionStatus: 'connecting',
          initialReplicationState: 'waiting-for-peer',
          initialReplicationSource: awaitInitialReplication.source,
          initialReplicationAt: null,
          lastError: null,
        });
        const ready = await waitForCondition(canCompleteInitialReplication, 30000, 250, isStopped);
        if (isStopped?.()) {
          if (stallTimer) clearTimeout(stallTimer);
          return;
        }
        if (!ready) {
          // Peer never became ready: do NOT give up silently (the old
          // behavior left the collection in 'waiting-for-peer' forever).
          // Mark it restartable and arm the sweep; the stall timer is no
          // longer needed.
          if (stallTimer) clearTimeout(stallTimer);
          recordCollection?.(collection, {
            status: 'reconnecting',
            connectionStatus: 'reconnecting',
            initialReplicationState: 'stalled-waiting-for-peer',
            reconnectingSince: new Date().toISOString(),
            lastRestartReason: 'peer-never-ready',
            lastRestartAt: new Date().toISOString(),
            restartCount: (restartCounter += 1),
          });
          scheduleRestart?.(collection, 1000);
          return;
        }
      }
      if (stallTimer) clearTimeout(stallTimer);
      recordCollection?.(collection, {
        status: 'connected',
        connectionStatus: 'connected',
        initialReplicationState: 'complete',
        initialReplicationSource: awaitInitialReplication.source,
        initialReplicationAt: new Date().toISOString(),
        reconnectingSince: null,
        lastError: null,
        lastLifecycleEvent: null,
      });
    })
    .catch((error) => {
      if (stallTimer) clearTimeout(stallTimer);
      if (isStopped?.()) return;
      recordCollection?.(collection, {
        status: 'error',
        connectionStatus: 'error',
        initialReplicationState: 'failed',
        initialReplicationSource: awaitInitialReplication.source,
        lastError: serializeError(error),
      });
    });
  return () => {
    if (stallTimer) clearTimeout(stallTimer);
  };
}

function initialReplicationProgressSignature(replicationState) {
  if (!replicationState || typeof replicationState !== 'object') return '';
  let status = {};
  try {
    status = typeof replicationState.getTransportStatus === 'function'
      ? replicationState.getTransportStatus() || {}
      : {};
  } catch {
    status = {};
  }
  return JSON.stringify({
    open: hasOpenNativePeerState(replicationState),
    pullInProgress: status.pullInProgress === true || replicationState.pullInProgress === true,
    pushInProgress: status.pushInProgress === true || replicationState.pushInProgress === true,
    pendingRequests: progressNumber(status.pendingRequests),
    pendingAcks: progressNumber(status.pendingAcks),
    activeTransfers: progressNumber(status.activeTransfers),
    incomingTransfers: progressNumber(status.incomingTransfers),
    sentFrames: progressNumber(status.sentFrames),
    sentBytes: progressNumber(status.sentBytes),
    receivedFrames: progressNumber(status.receivedFrames),
    receivedBytes: progressNumber(status.receivedBytes),
    queuedFrames: progressNumber(status.queuedFrames),
    sentScheduledFrames: progressNumber(status.sentScheduledFrames),
    pullCheckpoints: checkpointProgressSignature(replicationState.pullCheckpointsByPeer),
    pushCheckpoints: checkpointProgressSignature(replicationState.pushCheckpointsByPeer),
  });
}

function checkpointProgressSignature(map) {
  if (!map || typeof map.entries !== 'function') return [];
  return Array.from(map.entries())
    .map(([peerId, checkpoint]) => [
      String(peerId || ''),
      String(checkpoint?.id || ''),
      progressNumber(checkpoint?.lwt),
    ])
    .sort((a, b) => a[0].localeCompare(b[0]));
}

function progressNumber(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

// OS-A3: per-collection checkpoint progress for the diagnostics surface —
// the lwt of the newest master row this browser pulled / pushed (max across
// peers). Consumers read staleness as `pullCheckpointAgeMs` in the snapshot.
function checkpointDiagnosticFields(replicationState) {
  const fields = {};
  const pullLwt = maxCheckpointLwt(replicationState?.pullCheckpointsByPeer);
  const pushLwt = maxCheckpointLwt(replicationState?.pushCheckpointsByPeer);
  if (pullLwt > 0) fields.pullCheckpointLwt = pullLwt;
  if (pushLwt > 0) fields.pushCheckpointLwt = pushLwt;
  return fields;
}

function maxCheckpointLwt(map) {
  if (!map || typeof map.values !== 'function') return 0;
  let max = 0;
  for (const checkpoint of map.values()) {
    const lwt = progressNumber(checkpoint?.lwt);
    if (lwt > max) max = lwt;
  }
  return max;
}

// OS-C4: field-merge counters for merge-enabled collections (docs/ctox-rxdb.md
// §8.2). Zero-noise: collections without the strategy record nothing.
function mergeDiagnosticFields(rxCollection) {
  const stats = rxCollection?.storageCollection?.mergeStats;
  if (!stats) return {};
  const pull = progressNumber(stats.pullFieldMerges);
  const push = progressNumber(stats.pushConflictMerges);
  if (pull === 0 && push === 0) return {};
  return { pullFieldMerges: pull, pushConflictMerges: push };
}

function waitForCondition(predicate, timeoutMs, intervalMs, isStopped) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve) => {
    const tick = () => {
      if (isStopped?.()) {
        resolve(false);
        return;
      }
      try {
        if (predicate()) {
          resolve(true);
          return;
        }
      } catch {}
      if (Date.now() >= deadline) {
        resolve(false);
        return;
      }
      setTimeout(tick, intervalMs);
    };
    tick();
  });
}

function initialReplicationAwaiter(replicationState) {
  if (typeof replicationState?.awaitInitialReplication === 'function') {
    return { fn: replicationState.awaitInitialReplication, receiver: replicationState, source: 'awaitInitialReplication' };
  }
  if (typeof replicationState?.awaitInSync === 'function') {
    return { fn: replicationState.awaitInSync, receiver: replicationState, source: 'awaitInSync' };
  }
  if (replicationState?.peerStates$ && typeof replicationState.peerStates$.subscribe === 'function') {
    return { fn: () => awaitWebRtcPoolInitialReplication(replicationState), receiver: null, source: 'webrtcPeerReplicationState' };
  }
  return null;
}

async function awaitWebRtcPoolInitialReplication(pool) {
  const peerStates = await waitForWebRtcPeerStates(pool, 30000);
  const nestedStates = [...peerStates.values()]
    .map((peerState) => peerState?.replicationState)
    .filter(Boolean);
  if (!nestedStates.length) return true;
  await Promise.all(nestedStates.map((state) => {
    if (typeof state.awaitInitialReplication === 'function') {
      return state.awaitInitialReplication();
    }
    if (typeof state.awaitInSync === 'function') {
      return state.awaitInSync();
    }
    return true;
  }));
  return true;
}

function waitForWebRtcPeerStates(pool, timeoutMs) {
  const existing = pool.peerStates$?.getValue?.();
  if (existing?.size) return Promise.resolve(existing);
  return new Promise((resolve, reject) => {
    let settled = false;
    let subscription = null;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      try { subscription?.unsubscribe?.(); } catch {}
      reject(new Error('Timed out waiting for WebRTC peer state'));
    }, timeoutMs);
    subscription = pool.peerStates$.subscribe((peerStates) => {
      if (settled || !peerStates?.size) return;
      settled = true;
      clearTimeout(timer);
      try { subscription?.unsubscribe?.(); } catch {}
      resolve(peerStates);
    });
  });
}

function hasNativePeerProtocolEvidence(info, remoteCapabilities, remoteCheckpoint) {
  const capabilities = Array.isArray(remoteCapabilities) ? remoteCapabilities : [];
  const peerSession = info?.peerSession;
  const peerSessionId = typeof peerSession === 'string'
    ? peerSession
    : typeof peerSession?.sessionId === 'string'
      ? peerSession.sessionId
      : '';
  const peerRole = typeof peerSession === 'object' && peerSession
    ? peerSession.role
    : '';
  return info?.protocol === CTOX_RXDB_PROTOCOL &&
    (peerRole === 'ctox_instance' || String(peerSessionId).length > 0) &&
    capabilities.includes('ctox-peer-session-v1') &&
    capabilities.includes('ctox-checkpoint-epoch-v1') &&
    remoteCheckpoint?.state === 'advertised' &&
    typeof remoteCheckpoint.epoch === 'string' &&
    remoteCheckpoint.epoch.length > 0;
}

function isFatalPeerStormError(error) {
  const haystack = [
    error?.code,
    error?.parameters?.error?.code,
    error?.message,
    (() => {
      try { return JSON.stringify(error?.parameters || null); } catch { return ''; }
    })(),
  ].filter(Boolean).join('\n');
  return haystack.includes('ERR_SET_LOCAL_DESCRIPTION')
    || haystack.includes('ERR_PC_CONSTRUCTOR')
    || haystack.includes('ERR_CONNECTION_FAILURE')
    || haystack.includes('Cannot create so many PeerConnections')
    || haystack.includes('Still in CONNECTING state');
}

function createDiagnostics(config, mode = 'webrtc') {
  const iceServers = iceServersFromConfig(config);
  return {
    mode,
    phase: 'initializing',
    startedAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    syncRoom: typeof config?.sync_room === 'string' ? config.sync_room : null,
    signalingUrls: sanitizedSignalingUrls(config),
    iceServersConfigured: iceServers.length,
    iceServersHaveTurn: iceServersContainTurn(iceServers),
    iceServersHaveCredentialedTurn: iceServersContainCredentialedTurn(iceServers),
    protocol: CTOX_RXDB_PROTOCOL,
    capabilities: CTOX_BROWSER_CAPABILITIES,
    collections: {},
    commandPlane: createCommandPlaneDiagnostics(),
    roomCircuit: {
      state: 'closed',
      consecutiveFailures: 0,
      openUntilMs: 0,
      nextProbeAtMs: 0,
      permanent: false,
      lastError: null,
      lastFailureKey: '',
      lastFailureAtMs: 0,
      updatedAtMs: Date.now(),
    },
    lastError: null,
    lastLifecycleEvent: null,
  };
}

function createCommandPlaneDiagnostics() {
  return {
    schema: 'ctox.browser.command_plane.v1',
    counters: {},
    latency: {},
    commandTriggeredRestarts: 0,
  };
}

function recordCommandPlaneMetric(commandPlane, name, durationMs) {
  commandPlane.counters[name] = Number(commandPlane.counters[name] || 0) + 1;
  const duration = Number(durationMs);
  if (!Number.isFinite(duration) || duration < 0) return;
  const current = commandPlane.latency[name] || {
    samples: 0,
    totalMs: 0,
    maxMs: 0,
    recentMs: [],
  };
  current.samples += 1;
  current.totalMs += duration;
  current.maxMs = Math.max(current.maxMs, duration);
  current.recentMs.push(duration);
  if (current.recentMs.length > 256) current.recentMs.shift();
  const sorted = [...current.recentMs].sort((left, right) => left - right);
  current.avgMs = Math.round(current.totalMs / current.samples);
  current.p95Ms = sorted[Math.max(0, Math.ceil(sorted.length * 0.95) - 1)] || 0;
  commandPlane.latency[name] = current;
}

function snapshotDiagnostics(diagnostics) {
  // OS-A3: staleness ages are derived AT SNAPSHOT TIME from the recorded
  // checkpoint lwts, so no timer keeps them fresh (idle stays idle). An old
  // age on an idle collection is expected (nothing was written); an old age
  // while sibling collections advance points at a lagging pull.
  const nowMs = Date.now();
  const collections = {};
  for (const [name, entry] of Object.entries(diagnostics.collections)) {
    const next = { ...entry };
    if (Number(entry?.pullCheckpointLwt) > 0) {
      next.pullCheckpointAgeMs = Math.max(0, nowMs - Number(entry.pullCheckpointLwt));
    }
    if (Number(entry?.pushCheckpointLwt) > 0) {
      next.pushCheckpointAgeMs = Math.max(0, nowMs - Number(entry.pushCheckpointLwt));
    }
    collections[name] = next;
  }
  return {
    ...diagnostics,
    collections,
  };
}

function isUrgentDiagnosticUpdate(updates = {}) {
  if (updates.lastError != null || updates.lastLifecycleEvent != null) return true;
  return ['ready', 'failed', 'reconnecting', 'stopped'].includes(String(updates.phase || ''));
}

function isUrgentCollectionDiagnostic(update = {}, status = '') {
  if (update.lastError != null || update.lastLifecycleEvent != null) return true;
  return ['failed', 'error', 'reconnecting', 'stopped'].includes(String(status || ''));
}

function sanitizedSignalingUrls(config) {
  const urls = Array.isArray(config?.signaling_urls) ? config.signaling_urls : [];
  return urls
    .filter((url) => typeof url === 'string' && url.trim())
    .map((url) => redactUrlSecrets(url));
}

function redactUrlSecrets(value) {
  try {
    const url = new URL(value, window.location.href);
    for (const key of [...url.searchParams.keys()]) {
      if (isSecretParam(key)) url.searchParams.set(key, '[redacted]');
    }
    return url.toString();
  } catch {
    return String(value || '').replace(/([?&](?:token|password|secret|room_password|signaling_room_password)=)[^&]+/gi, '$1[redacted]');
  }
}

function isSecretParam(key) {
  return /(?:token|password|secret|credential|room_password|signaling_room_password)/i.test(key);
}

function serializeError(error) {
  if (!error) return null;
  const signalingControlPlaneError = classifySignalingControlPlaneError(error);
  if (signalingControlPlaneError) return signalingControlPlaneError;
  const serialized = {
    name: typeof error.name === 'string' ? error.name : 'Error',
    message: String(error.message || error),
    code: error.code || null,
    phase: error.phase || null,
    severity: error.severity || null,
    retryable: typeof error.retryable === 'boolean' ? error.retryable : null,
  };
  const details = serializeErrorDetails(error);
  if (details) serialized.details = details;
  return serialized;
}

function serializeErrorDetails(error) {
  if (!error || typeof error !== 'object') return null;
  const details = {};
  for (const key of ['parameters', 'errors', 'error', 'direction', 'collection', 'method', 'reason']) {
    if (error[key] == null) continue;
    details[key] = boundedDiagnosticValue(error[key]);
  }
  return Object.keys(details).length ? details : null;
}

function boundedDiagnosticValue(value) {
  try {
    const json = JSON.stringify(value, (key, nested) => (
      isSecretParam(key) ? '[redacted]' : nested
    ));
    if (!json) return String(value).slice(0, 4000);
    if (json.length > 16_000) return `${json.slice(0, 16_000)}…`;
    return JSON.parse(json);
  } catch {
    return String(value).slice(0, 4000);
  }
}

function sanitizeReplicationTransportStatus(status) {
  if (!status || typeof status !== 'object') return null;
  const hasTransportEvidence = status.protocol === 'ctox-rxdb-frame-v1'
    || Number(status.maxInlineFrameBytes) > 0
    || Number(status.maxChunkChars) > 0
    || Number(status.maxTransferBytes) > 0;
  if (!hasTransportEvidence) return null;
  const numberField = (key) => Number.isFinite(Number(status[key])) ? Number(status[key]) : 0;
  const stringField = (key, fallback = null, maxLength = 120) => {
    const value = status[key];
    return typeof value === 'string' && value.trim() ? value.slice(0, maxLength) : fallback;
  };
  return {
    protocol: stringField('protocol', 'ctox-rxdb-frame-v1', 80),
    collection: stringField('collection', null, 120),
    topic: stringField('topic', null, 180),
    localSignalingPeerId: stringField('localSignalingPeerId', null, 256),
    maxInlineFrameBytes: numberField('maxInlineFrameBytes'),
    maxChunkChars: numberField('maxChunkChars'),
    maxTransferBytes: numberField('maxTransferBytes'),
    ackWindow: numberField('ackWindow'),
    sendBufferHighWater: numberField('sendBufferHighWater'),
    sendBufferLowWater: numberField('sendBufferLowWater'),
    activePeerCount: numberField('activePeerCount'),
    activeTransfers: numberField('activeTransfers'),
    pendingAcks: numberField('pendingAcks'),
    incomingTransfers: numberField('incomingTransfers'),
    completedAckCacheSize: numberField('completedAckCacheSize'),
    sentFrames: numberField('sentFrames'),
    sentBytes: numberField('sentBytes'),
    receivedFrames: numberField('receivedFrames'),
    receivedBytes: numberField('receivedBytes'),
    retryCount: numberField('retryCount'),
    resumeRequestCount: numberField('resumeRequestCount'),
    resumeAckCount: numberField('resumeAckCount'),
    backpressureWaitCount: numberField('backpressureWaitCount'),
    backpressureStallCount: numberField('backpressureStallCount'),
    queuedFrames: numberField('queuedFrames'),
    sentScheduledFrames: numberField('sentScheduledFrames'),
    priorityQueueDepth: numberField('priorityQueueDepth'),
    highPriorityQueueDepth: numberField('highPriorityQueueDepth'),
    normalPriorityQueueDepth: numberField('normalPriorityQueueDepth'),
    lowPriorityQueueDepth: numberField('lowPriorityQueueDepth'),
    lastSendPriority: stringField('lastSendPriority', 'normal', 20),
    lastAckLagMs: numberField('lastAckLagMs'),
    lastBufferedAmount: numberField('lastBufferedAmount'),
    collectionReadinessState: normalizeCollectionReadinessState(status.collectionReadinessState),
    firstPullCompletedAtMs: numberField('firstPullCompletedAtMs'),
    pullInProgress: status.pullInProgress === true,
    pushInProgress: status.pushInProgress === true,
    demandLoading: sanitizeDemandLoadingStatus(status.demandLoading),
    demandTransport: sanitizeDemandTransportStatus(status.demandTransport),
    rtcConnections: sanitizeRtcConnectionSnapshots(status.rtcConnections),
    recentRtcEvents: sanitizeRecentRtcEvents(status.recentRtcEvents),
    connectionStates: sanitizeRtcConnectionStates(status.connectionStates),
    rtcConnectionPool: sanitizeRtcConnectionPool(status.rtcConnectionPool),
    updatedAtMs: numberField('updatedAtMs'),
    observedAt: new Date().toISOString(),
  };
}

function sanitizeDemandLoadingStatus(value) {
  if (!value || typeof value !== 'object') return null;
  const boolField = (key) => value[key] === true;
  const numberField = (key) => sanitizeNumber(value[key]);
  return {
    rxdbRuntime: sanitizeShortString(value.rxdbRuntime, 80),
    rxdbProtocolVersion: sanitizeShortString(value.rxdbProtocolVersion, 24),
    peerConnected: boolField('peerConnected'),
    peerCapabilityQueryFetchV1: boolField('peerCapabilityQueryFetchV1'),
    queryDemandLoadingEnabled: boolField('queryDemandLoadingEnabled'),
    queryDemandLoadingActive: boolField('queryDemandLoadingActive'),
    queryFetchInFlight: numberField('queryFetchInFlight'),
    pendingQueryFetchCollectors: numberField('pendingQueryFetchCollectors'),
    queuedQueryFetchRequests: numberField('queuedQueryFetchRequests'),
    maxPendingQueryFetchCollectors: numberField('maxPendingQueryFetchCollectors'),
    queryFetchSuccessCount: numberField('queryFetchSuccessCount'),
    queryFetchErrorCount: numberField('queryFetchErrorCount'),
    queryFetchDedupHitCount: numberField('queryFetchDedupHitCount'),
    activeFileStreams: numberField('activeFileStreams'),
    pendingFileFetchCollectors: numberField('pendingFileFetchCollectors'),
    maxPendingFileFetchCollectors: numberField('maxPendingFileFetchCollectors'),
    fileBytesReceived: numberField('fileBytesReceived'),
    fileStreamErrors: numberField('fileStreamErrors'),
    fileStreamDedupHits: numberField('fileStreamDedupHits'),
    lastQueryFetchMs: nullableNumber(value.lastQueryFetchMs),
    lastFileFetchMs: nullableNumber(value.lastFileFetchMs),
  };
}

function sanitizeDemandTransportStatus(value) {
  if (!value || typeof value !== 'object') return null;
  const numberField = (key) => sanitizeNumber(value[key]);
  return {
    schema: sanitizeShortString(value.schema, 80),
    pendingQueryCollectors: numberField('pendingQueryCollectors'),
    pendingFileCollectors: numberField('pendingFileCollectors'),
    queuedQueryRequests: numberField('queuedQueryRequests'),
    activeQueryStreams: numberField('activeQueryStreams'),
    bufferedQueryChunks: numberField('bufferedQueryChunks'),
    bufferedFileChunks: numberField('bufferedFileChunks'),
    cancelledQueryRequestCacheSize: numberField('cancelledQueryRequestCacheSize'),
    queryFetchRequests: numberField('queryFetchRequests'),
    fileFetchRequests: numberField('fileFetchRequests'),
    queryChunksReceived: numberField('queryChunksReceived'),
    fileChunksReceived: numberField('fileChunksReceived'),
    queryCollectorsRejected: numberField('queryCollectorsRejected'),
    fileCollectorsRejected: numberField('fileCollectorsRejected'),
    queryCancelRequests: numberField('queryCancelRequests'),
    fileCancelRequests: numberField('fileCancelRequests'),
    maxPendingQueryCollectors: numberField('maxPendingQueryCollectors'),
    maxPendingFileCollectors: numberField('maxPendingFileCollectors'),
    maxQueuedQueryRequests: numberField('maxQueuedQueryRequests'),
    maxBufferedQueryChunks: numberField('maxBufferedQueryChunks'),
    maxBufferedFileChunks: numberField('maxBufferedFileChunks'),
  };
}

function sanitizeRtcConnectionSnapshots(value) {
  if (!Array.isArray(value)) return [];
  return value.slice(-12).map((entry) => ({
    peerId: sanitizeShortString(entry?.peerId, 80),
    collection: sanitizeShortString(entry?.collection, 120),
    ageMs: sanitizeNumber(entry?.ageMs),
    signalingState: sanitizeShortString(entry?.signalingState, 40),
    iceConnectionState: sanitizeShortString(entry?.iceConnectionState, 40),
    iceGatheringState: sanitizeShortString(entry?.iceGatheringState, 40),
    connectionState: sanitizeShortString(entry?.connectionState, 40),
    channelReadyState: sanitizeShortString(entry?.channelReadyState, 40),
    pendingCandidates: sanitizeNumber(entry?.pendingCandidates),
    hasLocalDescription: entry?.hasLocalDescription === true,
    hasRemoteDescription: entry?.hasRemoteDescription === true,
    localCandidateTypes: sanitizeCandidateTypeCounts(entry?.localCandidateTypes),
    remoteCandidateTypes: sanitizeCandidateTypeCounts(entry?.remoteCandidateTypes),
    signal: sanitizeSignalStats(entry?.signal),
    lastError: entry?.lastError ? serializeError(entry.lastError) : null,
  }));
}

function sanitizeRecentRtcEvents(value) {
  if (!Array.isArray(value)) return [];
  return value.slice(-24).map((entry) => ({
    atMs: sanitizeNumber(entry?.atMs),
    event: sanitizeShortString(entry?.event, 80),
    peerId: sanitizeShortString(entry?.peerId, 80),
    collection: sanitizeShortString(entry?.collection, 120),
    state: sanitizeShortString(entry?.state, 80),
    signalingState: sanitizeShortString(entry?.signalingState, 80),
    connectionState: sanitizeShortString(entry?.connectionState, 80),
    iceConnectionState: sanitizeShortString(entry?.iceConnectionState, 80),
    iceGatheringState: sanitizeShortString(entry?.iceGatheringState, 80),
    pendingCandidates: sanitizeNumber(entry?.pendingCandidates),
    ageMs: sanitizeNumber(entry?.ageMs),
  }));
}

function sanitizeRtcConnectionStates(value) {
  if (!Array.isArray(value)) return [];
  return value.slice(-12).map((entry) => ({
    peerId: sanitizeShortString(entry?.peerId, 80),
    peerConnectionState: sanitizeShortString(entry?.peerConnectionState, 40),
    iceConnectionState: sanitizeShortString(entry?.iceConnectionState, 40),
    iceGatheringState: sanitizeShortString(entry?.iceGatheringState, 40),
    signalingState: sanitizeShortString(entry?.signalingState, 40),
    channelState: sanitizeShortString(entry?.channelState, 40),
    channelLabel: sanitizeShortString(entry?.channelLabel, 80),
    pendingCandidates: sanitizeNumber(entry?.pendingCandidates),
  }));
}

function sanitizeRtcConnectionPool(value) {
  if (!value || typeof value !== 'object') return null;
  return {
    maxConnections: sanitizeNumber(value.maxConnections),
    activeConnections: sanitizeNumber(value.activeConnections),
    queuedConnections: sanitizeNumber(value.queuedConnections),
    criticalActiveConnections: sanitizeNumber(value.criticalActiveConnections),
    criticalQueuedConnections: sanitizeNumber(value.criticalQueuedConnections),
  };
}

function sanitizeSignalStats(value) {
  if (!value || typeof value !== 'object') return {};
  return {
    offerSent: sanitizeNumber(value.offerSent),
    offerReceived: sanitizeNumber(value.offerReceived),
    answerSent: sanitizeNumber(value.answerSent),
    answerReceived: sanitizeNumber(value.answerReceived),
    candidateSent: sanitizeNumber(value.candidateSent),
    candidateReceived: sanitizeNumber(value.candidateReceived),
    localCandidateComplete: value.localCandidateComplete === true,
    lastLocalCandidateType: sanitizeShortString(value.lastLocalCandidateType, 40),
    lastRemoteCandidateType: sanitizeShortString(value.lastRemoteCandidateType, 40),
    lastSignalAtMs: sanitizeNumber(value.lastSignalAtMs),
  };
}

function sanitizeCandidateTypeCounts(value) {
  if (!value || typeof value !== 'object') return {};
  const result = {};
  for (const [key, count] of Object.entries(value)) {
    const normalized = sanitizeShortString(key, 40);
    if (!normalized) continue;
    result[normalized] = sanitizeNumber(count);
  }
  return result;
}

function sanitizeShortString(value, maxLength = 120) {
  return typeof value === 'string' && value.trim() ? value.slice(0, maxLength) : '';
}

function sanitizeNumber(value) {
  return Number.isFinite(Number(value)) ? Number(value) : 0;
}

function nullableNumber(value) {
  return value == null ? null : sanitizeNumber(value);
}

function isTransientSignalingSocketError(error) {
  return String(error?.code || '').trim() === 'ctox_signaling_socket_error';
}

function isHealthyCollectionStatus(status) {
  return ['connected', 'running', 'reused'].includes(String(status || '').trim());
}

function isStalledReconnectingCollection(
  current,
  nowMs = Date.now(),
  minimumAgeMs = STALLED_RECONNECT_MIN_AGE_MS,
) {
  if (!current || typeof current !== 'object') return false;
  const status = String(current.connectionStatus || current.status || '').trim();
  if (!['reconnecting', 'restarting'].includes(status)) return false;
  if (current.lastError?.retryable === false) return false;
  const reconnectingAtMs = Date.parse(String(current.reconnectingSince || ''));
  if (!Number.isFinite(reconnectingAtMs)) return false;
  return Number(nowMs) - reconnectingAtMs >= Math.max(0, Number(minimumAgeMs) || 0);
}

function repairCandidateCollectionNames(activeCollections, collectionDiagnostics = {}) {
  const candidates = new Set(activeCollections || []);
  for (const [collection, current] of Object.entries(collectionDiagnostics || {})) {
    // Demand-only leases may expire while an already-open bridge is still
    // marked active and reconnecting. Such a bridge disappeared from the
    // activeCollections Set and therefore from every repair batch, even though
    // the diagnostic state proved it still needed a peer. Keep the candidate
    // set aligned with the runtime's own active marker.
    if (current?.active === true) candidates.add(collection);
  }
  return [...candidates];
}

function classifySignalingControlPlaneError(error) {
  if (!error || typeof error !== 'object') return null;
  const source = error?.detail && typeof error.detail === 'object' ? error.detail : error;
  const scope = typeof source.scope === 'string' ? source.scope : '';
  const type = typeof source.type === 'string' ? source.type : '';
  const phase = typeof source.phase === 'string' ? source.phase : '';
  const isControlPlane = source.name === 'CtoxSignalingControlPlaneError'
    || phase === 'signaling-control-plane'
    || (type === 'ctoxError' && scope === 'control-plane');
  if (!isControlPlane) return null;
  const code = typeof source.code === 'string' && source.code.trim()
    ? source.code.trim()
    : 'control_plane_rejected';
  const message = typeof source.message === 'string' && source.message.trim()
    ? source.message.trim()
    : typeof source.reason === 'string' && source.reason.trim()
      ? source.reason.trim()
      : code;
  return {
    name: 'CtoxSignalingControlPlaneError',
    code,
    phase: 'signaling-control-plane',
    severity: 'error',
    retryable: RETRYABLE_CONTROL_PLANE_CODES.has(code),
    message,
  };
}

function sanitizeRemoteCheckpoint(value) {
  if (!value || typeof value !== 'object') return null;
  return {
    source: typeof value.source === 'string' ? value.source.slice(0, 80) : null,
    state: typeof value.state === 'string' ? value.state.slice(0, 40) : null,
    collection: typeof value.collection === 'string' ? value.collection.slice(0, 120) : null,
    schemaHash: typeof value.schemaHash === 'string' ? value.schemaHash.slice(0, 96) : null,
    latestLwt: Number.isFinite(Number(value.latestLwt)) ? Number(value.latestLwt) : null,
    latestIdHash: typeof value.latestIdHash === 'string' ? value.latestIdHash.slice(0, 96) : null,
    epoch: typeof value.epoch === 'string' ? value.epoch.slice(0, 96) : null,
  };
}

function classifyCheckpointProtocolError(collection, remoteCapabilities, remoteCheckpoint) {
  const capabilities = Array.isArray(remoteCapabilities) ? remoteCapabilities : [];
  if (!capabilities.includes('ctox-checkpoint-epoch-v1')) {
    return createCheckpointProtocolError(
      'ctox_checkpoint_capability_missing',
      collection,
      'Remote RxDB peer did not advertise checkpoint epoch capability.',
    );
  }
  if (!remoteCheckpoint || remoteCheckpoint.state !== 'advertised' || !remoteCheckpoint.epoch) {
    return createCheckpointProtocolError(
      'ctox_checkpoint_epoch_missing',
      collection,
      'Remote RxDB peer did not provide advertised checkpoint epoch evidence.',
    );
  }
  return null;
}

function createCheckpointProtocolError(code, collection, message) {
  return {
    name: 'CtoxCheckpointProtocolError',
    code,
    phase: 'checkpoint-handshake',
    severity: 'error',
    retryable: false,
    collection,
    message,
  };
}

export function classifySchemaProtocolError(collection, error) {
  const serialized = serializeError(error);
  const details = extractProtocolErrorDetails(error);
  const rawCode = String(serialized?.code || '').trim();
  const rawName = String(serialized?.name || '').trim();
  const haystack = [
    rawName,
    rawCode,
    serialized?.code,
    serialized?.message,
    details.expected,
    details.actual,
    details.collection,
    details.message,
  ].filter(Boolean).join('\n');
  if (
    rawName !== 'CtoxRxdbProtocolError'
    && !rawCode.startsWith('ctox_rxdb_')
    && !haystack.includes('RC_WEBRTC_PROTOCOL')
    && !haystack.includes('schemaHash')
    && !haystack.includes('collection schema hash')
  ) {
    return null;
  }
  let code = 'ctox_schema_protocol_mismatch';
  if (rawCode.startsWith('ctox_rxdb_')) {
    code = rawCode;
  } else if (haystack.includes('collection schema hash') || haystack.includes('schemaHash')) {
    code = details.actual ? 'ctox_schema_hash_mismatch' : 'ctox_schema_hash_missing';
  } else if (details.expected === CTOX_RXDB_PROTOCOL || haystack.includes(CTOX_RXDB_PROTOCOL)) {
    code = 'ctox_schema_protocol_mismatch';
  } else if (details.expected === collection || details.collection === collection) {
    code = 'ctox_schema_collection_mismatch';
  }
  return {
    name: 'CtoxSchemaProtocolError',
    code,
    phase: 'schema-handshake',
    severity: 'error',
    retryable: false,
    collection,
    expected: sanitizeProtocolDetail(details.expected),
    actual: sanitizeProtocolDetail(details.actual),
    message: schemaProtocolMessageFor(code),
  };
}

function extractProtocolErrorDetails(error) {
  const candidates = [
    error,
    error?.parameters,
    error?.parameters?.error,
    error?.parameters?.error?.parameters,
  ];
  for (const candidate of candidates) {
    if (!candidate || typeof candidate !== 'object') continue;
    const expected = typeof candidate.expected === 'string' ? candidate.expected : '';
    const actual = typeof candidate.actual === 'string' ? candidate.actual : '';
    const collection = typeof candidate.collection === 'string' ? candidate.collection : '';
    const message = typeof candidate.message === 'string' ? candidate.message : '';
    if (expected || actual || collection || message) {
      return { expected, actual, collection, message };
    }
  }
  const raw = String(error?.message || error || '');
  return { expected: '', actual: '', collection: '', message: raw };
}

function sanitizeProtocolDetail(value) {
  return typeof value === 'string' && value.trim() ? value.trim().slice(0, 120) : null;
}

function schemaProtocolMessageFor(code) {
  if (code === 'ctox_rxdb_protocol_missing') return 'Remote RxDB peer did not provide the CTOX RxDB protocol marker.';
  if (code === 'ctox_rxdb_protocol_mismatch') return 'Remote RxDB peer uses an incompatible CTOX RxDB protocol.';
  if (code === 'ctox_rxdb_capability_missing') return 'Remote RxDB peer is missing a required CTOX capability.';
  if (code === 'ctox_rxdb_collection_mismatch') return 'Remote RxDB peer answered with a different collection name.';
  if (code === 'ctox_rxdb_schema_version_mismatch') return 'Remote RxDB peer collection schema version does not match the Browser schema.';
  if (code === 'ctox_rxdb_schema_hash_mismatch') return 'Remote RxDB peer collection schema hash does not match the Browser schema.';
  if (code === 'ctox_schema_hash_mismatch') return 'Remote RxDB peer collection schema hash does not match the Browser schema.';
  if (code === 'ctox_schema_hash_missing') return 'Remote RxDB peer did not provide a collection schema hash.';
  if (code === 'ctox_schema_collection_mismatch') return 'Remote RxDB peer answered with a different collection name.';
  return 'Remote RxDB peer is not compatible with the CTOX RxDB protocol.';
}

export function classifyReplicationIoError(collection, error) {
  const serialized = serializeError(error);
  const details = extractReplicationErrorDetails(error);
  const rawCode = String(serialized?.code || details.code || '').trim();
  const direction = details.direction === 'push' || rawCode === 'RC_PUSH' || rawCode === 'RC_PUSH_NO_AR'
    ? 'push'
    : details.direction === 'pull' || rawCode === 'RC_PULL'
      ? 'pull'
      : '';
  if (!['RC_PULL', 'RC_PUSH', 'RC_PUSH_NO_AR'].includes(rawCode) && !direction) return null;
  let code = 'ctox_replication_io_failed';
  if (rawCode === 'RC_PUSH_NO_AR') {
    code = 'ctox_replication_push_contract_invalid';
  } else if (direction === 'pull') {
    code = 'ctox_replication_pull_failed';
  } else if (direction === 'push') {
    code = 'ctox_replication_push_failed';
  }
  return {
    name: 'CtoxReplicationIoError',
    code,
    phase: direction === 'pull' ? 'replication-pull' : direction === 'push' ? 'replication-push' : 'replication-io',
    severity: 'error',
    retryable: rawCode !== 'RC_PUSH_NO_AR',
    collection,
    direction: direction || null,
    upstreamCode: rawCode || null,
    batchSize: details.batchSize !== null && Number.isFinite(Number(details.batchSize)) ? Number(details.batchSize) : null,
    rowCount: details.rowCount !== null && Number.isFinite(Number(details.rowCount)) ? Number(details.rowCount) : null,
    message: replicationIoMessageFor(code),
  };
}

function extractReplicationErrorDetails(error) {
  const candidates = [
    error,
    error?.parameters,
    error?.parameters?.error,
    error?.parameters?.error?.parameters,
  ];
  let codeOnlyFallback = null;
  for (const candidate of candidates) {
    if (!candidate || typeof candidate !== 'object') continue;
    const direction = typeof candidate.direction === 'string' ? candidate.direction : '';
    const code = typeof candidate.code === 'string' ? candidate.code : '';
    const batchSize = candidate.batchSize ?? candidate.batch_size ?? null;
    const explicitRowCount = Number.isFinite(Number(candidate.rowCount)) ? Number(candidate.rowCount) : null;
    const pushRows = Array.isArray(candidate.pushRows) ? candidate.pushRows : null;
    const pullRows = Array.isArray(candidate.pullRows) ? candidate.pullRows : null;
    if (direction || batchSize !== null || explicitRowCount !== null || pushRows || pullRows) {
      return {
        direction,
        code,
        batchSize,
        rowCount: explicitRowCount !== null ? explicitRowCount : pushRows ? pushRows.length : pullRows ? pullRows.length : null,
      };
    }
    if (code && !codeOnlyFallback) {
      codeOnlyFallback = { direction: '', code, batchSize: null, rowCount: null };
    }
  }
  return codeOnlyFallback || { direction: '', code: '', batchSize: null, rowCount: null };
}

// OS-G1: the load-bearing classification ORDER as a pure function — control
// plane (fatal) -> schema (fatal) -> replication IO -> transient shutdown ->
// peer lifecycle -> signaling blip -> generic. The live error$ subscriber in
// startWebRtcReplication implements the SAME chain (with state-dependent
// side effects); the shared corpus fixture
// src/core/rxdb/tests/fixtures/replication-error-classification.json pins
// both the order and each classifier's verdict, so a reorder or a drifted
// error shape fails error-classification-corpus-smoke instead of shipping as
// "network flakiness". Returns { kind, classified } where `classified` is
// the classifier's normalized error (null for blip/generic).
export function classifyReplicationErrorKind(collection, error) {
  const controlPlane = classifySignalingControlPlaneError(error);
  if (controlPlane) return { kind: 'control-plane', classified: controlPlane };
  const schema = classifySchemaProtocolError(collection, error);
  if (schema) return { kind: 'schema-protocol', classified: schema };
  const io = classifyReplicationIoError(collection, error);
  if (io) return { kind: 'replication-io', classified: io };
  const shutdown = classifyTransientShutdownEvent(error);
  if (shutdown) return { kind: 'transient-shutdown', classified: shutdown };
  const lifecycle = classifyPeerLifecycleEvent(error);
  if (lifecycle) return { kind: 'peer-lifecycle', classified: lifecycle };
  if (isTransientSignalingSocketError(error)) return { kind: 'signaling-blip', classified: null };
  return { kind: 'generic', classified: null };
}

export const __ctoxSyncTestHooks = {
  classifySignalingControlPlaneError,
  classifyPeerLifecycleEvent,
  classifySchemaProtocolError,
  classifyReplicationIoError,
  classifyTransientShutdownEvent,
  classifyReplicationErrorKind,
  isTransientSignalingSocketError,
  extractReplicationErrorDetails,
  initialReplicationProgressSignature,
  isDemandOnlyPullCollection,
  isModuleDemandOnlyCollection,
  moduleSyncCollections,
  shouldReplaceCachedBridgeForStart,
  createPendingCollectionBridge,
  DEMAND_ONLY_COLLECTION_START_ERROR,
  createFollowerBridge,
  flushLeaderDirtyCollection,
  COMMAND_FOLLOWER_DIRECT_OPEN_TIMEOUT_MS,
  COMMAND_FOLLOWER_DIRECT_FLUSH_TIMEOUT_MS,
  COMMAND_FOLLOWER_BRIDGE_TIMEOUT_MS,
  checkpointDiagnosticFields,
  signalingUrlWithBrowserMetadata,
  maxCheckpointLwt,
  snapshotDiagnostics,
  boundedCollectionStartQueueStep,
  repairRestartBatch,
  applyRoomRepairCycleOutcome,
  isStalledReconnectingCollection,
  repairCandidateCollectionNames,
  resetRoomCircuitState,
  collectionForReplication,
  projectDesktopIconForReplication,
  multiTabCoordinatorRoom,
};

function multiTabCoordinatorRoom(room) {
  // Shell packs can update the DB bundle without changing APP_BUILD. Use the
  // canonical loader identity so such a tab never follows an older DB runtime.
  return `${String(room || '').trim()}|release=${MULTI_TAB_COORDINATOR_EPOCH}|db=${RXDB_BUNDLE_URL}`;
}

function replicationIoMessageFor(code) {
  if (code === 'ctox_replication_pull_failed') return 'RxDB WebRTC pull from the remote peer failed.';
  if (code === 'ctox_replication_push_failed') return 'RxDB WebRTC push to the remote peer failed.';
  if (code === 'ctox_replication_push_contract_invalid') return 'Remote RxDB peer returned an invalid push response contract.';
  return 'RxDB WebRTC replication I/O failed.';
}

function classifyPeerLifecycleEvent(error) {
  const code = String(error?.code || error?.parameters?.error?.code || '');
  const message = [
    error?.message,
    (() => {
      try { return JSON.stringify(error?.parameters || null); } catch { return ''; }
    })(),
  ].filter(Boolean).join('\n');
  const haystack = [code, message].filter(Boolean).join('\n');
  let lifecycleCode = '';
  let lifecycleMessage = '';
  if (haystack.includes('ERR_CONNECTION_FAILURE')) {
    lifecycleCode = 'peer_connection_lost';
    lifecycleMessage = 'WebRTC peer connection was lost; reconnect repair is scheduled.';
  } else if (haystack.includes('peer_signal_stale') || haystack.includes('ERR_SET_REMOTE_DESCRIPTION') || haystack.includes('ERR_ADD_ICE_CANDIDATE')) {
    lifecycleCode = 'peer_signal_stale';
    lifecycleMessage = 'WebRTC peer received stale signaling data; reconnect repair is scheduled.';
  } else if (haystack.includes('ctox_data_channel_error')) {
    lifecycleCode = 'peer_data_channel_closed';
    lifecycleMessage = 'WebRTC data channel closed during peer replacement; reconnect repair is scheduled.';
  } else if (haystack.includes('peer_signal_stale')) {
    lifecycleCode = 'peer_signal_stale';
    lifecycleMessage = 'Stale WebRTC signaling arrived after peer state changed; reconnect repair is scheduled.';
  } else if (haystack.includes('ERR_SET_LOCAL_DESCRIPTION')) {
    lifecycleCode = 'peer_negotiation_failed';
    lifecycleMessage = 'WebRTC peer negotiation failed; reconnect repair is scheduled.';
  } else if (haystack.includes('ERR_SET_REMOTE_DESCRIPTION') || haystack.includes('ERR_ADD_ICE_CANDIDATE')) {
    lifecycleCode = 'peer_negotiation_failed';
    lifecycleMessage = 'WebRTC peer remote signaling failed; reconnect repair is scheduled.';
  } else if (haystack.includes('ERR_PC_CONSTRUCTOR') || haystack.includes('Cannot create so many PeerConnections')) {
    lifecycleCode = 'peer_connection_limit';
    lifecycleMessage = 'Browser peer connection limit was reached; reconnect repair is scheduled.';
  } else if (haystack.includes('ctox_webrtc_send_queue_budget_exceeded')) {
    lifecycleCode = 'peer_send_queue_pressure';
    lifecycleMessage = 'WebRTC send queue exceeded its hard budget; the wedged peer was recycled and reconnect repair is scheduled.';
  } else if (haystack.includes('Still in CONNECTING state')) {
    lifecycleCode = 'peer_connect_timeout';
    lifecycleMessage = 'WebRTC peer stayed in connecting state; reconnect repair is scheduled.';
  }
  if (!lifecycleCode) return null;
  return {
    name: 'CtoxWebRtcPeerLifecycleEvent',
    code: lifecycleCode,
    phase: 'peer-reconnect',
    severity: 'recoverable',
    retryable: true,
    lifecycle: true,
    message: lifecycleMessage,
  };
}

function classifyTransientShutdownEvent(error) {
  const message = [
    error?.name,
    error?.message,
    (() => {
      try { return JSON.stringify(error?.parameters || null); } catch { return ''; }
    })(),
  ].filter(Boolean).join('\n');
  if (
    message.includes('InvalidStateError')
    && message.includes('database connection is closing')
  ) {
    return {
      name: 'CtoxWebRtcPeerLifecycleEvent',
      code: 'local_database_closing',
      phase: 'local-restart',
      severity: 'recoverable',
      retryable: true,
      lifecycle: true,
      message: 'Local RxDB connection is closing during Browser restart; sync will reopen with the new runtime.',
    };
  }
  if (/WebRTC peer .+ is not open/.test(message)) {
    return {
      name: 'CtoxWebRtcPeerLifecycleEvent',
      code: 'peer_channel_not_open',
      phase: 'peer-reconnect',
      severity: 'recoverable',
      retryable: true,
      lifecycle: true,
      message: 'WebRTC peer channel is not open during peer replacement; reconnect will reopen the data channel.',
    };
  }
  return null;
}

function isLifecycleEvent(value) {
  return Boolean(value && value.lifecycle === true && value.name === 'CtoxWebRtcPeerLifecycleEvent');
}

function subscribeReplicationMetric(observable, subscriptions, onValue) {
  const subscription = observable?.subscribe?.((value) => {
    try {
      onValue(value);
    } catch (error) {
      // Never swallow silently: this wrapper hid a ReferenceError in the
      // active$ handler for months, which disabled the peer-drop repair path.
      console.error('[business-os] replication metric handler failed', error);
    }
  });
  if (subscription) subscriptions.push(subscription);
}

function firstSignalingUrl(config) {
  const urls = Array.isArray(config?.signaling_urls) ? config.signaling_urls : [];
  const url = urls.find((candidate) => typeof candidate === 'string' && candidate.trim());
  if (!url) throw new Error('Business OS WebRTC sync requires a signaling URL');
  return url;
}

async function signalingUrlWithBrowserMetadata(rawUrl, config) {
  let url;
  try {
    url = new URL(rawUrl, globalThis.window?.location?.href || 'http://localhost/');
  } catch {
    throw new Error('Business OS WebRTC sync requires a valid signaling URL');
  }
  if (!['ws:', 'wss:'].includes(url.protocol)) {
    throw new Error('Business OS WebRTC sync requires a ws(s) signaling URL');
  }
  const loopbackHost = ['localhost', '127.0.0.1', '[::1]', '::1'].includes(url.hostname.toLowerCase());
  if (url.protocol !== 'wss:' && !loopbackHost) {
    throw new Error('Business OS WebRTC sync requires TLS for non-loopback signaling');
  }
  if (url.username || url.password) {
    throw new Error('Business OS WebRTC sync signaling URLs must not contain userinfo credentials');
  }
  if (url.hostname === 'signaling.ctox.dev' && ['/', '/signal'].includes(url.pathname)) {
    url.pathname = '/v2';
  }
  const preserved = [...url.searchParams.entries()]
    .filter(([key]) => ![
      'client', 'role', 'instance_id', 'protocol', 'cap', 'token', 'token_iat', 'token_exp',
      'auth_version', 'browser_token_hash', 'native_token_hash',
      'signaling_browser_token', 'signalingBrowserToken',
      'signaling_room_password', 'signalingRoomPassword', 'room_password', 'roomPassword',
    ].includes(key));
  url.search = '';
  for (const [key, value] of preserved) url.searchParams.append(key, value);
  url.searchParams.set('client', 'ctox-business-os-browser');
  url.searchParams.set('role', 'browser');
  const instanceId = String(config?.instance_id || config?.instanceId || '').trim()
    || String(config?.sync_room || '').replace(/^ctox-business-os:/, '').split(':')[0];
  if (instanceId) url.searchParams.set('instance_id', instanceId);
  url.searchParams.set('protocol', CTOX_RXDB_PROTOCOL);

  // Browser signaling is a distinct role-bound credential. Never reconstruct
  // it from the native room password: that would re-expand a leaked browser
  // bootstrap into the native peer's long-lived authority.
  const token = String(config?.signaling_browser_token || config?.signalingBrowserToken || '').trim();
  if (!token) {
    throw new Error('Business OS WebRTC sync requires an explicit browser signaling token');
  }
  const authVersion = String(config?.signaling_auth_version || config?.signalingAuthVersion || '').trim();
  const browserTokenHash = String(config?.signaling_browser_token_hash || config?.signalingBrowserTokenHash || '').trim();
  const nativeTokenHash = String(config?.signaling_native_token_hash || config?.signalingNativeTokenHash || '').trim();
  if (
    authVersion !== 'ctox-role-bound-v1'
    || !/^[a-f0-9]{64}$/.test(browserTokenHash)
    || !/^[a-f0-9]{64}$/.test(nativeTokenHash)
    || browserTokenHash === nativeTokenHash
  ) {
    throw new Error('Business OS WebRTC sync requires valid role-bound signaling commitments');
  }
  const actualBrowserTokenHash = await sha256Hex(token);
  if (actualBrowserTokenHash !== browserTokenHash) {
    throw new Error('Business OS WebRTC sync browser signaling token does not match its commitment');
  }

  const issuedAt = Math.floor(Date.now() / 1000);
  url.searchParams.set('token', token);
  url.searchParams.set('token_iat', String(issuedAt));
  url.searchParams.set('token_exp', String(issuedAt + 24 * 60 * 60));
  url.searchParams.set('auth_version', authVersion);
  url.searchParams.set('browser_token_hash', browserTokenHash);
  url.searchParams.set('native_token_hash', nativeTokenHash);
  for (const capability of CTOX_BROWSER_CAPABILITIES) {
    url.searchParams.append('cap', capability);
  }
  return url.toString();
}

async function sha256Hex(value) {
  const cryptoApi = globalThis.crypto;
  const subtle = cryptoApi?.subtle;
  if (!subtle || typeof TextEncoder !== 'function') {
    throw new Error('Business OS WebRTC sync requires Web Crypto for signaling credential verification');
  }
  const digest = await subtle.digest('SHA-256', new TextEncoder().encode(value));
  const bytes = Array.from(new Uint8Array(digest));
  return bytes.map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

// SYNC-30: fetch a fresh ICE server list (incl. newly-minted ephemeral TURN
// credentials) from the control-plane sync-config endpoint. This is the
// mechanism the daemon already intends: `sync_config_for_browser` mints TURN
// creds per request and the config advertises `ice_servers_refresh_url`
// (server.rs allowlists `/api/business-os/sync/config` on the control plane —
// it carries bootstrap/config, never Business OS records, exactly like
// subscription-auth or release-check). The WebRTC runtime stays fetch-free; this
// shell helper is the only refresh caller.
async function refreshIceServersFromControlPlane(url) {
  const response = await fetch(url, {
    method: 'GET',
    headers: { accept: 'application/json' },
    credentials: 'same-origin',
    cache: 'no-store',
  });
  if (!response.ok) {
    throw new Error(`ICE server refresh failed: HTTP ${response.status}`);
  }
  const fresh = await response.json();
  return iceServersFromConfig(fresh);
}

function iceServersFromConfig(config) {
  const value = Array.isArray(config?.ice_servers) ? config.ice_servers : config?.iceServers;
  if (!Array.isArray(value)) return [];
  return value
    .map((entry) => {
      if (!entry || typeof entry !== 'object') return null;
      const urls = typeof entry.urls === 'string'
        ? entry.urls.trim()
        : Array.isArray(entry.urls)
          ? entry.urls.map((url) => (typeof url === 'string' ? url.trim() : '')).filter(Boolean)
          : null;
      if (!urls || (Array.isArray(urls) && !urls.length)) return null;
      const server = { urls };
      if (typeof entry.username === 'string' && entry.username.trim()) server.username = entry.username.trim();
      if (typeof entry.credential === 'string' && entry.credential.trim()) server.credential = entry.credential;
      return server;
    })
    .filter(Boolean);
}

function iceServersContainTurn(iceServers) {
  if (!Array.isArray(iceServers)) return false;
  return iceServers.some((entry) => {
    const urls = Array.isArray(entry?.urls) ? entry.urls : [entry?.urls];
    return urls.some((url) => /^turns?:/i.test(String(url || '').trim()));
  });
}

function iceServersContainCredentialedTurn(iceServers) {
  if (!Array.isArray(iceServers)) return false;
  return iceServers.some((entry) => {
    const urls = Array.isArray(entry?.urls) ? entry.urls : [entry?.urls];
    const hasTurn = urls.some((url) => /^turns?:/i.test(String(url || '').trim()));
    return hasTurn
      && typeof entry?.username === 'string'
      && entry.username.trim()
      && typeof entry?.credential === 'string'
      && entry.credential.trim();
  });
}

function normalizeCollectionName(collection) {
  return String(collection || '').trim();
}

function ensureBrowserProcessNextTick() {
  if (!globalThis.process) globalThis.process = {};
  if (typeof globalThis.process.nextTick !== 'function') {
    globalThis.process.nextTick = (callback, ...args) => Promise.resolve().then(() => callback(...args));
  }
}

function isReadOnlyProjectionCollection(collection) {
  return collection === 'ctox_queue_tasks'
    || collection === 'business_chats'
    || collection === 'business_module_catalog'
    || collection === 'business_workspace_branding'
    || collection === 'business_users'
    || collection === 'channel_pairing_state'
    || collection === 'communication_accounts'
    || collection === 'browser_sessions'
    || collection === 'knowledge_tables'
    || collection === 'ctox_runtime_settings';
}

// SYNC-13: a runtime-installed module can declare `syncProfile` in its
// schema.js; rx-database.mjs captures it into the globalThis-mirrored registry
// at collection registration (before sync starts). Read it WITHOUT importing
// the bundle (which would risk a duplicate module graph). Returns null for an
// undeclared collection, so the built-in static lists below stay authoritative.
function declaredCollectionSyncProfile(collection) {
  const registry = globalThis.__ctoxCollectionSyncProfiles;
  if (!(registry instanceof Map)) return null;
  return registry.get(normalizeCollectionName(collection)) || null;
}

function isDemandOnlyPullCollection(collection) {
  if (
    collection === 'desktop_file_chunks'
    || collection === 'document_blob_chunks'
    || collection === 'spreadsheet_blob_chunks'
    // Threads projections are append-heavy and can contain years of command
    // history. Pulling all of them on every clean browser profile starves the
    // query channel that is needed to render the current inbox and approval
    // cards. Keep the bridge open, but hydrate these records through bounded
    // demand queries from the Threads module.
    || collection === 'user_threads'
    || collection === 'user_thread_messages'
    || collection === 'user_thread_links'
    || collection === 'user_notifications'
    || collection === 'ctox_task_approval_requests'
    // Command submission still pushes through the live bridge. Status
    // tracking already re-queries the immutable command id every 1.5s, so a
    // full pull of the historical command/task ledger is unnecessary and can
    // delay a new command behind thousands of old records.
    || collection === 'business_commands'
    || collection === 'ctox_queue_tasks'
    // Browser history must never gate the interactive browser surface. A
    // user needs only a bounded, owner-scoped session window plus the tabs of
    // the selected session; replaying every historical session/tab delayed a
    // cold open by more than 90 seconds on the managed customer tenant instance.
    || collection === 'browser_sessions'
    || collection === 'browser_tabs'
    // Knowledge table documents embed dataframe rows and can grow far beyond
    // the WebRTC transfer ceiling as research accumulates. Research and
    // Knowledge hydrate bounded domain/table chunks through query demand
    // loading, so an eager full pull only starves those foreground reads.
    || collection === 'knowledge_tables'
    // Sellify's operational store is hundreds of thousands of rows. Its UI
    // already uses bounded selectors/pages; eagerly mirroring the entire CRM
    // into every browser caused ~15,000 WebRTC request/response cycles and
    // blocked all foreground apps for minutes. Keep only the small sync-status
    // metadata eager and hydrate business rows through query demand loading.
    || collection === 'sellify_activities'
    || collection === 'sellify_campaigns'
    || collection === 'sellify_companies'
    || collection === 'sellify_people'
    || collection === 'sellify_records'
    || collection === 'sellify_sql_rows'
  ) {
    return true;
  }
  // SYNC-13 fallback: a runtime module declaring demand-only OR demand-chunks
  // disables background pull (chunks hydrate on demand, so they are pull-only).
  const profile = declaredCollectionSyncProfile(collection);
  return profile === 'demand-only' || profile === 'demand-chunks';
}

function isModuleDemandOnlyCollection(collection) {
  if (
    collection === 'desktop_file_chunks'
    || collection === 'document_blob_chunks'
    || collection === 'spreadsheet_blob_chunks'
  ) {
    return true;
  }
  // SYNC-13 fallback: only demand-CHUNK collections are skipped at module sync
  // startup and leased on demand; a plain demand-only collection stays
  // module-startable for its bounded demand queries (mirrors user_threads).
  return declaredCollectionSyncProfile(collection) === 'demand-chunks';
}

function moduleSyncCollections(collections = []) {
  return (Array.isArray(collections) ? collections : [])
    .filter((collection) => typeof collection === 'string' && collection.trim())
    .filter((collection) => !MODULE_EXPLICIT_START_COLLECTIONS.has(collection))
    .filter((collection) => !isModuleDemandOnlyCollection(collection));
}

function shouldReplaceCachedBridgeForStart(bridge, options = {}) {
  if (options.forceDirect === true && bridge?.mode === 'follower') return true;
  return bridge?.mode === 'pending';
}
