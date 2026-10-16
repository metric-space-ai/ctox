// SKL-B2: Knopf → Befehl → agent.task.enqueue (Template + Skill) → Rückruf → eigene Collections, gegen die
// agent-task-Simulation (v4/tools/agent-sim). node --test modules/research/handlers/tests/skills.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';

import run from '../research.systematic.run.js';
import refresh from '../research.knowledge.refresh.js';
import runFailed from '../research.systematic.run.failed.js';
import refreshComplete from '../research.knowledge.refresh.complete.js';
import refreshFailed from '../research.knowledge.refresh.failed.js';
import knowledgeIngest from '../../../knowledge/handlers/knowledge.book.ingest.js';
import { createCtx as createKnowledgeCtx } from '../../../knowledge/handlers/tests/_ctx.mjs';

const APP_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function world(collections = {}, { commandResult } = {}) {
  const { ctx, state } = makeCtx({
    commandResult,
    collections: {
      research_tasks: [{ id: 'rt-1', title: 'Lagerlasten', status: 'collecting', knowledge_domain: 'drone/bearings', payload: { keep: true }, created_at_ms: 1, updated_at_ms: 1 }],
      research_runs: [{ id: 'run-1', task_id: 'rt-1', status: 'queued', command_id: 'cmd-ui-1', research_command_id: 'cmd_r1', payload: {}, created_at_ms: 1, updated_at_ms: 1 }],
      ...collections,
    },
    caps: {},
  });
  ctx.cap = { agent: {} }; // Simulation statt Proxy-Attrappe (sim.attach setzt agent.task und asset)
  // files (Kern §16.32): inhaltsadressiert, ref = sha256 der Bytes.
  const files = new Map();
  ctx.files = {
    async put(bytes, meta) {
      const ref = createHash('sha256').update(bytes).digest('hex');
      files.set(ref, { text: new TextDecoder().decode(bytes), meta });
      return { ref: Promise.resolve(ref) };
    },
  };
  const sim = createAgentTaskSim({ appId: 'research', appDir: APP_DIR, ctxFactory: () => ctx });
  return { ctx, state, sim, files, coll: (name) => state.collections.get(name) };
}

const runPayload = (extra = {}) => ({
  title: 'Research · Lagerlasten',
  priority: 'high',
  research_mode: 'library+living_dashboard',
  research_brief: {
    task_title: 'Lagerlasten',
    task_prompt: 'Finde Lastdaten für Drohnenlager.',
    criteria: 'nur Messdaten',
    scoring_model: [{ id: 'relevance', label: 'Relevanz', weight: 2 }, { id: 'quality', label: 'Qualität', weight: 1 }],
    axis_x: 'relevance',
    axis_y: 'quality',
    verified_source_count: 3,
  },
  thread_key: 'business-os/research/rt-1',
  research_run_id: 'run-1',
  research_command_id: 'cmd_r1',
  knowledge_domain: 'drone/bearings',
  web_stack_plan: {
    depth: 'exhaustive', target_verified_sources: 100, minimum_candidate_sources: 200,
    minimum_discovery_rounds: 6, minimum_scholarly_rounds: 2, exclude_urls: ['https://a.example', 'https://b.example'],
  },
  record_id: 'rt-1',
  module: 'research',
  ...extra,
});

// Erwarteter Text exakt wie der frühere Literal-Aufbau in index.js (vor SKL-B2, `.filter(Boolean).join('\n')`).
function legacyRunInstruction(p) {
  const b = p.research_brief;
  const w = p.web_stack_plan;
  return [
    `Führe die systematische Recherche für "${b.task_title}" mit dem System-Skill systematic-research fort.`,
    `Research Task ID: ${p.record_id}`,
    `Research Run ID: ${p.research_run_id}`,
    `Research Command ID: ${p.research_command_id}`,
    `Requested Discovery Depth: ${w.depth}`,
    `Target Verified Sources: ${w.target_verified_sources}`,
    `Minimum Candidate Sources: ${w.minimum_candidate_sources}`,
    `Minimum Discovery Rounds: ${w.minimum_discovery_rounds}`,
    `Minimum Scholarly Rounds: ${w.minimum_scholarly_rounds}`,
    `Knowledge domain: ${p.knowledge_domain}`,
    '',
    b.task_prompt,
    b.criteria ? `Kriterien:\n${b.criteria}` : null,
    `Scoring-Modell:\n${b.scoring_model.map((axis) => `- ${axis.id}: ${axis.label}; weight=${axis.weight}`).join('\n')}`,
    `Portfolio axes: x=${b.axis_x}, y=${b.axis_y}`,
    '',
    `Bekannte Quellen: ${w.exclude_urls.length}; davon bereits evidence-eligible: ${b.verified_source_count}. Die vollständige kanonische Exclude-Liste steht in web_stack_plan.exclude_urls.`,
    'Arbeite iterativ mit den typisierten Web-Stack-Werkzeugen, folge bei wissenschaftlichen Quellen den Referenzen und führe zwei orthogonale Nulltreffer-Runden durch. Behandle Discovery nur als Kandidatenmenge. Evidence, Knowledge, Graph und Reports dürfen ausschließlich aus gelesenen, gesnapshotpten und vom Evidence-Gate zugelassenen Originalquellen entstehen.',
    'Nutze die vom System materialisierte Skill-Anleitung und den serverseitigen Writeback-Vertrag. Erzeuge keine parallelen Tabellen, schreibe nicht direkt in Business-OS-Datenbanken und starte keine Child Agents.',
  ].filter(Boolean).join('\n');
}

