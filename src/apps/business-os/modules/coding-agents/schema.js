export const collections = {
  coding_agent_workspace_grants: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      provider: { type: 'string' },
      path: { type: 'string' },
      status: { type: 'string' },
      active: { type: 'boolean' },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' }
    },
    required: ['id', 'provider', 'path', 'status', 'updated_at_ms'],
    indexes: ['provider', 'path', 'updated_at_ms'],
    additionalProperties: true
  },
  coding_agent_sessions: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      session_id: { type: 'string' },
      provider: { type: 'string' },
      workspace_root: { type: 'string' },
      status: { type: 'string' },
      title: { type: 'string' },
      last_prompt: { type: 'string' },
      external_session_id: { type: 'string' },
      metadata: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' }
    },
    required: ['id', 'session_id', 'provider', 'workspace_root', 'status', 'updated_at_ms'],
    indexes: ['provider', 'workspace_root', 'status', 'updated_at_ms'],
    additionalProperties: true
  },
  coding_agent_events: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      event_id: { type: 'string' },
      session_id: { type: 'string' },
      provider: { type: 'string' },
      role: { type: 'string' },
      text: { type: 'string' },
      status: { type: 'string' },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' }
    },
    required: ['id', 'event_id', 'session_id', 'provider', 'role', 'text', 'status', 'updated_at_ms'],
    indexes: ['session_id', 'provider', 'created_at_ms'],
    additionalProperties: true
  }
};

export const migrationStrategies = {};
