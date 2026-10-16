// Origin: CTOX
// License: Apache-2.0
// support.note.create – ref: support.rs:733-782
import {
  commandConversationId, createRun, ensureConversation, fail, firstString, insertConversationEvent,
  insertNote, normalizeVisibility, updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const noteId = firstString(payload, ['id', 'note_id']) ?? run.newId('support_note_');
    const body = firstString(payload, ['body', 'text', 'note']);
    if (body === undefined) fail('invalid', 'body is required');
    const visibility = normalizeVisibility(firstString(payload, ['visibility']) ?? 'internal');
    await insertNote(store, run, noteId, conversationId, run.actorId, body, visibility,
      firstString(payload, ['source']) ?? 'business-os.support');
    await updateConversationFields(store, run, conversationId, { last_activity_at_ms: run.now });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.note.created',
      summary: 'Internal support note created.',
      payload: { note_id: noteId, visibility },
    });
    return run.result({ conversation_id: conversationId, note_id: noteId });
  });
}
