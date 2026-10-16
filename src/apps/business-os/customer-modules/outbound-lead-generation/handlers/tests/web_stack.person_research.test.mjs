// Handler web_stack.person_research als Agentenauftrag (Strang OLG4; vorher Teilschritt 8/OLG3 mit browser.capture):
// Knopf→Befehl→enqueue (Template prompts/person-research.md + Paket-Skill), complete→Auswertung (Sellify, Lückenaufgabe,
// Lead-Projektion), fail/cancel→Lead failed, Doppelzustellung ohne Wirkung, Fence je Lead bis zum Rückruf.
// Testnamen beginnen mit der Invariante (Mutationsbeleg tests/mutation-proof-scr.mjs, Gruppe web_stack.person_research).
import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../web_stack.person_research.js';
import done from '../web_stack.person_research.done.js';
import failed from '../web_stack.person_research.failed.js';
import { buildGapClosurePrompt, writebackRecordId, sellifyFuzzyCompanyProbe } from '../_lib/person-research.js';

const BOS = process.env.CTOX_BUSINESS_OS_ROOT || fileURLToPath(new URL('../../../../', import.meta.url));
const { createAgentTaskSim } = await import(new URL('v4/tools/agent-sim/agent-task-sim.mjs', `file://${BOS.endsWith('/') ? BOS : `${BOS}/`}`).href);
const APP = 'outbound-lead-generation';
const APP_DIR = fileURLToPath(new URL('../../', import.meta.url));
const LEADS = 'outbound_lead_generation_leads';
const contract = { collection: LEADS, record_ids: ['lead_1'] };
const INSTRUCTIONS = 'SCHRITT 0: Sellify lesen.\nSCHRITT 1: Register.';

function setup({ lead = {}, sellify = {} } = {}) {
  const fences = new Map();
  const t = createTestCtx({
    data: { sellify_companies: sellify, [LEADS]: { lead_1: { id: 'lead_1', company: 'ACME', data: {}, contacts: [], payload: { keep: 1 }, ...lead } } },
  });
  t.ctx.cap = {
    fence: {
      claim: async (args) => (fences.has(args.key) ? { token: null } : (fences.set(args.key, args), { token: args.key })),
      release: async ({ token }) => { fences.delete(token); },
    },
    agent: {},
  };
  const sim = createAgentTaskSim({ appId: APP, appDir: APP_DIR, ctxFactory: () => t.ctx });
  sim.attach(t.ctx);
  const start = async (command) => { sim.attach(t.ctx, command); return handle(command, t.ctx); };
  return { ...t, sim, start, fences };
}

const research = (payload = {}, extra = {}) => cmd('web_stack.person_research',
  { company: 'ACME GmbH', country: 'DE', mode: 'new_record', fields: ['firma_name', 'umsatz'], writeback_contract: contract,
    research_instructions: INSTRUCTIONS, ...payload },
  { id: 'cmd_r', module: APP, record_id: 'lead_1', ...extra });

const nameCandidate = { value: 'ACME GmbH', confidence: 'high', source_id: 'nd', source_url: 'https://northdata.de/a', quote: 'ACME GmbH', tier: 'S' };
const reply = (block) => ({ text: `Phase A fertig.\n\`\`\`json\n${JSON.stringify(block)}\n\`\`\`` });
const verifiedName = { firma_name: { value: 'ACME GmbH', confidence: 'high', source_id: 'nd', source_url: 'https://northdata.de/a', candidates: [nameCandidate] } };

