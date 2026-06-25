import assert from 'node:assert/strict';
import {
  THREAD_COLLECTIONS,
  buildApprovalRequestPayload,
  buildNotePayload,
  buildThreadsCommand,
  splitUserIds,
} from '../commands.js';
import { collections } from '../schema.js';

assert.ok(THREAD_COLLECTIONS.includes('user_threads'));
assert.ok(collections.user_threads);
assert.ok(collections.user_thread_messages);
assert.ok(collections.ctox_task_approval_requests);

assert.deepEqual(splitUserIds('alice, bob  alice\ncarol'), ['alice', 'bob', 'carol']);

const notePayload = buildNotePayload({
  body: ' Bitte pruefen ',
  targetUserIds: 'alice,bob',
  sourceContext: { module: 'tickets', record_id: 'T-1', label: 'Ticket 1', deep_link: '#tickets?record=T-1' },
});
assert.equal(notePayload.body, 'Bitte pruefen');
assert.deepEqual(notePayload.target_user_ids, ['alice', 'bob']);
assert.equal(notePayload.source_context.module, 'tickets');
assert.equal(notePayload.source_context.deep_link, '#tickets?record=T-1');

const approvalPayload = buildApprovalRequestPayload({
  prompt: 'CTOX soll das Ticket beantworten',
  reviewerUserId: 'lead',
  sourceContext: { module: 'support', record_id: 'conv-1', label: 'Kunde A' },
});
assert.equal(approvalPayload.reviewer_user_id, 'lead');
assert.equal(approvalPayload.target_module, 'support');
assert.equal(approvalPayload.target_record_id, 'conv-1');
assert.equal(approvalPayload.target_command_type, 'business_os.chat.task');

const command = buildThreadsCommand({
  commandType: 'threads.note.create',
  payload: notePayload,
  sourceModule: 'tickets',
});
assert.match(command.id, /^cmd_[0-9a-f-]{36}$/);
assert.equal(command.module, 'threads');
assert.equal(command.command_type, 'threads.note.create');
assert.equal(command.inbound_channel, 'tickets');
assert.equal(command.client_context.module_id, 'threads');

console.log('threads module smoke ok');
