export const DOCX_MIME_TYPE = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
export const DOCUMENT_BLOB_CHUNK_BASE64_SIZE = 256000;
const DOCUMENT_SYNC_LEASE_ATTEMPTS = 4;
const DOCUMENT_SYNC_LEASE_RETRY_DELAY_MS = 1_000;
const DOCUMENT_SYNC_PAYLOAD = Symbol('ctox.documents.sync-payload');

const DOCUMENT_COLLECTION_NAMES = [
  'documents',
  'document_versions',
  'document_blob_chunks',
];

export function createDocumentsFacade({
  db,
  openApp,
  sync,
  appId = 'documents',
  creatorAppId = appId,
  workspaceAppId = appId,
  crypto: cryptoProvider = globalThis.crypto,
  now = () => Date.now(),
} = {}) {
  const pendingCreates = new Map();

  return Object.freeze({
    async loadVersion(request = {}) {
      const collections = requireDocumentCollections(db);
      return loadStoredVersion(collections, request, cryptoProvider);
    },

    async createDocx(input = {}) {
      const normalized = await normalizeCreateDocxInput(
        input,
        cryptoProvider,
        resolveFacadeAppId(creatorAppId),
      );
      const fingerprint = canonicalJson({
        filename: normalized.filename,
        title: normalized.title,
        ownerId: normalized.ownerId,
        sha256: normalized.sha256,
        indexText: normalized.indexText,
        tags: normalized.tags,
        linkedRecords: normalized.linkedRecords,
        templateRef: normalized.templateRef,
        provenance: normalized.provenance,
      });
      const pending = pendingCreates.get(normalized.idempotencyKey);
      if (pending) {
        if (pending.fingerprint !== fingerprint) {
          throw documentsError(
            'DOCUMENTS_IDEMPOTENCY_CONFLICT',
            'idempotencyKey is already in use for a different DOCX payload.',
          );
        }
        return pending.promise;
      }

      const collections = requireDocumentCollections(db, { write: true });
      const promise = withDocumentSyncLeases(sync, collections, () => createStoredDocx({
        collections,
        input: normalized,
        cryptoProvider,
        now,
      }));
      pendingCreates.set(normalized.idempotencyKey, { fingerprint, promise });
      try {
        return await promise;
      } finally {
        if (pendingCreates.get(normalized.idempotencyKey)?.promise === promise) {
          pendingCreates.delete(normalized.idempotencyKey);
        }
      }
    },

    async createMailMerge(input = {}) {
      const normalized = await normalizeCreateMailMergeInput(
        input,
        cryptoProvider,
        resolveFacadeAppId(creatorAppId),
      );
      const fingerprint = canonicalJson({
        logicalDocumentKey: normalized.logicalDocumentKey,
        filename: normalized.filename,
        title: normalized.title,
        ownerId: normalized.ownerId,
        indexText: normalized.indexText,
        tags: normalized.tags,
        linkedRecords: normalized.linkedRecords,
        templateRef: normalized.templateRef,
        provenance: normalized.provenance,
        failures: normalized.failures,
        variants: normalized.variants.map((variant) => ({
          recipientId: variant.recipientId,
          recipientLabel: variant.recipientLabel,
          filename: variant.filename,
          sha256: variant.sha256,
          indexText: variant.indexText,
          linkedRecords: variant.linkedRecords,
          provenance: variant.provenance,
        })),
      });
      const pendingKey = `mail-merge:${normalized.idempotencyKey}`;
      const pending = pendingCreates.get(pendingKey);
      if (pending) {
        if (pending.fingerprint !== fingerprint) {
          throw documentsError(
            'DOCUMENTS_IDEMPOTENCY_CONFLICT',
            'idempotencyKey is already in use for a different mail merge payload.',
          );
        }
        return pending.promise;
      }

      const collections = requireDocumentCollections(db, { write: true });
      const promise = withDocumentSyncLeases(sync, collections, () => createStoredMailMerge({
        collections,
        input: normalized,
        cryptoProvider,
        now,
      }));
      pendingCreates.set(pendingKey, { fingerprint, promise });
      try {
        return await promise;
      } finally {
        if (pendingCreates.get(pendingKey)?.promise === promise) {
          pendingCreates.delete(pendingKey);
        }
      }
    },

    async open({ documentId, versionId } = {}) {
      const record = requireId(documentId, 'documentId');
      const version = versionId == null || versionId === ''
        ? ''
        : requireId(versionId, 'versionId');
      if (typeof openApp !== 'function') {
        throw documentsError(
          'DOCUMENTS_LAUNCH_UNAVAILABLE',
          'The Business OS app launcher is unavailable.',
        );
      }
      const resolvedAppId = typeof workspaceAppId === 'function' ? workspaceAppId() : workspaceAppId;
      const launched = await openApp(requireId(resolvedAppId, 'appId'), {
        args: {
          record,
          ...(version ? { version } : {}),
        },
      });
      if (!launched) {
        throw documentsError(
          'DOCUMENTS_LAUNCH_UNAVAILABLE',
          'The Documents app is not installed or available in this workspace.',
        );
      }
      return launched;
    },
  });
}

async function createStoredMailMerge({ collections, input, cryptoProvider, now }) {
  const bundleKey = input.logicalDocumentKey || input.idempotencyKey;
  const idHash = await sha256Hex(
    new TextEncoder().encode(`ctox-documents:create-mail-merge:${bundleKey}`),
    cryptoProvider,
  );
  const documentId = `doc_${idHash.slice(0, 40)}`;
  const createdAtMs = finiteTimestamp(now());
  const existingDocumentDoc = await findOne(collections.documents, documentId);
  let existingDocument = existingDocumentDoc ? documentJson(existingDocumentDoc) : null;
  const existingVersionDocs = await findDocumentVersions(collections.document_versions, documentId);
  const existingVersions = existingVersionDocs.map(documentJson);
  const replayVersionIndex = existingVersions.findIndex(
    (version) => version.idempotency_key === input.idempotencyKey,
  );
  const isReplay = replayVersionIndex >= 0;
  const versionNumber = isReplay
    ? Number(existingVersions[replayVersionIndex].version)
    : nextDocumentVersionNumber(existingVersions);
  const expected = buildStoredMailMergeVersion({
    documentId,
    versionNumber,
    input,
    createdAtMs,
  });
  const tracker = { chunks: [], versions: [], document: false };
  let stored;

  try {
    stored = await resolveStoredMailMergeVersion({
      collections,
      document: existingDocument || buildStoredMailMergeDocument({
        documentId,
        input,
        version: expected.version,
        versionCount: existingVersions.length || 1,
        createdAtMs,
      }),
      expected,
      existingVersionDoc: isReplay ? existingVersionDocs[replayVersionIndex] : null,
      cryptoProvider,
      tracker,
    });

    if (existingDocument) {
      assertStoredMailMergeBundle(existingDocument, { documentId, input, bundleKey });
      if (!isReplay) {
        const updatedDocument = buildStoredMailMergeDocument({
          documentId,
          input,
          version: stored.version,
          versionCount: existingVersions.length + 1,
          createdAtMs: existingDocument.created_at_ms,
          updatedAtMs: createdAtMs,
          idempotencyKey: existingDocument.idempotency_key,
        });
        await patchStoredDocument(collections.documents, existingDocumentDoc, updatedDocument);
        existingDocument = updatedDocument;
      }
    } else {
      existingDocument = buildStoredMailMergeDocument({
        documentId,
        input,
        version: stored.version,
        versionCount: Math.max(existingVersions.length, 1),
        createdAtMs,
      });
      await collections.documents.insert(existingDocument);
      tracker.document = true;
    }
  } catch (error) {
    await cleanupFailedMailMerge(
      collections,
      { id: documentId, idempotency_key: input.idempotencyKey },
      tracker,
    ).catch(() => {});
    throw error;
  }

  return attachDocumentSyncPayload(
    mailMergeCreationResult(existingDocument, stored.version, isReplay),
    existingDocument,
    [stored.version],
    stored.chunks,
  );
}

