// =============================================================================
// AGENT GUARDRAILS — ctox-rxdb data plane (read docs/ctox-rxdb.md first)
// =============================================================================
// This file orchestrates CTOX DB, the WebRTC-ONLY data plane between Business OS
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

// Per-collection sync runtime on top of CTOX DB. Repair philosophy: the
// shared native peer self-heals its transport; this layer only classifies
// errors and schedules bounded restarts.
import { batchSizeFor, collectionTopic, nativeRxdbPeerReady } from './sync-contract.js';

const CTOX_RXDB_PROTOCOL = 'ctox-rxdb-protocol-v1';
const CTOX_BROWSER_CAPABILITIES = [
  'ctox-control-plane-v1',
  'ctox-rxdb-browser-v1',
  'ctox-file-chunks-v1',
  'ctox-schema-hash-v1',
  'ctox-peer-session-v1',
  'ctox-checkpoint-epoch-v1',
];
const NATIVE_PEER_OPEN_WATCHDOG_MS = 30000;
const NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS = 30000;

const signalingErrorHandlers = new Set();
let signalingErrorObserverInstalled = false;

export function createSyncRuntime({ db, config, onDiagnostic }) {
  const bridges = new Map();
  const activeCollections = new Set();
  const suspendedCollections = new Set();
  let globalRestartTimer = null;
  let collectionStartQueue = Promise.resolve();
  let suspensionReason = '';
  let stopped = false;
  const useWebrtc = nativeRxdbPeerReady(config, db);
  if (!useWebrtc) {
    throw new Error('Business OS requires RxDB WebRTC sync; unsupported sync contract.');
  }
  const runtimeMode = 'webrtc';
  const diagnostics = createDiagnostics(config, runtimeMode);
  const emitDiagnostic = (updates = {}) => {
    if (updates.lastError !== undefined) diagnostics.lastError = updates.lastError;
    if (updates.lastLifecycleEvent !== undefined) diagnostics.lastLifecycleEvent = updates.lastLifecycleEvent;
    if (updates.phase) diagnostics.phase = updates.phase;
    if (updates.moduleId) diagnostics.moduleId = updates.moduleId;
    diagnostics.updatedAt = new Date().toISOString();
    onDiagnostic?.(snapshotDiagnostics(diagnostics));
  };
  const recordCollection = (collection, update) => {
    const current = diagnostics.collections[collection] || {};
    const updatedAt = new Date().toISOString();
    const next = {
      ...current,
      collection,
      updatedAt,
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
    emitDiagnostic({ phase: 'collection-sync' });
  };
  const stopAllBridges = async () => {
    const bridgePromises = [...bridges.values()];
    bridges.clear();
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
    return ['reconnecting', 'failed', 'error', 'stopped'].includes(status);
  };
  const scheduleRestartOfUnhealthyCollections = (triggerCollection, delayMs = 5000) => {
    if (stopped) return;
    if (globalRestartTimer) return;
    globalRestartTimer = setTimeout(async () => {
      if (stopped) return;
      globalRestartTimer = null;
      const collections = [...activeCollections].filter(collectionNeedsRestart);
      try {
        for (const collection of collections) {
          await syncRuntime.restartCollection(collection);
          await delay(250);
        }
      } catch (restartError) {
        const restartSerialized = serializeError(restartError);
        if (triggerCollection) {
          recordCollection(triggerCollection, { status: 'failed', connectionStatus: 'error', lastError: restartSerialized });
        }
        emitDiagnostic({ phase: 'failed', lastError: restartSerialized });
      } finally {
        if (!stopped && [...activeCollections].some(collectionNeedsRestart)) {
          scheduleRestartOfUnhealthyCollections(triggerCollection, 5000);
        }
      }
    }, delayMs);
  };
  const scheduleGlobalRestart = (triggerCollection, error) => {
    if (stopped) return;
    const serialized = serializeError(error);
    const lifecycleEvent = isLifecycleEvent(error) ? serialized : null;
    const reconnectingSince = new Date().toISOString();
    recordCollection(triggerCollection, {
      status: 'reconnecting',
      connectionStatus: 'reconnecting',
      lastError: !lifecycleEvent ? serialized : diagnostics.collections[triggerCollection]?.lastError || null,
      lastLifecycleEvent: lifecycleEvent || diagnostics.collections[triggerCollection]?.lastLifecycleEvent || null,
      reconnectingSince,
    });
    emitDiagnostic({
      phase: 'reconnecting',
      lastError: lifecycleEvent ? null : serialized,
      lastLifecycleEvent: lifecycleEvent,
    });
    scheduleRestartOfUnhealthyCollections(triggerCollection, 5000);
  };
  const onlineListener = () => scheduleRestartOfUnhealthyCollections(null, 250);
  if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
    window.addEventListener('online', onlineListener);
  }
  emitDiagnostic({ phase: 'ready' });
  const syncRuntime = {
    db,
    config,
    mode: runtimeMode,
    diagnostics,
    async startModule(moduleManifest) {
      const collections = moduleManifest?.collections || [];
      const results = [];
      emitDiagnostic({ phase: 'module-sync', moduleId: moduleManifest?.id || null });
      for (const collection of collections) {
        try {
          results.push({ status: 'fulfilled', value: await this.startCollection(collection) });
        } catch (reason) {
          results.push({ status: 'rejected', reason });
        }
        await delay(100);
      }
      return results;
    },
    async startCollection(collection) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      activeCollections.add(collection);
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
        const currentBridge = await withTimeout(currentBridgePromise, 1000);
        const currentStatus = current.connectionStatus || current.status || '';
        const restartNeeded = ['reconnecting', 'failed', 'error', 'stopped'].includes(currentStatus);
        if (currentBridge?.mode === 'pending') {
          // A 'pending' stub means the collection was not registered when
          // the bridge was created (schema/startup race). Reusing the cached
          // stub disabled that collection's sync until a page reload — drop
          // it and fall through to a fresh start instead.
          bridges.delete(collection);
        } else {
        const healthyReuse = Boolean(
          currentBridge
          && current.initialReplicationAt
          && current.remoteCheckpoint?.epoch,
        );
        if (healthyReuse) {
          recordCollection(collection, {
            status: 'reused',
            connectionStatus: 'connected',
            reconnectingSince: null,
            lastError: null,
            lastLifecycleEvent: null,
          });
          return bridges.get(collection);
        }
        if (!restartNeeded) {
          recordCollection(collection, {
            status: current.status || 'starting',
            connectionStatus: current.connectionStatus || 'connecting',
            reconnectingSince: null,
            lastError: null,
            lastLifecycleEvent: null,
          });
          return currentBridgePromise;
        }
        await this.stopCollection(collection);
        }
      }
      recordCollection(collection, { status: 'starting' });
      const bridgePromise = collectionStartQueue.then(() => {
        if (stopped) throw new Error('Business OS sync runtime has been stopped');
        return startWebRtcReplication({
          db,
          config,
          collection,
          recordCollection,
          onFatalPeerError: (error) => scheduleGlobalRestart(collection, error),
          // Passed down explicitly: startWebRtcReplication is a module-level
          // function, so it cannot see this closure. The previous direct call
          // threw a ReferenceError that the metric-subscription wrapper
          // swallowed — the primary "peer dropped → schedule repair" trigger
          // never ran.
          scheduleRestart: scheduleRestartOfUnhealthyCollections,
        });
      });
      // 50 ms spacing between collection starts. The original 500 ms guarded
      // the pre-multiplex world where every start dialled its own
      // RTCPeerConnection through signaling; since phase 3 all collections
      // share ONE room peer and a "start" is just a registration + catch-up
      // pull on the existing channel. 500 ms turned an 18-collection shell
      // start into 9 s of pure queue delay. A small gap is kept so start
      // bursts stay ordered and the first connection wins the race cleanly.
      collectionStartQueue = bridgePromise.catch(() => {}).then(() => delay(50));
      bridges.set(collection, bridgePromise);
      try {
        const bridge = await bridgePromise;
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
        bridges.delete(collection);
        const serialized = serializeError(error);
        recordCollection(collection, { status: 'failed', lastError: serialized });
        emitDiagnostic({ phase: 'failed', lastError: serialized });
        throw error;
      }
    },
    async stopCollection(collection) {
      const bridgePromise = bridges.get(collection);
      bridges.delete(collection);
      if (!bridgePromise) return false;
      recordCollection(collection, {
        status: 'restarting',
        connectionStatus: 'reconnecting',
        lastError: null,
        reconnectingSince: new Date().toISOString(),
      });
      try {
        const bridge = await bridgePromise;
        await withTimeout(bridge?.stop?.(), 3000);
      } catch {
        // The old bridge is already unusable. Dropping it from the cache is enough.
      }
      return true;
    },
    async restartCollection(collection) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      activeCollections.add(collection);
      await this.stopCollection(collection);
      return this.startCollection(collection);
    },
    async restartCollections(collections) {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      if (globalRestartTimer) clearTimeout(globalRestartTimer);
      globalRestartTimer = null;
      const requested = [...new Set((collections || []).filter((collection) => typeof collection === 'string' && collection.trim()))];
      for (const collection of requested) suspendedCollections.delete(collection);
      if (!suspendedCollections.size) suspensionReason = '';
      for (const collection of requested) activeCollections.add(collection);
      for (const collection of requested) {
        await this.stopCollection(collection);
      }
      collectionStartQueue = Promise.resolve();
      const restarted = [];
      for (const collection of requested) {
        restarted.push(await this.startCollection(collection));
        await delay(250);
      }
      for (let index = 0; index < requested.length; index += 1) {
        const collection = requested[index];
        try {
          await waitForNativePeerOpenState(restarted[index]?.state, collection, NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS);
        } catch {
          const lifecycleEvent = createNativePeerOpenTimeoutEvent(collection, NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS);
          recordCollection(collection, {
            status: 'reconnecting',
            connectionStatus: 'reconnecting',
            lastError: null,
            lastLifecycleEvent: lifecycleEvent,
            reconnectingSince: new Date().toISOString(),
          });
          await this.stopCollection(collection);
          restarted[index] = await this.startCollection(collection);
          try {
            await waitForNativePeerOpenState(restarted[index]?.state, collection, NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS);
          } catch (retryError) {
            throw new Error(`Native peer did not open for ${collection} after restart retry: ${formatLifecycleError(retryError)}`);
          }
        }
      }
      return restarted;
    },
    async suspendCollections(collections, reason = 'sync-suspended') {
      if (stopped) throw new Error('Business OS sync runtime has been stopped');
      if (globalRestartTimer) clearTimeout(globalRestartTimer);
      globalRestartTimer = null;
      const requested = [...new Set((collections || []).filter((collection) => typeof collection === 'string' && collection.trim()))];
      suspensionReason = reason || 'sync-suspended';
      for (const collection of requested) {
        activeCollections.add(collection);
        suspendedCollections.add(collection);
      }
      for (const collection of requested) {
        await this.stopCollection(collection);
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
      const requested = [...new Set((collections || []).filter((collection) => typeof collection === 'string' && collection.trim()))];
      for (const collection of requested) suspendedCollections.delete(collection);
      if (!suspendedCollections.size) suspensionReason = '';
      return this.restartCollections(requested);
    },
    async stop() {
      stopped = true;
      if (globalRestartTimer) clearTimeout(globalRestartTimer);
      globalRestartTimer = null;
      if (typeof window !== 'undefined' && typeof window.removeEventListener === 'function') {
        window.removeEventListener('online', onlineListener);
      }
      await stopAllBridges();
      emitDiagnostic({ phase: 'stopped' });
    },
  };
  return syncRuntime;
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

