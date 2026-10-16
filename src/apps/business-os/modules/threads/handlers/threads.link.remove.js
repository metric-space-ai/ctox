// threads.link.remove — Port von remove_link (threads.rs:2369-2393). Soft-Delete (is_deleted).
import {
  clone, ensureThreadParticipantOrAdmin, fail, idFromPayloadOrRecord, loadRecord, preflight, upsertRecord, valueString,
} from './_lib.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const linkId = idFromPayloadOrRecord(command, ['link_id', 'id'], 'link_id is required');
  const link = await loadRecord(ctx, 'user_thread_links', linkId);
  if (!link) fail('not_found', `thread link ${linkId} not found`);
  const threadId = valueString(link, 'thread_id');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  const next = clone(link);
  next.is_deleted = true;
  next.deleted_at_ms = now;
  next.updated_at_ms = now;
  await upsertRecord(ctx, 'user_thread_links', linkId, now, next);
  return { ok: true, thread_id: threadId, link_id: linkId, projections: [{ collection: 'user_thread_links', record_id: linkId }] };
}
