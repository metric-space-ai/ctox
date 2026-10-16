// SKL-B2: 7 Knöpfe ctox.knowledge.* → enqueue (Template + Skill knowledge/knowledge-maintenance) → Rückruf →
// knowledge_tasks / knowledge_items / knowledge_runbooks, gegen die agent-task-Simulation.
// Aus modules/knowledge/handlers: node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createCtx } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';
import { KNOWLEDGE_COMMANDS } from '../_lib/agent.js';
import documentModify from '../ctox.knowledge.document.modify.js';
import runbookModify from '../ctox.knowledge.runbook.modify.js';
import runbookExecute from '../ctox.knowledge.runbook.execute.js';
import bookCreate from '../ctox.knowledge.book.create.js';
import bookImport from '../ctox.knowledge.book.import.js';
import bookExport from '../ctox.knowledge.book.export.js';
import chat from '../ctox.knowledge.chat.js';
import taskComplete from '../knowledge.task.complete.js';
import taskFailed from '../knowledge.task.failed.js';

const APP_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const NOW = 1_700_000_000_000;

function world() {
  const ctx = createCtx({ now: NOW, seed: { knowledge_items: [{ id: 'skill-a', kind: 'skill', title: 'Alt', markdown: '# Alt', payload: { keep: 1 }, updated_at_ms: 1 }] } });
  const sim = createAgentTaskSim({ appId: 'knowledge', appDir: APP_DIR, ctxFactory: () => ctx });
  return { ctx, sim };
}

const cmd = (type, payload, id = `cmd-${type}`) => ({ id, type, payload: { module: 'knowledge', client_context: { active_tab: 'docs', capability_token: 'geheim' }, ...payload }, actor: { id: 'u1' }, app_id: 'knowledge' });

// Anweisungs-Literale vor SKL-B2 (index.js :2250, :2299, :2427, :2463, :2504) – müssen wörtlich am Anfang stehen.
const LEGACY = {
  'ctox.knowledge.document.modify': 'Prüfe und persistiere die folgende Knowledge-Änderung im CTOX Knowledge Store. Erhalte Skill-, Skillbook-, Runbook- und Ressourcenstruktur; schreibe Änderungen über die passende CTOX-Schicht zurück.',
  'ctox.knowledge.runbook.modify': 'Prüfe und persistiere die Runbook-Änderung im CTOX Knowledge Store. Aktualisiere Runbook, Items, Ressourcenbindungen und Ausführungskontrakt konsistent.',
  'ctox.knowledge.book.create': 'Lege ein neues Knowledge Book im CTOX Knowledge Store an. Erzeuge die Skillbook-Struktur, einen initialen Skill und bereite Runbook-/DataFrame-Slots vor.',
  'ctox.knowledge.book.import': 'Importiere die angegebene Quelle als Knowledge Book. Extrahiere Skillbook, Skill, Runbooks und DataFrame-Definitionen, ohne bestehende Knowledge-Struktur unkontrolliert zu ueberschreiben.',
  'ctox.knowledge.book.export': 'Exportiere Knowledge Books aus dem CTOX Knowledge Store mit Skillbook-, Runbook- und DataFrame-Metadaten.',
  'ctox.knowledge.runbook.execute': 'Schritt 1: Lastfall lesen.',
  'ctox.knowledge.chat': 'Fasse die Tabelle zusammen.',
};
const CASES = [
  [documentModify, 'ctox.knowledge.document.modify', { title: 'Knowledge Änderung · Alt', record_id: 'skill-a', markdown: '# Neu', selected_item: { id: 'skill-a', api_key: 'geheim-payload' } }],
  [runbookModify, 'ctox.knowledge.runbook.modify', { title: 'Runbook Änderung · R', record_id: 'rb-1', runbook: { id: 'rb-1' }, draft: { title: 'R', prompt: 'x' } }],
  [runbookExecute, 'ctox.knowledge.runbook.execute', { title: 'Runbook ausführen · R', record_id: 'rb-1', runbook_instruction: 'Schritt 1: Lastfall lesen.', required_output: 'Ergebnis', priority: 'normal', thread_key: 'business-os/knowledge' }],
  [bookCreate, 'ctox.knowledge.book.create', { title: 'Knowledge Book erstellen · K', record_id: 'knowledge:create', knowledge_book: { title: 'K' } }],
  [bookImport, 'ctox.knowledge.book.import', { title: 'Knowledge Book importieren · /x', record_id: 'knowledge:import', import_request: { source: '/x' } }],
  [bookExport, 'ctox.knowledge.book.export', { title: 'Knowledge Books exportieren · all', record_id: 'knowledge:export', export_request: { scope: 'all' } }],
  [chat, 'ctox.knowledge.chat', { title: 'Knowledge Daten bearbeiten · X', record_id: 'skill-a', user_message: 'Fasse die Tabelle zusammen.', mode: 'data', thread_key: 'business-os/knowledge' }],
];

