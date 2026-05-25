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
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    title: { type: 'string' },
    content: { type: 'string' },
    folder: { type: 'string' },
    updated_at_ms: { type: 'number' },
    notebook: { type: 'string' },
    tags: { type: 'string' },
    is_favorite: { type: 'boolean' },
    is_trashed: { type: 'boolean' },
    is_locked: { type: 'boolean' },
    lock_passcode: { type: 'string' }
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
  },
  notes: {
    1: (oldDoc) => ({
      ...oldDoc,
      notebook: oldDoc.notebook || '',
      tags: oldDoc.tags || '',
      is_favorite: !!oldDoc.is_favorite,
      is_trashed: !!oldDoc.is_trashed,
      is_locked: !!oldDoc.is_locked,
      lock_passcode: oldDoc.lock_passcode || ''
    })
  }
};
