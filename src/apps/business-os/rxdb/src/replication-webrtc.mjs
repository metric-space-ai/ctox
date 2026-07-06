// =============================================================================
// AGENT GUARDRAILS — ctox-rxdb data plane (read docs/ctox-rxdb.md first)
// =============================================================================
// This file is part of CTOX Sync Engine, the WebRTC-ONLY data plane between Business OS
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

// Phase-3 multiplex: ONE SharedRoomPeer carries EVERY collection of a sync
// room. Recovery semantics here (push re-run flag, pull/push retry timers,
// checkpoint retention keyed by storage epoch + native session) are pinned
// by tests/replication-recovery-smoke.mjs.
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
import {
  CTOX_PRESENCE_CAPABILITY,
  CTOX_PRESENCE_RPC,
  CTOX_QUERY_FETCH_CAPABILITY,
} from './protocol-contract.generated.mjs';
import { createDemandLoadingTransport } from './demand-loading-transport.mjs';
import { createQueryDemandLoader } from './query-demand-loader.mjs';
import { createFileDemandLoader } from './file-demand-loader.mjs';
import { QueryMetaStorage } from './query-meta-storage.mjs';
import { createIndexedDbMetaBackend } from './query-meta-backend-indexeddb.mjs';
import { createMemoryMetaBackend } from './query-meta-backend-memory.mjs';
import { getActiveCollectionRegistry } from './active-collections.mjs';
import { getPresenceRegistry } from './presence.mjs';
import { threeWayMergeDocuments } from './conflict-merge.mjs';
import { createV1_5StatusState, snapshotV1_5Status } from './v1_5_status.mjs';

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
const LOCAL_WRITE_PUSH_DEBOUNCE_MS = 50;

const BROWSER_CAPABILITIES = [
  'ctox-rxdb-browser-v1',
  'ctox-file-chunks-v1',
  'ctox-schema-hash-v1',
  'ctox-peer-session-v1',
  'ctox-checkpoint-epoch-v1',
  CTOX_QUERY_FETCH_CAPABILITY,
  CTOX_PRESENCE_CAPABILITY,
];

// Presence is optional on the wire: a native peer that predates
// ctox-presence-v1 must never receive `rxdb.presence.update` frames (it would
// route them into the replication message stream as an unknown method).
export function remoteSupportsPresence(remoteProtocol) {
  if (!remoteProtocol || typeof remoteProtocol !== 'object') return false;
  const capabilities = Array.isArray(remoteProtocol.capabilities) ? remoteProtocol.capabilities : [];
  return capabilities.includes(CTOX_PRESENCE_CAPABILITY);
}

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
const SHARED_HANDSHAKE_TIMEOUT_MS = 60000;
const SHARED_TOKEN_TIMEOUT_MS = 30000;
const SHARED_PEER_OPEN_WAIT_MS = 60000;
const SHARED_PROTOCOL_COLLECTION_CONCURRENCY = 8;
const VOLATILE_SIGNALING_QUERY_PARAMS = new Set([
  'client',
  'role',
  'peer_role',
  'instance_id',
  'instance',
  'protocol',
  'cap',
  'capability',
  'capabilities',
  'token',
  'token_iat',
  'token_exp',
]);

function sharedRoomPeerKey(signalingUrl, room) {
  return `${stableSignalingUrlKey(signalingUrl)}::${String(room || '')}`;
}

function stableSignalingUrlKey(signalingUrl) {
  const raw = String(signalingUrl || '');
  try {
    const url = new URL(raw, 'ws://local');
    for (const key of [...url.searchParams.keys()]) {
      if (VOLATILE_SIGNALING_QUERY_PARAMS.has(key)) {
        url.searchParams.delete(key);
      }
    }
    url.hash = '';
    return url.toString();
  } catch {
    return raw;
  }
}

export const replicationWebRtcTestInternals = Object.freeze({
  changeEventHasOnlyReplicationOriginWrites,
  sharedRoomPeerKey,
  stableSignalingUrlKey,
  shouldAttachQueryDemandLoader,
  // Lazy accessor (class is declared below): lets the activation-catch-up
  // smoke drive the real SharedRoomPeer registry wiring without a network.
  getSharedRoomPeerClass: () => SharedRoomPeer,
});