function legacyGraphInstruction(p) {
  const related = p.graph_focus.related_terms;
  return [
    `Führe eine gezielte Nachrecherche für den Research-Graph "${p.research_brief.task_title}" durch.`,
    `Research Run ID: ${p.research_run_id}`,
    `Research Command ID: ${p.research_command_id}`,
    `Fokusbegriff: ${p.graph_focus.label}`,
    related.length ? `Benachbarte Begriffe: ${related.join(', ')}` : '',
    `Knowledge domain: ${p.knowledge_domain}`,
    '',
    p.research_brief.task_prompt || '',
    '',
    'Nutze systematic-research und die CTOX Web-Research-Tools. Prüfe die vorhandenen Belege, schließe erkennbare Lücken und schreibe neue Quellen und Belege sofort in die bestehenden Knowledge-Tabellen.',
    `Schreibe auf jede erzeugte oder aktualisierte Knowledge-Zeile research_run_id=${p.research_run_id} und research_command_id=${p.research_command_id}.`,
    'Aktualisiere semantic_graph_nodes und semantic_graph_edges inkrementell aus verifizierten Evidenzzeilen. Verwende kanonische fachliche Mehrwort-Begriffe, klare Clusterbezeichnungen und typisierte Relationen. Technische Feldnamen, IDs, Hashes, URLs und generische Metadaten dürfen keine Konzepte werden. Jeder Knoten und jede Kante braucht Source-IDs, Konfidenz und Provenienz; überschreibe keine belegten Daten ohne neuen Nachweis.',
  ].filter(Boolean).join('\n');
}

test('Knopf „Research starten“ → enqueue: Template systematic-run wörtlich, Core-Skill systematic-research, beide Rückrufe', async () => {
  const { ctx, sim } = world();
  const command = makeCommand('research.systematic.run', runPayload(), { id: 'cmd-ui-1' });
  const out = await run(command, sim.attach(ctx, command));
  assert.equal(out.status, 'queued');
  const [{ args, skill }] = sim.enqueued;
  assert.deepEqual(skill, { skill: 'systematic-research', scope: 'core' });
  assert.equal(args.instruction, legacyRunInstruction(command.payload));
  assert.equal(args.on_complete, 'research.systematic.run.complete');
  assert.equal(args.on_failed, 'research.systematic.run.failed');
  assert.equal(args.idempotency_key, 'cmd-ui-1');
  assert.equal(args.payload.business_command_id, 'cmd-ui-1');
  assert.equal(args.thread_key, 'business-os/research/rt-1');
  // ohne Kriterien entfällt die Zeile ganz (wie filter(Boolean))
  const p2 = runPayload({ research_brief: { ...runPayload().research_brief, criteria: '' } });
  const c2 = makeCommand('research.systematic.run', p2, { id: 'cmd-ui-2' });
  await run(c2, sim.attach(ctx, c2));
  assert.equal(sim.enqueued[1].args.instruction, legacyRunInstruction(p2));
  assert.doesNotMatch(sim.enqueued[1].args.instruction, /Kriterien/);
});

test('Graph-Nachrecherche → Template graph-gap wörtlich (mit/ohne Nachbarbegriffe)', async () => {
  const { ctx, sim } = world();
  for (const [i, related] of [['a', ['Wälzlager', 'Drehzahl']], ['b', []]]) {
    const p = runPayload({ research_mode: 'targeted_graph_gap', graph_focus: { node_id: 'n1', label: 'Axiallast', related_terms: related, source_ids: [] } });
    const command = makeCommand('research.systematic.run', p, { id: `cmd-graph-${i}` });
    await run(command, sim.attach(ctx, command));
    assert.equal(sim.enqueued.at(-1).args.instruction, legacyGraphInstruction(p));
    assert.equal(sim.enqueued.at(-1).skill.skill, 'systematic-research');
  }
});

