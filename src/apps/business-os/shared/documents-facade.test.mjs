import test from 'node:test';
import { CollectionSyncRegistry } from './sync-collection-registry.js';
import assert from 'node:assert/strict';

import {
  createDocumentsFacade,
  DOCX_MIME_TYPE,
} from './documents.js';

class MemoryDocument {
  constructor(collection, data) {
    this.collection = collection;
    this._data = structuredClone(data);
    Object.assign(this, this._data);
  }

  toJSON() {
    return structuredClone(this._data);
  }

  async remove() {
    this.collection.rows.delete(this._data.id);
  }

  async incrementalPatch(patch) {
    this._data = { ...this._data, ...structuredClone(patch) };
    Object.assign(this, this._data);
    this.collection.rows.set(this._data.id, structuredClone(this._data));
    return this;
  }
}

class MemoryCollection {
  constructor({ reverseFind = false } = {}) {
    this.rows = new Map();
    this.reverseFind = reverseFind;
    this.bulkInsertCalls = 0;
  }

  async insert(document) {
    if (this.rows.has(document.id)) throw new Error(`duplicate: ${document.id}`);
    this.rows.set(document.id, structuredClone(document));
    return new MemoryDocument(this, document);
  }

  async upsert(document) {
    this.rows.set(document.id, structuredClone(document));
    return new MemoryDocument(this, document);
  }

  async bulkInsert(documents) {
    this.bulkInsertCalls += 1;
    for (const document of documents) {
      if (this.rows.has(document.id)) throw new Error(`duplicate: ${document.id}`);
    }
    for (const document of documents) this.rows.set(document.id, structuredClone(document));
    return {
      success: documents.map((document) => new MemoryDocument(this, document)),
      error: [],
    };
  }

  findOne(id) {
    return {
      exec: async () => {
        const document = this.rows.get(id);
        return document ? new MemoryDocument(this, document) : null;
      },
    };
  }

  find(query = {}) {
    return {
      exec: async () => {
        let documents = Array.from(this.rows.values());
        const blobId = query.selector?.blob_id;
        const documentId = query.selector?.document_id;
        if (blobId) documents = documents.filter((document) => document.blob_id === blobId);
        if (documentId) {
          documents = documents.filter((document) => document.document_id === documentId);
        }
        const sortField = Object.keys(query.sort?.[0] || {})[0] || 'idx';
        documents.sort((left, right) => Number(left[sortField]) - Number(right[sortField]));
        if (this.reverseFind && blobId) documents.reverse();
        return documents.map((document) => new MemoryDocument(this, document));
      },
    };
  }
}

function createMemoryDb({ reverseChunks = false, omit = '' } = {}) {
  const collections = {
    documents: new MemoryCollection(),
    document_versions: new MemoryCollection(),
    document_blob_chunks: new MemoryCollection({ reverseFind: reverseChunks }),
  };
  if (omit) delete collections[omit];
  return {
    collections,
    collection(name) {
      return collections[name] || null;
    },
  };
}

function createInput(overrides = {}) {
  return {
    filename: 'candidate-profile.docx',
    mimeType: DOCX_MIME_TYPE,
    bytes: new Uint8Array([0x50, 0x4b, 0x03, 0x04, 10, 20, 30, 40]),
    idempotencyKey: 'candidate-profile:v1',
    linkedRecords: [{ collection: 'candidates', id: 'candidate_17' }],
    templateRef: { template_id: 'profile-standard', version: 3 },
    provenance: { app_id: 'profile-builder', source: 'approved-template' },
    ...overrides,
  };
}

test('createDocx and loadVersion roundtrip bytes and generic provenance', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db, now: () => 123456 });
  const created = await documents.createDocx(createInput());
  const loaded = await documents.loadVersion({
    documentId: created.documentId,
    versionId: created.versionId,
    expectedSha256: created.sha256,
  });

  assert.deepEqual(loaded.bytes, createInput().bytes);
  assert.equal(loaded.document.mime_type, DOCX_MIME_TYPE);
  assert.deepEqual(loaded.document.linked_records, createInput().linkedRecords);
  assert.deepEqual(loaded.document.template_ref, createInput().templateRef);
  assert.deepEqual(loaded.version.provenance, createInput().provenance);
});

