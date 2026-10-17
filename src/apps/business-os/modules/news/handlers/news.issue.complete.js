// news.issue.complete – Rueckruf on_complete des Ausgabe-Agenten (Kern §16.30 Nr. 3):
// payload {task_id, status:'completed', result}. Den Ausgangsbefehl news.issue.generate liest der Handler aus der
// eigenen Aufgabe (agent.task.get → payload.command, gesetzt von enqueueIssue).
// ref: store_news_commands.rs apply_issue_reply :591 – Antwort ctox.news.issue.v1 pruefen (parse_issue_reply :844),
// Posts/Ausgabe/Quellen/Thema in EINER Transaktion schreiben (v4-Abweichung 6, V4-HANDLERS.md).
// Kommt ein Halte-Status (failed | blocked | cancelled) ueber diesen Weg, gilt #778 wie in news.issue.failed.
// Idempotent: ist die Ausgabe mit derselben task_id schon `ready`, schreibt ein zweiter Rueckruf nichts.

import {
  HOLD_STATUSES,
  NEWS_ISSUE_GENERATE,
  callbackOrigin,
  enforceWindow,
  fail,
  firstStringField,
  holdReason,
  parseIssueReply,
  patchRecord,
  normalizeStoryUrl,
  resultText,
  topicHistory,
  validRecordId,
  windowFromHistory,
} from './_lib/news.js';
import { markIssueFailed } from './news.issue.failed.js';

// ref: store_news_commands.rs apply_issue_reply :591
export async function applyIssueReply(original, reply, ctx) {
  const payload = original.payload ?? {};
  const commandId = original.id;
  const taskId = reply?.task_id ?? '';
  const topicId = firstStringField(payload, ['topic_id']) ?? (typeof original.record_id === 'string' ? original.record_id : '');
  const issueId = firstStringField(payload, ['issue_id']) ?? '';
  if (!validRecordId(topicId)) fail('invalid', 'news issue command is missing topic_id');
  if (!validRecordId(issueId)) fail('invalid', 'news issue command is missing issue_id');
  const parsed = parseIssueReply(reply?.text ?? '');
  const now = ctx.now();
  // #799 (ref: apply_issue_reply → enforce_window): gilt fuer Befehle mit Zeitfenster (window_start_ms), d. h. alle,
  // die UI und Planer seit dem Nachport senden; aeltere Befehle ohne Fenster laufen wie bisher durch.
  let rejected = null;
  const windowStart = Number.isInteger(payload.window_start_ms) && payload.window_start_ms > 0 ? payload.window_start_ms : 0;
  if (windowStart) {
    const cadence = typeof payload.record_snapshot?.topic?.cadence === 'string' ? payload.record_snapshot.topic.cadence : 'daily';
    const history = await topicHistory(ctx, topicId, issueId);
    const window = windowFromHistory(history.issues, history.posts, cadence, now);
    window.startMs = windowStart;
    for (const url of Array.isArray(payload.covered_urls) ? payload.covered_urls : []) {
      if (typeof url !== 'string') continue;
      const key = normalizeStoryUrl(url);
      if (!window.coveredUrls.includes(key)) window.coveredUrls.push(key);
    }
    rejected = enforceWindow(parsed, window, now);
  }
  const postId = (index) => `${issueId}_p${String(index + 1).padStart(2, '0')}`;

  return ctx.transaction(async (tx) => {
    const existing = await tx.get('news_issues', issueId);
    if (existing && existing.status === 'ready' && taskId && existing.task_id === taskId) {
      return { ok: true, issue_id: issueId, topic_id: topicId, post_count: existing.post_count ?? 0, dropped_posts: 0, duplicate: true };
    }
    for (const [index, post] of parsed.posts.entries()) {
      await tx.put('news_posts', {
        ...post,
        id: postId(index),
        issue_id: issueId,
        topic_id: topicId,
        rank: index + 1,
        bookmarked: false,
        payload: { command_id: commandId },
        created_at_ms: now,
        updated_at_ms: now,
      });
    }
    const issue = existing ?? {
      id: issueId,
      topic_id: topicId,
      title: '',
      issue_date: firstStringField(payload, ['issue_date']) ?? '',
      command_id: commandId,
      created_at_ms: now,
    };
    const issuePayload = { ...(issue.payload && typeof issue.payload === 'object' && !Array.isArray(issue.payload) ? issue.payload : {}) };
    issuePayload.trends = parsed.trends.map((trend) => {
      const { post_index: index, ...rest } = trend;
      return { ...rest, post_id: index !== null && index < parsed.posts.length ? postId(index) : '' };
    });
    if (parsed.summary) issuePayload.summary = parsed.summary;
    if (rejected) issuePayload.rejected_posts = rejected;
    Object.assign(issue, {
      status: 'ready',
      task_id: taskId,
      command_id: commandId,
      post_count: parsed.posts.length,
      generated_at_ms: now,
      error: '',
      payload: issuePayload,
      updated_at_ms: now,
    });
    await tx.put('news_issues', issue);
    // Nur Quellen dieses Themas duerfen vom Ausgabelauf beruehrt werden.
    for (const [sourceId, status, note] of parsed.sourceReport) {
      const source = await tx.get('news_sources', sourceId);
      if (source && source.topic_id === topicId) {
        await patchRecord(tx, 'news_sources', sourceId, now, { status, last_error: note, last_run_at_ms: now });
      }
    }
    await patchRecord(tx, 'news_topics', topicId, now, { last_issue_at_ms: now });
    return {
      ok: true,
      issue_id: issueId,
      topic_id: topicId,
      post_count: parsed.posts.length,
      dropped_posts: parsed.dropped,
      ...(rejected ? { rejected_posts: rejected.length } : {}),
    };
  });
}

export default async function handle(command, ctx) {
  const { taskId, original, status } = await callbackOrigin(command, ctx, NEWS_ISSUE_GENERATE);
  if (HOLD_STATUSES.includes(status)) return markIssueFailed(original, { task_id: taskId, status, reason: holdReason(command.payload) }, ctx);
  if (status !== 'completed') fail('invalid', `news.issue.complete: unknown task status ${status}`);
  try {
    return await applyIssueReply(original, { task_id: taskId, text: resultText(command.payload.result) }, ctx);
  } catch (error) {
    // Unbrauchbare Antwort (z. B. kein Post mit Quelle): Ausgabe zeigt den Fehler statt ewig zu laufen
    // (Rust: process_news_queue_command → mark_news_command_failed bei Rueckschreibfehler).
    if (error?.code === 'invalid') await markIssueFailed(original, { task_id: taskId, status: 'failed', reason: error.message }, ctx);
    throw error;
  }
}
