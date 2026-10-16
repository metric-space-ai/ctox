// Strang OUT3: Kontaktrecherche als agent.task + Rückruf; Ergebnis-Gleichheit mit dem bisherigen (Rust-)Fall.
import test from 'node:test';
import assert from 'node:assert/strict';
import { cmd, rejects } from './_ctx.mjs';
import { researchCtx, runResearch } from './_research.mjs';
import handle from '../outbound.pipeline.contact_research.js';
import done from '../outbound.pipeline.contact_research.done.js';
import { inferNameFromProfileUrl, contactsFromResearch } from '../_lib/research.js';

test('groups candidates by source url and writes GDPR-stamped rows (callback)', async () => {
  const research = { fields: {
    person_vorname: { candidates: [{ value: 'Anna', source_url: 'https://x/team' }] },
    person_nachname: { candidates: [{ value: 'Muster', source_url: 'https://x/team' }] },
    person_funktion: { candidates: [{ value: 'CEO', source_url: 'https://x/team' }] },
    person_linkedin: { candidates: [{ value: 'https://www.linkedin.com/in/bernd-beispiel-12345/', source_url: 'https://www.linkedin.com/in/bernd-beispiel-12345/' }] },
  } };
  const { t, log, tasks } = researchCtx({ now: 1000 });
  const { accepted, outcome: result } = await runResearch(handle, done, t,
    cmd('outbound.pipeline.contact_research', { pipeline_id: 'p1', company_id: 'co', title: 'Ansprechpartner recherchieren: Acme', campaign: { id: 'c' } }), research);
  const task = tasks.get(accepted.task_id);
  assert.equal(task.payload.research_request.mode, 'update_person');
  assert.equal(task.payload.research_request.country, 'DE');
  assert.equal(task.on_complete, 'outbound.pipeline.contact_research.done');
  assert.equal(result.records_count, 2);
  const append = log.find((a) => a[1] === 'append' && a[5] === 'campaign_c_contacts');
  const rows = JSON.parse(append[append.indexOf('--rows') + 1]);
  const names = rows.map((r) => r.contact_name).sort();
  assert.deepEqual(names, ['Anna Muster', 'Bernd Beispiel']);
  const anna = rows.find((r) => r.contact_name === 'Anna Muster');
  assert.equal(anna.role, 'CEO');
  assert.equal(anna.legal_basis, 'legitimate_interest');
  assert.equal(anna.expires_at_ms, 1000 + 180 * 24 * 60 * 60 * 1000);
  assert.match(anna.subject_key, /^subj_[0-9a-f]{16}$/);
  assert.match(anna.contact_id, /^contact_[0-9a-f]{16}$/);
  assert.equal(JSON.parse(anna.evidence_json).length, 3);
});

test('helpers and guards', async () => {
  assert.equal(inferNameFromProfileUrl('https://www.linkedin.com/in/max-mustermann-0815'), 'Max Mustermann');
  assert.equal(inferNameFromProfileUrl('not a url'), '');
  assert.deepEqual(contactsFromResearch('p', 'c', 'n', { fields: { person_email: { candidates: [{ value: 'a@b.de' }] } } }), []);
  const { t } = researchCtx();
  const empty = runResearch(handle, done, t, cmd('outbound.pipeline.contact_research', { pipeline_id: 'p', company_name: 'A', campaign: { id: 'c' } }), { fields: {} });
  const error = await rejects(empty);
  assert.equal(error.code, 'not_found');
  assert.equal(error.message, 'ctox contact research found no public contacts for A');
  assert.equal((await rejects(handle(cmd('outbound.pipeline.contact_research', { company_name: 'A', campaign: { id: 'c' } }), t.ctx))).code, 'invalid');
});
