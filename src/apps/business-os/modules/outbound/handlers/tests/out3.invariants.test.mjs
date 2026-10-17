// Strang OUT3 – Invarianten der verlagerten App-Logik, je mit Mutationsbeleg (tests/mutation-proof.mjs, OUT3_MUTATIONS).
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { researchCtx } from './_research.mjs';
import companyHandle from '../outbound.company.research.js';
import companyDone from '../outbound.company.research.done.js';
import contactHandle from '../outbound.pipeline.contact_research.js';
import contactDone from '../outbound.pipeline.contact_research.done.js';
import testHandle from '../outbound.research_source.test.js';
import sellify from '../outbound.sellify.lookup.js';
import { accountStates } from '../_lib/olg-registry.js';
import generate from '../outbound.research_source.generate_adapter.js';
import generateDone from '../outbound.research_source.generate_adapter.done.js';

/** Adapter mit gestelltem Generierungsauftrag; get liefert die Aufgabe zurück. */
async function generated({ registered = false } = {}) {
  const tasks = new Map();
  const t = createTestCtx({
    commandResult: ({ id }) => ({ id, status: 'accepted' }),
    caps: {
      'agent.task.enqueue': async (args) => { const id = `gen_${tasks.size + 1}`; tasks.set(id, JSON.parse(JSON.stringify(args))); return { task_id: id }; },
      'agent.task.get': async ({ task_id }) => (tasks.has(task_id) ? { task_id, payload: tasks.get(task_id).payload } : null),
    },
  });
  const run = (id) => generate(cmd('outbound.research_source.generate_adapter', { adapter_id: 'a', source_id: 's', target_key: 'k' }, { id }), t.ctx);
  await run('g1');
  const setRegistered = (value) => t.ctx.put('outbound_lead_generation_scrape_targets', { id: 'k', target_key: 'k', workspace_dir: 'w', script_registered: value });
  if (registered) await setRegistered(true);
  const done = (taskId, status, extra = {}) => generateDone(cmd('outbound.research_source.generate_adapter.done', { task_id: taskId, status, ...extra }, { id: `cb_${taskId}_${status}` }), t.ctx);
  return { t, run, done, setRegistered };
}

const companyCmd = () => cmd('outbound.company.research', { company: { id: 'co1', name: 'Acme' }, campaign: { id: 'c1' } }, { id: 'r1' });
const contactCmd = () => cmd('outbound.pipeline.contact_research', { pipeline_id: 'p1', company_name: 'Acme', campaign: { id: 'c1' } }, { id: 'r2' });
const appended = (log, table) => log.filter((a) => a[1] === 'append' && a[5] === table).map((a) => JSON.parse(a[a.indexOf('--rows') + 1]));

test('OUT3A Rückruf wirkt nur für eine Aufgabe des eigenen Befehlstyps (fremde Aufgabe → not_found, keine Zeile)', async () => {
  const { t, log } = researchCtx();
  const foreign = await contactHandle(contactCmd(), t.ctx);
  const error = await rejects(companyDone(cmd('outbound.company.research.done', { task_id: foreign.task_id, status: 'completed', result: { fields: {} } }), t.ctx));
  assert.equal(error.code, 'not_found');
  assert.equal(appended(log, 'campaign_c1_companies').length, 0);
  const own = await companyHandle(companyCmd(), t.ctx);
  const error2 = await rejects(contactDone(cmd('outbound.pipeline.contact_research.done', { task_id: own.task_id, status: 'completed', result: { fields: {} } }), t.ctx));
  assert.equal(error2.code, 'not_found');
  assert.equal(appended(log, 'campaign_c1_contacts').length, 0);
});

test('OUT3B gescheiterte Recherche-Aufgabe schreibt keine Firmen-/Kontaktzeile, nur Laufstatus failed', async () => {
  for (const [handle, done, command, table, runType] of [
    [companyHandle, companyDone, companyCmd(), 'campaign_c1_companies', 'company_research'],
    [contactHandle, contactDone, contactCmd(), 'campaign_c1_contacts', 'contact_research'],
  ]) {
    const { t, log } = researchCtx();
    const accepted = await handle(command, t.ctx);
    const out = await done(cmd(`${command.type}.done`, { task_id: accepted.task_id, status: 'failed', result: { error: 'Browser nicht erreichbar' } }), t.ctx);
    assert.deepEqual(out, { ok: false, task_id: accepted.task_id, status: 'failed', error: 'Browser nicht erreichbar' });
    assert.equal(appended(log, table).length, 0, table);
    const runs = appended(log, 'campaign_c1_research_runs').flat();
    assert.deepEqual(runs.map((r) => [r.run_type, r.status, r.command_id]), [[runType, 'failed', command.id]]);
  }
});

