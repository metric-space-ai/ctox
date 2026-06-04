import { CtoxSubject } from './observable.mjs';
import { createCtoxWebRtcNativePeer } from './webrtc-native.mjs';
import {
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  assertCompatibleProtocol,
  assertCollectionSchemasCompatible,
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
import { getActiveCollectionRegistry } from './active-collections.mjs';

// Phase 2: the wire method the browser uses to tell the native peer which
// collections are foreground (subscription-driven). Must match the native
// `ACTIVE_COLLECTIONS_METHOD` constant in connection_handler_rs.rs.
export const ACTIVE_COLLECTIONS_METHOD = 'rxdb.activeCollections';

// Phase 4: per-collection demand-cache memory budget. The query-meta sidecar
// evicts LRU document-access entries (and deletes them from the primary store)
// once the working set exceeds this. Without a budget the default is 0 and
// eviction never runs, so the cache grows unbounded under real-time
// replication. 128 MiB is a sane ceiling for a peer-driven cache.
export const DEFAULT_QUERY_META_BUDGET_BYTES = 128 * 1024 * 1024;

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

// ---------------------------------------------------------------------------
// Phase 3 (single multiplexed stream): one CtoxWebRtcNativePeer carries EVERY
// collection in a sync room. `replicateWebRTC` no longer opens its own peer per
// collection; instead it joins a process-wide `SharedRoomPeer` keyed by
// (signalingUrl, room=sync_room). The shared peer owns the single signaling
// socket + RTCPeerConnection + DataChannel and demultiplexes inbound frames by
// their `collection` field. Each collection's replication state registers its
// pull/push handlers and master-change sink into the shared peer.
// ---------------------------------------------------------------------------

const SHARED_ROOM_PEERS = new Map(); // key -> SharedRoomPeer

function sharedRoomPeerKey(signalingUrl, room) {
  return `${String(signalingUrl || '')}::${String(room || '')}`;
}

class SharedRoomPeer {
  constructor({ key, signalingUrl, room, iceServers, expectedNativePeerId }) {
    this.key = key;
    this.signalingUrl = signalingUrl;
    this.room = room;
    this.iceServers = iceServers;
    this.expectedNativePeerId = expectedNativePeerId;
    // collection name -> registration { collection, state }
    this.collections = new Map();
    this.refCount = 0;
    this.peer = null;
    // The shared demand-loading transport. Chunks route by requestId globally,
    // so one transport serves every collection on this connection.
    this.demandTransport = createDemandLoadingTransport({
      getPeerId: () => this.activeRemotePeerId,
    });
    this.activeRemotePeerId = null;
    this.started = false;
    this.peerOpenQueue = Promise.resolve();
    // Negotiated remote protocol from the room-level handshake, retained so a
    // collection that registers AFTER the handshake can immediately catch up.
    this.negotiated = null; // { peerId, remoteProtocol, queryFetchCapable }
    // Phase 3 schema-validation hardening: collections whose per-collection
    // schema hash mismatched the remote at handshake time. They stay quiesced
    // (no pull/push) until reconciled instead of disabling validation for the
    // whole room.
    this.schemaMismatchCollections = new Set();
    // Phase 2: subscription-driven active-collection priority. The shared peer
    // forwards the RxDB layer's active set (derived from real reactive
    // subscriptions, NOT app.js) to the native peer over `rxdb.activeCollections`
    // so the native send queue prioritizes the foreground collection.
    this.activeRegistry = getActiveCollectionRegistry();
    this.activeRegistryUnsub = null;
    this.lastActiveCollectionsSent = null;
  }

  representativeCollection() {
    const first = this.collections.keys().next();
    return first.done ? null : this.collections.get(first.value);
  }

  register(collection, registration) {
    this.collections.set(collection, registration);
    this.refCount += 1;
    // Catch-up: if the room handshake already completed (this collection
    // joined after another opened the peer), drive its initial pull/push now
    // instead of waiting for a peer-open that already fired.
    if (this.negotiated && this.isPeerOpen(this.negotiated.peerId)) {
      const { peerId, remoteProtocol, queryFetchCapable } = this.negotiated;
      Promise.resolve()
        .then(async () => {
          // Phase 3 schema-validation hardening: a collection that joins AFTER
          // the room handshake was not covered by the per-collection schema
          // check in `handlePeerOpen`. Validate just this collection's hash
          // against the negotiated remote `collectionSchemas` now; on mismatch
          // surface its error and skip its pull/push (do not quiesce the room).
          const localSchemas = await this.collectCollectionSchemas();
          const only = { [collection]: localSchemas[collection] };
          const mismatches = assertCollectionSchemasCompatible(only, remoteProtocol);
          if (mismatches.has(collection)) {
            this.schemaMismatchCollections.add(collection);
            registration.state.emitError(mismatches.get(collection));
            return;
          }
          await registration.state.onPeerReady(peerId, remoteProtocol, queryFetchCapable);
        })
        .catch((error) => registration.state.emitError(error));
    }
  }

  unregister(collection) {
    this.collections.delete(collection);
    this.refCount = Math.max(0, this.refCount - 1);
    if (this.refCount === 0) {
      SHARED_ROOM_PEERS.delete(this.key);
      try { this.peer?.close?.(); } catch {}
      this.peer = null;
      this.started = false;
      // Phase 2: stop forwarding active-collection changes once the room peer
      // is torn down.
      if (this.activeRegistryUnsub) {
        try { this.activeRegistryUnsub(); } catch {}
        this.activeRegistryUnsub = null;
      }
    }
  }

  ensurePeer() {
    if (this.peer) return this.peer;
    this.peer = createCtoxWebRtcNativePeer({
      signalingUrl: this.signalingUrl,
      // Phase 3: the room is the bare sync_room — NOT a per-collection topic.
      room: this.room,
      clientId: browserInitiatorPeerId(this.room),
      role: 'browser',
      capabilities: BROWSER_CAPABILITIES,
      iceServers: this.iceServers,
      expectedNativePeerId: this.expectedNativePeerId || '',
      protocolPayload: async ({ collection } = {}) => this.buildProtocolPayload(collection),
      requestHandlers: {
        masterChangesSince: async ({ params, peerId, collection }) =>
          this.routeMasterChangesSince(collection, params, peerId),
        masterWrite: async ({ params, peerId, collection }) =>
          this.routeMasterWrite(collection, params, peerId),
        ...this.demandTransport.requestHandlers,
      },
    });
    this.demandTransport.attach(this.peer);
    this.peer.on('error', (event) => this.fanout('error', event.detail || event));
    this.peer.on('transport-status', (event) => this.fanout('transport-status', event.detail || event));
    this.peer.on('peer-open', (event) => {
      const peerId = event.detail.peerId;
      this.peerOpenQueue = this.peerOpenQueue
        .then(() => this.handlePeerOpen(peerId))
        .catch((error) => this.fanout('handshake-error', error));
    });
    this.peer.on('peer-close', (event) => {
      // A closed peer invalidates the negotiated handshake; a fresh peer-open
      // will renegotiate and re-drive every collection's catch-up.
      if (this.negotiated && this.negotiated.peerId === event.detail?.peerId) {
        this.negotiated = null;
      }
      if (this.activeRemotePeerId === event.detail?.peerId) {
        this.activeRemotePeerId = null;
      }
      this.fanout('peer-close', event.detail);
    });
    this.peer.on('peer-state', (event) => this.fanout('peer-state', event.detail));
    this.peer.on('master-change', (event) => {
      // Fan a master-change to ONLY the collection it belongs to (when the
      // push is collection-qualified); otherwise to every collection (V1).
      const collection = event.detail?.collection || event.collection || null;
      this.fanoutMasterChange(collection);
    });
    // Phase 2: forward the RxDB layer's active-collection set to the native
    // peer whenever it changes. The listener fires immediately with the current
    // set and on every subsequent change (debounced inside the registry).
    if (!this.activeRegistryUnsub) {
      this.activeRegistryUnsub = this.activeRegistry.onChange((names) => {
        this.sendActiveCollections(names);
      });
    }
    return this.peer;
  }

  // Phase 2: send `rxdb.activeCollections` (fire-and-forget) to the active
  // native peer. No-op until a peer is open. Resent on (re)handshake because
  // the native peer drops its per-peer active set on disconnect.
  sendActiveCollections(names) {
    const list = Array.isArray(names) ? names : this.activeRegistry.activeCollectionsList();
    const peerId = this.activeRemotePeerId;
    if (!peerId || !this.peer) return;
    const key = list.join(' ');
    this.lastActiveCollectionsSent = key;
    try {
      this.peer.send(peerId, {
        id: `active-collections|${Date.now()}`,
        method: ACTIVE_COLLECTIONS_METHOD,
        params: [list],
      });
    } catch {
      // Best-effort transport hint; priority falls back to Normal if it fails.
    }
  }

  start() {
    this.ensurePeer();
    if (this.started) return;
    this.started = true;
    this.peer.connect();
  }

  fanout(eventName, detail) {
    for (const registration of this.collections.values()) {
      try { registration.state?.onSharedEvent?.(eventName, detail); } catch {}
    }
  }

  fanoutMasterChange(collection) {
    if (collection) {
      const registration = this.collections.get(collection);
      registration?.state?.onMasterChange?.();
      return;
    }
    for (const registration of this.collections.values()) {
      try { registration.state?.onMasterChange?.(); } catch {}
    }
  }

  async buildProtocolPayload(collection) {
    // Resolve the protocol payload for the collection the remote asked about
    // (multiplex), or the representative when none was tagged.
    const registration = (collection && this.collections.get(collection))
      || this.representativeCollection();
    if (!registration) {
      return buildProtocolPayload({
        role: 'browser',
        peerSessionId: `browser:${this.room}`,
        peerGeneration: 1,
        capabilities: BROWSER_CAPABILITIES,
      });
    }
    const payload = await registration.state.buildProtocolPayload();
    // Phase 3 schema-validation hardening: under multiplex the handshake runs
    // ONCE off the representative collection, so attach the per-collection
    // schema-hash map for EVERY collection on this connection. The remote
    // validates each entry individually instead of skipping schema validation.
    // Single-collection rooms omit the map (payload stays legacy-identical).
    if (this.collections.size > 1) {
      payload.collectionSchemas = await this.collectCollectionSchemas();
    }
    return payload;
  }

  // Build `{ collectionName -> { schemaVersion, schemaHash, schemaHashSource } }`
  // across every registered collection on this shared connection.
  async collectCollectionSchemas() {
    const map = {};
    for (const [name, registration] of this.collections.entries()) {
      const state = registration.state;
      if (!state) continue;
      let hash = state.schemaHashValue;
      if (!hash) {
        try { hash = await state.collection.schema.hash(); } catch { hash = null; }
      }
      map[name] = {
        schemaVersion: state.collection?.schema?.version ?? null,
        schemaHash: hash || null,
        schemaHashSource: schemaHashSource(name),
      };
    }
    return map;
  }

  async routeMasterChangesSince(collection, params, peerId) {
    const registration = collection && this.collections.get(collection);
    if (!registration) {
      // Unknown collection — return empty changes rather than leaking another
      // collection's documents.
      return { documents: [], checkpoint: params?.[0] || null };
    }
    return registration.state.masterChangesSince(params, peerId);
  }

  async routeMasterWrite(collection, params, peerId) {
    const registration = collection && this.collections.get(collection);
    if (!registration) return [];
    return registration.state.masterWrite(params, peerId);
  }

  async handlePeerOpen(peerId) {
    // The room-level handshake runs ONCE (off the representative collection).
    // Every registered collection observes the same open peer.
    const representative = this.representativeCollection();
    if (!representative) return;
    const localProtocol = await this.peer.protocolPayload(peerId, [], representative.collection);
    const remoteProtocol = await this.peer.request(
      peerId,
      'ctoxProtocol',
      [localProtocol],
      15000,
      representative.collection,
    );
    const normalizedRemoteProtocol = normalizeRemoteProtocol(remoteProtocol);
    const multiplexed = this.collections.size > 1;
    try {
      assertCompatibleProtocol(localProtocol, normalizedRemoteProtocol, {
        requiredCapabilities: CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
        // Under multiplex the representative collection in the room handshake
        // may differ from the remote's representative, so the SINGLE-collection
        // name/hash check on `localProtocol.collection` is meaningless here. We
        // still enforce protocol + required capabilities, and validate every
        // collection's schema individually below via `collectionSchemas`.
        validateSchema: !multiplexed,
      });
    } catch (error) {
      this.peer?.removeConnection?.(peerId, 'protocol-incompatible');
      this.fanout('handshake-error', error);
      throw error;
    }
    if (normalizedRemoteProtocol?.peerSession?.role !== 'ctox_instance') {
      this.peer?.removeConnection?.(peerId, 'non-native-peer-role');
      return;
    }
    // Phase 3 schema-validation hardening: validate EACH collection's schema
    // hash individually under multiplex. On mismatch, surface the
    // schemaHashMismatch error for THAT collection and skip just it (do NOT
    // tear down the connection or disable validation for the whole room).
    this.schemaMismatchCollections = new Set();
    if (multiplexed) {
      const localSchemas = await this.collectCollectionSchemas();
      const mismatches = assertCollectionSchemasCompatible(localSchemas, normalizedRemoteProtocol);
      for (const [name, error] of mismatches.entries()) {
        this.schemaMismatchCollections.add(name);
        const registration = this.collections.get(name);
        registration?.state?.emitError(error);
      }
    }
    await this.peer.request(peerId, 'token', [], 15000, representative.collection);
    await this.awaitRemoteMasterReady(peerId);
    const queryFetchCapable = remoteSupportsQueryFetch(normalizedRemoteProtocol);
    this.activeRemotePeerId = peerId;
    // Phase 2: the native peer cleared its per-peer active set on the prior
    // disconnect — re-send the current foreground set now that the handshake
    // completed so priority is correct from the first frame.
    this.sendActiveCollections();
    // Retain the negotiated handshake so collections that register later catch
    // up immediately (see `register`).
    this.negotiated = { peerId, remoteProtocol: normalizedRemoteProtocol, queryFetchCapable };
    // Notify every collection that the shared peer is open + protocol-ready, so
    // each runs its own initial pull/push and (optionally) demand-loading.
    // Skip collections whose schema mismatched — they stay quiesced (no rows
    // exchanged) until the schema is reconciled, while every other collection
    // syncs normally.
    for (const [name, registration] of this.collections.entries()) {
      if (this.schemaMismatchCollections.has(name)) continue;
      try {
        await registration.state.onPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable);
      } catch (error) {
        registration.state.emitError(error);
      }
    }
  }

  isPeerOpen(peerId) {
    const connection = this.peer?.connections?.get?.(peerId);
    if (!connection) return false;
    const channelState = connection.channel?.readyState || '';
    const pcState = connection.peer?.connectionState || '';
    return channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(pcState);
  }

  async awaitRemoteMasterReady(peerId) {
    try {
      await this.peer.waitForRequest?.(peerId, 'token', 2000);
    } catch {
      // Older or non-CTOX peers might not run the symmetric token request.
    }
    await delay(100);
  }

  getTransportStatus() {
    return this.peer?.getTransportStatus?.() || {};
  }
}

