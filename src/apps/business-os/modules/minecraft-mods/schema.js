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

const modProjectSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    title: { type: 'string' },
    loader: { type: 'string' },
    minecraft_version: { type: 'string' },
    project_path: { type: 'string' },
    package_id: { type: 'string' },
    status: { type: 'string' },
    notes: { type: 'string' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'title', 'loader', 'updated_at_ms'],
  additionalProperties: true
};

const modArtifactSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    project_id: { type: 'string' },
    filename: { type: 'string' },
    jar_path: { type: 'string' },
    loader: { type: 'string' },
    mod_id: { type: 'string' },
    version: { type: 'string' },
    sha256: { type: 'string' },
    build_status: { type: 'string' },
    metadata_json: { type: 'object', additionalProperties: true },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'filename', 'loader', 'mod_id', 'updated_at_ms'],
  additionalProperties: true
};

const modInstallationSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    artifact_id: { type: 'string' },
    target_name: { type: 'string' },
    minecraft_dir: { type: 'string' },
    profile: { type: 'string' },
    status: { type: 'string' },
    manifest_path: { type: 'string' },
    installed_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'target_name', 'minecraft_dir', 'updated_at_ms'],
  additionalProperties: true
};

const modMergeSetSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    title: { type: 'string' },
    source_dirs: { type: 'array', items: { type: 'string' } },
    target_dir: { type: 'string' },
    status: { type: 'string' },
    conflicts_json: { type: 'array', items: { type: 'object', additionalProperties: true } },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'title', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  business_commands: commandSchema,
  minecraft_mod_projects: modProjectSchema,
  minecraft_mod_artifacts: modArtifactSchema,
  minecraft_mod_installations: modInstallationSchema,
  minecraft_mod_merge_sets: modMergeSetSchema
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || ''
    })
  }
};

