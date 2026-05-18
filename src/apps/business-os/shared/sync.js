export function createSyncRuntime({ db, baseUrl, config }) {
  const p2pPools = new Map();
  const instanceBridges = new Map();
  return {
    db,
    baseUrl,
    config,
    mode: config?.sync_mode || 'p2p-first',
    async startModule(moduleManifest) {
      if (config?.http_bridge_available === false) {
        return { mode: 'static-preview', reason: 'http-bridge-disabled' };
      }
      const collections = moduleManifest?.collections || [];
      const started = collections.map((collection) => this.startCollection(collection));
      return Promise.allSettled(started);
    },
    async startCollection(collection) {
      if (p2pPools.has(collection)) return p2pPools.get(collection);
      const bridge = startInstanceBridge({
        db,
        collection,
        runtime: this,
      });
      instanceBridges.set(collection, bridge);
      hydrateCollectionFromInstance({ db, collection, runtime: this })
        .catch((error) => console.error(`[business-os] background CTOX instance pull failed for ${collection}`, error));
      let p2p = null;
      try {
        p2p = await tryStartP2PCollectionSync({
          db,
          collection,
          config,
        });
      } catch (error) {
        console.warn(`[business-os] WebRTC sync unavailable for ${collection}`, error);
        p2p = { mode: 'unavailable', collection, reason: error?.message || String(error) };
      }
      const pool = { mode: 'ctox-instance+p2p', collection, instanceBridge: bridge, p2p };
      p2pPools.set(collection, pool);
      return pool;
    },
    async pull(collection) {
      const res = await fetch(`${baseUrl}/rxdb/${encodeURIComponent(collection)}/pull`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ checkpoint: null, batch_size: 200 }),
      });
      if (!res.ok) throw new Error(`Pull failed for ${collection}: ${res.status}`);
      return res.json();
    },
    async pullAll(collection) {
      const documents = [];
      let checkpoint = null;
      for (;;) {
        const res = await fetch(`${baseUrl}/rxdb/${encodeURIComponent(collection)}/pull`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ checkpoint, batch_size: batchSizeFor(collection) }),
        });
        if (!res.ok) throw new Error(`Pull failed for ${collection}: ${res.status}`);
        const page = await res.json();
        const pageDocuments = Array.isArray(page?.documents) ? page.documents : [];
        documents.push(...pageDocuments);
        checkpoint = page?.checkpoint || checkpoint;
        if (!page?.has_more || !pageDocuments.length) break;
      }
      return { ok: true, documents, checkpoint };
    },
    async push(collection, documents) {
      const res = await fetch(`${baseUrl}/rxdb/${encodeURIComponent(collection)}/push`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ documents }),
      });
      if (!res.ok) throw new Error(`Push failed for ${collection}: ${res.status}`);
      return res.json();
    },
  };
}

function startInstanceBridge({ db, collection, runtime }) {
  const rxCollection = db?.raw?.[collection];
  if (!rxCollection) return { mode: 'pending', reason: 'collection-not-registered' };
  const serverAuthoritative = isServerAuthoritativeCollection(collection);

  const queue = new Map();
  let flushTimer = null;
  let pollTimer = null;
  let flushPromise = Promise.resolve();
  let hydrating = false;
  let stopped = false;

  const schedule = (documents, delayMs = 150) => {
    if (stopped || hydrating) return;
    for (const document of documents) {
      const json = toPlainDocument(document);
      const id = json?.id;
      if (!id) continue;
      queue.set(id, json);
    }
    if (!queue.size || flushTimer) return;
    flushTimer = setTimeout(() => {
      flushTimer = null;
      flushPromise = flushPromise
        .then(() => flushQueuedDocuments())
        .catch((error) => {
          console.error(`[business-os] CTOX instance push failed for ${collection}`, error);
          if (!stopped) schedule(Array.from(queue.values()), retryDelayFor(collection));
        });
    }, delayMs);
  };

  const flushQueuedDocuments = async () => {
    if (stopped || !queue.size) return;
    const documents = Array.from(queue.values());
    queue.clear();
    for (const batch of chunkArray(documents, batchSizeFor(collection))) {
      await runtime.push(collection, batch);
    }
  };

  const initialPush = serverAuthoritative
    ? Promise.resolve()
    : rxCollection.find().exec()
      .then((docs) => schedule(docs, 0))
      .catch((error) => console.error(`[business-os] initial CTOX instance push failed for ${collection}`, error));

  const hydrateFromInstance = async () => {
    if (stopped || hydrating) return;
    hydrating = true;
    try {
      await hydrateCollectionFromInstance({ db, collection, runtime });
    } catch (error) {
      console.error(`[business-os] CTOX instance pull failed for ${collection}`, error);
    } finally {
      hydrating = false;
    }
  };

  pollTimer = setInterval(hydrateFromInstance, pollDelayFor(collection));

  const sub = serverAuthoritative ? null : rxCollection.$?.subscribe?.((event) => {
    const document = event?.documentData
      || (event?.previousDocumentData ? { ...event.previousDocumentData, _deleted: true } : null);
    if (document) schedule([document]);
  });

  return {
    mode: 'ctox-instance',
    collection,
    initialPush,
    flush: () => flushPromise.then(() => flushQueuedDocuments()),
    stop() {
      stopped = true;
      if (flushTimer) clearTimeout(flushTimer);
      if (pollTimer) clearInterval(pollTimer);
      flushTimer = null;
      pollTimer = null;
      try { sub?.unsubscribe?.(); } catch {}
    },
  };
}

