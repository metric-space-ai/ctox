// Strang SKL-C: Agenten-Starts der App im Zielbild (Kern §16.30/§16.34) mit der agent-task-Simulation:
// Knopf→Befehl→enqueue (Paket-Skill + Template), complete→Ergebnis in den eigenen Collections, fail/cancel, Doppelzustellung.
// Testnamen beginnen mit der Invariante (Mutationsbeleg tests/mutation-proof-scr.mjs, Gruppe skl-research).
import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { createTestCtx, cmd } from './_ctx.mjs';
import leadResearch from '../outbound.lead.research.js';
import campaignResearch from '../outbound.campaign.research.js';
import researchFailed from '../outbound.lead.research.failed.js';
import researchDone from '../outbound.lead.research.done.js';
import remarksCheck from '../outbound.sellify.remarks.check.js';
import remarksComplete from '../outbound.sellify.remarks.complete.js';
import generationDone from '../outbound.research_source.generation.done.js';

// Business-OS-Wurzel: im Mutationsbeleg liegt die Kopie im Temp-Ordner, dann setzt runProof CTOX_BUSINESS_OS_ROOT.
const BOS = process.env.CTOX_BUSINESS_OS_ROOT || fileURLToPath(new URL('../../../../', import.meta.url));
const { createAgentTaskSim } = await import(new URL('v4/tools/agent-sim/agent-task-sim.mjs', `file://${BOS.endsWith('/') ? BOS : `${BOS}/`}`).href);
const APP = 'outbound-lead-generation';
const APP_DIR = fileURLToPath(new URL('../../', import.meta.url));
const LEADS = 'outbound_lead_generation_leads';
const INSTRUCTIONS = 'SCHRITT 0: Sellify lesen.\nSCHRITT 1: Register.';

function setup({ leads = { lead_1: { name: 'ACME GmbH' } }, data = {} } = {}) {
  const fences = new Set();
  const issued = new Map();
  const rows = Object.fromEntries(Object.entries(leads).map(([id, l]) => [id, { id, data: {}, contacts: [], payload: {}, research_status: 'new', ...l }]));
  const t = createTestCtx({ data: { [LEADS]: rows, ...data } });
  // Plain cap-Objekt: die Simulation hängt agent.task an (sim.attach), die übrigen Fähigkeiten sind Testdoubles.
  t.ctx.cap = {
    fence: {
      claim: async ({ key }) => (fences.has(key) ? { token: null } : (fences.add(key), { token: key })),
      release: async ({ token }) => { fences.delete(token); },
    },
    // CAP-3b #844: command.get → immer Objekt mit `found` (Attrappe nach Vertrag).
    command: { get: async ({ command_id }) => (issued.has(command_id) ? { found: true, command_id, ...issued.get(command_id) } : { found: false, command_id }) },
    agent: {},
  };
  const sim = createAgentTaskSim({ appId: APP, appDir: APP_DIR, ctxFactory: () => t.ctx });
  sim.attach(t.ctx);
  const dispatch = async (handler, type, payload, extra = {}) => {
    const command = cmd(type, payload, { module: APP, ...extra });
    issued.set(command.id, { command_type: type, module: APP, record_id: command.record_id, payload: command.payload });
    sim.attach(t.ctx, command);
    return handler(command, t.ctx);
  };
  return { ...t, sim, dispatch, fences };
}

const researchPayload = (extra = {}) => ({
  command_id: 'ui_1', lead_name: 'ACME GmbH', company: 'ACME GmbH', country: 'DE', mode: 'update_firm',
  fields: ['firma_name', 'umsatz'], include_private: [], research_instructions: INSTRUCTIONS,
  research_instructions_default: INSTRUCTIONS, password: 'geheim', ...extra,
});