function buildStoredMailMergeVersion({ documentId, versionNumber, input, createdAtMs }) {
  const versionId = `${documentId}_v${versionNumber}`;
  const variants = input.variants.map((variant, index) => {
    const blobId = `${versionId}_recipient_${index + 1}_blob`;
    const base64 = bytesToBase64(variant.bytes);
    const totalChunks = Math.ceil(base64.length / DOCUMENT_BLOB_CHUNK_BASE64_SIZE) || 1;
    return {
      descriptor: {
        recipientId: variant.recipientId,
        recipientLabel: variant.recipientLabel,
        index,
        total: input.variants.length,
        filename: variant.filename,
        blob_id: blobId,
        source_sha256: variant.sha256,
        linked_records: cloneJson(variant.linkedRecords),
        provenance: cloneJson(variant.provenance),
        index_text: variant.indexText,
      },
      chunks: Array.from({ length: totalChunks }, (_, chunkIndex) => ({
        id: `${blobId}_${chunkIndex}`,
        blob_id: blobId,
        document_id: documentId,
        version_id: versionId,
        idx: chunkIndex,
        total: totalChunks,
        mime_type: DOCX_MIME_TYPE,
        encoding: 'base64',
        data: base64.slice(
          chunkIndex * DOCUMENT_BLOB_CHUNK_BASE64_SIZE,
          (chunkIndex + 1) * DOCUMENT_BLOB_CHUNK_BASE64_SIZE,
        ),
        created_at_ms: createdAtMs,
      })),
    };
  });
  const firstVariant = variants[0].descriptor;
  return {
    version: {
      id: versionId,
      document_id: documentId,
      version: versionNumber,
      source_kind: 'mail_merge_export',
      blob_id: firstVariant.blob_id,
      source_sha256: firstVariant.source_sha256,
      filename: input.filename,
      mime_type: DOCX_MIME_TYPE,
      idempotency_key: input.idempotencyKey,
      linked_records: cloneJson(input.linkedRecords),
      template_ref: cloneJson(input.templateRef),
      provenance: cloneJson(input.provenance),
      mail_merge: {
        recipient_count: variants.length,
        requested_count: variants.length + input.failures.length,
        failed_count: input.failures.length,
        failures: cloneJson(input.failures),
      },
      mail_merge_variants: variants.map(({ descriptor }) => descriptor),
      model_json: {},
      diagnostics: [],
      created_at_ms: createdAtMs,
      updated_at_ms: createdAtMs,
    },
    variants,
  };
}

function buildStoredMailMergeDocument({
  documentId,
  input,
  version,
  versionCount,
  createdAtMs,
  updatedAtMs = createdAtMs,
  idempotencyKey = input.idempotencyKey,
}) {
  const bundleKey = input.logicalDocumentKey || input.idempotencyKey;
  return {
    id: documentId,
    title: input.title,
    filename: input.filename,
    mime_type: DOCX_MIME_TYPE,
    status: input.failures.length ? 'CreatedWithWarnings' : 'Created',
    document_type: 'mail_merge',
    owner_id: input.ownerId,
    current_version_id: version.id,
    source_sha256: version.source_sha256,
    page_count: 0,
    diagnostics_count: input.failures.length,
    tags: cloneJson(input.tags),
    linked_records: cloneJson(input.linkedRecords),
    template_ref: cloneJson(input.templateRef),
    provenance: cloneJson(input.provenance),
    idempotency_key: idempotencyKey,
    logical_document_key: input.logicalDocumentKey,
    mail_merge: {
      bundle_id: bundleKey,
      version_count: versionCount,
      recipient_count: version.mail_merge_variants.length,
      requested_count: version.mail_merge.requested_count,
      failed_count: version.mail_merge.failed_count,
      failures: cloneJson(version.mail_merge.failures),
      latest_run_idempotency_key: version.idempotency_key,
    },
    display_cache: {},
    index_text: [
      input.indexText,
      input.title,
      ...input.variants.flatMap((variant) => [variant.recipientLabel, variant.indexText]),
    ].filter(Boolean).join(' '),
    is_deleted: false,
    created_at_ms: finiteTimestamp(createdAtMs),
    updated_at_ms: finiteTimestamp(updatedAtMs),
  };
}

function assertStoredMailMergeBundle(document, { documentId, input, bundleKey }) {
  assertEquivalentFields(document, {
    id: documentId,
    mime_type: DOCX_MIME_TYPE,
    document_type: 'mail_merge',
    logical_document_key: input.logicalDocumentKey,
  }, ['id', 'mime_type', 'document_type', 'logical_document_key']);
  if (document.mail_merge?.bundle_id !== bundleKey) {
    throw documentsError(
      'DOCUMENTS_IDEMPOTENCY_CONFLICT',
      'Stored mail merge bundle differs from logicalDocumentKey.',
    );
  }
}

