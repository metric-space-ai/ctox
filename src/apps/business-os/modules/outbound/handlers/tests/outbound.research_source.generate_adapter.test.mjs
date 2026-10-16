import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import handle from '../outbound.research_source.generate_adapter.js';
import { researchScrapeTestInput, safePathComponent } from '../_lib/scrape-generation.js';

// Strang OUT3: Registrierung = foreign read der OLG-Ziele, Anlegen = ctx.command an OLG, Auftrag = agent.task.enqueue.
const OLG_TARGETS = 'outbound_lead_generation_scrape_targets';
const olgTarget = (key, extra = {}) => ({ [OLG_TARGETS]: { [key]: { id: key, target_key: key, workspace_dir: `outbound-lead-generation/scrape/targets/${key}`, script_registered: false, ...extra } } });

function scrapeCtx({ data = {}, taskStatus } = {}) {
  const tasks = [];
  const t = createTestCtx({
    data,
    commandResult: ({ id }) => ({ id, status: 'accepted' }),
    caps: { 'agent.task.enqueue': async (args) => { tasks.push(args); return taskStatus ? { task_id: 'task-1', task_status: taskStatus } : { task_id: 'task-1' }; } },
  });
  return { t, tasks };
}

test('new target: OLG upsert command and generation task via agent.task.enqueue (no core scrape capability)', async () => {
  const { t, tasks } = scrapeCtx();
  const result = await handle(cmd('outbound.research_source.generate_adapter', {
    adapter: { id: 'nd', label: 'North Data', target_key: 'northdata', countries: ['DE'] },
    test_input: { company: 'Acme', email: 'a@b.de' },
  }, { id: 'cmd-g' }), t.ctx);
  const doc = t.doc('outbound_research_adapters', 'nd');
  assert.equal(doc.status, 'generation_queued');
  assert.equal(doc.scrape_status, 'generation_queued');
  assert.equal(doc.test_ok, false);
  const effect = doc.payload.scrape_registry_effect;
  assert.equal(effect.registered_from, 'adapter_manifest');
  assert.equal(effect.target_manifest, 'outbound_lead_generation_scrape_targets/northdata');
  assert.equal(effect.generation_task.task_id, 'task-1');
  assert.equal(effect.generation_task.task_status, 'pending', 'neuer Auftrag ohne Status = pending');
  const [upsert] = t.calls.commands;
  assert.equal(upsert.type, 'outbound.scrape.target.upsert');
  assert.deepEqual(upsert.options, { id: 'cmd-g:scrape-target:northdata', module: 'outbound-lead-generation' });
  assert.equal(upsert.payload.manifest.target_kind, 'prospect-research');
  assert.deepEqual(upsert.payload.manifest.config.country_hints, ['DE']);
  const [task] = tasks;
  assert.equal(task.priority, 'low');
  assert.equal(task.skill, 'universal-scraping');
  assert.equal(task.idempotency_key, 'outbound-research-adapter:northdata:cmd-g');
  assert.equal(task.payload.workspace_root, 'outbound-lead-generation/scrape/targets/northdata');
  assert.equal(task.payload.adapter_id, 'nd');
  assert.equal(task.on_complete, 'outbound.research_source.generate_adapter.done');
  assert.equal(task.on_failed, 'outbound.research_source.generate_adapter.done');
  // swap_remove(task_id) moves email into its slot (serde preserve_order semantics)
  assert.match(task.instruction, /\{"company":"Acme","country":"DE","source_id":"nd","adapter_test":true,"email":"a@b.de","operation_timeout_ms":90000\}/);
  assert.match(task.instruction, /"timeout_seconds":120,/);
  assert.match(task.instruction, /outbound\.scrape\.script\.register/);
  assert.doesNotMatch(task.instruction, /ctox scrape/);
  assert.match(task.instruction, /^Erzeuge oder repariere einen CTOX Universal-Scraping Adapter fuer die Outbound Research Quelle\.\nZiel:\n- source_id: nd\n/);
  assert.equal(result.command_effect, 'adapter_requested');
  assert.deepEqual(t.calls.caps.map((c) => c.name), ['agent.task.enqueue']);
});

