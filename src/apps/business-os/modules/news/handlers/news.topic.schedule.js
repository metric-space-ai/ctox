// news.topic.schedule – Port aus store_news_commands.rs handle_topic_schedule :142 und
// store_schedule_task_id :235. GEMISCHT.
// Fachlogik hier: topic_id-Pruefung, Aktiv-Entscheidung (deleted / enabled / cron leer),
// Zeitzone mit Rueckfall Europe/Berlin, Route (topic, owner, cadence, deadline_date),
// Rueckschrieb schedule_task_id ans Thema. Faehigkeit: der Kern-Scheduler -> ctx.cap.schedule.{list,upsert,remove,set_enabled} (§16.30).

import {
  NEWS_ISSUE_GENERATE,
  digestScheduleName,
  NEWS_MODULE,
  ROUTE_MARKER,
  fail,
  firstStringField,
  patchRecord,
  recordIdOf,
  scheduleName,
  sessionUserId,
  topicThreadKey,
  validRecordId,
} from './_lib/news.js';

// chrono_tz::Tz-Parse-Ersatz: IANA-Zone, die Intl kennt.
function validTimezone(tz) {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: tz });
    return true;
  } catch {
    return false;
  }
}

async function storeScheduleTaskId(ctx, topicId, taskId) {
  await patchRecord(ctx, 'news_topics', topicId, ctx.now(), { schedule_task_id: taskId ?? '' });
}

export default async function handle(command, ctx) {
  if (command.app_id !== NEWS_MODULE) fail('forbidden', 'news control commands must originate from the news module');
  const payload = command.payload ?? {};
  const topicId = firstStringField(payload, ['topic_id']) ?? recordIdOf(command) ?? '';
  if (!validRecordId(topicId)) fail('invalid', 'news.topic.schedule requires a valid topic_id');
  const deleted = payload.deleted === true;
  // #800: die Wochenzusammenfassung hat ihren eigenen Termin news-digest:<topic>; kein Rueckschrieb ans Thema.
  const digest = firstStringField(payload, ['mode']) === 'digest';
  const cronExpr = firstStringField(payload, ['cron_expr']) ?? '';
  const enabled = !deleted && payload.enabled !== false && cronExpr.trim() !== '';
  const name = digest ? digestScheduleName(topicId) : scheduleName(topicId);
  const threadKey = topicThreadKey(topicId);
  const tasks = await ctx.cap.schedule.list({ name, thread_key: threadKey });
  const existing = (Array.isArray(tasks) ? tasks : []).find((t) => t?.name === name && t?.thread_key === threadKey);

  if (!enabled) {
    if (existing) {
      if (deleted) await ctx.cap.schedule.remove({ task_id: existing.task_id });
      else if (existing.enabled) await ctx.cap.schedule.set_enabled({ task_id: existing.task_id, enabled: false });
    }
    if (!deleted && !digest) await storeScheduleTaskId(ctx, topicId, existing?.task_id);
    return {
      ok: true,
      topic_id: topicId,
      enabled: false,
      removed: deleted,
      schedule_task_id: existing?.task_id ?? '',
    };
  }

  const tzCandidate = firstStringField(payload, ['timezone']);
  const timezone = tzCandidate && validTimezone(tzCandidate) ? tzCandidate : 'Europe/Berlin';
  const route = {
    topic_id: topicId,
    owner_user_id: sessionUserId(command),
    cadence: firstStringField(payload, ['cadence']) ?? '',
    deadline_date:
      firstStringField(payload, ['deadline_date']) ??
      (typeof payload.record_snapshot?.topic?.deadline_date === 'string' ? payload.record_snapshot.topic.deadline_date : '') ??
      '',
    ...(digest ? { mode: 'digest' } : {}),
  };
  // §16.30: schedule.upsert (flach), legt an oder aktualisiert nach name + thread_key.
  let task = await ctx.cap.schedule.upsert({
    name,
    cron_expr: cronExpr.trim(),
    // Altweg: der Rust-Emitter liest die Route aus dem Prompt (ROUTE_MARKER + JSON).
    prompt: `${ROUTE_MARKER}${JSON.stringify(route)}`,
    thread_key: threadKey,
    skill: null,
    calendar: { timezone, lead_minutes: 0 },
    // v4-Weg: der Scheduler feuert bei Faelligkeit diesen Befehl im Namen des Owners.
    command: { type: NEWS_ISSUE_GENERATE, payload: { trigger: 'schedule', ...route }, actor_id: route.owner_user_id },
  });
  if (!task?.enabled) task = await ctx.cap.schedule.set_enabled({ task_id: task?.task_id, enabled: true });
  if (!digest) await storeScheduleTaskId(ctx, topicId, task?.task_id);
  return {
    ok: true,
    topic_id: topicId,
    enabled: true,
    schedule_task_id: task?.task_id,
    next_run_at: task?.next_run_at ?? null,
  };
}