async function resolveStoredMailMergeVersion({
  collections,
  document,
  expected,
  existingVersionDoc,
  cryptoProvider,
  tracker,
}) {
  let versionDoc = existingVersionDoc || await findOne(
    collections.document_versions,
    expected.version.id,
  );
  let version = versionDoc ? documentJson(versionDoc) : null;
  if (version) {
    assertIdempotentVersion(version, expected.version, { allowSourceHashMismatch: true });
    assertEquivalentFields(version, expected.version, ['version', 'source_kind', 'mail_merge']);
    assertMailMergeVariantsEquivalent(version.mail_merge_variants, expected.version.mail_merge_variants);
  }

  const storedChunks = [];
  let variantsNeedPatch = false;
  const resolvedVariants = [];
  for (let index = 0; index < expected.variants.length; index += 1) {
    const expectedVariant = expected.variants[index];
    const storedVariant = version?.mail_merge_variants?.[index] || expectedVariant.descriptor;
    let chunks = (await findChunks(
      collections.document_blob_chunks,
      storedVariant.blob_id,
    )).map(documentJson);
    if (chunks.length) {
      chunks = assertChunkSet(chunks, {
        blobId: storedVariant.blob_id,
        documentId: document.id,
        versionId: expected.version.id,
        expectedTotal: version ? undefined : expectedVariant.chunks.length,
      });
      if (!version && canonicalJson(chunks.map((chunk) => chunk.data))
          !== canonicalJson(expectedVariant.chunks.map((chunk) => chunk.data))) {
        throw documentsError(
          'DOCUMENTS_IDEMPOTENCY_CONFLICT',
          'Stored mail merge chunks differ from the payload for this idempotencyKey.',
        );
      }
      resolvedVariants.push(storedVariant);
    } else {
      await insertChunks(collections.document_blob_chunks, expectedVariant.chunks);
      tracker.chunks.push(...expectedVariant.chunks);
      chunks = expectedVariant.chunks;
      if (version && (
        storedVariant.source_sha256 !== expectedVariant.descriptor.source_sha256
        || (index === 0 && version.source_sha256 !== expectedVariant.descriptor.source_sha256)
      )) {
        resolvedVariants.push(expectedVariant.descriptor);
        variantsNeedPatch = true;
      } else {
        resolvedVariants.push(storedVariant);
      }
    }
    storedChunks.push(...chunks);
  }

  if (!versionDoc) {
    versionDoc = await collections.document_versions.insert(expected.version);
    tracker.versions.push(expected.version);
    version = documentJson(versionDoc);
  } else if (variantsNeedPatch) {
    const patch = {
      mail_merge_variants: resolvedVariants,
      updated_at_ms: expected.version.updated_at_ms,
    };
    if (resolvedVariants[0]?.source_sha256 !== version.source_sha256) {
      patch.source_sha256 = resolvedVariants[0].source_sha256;
    }
    await patchStoredDocument(collections.document_versions, versionDoc, patch);
    versionDoc = await findOne(collections.document_versions, expected.version.id);
    version = documentJson(versionDoc);
  }

  for (const variant of version.mail_merge_variants) {
    const chunks = storedChunks.filter((chunk) => chunk.blob_id === variant.blob_id);
    await verifyStoredBytes({
      document: {
        ...document,
        current_version_id: version.id,
        source_sha256: variant.source_sha256,
      },
      version: {
        ...version,
        blob_id: variant.blob_id,
        source_sha256: variant.source_sha256,
      },
      chunks,
    }, variant.source_sha256, cryptoProvider);
    await requeueStoredChunks(collections.document_blob_chunks, chunks);
  }
  return { version, chunks: storedChunks };
}

function assertMailMergeVariantsEquivalent(actual, expected) {
  if (!Array.isArray(actual) || actual.length !== expected.length) {
    throw documentsError(
      'DOCUMENTS_IDEMPOTENCY_CONFLICT',
      'Stored mail merge recipients differ from the payload for this idempotencyKey.',
    );
  }
  const fields = [
    'recipientId',
    'recipientLabel',
    'index',
    'total',
    'filename',
    'blob_id',
    'linked_records',
    'provenance',
    'index_text',
  ];
  for (let index = 0; index < expected.length; index += 1) {
    assertEquivalentFields(actual[index], expected[index], fields);
  }
}

function nextDocumentVersionNumber(versions) {
  return versions.reduce((highest, version) => {
    const number = Number(version.version);
    return Number.isSafeInteger(number) && number > highest ? number : highest;
  }, 0) + 1;
}

async function createStoredDocx({ collections, input, cryptoProvider, now }) {
  const idHash = await sha256Hex(
    new TextEncoder().encode(`ctox-documents:create-docx:${input.idempotencyKey}`),
    cryptoProvider,
  );
  const documentId = `doc_${idHash.slice(0, 40)}`;
  const versionId = `${documentId}_v1`;
  const blobId = `${versionId}_blob`;
  const createdAtMs = finiteTimestamp(now());
  const base64 = bytesToBase64(input.bytes);
  const totalChunks = Math.ceil(base64.length / DOCUMENT_BLOB_CHUNK_BASE64_SIZE) || 1;
  const chunks = Array.from({ length: totalChunks }, (_, idx) => ({
    id: `${blobId}_${idx}`,
    blob_id: blobId,
    document_id: documentId,
    version_id: versionId,
    idx,
    total: totalChunks,
    mime_type: DOCX_MIME_TYPE,
    encoding: 'base64',
    data: base64.slice(
      idx * DOCUMENT_BLOB_CHUNK_BASE64_SIZE,
      (idx + 1) * DOCUMENT_BLOB_CHUNK_BASE64_SIZE,
    ),
    created_at_ms: createdAtMs,
  }));
  const version = {
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: 'app_created_docx',
    blob_id: blobId,
    source_sha256: input.sha256,
    filename: input.filename,
    mime_type: DOCX_MIME_TYPE,
    idempotency_key: input.idempotencyKey,
    linked_records: cloneJson(input.linkedRecords),
    template_ref: cloneJson(input.templateRef),
    provenance: cloneJson(input.provenance),
    model_json: {},
    diagnostics: [],
    created_at_ms: createdAtMs,
    updated_at_ms: createdAtMs,
  };
  const document = {
    id: documentId,
    title: input.title,
    filename: input.filename,
    mime_type: DOCX_MIME_TYPE,
    status: 'Created',
    document_type: 'word_document',
    owner_id: input.ownerId,
    current_version_id: versionId,
    source_sha256: input.sha256,
    page_count: 0,
    diagnostics_count: 0,
    tags: cloneJson(input.tags),
    linked_records: cloneJson(input.linkedRecords),
    template_ref: cloneJson(input.templateRef),
    provenance: cloneJson(input.provenance),
    idempotency_key: input.idempotencyKey,
    display_cache: {},
    index_text: input.indexText,
    is_deleted: false,
    created_at_ms: createdAtMs,
    updated_at_ms: createdAtMs,
  };
  const expected = { document, version, chunks, sha256: input.sha256 };
  const existing = await inspectIdempotentState(collections, expected, {
    acceptCompleteStoredPayload: true,
    allowPartial: true,
  });
  if (existing?.complete) {
    await verifyStoredBytes(
      existing,
      existing.version.source_sha256 || existing.document.source_sha256,
      cryptoProvider,
    );
    await requeueStoredChunks(collections.document_blob_chunks, existing.chunks);
    return attachDocumentSyncPayload(
      creationResult(existing, true),
      existing.document,
      existing.version ? [existing.version] : [],
      existing.chunks,
    );
  }

  const created = { document: false, version: false, chunks: false };

  try {
    if (existing?.requiresSourceHashRefresh) {
      await refreshPartialSourceHashes(collections, existing, expected, createdAtMs);
    }
    if (!existing?.chunks?.length) {
      await insertChunks(collections.document_blob_chunks, chunks);
      created.chunks = true;
    }
    if (!existing?.version) {
      await collections.document_versions.insert(version);
      created.version = true;
    }
    if (!existing?.document) {
      await collections.documents.insert(document);
      created.document = true;
    }
  } catch (error) {
    await cleanupFailedCreate(collections, expected, created).catch(() => {});
    throw error;
  }

  const stored = await inspectIdempotentState(collections, expected);
  if (!stored) {
    throw documentsError(
      'DOCUMENTS_PARTIAL_STATE',
      'DOCX creation did not produce a complete document, version, and chunk set.',
    );
  }
  await verifyStoredBytes(stored, input.sha256, cryptoProvider);
  return attachDocumentSyncPayload(
    creationResult(stored, false),
    stored.document,
    stored.version ? [stored.version] : [],
    stored.chunks,
  );
}