test('INV-PRA1 Knopf→enqueue: Template person-research + Paket-Skill + eigene Rückrufe, keine Core-Erfassung', async () => {
  const t = setup();
  const r = await t.start(research());
  assert.deepEqual(r, { ok: true, status: 'running', summary: 'Recherche wurde gestartet.', worker_started: true, task_id: r.task_id });
  const [task] = t.sim.enqueued;
  assert.equal(t.sim.enqueued.length, 1);
  assert.equal(task.args.skill, 'outbound-lead-generation/person-research');
  assert.deepEqual(task.skill, { skill: 'outbound-lead-generation/person-research', scope: 'app', path: 'skills/person-research/SKILL.md' });
  assert.equal(task.args.on_complete, 'web_stack.person_research.done');
  assert.equal(task.args.on_failed, 'web_stack.person_research.failed');
  assert.equal(task.args.idempotency_key, 'person-research:cmd_r');
  assert.equal(task.args.title, 'Personenrecherche: ACME GmbH');
  const text = task.args.instruction;
  assert.ok(text.startsWith('# Personen-/Firmenrecherche Phase A\n\nFirma: ACME GmbH\nLand: DE\nModus: new_record\nLead/Record: lead_1\nRecherche-Kommando: cmd_r\n'), text.slice(0, 160));
  assert.match(text, /- `umsatz`: aktuellster belegbarer Umsatz mit Zeitraum und Einheit/);
  assert.ok(text.includes(`--- BEGIN OWNER INSTRUCTIONS ---\n${INSTRUCTIONS}\n--- END OWNER INSTRUCTIONS ---`));
  assert.doesNotMatch(text, /\{\{|ctox business-os commands dispatch/);
  assert.equal(task.args.payload.workspace_root_rel, 'runtime/research/person/cmd_r');
  assert.equal(task.args.payload.person_research_phase_a.command.id, 'cmd_r');
  assert.equal('client_context' in task.args.payload.person_research_phase_a.command, false);
  // Die Core-Recherchemaschine wird nicht mehr gerufen (vorher browser.capture research/sources).
  assert.equal(t.calls.caps.length, 0);
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_status, 'running');
  assert.equal(lead.task_id, r.task_id);
  assert.equal(lead.payload.person_research_task_id, r.task_id);
  assert.equal(lead.payload.keep, 1);
  // Unbekanntes Feld scheitert vor dem Auftrag (vorher erst in der Erfassung, :2212).
  const u = setup();
  await rejects(u.start(research({ fields: ['firma_name', 'lieblingsfarbe'] })));
  assert.equal(u.sim.enqueued.length, 0);
});

test('INV-PRA2 Fence je Lead hält vom Auftrag bis zum Rückruf (ttl), ein zweiter Start startet keinen Worker', async () => {
  const t = setup();
  const r = await t.start(research());
  assert.equal(t.fences.size, 1);
  assert.equal(t.fences.get('outbound-research:lead_1').ttl_ms, 3 * 60 * 60 * 1000);
  assert.deepEqual(await t.start(research({}, { id: 'cmd_r2' })), { ok: true, status: 'running', summary: 'Recherche wurde gestartet.', worker_started: false });
  assert.equal(t.sim.enqueued.length, 1);
  const d = await t.sim.complete(r.task_id, reply({ fields: verifiedName }));
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(t.fences.size, 0);
});

test('INV-PRA3 complete: alle Felder belegt → Lead completed, keine Lückenaufgabe; Doppelzustellung ohne Wirkung', async () => {
  const t = setup();
  const r = await t.start(research({ fields: ['firma_name'] }));
  const d = await t.sim.complete(r.task_id, reply({ fields: { ...verifiedName, fremd: { value: 'x' } }, search_runs: [{ source_id: 'nd', ok: true }] }));
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.result.gap_closure.required, false);
  assert.deepEqual(Object.keys(d.result.fields), ['firma_name']); // Plan der App bleibt Autorität (:1179)
  assert.equal(d.result.summary, 'Recherche für ACME GmbH abgeschlossen: 1 von 1 Feldern gefunden.');
  assert.equal(d.result.workspace_root, 'runtime/research/person/cmd_r');
  assert.equal(d.result.research_instructions_len, new TextEncoder().encode(INSTRUCTIONS).length);
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_status, 'completed');
  assert.equal(lead.data.firma_name, 'ACME GmbH');
  assert.equal(lead.payload.native_research_terminal_status, 'completed');
  assert.equal(lead.payload.person_research_callback_task_id, r.task_id);
  assert.equal(t.sim.enqueued.length, 1);
  assert.deepEqual(t.calls.commands.map((c) => [c.type, c.payload]), [['outbound.contacts.email_validate', { record_id: 'lead_1' }]]);
  // Doppelzustellung: die Simulation verwirft, der Handler selbst erkennt den verarbeiteten Rückruf.
  assert.deepEqual(await t.sim.complete(r.task_id, reply({ fields: verifiedName })), { delivered: false, reason: 'duplicate' });
  const before = JSON.stringify(t.doc(LEADS, 'lead_1'));
  const again = await done(d.command, t.ctx);
  assert.equal(again.duplicate, true);
  assert.equal(JSON.stringify(t.doc(LEADS, 'lead_1')), before);
  assert.equal(t.calls.commands.length, 1);
});