test('createMailMerge appends two export runs with the same logicalDocumentKey as two versions of one document', async () => {
  const db = createMemoryDb();
  let timestamp = 123456;
  const documents = createDocumentsFacade({ db, now: () => timestamp++ });
  const base = {
    filename: 'sommeraktion-serienbrief.docx',
    title: 'Sommeraktion Serienbrief',
    mimeType: DOCX_MIME_TYPE,
    logicalDocumentKey: 'sellify:campaign-mail-merge:selection-7:template-2',
    variants: [{
      recipientId: 'person-1',
      recipientLabel: 'Ada Lovelace',
      filename: 'sommeraktion-ada-lovelace.docx',
      bytes: new Uint8Array([0x50, 0x4b, 1]),
    }],
  };

  const first = await documents.createMailMerge({ ...base, idempotencyKey: 'export-run-1' });
  const second = await documents.createMailMerge({
    ...base,
    idempotencyKey: 'export-run-2',
    variants: [{ ...base.variants[0], bytes: new Uint8Array([0x50, 0x4b, 2]) }],
  });

  assert.equal(first.documentId, second.documentId);
  assert.notEqual(first.versionId, second.versionId);
  assert.equal(db.collections.documents.rows.size, 1);
  assert.equal(db.collections.document_versions.rows.size, 2);
  assert.deepEqual(
    Array.from(db.collections.document_versions.rows.values()).map((version) => version.version),
    [1, 2],
  );
  assert.equal(second.document.current_version_id, second.versionId);
  assert.equal(second.document.mail_merge.version_count, 2);
});

test('createMailMerge stores N recipient variants in one export version', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db, now: () => 123456 });
  const input = {
    filename: 'sommeraktion-serienbrief.docx',
    title: 'Sommeraktion Serienbrief',
    mimeType: DOCX_MIME_TYPE,
    idempotencyKey: 'summer-mail-merge:v1',
    logicalDocumentKey: 'sellify:campaign-mail-merge:campaign-7:letter-standard',
    indexText: 'Kosten senken Beschaffung',
    tags: ['kampagne', 'brief'],
    linkedRecords: [{ collection: 'campaigns', id: 'campaign-7' }],
    templateRef: { template_id: 'letter-standard', version: 2 },
    provenance: { app_id: 'crm', source: 'campaign-mail-merge' },
    variants: [
      {
        recipientId: 'person-1',
        recipientLabel: 'Ada Lovelace',
        filename: 'sommeraktion-ada-lovelace.docx',
        bytes: new Uint8Array([0x50, 0x4b, 1]),
        indexText: 'Persönlicher Brief an Ada',
        linkedRecords: [{ collection: 'people', id: 'person-1' }],
        provenance: { merged_fields: ['name'] },
      },
      {
        recipientId: 'person-2',
        recipientLabel: 'Grace Hopper',
        filename: 'sommeraktion-grace-hopper.docx',
        bytes: new Uint8Array([0x50, 0x4b, 2]),
        linkedRecords: [{ collection: 'people', id: 'person-2' }],
        provenance: { merged_fields: ['name'] },
      },
    ],
  };

  const created = await documents.createMailMerge(input);
  const retry = await documents.createMailMerge(input);
  const second = await documents.loadVersion({
    documentId: created.documentId,
    versionId: created.versionId,
    recipientId: 'person-2',
    expectedSha256: created.variants[1].sha256,
  });

  assert.equal(db.collections.documents.rows.size, 1);
  assert.equal(db.collections.document_versions.rows.size, 1);
  assert.equal(created.versions.length, 1);
  assert.equal(created.recipientCount, 2);
  assert.equal(created.document.document_type, 'mail_merge');
  assert.equal(
    created.document.mail_merge.bundle_id,
    'sellify:campaign-mail-merge:campaign-7:letter-standard',
  );
  assert.equal(created.document.mail_merge.recipient_count, 2);
  assert.deepEqual(created.document.tags, ['kampagne', 'brief']);
  assert.match(created.document.index_text, /Kosten senken Beschaffung/);
  assert.match(created.document.index_text, /Persönlicher Brief an Ada/);
  assert.equal(created.document.provenance.app_id, 'crm');
  assert.deepEqual(created.version.mail_merge_variants.map(({ recipientLabel }) => recipientLabel), [
    'Ada Lovelace',
    'Grace Hopper',
  ]);
  assert.deepEqual(second.bytes, input.variants[1].bytes);
  assert.equal(second.recipientId, 'person-2');
  assert.equal(second.recipientLabel, 'Grace Hopper');
  assert.equal(retry.idempotent, true);
  assert.equal(db.collections.documents.rows.size, 1);
  assert.equal(db.collections.document_versions.rows.size, 1);
});

