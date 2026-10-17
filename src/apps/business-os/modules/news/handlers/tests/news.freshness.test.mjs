// #799 Nachport (main d4de046cb): Zeitfenster + Dubletten ueber Ausgaben hinweg – Anweisung, Planer, Uebernahme.
import assert from 'node:assert/strict';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd } from './_ctx.mjs';
import { NOW, setup, topic } from './_fixtures.mjs';
import { world } from './_sim.mjs';

const { default: generate } = await import('../news.issue.generate.js');
const lib = await import('../_lib/news.js');
const H = 3_600_000;

const snap = { topic: { id: 'topic_1', title: 'KI', prompt: '', cadence: 'daily', language: 'de', categories: ['model'], max_posts: 5 }, sources: [] };
const queued = { id: 'issue_new', topic_id: 'topic_1', issue_date: '2026-10-14', status: 'queued', payload: {}, created_at_ms: NOW, updated_at_ms: NOW };
const earlier = { id: 'issue_old', topic_id: 'topic_1', issue_date: '2026-10-13', status: 'ready', generated_at_ms: NOW - 9 * H, payload: {}, created_at_ms: NOW - 9 * H, updated_at_ms: NOW - 9 * H };
const oldPost = {
  id: 'issue_old_p01', issue_id: 'issue_old', topic_id: 'topic_1', intro: 'alt', rank: 1, category: 'model',
  link_card: { url: 'https://www.Cohere.com/blog/new/' }, sources: [{ url: 'https://news.ycombinator.com/item?id=1', kind: 'discussion' }],
  created_at_ms: NOW - 9 * H, updated_at_ms: NOW - 9 * H,
};

test('normalizeStoryUrl und windowFromHistory wie Rust (Fenster seit letzter Ausgabe, Diskussionslinks zaehlen nicht)', () => {
  assert.equal(lib.normalizeStoryUrl('https://x.ai/news#a'), 'x.ai/news');
  assert.equal(lib.normalizeStoryUrl('HTTPS://WWW.Example.com/a/b/'), 'example.com/a/b');
  const w = lib.windowFromHistory([earlier], [oldPost], 'daily', NOW);
  assert.equal(w.startMs, NOW - 9 * H);
  assert.equal(w.previousIssueId, 'issue_old');
  assert.deepEqual(w.coveredUrls, ['cohere.com/blog/new']);
  assert.equal(lib.windowFromHistory([], [], 'daily', NOW).startMs, NOW - 24 * H);
  assert.equal(lib.windowFromHistory([], [], 'weekly', NOW).startMs, NOW - 7 * 24 * H);
  // lange Pause: nie weiter als eine Woche zurueck
  assert.equal(lib.windowFromHistory([{ ...earlier, generated_at_ms: NOW - 20 * 24 * H }], [], 'daily', NOW).startMs, NOW - 24 * H);
});

test('Anweisung nennt das Zeitfenster, Auftragsdaten tragen window_start_ms und covered_urls', async () => {
  const w = await world({ seed: { news_issues: [queued, earlier] } });
  const c = cmd('news.issue.generate', {
    topic_id: 'topic_1', issue_id: 'issue_new', issue_date: '2026-10-14', record_snapshot: snap,
    previous_issue_id: 'issue_old', window_start: new Date(NOW - 9 * H).toISOString(), window_start_ms: NOW - 9 * H, covered_urls: ['cohere.com/blog/new'],
  }, { id: 'cmd_win' });
  await generate(c, w.attach(c));
  const text = w.sim.enqueued[0].args.instruction;
  assert.ok(text.split('\n').includes(`Zeitfenster: nur Meldungen, deren Primärquelle nach ${lib.rfc3339(NOW - 9 * H)} veröffentlicht wurde; ältere Beiträge verwirft die Übernahme.`));
  const context = JSON.parse(text.slice(text.indexOf('{')));
  assert.equal(context.window_start_ms, NOW - 9 * H);
  assert.deepEqual(context.covered_urls, ['cohere.com/blog/new']);
});

