// news.source.adapter.failed – Rueckruf on_failed des Quellen-Agenten (Kern §16.30 Nr. 3).
// ref: store_news_commands.rs mark_news_command_failed :742 – die Quelle zeigt den Fehler; Halte-Status
// failed | blocked | cancelled wie #778 (Grund = Notiz/Fehler getrimmt, sonst Standardgrund).
// Idempotent: payload.adapter_task_id der Quelle haelt die verarbeitete task_id.

import { NEWS_SOURCE_ADAPTER, callbackOrigin, clipMessage, firstStringField, holdReason, patchRecord, validRecordId } from './_lib/news.js';

export async function markSourceFailed(original, { task_id: taskId, status, reason }, ctx) {
  const sourceId = firstStringField(original.payload ?? {}, ['source_id']) ?? (typeof original.record_id === 'string' ? original.record_id : undefined);
  const result = { ok: false, task_id: taskId, status, reason };
  if (!sourceId || !validRecordId(sourceId)) return result;
  const source = await ctx.get('news_sources', sourceId);
  if (!source) return result;
  if (source.payload?.adapter_task_id === taskId) return { ...result, duplicate: true };
  await patchRecord(ctx, 'news_sources', sourceId, ctx.now(), {
    status: 'failed',
    last_error: clipMessage(reason),
    payload: { ...(source.payload ?? {}), adapter_task_id: taskId },
  });
  return result;
}

export default async function handle(command, ctx) {
  const { taskId, original, status } = await callbackOrigin(command, ctx, NEWS_SOURCE_ADAPTER);
  return markSourceFailed(original, { task_id: taskId, status: status || 'failed', reason: holdReason(command.payload) }, ctx);
}