test('createMailMerge rejects duplicate recipients and empty variant sets', async () => {
  const documents = createDocumentsFacade({ db: createMemoryDb() });
  const base = {
    filename: 'mail-merge.docx',
    mimeType: DOCX_MIME_TYPE,
    idempotencyKey: 'mail-merge-invalid',
  };
  await assert.rejects(
    documents.createMailMerge({ ...base, variants: [] }),
    (error) => error.code === 'DOCUMENTS_INVALID_INPUT',
  );
  await assert.rejects(
    documents.createMailMerge({
      ...base,
      variants: [
        {
          recipientId: 'person-1',
          recipientLabel: 'First',
          filename: 'first.docx',
          bytes: new Uint8Array([1]),
        },
        {
          recipientId: 'person-1',
          recipientLabel: 'Duplicate',
          filename: 'duplicate.docx',
          bytes: new Uint8Array([2]),
        },
      ],
    }),
    (error) => error.code === 'DOCUMENTS_INVALID_INPUT',
  );
});

test('createMailMerge replaying the same idempotencyKey does not add a version', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db });
  const input = {
    filename: 'stable-mail-merge.docx',
    mimeType: DOCX_MIME_TYPE,
    idempotencyKey: 'stable-mail-merge:v1',
    variants: [
      {
        recipientId: 'person-1',
        recipientLabel: 'Ada Lovelace',
        filename: 'ada.docx',
        bytes: new Uint8Array([0x50, 0x4b, 1]),
      },
      {
        recipientId: 'person-2',
        recipientLabel: 'Grace Hopper',
        filename: 'grace.docx',
        bytes: new Uint8Array([0x50, 0x4b, 2]),
      },
    ],
  };
  const created = await documents.createMailMerge(input);
  const retry = await documents.createMailMerge({
    ...input,
    variants: input.variants.map((variant, index) => ({
      ...variant,
      bytes: new Uint8Array([0x50, 0x4b, 90 + index]),
    })),
  });
  const loaded = await documents.loadVersion({
    documentId: retry.documentId,
    versionId: retry.versions[0].versionId,
    expectedSha256: created.versions[0].sha256,
  });

  assert.equal(retry.idempotent, true);
  assert.equal(retry.versionId, created.versionId);
  assert.equal(retry.versions[0].sha256, created.versions[0].sha256);
  assert.deepEqual(loaded.bytes, input.variants[0].bytes);
  assert.equal(db.collections.documents.rows.size, 1);
  assert.equal(db.collections.document_versions.rows.size, 1);
});

test('createMailMerge repairs missing recipient variant blobs without duplicating the parent', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db });
  const input = {
    filename: 'repair-mail-merge.docx',
    mimeType: DOCX_MIME_TYPE,
    idempotencyKey: 'repair-mail-merge:v1',
    variants: [
      {
        recipientId: 'person-1',
        recipientLabel: 'Ada Lovelace',
        filename: 'ada.docx',
        bytes: new Uint8Array([0x50, 0x4b, 1]),
      },
      {
        recipientId: 'person-2',
        recipientLabel: 'Grace Hopper',
        filename: 'grace.docx',
        bytes: new Uint8Array([0x50, 0x4b, 2]),
      },
    ],
  };
  const created = await documents.createMailMerge(input);
  const secondBlobId = created.variants[1].blobId;
  for (const [chunkId, chunk] of db.collections.document_blob_chunks.rows) {
    if (chunk.blob_id === secondBlobId) db.collections.document_blob_chunks.rows.delete(chunkId);
  }

  const repaired = await documents.createMailMerge(input);

  assert.equal(repaired.idempotent, true);
  assert.equal(db.collections.documents.rows.size, 1);
  assert.equal(db.collections.document_versions.rows.size, 1);
  assert.ok(Array.from(db.collections.document_blob_chunks.rows.values()).some(
    (chunk) => chunk.blob_id === secondBlobId,
  ));
});

