import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup, source, topic } from './_fixtures.mjs';
import { world } from './_sim.mjs';

const { default: handle } = await import('../news.issue.generate.js');

const snapshot = (o = {}) => ({
  topic: { id: 'topic_1', title: 'KI-Labs', prompt: 'Lab-Releases', cadence: 'daily', language: 'de', categories: ['model', 'tool'], max_posts: 5 },
  sources: [{ id: 'source_1', kind: 'lab', label: 'OpenAI', url: 'https://openai.com/news/', auth_source: '' }],
  ...o,
});
const issue = (o = {}) => ({ id: 'issue_1', topic_id: 'topic_1', issue_date: '2026-10-14', status: 'queued', payload: {}, created_at_ms: NOW - 5, updated_at_ms: NOW - 5, ...o });
const generate = (payload = {}, extra = {}) => cmd('news.issue.generate', {
  topic_id: 'topic_1', issue_id: 'issue_1', issue_date: '2026-10-14', thread_key: 'business-os/news/topic_1',
  output_contract: 'ctox.news.issue.v1', record_snapshot: snapshot(), ...payload,
}, { id: 'cmd_gen', ...extra });

test('Knopf → news.issue.generate → enqueue mit App-Skill, Template prompts/issue.md und eigenem Callback-Paar', async () => {
  const w = await world({ seed: { news_issues: [issue()] } });
  const c = generate({ instruction: 'ALTE ANWEISUNG VOM CLIENT' });
  const r = await handle(c, w.attach(c));
  assert.deepEqual(r, { task_id: 'task_news_1' });
  const [{ args, skill }] = w.sim.enqueued;
  assert.deepEqual(skill, { skill: 'news/news-aggregation', scope: 'app', path: 'skills/news-aggregation/SKILL.md' });
  assert.equal(args.on_complete, 'news.issue.complete');
  assert.equal(args.on_failed, 'news.issue.failed');
  assert.equal(args.idempotency_key, 'cmd_gen');
  assert.equal(args.thread_key, 'business-os/news/topic_1');
  assert.deepEqual(Object.keys(args).sort(), ['idempotency_key', 'instruction', 'on_complete', 'on_failed', 'payload', 'skill', 'thread_key', 'title']);
  assert.equal(args.payload.command.type, 'news.issue.generate');
  assert.equal(args.payload.command.payload.issue_id, 'issue_1');
  // Wortlaut des Bestands (core/news-model.mjs buildIssueInstruction), Platzhalter gefuellt, Client-Anweisung ignoriert.
  const lines = args.instruction.split('\n');
  assert.deepEqual(lines.slice(0, 5), [
    'Erstelle die News-Ausgabe für das Thema "KI-Labs" mit dem Skill news-aggregation.',
    'Fokus: Lab-Releases',
    'Sprache: Deutsch. Höchstens 5 Beiträge, Kategorien: model, tool.',
    'Quellen (1): lab:OpenAI.',
    'Liefere das Ergebnis im Vertrag ctox.news.issue.v1; jeder Beitrag braucht mindestens eine Quelle mit Link.',
  ]);
  assert.ok(!args.instruction.includes('ALTE ANWEISUNG'));
  assert.ok(!args.instruction.includes('--task-id'), 'ohne angemeldete Quelle keine --task-id-Zeile');
  const context = JSON.parse(args.instruction.slice(args.instruction.indexOf('{')));
  assert.equal(context.record_snapshot.sources[0].id, 'source_1');
  assert.equal(context.command_id, 'cmd_gen');
  // task_id an der Ausgabe nachgetragen (Sprung in die CTOX-App, Idempotenz des Rueckrufs)
  assert.equal(w.t.doc('news_issues', 'issue_1').task_id, 'task_news_1');
});

test('Template: angemeldete Quelle nennt die Befehls-ID, vorherige Ausgabe, Englisch; leerer Fokus entfaellt', async () => {
  const w = await world({ seed: { news_issues: [issue({ id: 'issue_0', issue_date: '2026-10-13', status: 'ready' })] } });
  const snap = snapshot({ sources: [{ id: 'source_r', kind: 'reddit', label: 'Reddit', auth_source: 'reddit.com' }] });
  snap.topic = { ...snap.topic, prompt: '', language: 'en' };
  const c = generate({ record_snapshot: snap, previous_issue_id: 'issue_0' }, { id: 'cmd_signed' });
  await handle(c, w.attach(c));
  const lines = w.sim.enqueued[0].args.instruction.split('\n');
  assert.equal(lines[1], 'Sprache: Englisch. Höchstens 5 Beiträge, Kategorien: model, tool.');
  assert.ok(lines.includes('Angemeldete Quellen (X, Reddit) über das CTOX-Konto lesen; --task-id cmd_signed.'));
  assert.ok(lines.includes('Vorherige Ausgabe 2026-10-13: wiederhole keine Meldung aus früheren Ausgaben (covered_urls).'));
  assert.ok(!lines.some((l) => l.startsWith('Fokus:')));
});

