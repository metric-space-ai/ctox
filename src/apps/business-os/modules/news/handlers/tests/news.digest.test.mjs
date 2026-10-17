// #800 Nachport (main 8b43f9e8d): Wochenzusammenfassung – eigener Termin, Ausloesung aus den Tagesausgaben,
// Anweisung ohne Quellen, Uebernahme nur mit Links der Woche.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd } from './_ctx.mjs';
import { NOW, setup, topic } from './_fixtures.mjs';
import { world } from './_sim.mjs';

const { default: schedule } = await import('../news.topic.schedule.js');
const { default: generate } = await import('../news.issue.generate.js');
const lib = await import('../_lib/news.js');
const H = 3_600_000;
const D = 24 * H;

const digestTopic = (o = {}) => topic({ payload: { digest: { enabled: true, weekday: 5, run_time: '17:00' } }, ...o });
const daily = (id, ageMs, o = {}) => ({ id, topic_id: 'topic_1', issue_date: '2026-10-1x', status: 'ready', generated_at_ms: NOW - ageMs, payload: {}, created_at_ms: NOW - ageMs, updated_at_ms: NOW - ageMs, ...o });
const post = (id, issueId, url, o = {}) => ({
  id, issue_id: issueId, topic_id: 'topic_1', intro: id, body: 'x'.repeat(2_000), rank: 1, category: 'model', hype_score: 1, discussion_count: 0,
  link_card: { url }, sources: [{ url, kind: 'primary' }], created_at_ms: NOW, updated_at_ms: NOW, ...o,
});
const trigger = { trigger: 'schedule', topic_id: 'topic_1', owner_user_id: 'u', cadence: 'daily', deadline_date: '', mode: 'digest', schedule_task_id: 'dg1', schedule_name: 'news-digest:topic_1', schedule_thread_key: 'business-os/news/topic_1', scheduled_for: '2026-10-16T17:00:00+02:00' };

test('news.topic.schedule mode digest: eigener Termin news-digest:<topic>, Route mit mode, kein schedule_task_id am Thema', async () => {
  const tasks = [{ task_id: 'regular', name: 'news:topic_1', thread_key: 'business-os/news/topic_1', enabled: true }];
  const caps = {
    'schedule.list': async () => tasks,
    'schedule.upsert': async (req) => { const t = { task_id: 'digest_1', enabled: true, ...req }; tasks.push(t); return t; },
    'schedule.set_enabled': async ({ task_id, enabled }) => { const t = tasks.find((x) => x.task_id === task_id); t.enabled = enabled; return { ...t }; },
    'schedule.remove': async ({ task_id }) => tasks.splice(tasks.findIndex((x) => x.task_id === task_id), 1),
  };
  const t = await setup({ seed: { news_topics: [digestTopic({ schedule_task_id: 'regular' })] }, caps });
  const r = await schedule(cmd('news.topic.schedule', { topic_id: 'topic_1', cadence: 'daily', mode: 'digest', cron_expr: '0 17 * * 5', enabled: true }), t.ctx);
  assert.equal(r.schedule_task_id, 'digest_1');
  const task = tasks.find((x) => x.task_id === 'digest_1');
  assert.equal(task.name, 'news-digest:topic_1');
  assert.equal(task.command.payload.mode, 'digest');
  assert.ok(task.prompt.includes('"mode":"digest"'));
  assert.equal(t.doc('news_topics', 'topic_1').schedule_task_id, 'regular', 'regulaerer Termin bleibt am Thema');
  // ausschalten pausiert nur den Wochen-Termin; loeschen entfernt ihn
  await schedule(cmd('news.topic.schedule', { topic_id: 'topic_1', mode: 'digest', cron_expr: '0 17 * * 5', enabled: false }), t.ctx);
  assert.deepEqual(tasks.map((x) => [x.task_id, x.enabled]), [['regular', true], ['digest_1', false]]);
  assert.equal(t.doc('news_topics', 'topic_1').schedule_task_id, 'regular');
  await schedule(cmd('news.topic.schedule', { topic_id: 'topic_1', mode: 'digest', deleted: true }), t.ctx);
  assert.deepEqual(tasks.map((x) => x.task_id), ['regular']);
});