test('createMailMerge resumes partial recipient data when the parent was not stored', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db });
  const input = {
    filename: 'resume-mail-merge.docx',
    mimeType: DOCX_MIME_TYPE,
    idempotencyKey: 'resume-mail-merge:v1',
    creatorAppId: 'crm',
    variants: [
      {
        recipientId: 'person-1',
        recipientLabel: 'Ada Lovelace',
        filename: 'ada.docx',
        bytes: new Uint8Array([0x50, 0x4b, 1]),
      },
      {
        recipientId: 'person-2',
        recipientLabel: 'Grace Hopper',
        filename: 'grace.docx',
        bytes: new Uint8Array([0x50, 0x4b, 2]),
      },
    ],
  };
  const created = await documents.createMailMerge(input);
  db.collections.documents.rows.delete(created.documentId);

  const resumed = await documents.createMailMerge(input);

  assert.equal(resumed.idempotent, true);
  assert.equal(resumed.document.provenance.app_id, 'crm');
  assert.equal(db.collections.documents.rows.size, 1);
  assert.equal(db.collections.document_versions.rows.size, 1);
});

test('createMailMerge repairs stale recipient hashes when partial blobs are missing', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db, now: () => 123456 });
  const input = {
    filename: 'repair-partial-mail-merge.docx',
    mimeType: DOCX_MIME_TYPE,
    idempotencyKey: 'repair-partial-mail-merge:v1',
    variants: [
      {
        recipientId: 'person-1',
        recipientLabel: 'Ada Lovelace',
        filename: 'ada.docx',
        bytes: new Uint8Array([0x50, 0x4b, 1]),
      },
    ],
  };
  const created = await documents.createMailMerge(input);
  db.collections.documents.rows.delete(created.documentId);
  const versionId = created.versions[0].versionId;
  const version = db.collections.document_versions.rows.get(versionId);
  db.collections.document_versions.rows.set(versionId, {
    ...version,
    source_sha256: '0'.repeat(64),
  });
  for (const [chunkId, chunk] of db.collections.document_blob_chunks.rows) {
    if (chunk.version_id === versionId) db.collections.document_blob_chunks.rows.delete(chunkId);
  }

  const repaired = await documents.createMailMerge(input);
  const repairedVersion = db.collections.document_versions.rows.get(versionId);

  assert.equal(repaired.idempotent, true);
  assert.equal(repairedVersion.source_sha256, created.versions[0].sha256);
  assert.equal(db.collections.document_blob_chunks.rows.size, 1);
  assert.equal(db.collections.documents.rows.size, 1);
});

test('createDocx answers repeated idempotencyKey calls without duplicates', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db, now: () => 123456 });

  const first = await documents.createDocx(createInput());
  const counts = Object.fromEntries(
    Object.entries(db.collections).map(([name, collection]) => [name, collection.rows.size]),
  );
  const second = await documents.createDocx(createInput());

  assert.equal(first.idempotent, false);
  assert.equal(second.idempotent, true);
  assert.equal(second.documentId, first.documentId);
  assert.equal(second.versionId, first.versionId);
  assert.deepEqual(
    Object.fromEntries(Object.entries(db.collections).map(([name, collection]) => [name, collection.rows.size])),
    counts,
  );
});

