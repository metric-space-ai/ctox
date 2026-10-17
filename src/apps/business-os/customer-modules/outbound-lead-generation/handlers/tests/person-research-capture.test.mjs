// Personenrecherche Phase A, App-Anteil (Strang OLG3): Prüfungen, Texte und Reihenfolge aus person_research_command.rs
// execute :2181-2409 (Rust-Stand 4794af03d) und web-stack sources/mod.rs:60/:102. Seit OLG4 kein browser.capture mehr:
// Phase A ist ein Agentenauftrag; hier die Prüfungen VOR dem Auftrag, Template-Werte und die Normalisierung der Antwort.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import {
  countryFromIso, normalizePhaseAOutcome, parseResearchRequest, personResearchTemplateValues, researchModeFromStr, researchSummary,
  safeRuntimeSourceIdentifier, verifiedResearchOwnerUserId,
} from '../_lib/person-research-capture.js';
import handle from '../web_stack.person_research.js';

const LEADS = 'outbound_lead_generation_leads';
const base = { company: ' ACME GmbH ', country: 'de', mode: 'NEU' };

test('country, mode and owner parse like the Rust request (sources/mod.rs:60/:102, :2140)', () => {
  assert.deepEqual(['DE', 'deu', ' Deutschland ', 'at', 'ÖSTERREICH', 'österreich', 'che', 'fr'].map(countryFromIso),
    ['DE', 'DE', 'DE', 'AT', 'AT', null, 'CH', null]);
  assert.deepEqual(['A', 'update_inventory', 'b 1', 'B2', 'neu', 'x'].map(researchModeFromStr),
    ['have_data', 'update_inventory_general', 'update_person', 'update_firm', 'new_record', null]);
  assert.equal(verifiedResearchOwnerUserId({ actor: { id: ' u1 ' } }), 'u1');
  assert.equal(verifiedResearchOwnerUserId({ owner_user_id: ' ', native_authorization: { actor: { id: 'u2' } } }), 'u2');
  assert.equal(verifiedResearchOwnerUserId(null), null);
  assert.equal(safeRuntimeSourceIdentifier('xing.com', 160), true);
  assert.equal(safeRuntimeSourceIdentifier('x y', 160), false);
  assert.equal(safeRuntimeSourceIdentifier('a'.repeat(129), 128), false);
});

test('request checks keep the Rust error texts (:2193-2228)', () => {
  const msg = (p, c) => { try { parseResearchRequest(p, c); return null; } catch (e) { return e.message; } };
  assert.equal(msg({ company: 'A', country: 'DE' }), 'invalid web_stack.person_research payload');
  assert.equal(msg({ ...base, company: '  ' }), 'company is required');
  assert.equal(msg({ ...base, country: 'FR' }), 'unsupported country `FR`; expected DE, AT or CH');
  assert.equal(msg({ ...base, mode: 'x' }), 'unsupported research mode `x`; expected new_record, update_firm, update_person, update_inventory_general or have_data');
  assert.equal(msg({ ...base, auto_browser_capture: true }, {}), 'authenticated person-research capture requires a verified command owner');
  assert.equal(msg({ ...base, auto_browser_capture: true }, '{"actor":{"id":"u1"}}'), null);
  const r = parseResearchRequest({ ...base, research_instructions: 'Grüße' }, null);
  assert.deepEqual([r.company, r.country, r.mode, r.research_instructions_len], ['ACME GmbH', 'DE', 'new_record', 7]);
});

test('summary line (:2367-2401)', () => {
  const result = { requested_fields: ['a', 'b', 'c'], fields: { a: { value: 'x' }, b: { value: null }, c: {} }, browser_assist_tasks: [] };
  assert.equal(researchSummary('ACME', result), 'Recherche für ACME abgeschlossen: 1 von 3 Feldern gefunden.');
  result.browser_assist_tasks = [{}, {}];
  assert.equal(researchSummary('ACME', result), 'Recherche für ACME abgeschlossen: 1 von 3 Feldern gefunden. 2 Quelle(n) benötigen eine Browser-Autorisierung.');
});

function setup() {
  const fences = new Set();
  const t = createTestCtx({
    data: { [LEADS]: { lead_1: { id: 'lead_1', data: {}, contacts: [], payload: {} } } },
    caps: {
      'fence.claim': async ({ key }) => (fences.has(key) ? { token: null } : (fences.add(key), { token: key })),
      'fence.release': async ({ token }) => { fences.delete(token); },
      'agent.task.find': async () => null,
      'agent.task.enqueue': async () => ({ task_id: 't1' }),
    },
  });
  return { ...t, fences };
}
const research = (payload) => cmd('web_stack.person_research', payload, { id: 'cmd r/1', module: 'other' });

test('INV-PRC1 authenticated capture without a verified owner never reaches the agent task (:2224)', async () => {
  const t = setup();
  const e = await rejects(handle(research({ ...base, auto_browser_capture: true }), t.ctx));
  assert.equal(e.message, 'authenticated person-research capture requires a verified command owner');
  assert.equal(t.calls.caps.filter((c) => c.name === 'agent.task.enqueue').length, 0);
  assert.equal(t.fences.size, 0);
});

test('INV-PRC2 an invalid runtime source fails before the agent task (:2265, vorher erst vor der Quellen-Erfassung)', async () => {
  const t = setup();
  const e = await rejects(handle(research({ ...base, fields: ['firma_name'], source_policy: { sources: [{ id: 'bad id', url: 'https://x.de', target_key: 'x' }] } }), t.ctx));
  assert.equal(e.message, 'invalid runtime research source identifier');
  assert.equal(t.calls.caps.filter((c) => c.name === 'agent.task.enqueue').length, 0);
  assert.equal(t.fences.size, 0);
});

test('template values and answer normalization (plan authority :1179, missing fields :1059)', async () => {
  const command = research({ ...base, fields: ['firma_name', 'umsatz'], include_private: ['xing.com'], research_instructions: 'abc',
    source_policy: { sources: [{ id: 'nd', target_key: 'northdata-de' }] }, person_priorities: ['Finanzen'] });
  const request = parseResearchRequest(command.payload, null);
  const v = personResearchTemplateValues(command, request);
  assert.equal(v.record_id, '-');
  assert.equal(v.research_command_id, 'cmd r/1');
  assert.equal(v.field_lines.split('\n').length, 2);
  assert.match(v.field_lines, /^- `firma_name`: aktuelle rechtliche Firmierung$/m);
  assert.deepEqual(JSON.parse(v.include_private), ['xing.com']);
  assert.deepEqual(JSON.parse(v.runtime_sources), [{ id: 'nd', target_key: 'northdata-de' }]);
  assert.deepEqual(JSON.parse(v.person_priorities), ['Finanzen']);
  const out = normalizePhaseAOutcome({ fields: { firma_name: { value: 'ACME GmbH', candidates: [{ value: 'ACME GmbH' }, 'x'] }, fremd: { value: 1 } },
    search_runs: [{ q: 'a' }, 7], person_records: { item: [] } }, command, request);
  assert.deepEqual(out.requested_fields, ['firma_name', 'umsatz']);
  assert.deepEqual(Object.keys(out.fields), ['firma_name', 'umsatz']);
  assert.deepEqual(out.fields.umsatz, { value: null, confidence: 'missing', candidates: [] });
  assert.deepEqual(out.fields.firma_name.candidates, [{ value: 'ACME GmbH' }]);
  assert.deepEqual(out.search_runs, [{ q: 'a' }]);
  assert.deepEqual(out.person_records, []);
  assert.deepEqual(out.read_runs, []);
});