test('jeder der 7 Typen hat einen Handler und ein Template im Paket', () => {
  assert.deepEqual(Object.keys(KNOWLEDGE_COMMANDS).sort(), CASES.map((c) => c[1]).sort());
});

test('Knopf → enqueue: Template wörtlich, Paket-Skill, beide Rückrufe, keine Geheimnisse im Prompt, knowledge_tasks queued', async () => {
  for (const [handler, type, payload] of CASES) {
    const { ctx, sim } = world();
    const command = cmd(type, payload);
    const out = await handler(command, sim.attach(ctx, command));
    assert.equal(out.status, 'queued', type);
    const [{ args, skill }] = sim.enqueued;
    assert.deepEqual(skill, { skill: 'knowledge/knowledge-maintenance', scope: 'app', path: 'skills/knowledge-maintenance/SKILL.md' }, type);
    assert.ok(args.instruction.startsWith(`${LEGACY[type]}\nBusiness OS command:\n- command_id: ${command.id}\n- module: knowledge\n- type: ${type}\n- record_id: ${payload.record_id}\n`), type);
    assert.match(args.instruction, /Payload JSON:\n\{/);
    assert.doesNotMatch(args.instruction, /geheim|capability_token/, `${type}: sensible Schlüssel bleiben draußen (store.rs:27126)`);
    assert.equal(args.title, payload.title);
    assert.equal(args.thread_key, 'business-os/knowledge');
    assert.equal(args.on_complete, 'knowledge.task.complete');
    assert.equal(args.on_failed, 'knowledge.task.failed');
    assert.equal(args.idempotency_key, command.id);
    const rec = ctx.$.doc('knowledge_tasks', out.task_id);
    assert.equal(rec.status, 'queued');
    assert.equal(rec.command_type, type);
  }
});

test('Pflichtwerte: runbook.execute ohne Runbook-Text, chat ohne Nachricht → invalid, kein Auftrag', async () => {
  for (const [handler, type] of [[runbookExecute, 'ctox.knowledge.runbook.execute'], [chat, 'ctox.knowledge.chat']]) {
    const { ctx, sim } = world();
    const command = cmd(type, { record_id: 'x' });
    await assert.rejects(handler(command, sim.attach(ctx, command)), { code: 'invalid' });
    assert.equal(sim.enqueued.length, 0);
  }
});

const writeback = (extra = {}) => ({
  text: JSON.stringify({
    schema_version: 'ctox.knowledge.writeback.v1',
    summary: 'Skill aktualisiert, Runbook angelegt.',
    items: [{ id: 'skill-a', kind: 'skill', title: 'Neu', markdown: '# Neu', payload: { source: 'x' } }],
    runbooks: [{ id: 'rb-1', kind: 'runbook', title: 'Lastfall', prompt: 'Schritt 1', items: [{ label: 'REG-01', title: 'a' }] }],
    ...extra,
  }),
  sha256: 'x',
});

async function started(type = 'ctox.knowledge.document.modify') {
  const w = world();
  const [handler, , payload] = CASES.find((c) => c[1] === type);
  const command = cmd(type, payload);
  await handler(command, w.sim.attach(w.ctx, command));
  return { ...w, taskId: w.sim.enqueued[0].task_id };
}

test('complete → knowledge_items/knowledge_runbooks upsert, knowledge_tasks completed; Doppelzustellung ohne Wirkung', async () => {
  const { ctx, sim, taskId } = await started();
  const d = await sim.complete(taskId, { text: 'ok' }, { workspace: { 'knowledge/writeback.json': writeback() } });
  assert.equal(d.command.type, 'knowledge.task.complete');
  assert.equal(d.result.status, 'completed');
  const item = ctx.$.doc('knowledge_items', 'skill-a');
  assert.equal(item.title, 'Neu');
  assert.equal(item.markdown, '# Neu');
  assert.deepEqual(item.payload, { keep: 1, source: 'x', last_task_id: taskId });
  assert.equal(ctx.$.doc('knowledge_runbooks', 'rb-1').prompt, 'Schritt 1');
  const rec = ctx.$.doc('knowledge_tasks', taskId);
  assert.equal(rec.status, 'completed');
  assert.deepEqual(rec.result.item_ids, ['skill-a']);
  assert.equal(ctx.$.transactions.length, 1, 'Anwenden in einer Transaktion');
  assert.deepEqual(await sim.complete(taskId, {}), { delivered: false, reason: 'duplicate' });
  const writes = ctx.$.writes.length;
  const again = await taskComplete(d.command, sim.attach(ctx, d.command));
  assert.equal(again.duplicate, true);
  assert.equal(ctx.$.writes.length, writes);
});

test('ungültiges Writeback → nichts angewendet, Auftrag failed (Zone)', async () => {
  for (const ws of [
    { 'knowledge/writeback.json': { text: 'kein json' } },
    { 'knowledge/writeback.json': writeback({ items: [{ id: 'z', kind: 'unbekannt', title: 'Z' }] }) },
    { 'knowledge/writeback.json': writeback({ schema_version: 'v0' }) },
    { 'anderes.json': { text: '{}' } },
  ]) {
    const { ctx, sim, taskId } = await started();
    const d = await sim.complete(taskId, {}, { workspace: ws });
    assert.equal(d.result.status, 'failed');
    assert.equal(ctx.$.doc('knowledge_items', 'skill-a').title, 'Alt');
    assert.equal(ctx.$.doc('knowledge_runbooks', 'rb-1'), null);
    const rec = ctx.$.doc('knowledge_tasks', taskId);
    assert.equal(rec.status, 'failed');
    assert.equal(rec.error.code, 'invalid_result');
  }
});

test('failed / cancelled → knowledge_tasks mit Fehler; Doppelzustellung ohne Wirkung', async () => {
  const { ctx, sim, taskId } = await started('ctox.knowledge.chat');
  const d = await sim.fail(taskId, { code: 'quota', message: 'Kontingent erschöpft' });
  assert.equal(d.command.type, 'knowledge.task.failed');
  const rec = ctx.$.doc('knowledge_tasks', taskId);
  assert.equal(rec.status, 'failed');
  assert.equal(rec.error.message, 'Kontingent erschöpft');
  const writes = ctx.$.writes.length;
  assert.equal((await taskFailed(d.command, sim.attach(ctx, d.command))).duplicate, true);
  assert.equal(ctx.$.writes.length, writes);
  const w2 = await started('ctox.knowledge.book.export');
  const cb = { id: 'cb', type: 'knowledge.task.failed', payload: { task_id: w2.taskId, status: 'cancelled', result: null } };
  await taskFailed(cb, w2.sim.attach(w2.ctx, cb));
  assert.equal(w2.ctx.$.doc('knowledge_tasks', w2.taskId).status, 'cancelled');
});