async function inspectIdempotentState(
  collections,
  expected,
  { acceptCompleteStoredPayload = false, allowPartial = false } = {},
) {
  const [documentDoc, versionDoc, chunkDocs] = await Promise.all([
    findOne(collections.documents, expected.document.id),
    findOne(collections.document_versions, expected.version.id),
    findChunks(collections.document_blob_chunks, expected.version.blob_id),
  ]);
  const document = documentDoc ? documentJson(documentDoc) : null;
  const version = versionDoc ? documentJson(versionDoc) : null;
  const chunks = chunkDocs.map(documentJson);
  const populated = Number(Boolean(document)) + Number(Boolean(version)) + Number(chunks.length > 0);
  if (populated === 0) return null;
  const acceptsStoredPayload = acceptCompleteStoredPayload && populated === 3;
  const allowSourceHashMismatch = acceptsStoredPayload || (allowPartial && chunks.length === 0);
  if (document) assertIdempotentDocument(document, expected.document, { allowSourceHashMismatch });
  if (version) assertIdempotentVersion(version, expected.version, { allowSourceHashMismatch });
  let sortedChunks = [];
  if (chunks.length) {
    sortedChunks = assertChunkSet(chunks, {
      blobId: expected.version.blob_id,
      documentId: expected.document.id,
      versionId: expected.version.id,
      expectedTotal: acceptsStoredPayload ? undefined : expected.chunks.length,
    });
    if (!acceptsStoredPayload && canonicalJson(sortedChunks.map((chunk) => chunk.data))
        !== canonicalJson(expected.chunks.map((chunk) => chunk.data))) {
      throw documentsError(
        'DOCUMENTS_IDEMPOTENCY_CONFLICT',
        'Stored DOCX chunks differ from the payload for this idempotencyKey.',
      );
    }
  }
  if (populated !== 3 && !allowPartial) {
    throw documentsError(
      'DOCUMENTS_PARTIAL_STATE',
      'A partial document, version, or chunk set already exists for this idempotencyKey.',
    );
  }
  return {
    document,
    version,
    chunks: sortedChunks,
    complete: populated === 3,
    documentDoc,
    versionDoc,
    requiresSourceHashRefresh: allowSourceHashMismatch && (
      (document && document.source_sha256 !== expected.document.source_sha256)
      || (version && version.source_sha256 !== expected.version.source_sha256)
    ),
  };
}

function assertIdempotentDocument(actual, expected, { allowSourceHashMismatch = false } = {}) {
  assertEquivalentFields(actual, expected, [
    'id',
    'current_version_id',
    'filename',
    'mime_type',
    ...(!allowSourceHashMismatch ? ['source_sha256'] : []),
    'idempotency_key',
    'linked_records',
    'template_ref',
    'provenance',
    'tags',
    'index_text',
  ]);
}

function assertIdempotentVersion(actual, expected, { allowSourceHashMismatch = false } = {}) {
  assertEquivalentFields(actual, expected, [
    'id',
    'document_id',
    'blob_id',
    'filename',
    'mime_type',
    ...(!allowSourceHashMismatch ? ['source_sha256'] : []),
    'idempotency_key',
    'linked_records',
    'template_ref',
    'provenance',
  ]);
}

function assertEquivalentFields(actual, expected, fields) {
  const differingFields = fields.filter(
    (field) => canonicalJson(actual[field]) !== canonicalJson(expected[field]),
  );
  if (differingFields.length) {
    throw documentsError(
      'DOCUMENTS_IDEMPOTENCY_CONFLICT',
      `Stored document metadata differs from the payload for this idempotencyKey: ${differingFields.join(', ')}.`,
    );
  }
}

async function refreshPartialSourceHashes(collections, existing, expected, updatedAtMs) {
  const updates = [];
  if (existing.documentDoc && existing.document?.source_sha256 !== expected.document.source_sha256) {
    updates.push(patchStoredDocument(
      collections.documents,
      existing.documentDoc,
      { source_sha256: expected.document.source_sha256, updated_at_ms: updatedAtMs },
    ));
  }
  if (existing.versionDoc && existing.version?.source_sha256 !== expected.version.source_sha256) {
    updates.push(patchStoredDocument(
      collections.document_versions,
      existing.versionDoc,
      { source_sha256: expected.version.source_sha256, updated_at_ms: updatedAtMs },
    ));
  }
  await Promise.all(updates);
}

async function patchStoredDocument(collection, document, patch) {
  if (typeof document?.incrementalPatch === 'function') {
    await document.incrementalPatch(patch);
    return;
  }
  if (typeof document?.patch === 'function') {
    await document.patch(patch);
    return;
  }
  if (typeof collection?.upsert === 'function') {
    await collection.upsert({ ...documentJson(document), ...patch });
    return;
  }
  throw documentsError(
    'DOCUMENTS_COLLECTIONS_UNAVAILABLE',
    'The document collection cannot repair partial metadata.',
  );
}

async function loadStoredVersion(collections, request, cryptoProvider) {
  const documentId = requireId(request.documentId, 'documentId');
  const expectedSha256 = requireSha256(request.expectedSha256, 'expectedSha256');
  const documentDoc = await findOne(collections.documents, documentId);
  if (!documentDoc) {
    throw documentsError('DOCUMENTS_NOT_FOUND', `Document ${documentId} was not found.`);
  }
  const document = documentJson(documentDoc);
  validateStoredDocument(document);
  const versionId = request.versionId == null || request.versionId === ''
    ? requireId(document.current_version_id, 'document.current_version_id')
    : requireId(request.versionId, 'versionId');
  const versionDoc = await findOne(collections.document_versions, versionId);
  if (!versionDoc) {
    throw documentsError('DOCUMENTS_NOT_FOUND', `Document version ${versionId} was not found.`);
  }
  const version = documentJson(versionDoc);
  if (version.document_id !== documentId) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Document version belongs to another document.');
  }
  const variant = selectMailMergeVariant(version, request);
  const blobId = requireId(variant?.blob_id || version.blob_id, 'version.blob_id');
  const chunks = (await findChunks(collections.document_blob_chunks, blobId)).map(documentJson);
  const verificationVersion = variant
    ? { ...version, blob_id: variant.blob_id, source_sha256: variant.source_sha256 }
    : version;
  const verificationDocument = variant
    ? { ...document, source_sha256: variant.source_sha256 }
    : document;
  const verified = await verifyStoredBytes(
    { document: verificationDocument, version: verificationVersion, chunks },
    expectedSha256,
    cryptoProvider,
  );
  return {
    document,
    version,
    ...(variant ? {
      recipientId: variant.recipientId,
      recipientLabel: variant.recipientLabel,
      variant: cloneJson(variant),
    } : {}),
    bytes: verified.bytes,
    sha256: verified.sha256,
    filename: variant?.filename || document.filename,
    mimeType: DOCX_MIME_TYPE,
  };
}