async function hydrateCollectionFromInstance({ db, collection, runtime }) {
  const rxCollection = db?.raw?.[collection];
  if (!rxCollection) return;
  const payload = typeof runtime.pullAll === 'function'
    ? await runtime.pullAll(collection)
    : await runtime.pull(collection);
  const documents = Array.isArray(payload?.documents) ? payload.documents : [];
  if (documents.length && typeof rxCollection.bulkUpsert === 'function') {
    const result = await rxCollection.bulkUpsert(documents);
    const errors = Array.isArray(result?.error) ? result.error : [];
    if (!errors.length) {
      await markMissingServerDocsStale({ rxCollection, collection, documents });
      return;
    }
  }
  for (const doc of documents) {
    await rxCollection.upsert(doc);
  }
  await markMissingServerDocsStale({ rxCollection, collection, documents });
}

async function markMissingServerDocsStale({ rxCollection, collection, documents }) {
  if (!isServerAuthoritativeCollection(collection)) return;
  const remoteIds = new Set(documents.map((doc) => doc?.id).filter(Boolean));
  const localDocs = await rxCollection.find().exec();
  const now = Date.now();
  for (const localDoc of localDocs) {
    const json = toPlainDocument(localDoc);
    const id = json?.id;
    if (!id || remoteIds.has(id) || json?._deleted) continue;
    if (!isActiveProjection(json)) continue;
    const patch = collection === 'business_commands'
      ? {
          status: 'stale_missing_native',
          task_status: 'stale_missing_native',
          updated_at_ms: now,
          client_context: {
            ...(json.client_context || {}),
            stale_reason: 'not present in native Business OS store or CTOX queue',
          },
        }
      : {
          status: 'stale_missing_native',
          route_status: 'stale_missing_native',
          updated_at_ms: now,
        };
    await localDoc.incrementalPatch(patch);
  }
}

function isServerAuthoritativeCollection(collection) {
  return collection === 'business_commands' || collection === 'ctox_queue_tasks';
}

function isActiveProjection(doc) {
  const status = String(doc?.status || doc?.route_status || doc?.task_status || '').toLowerCase();
  return [
    'accepted',
    'queued',
    'queued_local',
    'pending',
    'leased',
    'working',
    'running',
  ].includes(status);
}

async function tryStartP2PCollectionSync({ db, collection, config }) {
  if (!db?.raw) return { mode: 'pending', reason: 'rxdb-not-ready' };
  const rxCollection = db.raw[collection];
  if (!rxCollection) return { mode: 'pending', reason: 'collection-not-registered' };
  const webrtc = await loadRxdbWebRTC();
  const replicateWebRTC = webrtc?.replicateWebRTC;
  const getConnectionHandlerSimplePeer = webrtc?.getConnectionHandlerSimplePeer;
  if (!replicateWebRTC || !getConnectionHandlerSimplePeer) {
    throw new Error('RxDB WebRTC runtime is missing from vendor/rxdb-bundle.mjs');
  }

  const signalingServerUrl = config?.signaling_urls?.[0];
  if (!signalingServerUrl) {
    throw new Error('Business OS P2P sync requires at least one signaling URL');
  }

  const pool = replicateWebRTC({
    collection: rxCollection,
    topic: `${config.sync_room}:${collection}`,
    connectionHandlerCreator: getConnectionHandlerSimplePeer({
      signalingServerUrl,
    }),
    pull: { batchSize: batchSizeFor(collection) },
    push: { batchSize: batchSizeFor(collection) },
  });
  return { mode: 'p2p', pool, collection, signalingServerUrl };
}

function batchSizeFor(collection) {
  return collection.includes('attachment') || collection.includes('chunk') ? 1 : 10;
}

function retryDelayFor(collection) {
  return collection.includes('chunk') ? 1500 : 750;
}

function pollDelayFor(collection) {
  return collection.includes('chunk') ? 30000 : 10000;
}

function toPlainDocument(document) {
  if (!document) return null;
  if (typeof document.toJSON === 'function') return document.toJSON();
  if (typeof document === 'object') return { ...document };
  return null;
}

function chunkArray(items, size) {
  const out = [];
  const chunkSize = Math.max(1, Number(size) || 1);
  for (let i = 0; i < items.length; i += chunkSize) {
    out.push(items.slice(i, i + chunkSize));
  }
  return out;
}

async function loadRxdbWebRTC() {
  const mod = await import('../vendor/rxdb-bundle.mjs');
  if (typeof mod.rxdbWebRTC === 'function') return mod.rxdbWebRTC();
  if (globalThis.rxdbWebRTC) return globalThis.rxdbWebRTC();
  return mod;
}
