import { batchSizeFor } from './sync-contract.js';

export function createSyncRuntime({ db, baseUrl = '/api/business-os', config }) {
  const bridges = new Map();
  return {
    db,
    baseUrl,
    config,
    mode: config?.http_bridge_available === false ? 'local-only' : 'native-http',
    async startModule(moduleManifest) {
      if (config?.http_bridge_available === false) {
        return { mode: 'local-only', reason: 'http-bridge-disabled' };
      }
      const collections = moduleManifest?.collections || [];
      const started = collections.map((collection) => this.startCollection(collection));
      return Promise.allSettled(started);
    },
    async startCollection(collection) {
      if (bridges.has(collection)) return bridges.get(collection);
      const bridge = startNativeBridge({ db, baseUrl, collection });
      bridges.set(collection, bridge);
      bridge.pullNow().catch((error) => {
        console.error(`[business-os] native pull failed for ${collection}`, error);
      });
      return bridge;
    },
    async pull(collection, sinceMs = 0) {
      const url = new URL(`${baseUrl}/rxdb/pull`, window.location.origin);
      url.searchParams.set('collection', collection);
      url.searchParams.set('since_ms', String(Math.max(0, Number(sinceMs) || 0)));
      url.searchParams.set('limit', String(Math.max(1, batchSizeFor(collection) * 50)));
      const res = await fetch(url);
      if (!res.ok) throw new Error(`Pull failed for ${collection}: ${res.status}`);
      return res.json();
    },
    async push(collection, documents) {
      const res = await fetch(`${baseUrl}/rxdb/push`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          collection,
          documents: documents.map(stripRxdbInternals),
        }),
      });
      if (!res.ok) throw new Error(`Push failed for ${collection}: ${res.status}`);
      return res.json();
    },
  };
}

function startNativeBridge({ db, baseUrl, collection }) {
  const rxCollection = db?.raw?.[collection];
  if (!rxCollection) return { mode: 'pending', collection, reason: 'collection-not-registered' };

  let stopped = false;
  let hydrating = false;
  const bridgeStartedAt = Date.now();
  let lastPullMs = 0;
  let flushTimer = null;
  let flushPromise = Promise.resolve();
  const queued = new Map();

  const runtime = {
    async pull() {
      const url = new URL(`${baseUrl}/rxdb/pull`, window.location.origin);
      url.searchParams.set('collection', collection);
      url.searchParams.set('since_ms', String(Math.max(0, lastPullMs - 1)));
      url.searchParams.set('limit', String(Math.max(1, batchSizeFor(collection) * 50)));
      const res = await fetch(url);
      if (!res.ok) throw new Error(`Pull failed for ${collection}: ${res.status}`);
      return res.json();
    },
    async push(documents) {
      const res = await fetch(`${baseUrl}/rxdb/push`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          collection,
          documents: documents.map(stripRxdbInternals),
        }),
      });
      if (!res.ok) throw new Error(`Push failed for ${collection}: ${res.status}`);
      return res.json();
    },
  };

  async function pullNow() {
    if (stopped || hydrating) return;
    hydrating = true;
    try {
      const payload = await runtime.pull();
      const documents = Array.isArray(payload?.documents) ? payload.documents : [];
      for (const document of documents) {
        try {
          const plain = normalizePulledDocument(document, collection);
          const id = plain?.id;
          if (!id) continue;
          const updatedAt = Number(plain.updated_at_ms || 0);
          if (updatedAt > lastPullMs) lastPullMs = updatedAt;
          const existing = await rxCollection.findOne(id).exec();
          if (existing) {
            await existing.incrementalPatch(plain);
          } else {
            try {
              await rxCollection.insert(plain);
            } catch (insertError) {
              const lateExisting = await rxCollection.findOne(id).exec();
              if (!lateExisting) throw insertError;
              await lateExisting.incrementalPatch(plain);
            }
          }
        } catch (error) {
          console.error(`[business-os] native pull skipped invalid ${collection} record`, document?.id || document?.record_id || document, error);
        }
      }
    } finally {
      hydrating = false;
    }
  }

  function schedulePush(document) {
    if (stopped || hydrating || isReadOnlyProjectionCollection(collection)) return;
    const plain = stripRxdbInternals(document);
    const id = plain?.id;
    if (!id) return;
    if (collection === 'business_commands') {
      if (plain.status !== 'pending_sync') return;
      if (Number(plain.updated_at_ms || 0) < bridgeStartedAt - 15000) return;
    }
    queued.set(id, plain);
    if (flushTimer) return;
    flushTimer = window.setTimeout(() => {
      flushTimer = null;
      flushPromise = flushPromise
        .then(flushQueued)
        .catch((error) => {
          console.error(`[business-os] native push failed for ${collection}`, error);
        });
    }, 150);
  }

  async function flushQueued() {
    if (stopped || !queued.size) return;
    const documents = Array.from(queued.values());
    queued.clear();
    for (const batch of chunkArray(documents, batchSizeFor(collection))) {
      await runtime.push(batch);
    }
  }

  const subscription = rxCollection.$?.subscribe?.((event) => {
    const document = event?.documentData
      || (event?.previousDocumentData ? { ...event.previousDocumentData, _deleted: true } : null);
    if (document) schedulePush(document);
  });
  const pollTimer = window.setInterval(() => {
    pullNow().catch((error) => console.error(`[business-os] native pull failed for ${collection}`, error));
  }, pollDelayFor(collection));

  return {
    mode: 'native-http',
    collection,
    pullNow,
    flush: () => flushPromise.then(flushQueued),
    stop() {
      stopped = true;
      if (flushTimer) window.clearTimeout(flushTimer);
      window.clearInterval(pollTimer);
      try { subscription?.unsubscribe?.(); } catch {}
    },
  };
}

function normalizePulledDocument(document, collection) {
  const plain = stripRxdbInternals(document);
  if (plain._deleted) {
    plain.is_deleted = true;
    delete plain._deleted;
  }
  if (collection === 'documents') {
    plain.is_deleted = Boolean(plain.is_deleted);
    plain.linked_records = Array.isArray(plain.linked_records) ? plain.linked_records : [];
    plain.display_cache = plain.display_cache && typeof plain.display_cache === 'object' ? plain.display_cache : {};
    plain.owner_id = String(plain.owner_id || '');
    if (plain.status === 'Generated') plain.status = 'Draft';
  }
  if (collection === 'document_versions') {
    plain.model_json = plain.model_json && typeof plain.model_json === 'object' && !Array.isArray(plain.model_json)
      ? plain.model_json
      : {};
    plain.diagnostics = Array.isArray(plain.diagnostics) ? plain.diagnostics : [];
  }
  if (collection === 'document_blob_chunks') {
    delete plain.chunk_id;
  }
  return plain;
}

function stripRxdbInternals(document) {
  const {
    _attachments,
    _deleted,
    _meta,
    _rev,
    ...plain
  } = document || {};
  if (_deleted) plain._deleted = true;
  return plain;
}

function isReadOnlyProjectionCollection(collection) {
  return collection === 'ctox_queue_tasks'
    || collection === 'business_chats';
}

function pollDelayFor(collection) {
  if (collection === 'ctox_queue_tasks' || collection === 'business_commands' || collection === 'business_chats') return 2000;
  if (collection === 'documents' || collection === 'document_versions') return 3000;
  return 5000;
}

function chunkArray(items, size) {
  const chunks = [];
  const chunkSize = Math.max(1, size || 50);
  for (let index = 0; index < items.length; index += chunkSize) {
    chunks.push(items.slice(index, index + chunkSize));
  }
  return chunks;
}
