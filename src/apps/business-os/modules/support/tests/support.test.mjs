import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  SUPPORT_AGENT_SUGGESTION_KINDS,
  buildAgentWritebackCommand,
  buildSupportAgentTaskCommand,
  buildSupportCommand,
} from '../support-commands.mjs';
import {
  filterSupportConversations,
  mergeSupportTimeline,
  supportQueueCounts,
} from '../support-reducers.mjs';

const testDir = dirname(fileURLToPath(import.meta.url));
const moduleDir = resolve(testDir, '..');
const manifest = JSON.parse(readFileSync(resolve(moduleDir, 'module.json'), 'utf8'));
const schemaDocument = JSON.parse(readFileSync(resolve(moduleDir, 'collections.schema.json'), 'utf8'));
const collections = schemaDocument.collections || {};

for (const name of manifest.collections) {
  if (name === 'business_commands' || name === 'ctox_queue_tasks') continue;
  assert.ok(collections[name], `${name} is declared by support/collections.schema.json`);
}

assert.ok(collections.business_chats, 'support imports canonical business_chats for CTOX Harness replies');
assert.ok(collections.communication_messages, 'support imports canonical communication messages for the timeline');
assert.ok(collections.ctox_ticket_cases, 'support imports canonical ticket cases for context');
assert.ok(collections.customer_accounts, 'support imports canonical customer accounts for context');
assert.ok(collections.customer_contacts, 'support imports canonical customer contacts for context');
assert.ok(collections.desktop_files, 'support imports canonical desktop files for reply attachments');
assert.ok(collections.desktop_file_chunks, 'support imports canonical desktop file chunks for attachment content');
assert.ok(collections.support_agent_suggestions, 'agent suggestions collection exists');
assert.ok(SUPPORT_AGENT_SUGGESTION_KINDS.includes('draft_reply'), 'draft reply suggestions are allowed');

const command = buildSupportCommand({
  type: 'support.conversation.claim',
  recordId: 'conv_1',
  payload: { conversation_id: 'conv_1' },
});
assert.equal(command.module, 'support');
assert.equal(command.type, 'support.conversation.claim');
assert.equal(command.command_type, 'support.conversation.claim');
assert.equal(command.payload.conversation_id, 'conv_1');
assert.equal(Object.hasOwn(command.client_context, 'actor'), false);

const taskCommand = buildSupportAgentTaskCommand({
  conversationId: 'conv_1',
  title: 'Support summary',
  instruction: 'Summarize',
});
assert.equal(taskCommand.type, 'business_os.chat.task');
assert.equal(taskCommand.command_type, 'business_os.chat.task');
assert.equal(taskCommand.payload.thread_key, 'business-os/support/conv_1');
assert.equal(taskCommand.payload.writeback_contract.command_type, 'support.agent.writeback');
assert.equal(taskCommand.payload.writeback_contract.collection, 'support_agent_suggestions');

const writeback = buildAgentWritebackCommand({
  conversationId: 'conv_1',
  sourceCommandId: taskCommand.id,
  suggestionKind: 'summary',
  payload: { summary: 'Customer waits for contract data.' },
});
assert.equal(writeback.type, 'support.agent.writeback');
assert.equal(writeback.payload.source_command_id, taskCommand.id);
assert.equal(writeback.payload.required_human_action, 'review');

const bulkCommand = buildSupportCommand({
  type: 'support.bulk.resolve',
  payload: { conversation_ids: ['conv_1', 'conv_2'] },
});
assert.equal(bulkCommand.type, 'support.bulk.resolve');

const reportingCommand = buildSupportCommand({
  type: 'support.reporting.rebuild_rollups',
});
assert.equal(reportingCommand.type, 'support.reporting.rebuild_rollups');

const conversations = [
  { id: 'conv_a', status: 'open', assignee_id: '', priority: 'high', unread_count: 1, updated_at_ms: 1, last_activity_at_ms: 20, search_text: 'alpha contract' },
  { id: 'conv_b', status: 'resolved', assignee_id: 'user-1', priority: 'low', updated_at_ms: 2, last_activity_at_ms: 10, search_text: 'beta' },
  { id: 'conv_c', status: 'open', assignee_id: 'user-1', priority: 'normal', updated_at_ms: 3, last_activity_at_ms: 30, search_text: 'gamma' },
  { id: 'conv_d', status: 'waiting', assignee_id: 'user-1', priority: 'normal', updated_at_ms: 4, last_activity_at_ms: 40, search_text: 'delta' },
];
assert.deepEqual(filterSupportConversations(conversations, { status: 'open', query: 'alpha' }).map((item) => item.id), ['conv_a']);
assert.deepEqual(filterSupportConversations(conversations, { status: 'open' }).map((item) => item.id), ['conv_d', 'conv_c', 'conv_a']);
assert.equal(supportQueueCounts(conversations, 1000, 'user-1').open, 3);
assert.equal(supportQueueCounts(conversations, 1000, 'user-1').mine, 2);
assert.equal(supportQueueCounts(conversations, 1000, 'user-1').needsReply, 1);

const timeline = mergeSupportTimeline({
  notes: [{ id: 'note_1', created_at_ms: 30, body: 'internal' }],
  events: [{ id: 'event_1', occurred_at_ms: 20, event_type: 'claimed' }],
  suggestions: [{ id: 'suggestion_1', created_at_ms: 40, suggestion_kind: 'summary' }],
});
assert.deepEqual(timeline.map((row) => row.id), ['event_1', 'note_1', 'suggestion_1']);

console.log('support module phase-0 smoke OK');