test('Doppelter Knopfdruck (gleiche Befehls-ID) startet keinen zweiten Agenten', async () => {
  const w = await world({ seed: { news_issues: [issue()] } });
  const c = generate();
  await handle(c, w.attach(c));
  await handle(c, w.attach(c));
  assert.equal(w.sim.tasks.size, 1);
  assert.ok(w.sim.log.some((e) => e.event === 'idempotent_hit'));
});

test('Befehl ohne issue_id/topic_id startet keinen Agenten', async () => {
  const w = await world();
  const c1 = generate({ issue_id: '' });
  await rejects(handle(c1, w.attach(c1)), 'invalid', /issue_id/);
  const c2 = generate({ topic_id: '' });
  await rejects(handle(c2, w.attach(c2)), 'invalid', /topic_id/);
  assert.equal(w.sim.enqueued.length, 0);
});

test('Planer-Ausloesung: Ausgabe queued anlegen, Folgebefehl mit fester ID, task_id nachtragen', async () => {
  const sent = [];
  const t = await setup({
    seed: { news_topics: [topic()], news_sources: [source(), source({ id: 'source_off', enabled: false })] },
    commands: { 'news.issue.generate': async (c) => (sent.push(c), { id: c.id, task_id: 'q_sched' }) },
  });
  const scheduledFor = '2026-10-16T07:00:00+02:00';
  const p = { trigger: 'schedule', topic_id: 'topic_1', owner_user_id: 'user_owner', cadence: 'daily', deadline_date: '', schedule_task_id: 'sched_1', schedule_name: 'news:topic_1', schedule_thread_key: 'business-os/news/topic_1', scheduled_for: scheduledFor };
  const r = await handle(cmd('news.issue.generate', p), t.ctx);
  assert.deepEqual(r, { run_key: 'q_sched', outcome: 'emitted' });
  const digest = createHash('sha256').update(`sched_1::${scheduledFor}`).digest('hex').slice(0, 24);
  const i = t.doc('news_issues', `issue_sched_${digest}`);
  assert.equal(i.status, 'queued');
  assert.equal(i.issue_date, '2026-10-16');
  assert.equal(i.command_id, `cmd_news_issue_generate_sched_${digest}`);
  assert.equal(i.task_id, 'q_sched');
  assert.equal(sent[0].payload.record_snapshot.sources.length, 1);
  assert.equal(sent[0].payload.instruction, undefined, 'Anweisung entsteht erst im Folgebefehl aus prompts/issue.md');
});

test('Planer-Folgebefehl: Rust-Test scheduled_issue_names_its_command_for_signed_in_sources ueber das Template', async () => {
  const { generatePayload } = await import('../_lib/news.js');
  const t = { id: 'topic_1', title: 'KI', prompt: '', max_posts: 5 };
  const reddit = { id: 'source_1', kind: 'reddit', label: 'Reddit', items: ['r/LocalLLaMA'] };
  const lab = { id: 'source_2', kind: 'lab', label: 'OpenAI' };
  const w = await world({ seed: { news_issues: [issue()] } });
  const c = cmd('news.issue.generate', generatePayload(t, [reddit, lab], 'issue_1', '2026-10-14'), { id: 'cmd_1' });
  await handle(c, w.attach(c));
  const lines = w.sim.enqueued[0].args.instruction.split('\n');
  assert.equal(lines[3], 'Angemeldete Quellen (X, Reddit) über das CTOX-Konto lesen; --task-id cmd_1.');
  const c2 = cmd('news.issue.generate', generatePayload(t, [lab], 'issue_1', '2026-10-14'), { id: 'cmd_2' });
  await handle(c2, w.attach(c2));
  assert.equal(w.sim.enqueued[1].args.instruction.includes('--task-id'), false);
});

test('Planer-Ausloesung: Stichtag uebersprungen, deaktiviertes Thema uebersprungen, falsche Bindung', async () => {
  const t = await setup({ seed: { news_topics: [topic({ enabled: false })] } });
  const base = { trigger: 'schedule', topic_id: 'topic_1', schedule_task_id: 's', schedule_name: 'news:topic_1', schedule_thread_key: 'business-os/news/topic_1', scheduled_for: '2026-10-16T07:00:00Z' };
  assert.deepEqual(await handle(cmd('news.issue.generate', { ...base, cadence: 'deadline', deadline_date: '2026-11-01' }), t.ctx), { run_key: 'news-skip:s:2026-10-16T07:00:00Z', outcome: 'skipped' });
  assert.equal((await handle(cmd('news.issue.generate', base), t.ctx)).outcome, 'skipped');
  await rejects(handle(cmd('news.issue.generate', { ...base, schedule_name: 'news:other' }), t.ctx), 'invalid', /binding/);
});