test('Befehl ohne Fenster: keine Zeitfenster-Zeile (Altbefehle)', async () => {
  const w = await world({ seed: { news_issues: [queued] } });
  const c = cmd('news.issue.generate', { topic_id: 'topic_1', issue_id: 'issue_new', issue_date: '2026-10-14', record_snapshot: snap }, { id: 'cmd_plain' });
  await generate(c, w.attach(c));
  assert.ok(!w.sim.enqueued[0].args.instruction.includes('Zeitfenster'));
});

test('Planer-Folgebefehl rechnet das Fenster aus der Historie des Themas', async () => {
  const sent = [];
  const t = await setup({
    seed: { news_topics: [topic()], news_issues: [earlier], news_posts: [oldPost] },
    commands: { 'news.issue.generate': async (c) => (sent.push(c), { id: c.id, task_id: 'q1' }) },
  });
  const p = { trigger: 'schedule', topic_id: 'topic_1', owner_user_id: 'u', cadence: 'daily', deadline_date: '', schedule_task_id: 's1', schedule_name: 'news:topic_1', schedule_thread_key: 'business-os/news/topic_1', scheduled_for: '2026-10-16T07:00:00Z' };
  await generate(cmd('news.issue.generate', p), t.ctx);
  assert.equal(sent[0].payload.window_start_ms, NOW - 9 * H);
  assert.equal(sent[0].payload.previous_issue_id, 'issue_old');
  assert.deepEqual(sent[0].payload.covered_urls, ['cohere.com/blog/new']);
  assert.equal(sent[0].payload.window_start, lib.rfc3339(NOW - 9 * H));
});

test('Uebernahme verwirft alte, schon gebrachte, undatierte und doppelte Beitraege; Trends gehen mit', async () => {
  const w = await world({ seed: { news_topics: [topic()], news_issues: [queued, earlier], news_posts: [oldPost] } });
  const c = cmd('news.issue.generate', {
    topic_id: 'topic_1', issue_id: 'issue_new', issue_date: '2026-10-14', record_snapshot: snap, window_start_ms: NOW - 9 * H, covered_urls: [],
  }, { id: 'cmd_enf' });
  await generate(c, w.attach(c));
  const at = (ms) => new Date(ms).toISOString();
  const reply = JSON.stringify({
    schema: 'ctox.news.issue.v1',
    posts: [
      { intro: 'frisch', published_at: at(NOW - H), sources: [{ url: 'https://fresh.example/a', kind: 'primary' }] },
      { intro: 'zu alt', published_at: at(NOW - 20 * H), sources: [{ url: 'https://old.example/b' }] },
      { intro: 'schon gebracht', published_at: at(NOW - H), link_card: { url: 'https://cohere.com/blog/new' }, sources: [{ url: 'https://cohere.com/blog/new' }] },
      { intro: 'ohne Datum', sources: [{ url: 'https://nodate.example' }] },
      { intro: 'doppelt', published_at: at(NOW - 2 * H), sources: [{ url: 'https://www.fresh.example/a/' }] },
      { intro: 'im Spielraum', published_at: at(NOW - 13 * H), sources: [{ url: 'https://grace.example' }] },
    ],
    trends: [{ title: 'T-alt', post_index: 1 }, { title: 'T-gnade', post_index: 5 }, { title: 'T-frei', post_index: null }],
  });
  const d = await w.sim.complete(w.sim.enqueued[0].task_id, { text: reply });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.result.post_count, 2);
  assert.equal(d.result.rejected_posts, 4);
  assert.equal(w.t.doc('news_posts', 'issue_new_p01').intro, 'frisch');
  assert.equal(w.t.doc('news_posts', 'issue_new_p02').intro, 'im Spielraum');
  const i = w.t.doc('news_issues', 'issue_new');
  assert.deepEqual(i.payload.rejected_posts.map((r) => r.reason), ['published_before_window', 'already_covered', 'missing_published_at', 'duplicate_in_issue']);
  assert.deepEqual(i.payload.trends.map((tr) => [tr.title, tr.post_id]), [['T-gnade', 'issue_new_p02'], ['T-frei', '']]);
});
