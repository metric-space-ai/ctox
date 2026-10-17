// Origin: CTOX
// License: Apache-2.0
// support.intake.project – ZUSÄTZLICHER Handler (kein Befehlstyp in Rust), Port der
// Projektionsschleife project_communication_intake – ref: support.rs:1335-1506, Aufruf rxdb_peer.rs:5605.
// Rust liest communication_threads/communication_messages aus der Kanal-DB des Core
// (channels::pull_communication_*_for_business_os: updated_at_ms > since, aufsteigend, limit).
// Hier dieselben Daten aus den Fremd-Collections communication_threads/communication_messages
// (Manifest: foreign_collections read). Auslöser fehlt noch: braucht Zeitplan oder Collection-
// Ereignis (§2 W4); den Cursor max_updated_at_ms hält heute der Aufrufer.
import {
  createRun, firstString, insertConversationEvent, linkedConversationForThread, loadConversation,
  numberField, NAMESPACE_URL, supportThreadLinkCurrent, upsertConversationFromPayload, upsertThreadLink,
  uuidV5, withStore,
} from './_shared.js';

// ref: support.rs:1504-1506
function isSupportInternalThreadKey(threadKey) {
  return threadKey.trim().startsWith('business-os/support/');
}

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const sinceMs = Math.max(0, numberField(run.payload, ['since_ms']) ?? 0);
  const limit = Math.min(2000, Math.max(1, numberField(run.payload, ['limit']) ?? 500));
  const query = { selector: { updated_at_ms: { $gt: sinceMs } }, sort: [{ updated_at_ms: 'asc' }], limit };
  const threads = await ctx.read('communication_threads', query);
  if (!threads.length) return run.result({ changed_count: 0, max_updated_at_ms: sinceMs });
  const maxUpdatedAtMs = threads.reduce((max, t) => {
    const value = numberField(t, ['updated_at_ms']);
    return value !== undefined && value > max ? value : max;
  }, Number.NEGATIVE_INFINITY);
  const maxMs = Number.isFinite(maxUpdatedAtMs) ? maxUpdatedAtMs : sinceMs;
  const messages = await ctx.read('communication_messages', query);
  const inbound = new Map();
  for (const message of messages) {
    if (message.direction !== 'inbound') continue;
    const threadKey = firstString(message, ['thread_key']);
    if (threadKey === undefined || isSupportInternalThreadKey(threadKey)) continue;
    const messageKey = firstString(message, ['message_key', 'id']) ?? '';
    const updatedAt = numberField(message, ['updated_at_ms']) ?? 0;
    const entry = inbound.get(threadKey) || { key: '', at: 0, count: 0 };
    entry.count += 1;
    if (updatedAt >= entry.at) {
      entry.key = messageKey;
      entry.at = updatedAt;
    }
    inbound.set(threadKey, entry);
  }
  if (!inbound.size) return run.result({ changed_count: 0, max_updated_at_ms: maxMs });

  return withStore(ctx, run, async (store) => {
    let projected = 0;
    for (const thread of threads) {
      const threadKey = firstString(thread, ['thread_key', 'id']);
      if (threadKey === undefined || isSupportInternalThreadKey(threadKey)) continue;
      const latest = inbound.get(threadKey);
      if (!latest) continue;
      const before = run.projections.length;
      const conversationId = (await linkedConversationForThread(store, threadKey))
        ?? `support_conv_${uuidV5(NAMESPACE_URL, threadKey)}`;
      const lastMessageKey = firstString(thread, ['last_message_key']) ?? latest.key;
      const lastActivityAtMs = numberField(thread, ['updated_at_ms', 'last_message_at_ms']) ?? Math.max(latest.at, run.now);
      const unreadCount = numberField(thread, ['unread_count']) ?? latest.count;
      const searchText = firstString(thread, ['subject']) ?? threadKey;
      const channel = firstString(thread, ['channel']) ?? '';
      const accountKey = firstString(thread, ['account_key']) ?? '';
      const existing = await loadConversation(store, conversationId);
      const conversationCurrent = Boolean(existing
        && existing.primary_thread_key === threadKey
        && existing.last_message_key === lastMessageKey
        && existing.last_activity_at_ms === lastActivityAtMs
        && existing.unread_count === unreadCount
        && existing.search_text === searchText);
      const linkCurrent = await supportThreadLinkCurrent(store, conversationId, threadKey, channel, accountKey, 'primary');
      if (conversationCurrent && linkCurrent) continue;
      await upsertConversationFromPayload(store, run, conversationId, {
        conversation_id: conversationId,
        thread_key: threadKey,
        channel,
        account_key: accountKey,
        last_message_key: lastMessageKey,
        last_activity_at_ms: lastActivityAtMs,
        unread_count: unreadCount,
        search_text: searchText,
      });
      if ((await linkedConversationForThread(store, threadKey)) === null) {
        await insertConversationEvent(store, run, {
          conversation_id: conversationId,
          event_type: 'support.intake.communication_thread',
          actor_id: 'ctox',
          source_command_id: '',
          summary: 'Communication thread imported into Support.',
          payload: { thread_key: threadKey, last_message_key: lastMessageKey },
        });
      }
      await upsertThreadLink(store, run, conversationId, threadKey, channel, accountKey, 'primary');
      projected += run.projections.length - before;
    }
    return run.result({ changed_count: projected, max_updated_at_ms: maxMs });
  });
}