const REPLY = [
  'Recherche fertig: 1 Feld belegt, 1 ohne Treffer.',
  '```json',
  JSON.stringify({ field_status: {
    firma_name: { status: 'verified', value: 'ACME GmbH', sources: [{ source_id: 'nd', url: 'https://www.northdata.de/acme', quote: 'ACME GmbH, Hamburg' }] },
    umsatz: { status: 'no_match', reason: 'nicht veröffentlicht', attempts: [{ kind: 'web_search', query_or_url: 'q', at: 1 }] },
  } }),
  '```',
].join('\n');

test('INV-SKL1 Nachrecherche: Template + Paket-Skill + Callback; Rechercheablauf als eigener Block, nicht in der Vorschau', async () => {
  const t = setup();
  const r = await t.dispatch(leadResearch, 'outbound.lead.research', researchPayload(), { id: 'cmd_r1', record_id: 'lead_1' });
  const [task] = t.sim.enqueued;
  assert.equal(task.args.skill, 'outbound-lead-generation/research');
  assert.equal(task.args.on_complete, 'outbound.lead.research.done');
  assert.equal(task.args.on_failed, 'outbound.lead.research.failed');
  assert.equal(task.args.idempotency_key, 'outbound.lead.research:cmd_r1');
  const text = task.args.instruction;
  assert.ok(text.startsWith('Starte eine Outbound Nachrecherche für ACME GmbH [lead_1] (Auftrag ui_1).\n'), text.slice(0, 120));
  assert.ok(text.includes(`Rechercheablauf des Owners (research_instructions, wörtlich, unverändert, verbindlich):\n${INSTRUCTIONS}\n`));
  const preview = JSON.parse(text.slice(text.indexOf('Payload JSON:\n') + 'Payload JSON:\n'.length));
  assert.equal('research_instructions' in preview, false);
  assert.equal('research_instructions_default' in preview, false);
  assert.equal('password' in preview, false);
  assert.deepEqual(preview.stored_field_status, {});
  assert.doesNotMatch(text, /business_os\.execute_writeback|commands dispatch/);
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_status, 'running');
  assert.equal(lead.task_id, r.task_id);
  assert.equal(lead.command_id, 'ui_1');
  // Fortsetzung: eigenes Template, D&B-Pflicht nur ohne D&B-Beleg.
  const c = await t.dispatch(leadResearch, 'outbound.lead.research', researchPayload({ continuation: { open: ['umsatz'] }, fields: ['umsatz'], include_private: ['dnbhoovers.com'] }), { id: 'cmd_r2', record_id: 'lead_1' });
  const cont = t.sim.enqueued.find((e) => e.task_id === c.task_id).args.instruction;
  assert.ok(cont.startsWith('Setze die Outbound-Recherche für ACME GmbH [lead_1] fort (Auftrag ui_1): Lücken schließen, nur diese 1 Felder: umsatz.'));
  assert.match(cont, /Pflicht vor allem anderen: ctox_web_scrape mit target_key "dnbhoovers-com"/);
  await assert.rejects(() => t.dispatch(leadResearch, 'outbound.lead.research', researchPayload({ research_instructions: ' ' }), { id: 'cmd_r3', record_id: 'lead_1' }), /research_instructions is required/);
});

test('INV-SKL2 complete → Writeback in den Lead (an Task und Befehl gebunden); Doppelzustellung ohne Wirkung', async () => {
  const t = setup();
  const r = await t.dispatch(leadResearch, 'outbound.lead.research', researchPayload(), { id: 'cmd_r1', record_id: 'lead_1' });
  const d = await t.sim.complete(r.task_id, { text: REPLY });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.result.ok, true);
  assert.equal(d.result.research_command_id, 'cmd_r1');
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.data.firma_name, 'ACME GmbH');
  assert.equal(lead.research_status, 'needs_review');
  assert.equal(lead.field_status.firma_name.revision.writeback_id, `research-callback:${r.task_id}`);
  assert.equal(lead.field_status.firma_name.revision.command_id, 'cmd_r1');
  assert.equal(t.fences.size, 0);
  // Doppelzustellung: Simulation verwirft, der Handler selbst erkennt den angewandten Rückruf.
  assert.deepEqual(await t.sim.complete(r.task_id, { text: REPLY }), { delivered: false, reason: 'duplicate' });
  const before = JSON.stringify(t.doc(LEADS, 'lead_1'));
  const again = await researchDone(d.command, t.ctx);
  assert.equal(again.duplicate, true);
  assert.equal(JSON.stringify(t.doc(LEADS, 'lead_1')), before);
});

