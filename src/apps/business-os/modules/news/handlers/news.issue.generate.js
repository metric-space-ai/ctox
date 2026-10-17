// news.issue.generate – Port aus store_news_commands.rs. Eintritt des Knopfs „Ausgabe jetzt erzeugen“ und des Planers:
//   (a) Planer-Ausloesung `payload.trigger === 'schedule'` = emit_scheduled_issue :318 (Folgebefehl mit fester ID)
//   (b) sonst: prompts/issue.md fuellen und den Agenten mit dem App-Skill news/news-aggregation starten
//       (enqueueIssue, _lib/news.js). Das Ergebnis kommt als news.issue.complete / news.issue.failed zurueck
//       (Kern §16.30 Nr. 3); apply_issue_reply :591 steht jetzt in news.issue.complete.js.
import { NEWS_ISSUE_GENERATE, enqueueIssue, fail, generatePayload, patchRecord, scheduleName, topicThreadKey } from './_lib/news.js';
import { sha256Hex } from './_lib/sha256.js';

// ref: store_news_commands.rs emit_scheduled_issue :318
// Erwartet vom Planer: payload {trigger:'schedule', topic_id, owner_user_id, cadence, deadline_date,
// schedule_task_id, schedule_name, schedule_thread_key, scheduled_for (RFC 3339)}.
async function emitScheduledIssue(command, ctx) {
  const p = command.payload ?? {};
  const route = {
    topic_id: String(p.topic_id ?? ''),
    owner_user_id: String(p.owner_user_id ?? ''),
    cadence: String(p.cadence ?? ''),
    deadline_date: String(p.deadline_date ?? ''),
  };
  const taskId = String(p.schedule_task_id ?? '');
  const scheduledFor = String(p.scheduled_for ?? '');
  if (p.schedule_name !== scheduleName(route.topic_id) || p.schedule_thread_key !== topicThreadKey(route.topic_id)) {
    fail('invalid', 'news schedule does not match its topic binding');
  }
  const skipKey = `news-skip:${taskId}:${scheduledFor}`;
  const issueDate = scheduledFor.length >= 10 ? scheduledFor.slice(0, 10) : '';
  if (route.cadence === 'deadline' && route.deadline_date && issueDate !== route.deadline_date) {
    return { run_key: skipKey, outcome: 'skipped' };
  }
  const topic = await ctx.get('news_topics', route.topic_id);
  if (!topic || topic.enabled === false) return { run_key: skipKey, outcome: 'skipped' };
  // Rust: find_rxdb_collection_records_by_string_field(…, "topic_id", id, 200)
  const sources = (await ctx.read('news_sources', { selector: { topic_id: route.topic_id }, limit: 200 })).filter(
    (s) => s.enabled !== false,
  );
  const digest = sha256Hex(`${taskId}::${scheduledFor}`);
  const issueId = `issue_sched_${digest.slice(0, 24)}`;
  const commandId = `cmd_news_issue_generate_sched_${digest.slice(0, 24)}`;
  const now = ctx.now();
  const title = typeof topic.title === 'string' ? topic.title : '';
  if (!(await ctx.get('news_issues', issueId))) {
    await ctx.put('news_issues', {
      id: issueId,
      topic_id: route.topic_id,
      title,
      issue_date: issueDate,
      status: 'queued',
      command_id: commandId,
      task_id: '',
      post_count: 0,
      generated_at_ms: 0,
      error: '',
      payload: { trigger: 'schedule', scheduled_for: scheduledFor },
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
  // Rust: accept_rxdb_business_command_with_origin(... id = commandId, TrustedLocal) im Namen des Owners.
  // v4: Folgebefehl mit fester ID (Vertragswunsch: ctx.command(type, payload, {id, record_id, actor_id})).
  const accepted = await ctx.command(
    NEWS_ISSUE_GENERATE,
    generatePayload(topic, sources, issueId, issueDate),
    { id: commandId, record_id: route.topic_id, actor_id: route.owner_user_id, sync_collections: ['news_issues', 'news_posts'] },
  );
  const key = accepted?.task_id ?? accepted?.id;
  if (!key) fail('unavailable', 'news issue not admitted to the queue: null');
  await patchRecord(ctx, 'news_issues', issueId, ctx.now(), { task_id: key });
  return { run_key: key, outcome: 'emitted' };
}

export default async function handle(command, ctx) {
  if (command.payload?.trigger === 'schedule') return emitScheduledIssue(command, ctx);
  return enqueueIssue(ctx, command);
}
