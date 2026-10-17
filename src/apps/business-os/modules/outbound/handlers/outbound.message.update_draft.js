// ref: store_outbound_commands.rs:501-550 ("outbound.message.update_draft")
import { requiredFromPayloadOrRecord, loadRequired, str, fail, mergeFields, messageRevision } from './_lib/outbound.js';

const EDITABLE_FIELDS = [
  'recipient_email', 'recipient_address_text', 'recipient_address', 'channel', 'subject',
  'body_text', 'body_html', 'sender_account_id', 'scheduled_send_at_ms', 'payload',
];

export default async function handle(command, ctx) {
  const now = ctx.now();
  const messageId = requiredFromPayloadOrRecord(command, ['message_id', 'id'], 'message_id is required');
  return ctx.transaction(async (tx) => {
    const message = await loadRequired(tx, 'outbound_messages', messageId, 'message not found');
    const sendStatus = str(message, ['send_status']);
    if (sendStatus === 'queued_for_provider' || sendStatus === 'sent' || sendStatus === 'accepted') {
      fail('conflict', 'sent or queued outbound message drafts cannot be edited');
    }
    mergeFields(message, command.payload, EDITABLE_FIELDS);
    message.draft_status = 'prepared';
    message.approval_status = 'draft';
    message.send_status = 'not_scheduled';
    message.revision_id = messageRevision(message);
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
    return { ok: true, message };
  });
}