test('INV-SKL3 ohne JSON-Block bzw. Fehlschlag/Abbruch: Lead-Fehlerstand nur am aktuellen Task, sonst keine Wirkung', async () => {
  const t = setup({ leads: { lead_1: { name: 'ACME GmbH' }, lead_2: { name: 'Beta AG' } } });
  const a = await t.dispatch(leadResearch, 'outbound.lead.research', researchPayload(), { id: 'cmd_a', record_id: 'lead_1' });
  const none = await t.sim.complete(a.task_id, { text: 'Leider nichts gefunden.' });
  assert.equal(none.result.ok, false);
  assert.equal(t.doc(LEADS, 'lead_1').research_status, 'failed');
  const b = await t.dispatch(leadResearch, 'outbound.lead.research', researchPayload({ lead_name: 'Beta AG' }), { id: 'cmd_b', record_id: 'lead_2' });
  const failed = await t.sim.fail(b.task_id, 'quota exhausted');
  assert.equal(failed.result.research_status, 'failed');
  assert.equal(t.doc(LEADS, 'lead_2').research_error, 'quota exhausted');
  // Ein neuer Auftrag hat den Lead übernommen: ein später Abbruch des alten Tasks ändert nichts.
  const c = await t.dispatch(leadResearch, 'outbound.lead.research', researchPayload({ lead_name: 'Beta AG' }), { id: 'cmd_c', record_id: 'lead_2' });
  const stale = await researchFailed(cmd('outbound.lead.research.failed', { task_id: a.task_id, status: 'cancelled' }), t.ctx);
  assert.equal(stale.duplicate, true);
  const c2 = await t.dispatch(leadResearch, 'outbound.lead.research', researchPayload({ lead_name: 'Beta AG' }), { id: 'cmd_c2', record_id: 'lead_2' });
  const before = JSON.stringify(t.doc(LEADS, 'lead_2'));
  const superseded = await researchFailed(cmd('outbound.lead.research.failed', { task_id: c.task_id, status: 'cancelled' }), t.ctx);
  assert.equal(superseded.ignored, 'superseded');
  assert.equal(JSON.stringify(t.doc(LEADS, 'lead_2')), before);
  // Abbruch des aktuellen Tasks: Text „abgebrochen“.
  const cancel = await researchFailed(cmd('outbound.lead.research.failed', { task_id: c2.task_id, status: 'cancelled' }), t.ctx);
  assert.equal(cancel.research_status, 'failed');
  assert.equal(t.doc(LEADS, 'lead_2').research_error, 'Recherche abgebrochen.');
});

