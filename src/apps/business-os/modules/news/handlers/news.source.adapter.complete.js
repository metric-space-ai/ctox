// news.source.adapter.complete – Rueckruf on_complete des Quellen-Agenten (Kern §16.30 Nr. 3):
// payload {task_id, status:'completed', result}. ref: store_news_commands.rs apply_news_queue_reply :566 und
// parse_source_reply :769 – ctox.news.source.v1 (oder Prosa = ok) auf die Quelle; account_handle ohne Passwort.
// Kommt ein Halte-Status (failed | blocked | cancelled) ueber diesen Weg, gilt #778 wie in .failed.
// Idempotent: payload.adapter_task_id der Quelle haelt die verarbeitete task_id.

import {
  HOLD_STATUSES,
  NEWS_SOURCE_ADAPTER,
  callbackOrigin,
  fail,
  firstStringField,
  holdReason,
  parseSourceReply,
  patchRecord,
  resultText,
  validRecordId,
} from './_lib/news.js';
import { markSourceFailed } from './news.source.adapter.failed.js';

export default async function handle(command, ctx) {
  const { taskId, original, status } = await callbackOrigin(command, ctx, NEWS_SOURCE_ADAPTER);
  if (HOLD_STATUSES.includes(status)) return markSourceFailed(original, { task_id: taskId, status, reason: holdReason(command.payload) }, ctx);
  if (status !== 'completed') fail('invalid', `news.source.adapter.complete: unknown task status ${status}`);
  const sourceId = firstStringField(original.payload ?? {}, ['source_id']) ?? (typeof original.record_id === 'string' ? original.record_id : '');
  if (!validRecordId(sourceId)) fail('invalid', 'news.source.adapter is missing source_id');
  const source = await ctx.get('news_sources', sourceId);
  const report = parseSourceReply(resultText(command.payload.result));
  if (source?.payload?.adapter_task_id === taskId) return { ok: true, source_id: sourceId, status: report.status, duplicate: true };
  const now = ctx.now();
  const patch = { status: report.status, last_error: report.note, last_run_at_ms: now };
  if (report.account) patch.account_handle = report.account;
  if (source) patch.payload = { ...(source.payload ?? {}), adapter_task_id: taskId };
  await patchRecord(ctx, 'news_sources', sourceId, now, patch);
  return { ok: true, source_id: sourceId, status: report.status };
}