function withTimeout(value, ms) {
  return Promise.race([
    Promise.resolve(value),
    delay(ms),
  ]);
}

async function waitForNativePeerOpenState(state, collection, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (hasOpenNativePeerState(state)) return true;
    await withTimeout(state?.awaitInitialReplication?.(), 2000);
    await withTimeout(state?.awaitInSync?.(), 3000);
    if (hasOpenNativePeerState(state)) return true;
    await delay(500);
  }
  throw createNativePeerOpenTimeoutEvent(collection, timeoutMs);
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
    retryable: false,
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

async function startWebRtcReplication({ db, config, collection, recordCollection, onFatalPeerError, scheduleRestart }) {
  const rxCollection = db?.raw?.[collection] || db?.collection?.(collection);
  if (!rxCollection) {
    recordCollection?.(collection, { status: 'pending', reason: 'collection-not-registered' });
    return { mode: 'pending', collection, reason: 'collection-not-registered' };
  }
  const rxdb = db?.rxdb || await import('../rxdb/dist/ctox-rxdb-js.mjs?v=20260614-rxdb-cancel-unregister');
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
  const connectionHandlerCreator = rxdb.getConnectionHandlerSimplePeer({
    signalingServerUrl,
    config: iceServers.length ? { iceServers } : undefined,
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
    collection: rxCollection,
    // Phase 3: pass the BARE sync room so every collection multiplexes onto a
    // single shared CtoxWebRtcNativePeer for this room.
    topic: room,
    connectionHandlerCreator,
    pull: { batchSize },
    push: isReadOnlyProjectionCollection(collection) ? null : { batchSize },
    retryTime: 5000,
    ctox: {
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
    },
  });
  const recordTransportStatus = (status) => {
    if (stopped) return;
    const frameTransport = sanitizeReplicationTransportStatus(status);
    if (!frameTransport) return;
    recordCollection?.(collection, {
      frameTransport,
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
      });
      scheduleRestart?.(collection, 15000);
      return;
    }
    if (now - lastErrorLogAt > 5000) {
      lastErrorLogAt = now;
      console.error(`[business-os] WebRTC replication failed for ${collection}`, error);
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
  const stallTimer = setTimeout(() => {
    if (isStopped?.()) return;
    recordCollection?.(collection, {
      status: 'reconnecting',
      connectionStatus: 'reconnecting',
      initialReplicationState: 'stalled',
      reconnectingSince: new Date().toISOString(),
    });
    scheduleRestart?.(collection, 1000);
  }, INITIAL_REPLICATION_STALL_MS);
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
          clearTimeout(stallTimer);
          return;
        }
        if (!ready) {
          // Peer never became ready: do NOT give up silently (the old
          // behavior left the collection in 'waiting-for-peer' forever).
          // Mark it restartable and arm the sweep; the stall timer is no
          // longer needed.
          clearTimeout(stallTimer);
          recordCollection?.(collection, {
            status: 'reconnecting',
            connectionStatus: 'reconnecting',
            initialReplicationState: 'stalled-waiting-for-peer',
            reconnectingSince: new Date().toISOString(),
          });
          scheduleRestart?.(collection, 1000);
          return;
        }
      }
      clearTimeout(stallTimer);
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
      clearTimeout(stallTimer);
      if (isStopped?.()) return;
      recordCollection?.(collection, {
        status: 'error',
        connectionStatus: 'error',
        initialReplicationState: 'failed',
        initialReplicationSource: awaitInitialReplication.source,
        lastError: serializeError(error),
      });
    });
  return () => clearTimeout(stallTimer);
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
    lastError: null,
    lastLifecycleEvent: null,
  };
}

