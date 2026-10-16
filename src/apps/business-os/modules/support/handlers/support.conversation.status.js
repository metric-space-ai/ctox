// Origin: CTOX
// License: Apache-2.0
// support.conversation.status – ref: support.rs:572-590, 2960-3009
// Rust-Treue: anders als bulk.status prüft dieser Pfad bei "resolved" KEINE Pflichtfelder
// und setzt angewendete SLAs nicht auf resolved (§3 U8).
import {
  commandConversationId, createRun, ensureConversation, fail, firstString, normalizeStatus,
  updateConversationStatus, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const raw = firstString(payload, ['status']);
    if (raw === undefined) fail('invalid', 'status is required');
    const status = normalizeStatus(raw);
    await updateConversationStatus(store, run, conversationId, status);
    return run.result({ conversation_id: conversationId, status });
  });
}
