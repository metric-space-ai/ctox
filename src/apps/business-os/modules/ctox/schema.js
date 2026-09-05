export const collections = {
  ctox_crew_members: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      name: { type: 'string', maxLength: 60 },
      shape: { type: 'string', enum: ['round', 'square', 'triangle', 'blob'] },
      color: { type: 'string' },
      archived: { type: 'boolean' },
      state: { type: 'string', enum: ['home', 'on_duty', 'resting_after_failure'] },
      active_task_id: { type: ['string', 'null'] },
      soul: { type: 'object', additionalProperties: true },
      specialties: { type: 'object', additionalProperties: true },
      stats: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'name', 'shape', 'color', 'archived', 'state', 'updated_at_ms'],
    indexes: [['archived', 'state'], 'updated_at_ms'],
    additionalProperties: false
  },
  ctox_crew_learnings: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      member_id: { type: 'string', maxLength: 128 },
      text: { type: 'string', maxLength: 400 },
      kind: { type: 'string', enum: ['insight', 'pitfall', 'preference'] },
      scope: { type: 'object', additionalProperties: true },
      evidence_run_id: { type: 'string' },
      confirmed_by_owner: { type: 'boolean' },
      archived: { type: 'boolean' },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'member_id', 'text', 'kind', 'confirmed_by_owner', 'archived', 'created_at_ms', 'updated_at_ms'],
    indexes: [['member_id', 'created_at_ms'], ['member_id', 'confirmed_by_owner'], 'updated_at_ms'],
    additionalProperties: false
  },

  business_commands: {
    version: 2,
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
      result: { type: 'object', additionalProperties: true },
      execution_progress: { type: 'object', additionalProperties: true },
      task_id: { type: 'string' },
      task_status: { type: 'string' },
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
    version: 2,
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
      lease_owner: { type: 'string' },
      lease_expires_at: { type: 'string' },
      lease_worker_id: { type: 'string' },
      first_pending_at: { type: 'string' },
      failure_class: { type: 'string' },
      failure_attempt_count: { type: 'number' },
      retry_not_before: { type: 'string' },
      hold_reason: { type: 'string' },
      wait_entity_type: { type: 'string' },
      wait_entity_id: { type: 'string' },
      priority_time_credit_hours: { type: 'number' },
      attempt: { type: 'number' },
      crew_member_id: { type: 'string' },
      execution_progress: { type: 'object', additionalProperties: true },
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
    version: 1,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      task_id: { type: 'string', maxLength: 128 },
      command_id: { type: 'string' },
      work_id: { type: 'string' },
      crew_member_id: { type: 'string', maxLength: 128 },
      title: { type: 'string' },
      status: { type: 'string' },
      agent_outcome: { type: 'string' },
      source_kind: { type: 'string' },
      started_at_ms: { type: 'number' },
      finished_at_ms: { type: 'number' },
      metrics: {
        type: 'object',
        additionalProperties: true,
        properties: {
          model: { type: 'string' },
          provider: { type: 'string' },
          input_tokens: { type: 'number' },
          output_tokens: { type: 'number' },
          reasoning_tokens: { type: 'number' },
          cost_usd: { type: 'number' },
          tool_calls: { type: 'number' },
          thinking_turns: { type: 'number' },
          elapsed_ms: { type: 'number' }
        }
      },
      review: {
        type: 'object',
        additionalProperties: true,
        properties: {
          disposition: { type: 'string' },
          hold_reason: { type: 'string' }
        }
      },
      error_text: { type: 'string' },
      resumable: { type: 'boolean' },
      retrospective: { type: 'string' },
      payload: { type: 'object', additionalProperties: true },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'status', 'updated_at_ms'],
    indexes: [
      ['task_id', 'finished_at_ms'],
      'finished_at_ms',
      'crew_member_id'
    ],
    additionalProperties: true
  },
  ctox_harness_events: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      task_id: { type: 'string', maxLength: 128 },
      command_id: { type: 'string' },
      attempt: { type: 'number' },
      kind: { type: 'string' },
      title: { type: 'string' },
      tool_type: { type: 'string' },
      tool_name: { type: 'string' },
      call_id: { type: 'string' },
      success: { type: 'boolean' },
      usage: {
        type: 'object',
        additionalProperties: true,
        properties: {
          input: { type: 'number' },
          output: { type: 'number' },
          reasoning: { type: 'number' },
          total: { type: 'number' }
        }
      },
      runtime_seconds: { type: 'number' },
      step_position: { type: 'number' },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'task_id', 'kind', 'created_at_ms', 'updated_at_ms'],
    indexes: [
      ['task_id', 'created_at_ms'],
      'created_at_ms'
    ],
    additionalProperties: true
  },
  ctox_harness_status: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      service_running: { type: 'boolean' },
      busy: { type: 'boolean' },
      paused: { type: 'boolean' },
      pause_reason: { type: 'string' },
      worker_active_count: { type: 'number' },
      worker_phase: { type: 'string' },
      worker_capacity: { type: 'number' },
      pending_count: { type: 'number' },
      leased_count: { type: 'number' },
      blocked_count: { type: 'number' },
      review_count: { type: 'number' },
      failed_recent_count: { type: 'number' },
      pressure_active: { type: 'boolean' },
      pressure_threshold: { type: 'number' },
      work_hours: {
        type: 'object',
        additionalProperties: true,
        properties: {
          enabled: { type: 'boolean' },
          start: { type: 'string' },
          end: { type: 'string' },
          inside_window: { type: 'boolean' }
        }
      },
      active_task_ids: {
        type: 'array',
        items: { type: 'string' }
      },
      active_crew_member_id: { type: 'string' },
      last_error: { type: 'string' },
      boot_id: { type: 'string' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'updated_at_ms'],
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
      is_deleted: { type: 'boolean' },
      profile: { type: 'object', additionalProperties: true }
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
      commit_id: { type: 'string' },
      size_bytes: { type: 'number' },
      content: { type: 'string' },
      source_kind: { type: 'string' },
      synced_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'module_id', 'path', 'sha256', 'updated_at_ms'],
    additionalProperties: true
  },
  business_module_commits: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 512 },
      module_id: { type: 'string' },
      seq: { type: 'number' },
      parent_id: { type: 'string' },
      bundle_sha256: { type: 'string' },
      message: { type: 'string' },
      origin: { type: 'string' },
      label: { type: 'string' },
      author: { type: 'string' },
      authored_at_ms: { type: 'number' },
      sealed: { type: 'boolean' },
      file_manifest: { type: 'array', items: { type: 'object', additionalProperties: true } },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' }
    },
    required: ['id', 'module_id', 'seq', 'bundle_sha256', 'origin', 'authored_at_ms', 'updated_at_ms'],
    additionalProperties: true
  },
  business_module_source_blob_chunks: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 180 },
      blob_id: { type: 'string' },
      module_id: { type: 'string' },
      commit_id: { type: 'string' },
      idx: { type: 'number' },
      total: { type: 'number' },
      encoding: { type: 'string' },
      data: { type: 'string' },
      created_at_ms: { type: 'number' }
    },
    required: ['id', 'blob_id', 'module_id', 'commit_id', 'idx', 'total', 'encoding', 'data', 'created_at_ms'],
    additionalProperties: false
  },
  workjet_projects: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 128 },
      name: { type: 'string', maxLength: 256 },
      description: { type: 'string', maxLength: 4096 },
      status: { type: 'string', enum: ['active', 'archived'] },
      owner_user_id: { type: 'string', maxLength: 256 },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' },
      archived_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' }
    },
    required: ['id', 'name', 'status', 'owner_user_id', 'created_at_ms', 'updated_at_ms'],
    indexes: [
      'owner_user_id',
      'status',
      'updated_at_ms',
      ['owner_user_id', 'status', 'updated_at_ms']
    ],
    additionalProperties: false
  },
  workjet_working_copies: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 160 },
      project_id: { type: 'string', maxLength: 128 },
      computer_id: { type: 'string', maxLength: 256 },
      path: { type: 'string', maxLength: 4096 },
      label: { type: 'string', maxLength: 256 },
      status: { type: 'string', enum: ['active', 'detached', 'missing'] },
      owner_user_id: { type: 'string', maxLength: 256 },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' },
      verified_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' }
    },
    required: ['id', 'project_id', 'computer_id', 'path', 'status', 'owner_user_id', 'created_at_ms', 'updated_at_ms'],
    indexes: [
      'project_id',
      'computer_id',
      'status',
      'updated_at_ms',
      ['project_id', 'computer_id', 'status']
    ],
    additionalProperties: false
  },
  workjet_computers: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 256 },
      display_name: { type: 'string', maxLength: 256 },
      hosting_mode: { type: 'string', enum: ['workstation', 'self_hosted'] },
      status: { type: 'string', enum: ['assigned', 'unassigned'] },
      capabilities: {
        type: 'array',
        maxItems: 32,
        items: { type: 'string', maxLength: 80 }
      },
      self_hosted_colocation: { type: 'boolean' },
      device_binding_id: { type: 'string', maxLength: 160 },
      actor_epoch: { type: 'integer', minimum: 0 },
      last_seen_at_ms: { type: 'integer', minimum: 0 },
      replication_up: { type: 'boolean' },
      owner_user_id: { type: 'string', maxLength: 256 },
      created_at_ms: { type: 'integer', minimum: 0 },
      updated_at_ms: { type: 'integer', minimum: 0 },
      unassigned_at_ms: { type: 'integer', minimum: 0 },
      is_deleted: { type: 'boolean' }
    },
    required: [
      'id', 'display_name', 'hosting_mode', 'status', 'capabilities',
      'self_hosted_colocation', 'device_binding_id', 'actor_epoch',
      'last_seen_at_ms', 'replication_up', 'owner_user_id', 'created_at_ms',
      'updated_at_ms', 'is_deleted'
    ],
    indexes: [
      'owner_user_id',
      'status',
      'last_seen_at_ms',
      'updated_at_ms',
      ['owner_user_id', 'status', 'updated_at_ms']
    ],
    additionalProperties: false
  },
  workjet_sessions: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 160 },
      project_id: { type: 'string', maxLength: 128 },
      thread_id: { type: 'string', maxLength: 160 },
      coding_session_id: { type: 'string', maxLength: 128 },
      working_copy_id: { type: 'string', maxLength: 160 },
      computer_id: { type: 'string', maxLength: 256 },
      run_status: {
        type: 'string',
        enum: ['running', 'pausing', 'paused', 'transferring', 'resuming', 'transfer_failed']
      },
      fence_epoch: { type: 'integer', minimum: 0 },
      active_transfer_id: { type: 'string', maxLength: 160 },
      last_terminal_turn_id: { type: 'string', maxLength: 160 },
      owner_user_id: { type: 'string', maxLength: 256 },
      created_at_ms: { type: 'integer', minimum: 0 },
      updated_at_ms: { type: 'integer', minimum: 0 },
      is_deleted: { type: 'boolean' }
    },
    required: [
      'id', 'project_id', 'working_copy_id', 'computer_id', 'run_status',
      'fence_epoch', 'owner_user_id', 'created_at_ms', 'updated_at_ms',
      'is_deleted'
    ],
    indexes: [
      'owner_user_id',
      'project_id',
      'working_copy_id',
      'computer_id',
      'run_status',
      'updated_at_ms',
      ['owner_user_id', 'run_status', 'updated_at_ms']
    ],
    additionalProperties: false
  },
  workjet_session_transfers: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 160 },
      session_id: { type: 'string', maxLength: 160 },
      project_id: { type: 'string', maxLength: 128 },
      source_working_copy_id: { type: 'string', maxLength: 160 },
      source_computer_id: { type: 'string', maxLength: 256 },
      target_computer_id: { type: 'string', maxLength: 256 },
      target_path: { type: 'string', maxLength: 4096 },
      target_working_copy_id: { type: 'string', maxLength: 160 },
      state: {
        type: 'string',
        enum: [
          'pause_requested', 'packing', 'packed', 'shipping', 'applying',
          'applied', 'switching', 'resuming', 'completed', 'aborting',
          'rolled_back', 'failed'
        ]
      },
      fence_epoch: { type: 'integer', minimum: 1 },
      mode: { type: 'string', enum: ['git', 'copy'] },
      manifest_file_id: { type: 'string', maxLength: 160 },
      artifact_file_ids: {
        type: 'array',
        maxItems: 64,
        items: { type: 'string', maxLength: 160 }
      },
      artifact_generation_id: { type: 'string', maxLength: 160 },
      manifest_sha256: { type: 'string', minLength: 64, maxLength: 64, pattern: '^[0-9a-f]{64}$' },
      git: {
        type: 'object',
        properties: {
          head: { type: 'string', minLength: 40, maxLength: 64, pattern: '^(?:[0-9a-f]{40}|[0-9a-f]{64})$' },
          branch: { type: 'string', maxLength: 256 },
          base_commit: { type: 'string', minLength: 40, maxLength: 64, pattern: '^(?:[0-9a-f]{40}|[0-9a-f]{64})$' },
          bundle_file_id: { type: 'string', maxLength: 160 },
          patch_file_id: { type: 'string', maxLength: 160 },
          patch_sha256: { type: 'string', minLength: 64, maxLength: 64, pattern: '^[0-9a-f]{64}$' },
          untracked_file_id: { type: 'string', maxLength: 160 },
          untracked_sha256: { type: 'string', minLength: 64, maxLength: 64, pattern: '^[0-9a-f]{64}$' },
          dirty: { type: 'boolean' }
        },
        required: [
          'head', 'branch', 'base_commit', 'patch_file_id', 'patch_sha256',
          'untracked_file_id', 'untracked_sha256', 'dirty'
        ],
        additionalProperties: false
      },
      tree_sha256: { type: 'string', minLength: 64, maxLength: 64, pattern: '^[0-9a-f]{64}$' },
      error_code: { type: 'string', maxLength: 128 },
      error_detail: { type: 'string', maxLength: 512 },
      deadline_at_ms: { type: 'integer', minimum: 0 },
      created_at_ms: { type: 'integer', minimum: 0 },
      updated_at_ms: { type: 'integer', minimum: 0 },
      completed_at_ms: { type: 'integer', minimum: 0 },
      rolled_back_at_ms: { type: 'integer', minimum: 0 },
      owner_user_id: { type: 'string', maxLength: 256 },
      is_deleted: { type: 'boolean' }
    },
    required: [
      'id', 'session_id', 'project_id', 'source_working_copy_id',
      'source_computer_id', 'target_computer_id', 'target_path', 'state',
      'fence_epoch', 'artifact_file_ids', 'deadline_at_ms', 'created_at_ms',
      'updated_at_ms', 'owner_user_id', 'is_deleted'
    ],
    indexes: [
      'owner_user_id',
      'session_id',
      'project_id',
      'state',
      'deadline_at_ms',
      'updated_at_ms',
      ['owner_user_id', 'state', 'updated_at_ms']
    ],
    additionalProperties: false
  }
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || ''
    }),
    2: (oldDoc) => oldDoc
  },
  ctox_queue_tasks: {
    1: (oldDoc) => oldDoc,
    2: (oldDoc) => oldDoc
  },
  ctox_runs: {
    1: (oldDoc) => oldDoc
  }
};
