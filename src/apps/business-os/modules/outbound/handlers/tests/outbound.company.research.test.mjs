// Strang OUT3: Recherche als agent.task + Rückruf; Ergebnis-Gleichheit mit dem bisherigen (Rust-)Fall.
import test from 'node:test';
import assert from 'node:assert/strict';
import { cmd, rejects } from './_ctx.mjs';
import { researchCtx, runResearch } from './_research.mjs';
import handle from '../outbound.company.research.js';
import done from '../outbound.company.research.done.js';

const COMMAND = () => cmd('outbound.company.research', {
  title: 'Unternehmensdaten recherchieren: Acme',
  company: { id: 'co1', website: 'https://www.Acme.de/x', company_data: { keep: 1 } },
  campaign: { id: 'c1', name: 'K' },
  research_request: { fields: [{ id: 'city' }, 'domain', 'city'] },
}, { id: 'r1' });

const RESEARCH = { fields: {
  firma_ort: { value: ' Berlin ', candidates: [{ value: 'Berlin', source_url: 'https://a' }] },
  firma_domain: { value: 'acme.de', source_url: 'https://b' },
} };

test('researches company via agent task and appends knowledge row in the callback (same row as Rust)', async () => {
  const { t, log, tasks } = researchCtx({ now: 44 });
  const { accepted, outcome } = await runResearch(handle, done, t, COMMAND(), RESEARCH);
  assert.equal(accepted.status, 'accepted');
  const task = tasks.get(accepted.task_id);
  assert.deepEqual(task.payload.research_request, { company: 'Acme', country: 'DE', mode: 'new_record', fields: ['firma_ort', 'firma_domain'] });
  assert.equal(task.on_complete, 'outbound.company.research.done');
  assert.equal(task.on_failed, 'outbound.company.research.done');
  assert.equal(task.idempotency_key, 'outbound.company.research:r1');
  assert.equal(task.title, 'Unternehmensdaten recherchieren: Acme');
  assert.match(task.instruction, /Felder: firma_ort, firma_domain/);
  assert.deepEqual(outcome.record_ids, ['co1']);
  assert.equal(outcome.task_id, accepted.task_id);
  const append = log.find((a) => a[1] === 'append' && a[5] === 'campaign_c1_companies');
  const row = JSON.parse(append[append.indexOf('--rows') + 1])[0];
  assert.equal(row.city, 'Berlin');
  assert.equal(row.domain, 'acme.de');
  assert.equal(row.fit_score, 80);
  assert.equal(row.pipeline_status, 'not_started');
  const data = JSON.parse(row.company_data_json);
  assert.equal(data.keep, 1);
  assert.equal(data.firma_ort, 'Berlin');
  const evidence = JSON.parse(row.evidence_json);
  assert.deepEqual(evidence.map((e) => e.field), ['firma_ort', 'firma_domain']);
  assert.equal(evidence[1].source_url, 'https://b');
  assert.equal(t.calls.caps.some((c) => c.name === 'research.person'), false);
});

test('agent answer as text with embedded JSON is accepted; no JSON → invalid, no row', async () => {
  const a = researchCtx();
  const { outcome } = await runResearch(handle, done, a.t, COMMAND(), { text: `Ergebnis:\n${JSON.stringify(RESEARCH)}\nEnde` });
  assert.equal(outcome.records_count, 1);
  const b = researchCtx();
  const accepted = await handle(COMMAND(), b.t.ctx);
  const error = await rejects(done(cmd('outbound.company.research.done', { task_id: accepted.task_id, status: 'completed', result: 'nichts gefunden' }), b.t.ctx));
  assert.equal(error.code, 'invalid');
  assert.equal(b.log.some((a2) => a2[1] === 'append' && a2[5] === 'campaign_c1_companies'), false);
});

test('non-German without domain scores 50; guards before any task', async () => {
  const { t, log, tasks } = researchCtx();
  await runResearch(handle, done, t, cmd('outbound.company.research', { company: { id: 'x', name: 'Y', country: 'AT' }, campaign: { id: 'c' } }), { fields: {} });
  const append = log.find((a) => a[1] === 'append' && a[5] === 'campaign_c_companies');
  assert.equal(JSON.parse(append[append.indexOf('--rows') + 1])[0].fit_score, 50);
  const before = tasks.size;
  assert.equal((await rejects(handle(cmd('outbound.company.research', { company: { name: 'Y' }, campaign: { id: 'c' } }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.company.research', { company: { id: 'x' } }), t.ctx))).code, 'invalid');
  assert.equal(tasks.size, before, 'kein Auftrag bei ungültiger Eingabe');
});
