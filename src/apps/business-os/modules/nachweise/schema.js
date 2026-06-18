// Generic credential/expiry-vault collection. A subject (candidate, worker, …)
// holds verified artifacts with a validity window and a deployment gate.
// Baukasten: collection is domain-neutral; recruiting credential types are config.

const businessCredentialsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    subject_id: { type: 'string' },
    subject_type: { type: 'string' },
    credential_type: { type: 'string' },
    issuer: { type: 'string' },
    valid_from_ms: { type: 'number' },
    valid_until_ms: { type: 'number' },
    document_id: { type: 'string' },
    verified: { type: 'boolean' },
    verified_by: { type: 'string' },
    status: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'subject_id', 'credential_type', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  business_credentials: businessCredentialsSchema,
};

export const migrationStrategies = {};
