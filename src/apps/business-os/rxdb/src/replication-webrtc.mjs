import { CtoxSubject } from './observable.mjs';
import { createCtoxWebRtcNativePeer } from './webrtc-native.mjs';
import {
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  assertCompatibleProtocol,
  buildProtocolPayload,
  schemaHash,
  schemaHashSource,
} from './schema.mjs';
import { CTOX_QUERY_FETCH_CAPABILITY } from './protocol-contract.generated.mjs';
import { createDemandLoadingTransport } from './demand-loading-transport.mjs';
import { createQueryDemandLoader } from './query-demand-loader.mjs';
import { createFileDemandLoader } from './file-demand-loader.mjs';
import { QueryMetaStorage } from './query-meta-storage.mjs';
import { createIndexedDbMetaBackend } from './query-meta-backend-indexeddb.mjs';
import { createMemoryMetaBackend } from './query-meta-backend-memory.mjs';

const BROWSER_CAPABILITIES = [
  'ctox-rxdb-browser-v1',
  'ctox-file-chunks-v1',
  'ctox-schema-hash-v1',
  'ctox-peer-session-v1',
  'ctox-checkpoint-epoch-v1',
  CTOX_QUERY_FETCH_CAPABILITY,
];

export function remoteSupportsQueryFetch(remoteProtocol) {
  if (!remoteProtocol || typeof remoteProtocol !== 'object') return false;
  const capabilities = Array.isArray(remoteProtocol.capabilities) ? remoteProtocol.capabilities : [];
  if (!capabilities.includes(CTOX_QUERY_FETCH_CAPABILITY)) return false;
  // V1.5 production hardening: even when the capability is advertised, the
  // remote may have demand-loading toggled off at runtime. Treat that as
  // V1-only — no remote query-fetch round-trips.
  const flag = remoteProtocol.v1_5?.queryDemandLoadingEnabled;
  if (flag === false) return false;
  return true;
}

export function getConnectionHandlerSimplePeer({ signalingServerUrl, config } = {}) {
  return {
    kind: 'ctox-native-webrtc',
    signalingServerUrl,
    config: config || {},
  };
}

export async function replicateWebRTC({
  collection,
  topic,
  connectionHandlerCreator,
  pull = { batchSize: 10 },
  push = { batchSize: 10 },
  retryTime = 5000,
  ctox = {},
} = {}) {
  if (!collection) throw new Error('replicateWebRTC requires collection');
  if (!topic) throw new Error('replicateWebRTC requires topic');
  const state = new CtoxWebRtcReplicationState({ collection, topic, pull, push, retryTime, ctox });
  await state.start(connectionHandlerCreator);
  return state;
}

class CtoxWebRtcReplicationState {
  constructor({ collection, topic, pull, push, retryTime, ctox }) {
    this.collection = collection;
    this.topic = topic;
    this.pull = pull;
    this.push = push;
    this.retryTime = retryTime;
    this.ctox = ctox;
    this.error$ = new CtoxSubject();
    this.active$ = new CtoxSubject(false);
    this.canceled$ = new CtoxSubject(false);
    this.peerStates$ = new CtoxSubject(new Map());
    this.transportStatus$ = new CtoxSubject({});
    this.peer = null;
    this.initialReplicationDeferred = createDeferred();
    this.initialReplication = this.initialReplicationDeferred.promise;
    this.cancelled = false;
    this.pullCheckpointsByPeer = new Map();
    this.pushCheckpointsByPeer = new Map();
    this.changeSubscription = null;
    this.periodicPullTimer = null;
    this.periodicPushTimer = null;
    this.pullInProgress = false;
    this.pushInProgress = false;
    this.peerOpenQueue = Promise.resolve();
    this.activeRemotePeerId = null;
    this.demandTransport = createDemandLoadingTransport({
      getPeerId: () => this.activeRemotePeerId,
    });
    this.demandLoaderActive = false;
  }

