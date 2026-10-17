// news.issue.failed – Rueckruf on_failed des Ausgabe-Agenten (Kern §16.30 Nr. 3):
// payload {task_id, status:'failed'|'cancelled'|'blocked', error?, status_note?}.
// ref: store_news_commands.rs mark_news_command_failed :716 (Zweig news.issue.generate) und #778 (main e5f057ead,
// store.rs refresh_business_command_queue_task_projection): die Ausgabe wird `failed`, Grund = Notiz/Fehler
// getrimmt, sonst „CTOX hat die Ausgabe nicht abgeschlossen.“ – statt ewig auf „Ausgabe wird erstellt“ zu stehen.
// Idempotent: derselbe Rueckruf (gleiche task_id) ein zweites Mal schreibt nichts.

import { NEWS_ISSUE_GENERATE, callbackOrigin, clipMessage, firstStringField, holdReason, patchRecord, validRecordId } from './_lib/news.js';

export async function markIssueFailed(original, { task_id: taskId, status, reason }, ctx) {
  const issueId = firstStringField(original.payload ?? {}, ['issue_id']);
  const result = { ok: false, task_id: taskId, status, reason };
  if (!issueId || !validRecordId(issueId)) return result;
  const error = clipMessage(reason);
  const issue = await ctx.get('news_issues', issueId);
  if (!issue) return result;
  if (issue.task_id === taskId && (issue.status === 'ready' || (issue.status === 'failed' && issue.error === error))) {
    return { ...result, duplicate: true };
  }
  await patchRecord(ctx, 'news_issues', issueId, ctx.now(), { status: 'failed', error, task_id: taskId });
  return result;
}

export default async function handle(command, ctx) {
  const { taskId, original, status } = await callbackOrigin(command, ctx, NEWS_ISSUE_GENERATE);
  return markIssueFailed(original, { task_id: taskId, status: status || 'failed', reason: holdReason(command.payload) }, ctx);
}