test('INV-SKL4 Kampagne: der Handler fächert je Lead auf; Doppelzustellung legt keine zweiten Tasks an', async () => {
  const t = setup({ leads: { lead_1: { name: 'ACME GmbH' }, lead_2: { name: 'Beta AG' } } });
  const shared = { fields: ['firma_name'], research_instructions: INSTRUCTIONS, include_private: [] };
  const leads = [{ lead_id: 'lead_1', payload: { lead_name: 'ACME GmbH', mode: 'new_record' } }, { lead_id: 'lead_2', payload: { lead_name: 'Beta AG', mode: 'update_firm' } }, { lead_id: 'lead_x' }];
  const r = await t.dispatch(campaignResearch, 'outbound.campaign.research', { campaign: 'Chemie', run_id: 'run_1', shared, leads }, { id: 'cmd_camp', record_id: 'campaign:Chemie' });
  assert.deepEqual(r.queued.map((q) => q.lead_id), ['lead_1', 'lead_2']);
  assert.deepEqual(r.failed, [{ lead_id: 'lead_x', error: 'Lead nicht gefunden.' }]);
  assert.equal(t.doc(LEADS, 'lead_1').research_status, 'queued');
  // Doppelzustellung des Kampagnenbefehls.
  await t.dispatch(campaignResearch, 'outbound.campaign.research', { campaign: 'Chemie', run_id: 'run_1', shared, leads }, { id: 'cmd_camp', record_id: 'campaign:Chemie' });
  const children = t.calls.commands.filter((c) => c.type === 'outbound.lead.research');
  assert.equal(children.length, 4);
  for (const child of children) {
    assert.equal(child.payload.research_instructions, INSTRUCTIONS);
    await t.dispatch(leadResearch, child.type, child.payload, { id: child.id, record_id: child.payload.lead_id });
  }
  const tasks = new Set(t.sim.enqueued.map((e) => e.task_id));
  assert.equal(tasks.size, 2, 'je Lead genau ein Task');
  assert.deepEqual(t.sim.enqueued.map((e) => e.args.idempotency_key).slice(0, 2), ['outbound.campaign.research:run_1:lead_1', 'outbound.campaign.research:run_1:lead_2']);
  assert.ok(t.sim.enqueued[0].args.instruction.startsWith('Starte eine Outbound Neurecherche für ACME GmbH [lead_1]'));
  assert.equal(t.doc(LEADS, 'lead_2').research_status, 'running');
});

test('INV-SKL5 Sellify-Vermerke: Urteil wird im Handler aus der Antwort gelesen; unvollständig → Fehler am Prüfvermerk', async () => {
  const t = setup({ leads: { lead_1: { name: 'ACME GmbH' }, lead_2: { name: 'Beta AG' } } });
  const entries = [
    { lead_id: 'lead_1', signatur: 's1', pruefpersonen: [7], lead_data: { lead_id: 'lead_1', firma: 'ACME GmbH', firmenvermerke: [{ feld: 'note', text: 'kein Mailing' }], personen: [] } },
    { lead_id: 'lead_2', signatur: 's2', pruefpersonen: [9], lead_data: { lead_id: 'lead_2', firma: 'Beta AG', firmenvermerke: [], personen: [] } },
  ];
  const r = await t.dispatch(remarksCheck, 'outbound.sellify.remarks.check', { entries }, { id: 'cmd_s' });
  const [task] = t.sim.enqueued;
  assert.equal(task.args.skill, undefined);
  assert.equal(task.args.on_complete, 'outbound.sellify.remarks.complete');
  assert.match(task.args.instruction, /^Werte die Freitextvermerke aus dem CRM Sellify semantisch aus/);
  assert.match(task.args.instruction, /"freitext_leads": \[/);
  assert.equal(t.doc(LEADS, 'lead_1').payload.sellify_freitext_pruefung.task_id, r.task_id);
  const answer = { leads: {
    lead_1: { firma: { status: 'gesperrt', grund: 'kein Mailing', zitat: 'kein Mailing' }, personen: { 7: { status: 'frei' } }, namen: {} },
    lead_2: { firma: { status: 'frei' }, personen: {}, namen: {} },
  } };
  const d = await t.sim.complete(r.task_id, { text: `Urteile:\n\`\`\`json\n${JSON.stringify(answer)}\n\`\`\`` });
  assert.equal(d.error, undefined, d.error?.message);
  const urteil = t.doc(LEADS, 'lead_1').payload.sellify_freitext_urteil;
  assert.equal(urteil.firma.status, 'gesperrt');
  assert.equal(urteil.signatur, 's1');
  assert.equal(urteil.task_id, r.task_id);
  // lead_2: Person 9 ohne Urteil → kein Urteil, Prüfvermerk mit Fehler.
  assert.equal(t.doc(LEADS, 'lead_2').payload.sellify_freitext_urteil, undefined);
  assert.equal(t.doc(LEADS, 'lead_2').payload.sellify_freitext_pruefung.fehler, 'Antwort ohne vollständiges Urteil');
  const before = JSON.stringify(t.doc(LEADS, 'lead_1'));
  const again = await remarksComplete(d.command, t.ctx);
  assert.equal(again.urteile, 0);
  assert.equal(JSON.stringify(t.doc(LEADS, 'lead_1')), before);
});

test('INV-SKL6 Lückenschluss-Rückruf: Writeback mit gap_task_id aus dem Task, Abbruch beendet die Lückenphase', async () => {
  const t = setup({ leads: { lead_1: { name: 'ACME GmbH', research_phase: 'gap_closure' } } });
  // Die Simulation kennt nur Tasks aus enqueue: Lückentask wie in person-research.js anlegen.
  const contract = { record_id: 'lead_1', research_command_id: 'cmd_pr', module: APP, requested_fields: ['firma_name'] };
  const { task_id: gapTask } = await t.ctx.cap.agent.task.enqueue({
    title: 'Lückenschluss: ACME GmbH', instruction: 'x', skill: 'outbound-lead-generation/research', idempotency_key: 'gap:cmd_pr',
    payload: { workspace_root_rel: 'runtime/research/person/cmd_pr', person_research_gap_closure: contract },
    on_complete: 'outbound.lead.research.done', on_failed: 'outbound.lead.research.failed',
  });
  await t.ctx.put(LEADS, { ...t.doc(LEADS, 'lead_1'), gap_task_id: gapTask });
  const d = await t.sim.fail(gapTask, { code: 'cancelled', message: 'abgebrochen' });
  assert.equal(d.result.research_status, 'failed');
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.research_phase, null);
  assert.equal(lead.payload.research_failed_task_id, gapTask);
});