  async start(connectionHandlerCreator) {
    const schemaHashValue = await this.collection.schema.hash();
    const signalingUrl = connectionHandlerCreator?.signalingServerUrl;
    const iceServers = connectionHandlerCreator?.config?.iceServers || [];
    this.peer = createCtoxWebRtcNativePeer({
      signalingUrl,
      room: this.topic,
      clientId: browserInitiatorPeerId(this.topic),
      role: 'browser',
      capabilities: BROWSER_CAPABILITIES,
      iceServers,
      expectedNativePeerId: this.ctox?.expectedNativePeerId || '',
      protocolPayload: async () => {
        const checkpoint = await this.collection.storageCollection.replicationCheckpointStatus(schemaHashValue);
        return buildProtocolPayload({
          collectionName: this.collection.name,
          schemaVersion: this.collection.schema.version,
          schemaHash: schemaHashValue,
          schemaHashSource: schemaHashSource(this.collection.name),
          peerSessionId: `browser:${this.topic}`,
          peerGeneration: 1,
          checkpoint,
          role: 'browser',
          capabilities: BROWSER_CAPABILITIES,
        });
      },
      requestHandlers: {
        masterChangesSince: async ({ peerId, params }) => this.masterChangesSince(params, peerId),
        masterWrite: async ({ peerId, params }) => this.masterWrite(params, peerId),
        ...this.demandTransport.requestHandlers,
      },
    });
    this.demandTransport.attach(this.peer);
    this.peer.on('error', (event) => this.error$.next(event.detail || event));
    this.peer.on('transport-status', (event) => {
      this.transportStatus$.next(this.decorateTransportStatus(event.detail || event));
    });
    this.peer.on('peer-open', (event) => {
      const peerId = event.detail.peerId;
      this.peerOpenQueue = this.peerOpenQueue
        .then(() => this.handlePeerOpen(peerId))
        .catch((error) => this.error$.next(error));
    });
    this.peer.on('peer-close', (event) => {
      this.removePeer(event.detail?.peerId, event.detail?.reason || 'peer-close');
    });
    this.peer.on('peer-state', (event) => {
      const state = event.detail?.state || '';
      if (['closed', 'failed', 'disconnected'].includes(state)) {
        this.removePeer(event.detail?.peerId, `peer-${state}`);
      }
    });
    this.peer.on('master-change', () => {
      this.pullFromRemotePeers().catch((error) => this.error$.next(error));
    });
    this.peer.connect();
    this.changeSubscription = this.collection.observe(() => {
      this.pushToRemotePeers().catch((error) => this.error$.next(error));
    });
    const periodicPullMs = this.periodicPullIntervalMs();
    if (periodicPullMs > 0) {
      this.periodicPullTimer = setInterval(() => {
        this.pullFromRemotePeers().catch((error) => this.error$.next(error));
      }, periodicPullMs);
    }
    const periodicPushMs = this.periodicPushIntervalMs();
    if (periodicPushMs > 0) {
      this.periodicPushTimer = setInterval(() => {
        this.pushToRemotePeers().catch((error) => this.error$.next(error));
      }, periodicPushMs);
    }
  }

