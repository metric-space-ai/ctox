const commandSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    command_id: { type: 'string' },
    module: { type: 'string' },
    command_type: { type: 'string' },
    record_id: { type: 'string' },
    status: { type: 'string' },
    inbound_channel: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    client_context: { type: 'object', additionalProperties: true },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'command_id', 'module', 'command_type', 'status', 'updated_at_ms'],
  additionalProperties: true
};

export const businessProfileSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    company_name: { type: 'string' },
    operator_name: { type: 'string' },
    mission_statement: { type: 'string' },
    vision_statement: { type: 'string' },
    operating_principles: { type: 'array', items: { type: 'string' } },
    communication_paths: { type: 'object', additionalProperties: true },
    routing_policy: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    is_deleted: { type: 'boolean' }
  },
  required: ['id', 'company_name', 'updated_at_ms'],
  additionalProperties: true
};

export const businessOnboardingStateSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    status: { type: 'string' },
    profile_id: { type: 'string' },
    setup_module_id: { type: 'string' },
    completed_by: { type: 'string' },
    completed_at_ms: { type: 'number' },
    dismissed_at_ms: { type: 'number' },
    version: { type: 'number' },
    updated_at_ms: { type: 'number' },
    is_deleted: { type: 'boolean' }
  },
  required: ['id', 'status', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  business_commands: commandSchema,
  business_profile: businessProfileSchema,
  business_onboarding_state: businessOnboardingStateSchema
};
