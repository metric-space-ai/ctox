// Generic offer/placement lifecycle collections (recruiting: the closing
// surface — offers and confirmed placements with a guarantee clock).

const offersSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    candidate_id: { type: 'string' },
    vacancy_id: { type: 'string' },
    state: { type: 'string' },
    salary: { type: 'number' },
    start_date: { type: 'string' },
    role: { type: 'string' },
    package: { type: 'object', additionalProperties: true },
    negotiation_notes: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'state', 'updated_at_ms'],
  additionalProperties: true,
};

const placementsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    candidate_id: { type: 'string' },
    vacancy_id: { type: 'string' },
    client_account_id: { type: 'string' },
    offer_id: { type: 'string' },
    start_ms: { type: 'number' },
    guarantee_days: { type: 'number' },
    fee: { type: 'number' },
    fee_basis: { type: 'object', additionalProperties: true },
    status: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'candidate_id', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  offers: offersSchema,
  placements: placementsSchema,
};

export const migrationStrategies = {};