test('INV-PRA4 complete mit offenen Feldern: Lückenaufgabe mit Research-Skill; später Rückruf eines abgelösten Auftrags wirkungslos', async () => {
  const t = setup();
  const r = await t.start(research());
  const d = await t.sim.complete(r.task_id, reply({ fields: { ...verifiedName, umsatz: { value: null } }, search_runs: [{ q: 'acme' }] }));
  assert.equal(d.error, undefined, d.error?.message);
  const gap = t.sim.enqueued[1];
  assert.equal(gap.args.skill, 'outbound-lead-generation/research');
  assert.equal(gap.args.on_complete, 'outbound.lead.research.done');
  assert.equal(gap.args.idempotency_key, 'gap:cmd_r');
  assert.equal(t.sim.tasks.get(gap.task_id).metadata.person_research_gap_closure.writeback_contract.gap_task_id, gap.task_id);
  assert.deepEqual(t.sim.tasks.get(gap.task_id).metadata.person_research_gap_closure.attempted_sources, [{ q: 'acme', attempt_kind: 'web_search' }]);
  assert.match(t.sim.tasks.get(gap.task_id).instruction, /"gap_task_id": "task_outbound-lead-generation_2"/);
  assert.deepEqual(d.result.gap_closure.open_fields, ['umsatz']);
  assert.equal(d.result.gap_closure.workspace_root, 'runtime/research/person/cmd_r');
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_status, 'running');
  assert.equal(lead.research_phase, 'gap_closure');
  assert.equal(lead.gap_task_id, gap.task_id);
  // Neuer Auftrag: die offene Lückenaufgabe wird abgebrochen; eine späte Zustellung des alten Auftrags ändert nichts.
  const r2 = await t.start(research({}, { id: 'cmd_r3' }));
  assert.equal(r2.worker_started, true);
  assert.equal(t.sim.tasks.get(gap.task_id).status, 'cancelled');
  const before = JSON.stringify(t.doc(LEADS, 'lead_1'));
  const stale = await failed(cmd('web_stack.person_research.failed', { task_id: r.task_id, status: 'failed', error: 'spät' }), t.ctx);
  assert.equal(stale.duplicate, true);
  assert.equal(JSON.stringify(t.doc(LEADS, 'lead_1')), before);
  // Abgelöster Auftrag (Lead hängt an einem neueren Task): Rückruf ohne Wirkung, Fence trotzdem frei.
  const superseded = setup();
  const a = await superseded.start(research());
  await superseded.ctx.put(LEADS, { ...superseded.doc(LEADS, 'lead_1'), payload: { ...superseded.doc(LEADS, 'lead_1').payload, person_research_task_id: 'newer' } });
  const snapshot = JSON.stringify(superseded.doc(LEADS, 'lead_1'));
  const s = await superseded.sim.complete(a.task_id, reply({ fields: verifiedName }));
  assert.equal(s.result.ignored, 'superseded');
  assert.equal(JSON.stringify(superseded.doc(LEADS, 'lead_1')), snapshot);
  assert.equal(superseded.fences.size, 0);
});

test('INV-PRA5 Fehlschlag/Abbruch: Lead failed mit Grund, Fence frei; zweite Zustellung ohne Wirkung', async () => {
  const t = setup();
  const r = await t.start(research());
  const f = await t.sim.fail(r.task_id, 'quota exhausted');
  assert.equal(f.error, undefined, f.error?.message);
  assert.equal(f.result.research_status, 'failed');
  let lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_status, 'failed');
  assert.equal(lead.research_error, 'quota exhausted');
  assert.equal(lead.payload.native_research_terminal_status, 'failed');
  assert.equal(t.fences.size, 0);
  const before = JSON.stringify(lead);
  assert.equal((await failed(f.command, t.ctx)).duplicate, true);
  assert.equal(JSON.stringify(t.doc(LEADS, 'lead_1')), before);
  // Abbruch kommt als on_failed mit status 'cancelled'.
  const c = setup();
  const rc = await c.start(research());
  const x = await failed(cmd('web_stack.person_research.failed', { task_id: rc.task_id, status: 'cancelled' }), c.ctx);
  assert.equal(x.reason, 'Personenrecherche abgebrochen.');
  lead = c.doc(LEADS, 'lead_1');
  assert.equal(lead.research_error, 'Personenrecherche abgebrochen.');
  assert.equal(c.fences.size, 0);
});

