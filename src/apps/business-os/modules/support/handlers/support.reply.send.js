// Origin: CTOX
// License: Apache-2.0
// support.reply.send – ref: support.rs:917-960, 1963-1975
// Rust versendet NICHT: der Befehl legt nur eine Freigabe-Übergabe an ("pending_approval");
// send_mode "direct" wird abgewiesen. Damit reine Fachlogik (§3 U2). Der spätere Versand nach
// Freigabe ist Sache von ctx.cap.mail.send (§2 W2) und heute gar nicht verdrahtet.
import {
  attachmentRefsFromPayload, commandConversationId, createRun, ensureConversation, fail, firstString,
  insertConversationEvent, insertReportingEvent, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const sendMode = firstString(payload, ['send_mode', 'mode']) ?? 'approval_required';
    if (sendMode === 'direct') {
      fail('unavailable', 'support.reply.send direct mode requires a configured channel send gateway');
    }
    const body = firstString(payload, ['body', 'text', 'reply']);
    if (body === undefined) fail('invalid', 'reply body is required');
    const approvalId = firstString(payload, ['approval_id']) ?? run.newId('support_reply_approval_');
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.reply.pending_approval',
      summary: 'Support reply queued for human approval.',
      payload: {
        approval_id: approvalId,
        body,
        attachment_refs: attachmentRefsFromPayload(payload),
        channel: firstString(payload, ['channel']) ?? '',
        thread_key: firstString(payload, ['thread_key']) ?? '',
      },
    });
    await insertReportingEvent(store, run, conversationId, 'support.reply.pending_approval', 'count', 1, { approval_id: approvalId });
    return run.result({ conversation_id: conversationId, approval_id: approvalId, status: 'pending_approval' });
  });
}
