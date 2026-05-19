const commandSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    command_id: { type: 'string' },
    module: { type: 'string' },
    command_type: { type: 'string' },
    record_id: { type: 'string' },
    status: { type: 'string' },
    inbound_channel: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    client_context: { type: 'object', additionalProperties: true },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'command_id', 'module', 'command_type', 'status', 'updated_at_ms'],
  additionalProperties: true,
};

const outboundCampaignSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    name: { type: 'string' },
    objective: { type: 'string' },
    market: { type: 'string' },
    status: { type: 'string' },
    owner_id: { type: 'string' },
    source_count: { type: 'number' },
    company_count: { type: 'number' },
    qualified_count: { type: 'number' },
    pipeline_count: { type: 'number' },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'name', 'status', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

const outboundSourceSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    campaign_id: { type: 'string' },
    title: { type: 'string' },
    source_type: { type: 'string' },
    status: { type: 'string' },
    file_name: { type: 'string' },
    row_count: { type: 'number' },
    imported_count: { type: 'number' },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'campaign_id', 'title', 'source_type', 'status', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

const outboundCompanySchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    campaign_id: { type: 'string' },
    source_id: { type: 'string' },
    row_index: { type: 'number' },
    name: { type: 'string' },
    website: { type: 'string' },
    domain: { type: 'string' },
    city: { type: 'string' },
    country: { type: 'string' },
    qualification_status: { type: 'string' },
    research_status: { type: 'string' },
    pipeline_status: { type: 'string' },
    fit_score: { type: 'number' },
    fit_status: { type: 'string' },
    company_data: { type: 'object', additionalProperties: true },
    evidence: { type: 'array', items: { type: 'object', additionalProperties: true } },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'campaign_id', 'name', 'qualification_status', 'research_status', 'pipeline_status', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

const outboundPipelineSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    campaign_id: { type: 'string' },
    company_id: { type: 'string' },
    company_name: { type: 'string' },
    stage: { type: 'string' },
    contact_research_status: { type: 'string' },
    outreach_status: { type: 'string' },
    priority: { type: 'string' },
    contacts: { type: 'array', items: { type: 'object', additionalProperties: true } },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'campaign_id', 'company_id', 'company_name', 'stage', 'contact_research_status', 'outreach_status', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

const outboundResearchRunSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    campaign_id: { type: 'string' },
    company_id: { type: 'string' },
    pipeline_id: { type: 'string' },
    run_type: { type: 'string' },
    status: { type: 'string' },
    command_id: { type: 'string' },
    request: { type: 'object', additionalProperties: true },
    result: { type: 'object', additionalProperties: true },
    error: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'campaign_id', 'run_type', 'status', 'request', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  business_commands: commandSchema,
  outbound_campaigns: outboundCampaignSchema,
  outbound_sources: outboundSourceSchema,
  outbound_companies: outboundCompanySchema,
  outbound_pipeline_items: outboundPipelineSchema,
  outbound_research_runs: outboundResearchRunSchema,
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || '',
    }),
  },
};
