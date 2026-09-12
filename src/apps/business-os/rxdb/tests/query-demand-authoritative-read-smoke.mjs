import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

const local = {
  id: 'command-1',
  status: 'pending_sync',
};
const storageCollection = {
  databaseName: 'require-revision-authoritative',
  async bulkWrite() {
    // Simulate an LWW conflict that leaves the existing local row untouched.
  },
  async queryDocuments() {
    return [local];
  },
  async findDocumentsById(ids) {
    return ids.includes(local.id) ? { [local.id]: local } : {};
  },
};
const loader = createQueryDemandLoader({
  storageCollection,
  sidecar: createSidecarWithMemoryBackend({
    databaseName: 'require-revision-authoritative',
  }),
  collectionName: 'business_commands',
  schemaVersion: 1,
  requestQueryFetch: async () => ({
    documents: [{ id: local.id, status: 'completed' }],
    authoritativeRevision: 'server-completed',
  }),
  queryGeneration: () => 'authoritative-generation',
});

const [result] = await loader.resolveQuery({
  selector: { id: local.id },
  requireRevision: 'command-status:command-1:1',
});

if (result?.status !== 'completed') {
  throw new Error(
    'strict demand read returned the stale local row instead of the authoritative server revision',
  );
}

console.log('ctox-rxdb authoritative demand read smoke OK');
