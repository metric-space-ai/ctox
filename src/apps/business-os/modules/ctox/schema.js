export const collections = {
  business_commands: {
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
    indexes: [
      'status',
      'command_id',
      ['status', 'updated_at_ms'],
      ['module', 'command_type', 'status', 'updated_at_ms']
    ],
    additionalProperties: true
  },
  ctox_queue_tasks: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      command_id: { type: 'string' },
      command_type: { type: 'string' },
      title: { type: 'string' },
      status: { type: 'string' },
      module: { type: 'string' },
      source_module: { type: 'string' },
      inbound_channel: { type: 'string' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'title', 'status', 'module'],
    indexes: [
      'status',
      'command_id',
      'updated_at_ms',
      ['status', 'updated_at_ms'],
      ['command_id', 'status']
    ],
    additionalProperties: true
  },
  business_chats: {
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
      tracking_active: { type: 'boolean' },
      tracking_status: { type: 'string' },
      tracking_id: { type: 'string' },
      tracking_command_id: { type: 'string' },
      tracking_task_id: { type: 'string' },
      tracking_message_id: { type: 'string' },
      messages: {
        type: 'array',
        items: { type: 'object', additionalProperties: true }
      },
      draft: { type: 'string' },
      createdAt: { type: 'number' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'title', 'updated_at_ms'],
    indexes: [
      'owner_user_id',
      'lastTrackingId',
      'tracking_active',
      'tracking_status',
      'tracking_id',
      'tracking_command_id',
      'tracking_task_id',
      'updated_at_ms',
      ['tracking_active', 'updated_at_ms'],
      ['tracking_active', 'tracking_status', 'updated_at_ms']
    ],
    additionalProperties: true
  },
  ctox_runs: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      work_id: { type: 'string' },
      title: { type: 'string' },
      status: { type: 'string' },
      source_kind: { type: 'string' },
      started_at_ms: { type: 'number' },
      finished_at_ms: { type: 'number' },
      metrics: { type: 'object', additionalProperties: true },
      payload: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'status', 'updated_at_ms'],
    additionalProperties: true
  },
  ctox_runtime_settings: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      ok: { type: 'boolean' },
      can_manage: { type: 'boolean' },
      runtime: { type: 'object', additionalProperties: true },
      auth: { type: 'object', additionalProperties: true },
      service: { type: 'object', additionalProperties: true },
      diagnostics: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' },
    },
    required: ['id', 'runtime', 'auth', 'diagnostics', 'updated_at_ms'],
    additionalProperties: true
  },
  business_workspace_branding: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      ok: { type: 'boolean' },
      custom: { type: 'boolean' },
      name: { type: 'string' },
      light: { type: 'object', additionalProperties: true },
      dark: { type: 'object', additionalProperties: true },
      module_accents: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' },
    },
    required: ['id', 'name', 'light', 'dark', 'module_accents', 'updated_at_ms'],
    additionalProperties: true
  },
  business_module_catalog: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      ok: { type: 'boolean' },
      modules: {
        type: 'array',
        items: { type: 'object', additionalProperties: true }
      },
      templates: {
        type: 'array',
        items: { type: 'object', additionalProperties: true }
      },
      governance: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' },
    },
    required: ['id', 'modules', 'templates', 'updated_at_ms'],
    additionalProperties: true
  },
  ctox_bug_reports: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      title: { type: 'string' },
      status: { type: 'string' },
      module: { type: 'string' },
      inbound_channel: { type: 'string' },
      severity: { type: 'string' },
      surface: { type: 'string' },
      description: { type: 'string' },
      evidence: { type: 'object', additionalProperties: true },
      payload: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'title', 'status', 'updated_at_ms'],
    additionalProperties: true
  },
  business_module_acl: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 256 },
      module_id: { type: 'string' },
      user_id: { type: 'string' },
      role: { type: 'string' },
      active: { type: 'boolean' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'module_id', 'user_id', 'role', 'updated_at_ms'],
    additionalProperties: true
  },
  business_users: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 256 },
      user_id: { type: 'string', maxLength: 256 },
      display_name: { type: 'string' },
      role: { type: 'string' },
      active: { type: 'boolean' },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' }
    },
    required: ['id', 'display_name', 'role', 'active', 'updated_at_ms'],
    additionalProperties: true
  },
  business_module_releases: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 256 },
      version_id: { type: 'string' },
      module_id: { type: 'string' },
      version: { type: 'number' },
      status: { type: 'string' },
      created_by: { type: 'string' },
      created_at_ms: { type: 'number' },
      notes: { type: 'string' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'module_id', 'status', 'updated_at_ms'],
    additionalProperties: true
  },
  business_module_reports: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 256 },
      report_id: { type: 'string' },
      module_id: { type: 'string' },
      kind: { type: 'string' },
      severity: { type: 'string' },
      title: { type: 'string' },
      summary: { type: 'string' },
      expected: { type: 'string' },
      status: { type: 'string' },
      reporter_id: { type: 'string' },
      ctox_command_id: { type: 'string' },
      task_id: { type: 'string' },
      client_context: { type: 'object', additionalProperties: true },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'module_id', 'kind', 'title', 'status', 'updated_at_ms'],
    additionalProperties: true
  },
  business_module_source_files: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 512 },
      module_id: { type: 'string' },
      path: { type: 'string' },
      language: { type: 'string' },
      sha256: { type: 'string' },
      previous_sha256: { type: 'string' },
      snapshot_id: { type: 'string' },
      size_bytes: { type: 'number' },
      content: { type: 'string' },
      source_kind: { type: 'string' },
      synced_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'module_id', 'path', 'sha256', 'updated_at_ms'],
    additionalProperties: true
  }
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || ''
    })
  }
};
