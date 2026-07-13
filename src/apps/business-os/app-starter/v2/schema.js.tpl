const recordSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    title: { type: 'string' },
    status: { type: 'string' },
    notes: { type: 'string' },
    is_deleted: { type: 'boolean' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'title', 'status', 'notes', 'is_deleted', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = { '__COLLECTION__': recordSchema };
export const migrationStrategies = {};
