// RxDB projection of the canonical CTOX channel tables from runtime/ctox.sqlite3.
// Field names and types match the SQLite columns 1:1 so a CTOX-side projector can
// copy rows over without renaming. JSON-encoded TEXT columns are exposed here as
// native RxDB objects/arrays — the projector parses them once on write.
//
// Source of truth: src/core/mission/channels.rs (CREATE TABLE communication_*).
//
// LEGACY (Shell v3 only): v4 reads collections.schema.json (Port-Regel R17). This file stays only because
// modules/ctox/schema.js (fremder Strang) still imports it and the v3 app.js registers module schemas from it;
// it is deleted centrally together with the old app.js (V4-PORT.md, OFFEN). Since UC3b it no longer imports
// ../outbound/schema.js (Isolationsregel R1): the five outbound definitions and the outbound_messages migration
// are inlined verbatim (Stand 15.10.2026, gleich modules/outbound/schema.js und collections.schema.json).

export const collections = {
  outbound_campaigns: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 160
      },
      "name": {
        "type": "string"
      },
      "objective": {
        "type": "string"
      },
      "market": {
        "type": "string"
      },
      "status": {
        "type": "string"
      },
      "owner_id": {
        "type": "string"
      },
      "source_count": {
        "type": "number"
      },
      "company_count": {
        "type": "number"
      },
      "qualified_count": {
        "type": "number"
      },
      "pipeline_count": {
        "type": "number"
      },
      "payload": {
        "type": "object",
        "additionalProperties": true
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      }
    },
    "required": [
      "id",
      "name",
      "status",
      "payload",
      "created_at_ms",
      "updated_at_ms"
    ],
    "additionalProperties": true
  },
  outbound_pipeline_items: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "campaign_id": {
        "type": "string"
      },
      "company_id": {
        "type": "string"
      },
      "company_name": {
        "type": "string"
      },
      "stage": {
        "type": "string"
      },
      "contact_research_status": {
        "type": "string"
      },
      "outreach_status": {
        "type": "string"
      },
      "priority": {
        "type": "string"
      },
      "contacts": {
        "type": "array",
        "items": {
          "type": "object",
          "additionalProperties": true
        }
      },
      "payload": {
        "type": "object",
        "additionalProperties": true
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      }
    },
    "required": [
      "id",
      "campaign_id",
      "company_id",
      "company_name",
      "stage",
      "contact_research_status",
      "outreach_status",
      "payload",
      "created_at_ms",
      "updated_at_ms"
    ],
    "additionalProperties": true
  },
  outbound_engagements: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "campaign_id": {
        "type": "string"
      },
      "company_id": {
        "type": "string"
      },
      "pipeline_id": {
        "type": "string"
      },
      "contact_id": {
        "type": "string"
      },
      "sender_account_id": {
        "type": "string"
      },
      "sequence_id": {
        "type": "string"
      },
      "status": {
        "type": "string"
      },
      "next_action_at_ms": {
        "type": "number"
      },
      "paused_reason": {
        "type": "string"
      },
      "closed_reason": {
        "type": "string"
      },
      "payload": {
        "type": "object",
        "additionalProperties": true
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      }
    },
    "required": [
      "id",
      "campaign_id",
      "status",
      "payload",
      "created_at_ms",
      "updated_at_ms"
    ],
    "additionalProperties": true
  },
  outbound_messages: {
    "version": 1,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "engagement_id": {
        "type": "string"
      },
      "campaign_id": {
        "type": "string"
      },
      "channel": {
        "type": "string"
      },
      "message_type": {
        "type": "string"
      },
      "direction": {
        "type": "string"
      },
      "sender_account_id": {
        "type": "string"
      },
      "recipient_email": {
        "type": "string"
      },
      "recipient_address_text": {
        "type": "string"
      },
      "subject": {
        "type": "string"
      },
      "body_text": {
        "type": "string"
      },
      "body_html": {
        "type": "string"
      },
      "document_id": {
        "type": "string"
      },
      "document_version_id": {
        "type": "string"
      },
      "document_pdf_url": {
        "type": "string"
      },
      "physical_sent_at_ms": {
        "type": "number"
      },
      "draft_status": {
        "type": "string"
      },
      "approval_status": {
        "type": "string"
      },
      "send_status": {
        "type": "string"
      },
      "scheduled_send_at_ms": {
        "type": "number"
      },
      "sent_at_ms": {
        "type": "number"
      },
      "provider_message_id": {
        "type": "string"
      },
      "communication_account_key": {
        "type": "string"
      },
      "communication_message_key": {
        "type": "string"
      },
      "thread_key": {
        "type": "string"
      },
      "reply_to_message_id": {
        "type": "string"
      },
      "revision_id": {
        "type": "string"
      },
      "payload": {
        "type": "object",
        "additionalProperties": true
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      }
    },
    "required": [
      "id",
      "engagement_id",
      "campaign_id",
      "message_type",
      "direction",
      "draft_status",
      "approval_status",
      "send_status",
      "payload",
      "created_at_ms",
      "updated_at_ms"
    ],
    "additionalProperties": true
  },
  outbound_approvals: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "message_id": {
        "type": "string"
      },
      "engagement_id": {
        "type": "string"
      },
      "revision_id": {
        "type": "string"
      },
      "actor_user_id": {
        "type": "string"
      },
      "decision": {
        "type": "string"
      },
      "comment": {
        "type": "string"
      },
      "payload": {
        "type": "object",
        "additionalProperties": true
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      }
    },
    "required": [
      "id",
      "message_id",
      "engagement_id",
      "decision",
      "created_at_ms",
      "updated_at_ms"
    ],
    "additionalProperties": true
  },
  // Canonical definition for communication_accounts — modules/desktop imports
  // it from here. Includes the tombstone/projection bookkeeping fields the
  // Rust channel projector writes (updated_at_ms, is_deleted, deleted_at_ms).
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
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' },
      deleted_at_ms: { type: 'number' },
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

export const migrationStrategies = {
  outbound_messages: {
    1: (oldDoc) => ({
      ...oldDoc,
      channel: oldDoc.channel || oldDoc.payload?.channel || oldDoc.payload?.default_channel || 'email',
      recipient_address_text: oldDoc.recipient_address_text || oldDoc.payload?.recipient_address_text || '',
      document_id: oldDoc.document_id || '',
      document_version_id: oldDoc.document_version_id || '',
      document_pdf_url: oldDoc.document_pdf_url || '',
      physical_sent_at_ms: oldDoc.physical_sent_at_ms || 0,
    }),
  },
};
