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

const knowledgeRecordSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    kind: { type: 'string' },
    title: { type: 'string' },
    subtitle: { type: 'string' },
    summary: { type: 'string' },
    source_path: { type: 'string' },
    updated_at: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'kind', 'title', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  business_commands: commandSchema,
  knowledge_items: knowledgeRecordSchema,
  knowledge_runbooks: knowledgeRecordSchema,
  knowledge_tables: knowledgeRecordSchema
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || ''
    })
  }
};