function getOrCreateSharedRoomPeer({ signalingUrl, room, iceServers, expectedNativePeerId }) {
  const key = sharedRoomPeerKey(signalingUrl, room);
  let shared = SHARED_ROOM_PEERS.get(key);
  if (!shared) {
    shared = new SharedRoomPeer({ key, signalingUrl, room, iceServers, expectedNativePeerId });
    SHARED_ROOM_PEERS.set(key, shared);
  }
  return shared;
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
    // Phase 3: `topic` is the bare sync room shared by every collection.
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
    this.shared = null;
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
    this.activeRemotePeerId = null;
    this.demandLoaderActive = false;
    this.schemaHashValue = null;
  }

  get peer() {
    return this.shared?.peer || null;
  }

  async start(connectionHandlerCreator) {
    this.schemaHashValue = await this.collection.schema.hash();
    const signalingUrl = connectionHandlerCreator?.signalingServerUrl;
    const iceServers = connectionHandlerCreator?.config?.iceServers || [];
    this.shared = getOrCreateSharedRoomPeer({
      signalingUrl,
      room: this.topic,
      iceServers,
      expectedNativePeerId: this.ctox?.expectedNativePeerId || '',
    });
    this.shared.register(this.collection.name, {
      collection: this.collection.name,
      state: this,
    });
    this.shared.start();
    this.changeSubscription = this.collection.observe(() => {
      this.pushToRemotePeers().catch((error) => this.error$.next(error));
    });
    const periodicPushMs = this.periodicPushIntervalMs();
    if (periodicPushMs > 0) {
      this.periodicPushTimer = setInterval(() => {
        this.pushToRemotePeers().catch((error) => this.error$.next(error));
      }, periodicPushMs);
    }
  }

  // ----- shared peer event sinks (called by SharedRoomPeer) ---------------

  onSharedEvent(eventName, detail) {
    if (this.cancelled) return;
    if (eventName === 'error') {
      this.error$.next(detail?.detail || detail);
      return;
    }
    if (eventName === 'handshake-error') {
      this.rejectInitialReplication(detail);
      this.error$.next(detail);
      return;
    }
    if (eventName === 'transport-status') {
      this.transportStatus$.next(this.decorateTransportStatus(detail || {}));
      return;
    }
    if (eventName === 'peer-close') {
      this.removePeer(detail?.peerId, detail?.reason || 'peer-close');
      return;
    }
    if (eventName === 'peer-state') {
      const stateName = detail?.state || '';
      if (['closed', 'failed', 'disconnected'].includes(stateName)) {
        this.removePeer(detail?.peerId, `peer-${stateName}`);
      }
    }
  }

  onMasterChange() {
    if (this.cancelled) return;
    this.pullFromRemotePeers().catch((error) => this.error$.next(error));
  }

  emitError(error) {
    this.error$.next(error);
  }

  async buildProtocolPayload() {
    const checkpoint = await this.collection.storageCollection.replicationCheckpointStatus(this.schemaHashValue);
    return buildProtocolPayload({
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema.version,
      schemaHash: this.schemaHashValue,
      schemaHashSource: schemaHashSource(this.collection.name),
      peerSessionId: `browser:${this.topic}`,
      peerGeneration: 1,
      checkpoint,
      role: 'browser',
      capabilities: BROWSER_CAPABILITIES,
    });
  }

  async onPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable) {
    if (this.cancelled) return;
    this.ctox?.onPeerProtocol?.(normalizedRemoteProtocol);
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
    this.peerStates$.next(peerStates);
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

  // ----- pull / push (collection-tagged over the shared peer) -------------

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
      const result = await this.requestMasterChangesSince(peerId, checkpoint, batchSize);
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
          this.collection.name,
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
      await this.peer.request(
        peerId,
        'masterWrite',
        [rows],
        this.requestTimeoutMsFor('masterWrite'),
        this.collection.name,
      );
      checkpoint = result?.checkpoint || checkpoint;
      this.pushCheckpointsByPeer.set(peerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }

  // ----- master handler (when CTOX picks the browser as fork's master) ----

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
    return this.decorateTransportStatus(this.shared?.getTransportStatus?.() || this.transportStatus$.getValue?.() || {});
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
    // Drop this collection from the shared peer. The peer (and its single
    // connection) is closed only when the last collection unregisters.
    this.shared?.unregister?.(this.collection.name);
    this.shared = null;
  }

  /// V1.5 production wiring: build the sidecar + query demand loader and attach
  /// them to the underlying collection so that `find().exec()` and observable
  /// queries flow through the on-demand pipeline. Idempotent. Uses the SHARED
  /// peer's demand transport (chunks route by requestId globally).
  async enableDemandLoading({
    databaseName,
    indexedDbAvailable = typeof globalThis.indexedDB === 'object' && globalThis.indexedDB,
  } = {}) {
    if (this.demandLoaderActive) return this.demandLoader;
    const demandTransport = this.shared?.demandTransport;
    if (!demandTransport) return null;
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
    // Phase 4: set a memory budget so eviction ACTUALLY RUNS. Without this the
    // budget defaults to 0 and `evictDocuments` short-circuits (the cache grows
    // unbounded from real-time replication). 128 MiB is a sane per-collection
    // ceiling for a peer-driven cache; LRU document-access entries are evicted
    // (and removed from the primary store via `primaryDelete`) once the working
    // set exceeds it.
    try { await this.demandSidecar.setBudgetBytes(DEFAULT_QUERY_META_BUDGET_BYTES); } catch {}
    // Run cache eviction periodically in production. 30 s is conservative
    // for a peer-driven cache that grows from real-time replication.
    try { this.demandSidecar.startEvictionScheduler({ intervalMs: 30_000 }); } catch {}

    this.demandLoader = createQueryDemandLoader({
      storageCollection: this.collection.storageCollection,
      sidecar: this.demandSidecar,
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema?.version || 0,
      requestQueryFetch: (envelope) => demandTransport.requestQueryFetch(envelope),
      requestCancel: ({ requestId }) => demandTransport.requestQueryCancel({ requestId }),
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
        demandTransport.requestFileFetch({
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

const BROWSER_PEER_SESSION_ID = createBrowserPeerSessionId();

function browserInitiatorPeerId(topic) {
  const origin = browserPeerOriginId();
  const stableScope = `${String(topic || 'ctox')}|${origin}|${BROWSER_PEER_SESSION_ID}`;
  return `000-browser-${hashString(stableScope)}`;
}

function browserPeerOriginId() {
  try {
    return String(globalThis.location?.origin || globalThis.location?.host || 'local');
  } catch {
    return 'local';
  }
}

function createBrowserPeerSessionId() {
  try {
    const bytes = new Uint8Array(8);
    globalThis.crypto?.getRandomValues?.(bytes);
    if (bytes.some(Boolean)) {
      return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
    }
  } catch {}
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
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
