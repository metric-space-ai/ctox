// Generic DSGVO consent / legal-basis ledger collection. One row per
// subject+purpose grant. Reusable by every module; recruiting supplies purposes.

const businessConsentsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    subject_id: { type: 'string' },
    subject_type: { type: 'string' },
    purpose: { type: 'string' },
    legal_basis: { type: 'string' },
    granted_at_ms: { type: 'number' },
    withdrawn_at_ms: { type: 'number' },
    expires_at_ms: { type: 'number' },
    source: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'subject_id', 'purpose', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  business_consents: businessConsentsSchema,
};

export const migrationStrategies = {};
