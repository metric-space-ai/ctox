import assert from 'node:assert/strict';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, source } from './_fixtures.mjs';
import { world } from './_sim.mjs';

const { default: handle } = await import('../news.source.adapter.js');

const labSnap = { id: 'source_lab', kind: 'lab', label: 'OpenAI', url: 'https://openai.com/news/', target_key: 'news-openai', auth_source: '', items: [] };
const redditSnap = { id: 'source_1', kind: 'reddit', label: 'Reddit', url: '', auth_source: 'reddit.com', items: ['r/LocalLLaMA'] };

test('Zauberstab Scraper → enqueue mit System-Skill universal-scraping und prompts/source-adapter.md', async () => {
  const w = await world({ seed: { news_sources: [source({ id: 'source_lab', kind: 'lab' })] } });
  const c = cmd('news.source.adapter', { topic_id: 'topic_1', source_id: 'source_lab', instruction: 'alt', record_snapshot: { source: labSnap } }, { id: 'cmd_ad' });
  assert.deepEqual(await handle(c, w.attach(c)), { task_id: 'task_news_1' });
  const [{ args, skill }] = w.sim.enqueued;
  assert.deepEqual(skill, { skill: 'universal-scraping', scope: 'core' });
  assert.equal(args.on_complete, 'news.source.adapter.complete');
  assert.equal(args.on_failed, 'news.source.adapter.failed');
  assert.equal(args.thread_key, 'business-os/news/topic_1/sources');
  assert.equal(args.payload.command.payload.source_id, 'source_lab');
  assert.deepEqual(args.instruction.split('\n').slice(0, 2), [
    'Richte für die News-Quelle "OpenAI" (https://openai.com/news/) einen wiederverwendbaren Scraper mit dem Skill universal-scraping ein.',
    'target_key: news-openai. Extrahiere je Meldung Titel, Datum, URL und Kurztext; teste den Lauf einmal.',
  ]);
});

test('Zauberstab Konto (X/Reddit) → App-Skill news/news-aggregation, prompts/source-connect.md mit Befehls-ID', async () => {
  const w = await world({ seed: { news_sources: [source()] } });
  const c = cmd('news.source.adapter', { topic_id: 'topic_1', source_id: 'source_1', auth_source: 'reddit.com', record_snapshot: { source: redditSnap } }, { id: 'cmd_conn' });
  await handle(c, w.attach(c));
  const [{ args, skill }] = w.sim.enqueued;
  assert.equal(skill.skill, 'news/news-aggregation');
  assert.equal(args.payload.output_contract, 'ctox.news.source.v1');
  assert.deepEqual(args.instruction.split('\n').slice(0, 3), [
    'Verbinde die News-Quelle "Reddit" über ein CTOX-Konto bei reddit.com (Skill news-aggregation, Abschnitt "Konto verbinden").',
    'source_id: source_1; --task-id cmd_conn.',
    'Ohne Konto: mit dem CTOX-Postfach registrieren (auth-assist-signup --generate-credential --confirm-provisioning). Antworte im Vertrag ctox.news.source.v1.',
  ]);
});

test('ohne source_id kein Agent', async () => {
  const w = await world();
  const c = cmd('news.source.adapter', {});
  await rejects(handle(c, w.attach(c)), 'invalid', /source_id/);
  assert.equal(w.sim.enqueued.length, 0);
});

test('Ablauf: complete schreibt Status und account_handle; Doppelzustellung ohne Wirkung', async () => {
  const w = await world({ seed: { news_sources: [source({ status: 'needs_auth' })] } });
  const c = cmd('news.source.adapter', { source_id: 'source_1', auth_source: 'reddit.com', record_snapshot: { source: redditSnap } }, { id: 'cmd_conn' });
  await handle(c, w.attach(c));
  const [{ task_id }] = w.sim.enqueued;
  const d = await w.sim.complete(task_id, { text: '{"schema":"ctox.news.source.v1","status":"connected","account":"ctox_news"}' });
  assert.equal(d.error, undefined, d.error?.message);
  assert.deepEqual(d.result, { ok: true, source_id: 'source_1', status: 'ok' });
  const s = w.t.doc('news_sources', 'source_1');
  assert.deepEqual([s.status, s.account_handle, s.last_error, s.last_run_at_ms, s.payload.adapter_task_id], ['ok', 'ctox_news', '', NOW, task_id]);
  // Zweite Zustellung (Core-Garantie: hoechstens einmal; Handler trotzdem idempotent)
  assert.deepEqual(await w.sim.complete(task_id, {}), { delivered: false, reason: 'duplicate' });
  const writes = w.t.writeCount();
  const { default: complete } = await import('../news.source.adapter.complete.js');
  w.t.setNow(NOW + 1000);
  const again = await complete(d.command, d.ctx);
  assert.equal(again.duplicate, true);
  assert.equal(w.t.writeCount(), writes);
  assert.equal(w.t.doc('news_sources', 'source_1').last_run_at_ms, NOW);
});

test('Ablauf: Prosa-Antwort des Scrapers = ok', async () => {
  const w = await world({ seed: { news_sources: [source({ id: 'source_lab', kind: 'lab' })] } });
  const c = cmd('news.source.adapter', { source_id: 'source_lab', record_snapshot: { source: labSnap } });
  await handle(c, w.attach(c));
  await w.sim.complete(w.sim.enqueued[0].task_id, 'Scraper eingerichtet.');
  assert.equal(w.t.doc('news_sources', 'source_lab').status, 'ok');
});

test('Ablauf: failed/cancelled/blocked markieren die Quelle (#778-Grund), zweiter Rueckruf ohne Wirkung', async () => {
  const w = await world({ seed: { news_sources: [source()] } });
  const c = cmd('news.source.adapter', { source_id: 'source_1', record_snapshot: { source: redditSnap } });
  await handle(c, w.attach(c));
  const [{ task_id }] = w.sim.enqueued;
  const d = await w.sim.fail(task_id, { code: 'failed', message: '  Timeout  ' });
  assert.equal(d.error, undefined, d.error?.message);
  assert.deepEqual([w.t.doc('news_sources', 'source_1').status, w.t.doc('news_sources', 'source_1').last_error], ['failed', 'Timeout']);
  const { default: failed } = await import('../news.source.adapter.failed.js');
  const writes = w.t.writeCount();
  assert.equal((await failed(d.command, d.ctx)).duplicate, true);
  assert.equal(w.t.writeCount(), writes);
  for (const status of ['cancelled', 'blocked']) {
    const w2 = await world({ seed: { news_sources: [source()] } });
    const c2 = cmd('news.source.adapter', { source_id: 'source_1', record_snapshot: { source: redditSnap } });
    await handle(c2, w2.attach(c2));
    const tid = w2.sim.enqueued[0].task_id;
    const cb = cmd('news.source.adapter.failed', { task_id: tid, status, result: null }, { id: `cb_${status}` });
    const r = await failed(cb, w2.attach(cb));
    assert.equal(r.reason, 'CTOX hat die Ausgabe nicht abgeschlossen.');
    assert.equal(w2.t.doc('news_sources', 'source_1').status, 'failed');
  }
});
