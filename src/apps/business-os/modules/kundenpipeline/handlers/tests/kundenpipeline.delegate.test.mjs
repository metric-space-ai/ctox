import assert from 'node:assert/strict';
import test from 'node:test';
import handle from '../kundenpipeline.delegate.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup, vorgang } from './_fixtures.mjs';

test('reiht einen Queue-Task im Code-Projekt ein und setzt run_json', async () => {
  const tasks = [];
  const t = await setup({
    seed: {
      kundenpipeline_vorgaenge: [vorgang({ triage_json: { aufgabe: { beschreibung: 'CSV anpassen' } } })],
      kundenpipeline_projekte: [{ id: 'proj-acme', name: 'ACME', code_projekt: '/srv/acme', updated_at_ms: NOW }],
    },
    caps: { 'agent.task.enqueue': async (req) => (tasks.push(req), { task_id: 'queue:42', thread_key: req.thread_key }) },
  });
  const r = await handle(cmd('kundenpipeline.delegate', { vorgang_id: 'kpl-v-1' }), t.ctx);
  assert.deepEqual(r, { ok: true, task_key: 'queue:42', thread_key: 'kundenpipeline/auftrag/kpl-v-1' });
  assert.equal(tasks[0].title, 'Kundenauftrag: Bitte Export anpassen');
  assert.equal(tasks[0].payload.workspace_root, '/srv/acme');
  assert.equal(tasks[0].idempotency_key, 'kpl-auftrag-kpl-v-1');
  assert.deepEqual(Object.keys(tasks[0]).sort(), ['idempotency_key', 'instruction', 'on_complete', 'on_failed', 'payload', 'priority', 'thread_key', 'title']);
  assert.equal(tasks[0].on_complete, 'kundenpipeline.auftrag.complete');
  assert.equal(tasks[0].on_failed, 'kundenpipeline.auftrag.failed');
  assert.match(tasks[0].instruction, /AUFGABE:\nCSV anpassen/);
  assert.doesNotMatch(tasks[0].instruction, /ctox business-os|kundenpipeline\.triage\.write|triage":\{\}/, 'kein CLI-Rückschrieb, kein leeres triage:{}');
  assert.deepEqual(tasks[0].payload, { source: 'decision-hub-delegate', vorgang_id: 'kpl-v-1', workspace_root: '/srv/acme' });
  const v = t.doc('kundenpipeline_vorgaenge', 'kpl-v-1');
  assert.equal(v.status, 'inArbeit');
  assert.deepEqual(v.run_json, { task_key: 'queue:42', thread_key: 'kundenpipeline/auftrag/kpl-v-1', workspace_root: '/srv/acme', gestartet_ms: NOW });
  assert.equal(v.audit_json.at(-1).aktion, 'auftrag:delegiert');
});

test('payload.beschreibung hat Vorrang; ohne Projekt workspace_root null; ohne Text invalid', async () => {
  const tasks = [];
  const t = await setup({
    seed: { kundenpipeline_vorgaenge: [vorgang({ kunde_id: '' })] },
    caps: { 'agent.task.enqueue': async (req) => (tasks.push(req), { task_id: 'q1' }) },
  });
  await rejects(handle(cmd('kundenpipeline.delegate', { vorgang_id: 'kpl-v-1' }), t.ctx), 'invalid', /no task description/);
  await handle(cmd('kundenpipeline.delegate', { vorgang_id: 'kpl-v-1', beschreibung: 'Direkt' }), t.ctx);
  assert.equal(tasks[0].payload.workspace_root, null);
  assert.match(tasks[0].instruction, /AUFGABE:\nDirekt/);
});
