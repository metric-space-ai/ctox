const baseRecordFields = {
  id: { type: 'string', maxLength: 160 },
  kind: { type: 'string' },
  title: { type: 'string' },
  source_type: { type: 'string' },
  source_ref: { type: 'string' },
  definition_id: { type: 'string' },
  schema_version: { type: 'string' },
  status: { type: 'string' },
  data: { type: 'object', additionalProperties: true },
  created_at_ms: { type: 'number' },
  updated_at_ms: { type: 'number' },
  _deleted: { type: 'boolean' }
};

const matchingRequirementsSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    prompt_key: { type: 'string' },
    parsed_requirement: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'kind', 'title', 'status', 'data', 'updated_at_ms'],
  additionalProperties: true
};

const matchingObjectsSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    prompt_key: { type: 'string' },
    parsed_object: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'kind', 'title', 'status', 'data', 'updated_at_ms'],
  additionalProperties: true
};

const matchingResultsSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    requirement_id: { type: 'string' },
    object_id: { type: 'string' },
    score: { type: 'number' },
    evidence: { type: 'array', items: { type: 'object', additionalProperties: true } },
    parsed_match: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'kind', 'title', 'status', 'data', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  matching_requirements: matchingRequirementsSchema,
  matching_objects: matchingObjectsSchema,
  matching_results: matchingResultsSchema
};

const preserveDocument = (doc) => doc;

export const migrationStrategies = {
  matching_requirements: { 1: preserveDocument },
  matching_objects: { 1: preserveDocument },
  matching_results: { 1: preserveDocument }
};
