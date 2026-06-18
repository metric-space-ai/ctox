// Generic multi-channel intake collection: one normalized inbound record
// (recruiting: a job application feeding the candidate pool).

const applicationsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    channel: { type: 'string' },
    vacancy_id: { type: 'string' },
    candidate: { type: 'object', additionalProperties: true },
    documents: { type: 'array', items: { type: 'object', additionalProperties: true } },
    dedupe_key: { type: 'string' },
    status: { type: 'string' },
    received_at_ms: { type: 'number' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'channel', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  applications: applicationsSchema,
};

export const migrationStrategies = {};