function selectMailMergeVariant(version, request) {
  const variants = Array.isArray(version.mail_merge_variants)
    ? version.mail_merge_variants
    : [];
  if (!variants.length) return null;
  const requestedRecipientId = request.recipientId == null || request.recipientId === ''
    ? ''
    : requireId(request.recipientId, 'recipientId');
  const requestedRecipientLabel = request.recipientLabel == null || request.recipientLabel === ''
    ? ''
    : requireText(request.recipientLabel, 'recipientLabel', 512);
  if (!requestedRecipientId && !requestedRecipientLabel) return variants[0];
  const variant = variants.find((candidate) => (
    (!requestedRecipientId || candidate.recipientId === requestedRecipientId)
    && (!requestedRecipientLabel || candidate.recipientLabel === requestedRecipientLabel)
  ));
  if (!variant) {
    throw documentsError(
      'DOCUMENTS_NOT_FOUND',
      'The requested mail merge recipient was not found in this version.',
    );
  }
  return variant;
}

async function verifyStoredBytes(stored, expectedSha256, cryptoProvider) {
  validateStoredDocument(stored.document);
  if (stored.version.mime_type && stored.version.mime_type !== DOCX_MIME_TYPE) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Document version has an invalid DOCX MIME type.');
  }
  const chunks = assertChunkSet(stored.chunks, {
    blobId: stored.version.blob_id,
    documentId: stored.document.id,
    versionId: stored.version.id,
  });
  const bytes = base64ToBytes(chunks.map((chunk) => chunk.data).join(''));
  if (!bytes.length) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Stored DOCX bytes are empty.');
  }
  const sha256 = await sha256Hex(bytes, cryptoProvider);
  const storedVersionSha = normalizeOptionalSha256(stored.version.source_sha256);
  const storedDocumentSha = stored.document.current_version_id === stored.version.id
    ? normalizeOptionalSha256(stored.document.source_sha256)
    : '';
  if (sha256 !== expectedSha256
      || (storedVersionSha && sha256 !== storedVersionSha)
      || (storedDocumentSha && sha256 !== storedDocumentSha)) {
    throw documentsError('DOCUMENTS_HASH_MISMATCH', 'Stored DOCX bytes do not match the expected SHA-256.');
  }
  return { bytes, sha256 };
}

function validateStoredDocument(document) {
  requireId(document.id, 'document.id');
  validateFilename(document.filename);
  if (document.mime_type !== DOCX_MIME_TYPE) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Document has an invalid DOCX MIME type.');
  }
}

function assertChunkSet(chunks, { blobId, documentId, versionId, expectedTotal } = {}) {
  if (!chunks.length) {
    throw documentsError('DOCUMENTS_PARTIAL_STATE', 'Document blob chunks are missing.');
  }
  const sorted = [...chunks].sort((left, right) => Number(left.idx) - Number(right.idx));
  const total = Number(sorted[0].total);
  if (!Number.isSafeInteger(total) || total <= 0 || total !== sorted.length
      || (expectedTotal != null && total !== expectedTotal)) {
    throw documentsError(
      'DOCUMENTS_PARTIAL_STATE',
      `Document blob chunk count is inconsistent (declared=${total}, found=${sorted.length}, expected=${expectedTotal ?? 'n/a'}).`,
    );
  }
  for (let idx = 0; idx < sorted.length; idx += 1) {
    const chunk = sorted[idx];
    if (chunk.idx !== idx
        || chunk.total !== total
        || chunk.blob_id !== blobId
        || chunk.document_id !== documentId
        || chunk.version_id !== versionId
        || chunk.mime_type !== DOCX_MIME_TYPE
        || chunk.encoding !== 'base64'
        || typeof chunk.data !== 'string') {
      throw documentsError('DOCUMENTS_PARTIAL_STATE', 'Document blob chunk metadata is inconsistent.');
    }
  }
  return sorted;
}

async function normalizeCreateDocxInput(input, cryptoProvider, facadeAppId) {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'createDocx input must be an object.');
  }
  if (input.mimeType !== DOCX_MIME_TYPE) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `mimeType must be ${DOCX_MIME_TYPE}.`);
  }
  const filename = validateFilename(input.filename);
  const bytes = await normalizeBytes(input.bytes);
  if (!bytes.length) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'DOCX bytes must not be empty.');
  }
  const idempotencyKey = requireText(input.idempotencyKey, 'idempotencyKey', 512);
  const title = input.title == null || input.title === ''
    ? filename.replace(/\.docx$/i, '')
    : requireText(input.title, 'title', 512);
  const ownerId = input.ownerId == null ? '' : requireText(input.ownerId, 'ownerId', 256, { allowEmpty: true });
  const linkedRecords = input.linkedRecords == null
    ? []
    : cloneJsonField(input.linkedRecords, 'linkedRecords');
  if (!Array.isArray(linkedRecords) || linkedRecords.some((entry) => !isPlainObject(entry))) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'linkedRecords must be an array of objects.');
  }
  const templateRef = input.templateRef == null
    ? null
    : cloneJsonField(input.templateRef, 'templateRef');
  const provenanceInput = input.provenance == null
    ? {}
    : cloneJsonField(input.provenance, 'provenance');
  const provenance = {
    ...provenanceInput,
    app_id: requireId(
      input.creatorAppId || provenanceInput.app_id || facadeAppId,
      'creatorAppId',
    ),
  };
  return {
    bytes,
    filename,
    title,
    ownerId,
    idempotencyKey,
    linkedRecords,
    templateRef,
    provenance,
    indexText: normalizeIndexText(input.indexText),
    tags: normalizeTags(input.tags),
    sha256: await sha256Hex(bytes, cryptoProvider),
  };
}