test('OUT3C Weitergabe an die Eigentümer-App: Ablehnung ist ein Fehler, nie ein stilles ok', async () => {
  const t = createTestCtx({ commandResult: ({ id }) => ({ id, status: 'rejected', reason: 'forbidden' }) });
  const error = await rejects(testHandle(cmd('outbound.research_source.test', { source_id: 's' }), t.ctx));
  assert.equal(error.code, 'unavailable');
});

test('OUT3D Wiederholung desselben Befehls erzeugt dieselbe OLG-Befehls-Id (kein zweiter Testlauf)', async () => {
  const t = createTestCtx({ commandResult: ({ id }) => ({ id, status: 'accepted' }) });
  const a = await testHandle(cmd('outbound.research_source.test', {}, { id: 'cmd-x' }), t.ctx);
  const b = await testHandle(cmd('outbound.research_source.test', {}, { id: 'cmd-x' }), t.ctx);
  assert.equal(a.command_id, b.command_id);
  assert.equal(t.calls.commands[0].options.id, t.calls.commands[1].options.id);
});

test('OUT3E Registry-Lesesicht: probe_in_progress nur bei nicht abgelaufener Lease (INV-SCR4 der Eigentümer-App)', async () => {
  const now = 5_000;
  const t = createTestCtx({ now, data: { outbound_lead_generation_scrape_account_states: {
    a: { id: 'a', target_id: 'ta', probe_lease_owner: 'cmd', probe_lease_until_ms: now },
    b: { id: 'b', target_id: 'tb', probe_lease_owner: 'cmd', probe_lease_until_ms: now + 1 },
  } } });
  const states = await accountStates(t.ctx);
  assert.equal(states.ta.probe_in_progress, false);
  assert.equal(states.tb.probe_in_progress, true);
});

test('OUT3F Sellify-Kampagnengruppen schließen gelöschte Kampagnen aus (Rust :1547)', async () => {
  const t = createTestCtx({ data: { sellify_campaigns: {
    m1: { id: 'm1', name: 'Herbst', contact_id: 1 },
    m2: { id: 'm2', name: 'Herbst', contact_id: 2, is_deleted: true },
  } } });
  const r = await sellify(cmd('outbound.sellify.lookup', { entity: 'campaign', group_by: 'name', selectors: [{ field: 'name', value: 'herbst' }] }), t.ctx);
  assert.deepEqual(r.groups, [{ name: 'Herbst', count: 1 }]);
  assert.equal(t.calls.writes.length, 0, 'read-only');
});

test('OUT3G Generierungs-Rückruf: completed + registriertes Skript → adapter_ready; Fehlschlag → adapter_failed', async () => {
  const ok = await generated({ registered: true });
  const r = await ok.done('gen_1', 'completed');
  assert.deepEqual([r.status, r.scrape_status, r.outcome], ['adapter_ready', 'registered', 'registered']);
  assert.equal(ok.t.doc('outbound_research_adapters', 'a').payload.generation_task_outcome.task_id, 'gen_1');
  const none = await generated();
  const n = await none.done('gen_1', 'completed');
  assert.deepEqual([n.status, n.scrape_status], ['adapter_failed', 'generation_incomplete'], 'ohne registriertes Skript kein ready');
  const bad = await generated();
  const f = await bad.done('gen_1', 'failed', { result: { error: 'Portal gesperrt' } });
  assert.deepEqual([f.status, f.scrape_status], ['adapter_failed', 'generation_failed']);
  assert.equal(bad.t.doc('outbound_research_adapters', 'a').last_error, 'Portal gesperrt');
});

test('OUT3H Generierungs-Rückruf idempotent je task_id: zweiter Rückruf derselben Aufgabe schreibt nichts', async () => {
  const g = await generated({ registered: true });
  await g.done('gen_1', 'completed');
  const writes = g.t.calls.writes.length;
  const again = await g.done('gen_1', 'failed', { result: { error: 'spät' } });
  assert.equal(again.duplicate, true);
  assert.equal(g.t.calls.writes.length, writes);
  assert.equal(g.t.doc('outbound_research_adapters', 'a').status, 'adapter_ready');
});

test('OUT3I Rückruf eines älteren Auftrags überschreibt den Status des neueren nicht', async () => {
  const g = await generated();
  await g.run('g2'); // neuer Auftrag gen_2 ist jetzt der aktuelle
  const writes = g.t.calls.writes.length;
  const old = await g.done('gen_1', 'failed', { result: { error: 'alt' } });
  assert.equal(old.stale, true);
  assert.equal(g.t.calls.writes.length, writes);
  assert.equal(g.t.doc('outbound_research_adapters', 'a').status, 'generation_queued');
});
