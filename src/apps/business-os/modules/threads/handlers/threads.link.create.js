// threads.link.create — Port von create_link (threads.rs:2315-2367).
import {
  ensureSourceContextReadPolicy, ensureThreadParticipantOrAdmin, fail, firstStringField, idFromPayloadOrRecord,
  loadRecord, payloadOf, preflight, slugPart, sourceContext, sourceString, upsertRecord,
} from './_lib.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const payload = payloadOf(command);
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  const source = sourceContext(command);
  await ensureSourceContextReadPolicy(ctx, source);
  const module = sourceString(source, 'module') ?? '';
  const recordId = sourceString(source, 'record_id') ?? '';
  // U-10: sourceContext füllt module immer (Rückfall command.module = threads) → Prüfung greift praktisch nie (wie Rust).
  if (module === '' && recordId === '') fail('invalid', 'link source module or record_id is required');
  const linkId = firstStringField(payload, ['link_id', 'id'])
    ?? `link_${slugPart(threadId)}_${slugPart(module)}_${slugPart(recordId)}`;
  const record = {
    id: linkId,
    thread_id: threadId,
    source_module: module,
    source_record_type: sourceString(source, 'record_type') ?? '',
    source_record_id: recordId,
    source_label: sourceString(source, 'label') ?? '',
    link_role: firstStringField(payload, ['link_role', 'link_type']) ?? 'related',
    command_id: firstStringField(payload, ['command_id']) ?? '',
    task_id: firstStringField(payload, ['task_id']) ?? '',
    case_id: firstStringField(payload, ['case_id']) ?? '',
    deep_link: firstStringField(payload, ['deep_link']) ?? sourceString(source, 'deep_link') ?? '',
    context: source,
    created_at_ms: now,
    updated_at_ms: now,
  };
  await upsertRecord(ctx, 'user_thread_links', linkId, now, record);
  return { ok: true, thread_id: threadId, link_id: linkId, projections: [{ collection: 'user_thread_links', record_id: linkId }] };
}