async function normalizeCreateMailMergeInput(input, cryptoProvider, facadeAppId) {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'createMailMerge input must be an object.');
  }
  if (input.mimeType !== DOCX_MIME_TYPE) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `mimeType must be ${DOCX_MIME_TYPE}.`);
  }
  if (!Array.isArray(input.variants) || input.variants.length === 0) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'variants must contain at least one recipient.');
  }
  const filename = validateFilename(input.filename);
  const idempotencyKey = requireText(input.idempotencyKey, 'idempotencyKey', 512);
  const logicalDocumentKey = input.logicalDocumentKey == null || input.logicalDocumentKey === ''
    ? ''
    : requireText(input.logicalDocumentKey, 'logicalDocumentKey', 512);
  const title = input.title == null || input.title === ''
    ? filename.replace(/\.docx$/i, '')
    : requireText(input.title, 'title', 512);
  const ownerId = input.ownerId == null
    ? ''
    : requireText(input.ownerId, 'ownerId', 256, { allowEmpty: true });
  const linkedRecords = input.linkedRecords == null
    ? []
    : cloneJsonField(input.linkedRecords, 'linkedRecords');
  if (!Array.isArray(linkedRecords) || linkedRecords.some((entry) => !isPlainObject(entry))) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'linkedRecords must be an array of objects.');
  }
  const templateRef = input.templateRef == null
    ? null
    : cloneJsonField(input.templateRef, 'templateRef');
  const provenanceInput = input.provenance == null
    ? {}
    : cloneJsonField(input.provenance, 'provenance');
  const creatorAppId = requireId(
    input.creatorAppId || provenanceInput.app_id || facadeAppId,
    'creatorAppId',
  );
  const provenance = { ...provenanceInput, app_id: creatorAppId };
  const failures = input.failures == null
    ? []
    : cloneJsonField(input.failures, 'failures');
  if (!Array.isArray(failures) || failures.some((entry) => !isPlainObject(entry))) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'failures must be an array of objects.');
  }

  const seenRecipients = new Set();
  const variants = [];
  for (let index = 0; index < input.variants.length; index += 1) {
    const variant = input.variants[index];
    if (!isPlainObject(variant)) {
      throw documentsError('DOCUMENTS_INVALID_INPUT', `variants[${index}] must be an object.`);
    }
    const recipientId = requireId(variant.recipientId, `variants[${index}].recipientId`);
    if (seenRecipients.has(recipientId)) {
      throw documentsError('DOCUMENTS_INVALID_INPUT', `Duplicate mail merge recipient: ${recipientId}.`);
    }
    seenRecipients.add(recipientId);
    const bytes = await normalizeBytes(variant.bytes);
    if (!bytes.length) {
      throw documentsError('DOCUMENTS_INVALID_INPUT', `variants[${index}].bytes must not be empty.`);
    }
    const variantLinkedRecords = variant.linkedRecords == null
      ? []
      : cloneJsonField(variant.linkedRecords, `variants[${index}].linkedRecords`);
    if (!Array.isArray(variantLinkedRecords)
        || variantLinkedRecords.some((entry) => !isPlainObject(entry))) {
      throw documentsError(
        'DOCUMENTS_INVALID_INPUT',
        `variants[${index}].linkedRecords must be an array of objects.`,
      );
    }
    variants.push({
      recipientId,
      recipientLabel: requireText(
        variant.recipientLabel,
        `variants[${index}].recipientLabel`,
        512,
      ),
      filename: validateFilename(variant.filename),
      bytes,
      linkedRecords: variantLinkedRecords,
      provenance: {
        ...(variant.provenance == null
          ? {}
          : cloneJsonField(variant.provenance, `variants[${index}].provenance`)),
        app_id: creatorAppId,
      },
      indexText: normalizeIndexText(variant.indexText),
      sha256: await sha256Hex(bytes, cryptoProvider),
    });
  }
  return {
    filename,
    title,
    ownerId,
    idempotencyKey,
    logicalDocumentKey,
    linkedRecords,
    templateRef,
    provenance,
    indexText: normalizeIndexText(input.indexText),
    tags: normalizeTags(input.tags),
    failures,
    variants,
  };
}

function requireDocumentCollections(db, { write = false } = {}) {
  const collections = Object.fromEntries(DOCUMENT_COLLECTION_NAMES.map((name) => [
    name,
    resolveCollection(db, name),
  ]));
  for (const name of DOCUMENT_COLLECTION_NAMES) {
    const collection = collections[name];
    const canRead = collection
      && typeof collection.findOne === 'function'
      && (name === 'documents' || typeof collection.find === 'function');
    const canWrite = !write
      || (typeof collection?.insert === 'function'
        && (name !== 'document_blob_chunks'
          || typeof collection.bulkUpsert === 'function'
          || typeof collection.bulkInsert === 'function'
          || typeof collection.insert === 'function'));
    if (!canRead || !canWrite) {
      throw documentsError(
        'DOCUMENTS_COLLECTIONS_UNAVAILABLE',
        `Required Business OS collection is unavailable: ${name}.`,
      );
    }
  }
  return collections;
}

function resolveCollection(db, name) {
  return db?.collection?.(name)
    || db?.collections?.[name]
    || db?.raw?.[name]
    || null;
}

async function findOne(collection, id) {
  const query = collection.findOne(id);
  if (!query || typeof query.exec !== 'function') {
    throw documentsError('DOCUMENTS_COLLECTIONS_UNAVAILABLE', 'RxDB findOne().exec() is required.');
  }
  return query.exec();
}

async function findChunks(collection, blobId) {
  const query = collection.find({
    selector: { blob_id: blobId },
    sort: [{ idx: 'asc' }],
  });
  if (!query || typeof query.exec !== 'function') {
    throw documentsError('DOCUMENTS_COLLECTIONS_UNAVAILABLE', 'RxDB find().exec() is required.');
  }
  const chunks = await query.exec();
  return Array.isArray(chunks) ? chunks : [];
}

async function findDocumentVersions(collection, documentId) {
  const query = collection.find({ selector: { document_id: documentId } });
  if (!query || typeof query.exec !== 'function') {
    throw documentsError('DOCUMENTS_COLLECTIONS_UNAVAILABLE', 'RxDB find().exec() is required.');
  }
  const versions = await query.exec();
  return Array.isArray(versions)
    ? [...versions].sort((left, right) => (
      Number(documentJson(left).version) - Number(documentJson(right).version)
    ))
    : [];
}

async function insertChunks(collection, chunks) {
  let result;
  if (typeof collection.bulkUpsert === 'function') {
    result = await collection.bulkUpsert(chunks);
  } else if (typeof collection.bulkInsert === 'function') {
    result = await collection.bulkInsert(chunks);
  } else {
    for (const chunk of chunks) await collection.insert(chunk);
    return;
  }
  const errors = Array.isArray(result?.error)
    ? result.error
    : Object.values(result?.error || {});
  if (errors.length) {
    throw documentsError(
      'DOCUMENTS_BLOB_WRITE_FAILED',
      `Document blob write failed for ${errors.length} chunk(s).`,
    );
  }
}

async function requeueStoredChunks(collection, chunks) {
  let result;
  if (typeof collection.bulkUpsert === 'function') {
    result = await collection.bulkUpsert(chunks);
  } else if (typeof collection.upsert === 'function') {
    for (const chunk of chunks) await collection.upsert(chunk);
    return;
  } else {
    throw documentsError(
      'DOCUMENTS_COLLECTIONS_UNAVAILABLE',
      'The document blob collection cannot requeue an idempotent payload.',
    );
  }
  const errors = Array.isArray(result?.error)
    ? result.error
    : Object.values(result?.error || {});
  if (errors.length) {
    throw documentsError(
      'DOCUMENTS_BLOB_WRITE_FAILED',
      `Document blob requeue failed for ${errors.length} chunk(s).`,
    );
  }
}

