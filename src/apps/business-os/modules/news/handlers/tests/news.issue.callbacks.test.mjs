// news.issue.complete / news.issue.failed: Rueckruf des Kerns (§16.30 Nr. 3) ueber die Agenten-Simulation.
import assert from 'node:assert/strict';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, source, topic } from './_fixtures.mjs';
import { world } from './_sim.mjs';

const { default: generate } = await import('../news.issue.generate.js');
const { default: complete } = await import('../news.issue.complete.js');
const { default: failed } = await import('../news.issue.failed.js');
const { HOLD_DEFAULT_REASON } = await import('../_lib/news.js');

const issue = (o = {}) => ({ id: 'issue_1', topic_id: 'topic_1', issue_date: '2026-10-14', status: 'queued', payload: { keep: true }, created_at_ms: NOW - 5, updated_at_ms: NOW - 5, ...o });
const snap = { topic: { id: 'topic_1', title: 'KI', prompt: 'Modelle', language: 'de', categories: ['model'], max_posts: 5 }, sources: [] };
const replyText = JSON.stringify({
  schema: 'ctox.news.issue.v1',
  summary: 'Kurz',
  posts: [
    { intro: 'A', category: 'tool', sources: [{ label: 'L', url: 'https://a.example' }] },
    { intro: 'B', sources: [{ url: 'http://b.example' }] },
  ],
  trends: [{ title: 'T1', post_index: 1 }, { title: 'T2', post_index: 7 }],
  source_report: [
    { source_id: 'source_1', status: 'needs_auth', note: 'Login' },
    { source_id: 'source_other', status: 'failed', note: 'fremd' },
  ],
});

async function started({ seed, payload = {} } = {}) {
  const w = await world({
    seed: seed ?? { news_topics: [topic()], news_issues: [issue()], news_sources: [source(), source({ id: 'source_other', topic_id: 'topic_2' })] },
  });
  const c = cmd('news.issue.generate', { topic_id: 'topic_1', issue_id: 'issue_1', issue_date: '2026-10-14', record_snapshot: snap, ...payload }, { id: 'cmd_gen' });
  await generate(c, w.attach(c));
  return { ...w, taskId: w.sim.enqueued[0].task_id };
}

test('complete → Posts, Ausgabe, eigene Quellen und Thema in den Collections', async () => {
  const w = await started();
  const d = await w.sim.complete(w.taskId, { text: replyText });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.command.type, 'news.issue.complete');
  assert.deepEqual(d.result, { ok: true, issue_id: 'issue_1', topic_id: 'topic_1', post_count: 2, dropped_posts: 0 });
  const p1 = w.t.doc('news_posts', 'issue_1_p01');
  assert.equal(p1.rank, 1);
  assert.equal(p1.category, 'tool');
  assert.deepEqual(p1.payload, { command_id: 'cmd_gen' });
  assert.equal(w.t.doc('news_posts', 'issue_1_p02').category, 'other');
  const i = w.t.doc('news_issues', 'issue_1');
  assert.equal(i.status, 'ready');
  assert.equal(i.task_id, w.taskId);
  assert.equal(i.post_count, 2);
  assert.deepEqual(i.payload, { keep: true, trends: [{ title: 'T1', category: '', discussion_count: 0, post_id: 'issue_1_p02' }, { title: 'T2', category: '', discussion_count: 0, post_id: '' }], summary: 'Kurz' });
  assert.equal(w.t.doc('news_sources', 'source_1').status, 'needs_auth');
  assert.equal(w.t.doc('news_sources', 'source_other').status, undefined);
  assert.equal(w.t.doc('news_topics', 'topic_1').last_issue_at_ms, NOW);
});

test('Doppelzustellung: Simulation verwirft, Handler schreibt beim zweiten Aufruf nichts', async () => {
  const w = await started();
  const d = await w.sim.complete(w.taskId, { text: replyText });
  assert.deepEqual(await w.sim.complete(w.taskId, { text: replyText }), { delivered: false, reason: 'duplicate' });
  const writes = w.t.writeCount();
  w.t.setNow(NOW + 5000);
  const again = await complete(d.command, d.ctx);
  assert.equal(again.duplicate, true);
  assert.equal(w.t.writeCount(), writes);
  assert.equal(w.t.doc('news_issues', 'issue_1').generated_at_ms, NOW);
});