function isTransientSharedPeerError(error) {
  const message = String(error?.message || error || '');
  return message.includes(' is not open')
    || message.includes('WebRTC peer')
    || message.includes('Peer closed')
    || message.includes('peer closed')
    || message.includes('channel-close')
    || message.includes('Timed out waiting for WebRTC response ctoxProtocol');
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
    this.collectionCatchUps = new Map();
    this.negotiationCatchUp = null;
    // Phase 2: subscription-driven active-collection priority. The shared peer
    // forwards the RxDB layer's active set (derived from real reactive
    // subscriptions, NOT app.js) to the native peer over `rxdb.activeCollections`
    // so the native send queue prioritizes the foreground collection.
    this.activeRegistry = getActiveCollectionRegistry();
    this.activeRegistryUnsub = null;
    this.lastActiveCollectionsSent = null;
    this.lastActiveCollectionsSet = null;
    // Presence (ctox-presence-v1): local entries flow registry -> wire via
    // `rxdb.presence.update`; remote aggregates flow wire -> registry via the
    // `presence$` push. Capability-gated per handshake.
    this.presenceRegistry = getPresenceRegistry();
    this.presenceUnsub = null;
    this.presenceCapable = false;
  }

  representativeCollection() {
    const first = this.collections.keys().next();
    return first.done ? null : this.collections.get(first.value);
  }

  register(collection, registration) {
    const isNewCollection = !this.collections.has(collection);
    this.collections.set(collection, registration);
    this.refCount += 1;
    if (isNewCollection) {
      this.schemaMismatchCollections.delete(collection);
      if (this.negotiated) {
        // The room handshake carries a point-in-time collectionSchemas map.
        // Runtime-installed app modules register their collections after the
        // shell-critical room is already open, so a cached handshake cannot be
        // reused for the new collection without producing a false schema hash
        // mismatch. Drop it and let the catch-up path renegotiate this room
        // with the complete collection set.
        this.negotiated = null;
      }
    }
    this.scheduleCollectionCatchUp(collection, registration);
  }

  scheduleAllCollectionCatchUps() {
    for (const [collection, registration] of this.collections.entries()) {
      this.scheduleCollectionCatchUp(collection, registration);
    }
  }

  scheduleCollectionCatchUp(collection, registration) {
    if (!collection || this.collectionCatchUps.has(collection)) return;
    const run = this.peerOpenQueue
      .then(() => this.catchUpRegisteredCollection(collection, registration))
      .catch((error) => registration.state?.emitError?.(error))
      .finally(() => this.collectionCatchUps.delete(collection));
    this.collectionCatchUps.set(collection, run);
  }

  async catchUpRegisteredCollection(collection, registration) {
    const negotiated = await this.ensureNegotiatedPeer();
    if (!negotiated || !this.isPeerOpen(negotiated.peerId)) return;
    const { peerId, queryFetchCapable } = negotiated;
    const existingPeerStates = registration.state?.peerStates$?.getValue?.();
    if (existingPeerStates?.has?.(peerId) && registration.state?.isPeerOpen?.(peerId)) return;
    if (this.schemaMismatchCollections.has(collection)) return;
    const remoteProtocol = this.remoteProtocolForCollection(negotiated.remoteProtocol, collection);
    const localSchemas = await this.collectCollectionSchemas();
    const only = { [collection]: localSchemas[collection] };
    const mismatches = assertCollectionSchemasCompatible(only, remoteProtocol);
    if (mismatches.has(collection)) {
      this.schemaMismatchCollections.add(collection);
      registration.state?.emitError?.(mismatches.get(collection));
      return;
    }
    await registration.state?.onPeerReady?.(peerId, remoteProtocol, queryFetchCapable);
  }

  async ensureNegotiatedPeer(peerIdHint = '') {
    if (this.negotiated && this.isPeerOpen(this.negotiated.peerId)) return this.negotiated;
    if (this.negotiationCatchUp) return this.negotiationCatchUp;
    const hintedPeerId = peerIdHint && this.isPeerOpen(peerIdHint) ? peerIdHint : '';
    const peerId = hintedPeerId || this.openSharedPeerIds()[0]
      || await this.waitForOpenSharedPeerId().catch(() => null);
    if (!peerId) return null;
    this.negotiationCatchUp = Promise.resolve()
      .then(async () => {
        if (this.negotiated && this.isPeerOpen(this.negotiated.peerId)) return this.negotiated;
        if (!this.isPeerOpen(peerId)) return null;
        return this.negotiatePeer(peerId);
      })
      .finally(() => {
        this.negotiationCatchUp = null;
      });
    return this.negotiationCatchUp;
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
      // Presence: stop forwarding local changes and drop remote hints — a
      // torn-down room has no live aggregate.
      if (this.presenceUnsub) {
        try { this.presenceUnsub(); } catch {}
        this.presenceUnsub = null;
      }
      this.presenceCapable = false;
      try { this.presenceRegistry.applyRemote([]); } catch {}
    }
  }

  abortPeerRequests(peerId, reason = 'peer-close') {
    return this.demandTransport?.abortPeerRequests?.(peerId, reason) || 0;
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
        .then(async () => {
          try {
            const negotiated = await this.ensureNegotiatedPeer(peerId);
            if (!negotiated) return;
            this.scheduleAllCollectionCatchUps();
          } catch (error) {
            if (isTransientSharedPeerError(error)) return;
            this.fanout('handshake-error', error);
          }
        });
    });
    this.peer.on('peer-close', (event) => {
      // A closed peer invalidates the negotiated handshake; a fresh peer-open
      // will renegotiate and re-drive every collection's catch-up.
      try { this.demandTransport.abortPeerRequests(event.detail?.peerId, event.detail?.reason || 'peer-close'); } catch {}
      if (this.negotiated && this.negotiated.peerId === event.detail?.peerId) {
        this.negotiated = null;
      }
      if (this.activeRemotePeerId === event.detail?.peerId) {
        this.activeRemotePeerId = null;
        // The native hub is gone — its last aggregate is stale. Clear the
        // remote hints; the post-reconnect handshake re-seeds them.
        this.presenceCapable = false;
        try { this.presenceRegistry.applyRemote([]); } catch {}
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
    // Presence push from the native hub: the aggregate of every OTHER peer's
    // entries. Replace the registry's remote state wholesale.
    this.peer.on('presence', (event) => {
      const entries = event.detail?.entries ?? event.entries ?? [];
      try { this.presenceRegistry.applyRemote(entries); } catch {}
    });
    // Forward local presence changes to the native hub. Fires immediately
    // with the current set; refresh ticks re-stamp the native TTL clock while
    // local entries exist (no entries -> no timer -> no traffic).
    if (!this.presenceUnsub) {
      this.presenceUnsub = this.presenceRegistry.onLocalChange((entries) => {
        this.sendPresenceUpdate(entries);
      });
    }
    // Phase 2: forward the RxDB layer's active-collection set to the native
    // peer whenever it changes. The listener fires immediately with the current
    // set and on every subsequent change (debounced inside the registry).
    if (!this.activeRegistryUnsub) {
      this.activeRegistryUnsub = this.activeRegistry.onChange((names) => {
        const previous = this.lastActiveCollectionsSet || new Set();
        const current = new Set(Array.isArray(names) ? names : []);
        this.lastActiveCollectionsSet = current;
        this.sendActiveCollections(names);
        // Catch-up on (re-)activation: the native peer DROPS master-change
        // relays for collections outside the reported active set, and pulls
        // are purely event-driven. A collection that just became active may
        // therefore have missed events while inactive — run one
        // checkpoint-based pull now so it converges instead of staying stale
        // until the next native write (rxdb-soak viewer-restart: a
        // ctox.file.materialize landing while desktop_files was inactive
        // never reached the browser). The native peer additionally pushes a
        // resync master-change when it applies the new set; this local pull
        // covers peers that predate that contract.
        for (const name of current) {
          if (previous.has(name)) continue;
          const registration = this.collections.get(name);
          try { registration?.state?.onMasterChange?.(); } catch {}
        }
      });
    }
    return this.peer;
  }

  // Presence: send `rxdb.presence.update` (fire-and-forget) to the active
  // native peer. Gated on the handshake capability so a pre-presence native
  // peer never sees the unknown method. No-op until a peer is open; resent on
  // (re)handshake because the native hub drops per-peer presence on
  // disconnect.
  sendPresenceUpdate(entries) {
    if (!this.presenceCapable) return;
    const peerId = this.activeRemotePeerId;
    if (!peerId || !this.peer) return;
    const list = Array.isArray(entries) ? entries : this.presenceRegistry.localEntries();
    try {
      this.peer.send(peerId, {
        id: `presence-update|${Date.now()}`,
        method: CTOX_PRESENCE_RPC.update,
        params: [list],
      });
    } catch {
      // Best-effort UX hint; the next change/refresh tick retries.
    }
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
      payload.collectionCheckpoints = await this.collectCollectionCheckpoints();
    }
    return payload;
  }

  // Build `{ collectionName -> { schemaVersion, schemaHash, schemaHashSource } }`
  // across every registered collection on this shared connection.
  async collectCollectionSchemas() {
    return this.collectCollectionMap(async (name, registration) => {
      const state = registration.state;
      if (!state) return null;
      let hash = state.schemaHashValue;
      if (!hash) {
        try { hash = await state.collection.schema.hash(); } catch { hash = null; }
      }
      return [name, {
        schemaVersion: state.collection?.schema?.version ?? null,
        schemaHash: hash || null,
        schemaHashSource: schemaHashSource(name),
      }];
    });
  }

  async collectCollectionCheckpoints() {
    return this.collectCollectionMap(async (name, registration) => {
      const state = registration.state;
      if (!state) return null;
      let hash = state.schemaHashValue;
      if (!hash) {
        try { hash = await state.collection.schema.hash(); } catch { hash = null; }
      }
      try {
        const checkpoint = await state.collection.storageCollection.replicationCheckpointStatus(hash || null);
        if (checkpoint && typeof checkpoint === 'object') {
          return [name, {
            ...checkpoint,
            collection: checkpoint.collection || name,
          }];
        }
      } catch {
        // The room-level payload still carries the representative checkpoint.
        // A per-collection checkpoint will be absent until the storage opens.
      }
      return null;
    });
  }

  async collectCollectionMap(mapper) {
    const entries = [...this.collections.entries()];
    const results = new Array(entries.length);
    let nextIndex = 0;
    const workerCount = Math.min(SHARED_PROTOCOL_COLLECTION_CONCURRENCY, entries.length);
    await Promise.all(Array.from({ length: workerCount }, async () => {
      while (nextIndex < entries.length) {
        const index = nextIndex;
        nextIndex += 1;
        const [name, registration] = entries[index];
        results[index] = await mapper(name, registration);
      }
    }));
    const map = {};
    for (const result of results) {
      if (!Array.isArray(result) || !result[0]) continue;
      map[result[0]] = result[1];
    }
    return map;
  }

  remoteProtocolForCollection(remoteProtocol, collection) {
    if (!remoteProtocol || typeof remoteProtocol !== 'object' || !collection) return remoteProtocol;
    const checkpoint = remoteProtocol.collectionCheckpoints?.[collection]
      || (remoteProtocol.collection?.name === collection ? remoteProtocol.collection?.checkpoint : null)
      || (remoteProtocol.checkpoint?.collection === collection ? remoteProtocol.checkpoint : null)
      || null;
    const schema = remoteProtocol.collectionSchemas?.[collection] || null;
    if (!checkpoint && !schema && remoteProtocol.collection?.name === collection) return remoteProtocol;
    const baseCollection = remoteProtocol.collection && typeof remoteProtocol.collection === 'object'
      ? remoteProtocol.collection
      : {};
    return {
      ...remoteProtocol,
      checkpoint: checkpoint || remoteProtocol.checkpoint || null,
      collection: {
        ...baseCollection,
        name: collection,
        ...(schema || {}),
        checkpoint: checkpoint || baseCollection.checkpoint || remoteProtocol.checkpoint || null,
      },
    };
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

  async negotiatePeer(peerId) {
    // The room-level handshake runs ONCE (off the representative collection).
    // Collection activation is driven separately by catch-up tasks so peer-open
    // events and late registrations cannot start duplicate handshakes.
    const representative = this.representativeCollection();
    if (!representative) return null;
    if (!this.isPeerOpen(peerId)) return null;
    const localProtocol = await this.peer.protocolPayload(peerId, [], representative.collection);
    if (!this.isPeerOpen(peerId)) return null;
    const remoteProtocol = await this.peer.request(
      peerId,
      'ctoxProtocol',
      [localProtocol],
      SHARED_HANDSHAKE_TIMEOUT_MS,
      representative.collection,
    );
    const normalizedRemoteProtocol = normalizeRemoteProtocol(remoteProtocol);
    if (!this.isPeerOpen(peerId)) return null;
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
      return null;
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
    await this.awaitRemoteMasterReady(peerId);
    const queryFetchCapable = remoteSupportsQueryFetch(normalizedRemoteProtocol);
    this.activeRemotePeerId = peerId;
    // Phase 2: the native peer cleared its per-peer active set on the prior
    // disconnect — re-send the current foreground set now that the handshake
    // completed so priority is correct from the first frame.
    this.sendActiveCollections();
    // Presence: same re-seed logic — the native hub dropped this peer's
    // presence on disconnect, so re-publish the local set (if any) now.
    this.presenceCapable = remoteSupportsPresence(normalizedRemoteProtocol);
    this.sendPresenceUpdate();
    // Retain the negotiated handshake so collections that register later catch
    // up immediately (see `register`).
    this.negotiated = { peerId, remoteProtocol: normalizedRemoteProtocol, queryFetchCapable };
    return this.negotiated;
  }

  isPeerOpen(peerId) {
    const connection = this.peer?.connections?.get?.(peerId);
    if (!connection) return false;
    const channelState = connection.channel?.readyState || '';
    const pcState = connection.peer?.connectionState || '';
    return channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(pcState);
  }

  openSharedPeerIds() {
    const ids = [];
    for (const peerId of this.peer?.connections?.keys?.() || []) {
      if (this.isPeerOpen(peerId)) ids.push(peerId);
    }
    return ids;
  }

  async waitForOpenSharedPeerId(timeoutMs = SHARED_PEER_OPEN_WAIT_MS) {
    const immediate = this.openSharedPeerIds()[0];
    if (immediate) return immediate;
    this.ensurePeer();
    return new Promise((resolve, reject) => {
      let settled = false;
      let unsubscribe = null;
      let interval = null;
      const settle = (handler, value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        if (interval) clearInterval(interval);
        try { unsubscribe?.(); } catch {}
        handler(value);
      };
      const inspect = () => {
        const peerId = this.openSharedPeerIds()[0];
        if (peerId) settle(resolve, peerId);
      };
      const timer = setTimeout(() => {
        settle(reject, new Error(`Timed out waiting for shared WebRTC peer in ${this.room}`));
      }, timeoutMs);
      unsubscribe = this.peer?.on?.('peer-open', (event) => {
        const peerId = event.detail?.peerId;
        if (peerId && this.isPeerOpen(peerId)) settle(resolve, peerId);
        else inspect();
      }) || null;
      interval = setInterval(inspect, 500);
      inspect();
    });
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
    return {
      ...(this.peer?.getTransportStatus?.() || {}),
      demandTransport: this.demandTransport?.diagnostics?.() || null,
    };
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
    this.pullInProgressPromise = null;
    this.pullAgainAfterCurrent = false;
    this.pushInProgress = false;
    this.pushInProgressPromise = null;
    this.pushAgainAfterCurrent = false;
    this.pullRetryTimer = null;
    this.pushRetryTimer = null;
    this.localPushTimer = null;
    // Checkpoints retained across a peer drop, keyed by the remote storage
    // epoch + native peer session. Reused on reconnect when both still match,
    // so a transport blip does not force a from-scratch resync.
    this.retainedCheckpoints = null;
    this.activeRemotePeerId = null;
    this.demandLoaderActive = false;
    this.demandStatus = createV1_5StatusState();
    this.schemaHashValue = null;
    this.peerReadyPromisesByPeer = new Map();
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
    this.changeSubscription = this.collection.observe((event) => {
      if (changeEventHasOnlyReplicationOriginWrites(event)) {
        return;
      }
      this.scheduleLocalWritePush();
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
      // 'disconnected' is transient (the transport layer keeps the
      // connection through a grace window and the Rust peer does the same).
      // Dropping the replication peer state here while the DataChannel
      // survived meant nobody re-added it after ICE recovered — the
      // collection silently stopped replicating. Only terminal states drop
      // the peer; a real teardown also emits 'peer-close'.
      if (['closed', 'failed'].includes(stateName)) {
        this.removePeer(detail?.peerId, `peer-${stateName}`);
      }
    }
  }

  onMasterChange() {
    if (this.cancelled) return;
    this.pullFromRemotePeers().catch((error) => {
      this.error$.next(error);
      this.schedulePullRetry();
    });
  }

  emitError(error) {
    this.error$.next(error);
  }

  async buildProtocolPayload() {
    const checkpoint = await this.collection.storageCollection.replicationCheckpointStatus(this.schemaHashValue);
    // #12c: attach the browser's CTOX capability token so the native (master)
    // peer can bind this peer to its role for per-collection read authz. Best
    // effort — a missing/failed token simply omits the field (native treats it
    // as least privilege). Never let token resolution break the handshake.
    const capabilityToken = await resolveCapabilityToken(this.ctox);
    return buildProtocolPayload({
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema.version,
      schemaHash: this.schemaHashValue,
      schemaHashSource: schemaHashSource(this.collection.name),
      peerSessionId: `browser:${this.topic}`,
      peerGeneration: 1,
      checkpoint,
      role: 'browser',
      capabilityToken,
      capabilities: BROWSER_CAPABILITIES,
      capabilityToken: typeof capabilityToken === 'string' ? capabilityToken : null,
    });
  }

  async onPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable) {
    if (this.peerReadyPromisesByPeer.has(peerId)) {
      return this.peerReadyPromisesByPeer.get(peerId);
    }
    const run = this.runPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable)
      .finally(() => this.peerReadyPromisesByPeer.delete(peerId));
    this.peerReadyPromisesByPeer.set(peerId, run);
    return run;
  }

  async runPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable) {
    if (this.cancelled) return;
    this.ctox?.onPeerProtocol?.(normalizedRemoteProtocol);
    this.activeRemotePeerId = peerId;
    this.demandStatus.peerConnected = true;
    this.demandStatus.peerCapabilityQueryFetchV1 = queryFetchCapable === true;
    // Seed retained checkpoints when the native storage generation matches —
    // the catch-up pull/push below then resumes incrementally instead of
    // re-reading everything from a null checkpoint after each reconnect.
    const validityKey = checkpointValidityKeyFromProtocol(normalizedRemoteProtocol);
    const retained = this.retainedCheckpoints;
    if (retained && validityKey) {
      if (retained.validityKey === validityKey) {
        if (retained.pull && !this.pullCheckpointsByPeer.has(peerId)) {
          this.pullCheckpointsByPeer.set(peerId, retained.pull);
        }
        if (retained.push && !this.pushCheckpointsByPeer.has(peerId)) {
          this.pushCheckpointsByPeer.set(peerId, retained.push);
        }
      } else {
        // Different daemon run / storage generation: the retained
        // checkpoints are meaningless there — drop them so this and every
        // later reconnect does the (correct) full resync.
        this.retainedCheckpoints = null;
      }
    }
    const peerStates = new Map(this.peerStates$.getValue() || new Map());
    peerStates.set(peerId, {
      peerId,
      replicationState: this,
      remoteProtocol: normalizedRemoteProtocol,
      queryFetchCapable,
    });
    this.peerStates$.next(peerStates);
    this.active$.next(true);
    this.transportStatus$.next(this.decorateTransportStatus(this.shared?.getTransportStatus?.() || this.transportStatus$.getValue?.() || {}));
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
    if (!this.pull) return;
    if (this.pullInProgressPromise) {
      this.pullAgainAfterCurrent = true;
      return this.pullInProgressPromise;
    }
    this.pullInProgress = true;
    this.pullAgainAfterCurrent = false;
    this.pullInProgressPromise = (async () => {
      do {
        this.pullAgainAfterCurrent = false;
        const peerIds = this.openPeerIds();
        const results = await Promise.allSettled(peerIds.map((peerId) => this.pullFromPeer(peerId)));
        this.reportPeerResults(results, peerIds);
        if (results.some((result) => result.status === 'rejected')) {
          this.schedulePullRetry();
        }
      } while (this.pullAgainAfterCurrent && !this.cancelled);
    })().finally(() => {
      this.pullInProgress = false;
      this.pullInProgressPromise = null;
      this.pullAgainAfterCurrent = false;
    });
    return this.pullInProgressPromise;
  }

  // Pulls are otherwise purely event-driven (`masterChangeStream$`): a pull
  // that failed past its in-band attempts was simply LOST until the next
  // remote write produced a new master-change event — a quiet collection
  // stayed stale until reload. Re-arm a single retry timer with the
  // configured `retryTime` (which was previously stored and never read).
  schedulePullRetry() {
    if (this.cancelled || this.pullRetryTimer) return;
    this.pullRetryTimer = setTimeout(() => {
      this.pullRetryTimer = null;
      if (this.cancelled) return;
      this.pullFromRemotePeers().catch((error) => {
        this.error$.next(error);
        this.schedulePullRetry();
      });
    }, Math.max(1000, Number(this.retryTime) || 5000));
  }

  schedulePushRetry() {
    if (this.cancelled || this.pushRetryTimer) return;
    this.pushRetryTimer = setTimeout(() => {
      this.pushRetryTimer = null;
      if (this.cancelled) return;
      this.pushToRemotePeers().catch((error) => {
        this.error$.next(error);
        this.schedulePushRetry();
      });
    }, Math.max(1000, Number(this.retryTime) || 5000));
  }

  scheduleLocalWritePush() {
    if (this.cancelled || !this.push || this.localPushTimer) return;
    this.localPushTimer = setTimeout(() => {
      this.localPushTimer = null;
      if (this.cancelled) return;
      this.pushToRemotePeers().catch((error) => this.error$.next(error));
    }, LOCAL_WRITE_PUSH_DEBOUNCE_MS);
    this.localPushTimer.unref?.();
  }

  async pullFromPeer(peerId) {
    const batchSize = Number(this.pull?.batchSize || 10);
    let activePeerId = peerId;
    let checkpoint = this.pullCheckpointsByPeer.get(activePeerId) || null;
    while (!this.cancelled) {
      const response = await this.requestMasterChangesSince(activePeerId, checkpoint, batchSize);
      activePeerId = response.peerId || activePeerId;
      const result = response.result || {};
      const documents = Array.isArray(result?.documents) ? result.documents : [];
      if (documents.length) {
        await this.collection.storageCollection.bulkWrite(documents, {
          replicationOrigin: this.replicationOriginForPeer(activePeerId),
        });
        await this.invalidateDemandCacheForRemoteWrite(documents);
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pullCheckpointsByPeer.set(activePeerId, checkpoint);
      // Drain until an EMPTY answer, not until a partial batch: the master
      // legitimately returns fewer documents than asked for (the
      // desktop_file_chunks response limiter caps answers at 96 KiB with a
      // checkpoint pointing at the last KEPT doc). Treating a short batch as
      // "drained" would strand the remainder until the next master-change
      // event. The final empty round-trip is the price of correctness.
      if (!documents.length) break;
    }
  }

  async requestMasterChangesSince(peerId, checkpoint, batchSize) {
    const timeoutMs = this.requestTimeoutMsFor('masterChangesSince');
    const maxAttempts = 3;
    let activePeerId = peerId;
    let lastError = null;
    for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
      try {
        const result = await this.peer.request(
          activePeerId,
          'masterChangesSince',
          [checkpoint, batchSize],
          timeoutMs,
          this.collection.name,
        );
        return { peerId: activePeerId, result };
      } catch (error) {
        lastError = error;
        if (
          attempt >= maxAttempts
          || this.cancelled
          || !this.isTransientMasterChangesSinceError(error)
        ) {
          throw error;
        }
        activePeerId = await this.waitForOpenPeerId().catch(() => {
          throw error;
        });
        await delay(250);
      }
    }
    throw lastError;
  }

  async pushToRemotePeers() {
    if (!this.push) return;
    if (this.pushInProgressPromise) {
      // Re-run after the in-flight push: a local write that lands during the
      // masterWrite round-trip used to be coalesced into the running push and
      // never re-read — the trailing writes of any burst sat unsynced until
      // the NEXT local write. Mirrors the pull loop's re-run flag.
      this.pushAgainAfterCurrent = true;
      return this.pushInProgressPromise;
    }
    this.pushInProgress = true;
    this.pushAgainAfterCurrent = false;
    this.pushInProgressPromise = (async () => {
      try {
        do {
          this.pushAgainAfterCurrent = false;
          const peerIds = this.openPeerIds();
          const results = await Promise.allSettled(peerIds.map((peerId) => this.pushToPeer(peerId)));
          this.reportPeerResults(results, peerIds);
          if (results.some((result) => result.status === 'rejected')) {
            this.schedulePushRetry();
          }
        } while (this.pushAgainAfterCurrent && !this.cancelled);
      } finally {
        this.pushInProgress = false;
        this.pushInProgressPromise = null;
        this.pushAgainAfterCurrent = false;
      }
    })();
    return this.pushInProgressPromise;
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
      this.recordLocalPushChangedSinceRead(result, documents);
      if (!documents.length) {
        const nextCheckpoint = result?.checkpoint || checkpoint;
        if (
          result?.scanLimitReached
          && nextCheckpoint
          && checkpointKey(nextCheckpoint) !== checkpointKey(checkpoint)
        ) {
          checkpoint = nextCheckpoint;
          this.pushCheckpointsByPeer.set(peerId, checkpoint);
          continue;
        }
        checkpoint = nextCheckpoint;
        this.pushCheckpointsByPeer.set(peerId, checkpoint);
        break;
      }
      let rows = documents.map((doc) => ({
        newDocumentState: doc,
        assumedMasterState: null,
      }));
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const conflicts = await this.peer.request(
          peerId,
          'masterWrite',
          [rows],
          this.requestTimeoutMsFor('masterWrite'),
          this.collection.name,
        );
        const conflictMap = documentsByPrimaryPath(conflicts, this.collection.schema.primaryPath);
        if (!conflictMap.size) {
          rows = [];
          break;
        }
        rows = rows
          .map((row) => {
            const id = primaryValue(row.newDocumentState, this.collection.schema.primaryPath);
            const assumedMasterState = conflictMap.get(id);
            return assumedMasterState ? { ...row, assumedMasterState } : null;
          })
          .filter(Boolean);
        if (!rows.length) break;
        // Field-merge collections: absorb the master's concurrent state into
        // the retry rows instead of force-overwriting it whole-doc (the
        // default LWW retry keeps its local-wins semantics unchanged).
        rows = await this.absorbMasterStateIntoConflictRows(rows);
      }
      if (rows.length) {
        throw new Error(`masterWrite conflicts remained for ${this.collection.name}`);
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pushCheckpointsByPeer.set(peerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }

  recordLocalPushChangedSinceRead(result, documents = []) {
    const scanned = Number.isFinite(Number(result?.scanned))
      ? Math.max(0, Number(result.scanned))
      : (Array.isArray(documents) ? documents.length : 0);
    this.demandStatus.localPushChangedSinceCalls =
      Number(this.demandStatus.localPushChangedSinceCalls || 0) + 1;
    this.demandStatus.localPushChangedSinceScannedRows =
      Number(this.demandStatus.localPushChangedSinceScannedRows || 0) + scanned;
    this.demandStatus.localPushChangedSinceMaxScannedRows = Math.max(
      Number(this.demandStatus.localPushChangedSinceMaxScannedRows || 0),
      scanned,
    );
    if (result?.scanLimitReached) {
      this.demandStatus.localPushChangedSinceScanLimitHits =
        Number(this.demandStatus.localPushChangedSinceScanLimitHits || 0) + 1;
    }
  }

  // Field-merge push repair: a masterWrite conflict means the master row
  // moved while our local write was unsynced. For `field-merge` collections
  // we three-way merge (stored base, local doc, master's conflict row),
  // persist the merged doc locally as a LOCAL write (it still carries
  // unsynced state), and retry the push with the merged doc + the master row
  // as assumedMasterState. LWW collections pass through untouched and keep
  // the existing local-wins force retry.
  async absorbMasterStateIntoConflictRows(rows) {
    const storage = this.collection?.storageCollection;
    if (!rows.length || storage?.conflictStrategy !== 'field-merge') return rows;
    const primaryPath = this.collection.schema.primaryPath;
    const mergedRows = [];
    for (const row of rows) {
      const id = primaryValue(row.newDocumentState, primaryPath);
      let record = null;
      try {
        record = await storage.getStoredRecord?.(id);
      } catch {}
      const { merged } = threeWayMergeDocuments(
        record?.base,
        row.newDocumentState,
        row.assumedMasterState,
        { primaryPath },
      );
      if (storage.mergeStats) storage.mergeStats.pushConflictMerges += 1;
      try {
        // Keep the local store in step with what we are about to push — a
        // plain local write (stays pushable until the push round-trips),
        // with the master's conflict row as the NEW base: the merged doc has
        // absorbed that state, so the stale stored base must not re-win
        // those fields on the next merge round (OS-C4).
        await storage.bulkWrite([merged], { baseById: { [id]: row.assumedMasterState } });
      } catch {}
      mergedRows.push({ newDocumentState: merged, assumedMasterState: row.assumedMasterState });
    }
    return mergedRows;
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
      await this.invalidateDemandCacheForRemoteWrite(docs);
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

  getTransportStatus(options = {}) {
    return this.decorateTransportStatus(this.shared?.getTransportStatus?.(options) || this.transportStatus$.getValue?.() || {});
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
    if (this.pullRetryTimer) {
      clearTimeout(this.pullRetryTimer);
      this.pullRetryTimer = null;
    }
    if (this.pushRetryTimer) {
      clearTimeout(this.pushRetryTimer);
      this.pushRetryTimer = null;
    }
    if (this.localPushTimer) {
      clearTimeout(this.localPushTimer);
      this.localPushTimer = null;
    }
    // Drop this collection from the shared peer before slower sidecar cleanup.
    // Restart paths stop collections with a bounded timeout and then
    // immediately start new bridges. If sidecar close stalls while the old
    // SharedRoomPeer is still registered, the restart can attach to a closing
    // peer and miss the first post-restart native connection.
    const shared = this.shared;
    this.shared = null;
    try { shared?.unregister?.(this.collection.name); } catch {}
    try { this.demandLoader?.abortAllInFlight?.('replication-cancel'); } catch {}
    try { this.demandFileLoader?.abortAllInFlight?.('replication-cancel'); } catch {}
    try { this.demandSidecar?.stopEvictionScheduler?.(); } catch {}
    try { await this.demandSidecar?.close?.(); } catch {}
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
    const queryDemandEnabled = shouldAttachQueryDemandLoader(this.collection.name);
    const fileDemandEnabled = shouldAttachFileDemandLoader(this.collection.name);
    if (!queryDemandEnabled && !fileDemandEnabled) {
      this.demandStatus.queryDemandLoadingEnabled = false;
      this.demandStatus.queryDemandLoadingActive = false;
      if (typeof this.collection.setDemandLoader === 'function') {
        this.collection.setDemandLoader(null);
      }
      this.demandLoader = null;
      this.demandFileLoader = null;
      this.demandLoaderActive = true;
      return null;
    }
    const dbName = databaseName || `ctox_business_os_v1_5_meta_${this.collection.name}`;
    const backend = indexedDbAvailable
      ? createIndexedDbMetaBackend({ databaseName: dbName })
      : createMemoryMetaBackend();
    this.demandStatus.queryDemandLoadingEnabled = queryDemandEnabled || fileDemandEnabled;
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

    // Demand-fetched documents are MASTER state: stamp them with the active
    // peer's replication origin so the push pipeline never echoes them back
    // and the LWW gate treats them as replicated (not unsynced-local) rows.
    const demandReplicationOrigin = () => (
      this.replicationOriginForPeer(this.activeRemotePeerId)
        || { role: 'ctox_instance', peerId: this.activeRemotePeerId || '', sessionId: '', collection: this.collection.name }
    );
    this.demandLoader = queryDemandEnabled ? createQueryDemandLoader({
      storageCollection: this.collection.storageCollection,
      sidecar: this.demandSidecar,
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema?.version || 0,
      requestQueryFetch: (envelope) => demandTransport.requestQueryFetch(envelope),
      requestCancel: ({ requestId, reason }) => demandTransport.requestQueryCancel({ requestId, reason }),
      status: this.demandStatus,
      replicationOrigin: demandReplicationOrigin,
    }) : null;
    if (typeof this.collection.setDemandLoader === 'function') {
      this.collection.setDemandLoader(this.demandLoader);
    }

    this.demandFileLoader = fileDemandEnabled ? createFileDemandLoader({
      collectionName: this.collection.name,
      storageCollection: this.collection.storageCollection,
      sidecarBackend: backend,
      persistChunks: shouldPersistFetchedFileChunks(this.collection.name),
      replicationOrigin: demandReplicationOrigin,
      requestFileFetch: ({ requestId, fileId, range, knownSequences }) =>
        demandTransport.requestFileFetch({
          requestId,
          fileId,
          range,
          knownSequences,
          collectionName: this.collection.name,
        }),
      requestFileCancel: ({ requestId, reason }) =>
        demandTransport.requestFileCancel({ requestId, reason }),
      status: this.demandStatus,
    }) : null;

    this.demandLoaderActive = true;
    this.demandStatus.queryDemandLoadingActive = queryDemandEnabled || fileDemandEnabled;
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
    // Retain the checkpoints (validity-keyed) BEFORE dropping the peer.
    // Discarding them outright meant EVERY reconnect re-synced the whole
    // collection from a null checkpoint — across ~80 collections that
    // saturated the fresh DataChannel and manufactured the next timeouts.
    const validityKey = this.checkpointValidityKeyForPeer(peerId);
    const retainedPull = this.pullCheckpointsByPeer.get(peerId) || null;
    const retainedPush = this.pushCheckpointsByPeer.get(peerId) || null;
    if (validityKey && (retainedPull || retainedPush)) {
      this.retainedCheckpoints = { validityKey, pull: retainedPull, push: retainedPush };
    }
    peerStates.delete(peerId);
    this.pullCheckpointsByPeer.delete(peerId);
    this.pushCheckpointsByPeer.delete(peerId);
    this.peerStates$.next(peerStates);
    try { this.demandLoader?.abortAllInFlight?.(`peer-${reason}`); } catch {}
    try { this.demandFileLoader?.abortAllInFlight?.(`peer-${reason}`); } catch {}
    try { this.shared?.abortPeerRequests?.(peerId, reason); } catch {}
    if (!peerStates.size) {
      this.demandStatus.peerConnected = false;
      this.active$.next(false);
    }
    this.ctox?.onPeerClose?.({ peerId, reason });
  }

  // Checkpoints are only reusable against the SAME native storage generation:
  // the storage epoch (bumped on a wire-format/storage reset) plus the native
  // peer session id (new on every daemon run). A daemon restart therefore
  // still forces a conservative full resync; a transport-level reconnect
  // within one daemon run resumes from the last acknowledged checkpoint.
  checkpointValidityKeyForPeer(peerId) {
    const remoteProtocol = this.remoteProtocolForPeer(peerId);
    return checkpointValidityKeyFromProtocol(remoteProtocol);
  }

  remoteProtocolForPeer(peerId) {
    const localProtocol = (this.peerStates$.getValue() || new Map()).get(peerId)?.remoteProtocol || null;
    if (localProtocol) return localProtocol;
    const negotiated = this.shared?.negotiated || null;
    return negotiated?.peerId === peerId
      ? this.shared?.remoteProtocolForCollection?.(negotiated.remoteProtocol, this.collection.name) || negotiated.remoteProtocol || null
      : null;
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

  async invalidateDemandCacheForRemoteWrite(changedDocuments = []) {
    try {
      const ids = changedDocuments
        .map((doc) => primaryValue(doc, this.collection.schema.primaryPath))
        .filter(Boolean);
      if (typeof this.demandLoader?.invalidateDocumentChange === 'function') {
        await this.demandLoader.invalidateDocumentChange(ids);
      } else {
        await this.demandLoader?.invalidateCollectionChange?.();
      }
    } catch {
      // Demand-cache invalidation is a freshness hint; replication must not fail
      // just because the sidecar backend is unavailable.
    }
  }

  requestTimeoutMsFor(method) {
    if (this.collection.name === 'desktop_file_chunks') {
      return method === 'masterChangesSince' ? 45000 : 30000;
    }
    if (method === 'masterWrite') {
      if ([
        'business_commands',
        'ctox_queue_tasks',
        'business_chats',
        'research_runs',
        'research_notes',
        'knowledge_items',
      ].includes(this.collection.name)) {
        return 60000;
      }
      return 45000;
    }
    return 15000;
  }

  periodicPullIntervalMs() {
    return 0;
  }

  periodicPushIntervalMs() {
    if (!this.push) return 0;
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
    if (!open.length && this.shared?.negotiated?.peerId && this.shared.isPeerOpen?.(this.shared.negotiated.peerId)) {
      open.push(this.shared.negotiated.peerId);
    }
    return open;
  }

  async waitForOpenPeerId(timeoutMs = 8000) {
    const immediatePeerId = this.openPeerIds()[0];
    if (immediatePeerId) return immediatePeerId;
    return new Promise((resolve, reject) => {
      let settled = false;
      let subscription = null;
      const settle = (handler, value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        try {
          subscription?.unsubscribe?.();
        } catch {
          // Ignore observer cleanup failures; the peer wait is already settled.
        }
        handler(value);
      };
      const inspect = () => {
        const peerId = this.openPeerIds()[0];
        if (peerId) settle(resolve, peerId);
      };
      const timer = setTimeout(() => {
        settle(reject, new Error(`Timed out waiting for WebRTC peer reopen for ${this.collection.name}`));
      }, timeoutMs);
      subscription = this.peerStates$?.subscribe?.(inspect) || null;
      inspect();
    });
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
    const demandTransport = status.demandTransport || this.shared?.demandTransport?.diagnostics?.() || null;
    if (demandTransport) {
      this.demandStatus.pendingQueryFetchCollectors = Number(demandTransport.pendingQueryCollectors || 0);
      this.demandStatus.pendingFileFetchCollectors = Number(demandTransport.pendingFileCollectors || 0);
      this.demandStatus.queuedQueryFetchRequests = Number(demandTransport.queuedQueryRequests || 0);
      this.demandStatus.maxPendingQueryFetchCollectors = Math.max(
        Number(this.demandStatus.maxPendingQueryFetchCollectors || 0),
        Number(demandTransport.maxPendingQueryCollectors || 0),
      );
      this.demandStatus.maxPendingFileFetchCollectors = Math.max(
        Number(this.demandStatus.maxPendingFileFetchCollectors || 0),
        Number(demandTransport.maxPendingFileCollectors || 0),
      );
    }
    const localPeerCount = (this.peerStates$.getValue?.() || new Map()).size;
    const sharedPeerCount = this.shared?.openSharedPeerIds?.().length || 0;
    const connectionPeerCount = Array.isArray(status.connectionStates)
      ? status.connectionStates.filter((connection) => {
          const channelState = connection?.channelState || '';
          const pcState = connection?.peerConnectionState || '';
          return channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(pcState);
        }).length
      : 0;
    return {
      ...status,
      collection: this.collection.name,
      topic: this.topic,
      activePeerCount: Math.max(localPeerCount, sharedPeerCount, connectionPeerCount),
      pullInProgress: this.pullInProgress,
      pushInProgress: this.pushInProgress,
      demandLoading: snapshotV1_5Status(this.demandStatus),
      demandTransport,
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

// Native storage generation a checkpoint is valid against: storage epoch +
// the native peer's per-run session id. Both must match for retained
// checkpoints to be reused after a reconnect; empty when either is missing
// (then no reuse happens and the conservative full resync runs).
function checkpointValidityKeyFromProtocol(remoteProtocol) {
  if (!remoteProtocol || typeof remoteProtocol !== 'object') return '';
  const epoch = typeof remoteProtocol.checkpoint?.epoch === 'string'
    ? remoteProtocol.checkpoint.epoch.trim()
    : '';
  const sessionId = typeof remoteProtocol.peerSession?.sessionId === 'string'
    ? remoteProtocol.peerSession.sessionId.trim()
    : '';
  if (!epoch || !sessionId) return '';
  return `${epoch}|${sessionId}`;
}

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

function documentsByPrimaryPath(documents = [], primaryPath = 'id') {
  const map = new Map();
  for (const doc of Array.isArray(documents) ? documents : []) {
    const id = primaryValue(doc, primaryPath);
    if (id) map.set(id, doc);
  }
  return map;
}

function changeEventHasOnlyReplicationOriginWrites(event) {
  const docs = Object.values(event?.success || {});
  return docs.length > 0 && docs.every((doc) => Boolean(doc?._meta?.ctoxReplicationOrigin?.role));
}

async function resolveCapabilityToken(ctox = {}) {
  if (typeof ctox?.capabilityTokenProvider === 'function') {
    try {
      const token = await ctox.capabilityTokenProvider();
      return typeof token === 'string' && token.trim() ? token.trim() : null;
    } catch {
      return null;
    }
  }
  // #12c: also support ctox.capabilityToken being a function (best-effort
  // resolver) in addition to a plain string. Never let resolution throw.
  const source = ctox?.capabilityToken;
  if (typeof source === 'function') {
    try {
      const token = await source();
      return typeof token === 'string' && token.trim() ? token.trim() : null;
    } catch {
      return null;
    }
  }
  return typeof source === 'string' && source.trim() ? source.trim() : null;
}

function checkpointKey(checkpoint) {
  if (!checkpoint) return '';
  return `${Number(checkpoint.lwt || 0)}\0${String(checkpoint.id || '')}`;
}

function primaryValue(doc = {}, primaryPath = 'id') {
  if (!doc || typeof doc !== 'object') return '';
  if (doc.id != null) return String(doc.id);
  if (doc._id != null) return String(doc._id);
  return String(replicationValueAtPath(doc, primaryPath) ?? '');
}

function shouldPersistFetchedFileChunks(collectionName = '') {
  return String(collectionName || '').endsWith('_chunks');
}

function shouldAttachQueryDemandLoader(collectionName = '') {
  return !String(collectionName || '').endsWith('_chunks');
}

function shouldAttachFileDemandLoader(collectionName = '') {
  return String(collectionName || '') !== 'desktop_file_chunks';
}

function replicationValueAtPath(obj, path) {
  if (!path || path === 'id') return obj?.id;
  return String(path).split('.').reduce((acc, part) => (acc == null ? undefined : acc[part]), obj);
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
    collectionCheckpoints: normalizeRemoteCollectionCheckpoints(payload.collectionCheckpoints),
  };
}

function normalizeRemoteCollectionCheckpoints(map) {
  if (!map || typeof map !== 'object') return null;
  const out = {};
  for (const [name, entry] of Object.entries(map)) {
    if (!name || !entry || typeof entry !== 'object') continue;
    out[name] = {
      ...entry,
      collection: typeof entry.collection === 'string' && entry.collection ? entry.collection : name,
    };
  }
  return Object.keys(out).length > 0 ? out : null;
}