test('ohne research_brief kein Auftrag (Template streng)', async () => {
  const { ctx, sim } = world();
  const command = makeCommand('research.systematic.run', { instruction: 'Recherchiere' });
  await rejectsWithCode(run(command, sim.attach(ctx, command)), 'invalid');
  assert.equal(sim.enqueued.length, 0);
});

test('Lauf fehlgeschlagen → research_runs/research_tasks failed; Doppelzustellung ohne Wirkung', async () => {
  const { ctx, sim, coll } = world();
  const command = makeCommand('research.systematic.run', runPayload(), { id: 'cmd-ui-1' });
  await run(command, sim.attach(ctx, command));
  const { task_id } = sim.enqueued[0];
  const d = await sim.fail(task_id, { code: 'harness', message: 'Turn abgebrochen' });
  assert.equal(d.delivered, true);
  assert.equal(d.command.type, 'research.systematic.run.failed');
  assert.equal(d.result.status, 'failed');
  assert.equal(coll('research_runs').get('run-1').status, 'failed');
  assert.equal(coll('research_runs').get('run-1').payload.error.message, 'Turn abgebrochen');
  assert.equal(coll('research_tasks').get('rt-1').status, 'failed');
  assert.equal(coll('research_tasks').get('rt-1').payload.keep, true);
  assert.deepEqual(await sim.fail(task_id, 'nochmal'), { delivered: false, reason: 'duplicate' });
  const before = JSON.stringify([...coll('research_runs').values(), ...coll('research_tasks').values()]);
  const again = await runFailed(d.command, sim.attach(ctx, d.command));
  assert.equal(again.duplicate, true);
  assert.equal(JSON.stringify([...coll('research_runs').values(), ...coll('research_tasks').values()]), before);
});

test('Lauf abgebrochen (on_failed status cancelled) → Lauf cancelled, Aufgabe wieder ready', async () => {
  const { ctx, sim, coll } = world();
  const command = makeCommand('research.systematic.run', runPayload(), { id: 'cmd-ui-1' });
  await run(command, sim.attach(ctx, command));
  const { task_id } = sim.enqueued[0];
  const cb = { id: 'cb-1', type: 'research.systematic.run.failed', payload: { task_id, status: 'cancelled', result: null } };
  const out = await runFailed(cb, sim.attach(ctx, cb));
  assert.equal(out.status, 'cancelled');
  assert.equal(coll('research_runs').get('run-1').status, 'cancelled');
  assert.equal(coll('research_tasks').get('rt-1').status, 'ready');
  assert.equal(coll('research_tasks').get('rt-1').payload.last_error.status, 'cancelled');
});

// ------------------------------------------------------------------------------------------- Knowledge Book

const refreshPayload = () => ({
  title: 'Knowledge aktualisieren · Lagerlasten',
  priority: 'high',
  research_brief: { task_title: 'Lagerlasten' },
  update_mode: 'upsert',
  thread_key: 'business-os/research/rt-1/knowledge',
  research_task_id: 'rt-1',
  research_run_id: 'run-1',
  research_command_id: 'cmd_k1',
  knowledge_domain: 'drone/bearings',
  knowledge_version_id: 'knowledge-v7',
  source_lineage: [{ source_id: 'src-1', canonical_url: 'https://example.org/p1' }],
  record_id: 'rt-1',
  module: 'research',
});

function legacyRefreshInstruction(p) {
  return [
    `Baue oder aktualisiere die Knowledge Base fuer das abgeschlossene Research "${p.research_brief.task_title}".`,
    `Research Task ID: ${p.research_task_id}`,
    `Research Run ID: ${p.research_run_id}`,
    `Research Command ID: ${p.research_command_id}`,
    `Knowledge domain: ${p.knowledge_domain}`,
    '',
    'Erzeuge bzw. aktualisiere einen fachlichen Skill/Skillbook und die dazugehoerigen Runbooks und Ressourcen.',
    'Verwende die bestehenden stabilen IDs und aktualisiere vorhandene Elemente per Upsert; erzeuge keine parallelen Kopien derselben Knowledge Base.',
    'Der Skill ist der Wissens- und Arbeits-Hub, aber keine Ersatzquelle: Jede faktische Aussage muss auf die originalen source_id/source_url-Eintraege aus source_catalog und evidence_points zurueckverweisen.',
    'Uebernimm keine unbelegten Aussagen. Halte Quellen, Evidenz, Tabellen und Ableitungen getrennt nachvollziehbar.',
    'Erzeuge Runbooks fuer wiederkehrende Analysen und Dokumenttypen, die dieses Knowledge und bei Bedarf die Originalquellen erneut lesen.',
    'Bewahre die Verbindung zu Research Task, Research Run und Knowledge-Tabellen, damit spaetere Research-Laeufe dieselben Elemente aktualisieren koennen.',
  ].join('\n');
}