test('createDocx returns and reuses the original complete payload for an idempotencyKey', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db, now: () => 123456 });
  const originalInput = createInput();
  const first = await documents.createDocx(originalInput);
  const retry = await documents.createDocx(createInput({
    bytes: new Uint8Array([...originalInput.bytes, 99]),
  }));
  const loaded = await documents.loadVersion({
    documentId: retry.documentId,
    expectedSha256: first.sha256,
  });

  assert.equal(retry.idempotent, true);
  assert.equal(retry.sha256, first.sha256);
  assert.deepEqual(loaded.bytes, originalInput.bytes);
});

test('idempotent retry requeues stored chunks before flushing replication', async () => {
  const calls = [];
  const db = createMemoryDb();
  const originalUpsert = db.collections.document_blob_chunks.upsert.bind(
    db.collections.document_blob_chunks,
  );
  db.collections.document_blob_chunks.upsert = async (document) => {
    calls.push(`requeue:${document.id}`);
    return originalUpsert(document);
  };
  const documents = createDocumentsFacade({
    db,
    sync: {
      async leaseCollection() {
        return {
          bridge: {
            state: {
              async pushToRemotePeers() { calls.push('push'); },
            },
          },
          async release() { calls.push('release'); },
        };
      },
    },
  });
  const first = await documents.createDocx(createInput());
  calls.length = 0;

  const retry = await documents.createDocx(createInput());

  assert.equal(retry.idempotent, true);
  assert.equal(retry.sha256, first.sha256);
  assert.deepEqual(calls, [
    `requeue:${first.version.blob_id}_0`,
    'push',
    'push',
    'push',
    'release',
    'release',
    'release',
  ]);
});

test('loadVersion rejects a SHA-256 mismatch', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db });
  const created = await documents.createDocx(createInput());

  await assert.rejects(
    documents.loadVersion({
      documentId: created.documentId,
      versionId: created.versionId,
      expectedSha256: '0'.repeat(64),
    }),
    (error) => error.code === 'DOCUMENTS_HASH_MISMATCH',
  );
});

test('loadVersion orders chunk rows by idx before decoding', async () => {
  const db = createMemoryDb({ reverseChunks: true });
  const bytes = new Uint8Array(400000);
  for (let index = 0; index < bytes.length; index += 1) bytes[index] = index % 251;
  const documents = createDocumentsFacade({ db });
  const created = await documents.createDocx(createInput({
    bytes,
    idempotencyKey: 'large-document:v1',
  }));

  assert.ok(db.collections.document_blob_chunks.rows.size > 1);
  const loaded = await documents.loadVersion({
    documentId: created.documentId,
    expectedSha256: created.sha256,
  });
  assert.deepEqual(loaded.bytes, bytes);
});

test('facade fails closed when a required collection is missing', async () => {
  const documents = createDocumentsFacade({
    db: createMemoryDb({ omit: 'document_blob_chunks' }),
  });

  await assert.rejects(
    documents.createDocx(createInput()),
    (error) => error.code === 'DOCUMENTS_COLLECTIONS_UNAVAILABLE',
  );
  await assert.rejects(
    documents.loadVersion({
      documentId: 'doc_missing',
      expectedSha256: '0'.repeat(64),
    }),
    (error) => error.code === 'DOCUMENTS_COLLECTIONS_UNAVAILABLE',
  );
});

test('createDocx repairs a matching partial idempotent dataset', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db });
  const created = await documents.createDocx(createInput());
  db.collections.document_versions.rows.delete(created.versionId);

  const repaired = await documents.createDocx(createInput());

  assert.equal(repaired.idempotent, false);
  assert.ok(db.collections.document_versions.rows.has(created.versionId));
  assert.equal(db.collections.document_blob_chunks.bulkInsertCalls, 1);
});

test('createDocx refreshes a stale source hash when chunkless metadata is otherwise identical', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db, now: () => 123456 });
  const created = await documents.createDocx(createInput());
  db.collections.document_blob_chunks.rows.clear();
  db.collections.documents.rows.get(created.documentId).source_sha256 = '0'.repeat(64);
  db.collections.document_versions.rows.get(created.versionId).source_sha256 = '0'.repeat(64);

  const repaired = await documents.createDocx(createInput());

  assert.equal(repaired.idempotent, false);
  assert.equal(db.collections.documents.rows.get(created.documentId).source_sha256, repaired.sha256);
  assert.equal(db.collections.document_versions.rows.get(created.versionId).source_sha256, repaired.sha256);
  assert.equal(db.collections.document_blob_chunks.rows.size, 1);
});

