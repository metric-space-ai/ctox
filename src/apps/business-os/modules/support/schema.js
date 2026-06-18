import { collections as ctoxCollections } from '../ctox/schema.js';
import { collections as conversationCollections } from '../conversations/schema.js';
import { collections as customerCollections } from '../customers/schema.js';
import { collections as desktopCollections } from '../desktop/schema.js';
import { collections as ticketCollections } from '../tickets/schema.js';

const textArray = { type: 'array', items: { type: 'string' } };
const jsonObject = { type: 'object', additionalProperties: true };

function supportSchema(properties, required = [], indexes = []) {
  return {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 180 },
      is_deleted: { type: 'boolean' },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' },
      deleted_at_ms: { type: 'number' },
      ...properties,
    },
    required: ['id', 'updated_at_ms', ...required],
    indexes: [
      'updated_at_ms',
      ['is_deleted', 'updated_at_ms'],
      ...indexes,
    ],
    additionalProperties: true,
  };
}

export const supportCollections = {
  support_inboxes: supportSchema({
    name: { type: 'string' },
    description: { type: 'string' },
    status: { type: 'string' },
    channel_filters_json: jsonObject,
    team_id: { type: 'string' },
    assignment_policy_id: { type: 'string' },
    sla_policy_id: { type: 'string' },
    policy_json: jsonObject,
    is_default: { type: 'boolean' },
    sort_key: { type: 'string' },
  }, ['name', 'status'], ['status', 'team_id', 'is_default']),

  support_conversations: supportSchema({
    inbox_id: { type: 'string' },
    primary_thread_key: { type: 'string', maxLength: 256 },
    status: { type: 'string' },
    priority: { type: 'string' },
    assignee_id: { type: 'string' },
    team_id: { type: 'string' },
    customer_account_id: { type: 'string' },
    customer_contact_id: { type: 'string' },
    ticket_case_id: { type: 'string' },
    last_message_key: { type: 'string' },
    last_activity_at_ms: { type: 'number' },
    waiting_since_ms: { type: 'number' },
    snoozed_until_ms: { type: 'number' },
    unread_count: { type: 'number' },
    label_ids: textArray,
    custom_attributes: jsonObject,
    search_text: { type: 'string' },
  }, ['status', 'priority', 'last_activity_at_ms', 'search_text'], [
    'inbox_id',
    'status',
    'priority',
    'assignee_id',
    'team_id',
    'customer_account_id',
    'customer_contact_id',
    'last_activity_at_ms',
    ['status', 'last_activity_at_ms'],
    ['assignee_id', 'last_activity_at_ms'],
    ['team_id', 'last_activity_at_ms'],
  ]),

  support_thread_links: supportSchema({
    conversation_id: { type: 'string' },
    thread_key: { type: 'string', maxLength: 256 },
    channel: { type: 'string' },
    account_key: { type: 'string', maxLength: 256 },
    link_role: { type: 'string' },
  }, ['conversation_id', 'thread_key'], [
    'conversation_id',
    'thread_key',
    ['conversation_id', 'updated_at_ms'],
  ]),

  support_identity_links: supportSchema({
    channel: { type: 'string' },
    account_key: { type: 'string', maxLength: 256 },
    external_identity: { type: 'string' },
    normalized_identity: { type: 'string' },
    customer_account_id: { type: 'string' },
    customer_contact_id: { type: 'string' },
    confidence: { type: 'number' },
    status: { type: 'string' },
    source: { type: 'string' },
    payload: jsonObject,
  }, ['channel', 'normalized_identity', 'status'], [
    'channel',
    'account_key',
    'normalized_identity',
    'customer_contact_id',
    'status',
  ]),

  support_notes: supportSchema({
    conversation_id: { type: 'string' },
    author_id: { type: 'string' },
    body: { type: 'string' },
    visibility: { type: 'string' },
    source: { type: 'string' },
  }, ['conversation_id', 'body', 'visibility'], [
    'conversation_id',
    'author_id',
    ['conversation_id', 'created_at_ms'],
  ]),

  support_conversation_events: supportSchema({
    conversation_id: { type: 'string' },
    event_type: { type: 'string' },
    actor_id: { type: 'string' },
    source_command_id: { type: 'string' },
    source_task_id: { type: 'string' },
    summary: { type: 'string' },
    payload: jsonObject,
    occurred_at_ms: { type: 'number' },
  }, ['conversation_id', 'event_type', 'occurred_at_ms'], [
    'conversation_id',
    'event_type',
    'source_command_id',
    ['conversation_id', 'occurred_at_ms'],
  ]),

  support_labels: supportSchema({
    title: { type: 'string' },
    color: { type: 'string' },
    description: { type: 'string' },
    sidebar: { type: 'boolean' },
  }, ['title'], ['title', 'sidebar']),

  support_label_assignments: supportSchema({
    conversation_id: { type: 'string' },
    label_id: { type: 'string' },
    assigned_by_id: { type: 'string' },
  }, ['conversation_id', 'label_id'], [
    'conversation_id',
    'label_id',
    ['conversation_id', 'updated_at_ms'],
  ]),

  support_views: supportSchema({
    title: { type: 'string' },
    owner_id: { type: 'string' },
    scope: { type: 'string' },
    position: { type: 'number' },
    filters_json: jsonObject,
    sorts_json: jsonObject,
  }, ['title', 'scope'], ['owner_id', 'scope', 'position']),

  support_view_filters: supportSchema({
    view_id: { type: 'string' },
    field: { type: 'string' },
    operator: { type: 'string' },
    value: jsonObject,
    position: { type: 'number' },
  }, ['view_id', 'field', 'operator'], ['view_id', 'field', 'position']),

  support_assignment_policies: supportSchema({
    name: { type: 'string' },
    strategy: { type: 'string' },
    fair_distribution_limit: { type: 'number' },
    fair_distribution_window_ms: { type: 'number' },
    payload: jsonObject,
  }, ['name', 'strategy'], ['strategy']),

  support_assignment_events: supportSchema({
    conversation_id: { type: 'string' },
    policy_id: { type: 'string' },
    assignee_id: { type: 'string' },
    previous_assignee_id: { type: 'string' },
    event_type: { type: 'string' },
    occurred_at_ms: { type: 'number' },
    payload: jsonObject,
  }, ['conversation_id', 'event_type', 'occurred_at_ms'], [
    'conversation_id',
    'policy_id',
    'assignee_id',
    ['conversation_id', 'occurred_at_ms'],
  ]),

  support_macros: supportSchema({
    title: { type: 'string' },
    visibility: { type: 'string' },
    owner_id: { type: 'string' },
    actions_json: { type: 'array', items: jsonObject },
    payload: jsonObject,
  }, ['title', 'visibility'], ['visibility', 'owner_id']),

  support_automation_rules: supportSchema({
    name: { type: 'string' },
    event_name: { type: 'string' },
    active: { type: 'boolean' },
    query_operator: { type: 'string' },
    conditions_json: { type: 'array', items: jsonObject },
    actions_json: { type: 'array', items: jsonObject },
  }, ['name', 'event_name', 'active'], ['event_name', 'active']),

  support_sla_policies: supportSchema({
    name: { type: 'string' },
    active: { type: 'boolean' },
    first_response_target_ms: { type: 'number' },
    next_response_target_ms: { type: 'number' },
    resolution_target_ms: { type: 'number' },
    business_hours_json: jsonObject,
    payload: jsonObject,
  }, ['name', 'active'], ['active']),

  support_applied_slas: supportSchema({
    conversation_id: { type: 'string' },
    policy_id: { type: 'string' },
    status: { type: 'string' },
    first_response_due_at_ms: { type: 'number' },
    next_response_due_at_ms: { type: 'number' },
    resolution_due_at_ms: { type: 'number' },
    breached_at_ms: { type: 'number' },
    payload: jsonObject,
  }, ['conversation_id', 'policy_id', 'status'], [
    'conversation_id',
    'policy_id',
    'status',
    'resolution_due_at_ms',
  ]),

  support_sla_events: supportSchema({
    conversation_id: { type: 'string' },
    applied_sla_id: { type: 'string' },
    event_type: { type: 'string' },
    occurred_at_ms: { type: 'number' },
    payload: jsonObject,
  }, ['conversation_id', 'event_type', 'occurred_at_ms'], [
    'conversation_id',
    'applied_sla_id',
    'event_type',
    ['conversation_id', 'occurred_at_ms'],
  ]),

  support_agent_requests: supportSchema({
    conversation_id: { type: 'string' },
    command_id: { type: 'string' },
    task_id: { type: 'string' },
    request_kind: { type: 'string' },
    status: { type: 'string' },
    required_skills: textArray,
    writeback_contract: jsonObject,
    payload: jsonObject,
  }, ['conversation_id', 'request_kind', 'status'], [
    'conversation_id',
    'command_id',
    'task_id',
    'status',
  ]),

  support_agent_suggestions: supportSchema({
    conversation_id: { type: 'string' },
    source_command_id: { type: 'string' },
    task_id: { type: 'string' },
    suggestion_kind: { type: 'string' },
    status: { type: 'string' },
    confidence: { type: 'number' },
    required_human_action: { type: 'string' },
    summary: { type: 'string' },
    payload: jsonObject,
  }, ['conversation_id', 'suggestion_kind', 'status'], [
    'conversation_id',
    'source_command_id',
    'task_id',
    'suggestion_kind',
    'status',
  ]),

  support_reporting_events: supportSchema({
    conversation_id: { type: 'string' },
    event_name: { type: 'string' },
    metric_name: { type: 'string' },
    value_ms: { type: 'number' },
    occurred_at_ms: { type: 'number' },
    payload: jsonObject,
  }, ['event_name', 'occurred_at_ms'], [
    'conversation_id',
    'event_name',
    'metric_name',
    ['event_name', 'occurred_at_ms'],
  ]),

  support_reporting_rollups: supportSchema({
    rollup_key: { type: 'string' },
    bucket_start_ms: { type: 'number' },
    bucket_end_ms: { type: 'number' },
    metric_name: { type: 'string' },
    dimensions: jsonObject,
    value: { type: 'number' },
    count: { type: 'number' },
  }, ['rollup_key', 'bucket_start_ms', 'metric_name'], [
    'rollup_key',
    'metric_name',
    'bucket_start_ms',
  ]),
};

export const collections = {
  business_chats: ctoxCollections.business_chats,
  communication_threads: conversationCollections.communication_threads,
  communication_messages: conversationCollections.communication_messages,
  ctox_ticket_cases: ticketCollections.ctox_ticket_cases,
  customer_accounts: customerCollections.customer_accounts,
  customer_contacts: customerCollections.customer_contacts,
  desktop_files: desktopCollections.desktop_files,
  desktop_file_chunks: desktopCollections.desktop_file_chunks,
  ...supportCollections,
};

export const migrationStrategies = Object.fromEntries(
  Object.keys(collections).map((collection) => [collection, {}]),
);