const result = (extra = {}) => JSON.stringify({
  schema_version: 'ctox.research.knowledge-refresh.v1',
  research_task_id: 'rt-1', research_run_id: 'run-1', research_command_id: 'cmd_k1', knowledge_domain: 'drone/bearings',
  knowledge_version_id: 'knowledge-v7',
  summary: 'Skillbook und ein Runbook angelegt.',
  elements: [
    { id: 'drone/bearings/skillbook/main', kind: 'skillbook', title: 'Drohnenlager', markdown: '# Drohnenlager', citations: [{ source_id: 'src-1' }] },
    { id: 'drone/bearings/runbook/last', kind: 'runbook', title: 'Lastfall prüfen', markdown: '1. …' },
  ],
  gaps: [],
  ...extra,
});

async function startRefresh(options) {
  const w = world({}, options);
  const command = makeCommand('research.knowledge.refresh', refreshPayload(), { id: 'cmd-ui-k1' });
  await refresh(command, w.sim.attach(w.ctx, command));
  return { ...w, command, taskId: w.sim.enqueued[0].task_id };
}

test('Knopf „Knowledge aufbauen“ → enqueue mit Paket-Skill research/knowledge-refresh, Template wörtlich, Rückrufe', async () => {
  const { sim, command } = await startRefresh();
  const [{ args, skill }] = sim.enqueued;
  assert.deepEqual(skill, { skill: 'research/knowledge-refresh', scope: 'app', path: 'skills/knowledge-refresh/SKILL.md' });
  assert.equal(args.instruction, legacyRefreshInstruction(command.payload));
  assert.equal(args.on_complete, 'research.knowledge.refresh.complete');
  assert.equal(args.on_failed, 'research.knowledge.refresh.failed');
  assert.equal(args.thread_key, 'business-os/research/rt-1/knowledge');
});

test('Knowledge-Rückruf complete → Book als files-ref per ctx.command an knowledge, research_notes nur Verweis; Doppelzustellung ohne Wirkung', async () => {
  const { ctx, sim, state, files, coll, taskId } = await startRefresh();
  const d = await sim.complete(taskId, { text: 'fertig' }, { workspace: { 'knowledge-refresh/result.json': { text: result(), sha256: 'x' } } });
  assert.equal(d.command.type, 'research.knowledge.refresh.complete');
  assert.deepEqual(d.command.payload.workspace_refs, ['knowledge-refresh/result.json']);
  assert.equal(d.result.status, 'completed');
  // genau ein Befehl an die App knowledge, mit BlobRef statt Inhalt
  assert.equal(state.commands.length, 1);
  const [sent] = state.commands;
  assert.equal(sent.type, 'knowledge.book.ingest');
  assert.deepEqual(sent.opts, { id: `${taskId}:knowledge-book`, module: 'knowledge', record_id: 'rt-1' });
  assert.match(sent.payload.book.ref, /^[0-9a-f]{64}$/);
  assert.equal(sent.payload.knowledge_domain, 'drone/bearings');
  assert.equal(sent.payload.source.app, 'research');
  assert.equal(sent.payload.elements, undefined);
  const book = JSON.parse(files.get(sent.payload.book.ref).text);
  assert.equal(book.schema_version, 'ctox.knowledge.book.v1');
  assert.deepEqual(book.elements.map((el) => el.id), ['drone/bearings/skillbook/main', 'drone/bearings/runbook/last']);
  // research_notes: nur ein Verweis-Dokument, keine Elemente mehr
  const notes = [...coll('research_notes').values()];
  assert.equal(notes.length, 1);
  assert.equal(notes[0].id, 'kb:rt-1');
  assert.equal(notes[0].kind, 'knowledge_book');
  assert.equal(notes[0].payload.owner_app, 'knowledge');
  assert.equal(notes[0].payload.book_ref, sent.payload.book.ref);
  assert.equal(notes[0].payload.knowledge_command_id, `${taskId}:knowledge-book`);
  assert.equal(notes[0].body, undefined);
  const st = coll('research_tasks').get('rt-1').payload.knowledge_refresh;
  assert.equal(st.status, 'completed');
  assert.equal(st.task_id, taskId);
  assert.equal(st.book_ref, sent.payload.book.ref);
  assert.equal(st.element_ids.length, 2);
  assert.deepEqual(await sim.complete(taskId, {}), { delivered: false, reason: 'duplicate' });
  const before = JSON.stringify([...coll('research_notes').values(), ...coll('research_tasks').values()]);
  const again = await refreshComplete(d.command, sim.attach(ctx, d.command));
  assert.equal(again.duplicate, true);
  assert.equal(state.commands.length, 1);
  assert.equal(JSON.stringify([...coll('research_notes').values(), ...coll('research_tasks').values()]), before);
});