test('INV-SKL7 Adapter-Erzeugung: Rückruf setzt adapter_ready nur mit registriertem Skript; zweiter Rückruf ohne Wirkung', async () => {
  const ADAPTERS = 'outbound_research_adapters';
  const TARGETS = 'outbound_lead_generation_scrape_targets';
  const t = setup({ data: {
    [ADAPTERS]: { a1: { id: 'a1', source_id: 'northdata.de', status: 'generation_queued', scrape_status: 'generation_queued' } },
    [TARGETS]: { 'northdata-de': { id: 'northdata-de', script_registered: false } },
  } });
  const { task_id: taskId } = await t.ctx.cap.agent.task.enqueue({
    title: 'Outbound Scraper Adapter erzeugen: Northdata', instruction: 'x', skill: 'universal-scraping', idempotency_key: 'gen:1',
    payload: { adapter_source_id: 'northdata.de', target_key: 'northdata-de' },
    on_complete: 'outbound.research_source.generation.done', on_failed: 'outbound.research_source.generation.done',
  });
  const d = await t.sim.complete(taskId, { text: 'fertig' });
  assert.equal(d.result.status, 'adapter_failed');
  assert.equal(t.doc(ADAPTERS, 'a1').last_error, 'Adaptererzeugung beendet ohne registriertes Skript');
  const again = await generationDone(d.command, t.ctx);
  assert.equal(again.updated, 0);
  await t.ctx.put(ADAPTERS, { id: 'a2', source_id: 'northdata.de', status: 'generation_queued' });
  await t.ctx.put(TARGETS, { id: 'northdata-de', script_registered: true });
  const ok = await generationDone(d.command, t.ctx);
  assert.equal(ok.status, 'adapter_ready');
  assert.equal(t.doc(ADAPTERS, 'a2').scrape_status, 'registered');
});
