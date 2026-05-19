// RxDB projection of the canonical CTOX channel tables from runtime/ctox.sqlite3.
// Field names and types match the SQLite columns 1:1 so a CTOX-side projector can
// copy rows over without renaming. JSON-encoded TEXT columns are exposed here as
// native RxDB objects/arrays — the projector parses them once on write.
//
// Source of truth: src/core/mission/channels.rs (CREATE TABLE communication_*).

export const collections = {
  communication_accounts: {
    version: 0,
    primaryKey: 'account_key',
    type: 'object',
    properties: {
      account_key: { type: 'string', maxLength: 256 },
      channel: { type: 'string' },
      address: { type: 'string' },
      provider: { type: 'string' },
      profile_json: { type: 'object', additionalProperties: true },
      created_at: { type: 'string' },
      updated_at: { type: 'string' },
      last_inbound_ok_at: { type: 'string' },
      last_outbound_ok_at: { type: 'string' },
    },
    required: ['account_key', 'channel', 'address', 'provider', 'created_at', 'updated_at'],
    additionalProperties: true,
  },
  communication_threads: {
    version: 0,
    primaryKey: 'thread_key',
    type: 'object',
    properties: {
      thread_key: { type: 'string', maxLength: 256 },
      channel: { type: 'string' },
      account_key: { type: 'string', maxLength: 256 },
      subject: { type: 'string' },
      participant_keys_json: { type: 'array', items: { type: 'string' } },
      last_message_key: { type: 'string' },
      last_message_at: { type: 'string' },
      message_count: { type: 'number' },
      unread_count: { type: 'number' },
      metadata_json: { type: 'object', additionalProperties: true },
      updated_at: { type: 'string' },
    },
    required: ['thread_key', 'channel', 'account_key', 'last_message_at', 'updated_at'],
    additionalProperties: true,
  },
  communication_messages: {
    version: 0,
    primaryKey: 'message_key',
    type: 'object',
    properties: {
      message_key: { type: 'string', maxLength: 256 },
      channel: { type: 'string' },
      account_key: { type: 'string', maxLength: 256 },
      thread_key: { type: 'string', maxLength: 256 },
      remote_id: { type: 'string' },
      direction: { type: 'string' },
      folder_hint: { type: 'string' },
      sender_display: { type: 'string' },
      sender_address: { type: 'string' },
      recipient_addresses_json: { type: 'array', items: { type: 'string' } },
      cc_addresses_json: { type: 'array', items: { type: 'string' } },
      bcc_addresses_json: { type: 'array', items: { type: 'string' } },
      subject: { type: 'string' },
      preview: { type: 'string' },
      body_text: { type: 'string' },
      body_html: { type: 'string' },
      raw_payload_ref: { type: 'string' },
      trust_level: { type: 'string' },
      status: { type: 'string' },
      seen: { type: 'number' },
      has_attachments: { type: 'number' },
      external_created_at: { type: 'string' },
      observed_at: { type: 'string' },
      metadata_json: { type: 'object', additionalProperties: true },
      route_status: { type: 'string' },
      ticket_self_work_id: { type: 'string' },
      work_id: { type: 'string' },
    },
    required: ['message_key', 'channel', 'account_key', 'thread_key', 'direction', 'external_created_at', 'observed_at'],
    additionalProperties: true,
  },
};

export const migrationStrategies = {};