async function withDocumentSyncLeases(sync, collections, operation) {
  if (typeof sync?.leaseCollection !== 'function') return operation();
  const leases = [];
  try {
    for (const collection of DOCUMENT_COLLECTION_NAMES) {
      const lease = await acquireDocumentSyncLease(sync, collection);
      leases.push(lease);
      await waitForDocumentSync(lease);
    }
    const result = await operation();
    const documentsByCollection = await collectDocumentSyncPayload(collections, result);
    // Publish payload dependencies before the parent metadata becomes visible.
    for (const lease of [...leases].reverse()) {
      await flushDocumentSync(
        lease,
        documentsByCollection.get(lease?.collection) || [],
      );
    }
    return result;
  } finally {
    await Promise.allSettled(leases.reverse().map((lease) => lease?.release?.()));
  }
}

async function acquireDocumentSyncLease(sync, collection) {
  let lastError = null;
  for (let attempt = 1; attempt <= DOCUMENT_SYNC_LEASE_ATTEMPTS; attempt += 1) {
    try {
      const lease = await sync.leaseCollection(collection, `documents-write:${collection}`);
      const bridge = lease?.bridge || null;
      if (bridge?.mode === 'follower' || bridge?.mode === 'pending' || !bridge?.state) {
        try {
          const directBridge = await sync.startCollection?.(collection, {
            pin: false,
            forceDirect: true,
          });
          if (!directBridge?.state) {
            throw documentsError(
              'DOCUMENTS_SYNC_UNAVAILABLE',
              `Document storage did not acquire a direct native peer for ${collection}.`,
            );
          }
          // The runtime publishes the direct bridge to the retained lease.
        } catch (error) {
          await lease?.release?.().catch(() => null);
          throw error;
        }
      }
      return lease;
    } catch (error) {
      lastError = error;
      if (error?.retryable !== true || attempt === DOCUMENT_SYNC_LEASE_ATTEMPTS) throw error;
      await delay(DOCUMENT_SYNC_LEASE_RETRY_DELAY_MS);
    }
  }
  throw lastError;
}

async function waitForDocumentSync(lease) {
  const bridge = lease?.bridge || lease || null;
  const replication = bridge?.state || lease?.state || null;
  const followsBridge = typeof lease?.subscribeBridge === 'function';
  if (!replication && !followsBridge) return;
  if (followsBridge || hasSyncPeerStatus(replication)) {
    const deadline = Date.now() + 15_000;
    while (Date.now() < deadline) {
      const current = followsBridge ? lease.bridge : bridge;
      if (['released', 'stopped', 'failed'].includes(current?.mode)) {
        throw documentsError('DOCUMENTS_SYNC_UNAVAILABLE', 'Document storage sync lease is unavailable.');
      }
      if (current?.state && !current.state.cancelled
        && current.state.canceled$?.getValue?.() !== true
        && syncPeerConnected(current.state)) return;
      await delay(100);
    }
    throw documentsError(
      'DOCUMENTS_SYNC_UNAVAILABLE',
      'Document storage did not connect to the native Business OS peer.',
    );
  }
  await withDocumentsTimeout(
    () => replication.awaitInSync?.() || replication.awaitInitialReplication?.(),
    15_000,
    'Document storage did not become ready.',
  );
}

async function collectDocumentSyncPayload(collections, result) {
  if (result?.[DOCUMENT_SYNC_PAYLOAD] instanceof Map) {
    return result[DOCUMENT_SYNC_PAYLOAD];
  }
  const documentDoc = await findOne(collections.documents, result.documentId);
  const document = documentDoc ? documentJson(documentDoc) : null;
  const versionIds = Array.isArray(result.versions)
    ? result.versions.map((version) => version.versionId)
    : [result.versionId];
  const versions = (await Promise.all(
    versionIds.map((versionId) => findOne(collections.document_versions, versionId)),
  )).map(documentJson).filter(Boolean);
  const chunks = (
    await Promise.all(
      versions.map((version) => findChunks(collections.document_blob_chunks, version.blob_id)),
    )
  ).flat().map(documentJson).filter(Boolean);
  return new Map([
    ['documents', document ? [document] : []],
    ['document_versions', versions],
    ['document_blob_chunks', chunks],
  ]);
}

function attachDocumentSyncPayload(result, document, versions, chunks) {
  Object.defineProperty(result, DOCUMENT_SYNC_PAYLOAD, {
    value: new Map([
      ['documents', document ? [cloneJson(document)] : []],
      ['document_versions', (versions || []).map(cloneJson)],
      ['document_blob_chunks', (chunks || []).map(cloneJson)],
    ]),
    enumerable: false,
  });
  return result;
}

async function flushDocumentSync(lease, documents = []) {
  const bridge = lease?.bridge || lease || null;
  if (bridge?.mode === 'follower' && typeof bridge.flush === 'function') {
    await withDocumentsTimeout(
      () => bridge.flush(),
      60_000,
      'The sync leader did not confirm the document blob push.',
    );
    return;
  }
  const replication = bridge?.state || lease?.state || null;
  if (!replication) {
    if (typeof lease?.subscribeBridge === 'function') {
      throw documentsError('DOCUMENTS_SYNC_UNAVAILABLE', 'Document storage has no current bridge to confirm delivery.');
    }
    return;
  }
  await withDocumentsTimeout(
    () => {
      if (documents.length && typeof replication.pushDocumentsToRemotePeers === 'function') {
        return replication.pushDocumentsToRemotePeers(documents);
      }
      if (typeof replication.pushToRemotePeers === 'function') {
        return replication.pushToRemotePeers({ requireSuccess: true });
      }
      if (typeof replication.scheduleLocalWritePush === 'function') {
        return replication.scheduleLocalWritePush();
      }
      return replication.awaitInSync?.();
    },
    60_000,
    'Document storage could not confirm the document blob push.',
  );
}

function hasSyncPeerStatus(replication) {
  return typeof replication?.getTransportStatus === 'function'
    || Boolean(replication?.peerStates$)
    || Boolean(replication?.active$)
    || Boolean(replication?.transportStatus$);
}

function syncPeerConnected(replication) {
  if (String(replication?.activeRemotePeerId || '').trim()) return true;
  const status = replication?.getTransportStatus?.() || {};
  if (Number(status?.activePeerCount || 0) > 0) return true;
  const connections = Array.isArray(status?.connectionStates) ? status.connectionStates : [];
  if (connections.some((connection) => {
    const channelState = connection?.channelState || connection?.channelReadyState || '';
    const peerState = connection?.peerConnectionState || '';
    return connection?.open === true
      || (channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(peerState));
  })) return true;
  return replication?.active$?.getValue?.() === true
    && Boolean(String(replication?.activeRemotePeerId || '').trim());
}