test('INV-PRA6 Schlussantwort ohne JSON-Block: Lead failed, nichts gespeichert, keine Lückenaufgabe', async () => {
  const t = setup();
  const r = await t.start(research());
  const d = await t.sim.complete(r.task_id, { text: 'Leider nichts gefunden.' });
  assert.equal(d.result.ok, false);
  assert.equal(d.result.reason, 'no_result_block');
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_status, 'failed');
  assert.equal(lead.research_error, 'Die Personenrecherche endete ohne JSON-Ergebnisblock; nichts gespeichert.');
  assert.equal(lead.data.firma_name, undefined);
  assert.equal(t.sim.enqueued.length, 1);
  assert.equal(t.fences.size, 0);
});

test('INV-PRA7 have_data/update_inventory_general: kein Agent, leerer Plan (mode_skipped) direkt ausgewertet', async () => {
  const t = setup();
  const r = await t.start(research({ mode: 'have_data', fields: ['firma_name'] }));
  assert.equal(r.skipped_reason, 'mode_skipped');
  assert.equal(t.sim.enqueued.filter((e) => e.args.skill.endsWith('/person-research')).length, 0);
  assert.equal(t.sim.enqueued.length, 1); // nur die Lückenaufgabe (wie vorher bei leerem Plan)
  assert.equal(t.sim.enqueued[0].args.skill, 'outbound-lead-generation/research');
  assert.equal(t.fences.size, 0);
});

test('Prüffehler vor dem Auftrag: Lead failed, Fence frei, offene Lückenaufgabe abgebrochen', async () => {
  const t = setup({ lead: { research_phase: 'gap_closure', gap_task_id: 'old' } });
  t.ctx.cap.agent.task.get = async () => ({ route_status_terminal: false });
  t.ctx.cap.agent.task.cancel = async (args) => { t.calls.caps.push({ name: 'agent.task.cancel', args: [args] }); return {}; };
  const command = research({ country: 'FR' });
  const e = await rejects(handle(command, t.ctx));
  assert.match(e.message, /unsupported country `FR`; expected DE, AT or CH/);
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_status, 'failed');
  assert.equal(lead.research_error, 'unsupported country `FR`; expected DE, AT or CH');
  assert.equal(lead.payload.research_execution_phase, null);
  assert.match(t.calls.caps.find((c) => c.name === 'agent.task.cancel').args[0].status_note, /superseded this gap closure/);
  assert.equal(t.fences.size, 0);
  assert.equal(writebackRecordId({ module: APP, record_id: ' lead_1 ', payload: { writeback_contract: { allowed_collections: [LEADS], record_ids: ['lead_1'] } } }), 'lead_1');
  assert.equal(writebackRecordId({ module: 'outbound', record_id: 'lead_1', payload: { writeback_contract: contract } }), null);
  await assert.rejects(() => buildGapClosurePrompt(t.ctx, { company: 'A', record_id: 'r', research_command_id: 'c', module: 'm', open_fields: [] }), /writeback_contract is required/);
});

test('Sellify-Grundlinie im Complete-Rückruf (ref :2430, :2511, :2535)', async () => {
  assert.equal(sellifyFuzzyCompanyProbe('BNT Chemicals GmbH'), 'BNT Chemicals');
  assert.equal(sellifyFuzzyCompanyProbe('AB GmbH'), null);
  const t = setup({ sellify: { s1: { id: 's1', name: 'ACME GmbH', phone: '+49 40 1', contact_id: '77' } } });
  const r = await t.start(research({ fields: ['firma_name', 'firma_telefon'] }));
  const d = await t.sim.complete(r.task_id, reply({ plan: [], fields: { ...verifiedName, firma_telefon: { value: null, confidence: 'missing' } } }));
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.result.summary, 'Recherche für ACME GmbH abgeschlossen: 2 von 2 Feldern gefunden.'); // ref :2393
  const receipt = d.result.sellify_lookup_runs[0];
  assert.equal(receipt.classification, 'succeeded');
  assert.equal(receipt.selected_record_id, 's1');
  assert.equal(receipt.evidence_count, 2);
  assert.equal(receipt.country, 'DE');
  assert.equal(d.result.fields.firma_telefon.value, '+49 40 1');
  assert.equal(d.result.fields.firma_telefon.source_id, 'sellify');
  assert.equal(d.result.fields.firma_name.candidates.length, 2);
  assert.equal(d.result.plan[0].source_id, 'sellify');
});