  async handlePeerOpen(peerId) {
    const localProtocol = await this.peer.protocolPayload(peerId);
    const remoteProtocol = await this.peer.request(peerId, 'ctoxProtocol', [
      localProtocol,
    ]);
    const normalizedRemoteProtocol = normalizeRemoteProtocol(remoteProtocol);
    try {
      assertCompatibleProtocol(localProtocol, normalizedRemoteProtocol, {
        requiredCapabilities: CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
      });
    } catch (error) {
      // Protocol/schema handshake is incompatible. Free the pooled RTC slot and
      // surface the real error instead of leaking the connection and letting
      // sync.js's 30s watchdog mislabel it as peer_connect_timeout. The
      // peer-open caller routes the rethrown error to error$, so we only reject
      // the initial-replication promise here to avoid a double emit.
      this.peer?.removeConnection?.(peerId, 'protocol-incompatible');
      this.rejectInitialReplication(error);
      throw error;
    }
    if (normalizedRemoteProtocol?.peerSession?.role !== 'ctox_instance') {
      this.peer?.removeConnection?.(peerId, 'non-native-peer-role');
      return;
    }
    this.ctox?.onPeerProtocol?.(normalizedRemoteProtocol);
    await this.peer.request(peerId, 'token', []);
    await this.awaitRemoteMasterReady(peerId);
    this.pruneReplacedNativePeers(peerId, normalizedRemoteProtocol);
    const queryFetchCapable = remoteSupportsQueryFetch(normalizedRemoteProtocol);
    this.activeRemotePeerId = peerId;
    if (queryFetchCapable && !this.demandLoaderActive) {
      try {
        await this.enableDemandLoading();
      } catch (error) {
        this.error$.next(error);
      }
    }
    this.ctox?.onPeerCapabilityNegotiated?.({
      peerId,
      queryFetchCapable,
      capabilities: normalizedRemoteProtocol?.capabilities || [],
      demandLoaderActive: this.demandLoaderActive,
    });
    const peerStates = new Map(this.peerStates$.getValue() || new Map());
    peerStates.set(peerId, {
      peerId,
      replicationState: this,
      remoteProtocol: normalizedRemoteProtocol,
      queryFetchCapable,
    });
    this.peerStates$.next(this.retainOnlyNativePeer(peerId, normalizedRemoteProtocol, peerStates));
    this.active$.next(true);
    try {
      this.initialReplication = this.pullFromRemotePeers().then(() => this.pushToRemotePeers());
      await this.initialReplication;
      this.resolveInitialReplication();
    } catch (error) {
      this.rejectInitialReplication(error);
      throw error;
    }
  }

  async awaitRemoteMasterReady(peerId) {
    try {
      await this.peer.waitForRequest?.(peerId, 'token', 2000);
    } catch {
      // Older or non-CTOX peers might not run the symmetric token request.
      // A short yield still avoids sending the first pull before a native
      // CTOX master subscribes to post-handshake replication requests.
    }
    await delay(100);
  }

  async pullFromRemotePeers() {
    if (this.pullInProgress) return;
    this.pullInProgress = true;
    const peerIds = this.openPeerIds();
    try {
      const results = await Promise.allSettled(peerIds.map((peerId) => this.pullFromPeer(peerId)));
      this.reportPeerResults(results, peerIds);
    } finally {
      this.pullInProgress = false;
    }
  }

