// Generic share-out / submission collection (recruiting: candidate presented to
// a client contact, with double-submission + consent protection).

const submissionsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    candidate_id: { type: 'string' },
    vacancy_id: { type: 'string' },
    client_account_id: { type: 'string' },
    client_contact_id: { type: 'string' },
    consent_id: { type: 'string' },
    sent_at_ms: { type: 'number' },
    status: { type: 'string' },
    feedback: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'candidate_id', 'client_account_id', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  submissions: submissionsSchema,
};

export const migrationStrategies = {};