function snapshotDiagnostics(diagnostics) {
  return {
    ...diagnostics,
    collections: { ...diagnostics.collections },
  };
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
  return {
    name: typeof error.name === 'string' ? error.name : 'Error',
    message: String(error.message || error),
    code: error.code || null,
    phase: error.phase || null,
    severity: error.severity || null,
    retryable: typeof error.retryable === 'boolean' ? error.retryable : null,
  };
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
    queuedFrames: numberField('queuedFrames'),
    sentScheduledFrames: numberField('sentScheduledFrames'),
    priorityQueueDepth: numberField('priorityQueueDepth'),
    highPriorityQueueDepth: numberField('highPriorityQueueDepth'),
    normalPriorityQueueDepth: numberField('normalPriorityQueueDepth'),
    lowPriorityQueueDepth: numberField('lowPriorityQueueDepth'),
    lastSendPriority: stringField('lastSendPriority', 'normal', 20),
    lastAckLagMs: numberField('lastAckLagMs'),
    lastBufferedAmount: numberField('lastBufferedAmount'),
    pullInProgress: status.pullInProgress === true,
    pushInProgress: status.pushInProgress === true,
    rtcConnections: sanitizeRtcConnectionSnapshots(status.rtcConnections),
    recentRtcEvents: sanitizeRecentRtcEvents(status.recentRtcEvents),
    connectionStates: sanitizeRtcConnectionStates(status.connectionStates),
    rtcConnectionPool: sanitizeRtcConnectionPool(status.rtcConnectionPool),
    updatedAtMs: numberField('updatedAtMs'),
    observedAt: new Date().toISOString(),
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

function isTransientSignalingSocketError(error) {
  return String(error?.code || '').trim() === 'ctox_signaling_socket_error';
}

function isHealthyCollectionStatus(status) {
  return ['connected', 'running', 'reused'].includes(String(status || '').trim());
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
    retryable: false,
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

export const __ctoxSyncTestHooks = {
  classifySignalingControlPlaneError,
  classifyPeerLifecycleEvent,
  classifySchemaProtocolError,
  classifyReplicationIoError,
  extractReplicationErrorDetails,
};

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
  try {
    const url = new URL(rawUrl, window.location.href);
    const preserved = [...url.searchParams.entries()]
      .filter(([key]) => !['client', 'role', 'instance_id', 'protocol', 'cap', 'token', 'token_iat', 'token_exp'].includes(key));
    url.search = '';
    for (const [key, value] of preserved) url.searchParams.append(key, value);
    url.searchParams.set('client', 'ctox-business-os-browser');
    url.searchParams.set('role', 'browser');
    const instanceId = String(config?.instance_id || config?.instanceId || '').trim()
      || String(config?.sync_room || '').replace(/^ctox-business-os:/, '').split(':')[0];
    if (instanceId) url.searchParams.set('instance_id', instanceId);
    url.searchParams.set('protocol', CTOX_RXDB_PROTOCOL);
    const token = await signalingTokenFromRoomPassword(config?.signaling_room_password || config?.room_password || '');
    if (token) {
      const issuedAt = Math.floor(Date.now() / 1000);
      url.searchParams.set('token', token);
      url.searchParams.set('token_iat', String(issuedAt));
      url.searchParams.set('token_exp', String(issuedAt + 24 * 60 * 60));
    }
    for (const capability of CTOX_BROWSER_CAPABILITIES) {
      url.searchParams.append('cap', capability);
    }
    return url.toString();
  } catch {
    return rawUrl;
  }
}

async function signalingTokenFromRoomPassword(roomPassword) {
  const password = String(roomPassword || '').trim();
  if (!password) return '';
  const cryptoApi = globalThis.crypto;
  const subtle = cryptoApi?.subtle;
  if (!subtle || typeof TextEncoder !== 'function') return '';
  const digest = await subtle.digest('SHA-256', new TextEncoder().encode(password));
  const bytes = Array.from(new Uint8Array(digest));
  const base64 = btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '');
  return base64.slice(0, 32);
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
    || collection === 'business_users'
    || collection === 'channel_pairing_state'
    || collection === 'communication_accounts'
    || collection === 'knowledge_tables'
    || collection === 'ctox_runtime_settings';
}
