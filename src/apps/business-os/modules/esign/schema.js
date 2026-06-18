// Generic e-signature request collection: route a document to one or more
// signers and track status (recruiting: Arbeits-/Vermittlungs-/Überlassungsvertrag).

const signatureRequestsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    document_id: { type: 'string' },
    subject_kind: { type: 'string' },
    signers: { type: 'array', items: { type: 'object', additionalProperties: true } },
    sent_at_ms: { type: 'number' },
    expires_at_ms: { type: 'number' },
    status: { type: 'string' },
    signed_artifact_id: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'document_id', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  signature_requests: signatureRequestsSchema,
};

export const migrationStrategies = {};
