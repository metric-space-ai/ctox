// The credentials app reads/writes ONLY the shared business_commands
// collection — there is no synced secrets collection. This schema MUST stay
// byte-identical to the canonical business_commands schema used by every other
// module (see src/apps/business-os/rxdb/src/schema.mjs and
// src/core/business_os/business_os_schema_hashes.json); a drifted hash silently
// quiesces the collection for this peer.
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

export const collections = {
  business_commands: commandSchema
};

export const migrationStrategies = {
  business_commands: {
    1: (oldDoc) => ({
      ...oldDoc,
      inbound_channel: oldDoc.inbound_channel || oldDoc.module || ''
    })
  }
};
