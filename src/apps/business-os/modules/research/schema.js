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

const businessChatSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    title: { type: 'string' },
    open: { type: 'boolean' },
    minimized: { type: 'boolean' },
    owner_user_id: { type: 'string' },
    lastTrackingId: { type: 'string' },
    messages: {
      type: 'array',
      items: { type: 'object', additionalProperties: true },
    },
    draft: { type: 'string' },
    createdAt: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'title', 'updated_at_ms'],
  additionalProperties: true,
};

const ctoxQueueTaskSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    command_id: { type: 'string' },
    title: { type: 'string' },
    status: { type: 'string' },
    route_status: { type: 'string' },
    module: { type: 'string' },
    source_module: { type: 'string' },
    inbound_channel: { type: 'string' },
    command_type: { type: 'string' },
    priority: { type: 'string' },
    thread_key: { type: 'string' },
    prompt: { type: 'string' },
    workspace_root: { type: 'string' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'title', 'status', 'module'],
  additionalProperties: true,
};

const researchTaskSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    title: { type: 'string' },
    prompt: { type: 'string' },
    criteria: { type: 'string' },
    status: { type: 'string' },
    knowledge_domain: { type: 'string' },
    source_catalog_key: { type: 'string' },
    curated_table_key: { type: 'string' },
    measurements_table_key: { type: 'string' },
    x_axis: { type: 'string' },
    y_axis: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'title', 'status', 'knowledge_domain', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

const researchRunSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    task_id: { type: 'string' },
    status: { type: 'string' },
    command_id: { type: 'string' },
    task_queue_id: { type: 'string' },
    identified_count: { type: 'number' },
    accepted_count: { type: 'number' },
    used_count: { type: 'number' },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'task_id', 'status', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

const researchNoteSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    task_id: { type: 'string' },
    kind: { type: 'string' },
    title: { type: 'string' },
    body: { type: 'string' },
    status: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'task_id', 'kind', 'title', 'payload', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true,
};

const knowledgeRecordSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    kind: { type: 'string' },
    title: { type: 'string' },
    subtitle: { type: 'string' },
    summary: { type: 'string' },
    source_path: { type: 'string' },
    updated_at: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    updated_at_ms: { type: 'number' },
  },
  required: ['id', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  business_commands: commandSchema,
  business_chats: businessChatSchema,
  ctox_queue_tasks: ctoxQueueTaskSchema,
  research_tasks: researchTaskSchema,
  research_runs: researchRunSchema,
  research_notes: researchNoteSchema,
  knowledge_tables: knowledgeRecordSchema,
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || '',
    }),
  },
};