  async pullFromPeer(peerId) {
    const batchSize = Number(this.pull?.batchSize || 10);
    let checkpoint = this.pullCheckpointsByPeer.get(peerId) || null;
    while (!this.cancelled) {
      const result = await this.requestMasterChangesSince(
        peerId,
        checkpoint,
        batchSize,
      );
      const documents = Array.isArray(result?.documents) ? result.documents : [];
      if (documents.length) {
        await this.collection.storageCollection.bulkWrite(documents, {
          replicationOrigin: this.replicationOriginForPeer(peerId),
        });
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pullCheckpointsByPeer.set(peerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }

  async requestMasterChangesSince(peerId, checkpoint, batchSize) {
    const timeoutMs = this.requestTimeoutMsFor('masterChangesSince');
    const maxAttempts = 2;
    let lastError = null;
    for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
      try {
        return await this.peer.request(
          peerId,
          'masterChangesSince',
          [checkpoint, batchSize],
          timeoutMs,
        );
      } catch (error) {
        lastError = error;
        if (
          attempt >= maxAttempts
          || this.cancelled
          || !this.isPeerOpen(peerId)
          || !this.isTransientMasterChangesSinceError(error)
        ) {
          throw error;
        }
        await delay(250);
      }
    }
    throw lastError;
  }

  async pushToRemotePeers() {
    if (!this.push) return;
    if (this.pushInProgress) return;
    this.pushInProgress = true;
    const peerIds = this.openPeerIds();
    try {
      const results = await Promise.allSettled(peerIds.map((peerId) => this.pushToPeer(peerId)));
      this.reportPeerResults(results, peerIds);
    } finally {
      this.pushInProgress = false;
    }
  }

  async pushToPeer(peerId) {
    if (!this.push || this.cancelled) return;
    const batchSize = Number(this.push?.batchSize || 10);
    let checkpoint = this.pushCheckpointsByPeer.get(peerId) || null;
    while (!this.cancelled) {
      const result = await this.collection.storageCollection.getChangedDocumentsSince(
        checkpoint,
        batchSize,
        this.changedDocumentReadOptionsForPeer(peerId),
      );
      const documents = Array.isArray(result?.documents) ? result.documents : [];
      if (!documents.length) {
        checkpoint = result?.checkpoint || checkpoint;
        this.pushCheckpointsByPeer.set(peerId, checkpoint);
        break;
      }
      const rows = documents.map((doc) => ({
        newDocumentState: doc,
        assumedMasterState: null,
      }));
      await this.peer.request(peerId, 'masterWrite', [rows], this.requestTimeoutMsFor('masterWrite'));
      checkpoint = result?.checkpoint || checkpoint;
      this.pushCheckpointsByPeer.set(peerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }

  async masterChangesSince(params, peerId = '') {
    const checkpoint = params?.[0] || null;
    const batchSize = Number(params?.[1] || this.pull?.batchSize || 10);
    return this.collection.storageCollection.getChangedDocumentsSince(
      checkpoint,
      batchSize,
      this.changedDocumentReadOptionsForPeer(peerId),
    );
  }

  async masterWrite(params, peerId = '') {
    const rows = Array.isArray(params?.[0]) ? params[0] : [];
    const docs = rows.map((row) => row?.newDocumentState || row?.document || row).filter(Boolean);
    if (docs.length) {
      await this.collection.storageCollection.bulkWrite(docs, {
        replicationOrigin: this.replicationOriginForPeer(peerId),
      });
    }
    return [];
  }

  awaitInitialReplication() {
    return this.initialReplication;
  }

  awaitInSync() {
    return Promise.resolve()
      .then(() => this.awaitInitialReplication())
      .then(() => this.pullFromRemotePeers())
      .then(() => this.pushToRemotePeers());
  }

  getTransportStatus() {
    return this.decorateTransportStatus(this.peer?.getTransportStatus?.() || this.transportStatus$.getValue?.() || {});
  }

  async cancel() {
    this.cancelled = true;
    this.rejectInitialReplication(new Error('WebRTC replication cancelled'));
    this.active$.next(false);
    this.canceled$.next(true);
    this.changeSubscription?.unsubscribe?.();
    if (this.periodicPullTimer) {
      clearInterval(this.periodicPullTimer);
      this.periodicPullTimer = null;
    }
    if (this.periodicPushTimer) {
      clearInterval(this.periodicPushTimer);
      this.periodicPushTimer = null;
    }
    try { this.demandLoader?.abortAllInFlight?.('replication-cancel'); } catch {}
    try { this.demandSidecar?.stopEvictionScheduler?.(); } catch {}
    try { await this.demandSidecar?.close?.(); } catch {}
    this.peer?.close?.();
  }

  /// V1.5 production wiring: build the sidecar + query demand loader and
  /// attach them to the underlying collection so that `find().exec()` and
  /// observable queries flow through the on-demand pipeline. Idempotent.
  async enableDemandLoading({
    databaseName,
    indexedDbAvailable = typeof globalThis.indexedDB === 'object' && globalThis.indexedDB,
  } = {}) {
    if (this.demandLoaderActive) return this.demandLoader;
    const dbName = databaseName || `ctox_business_os_v1_5_meta_${this.collection.name}`;
    const backend = indexedDbAvailable
      ? createIndexedDbMetaBackend({ databaseName: dbName })
      : createMemoryMetaBackend();
    const primaryDelete = async (collection, id) => {
      if (collection !== this.collection.name) return;
      if (typeof this.collection.storageCollection.hardDeleteByIds === 'function') {
        await this.collection.storageCollection.hardDeleteByIds([id]);
      }
    };
    this.demandSidecar = new QueryMetaStorage(backend, {
      databaseName: dbName,
      primaryDelete,
    });
    // Run cache eviction periodically in production. 30 s is conservative
    // for a peer-driven cache that grows from real-time replication.
    try { this.demandSidecar.startEvictionScheduler({ intervalMs: 30_000 }); } catch {}

    this.demandLoader = createQueryDemandLoader({
      storageCollection: this.collection.storageCollection,
      sidecar: this.demandSidecar,
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema?.version || 0,
      requestQueryFetch: (envelope) => this.demandTransport.requestQueryFetch(envelope),
      requestCancel: ({ requestId }) => this.demandTransport.requestQueryCancel({ requestId }),
      status: null,
    });
    if (typeof this.collection.setDemandLoader === 'function') {
      this.collection.setDemandLoader(this.demandLoader);
    }

    this.demandFileLoader = createFileDemandLoader({
      collectionName: this.collection.name,
      storageCollection: this.collection.storageCollection,
      sidecarBackend: backend,
      requestFileFetch: ({ requestId, fileId, range, knownSequences }) =>
        this.demandTransport.requestFileFetch({
          requestId,
          fileId,
          range,
          knownSequences,
          collectionName: this.collection.name,
        }),
    });

    this.demandLoaderActive = true;
    return this.demandLoader;
  }

  resolveInitialReplication() {
    this.initialReplicationDeferred?.resolve?.(true);
  }

  rejectInitialReplication(error) {
    this.initialReplicationDeferred?.reject?.(error);
  }

  removePeer(peerId, reason = 'closed') {
    if (!peerId) return;
    const peerStates = new Map(this.peerStates$.getValue() || new Map());
    if (!peerStates.has(peerId)) return;
    peerStates.delete(peerId);
    this.pullCheckpointsByPeer.delete(peerId);
    this.pushCheckpointsByPeer.delete(peerId);
    this.peerStates$.next(peerStates);
    if (!peerStates.size) this.active$.next(false);
    this.ctox?.onPeerClose?.({ peerId, reason });
  }

  pruneReplacedNativePeers(activePeerId, remoteProtocol) {
    if (remoteProtocol?.peerSession?.role !== 'ctox_instance') return;
    const peerStates = new Map(this.peerStates$.getValue() || new Map());
    let changed = false;
    for (const [peerId, state] of peerStates.entries()) {
      if (peerId === activePeerId) continue;
      if (state?.remoteProtocol?.peerSession?.role !== 'ctox_instance') continue;
      peerStates.delete(peerId);
      this.pullCheckpointsByPeer.delete(peerId);
      this.pushCheckpointsByPeer.delete(peerId);
      this.peer?.removeConnection?.(peerId, 'native-peer-replaced');
      changed = true;
    }
    if (changed) this.peerStates$.next(peerStates);
  }

  retainOnlyNativePeer(activePeerId, remoteProtocol, peerStates) {
    if (remoteProtocol?.peerSession?.role !== 'ctox_instance') return peerStates;
    const activeState = peerStates.get(activePeerId);
    const nextPeerStates = new Map([[activePeerId, activeState]]);
    for (const peerId of peerStates.keys()) {
      if (peerId === activePeerId) continue;
      this.pullCheckpointsByPeer.delete(peerId);
      this.pushCheckpointsByPeer.delete(peerId);
      this.peer?.removeConnection?.(peerId, 'native-peer-retained-singleton');
    }
    return nextPeerStates;
  }

  remoteProtocolForPeer(peerId) {
    return (this.peerStates$.getValue() || new Map()).get(peerId)?.remoteProtocol || null;
  }

  replicationOriginForPeer(peerId) {
    const remoteProtocol = this.remoteProtocolForPeer(peerId);
    const peerSession = remoteProtocol?.peerSession || {};
    const role = typeof peerSession.role === 'string' ? peerSession.role : '';
    if (!role) return null;
    return {
      role,
      peerId,
      sessionId: typeof peerSession.sessionId === 'string' ? peerSession.sessionId : '',
      collection: this.collection.name,
    };
  }

  changedDocumentReadOptionsForPeer(peerId) {
    const role = this.replicationOriginForPeer(peerId)?.role || '';
    return role ? { excludeReplicationOriginRole: role } : {};
  }

  requestTimeoutMsFor(method) {
    if (this.collection.name === 'desktop_file_chunks') {
      return method === 'masterChangesSince' ? 45000 : 30000;
    }
    return 15000;
  }

  periodicPullIntervalMs() {
    return 0;
  }

  periodicPushIntervalMs() {
    return ['business_commands', 'ctox_queue_tasks'].includes(this.collection.name) ? 1000 : 0;
  }

  openPeerIds() {
    const peerStates = this.peerStates$.getValue() || new Map();
    const open = [];
    for (const peerId of peerStates.keys()) {
      if (this.isPeerOpen(peerId)) {
        open.push(peerId);
      } else {
        this.removePeer(peerId, 'peer-not-open');
      }
    }
    return open;
  }

  isPeerOpen(peerId) {
    const connection = this.peer?.connections?.get?.(peerId);
    if (!connection) return false;
    const channelState = connection.channel?.readyState || '';
    const pcState = connection.peer?.connectionState || '';
    return channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(pcState);
  }

  isTransientMasterChangesSinceError(error) {
    const message = typeof error?.message === 'string' ? error.message : String(error || '');
    return message.includes('Timed out waiting for WebRTC response masterChangesSince');
  }

  decorateTransportStatus(status = {}) {
    return {
      ...status,
      collection: this.collection.name,
      topic: this.topic,
      activePeerCount: (this.peerStates$.getValue?.() || new Map()).size,
      pullInProgress: this.pullInProgress,
      pushInProgress: this.pushInProgress,
      updatedAtMs: Date.now(),
    };
  }

  reportPeerResults(results, peerIds) {
    results.forEach((result, index) => {
      if (result.status !== 'rejected') return;
      const peerId = peerIds[index];
      if (this.shouldRetainPeerAfterError(peerId, result.reason)) {
        this.error$.next(result.reason);
        return;
      }
      this.removePeer(peerId, result.reason?.message || 'request-failed');
      this.error$.next(result.reason);
    });
  }

  shouldRetainPeerAfterError(peerId, error) {
    return this.isPeerOpen(peerId) && this.isTransientMasterChangesSinceError(error);
  }
}

function browserInitiatorPeerId(topic) {
  return `000-browser-${hashString(String(topic || 'ctox'))}`;
}

function hashString(value) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function createDeferred() {
  let settled = false;
  let resolve;
  let reject;
  const promise = new Promise((promiseResolve, promiseReject) => {
    resolve = (value) => {
      if (settled) return;
      settled = true;
      promiseResolve(value);
    };
    reject = (error) => {
      if (settled) return;
      settled = true;
      promiseReject(error);
    };
  });
  return { promise, resolve, reject };
}

function normalizeRemoteProtocol(payload) {
  if (!payload || typeof payload !== 'object') return payload;
  return {
    ...payload,
    checkpoint: payload.checkpoint || payload.collection?.checkpoint || null,
  };
}