test('existing script -> adapter_ready/registered; missing target_key -> adapter_failed; blocked task', async () => {
  const ready = scrapeCtx({ data: olgTarget('k', { script_registered: true }) });
  await handle(cmd('outbound.research_source.generate_adapter', { adapter_id: 'a', source_id: 's', target_key: 'k' }), ready.t.ctx);
  assert.equal(ready.t.doc('outbound_research_adapters', 'a').status, 'adapter_ready');
  assert.equal(ready.t.doc('outbound_research_adapters', 'a').scrape_status, 'registered');
  assert.equal(ready.t.calls.commands.length, 0, 'registriertes Ziel: kein Upsert');
  assert.equal(ready.tasks.length, 0);

  const missing = scrapeCtx();
  await handle(cmd('outbound.research_source.generate_adapter', { adapter_id: 'a', source_id: 's' }), missing.t.ctx);
  const doc = missing.t.doc('outbound_research_adapters', 'a');
  assert.equal(doc.status, 'adapter_failed');
  assert.equal(doc.scrape_status, 'target_key_missing');

  const blocked = scrapeCtx({ data: olgTarget('k'), taskStatus: 'blocked' });
  await handle(cmd('outbound.research_source.generate_adapter', { adapter_id: 'a', source_id: 's', target_key: 'k' }), blocked.t.ctx);
  assert.equal(blocked.t.doc('outbound_research_adapters', 'a').status, 'generation_blocked');
  assert.equal(blocked.t.calls.commands.length, 0, 'vorhandenes Ziel ohne Skript: kein Upsert, nur Auftrag');
  assert.match(blocked.tasks[0].instruction, /Es fehlt ein gueltiger expliziter Testinput mit company/);
});

test('OLG rejects the target upsert -> registration_failed, no task', async () => {
  const tasks = [];
  const t = createTestCtx({
    commandResult: ({ id }) => ({ id, status: 'rejected', reason: 'invalid' }),
    caps: { 'agent.task.enqueue': async (a) => { tasks.push(a); return { task_id: 'x' }; } },
  });
  await handle(cmd('outbound.research_source.generate_adapter', { adapter_id: 'a', source_id: 's', target_key: 'k' }), t.ctx);
  const doc = t.doc('outbound_research_adapters', 'a');
  assert.equal(doc.status, 'adapter_failed');
  assert.equal(doc.scrape_status, 'registration_failed');
  assert.equal(doc.last_error, 'outbound-lead-generation rejected outbound.scrape.target.upsert: invalid');
  assert.equal(tasks.length, 0);
});

test('non scrape adapter kinds are skipped', async () => {
  const t = createTestCtx();
  await handle(cmd('outbound.research_source.generate_adapter', { adapter_id: 'a', source_id: 's', adapter_kind: 'manual' }), t.ctx);
  const doc = t.doc('outbound_research_adapters', 'a');
  assert.equal(doc.payload.scrape_registry_effect.reason, 'source_is_not_scrape_target');
  assert.equal(doc.status, 'adapter_requested');
  assert.equal(doc.scrape_status, 'registration_requested');
});

test('helpers', () => {
  assert.equal(safePathComponent('a b/ü'), 'a-b--');
  assert.equal(safePathComponent(''), 'adapter');
  const entries = researchScrapeTestInput(cmd('x', { company: 'A', test_input: { operation_timeout_ms: 5000 } }), { countries: ['AT'] }, 's');
  assert.deepEqual(Object.fromEntries(entries).country, 'AT');
  assert.throws(() => researchScrapeTestInput(cmd('x', { company: 'A', test_input: { operation_timeout_ms: 10 } }), {}, 's'));
});