async function withDocumentsTimeout(operation, timeoutMs, message) {
  let timer = null;
  try {
    return await Promise.race([
      Promise.resolve().then(operation),
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(documentsError('DOCUMENTS_SYNC_UNAVAILABLE', message)), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function cleanupFailedCreate(collections, expected, created = {}) {
  if (created.document) {
    await removeMatchingDocument(collections.documents, expected.document.id, (document) => (
      document.idempotency_key === expected.document.idempotency_key
    ));
  }
  if (created.version) {
    await removeMatchingDocument(collections.document_versions, expected.version.id, (version) => (
      version.idempotency_key === expected.version.idempotency_key
    ));
  }
  if (!created.chunks) return;
  for (const expectedChunk of expected.chunks) {
    await removeMatchingDocument(collections.document_blob_chunks, expectedChunk.id, (chunk) => (
      chunk.blob_id === expectedChunk.blob_id && chunk.data === expectedChunk.data
    ));
  }
}

async function cleanupFailedMailMerge(collections, document, created) {
  if (created.document) {
    await removeMatchingDocument(collections.documents, document.id, (stored) => (
      stored.idempotency_key === document.idempotency_key
    ));
  }
  for (const version of created.versions) {
    await removeMatchingDocument(collections.document_versions, version.id, (stored) => (
      stored.idempotency_key === version.idempotency_key
    ));
  }
  for (const chunk of created.chunks) {
    await removeMatchingDocument(collections.document_blob_chunks, chunk.id, (stored) => (
      stored.blob_id === chunk.blob_id && stored.data === chunk.data
    ));
  }
}

async function removeMatchingDocument(collection, id, matches) {
  const doc = await findOne(collection, id);
  if (!doc || !matches(documentJson(doc)) || typeof doc.remove !== 'function') return;
  await doc.remove();
}

function creationResult(stored, idempotent) {
  return {
    documentId: stored.document.id,
    versionId: stored.version.id,
    sha256: stored.version.source_sha256 || stored.document.source_sha256,
    document: cloneJson(stored.document),
    version: cloneJson(stored.version),
    idempotent,
  };
}

function mailMergeCreationResult(document, version, idempotent) {
  const variants = version.mail_merge_variants.map((variant) => ({
    versionId: version.id,
    recipientId: variant.recipientId,
    recipientLabel: variant.recipientLabel,
    filename: variant.filename,
    blobId: variant.blob_id,
    sha256: variant.source_sha256,
  }));
  return {
    documentId: document.id,
    versionId: version.id,
    recipientCount: variants.length,
    document: cloneJson(document),
    version: cloneJson(version),
    variants,
    versions: [{
      versionId: version.id,
      version: version.version,
      sha256: version.source_sha256,
      recipientCount: variants.length,
      variants: cloneJson(variants),
    }],
    idempotent,
  };
}

async function normalizeBytes(value) {
  if (value instanceof Uint8Array) return value.slice();
  if (value instanceof ArrayBuffer) return new Uint8Array(value.slice(0));
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength));
  }
  if (typeof Blob === 'function' && value instanceof Blob) {
    return new Uint8Array(await value.arrayBuffer());
  }
  throw documentsError(
    'DOCUMENTS_INVALID_INPUT',
    'bytes must be a Uint8Array, ArrayBuffer, ArrayBuffer view, or Blob.',
  );
}

function validateFilename(value) {
  const filename = requireText(value, 'filename', 255);
  if (!/\.docx$/i.test(filename)
      || filename.includes('/')
      || filename.includes('\\')
      || /[\u0000-\u001f\u007f]/.test(filename)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'filename must be a safe .docx filename.');
  }
  return filename;
}

function resolveFacadeAppId(appId) {
  return requireId(typeof appId === 'function' ? appId() : appId, 'appId');
}

function normalizeIndexText(value) {
  if (value == null || value === '') return '';
  if (typeof value !== 'string' || value.length > 200_000) {
    throw documentsError(
      'DOCUMENTS_INVALID_INPUT',
      'indexText must be a string with at most 200000 characters.',
    );
  }
  return value.trim();
}

function normalizeTags(value) {
  if (value == null) return [];
  if (!Array.isArray(value)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'tags must be an array of strings.');
  }
  const tags = value.map((tag, index) => requireText(tag, `tags[${index}]`, 80));
  return [...new Set(tags)].slice(0, 50);
}

function requireId(value, field) {
  const id = requireText(value, field, 180);
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(id)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} contains invalid characters.`);
  }
  return id;
}

function requireText(value, field, maxLength, { allowEmpty = false } = {}) {
  if (typeof value !== 'string') {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} must be a string.`);
  }
  const normalized = value.trim();
  if ((!allowEmpty && !normalized) || normalized.length > maxLength) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} is empty or too long.`);
  }
  return normalized;
}

function requireSha256(value, field) {
  if (typeof value !== 'string' || !/^[a-fA-F0-9]{64}$/.test(value.trim())) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} must be a SHA-256 hex digest.`);
  }
  return value.trim().toLowerCase();
}

function normalizeOptionalSha256(value) {
  if (value == null || value === '') return '';
  return requireSha256(value, 'stored source_sha256');
}

function finiteTimestamp(value) {
  const timestamp = Number(value);
  if (!Number.isFinite(timestamp) || timestamp < 0) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'now() must return a finite timestamp.');
  }
  return timestamp;
}

async function sha256Hex(bytes, cryptoProvider) {
  if (!cryptoProvider?.subtle?.digest) {
    throw documentsError('DOCUMENTS_CRYPTO_UNAVAILABLE', 'WebCrypto SHA-256 is unavailable.');
  }
  const digest = await cryptoProvider.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

function bytesToBase64(bytes) {
  if (typeof btoa !== 'function') {
    throw documentsError('DOCUMENTS_CRYPTO_UNAVAILABLE', 'Base64 encoding is unavailable.');
  }
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

function base64ToBytes(base64) {
  if (typeof atob !== 'function'
      || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(base64)) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Stored document chunks are not valid Base64.');
  }
  let binary;
  try {
    binary = atob(base64);
  } catch {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Stored document chunks are not valid Base64.');
  }
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

function cloneJsonField(value, field) {
  try {
    assertJsonValue(value, new Set());
    return cloneJson(value);
  } catch (error) {
    if (error?.code === 'DOCUMENTS_INVALID_INPUT') throw error;
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} must be JSON-compatible.`);
  }
}

function assertJsonValue(value, seen) {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return;
  if (typeof value === 'number' && Number.isFinite(value)) return;
  if (Array.isArray(value)) {
    if (seen.has(value)) throw new Error('cyclic JSON value');
    seen.add(value);
    for (const entry of value) assertJsonValue(entry, seen);
    seen.delete(value);
    return;
  }
  if (isPlainObject(value)) {
    if (seen.has(value)) throw new Error('cyclic JSON value');
    seen.add(value);
    for (const entry of Object.values(value)) assertJsonValue(entry, seen);
    seen.delete(value);
    return;
  }
  throw new Error('non-JSON value');
}

function isPlainObject(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function cloneJson(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

function canonicalJson(value) {
  if (value === undefined) return 'undefined';
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
}

function documentJson(document) {
  return typeof document?.toJSON === 'function' ? document.toJSON() : { ...document };
}

function documentsError(code, message) {
  const error = new Error(message);
  error.name = 'DocumentsFacadeError';
  error.code = code;
  return error;
}