test('Ende-zu-Ende research → knowledge: der gesendete Befehl legt die Elemente in knowledge_items/knowledge_runbooks ab', async () => {
  const { sim, state, files, taskId } = await startRefresh();
  await sim.complete(taskId, {}, { workspace: { 'knowledge-refresh/result.json': { text: result() } } });
  const [sent] = state.commands;
  const kctx = createKnowledgeCtx({ files: Object.fromEntries([...files].map(([ref, f]) => [ref, f.text])) });
  const out = await knowledgeIngest({ id: sent.opts.id, type: sent.type, record_id: sent.opts.record_id, payload: sent.payload, app_id: 'knowledge' }, kctx);
  assert.equal(out.status, 'completed');
  assert.equal(kctx.$.doc('knowledge_items', 'drone/bearings/skillbook/main').kind, 'skillbook');
  assert.deepEqual(kctx.$.doc('knowledge_items', 'drone/bearings/skillbook/main').payload.citations, [{ source_id: 'src-1' }]);
  assert.equal(kctx.$.doc('knowledge_runbooks', 'drone/bearings/runbook/last').title, 'Lastfall prüfen');
  assert.equal(kctx.$.doc('knowledge_tasks', `${taskId}:knowledge-book`).record_id, 'rt-1');
});

test('knowledge lehnt den Befehl ab → knowledge_refresh failed (knowledge_rejected), kein Verweis', async () => {
  const { sim, coll, taskId } = await startRefresh({ commandResult: (type, payload, opts, id) => ({ id, status: 'rejected', reason: 'not accepted from research' }) });
  const d = await sim.complete(taskId, {}, { workspace: { 'knowledge-refresh/result.json': { text: result() } } });
  assert.equal(d.result.status, 'failed');
  assert.equal(coll('research_notes')?.size ?? 0, 0);
  const st = coll('research_tasks').get('rt-1').payload.knowledge_refresh;
  assert.equal(st.status, 'failed');
  assert.equal(st.error.code, 'knowledge_rejected');
});

test('Knowledge-Ergebnis mit unbekannter Quelle / ohne Datei → fail-closed, keine Notizen', async () => {
  for (const workspace of [
    { 'knowledge-refresh/result.json': { text: result({ elements: [{ id: 'x', kind: 'skill', title: 'X', citations: [{ source_id: 'fremd' }] }] }) } },
    { 'andere/datei.json': { text: '{}' } },
    { 'knowledge-refresh/result.json': { text: result({ research_command_id: 'cmd_andere' }) } },
  ]) {
    const { sim, coll, state, taskId } = await startRefresh();
    const d = await sim.complete(taskId, {}, { workspace });
    assert.equal(d.result.status, 'failed');
    assert.equal(coll('research_notes')?.size ?? 0, 0);
    assert.equal(state.commands.length, 0);
    assert.equal(coll('research_tasks').get('rt-1').payload.knowledge_refresh.status, 'failed');
    assert.match(coll('research_tasks').get('rt-1').payload.knowledge_refresh.error.message, /source|missing|match/);
  }
});

test('Knowledge-Rückruf failed/cancelled → knowledge_refresh failed bzw. cancelled, idempotent', async () => {
  const { ctx, sim, coll, taskId } = await startRefresh();
  const d = await sim.fail(taskId, { code: 'quota', message: 'Kontingent' });
  assert.equal(d.command.type, 'research.knowledge.refresh.failed');
  assert.equal(coll('research_tasks').get('rt-1').payload.knowledge_refresh.status, 'failed');
  assert.equal((await refreshFailed(d.command, sim.attach(ctx, d.command))).duplicate, true);
  const w2 = await startRefresh();
  const cb = { id: 'cb', type: 'research.knowledge.refresh.failed', payload: { task_id: w2.taskId, status: 'cancelled', result: null } };
  await refreshFailed(cb, w2.sim.attach(w2.ctx, cb));
  assert.equal(w2.coll('research_tasks').get('rt-1').payload.knowledge_refresh.status, 'cancelled');
});