test('createDocx rejects a conflicting partial idempotent dataset', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db });
  const created = await documents.createDocx(createInput());
  db.collections.document_versions.rows.delete(created.versionId);
  db.collections.documents.rows.get(created.documentId).filename = 'different.docx';

  await assert.rejects(
    documents.createDocx(createInput()),
    (error) => error.code === 'DOCUMENTS_IDEMPOTENCY_CONFLICT',
  );
});

test('createDocx leases and flushes document metadata, versions, and blobs', async () => {
  const calls = [];
  const replication = {
    async awaitInitialReplication() { calls.push('initial'); },
    async awaitInSync() { calls.push('in-sync'); },
    async pushDocumentsToRemotePeers(documents) { calls.push(`push-direct:${documents.length}`); },
  };
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    sync: {
      async leaseCollection(collection, reason) {
        calls.push(`lease:${collection}:${reason}`);
        return {
          collection,
          bridge: { state: replication },
          async release() { calls.push('release'); },
        };
      },
    },
  });

  await documents.createDocx(createInput());

  assert.deepEqual(calls, [
    'lease:documents:documents-write:documents',
    'in-sync',
    'lease:document_versions:documents-write:document_versions',
    'in-sync',
    'lease:document_blob_chunks:documents-write:document_blob_chunks',
    'in-sync',
    'push-direct:1',
    'push-direct:1',
    'push-direct:1',
    'release',
    'release',
    'release',
  ]);
});

test('createDocx retries explicitly retryable native peer lease failures', async () => {
  const calls = [];
  const retryable = Object.assign(new Error('native peer is reconnecting'), {
    code: 'peer_connect_timeout',
    retryable: true,
  });
  let attempts = 0;
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    sync: {
      async leaseCollection() {
        attempts += 1;
        calls.push(`lease:${attempts}`);
        if (attempts < 3) throw retryable;
        return {
          bridge: { state: { async pushToRemotePeers() { calls.push('push'); } } },
          async release() { calls.push('release'); },
        };
      },
    },
  });

  const created = await documents.createDocx(createInput());

  assert.equal(created.idempotent, false);
  assert.deepEqual(calls, [
    'lease:1',
    'lease:2',
    'lease:3',
    'lease:4',
    'lease:5',
    'push',
    'push',
    'push',
    'release',
    'release',
    'release',
  ]);
});

test('createDocx replaces a follower lease with a direct bridge before publishing dependencies', async () => {
  const calls = [];
  const registry = new CollectionSyncRegistry();
  const replication = {
    getTransportStatus() { return { activePeerCount: 1 }; },
    async pushDocumentsToRemotePeers(documents) {
      calls.push(`push:${documents.length}`);
      return true;
    },
  };
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    sync: {
      async leaseCollection(collection) {
        calls.push(`lease:${collection}`);
        registry.set(collection, Promise.resolve({ mode: 'follower', state: null }));
        return registry.acquire(collection, 'document-test', async () => { calls.push(`release:${collection}`); });
      },
      async startCollection(collection, options) {
        calls.push(`direct:${collection}:${options?.forceDirect === true}`);
        const bridge = { mode: 'leader', state: replication };
        registry.set(collection, Promise.resolve(bridge));
        return bridge;
      },
    },
  });

  await documents.createDocx(createInput());

  assert.deepEqual(calls, [
    'lease:documents',
    'direct:documents:true',
    'lease:document_versions',
    'direct:document_versions:true',
    'lease:document_blob_chunks',
    'direct:document_blob_chunks:true',
    'push:1',
    'push:1',
    'push:1',
    'release:document_blob_chunks',
    'release:document_versions',
    'release:documents',
  ]);
});

test('createDocx does not retry non-retryable lease failures', async () => {
  let attempts = 0;
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    sync: {
      async leaseCollection() {
        attempts += 1;
        throw Object.assign(new Error('permission denied'), { code: 'permission_denied' });
      },
    },
  });

  await assert.rejects(documents.createDocx(createInput()), /permission denied/);
  assert.equal(attempts, 1);
});

