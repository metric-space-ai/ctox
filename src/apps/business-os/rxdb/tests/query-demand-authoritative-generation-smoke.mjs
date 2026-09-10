import assert from 'node:assert/strict';
import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

function makeStorage() {
  const documents = new Map();
  return {
    documents,
    databaseName: 'authority-generation',
    async bulkWrite(rows) {
      for (const row of rows) {
        const document = row?.document || row;
        documents.set(document.id, { ...document });
      }
    },
    async findDocumentsById(ids) {
      return Object.fromEntries(ids.filter((id) => documents.has(id)).map((id) => [documents.get(id).id, documents.get(id)]));
    },
    async queryDocuments(query, { matchesSelector } = {}) {
      return Array.from(documents.values()).filter((doc) => !matchesSelector || matchesSelector(doc, query.selector || {}));
    },
  };
}

function makeLoader({ storage, sidecar, generations, fetches }) {
  return createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'desktop_layout',
    schemaVersion: 1,
    requestQueryFetch: (...arguments_) => fetches[0](...arguments_),
    queryGeneration: () => generations.at(-1),
  });
}

// Strict reads require a generation and can never come from ordinary defaults.
{
  const loader = createQueryDemandLoader({
    storageCollection: makeStorage(),
    sidecar: createSidecarWithMemoryBackend({ databaseName: 'authority-missing-generation' }),
    collectionName: 'desktop_layout',
    schemaVersion: 1,
    requestQueryFetch: async () => ({ documents: [] }),
  });
  await assert.rejects(
    () => loader.resolveQuery({ selector: { id: 'layout' }, requireRevision: 'token-1' }),
    /QUERY_GENERATION_REQUIRED/,
  );
}

// A generation replacement rejects an in-flight strict read. The remote effect
// may already be committed; it must not be returned or turn into local null.
{
  const generations = ['gen-1'];
  const storage = makeStorage();
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'authority-replaced' });
  let releaseFetch;
  const fetches = [() => new Promise((resolve) => { releaseFetch = resolve; })];
  const loader = makeLoader({ storage, sidecar, generations, fetches });
  let replacedDuringMaterialize = false;
  const originalBulkWrite = storage.bulkWrite.bind(storage);
  storage.bulkWrite = async (rows) => {
    if (!replacedDuringMaterialize) {
      replacedDuringMaterialize = true;
      generations.push('gen-2');
    }
    return originalBulkWrite(rows);
  };
  const pending = loader.resolveQuery({ selector: { id: 'layout' }, requireRevision: 'same-token' });
  await new Promise((resolve) => setImmediate(resolve));
  releaseFetch({ documents: [{ id: 'layout', taskbar_pins: ['late'] }], authoritativeRevision: 'late' });
  await assert.rejects(() => pending, (error) => error?.code === 'QUERY_CANCELLED' && error?.generationChanged === true);
  assert.equal(storage.documents.has('layout'), true, 'committed materialization is not rolled back');
}

// Reconnect cancellation cannot invent a local null for a strict read.
{
  const generations = ['gen-1'];
  const storage = makeStorage();
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'authority-cancelled' });
  const fetches = [() => new Promise(() => {})];
  const loader = makeLoader({ storage, sidecar, generations, fetches });
  const pending = loader.resolveQuery({ selector: { id: 'layout' }, requireRevision: 'token-cancel' });
  await Promise.resolve();
  await loader.abortAllInFlight('peer-close');
  await assert.rejects(() => pending, /QUERY_CANCELLED.*peer-close/);
}

// Empty native answers are authority. Same token/generation dedups to cache;
// a new generation with a new token always takes a new strict request.
{
  const generations = ['gen-1'];
  const storage = makeStorage();
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'authority-empty' });
  let fetchCount = 0;
  const fetches = [async () => {
    fetchCount += 1;
    return { documents: [], authoritativeRevision: `native-${fetchCount}` };
  }];
  const loader = makeLoader({ storage, sidecar, generations, fetches });
  const query = { selector: { id: 'layout' }, requireRevision: 'token-1' };
  assert.deepEqual(await loader.resolveQuery(query), []);
  assert.equal(fetchCount, 1);
  assert.deepEqual(await loader.resolveQuery(query), []);
  assert.equal(fetchCount, 1, 'same token and generation must reuse authority');
  const windows = await sidecar.backend.scanQueryWindows();
  assert.equal(windows.length, 1);
  assert.deepEqual(windows[0].documentIds, []);
  assert.equal(windows[0].satisfiedRevision, 'token-1');
  assert.equal(windows[0].satisfiedGeneration, 'gen-1');
  generations.push('gen-2');
  await loader.resolveQuery({ ...query, requireRevision: 'token-2' });
  assert.equal(fetchCount, 2, 'generation/token replacement must fetch');
}

// Consumer cancellation cancels only its invocation and rejects instead of
// returning local documents.
{
  const generations = ['gen-1'];
  const storage = makeStorage();
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'authority-abort' });
  const controller = new AbortController();
  const fetches = [() => new Promise(() => {})];
  const loader = makeLoader({ storage, sidecar, generations, fetches });
  const pending = loader.resolveQuery(
    { selector: { id: 'layout' }, requireRevision: 'token-abort' },
    { signal: controller.signal },
  );
  await Promise.resolve();
  controller.abort();
  await assert.rejects(() => pending, /QUERY_CANCELLED.*consumer-abort/);
}

console.log('ctox-rxdb authoritative query generation smoke OK');
