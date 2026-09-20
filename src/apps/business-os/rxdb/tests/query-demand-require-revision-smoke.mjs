import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

function makeStorageCollection(databaseName) {
  const documents = new Map();
  return {
    databaseName,
    async bulkWrite(rows) {
      for (const row of rows) {
        const document = row?.document || row;
        documents.set(document.id, { ...document });
      }
    },
    async queryDocuments(query, { matchesSelector, sortDocuments }) {
      let matches = Array.from(documents.values())
        .filter((document) => matchesSelector(document, query.selector || {}));
      matches = sortDocuments(matches, query.sort || []);
      if (query.skip > 0) matches = matches.slice(query.skip);
      if (Number.isFinite(query.limit)) matches = matches.slice(0, query.limit);
      return matches;
    },
    async findDocumentsById(ids) {
      return Object.fromEntries(
        ids.filter((id) => documents.has(id)).map((id) => [id, documents.get(id)]),
      );
    },
  };
}

// Same caller token: the first exec fetches and satisfies the token; the next
// exec must hit cache even though the server revision is a different value.
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'require-revision-token' });
  const storageCollection = makeStorageCollection('require-revision-token');
  const fetchedFingerprints = [];
  let fetches = 0;
  const loader = createQueryDemandLoader({
    storageCollection,
    sidecar,
    collectionName: 'spreadsheet_rows',
    schemaVersion: 1,
    queryGeneration: () => 'spreadsheet-authority-generation',
    requestQueryFetch: async (request) => {
      fetches += 1;
      fetchedFingerprints.push(request.queryFingerprint);
      return {
        documents: [{ id: 'row-1', spreadsheet_id: 'sheet-1', fetch: fetches }],
        // This mirrors the vestigial server echo and intentionally can never
        // equal the caller's spreadsheet revision token.
        authoritativeRevision: request.queryFingerprint,
      };
    },
  });
  const selector = { spreadsheet_id: 'sheet-1' };
  const firstToken = 'spreadsheet:sheet-1:1000';
  const secondToken = 'spreadsheet:sheet-1:2000';

  await loader.resolveQuery({ selector, requireRevision: firstToken });
  assert(fetches === 1, 'first token must fetch once (got ' + fetches + ')');
  let [cached] = await sidecar.backend.scanQueryWindows();
  assert(cached.satisfiedRevision === firstToken, 'first caller token was not persisted as satisfied');
  assert(
    cached.authoritativeRevision !== firstToken,
    'smoke fixture must keep server digest and caller token in different value domains',
  );

  await loader.resolveQuery({ selector, requireRevision: firstToken });
  assert(fetches === 1, 'same token must be a cache hit (got ' + fetches + ' fetches)');

  await loader.resolveQuery({ selector, requireRevision: secondToken });
  assert(fetches === 2, 'token change must cause exactly one refetch (got ' + fetches + ')');
  [cached] = await sidecar.backend.scanQueryWindows();
  assert(cached.satisfiedRevision === secondToken, 'changed caller token was not persisted as satisfied');

  await loader.resolveQuery({ selector, requireRevision: secondToken });
  assert(fetches === 2, 'changed token must converge back to cache hits (got ' + fetches + ' fetches)');
  assert(
    fetchedFingerprints.length === 2 && fetchedFingerprints[0] === fetchedFingerprints[1],
    'requireRevision token changes must not alter the query fingerprint',
  );
}

// No caller token: preserve the existing miss-then-hit behavior without a
// revision gate.
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'require-revision-absent' });
  const storageCollection = makeStorageCollection('require-revision-absent');
  let fetches = 0;
  const loader = createQueryDemandLoader({
    storageCollection,
    sidecar,
    collectionName: 'spreadsheet_rows',
    schemaVersion: 1,
    requestQueryFetch: async (request) => {
      fetches += 1;
      return {
        documents: [{ id: 'row-2', spreadsheet_id: 'sheet-2' }],
        authoritativeRevision: request.queryFingerprint,
      };
    },
  });
  const query = { selector: { spreadsheet_id: 'sheet-2' } };

  await loader.resolveQuery(query);
  await loader.resolveQuery(query);
  assert(fetches === 1, 'query without requireRevision must retain cache-hit behavior (got ' + fetches + ')');
}

console.log('ctox-rxdb query-demand requireRevision smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