test('createDocx waits for an open native peer before writing chunks', async () => {
  const calls = [];
  const replication = {
    getTransportStatus() {
      return { activePeerCount: 1 };
    },
    async pushToRemotePeers() { calls.push('push'); },
  };
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    sync: {
      async leaseCollection() {
        calls.push('lease');
        return {
          bridge: { state: replication },
          async release() { calls.push('release'); },
        };
      },
    },
  });

  await documents.createDocx(createInput());

  assert.deepEqual(calls, [
    'lease',
    'lease',
    'lease',
    'push',
    'push',
    'push',
    'release',
    'release',
    'release',
  ]);
});

test('createDocx rejects row-level bulk chunk errors before metadata is written', async () => {
  const db = createMemoryDb();
  db.collections.document_blob_chunks.bulkInsert = async () => ({
    success: [],
    error: [{ status: 422 }],
  });
  const documents = createDocumentsFacade({ db });

  await assert.rejects(
    documents.createDocx(createInput()),
    (error) => error.code === 'DOCUMENTS_BLOB_WRITE_FAILED',
  );
  assert.equal(db.collections.documents.rows.size, 0);
  assert.equal(db.collections.document_versions.rows.size, 0);
});

test('open dispatches Documents through the generic app launcher with record args', async () => {
  const calls = [];
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    openApp: async (...args) => {
      calls.push(args);
      return 'window-documents';
    },
  });

  const result = await documents.open({ documentId: 'doc_17', versionId: 'doc_17_v3' });

  assert.equal(result, 'window-documents');
  assert.deepEqual(calls, [[
    'documents',
    { args: { record: 'doc_17', version: 'doc_17_v3' } },
  ]]);
});

test('open rejects a missing Documents app instead of reporting false success', async () => {
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    openApp: async () => null,
  });

  await assert.rejects(
    documents.open({ documentId: 'doc_18', versionId: 'doc_18_v1' }),
    (error) => error.code === 'DOCUMENTS_LAUNCH_UNAVAILABLE',
  );
});

test('open supports a resolved generic Documents workspace id', async () => {
  const calls = [];
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    appId: 'documents-workspace',
    openApp: async (...args) => {
      calls.push(args);
      return 'window-documents-workspace';
    },
  });

  assert.equal(
    await documents.open({ documentId: 'doc_19' }),
    'window-documents-workspace',
  );
  assert.deepEqual(calls, [[
    'documents-workspace',
    { args: { record: 'doc_19' } },
  ]]);
});

test('creator app provenance and Documents workspace launch target remain independent', async () => {
  const calls = [];
  const db = createMemoryDb();
  const documents = createDocumentsFacade({
    db,
    creatorAppId: 'customer-workspace',
    workspaceAppId: 'documents-workspace',
    openApp: async (...args) => {
      calls.push(args);
      return 'window-documents-workspace';
    },
  });

  const created = await documents.createDocx(createInput({
    idempotencyKey: 'customer-workspace:document:v1',
    provenance: {},
  }));
  const stored = db.collections.documents.rows.get(created.documentId);
  await documents.open({ documentId: created.documentId });

  assert.equal(stored.provenance.app_id, 'customer-workspace');
  assert.deepEqual(calls, [[
    'documents-workspace',
    { args: { record: created.documentId } },
  ]]);
});

test('open resolves the Documents workspace id at launch time', async () => {
  const calls = [];
  let workspaceId = 'documents';
  const documents = createDocumentsFacade({
    db: createMemoryDb(),
    appId: () => workspaceId,
    async openApp(appId, options) {
      calls.push({ appId, options });
      return 'window-documents';
    },
  });
  workspaceId = 'documents-workspace';

  await documents.open({ documentId: 'doc-1', versionId: 'version-1' });

  assert.deepEqual(calls, [{
    appId: 'documents-workspace',
    options: { args: { record: 'doc-1', version: 'version-1' } },
  }]);
});
