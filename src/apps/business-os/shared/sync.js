import { batchSizeFor, collectionTopic, nativeRxdbPeerReady } from './sync-contract.js';

export function createSyncRuntime({ db, config }) {
  const bridges = new Map();
  const useWebrtc = nativeRxdbPeerReady(config, db);
  return {
    db,
    config,
    mode: useWebrtc ? 'webrtc' : 'local-only',
    async startModule(moduleManifest) {
      if (!useWebrtc) {
        return {
          mode: 'local-only',
          reason: config?.native_rxdb_peer_reason || 'sync-transport-unavailable',
        };
      }
      const collections = moduleManifest?.collections || [];
      const started = collections.map((collection) => this.startCollection(collection));
      return Promise.allSettled(started);
    },
    async startCollection(collection) {
      if (bridges.has(collection)) return bridges.get(collection);
      const bridgePromise = useWebrtc
        ? startWebRtcReplication({ db, config, collection })
        : Promise.resolve({
            mode: 'local-only',
            collection,
            reason: config?.native_rxdb_peer_reason || 'sync-transport-unavailable',
          });
      bridges.set(collection, bridgePromise);
      try {
        return await bridgePromise;
      } catch (error) {
        bridges.delete(collection);
        throw error;
      }
    },
  };
}

async function startWebRtcReplication({ db, config, collection }) {
  const rxCollection = db?.raw?.[collection] || db?.collection?.(collection);
  if (!rxCollection) return { mode: 'pending', collection, reason: 'collection-not-registered' };
  const rxdb = db?.rxdb || await import('../vendor/rxdb-bundle.mjs');
  if (typeof rxdb?.replicateWebRTC !== 'function' || typeof rxdb?.getConnectionHandlerSimplePeer !== 'function') {
    throw new Error('RxDB WebRTC bundle is missing replicateWebRTC/getConnectionHandlerSimplePeer');
  }

  ensureBrowserProcessNextTick();
  const signalingServerUrl = firstSignalingUrl(config);
  const topic = collectionTopic(config.sync_room, collection);
  const batchSize = batchSizeFor(collection);
  const connectionHandlerCreator = rxdb.getConnectionHandlerSimplePeer({ signalingServerUrl });
  const replicationState = await rxdb.replicateWebRTC({
    collection: rxCollection,
    topic,
    connectionHandlerCreator,
    pull: { batchSize },
    push: isReadOnlyProjectionCollection(collection) ? undefined : { batchSize },
    retryTime: 5000,
  });
  const errorSubscription = replicationState.error$?.subscribe?.((error) => {
    console.error(`[business-os] WebRTC replication failed for ${collection}`, error);
  });

  return {
    mode: 'webrtc',
    collection,
    topic,
    state: replicationState,
    pullNow: async () => {},
    flush: async () => {},
    stop() {
      try { errorSubscription?.unsubscribe?.(); } catch {}
      try { replicationState.cancel?.(); } catch {}
    },
  };
}

function firstSignalingUrl(config) {
  const urls = Array.isArray(config?.signaling_urls) ? config.signaling_urls : [];
  const url = urls.find((candidate) => typeof candidate === 'string' && candidate.trim());
  if (!url) throw new Error('Business OS WebRTC sync requires a signaling URL');
  return url;
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
    || collection === 'ctox_runtime_settings';
}
