const sourceSchema = {
  version: 0, primaryKey: 'id', type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 }, label: { type: 'string' }, url: { type: 'string' },
    countries: { type: 'array', items: { type: 'string' } }, field_keys: { type: 'array', items: { type: 'string' } },
    enabled: { type: 'boolean' }, requires_credential: { type: 'boolean' }, credential_secret_name: { type: 'string' },
    target_key: { type: 'string' }, adapter_status: { type: 'string' }, scrape_status: { type: 'string' }, auth_status: { type: 'string' },
    payload: { type: 'object', additionalProperties: true }, created_at_ms: { type: 'number' }, updated_at_ms: { type: 'number' },
  },
  required: ['id', 'label', 'url', 'countries', 'field_keys', 'enabled', 'requires_credential', 'credential_secret_name', 'target_key', 'adapter_status', 'scrape_status', 'auth_status', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};
const adapterSchema = {
  version: 2, primaryKey: 'id', type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 }, source_id: { type: 'string' }, status: { type: 'string' }, scrape_status: { type: 'string' }, auth_status: { type: 'string' },
    last_command_id: { type: 'string' }, last_task_id: { type: 'string' }, last_error: { type: 'string' }, payload: { type: 'object', additionalProperties: true }, created_at_ms: { type: 'number' }, updated_at_ms: { type: 'number' },
  },
  required: ['id', 'source_id', 'status', 'scrape_status', 'auth_status', 'last_command_id', 'last_error', 'payload', 'created_at_ms', 'updated_at_ms'], additionalProperties: true,
};
const importSchema = {
  version: 0, primaryKey: 'id', type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 }, title: { type: 'string' }, source_type: { type: 'string' }, status: { type: 'string' }, lead_count: { type: 'number' },
    payload: { type: 'object', additionalProperties: true }, created_at_ms: { type: 'number' }, updated_at_ms: { type: 'number' },
  },
  required: ['id', 'title', 'source_type', 'status', 'lead_count', 'payload', 'created_at_ms', 'updated_at_ms'], additionalProperties: true,
};
const researchPolicySchema = {
  version: 0, primaryKey: 'id', type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 }, title: { type: 'string' }, version_number: { type: 'number' }, status: { type: 'string' },
    skill_name: { type: 'string' }, skill_version: { type: 'string' }, min_independent_sources: { type: 'number' },
    rules: { type: 'array', items: { type: 'object', additionalProperties: true } }, instructions: { type: 'string' },
    field_keys: { type: 'array', items: { type: 'string' } },
    configuration_digest: { type: 'string' }, reconciliation_status: { type: 'string' },
    reconciliation_command_id: { type: 'string' }, reconciliation_task_id: { type: 'string' }, reconciliation_error: { type: 'string' },
    created_at_ms: { type: 'number' }, updated_at_ms: { type: 'number' },
  },
  required: ['id', 'title', 'version_number', 'status', 'skill_name', 'skill_version', 'min_independent_sources', 'rules', 'created_at_ms', 'updated_at_ms'], additionalProperties: true,
};
const leadSchema = {
  version: 0, primaryKey: 'id', type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 }, import_id: { type: 'string' }, campaign: { type: 'string' }, name: { type: 'string' }, domain: { type: 'string' }, website: { type: 'string' }, city: { type: 'string' }, country: { type: 'string' },
    research_status: { type: 'string' }, validation_status: { type: 'string' }, sellify_status: { type: 'string' }, task_id: { type: 'string' }, command_id: { type: 'string' },
    data: { type: 'object', additionalProperties: true }, contacts: { type: 'array', items: { type: 'object', additionalProperties: true } }, selected_contact_ids: { type: 'array', items: { type: 'string' } }, evidence: { type: 'array', items: { type: 'object', additionalProperties: true } },
    payload: { type: 'object', additionalProperties: true }, created_at_ms: { type: 'number' }, updated_at_ms: { type: 'number' },
  },
  required: ['id', 'import_id', 'campaign', 'name', 'domain', 'website', 'city', 'country', 'research_status', 'validation_status', 'sellify_status', 'task_id', 'command_id', 'data', 'contacts', 'selected_contact_ids', 'evidence', 'payload', 'created_at_ms', 'updated_at_ms'], additionalProperties: true,
};

export const collections = {
  outbound_lead_generation_sources: sourceSchema,
  outbound_lead_generation_adapters: adapterSchema,
  outbound_lead_generation_imports: importSchema,
  outbound_lead_generation_research_policies: researchPolicySchema,
  outbound_lead_generation_leads: leadSchema,
};


// CTOX adapter schema moved from v0 to v1 when last_task_id was added to the
// persisted adapter-command state. Existing v0 rows must receive the new
// required field during migration; otherwise RxDB rejects the collection at
// registration before any CRM lookup can run.
export const migrationStrategies = {
  outbound_lead_generation_adapters: {
    1: (oldDoc) => ({
      ...oldDoc,
      last_task_id: String(oldDoc?.last_task_id || ''),
    }),
    2: (oldDoc) => ({
      ...oldDoc,
      last_task_id: String(oldDoc?.last_task_id || ''),
    }),
  },
};

export default collections;