test('complete ohne vorhandene Ausgabe legt sie an; unbrauchbare Antwort markiert die Ausgabe failed, ohne Posts', async () => {
  const w = await started({ seed: { news_topics: [topic()] }, payload: { issue_id: 'issue_new', issue_date: '2026-10-15' } });
  await w.sim.complete(w.taskId, { text: replyText });
  assert.equal(w.t.doc('news_issues', 'issue_new').issue_date, '2026-10-15');
  const w2 = await started();
  const d = await w2.sim.complete(w2.taskId, { text: '{"posts":[]}' });
  assert.equal(d.error?.code, 'invalid');
  assert.match(d.error.message, /no post/);
  assert.equal(w2.t.all('news_posts').length, 0);
  assert.deepEqual([w2.t.doc('news_issues', 'issue_1').status, w2.t.doc('news_issues', 'issue_1').error], ['failed', 'news issue reply has no post with intro and a linked source']);
});

test('failed → Ausgabe failed mit Fehlertext (max. 500 Zeichen), zweiter Rueckruf ohne Wirkung', async () => {
  const w = await started();
  const d = await w.sim.fail(w.taskId, { code: 'failed', message: 'x'.repeat(600) });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.command.type, 'news.issue.failed');
  const i = w.t.doc('news_issues', 'issue_1');
  assert.equal(i.status, 'failed');
  assert.equal(i.error.length, 500);
  const writes = w.t.writeCount();
  assert.equal((await failed(d.command, d.ctx)).duplicate, true);
  assert.equal(w.t.writeCount(), writes);
});

test('#778 Halte-Projektion: blockierter Lauf ohne Notiz markiert die Ausgabe als fehlgeschlagen (Standardgrund)', async () => {
  for (const type of ['news.issue.failed', 'news.issue.complete']) {
    const w = await started();
    const cb = cmd(type, { task_id: w.taskId, status: 'blocked', result: null }, { id: `cb_${type}` });
    const r = await (type === 'news.issue.failed' ? failed : complete)(cb, w.attach(cb));
    assert.deepEqual(r, { ok: false, task_id: w.taskId, status: 'blocked', reason: HOLD_DEFAULT_REASON });
    const doc = w.t.doc('news_issues', 'issue_1');
    assert.equal(doc.status, 'failed');
    assert.equal(doc.error, 'CTOX hat die Ausgabe nicht abgeschlossen.');
  }
});

test('#778 Halte-Projektion: cancelled/failed übernehmen die getrimmte status_note als Grund', async () => {
  for (const status of ['cancelled', 'failed']) {
    const w = await started();
    const cb = cmd('news.issue.failed', { task_id: w.taskId, status, status_note: '  technischer Halt nach direct session timeout  ' });
    await failed(cb, w.attach(cb));
    assert.equal(w.t.doc('news_issues', 'issue_1').status, 'failed');
    assert.equal(w.t.doc('news_issues', 'issue_1').error, 'technischer Halt nach direct session timeout');
  }
});

test('Rueckruf ohne task_id bzw. zu fremder Aufgabe wird abgelehnt', async () => {
  const w = await started();
  const c1 = cmd('news.issue.complete', { status: 'completed' });
  await rejects(complete(c1, w.attach(c1)), 'invalid', /task_id/);
  const adapter = cmd('news.source.adapter', { source_id: 'source_1', record_snapshot: { source: { id: 'source_1', kind: 'lab', label: 'L' } } }, { id: 'cmd_other' });
  const { default: adapterHandle } = await import('../news.source.adapter.js');
  await adapterHandle(adapter, w.attach(adapter));
  const foreignTask = w.sim.enqueued[1].task_id;
  const c2 = cmd('news.issue.complete', { task_id: foreignTask, status: 'completed', result: { text: replyText } });
  await rejects(complete(c2, w.attach(c2)), 'not_found', /news.issue.generate/);
  const c3 = cmd('news.issue.complete', { task_id: w.taskId, status: 'weird' });
  await rejects(complete(c3, w.attach(c3)), 'invalid', /unknown task status/);
});
