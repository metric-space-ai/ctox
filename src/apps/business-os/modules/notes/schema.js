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

const noteRecordSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    title: { type: 'string' },
    content: { type: 'string' },
    folder: { type: 'string' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'title', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  business_commands: commandSchema,
  notes: noteRecordSchema
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || ''
    })
  }
};
