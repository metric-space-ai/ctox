const commandSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    command_id: { type: 'string' },
    module: { type: 'string' },
    command_type: { type: 'string' },
    record_id: { type: 'string' },
    status: { type: 'string' },
    inbound_channel: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    client_context: { type: 'object', additionalProperties: true },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'command_id', 'module', 'command_type', 'status', 'updated_at_ms'],
  additionalProperties: true
};

const documentSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    title: { type: 'string' },
    filename: { type: 'string' },
    mime_type: { type: 'string' },
    status: { type: 'string' },
    document_type: { type: 'string' },
    owner_id: { type: 'string' },
    current_version_id: { type: 'string' },
    source_sha256: { type: 'string' },
    page_count: { type: 'number' },
    diagnostics_count: { type: 'number' },
    linked_records: { type: 'array', items: { type: 'object', additionalProperties: true } },
    display_cache: { type: 'object', additionalProperties: true },
    index_text: { type: 'string' },
    is_deleted: { type: 'boolean' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'title', 'filename', 'mime_type', 'status', 'current_version_id', 'index_text', 'is_deleted', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const documentVersionSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    document_id: { type: 'string' },
    version: { type: 'number' },
    source_kind: { type: 'string' },
    blob_id: { type: 'string' },
    model_json: { type: 'object', additionalProperties: true },
    diagnostics: { type: 'array', items: { type: 'object', additionalProperties: true } },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'document_id', 'version', 'source_kind', 'blob_id', 'model_json', 'diagnostics', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const documentBlobChunkSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    blob_id: { type: 'string' },
    document_id: { type: 'string' },
    version_id: { type: 'string' },
    idx: { type: 'number' },
    total: { type: 'number' },
    mime_type: { type: 'string' },
    encoding: { type: 'string' },
    data: { type: 'string' },
    created_at_ms: { type: 'number' }
  },
  required: ['id', 'blob_id', 'document_id', 'version_id', 'idx', 'total', 'mime_type', 'encoding', 'data', 'created_at_ms'],
  additionalProperties: false
};

const documentRunbookSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    document_type: { type: 'string' },
    title: { type: 'string' },
    description: { type: 'string' },
    command_type: { type: 'string' },
    prompt_template: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'document_type', 'title', 'command_type', 'prompt_template', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  business_commands: commandSchema,
  documents: documentSchema,
  document_versions: documentVersionSchema,
  document_blob_chunks: documentBlobChunkSchema,
  document_runbooks: documentRunbookSchema
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || ''
    })
  }
};
