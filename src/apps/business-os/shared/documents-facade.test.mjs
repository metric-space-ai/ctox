import test from 'node:test';
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
}

class MemoryCollection {
  constructor({ reverseFind = false } = {}) {
    this.rows = new Map();
    this.reverseFind = reverseFind;
  }

  async insert(document) {
    if (this.rows.has(document.id)) throw new Error(`duplicate: ${document.id}`);
    this.rows.set(document.id, structuredClone(document));
    return new MemoryDocument(this, document);
  }

  async bulkInsert(documents) {
    for (const document of documents) {
      if (this.rows.has(document.id)) throw new Error(`duplicate: ${document.id}`);
    }
    for (const document of documents) this.rows.set(document.id, structuredClone(document));
    return documents.map((document) => new MemoryDocument(this, document));
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
        if (blobId) documents = documents.filter((document) => document.blob_id === blobId);
        documents.sort((left, right) => left.idx - right.idx);
        if (this.reverseFind) documents.reverse();
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

test('createDocx rejects a partial idempotent dataset', async () => {
  const db = createMemoryDb();
  const documents = createDocumentsFacade({ db });
  const created = await documents.createDocx(createInput());
  db.collections.document_versions.rows.delete(created.versionId);

  await assert.rejects(
    documents.createDocx(createInput()),
    (error) => error.code === 'DOCUMENTS_PARTIAL_STATE',
  );
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