test('Planer-Ausloesung mode digest: Ausgabe issue_digest_*, Folgebefehl mit Beitraegen und Links der Woche, ohne Quellen', async () => {
  const sent = [];
  const t = await setup({
    seed: {
      news_topics: [digestTopic({ max_posts: 20 })],
      news_issues: [daily('d1', 2 * D), daily('d2', 10 * D), daily('dg', D, { payload: { kind: 'weekly_digest' } }), daily('df', D, { status: 'failed' })],
      news_posts: [
        post('p_low', 'd1', 'https://a.example/1', { hype_score: 1 }),
        post('p_high', 'd1', 'https://www.b.example/2/', { hype_score: 9 }),
        post('p_old', 'd2', 'https://old.example'),
        post('p_dg', 'dg', 'https://digest.example'),
      ],
    },
    commands: { 'news.issue.generate': async (c) => (sent.push(c), { id: c.id, task_id: 'q_dg' }) },
  });
  const r = await generate(cmd('news.issue.generate', trigger), t.ctx);
  assert.deepEqual(r, { run_key: 'q_dg', outcome: 'emitted' });
  const hash = createHash('sha256').update(`dg1::${trigger.scheduled_for}`).digest('hex').slice(0, 24);
  const issue = t.doc('news_issues', `issue_digest_${hash}`);
  assert.equal(issue.payload.kind, 'weekly_digest');
  assert.equal(issue.task_id, 'q_dg');
  assert.equal(issue.command_id, `cmd_news_digest_sched_${hash}`);
  const p = sent[0].payload;
  assert.equal(p.mode, 'weekly_digest');
  assert.deepEqual(p.digest_posts.map((x) => x.post_id), ['p_high', 'p_low']);
  assert.equal(p.digest_posts[0].body.length, 1_200);
  assert.deepEqual(p.digest_urls, ['b.example/2', 'a.example/1']);
  assert.deepEqual(p.record_snapshot.sources, []);
  assert.equal(p.record_snapshot.topic.max_posts, 10);
  assert.equal(p.window_start_ms, NOW - 7 * D);
});

test('Planer-Ausloesung mode digest: ausgeschaltet oder leere Woche → skipped; falsche Bindung abgelehnt', async () => {
  const off = await setup({ seed: { news_topics: [topic()] } });
  assert.equal((await generate(cmd('news.issue.generate', trigger), off.ctx)).outcome, 'skipped');
  const empty = await setup({ seed: { news_topics: [digestTopic()] } });
  assert.equal((await generate(cmd('news.issue.generate', trigger), empty.ctx)).outcome, 'skipped');
  await assert.rejects(generate(cmd('news.issue.generate', { ...trigger, schedule_name: 'news:topic_1' }), empty.ctx), (e) => e.code === 'invalid');
});

test('Digest-Lauf: Anweisung aus prompts/digest.md ohne Laufvertrag; Uebernahme behaelt nur Links der Woche', async () => {
  const issue = { id: 'issue_dg', topic_id: 'topic_1', issue_date: '2026-10-16', status: 'queued', payload: { kind: 'weekly_digest' }, created_at_ms: NOW, updated_at_ms: NOW };
  const w = await world({ seed: { news_topics: [digestTopic()], news_issues: [issue] } });
  const weekPosts = [post('p1', 'd1', 'https://week.example/a')];
  const c = cmd('news.issue.generate', lib.digestPayload(digestTopic(), weekPosts, 'issue_dg', '2026-10-16', NOW), { id: 'cmd_dg' });
  await generate(c, w.attach(c));
  const text = w.sim.enqueued[0].args.instruction;
  const lines = text.split('\n');
  assert.equal(lines[0], 'Erstelle die Wochenzusammenfassung für das Thema "KI" mit dem Skill news-aggregation im Modus weekly_digest.');
  assert.ok(lines.includes('Grundlage sind ausschließlich die 1 Beiträge der Tagesausgaben dieser Woche (digest_posts); keine neue Recherche, keine neuen Quellen.'));
  assert.ok(!text.includes('Laufvertrag'));
  assert.ok(!text.includes('Zeitfenster'));
  assert.deepEqual(JSON.parse(text.slice(text.indexOf('{'))).digest_urls, ['week.example/a']);
  const reply = JSON.stringify({
    schema: 'ctox.news.issue.v1',
    posts: [
      { intro: 'aus der Woche', link_card: { url: 'https://www.week.example/a/' }, sources: [{ url: 'https://week.example/a' }] },
      { intro: 'neu recherchiert', sources: [{ url: 'https://fresh.example' }] },
    ],
    trends: [{ title: 'neu', post_index: 1 }, { title: 'Woche', post_index: 0 }],
  });
  const d = await w.sim.complete(w.sim.enqueued[0].task_id, { text: reply });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.result.post_count, 1);
  const done = w.t.doc('news_issues', 'issue_dg');
  assert.equal(done.payload.kind, 'weekly_digest');
  assert.deepEqual(done.payload.rejected_posts.map((x) => x.reason), ['not_in_week']);
  assert.deepEqual(done.payload.trends.map((x) => [x.title, x.post_id]), [['Woche', 'issue_dg_p01']]);
});
