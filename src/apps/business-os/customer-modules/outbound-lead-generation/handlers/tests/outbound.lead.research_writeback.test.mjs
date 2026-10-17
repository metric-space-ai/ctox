// Teilschritt 7: Handler outbound.lead.research_writeback (Chat- und Lückenweg, optimistisches Schreiben).
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.lead.research_writeback.js';

const LEADS = 'outbound_lead_generation_leads';
const MODULE = 'outbound-lead-generation';
const WITNESSES = 'outbound_lead_generation_field_status_witnesses';

function setup({ lead = {}, original, task = null, witnesses = [], files = {} } = {}) {
  const fences = new Set();
  const t = createTestCtx({
    data: {
      [LEADS]: { lead_1: { id: 'lead_1', data: {}, contacts: [], payload: {}, ...lead } },
      ...(witnesses.length ? { [WITNESSES]: { lead_1: { id: 'lead_1', record_id: 'lead_1', witnesses } } } : {}),
    },
    caps: {
      'fence.claim': async ({ key }) => (fences.has(key) ? { token: null } : (fences.add(key), { token: key })),
      'fence.release': async ({ token }) => { fences.delete(token); },
      // CAP-3b #844 Ergebnisform: immer ein Objekt mit `found` (Attrappe nach Vertrag, Stub 74137b1 kennt cap nicht).
      'command.get': async ({ command_id }) => (command_id === 'research_1'
        ? { found: true, command_id, status: 'completed', ...(original ?? { command_type: 'business_os.chat.task', module: MODULE, record_id: 'lead_1', payload: { fields: ['firma_name', 'umsatz'] } }) }
        : { found: false, command_id }),
      'agent.task.get': async (args) => (args.task_id ? task : null),
      'agent.task.find': async (args) => (args.command_id ? { attempt: 2 } : null),
      'agent.task.workspace.read': async ({ path }) => {
        if (!files[path]) throw new Error('missing');
        return files[path];
      },
    },
  });
  return { ...t, fences };
}

const writeback = (payload, extra = {}) => cmd('outbound.lead.research_writeback',
  { record_id: 'lead_1', module: MODULE, research_command_id: 'research_1', ...payload },
  { id: 'wb_1', module: MODULE, record_id: 'lead_1', ...extra });

test('chat writeback stores verified fields, reports open fields and stamps the revision', async () => {
  const t = setup();
  const r = await handle(writeback({ field_status: {
    firma_name: { status: 'verified', value: 'ACME GmbH', sources: [{ source_id: 'nd', url: 'https://www.northdata.de/acme', quote: 'ACME GmbH, Hamburg' }] },
    umsatz: { status: 'no_match', reason: 'nicht veröffentlicht', attempts: [{ kind: 'web_search', query_or_url: 'q', at: 1 }] },
  } }), t.ctx);
  assert.equal(r.ok, true);
  assert.equal(r.writeback_id, 'wb_1');
  assert.deepEqual(r.accepted_fields, ['firma_name', 'umsatz']);
  assert.deepEqual(r.open_fields, []);
  assert.deepEqual(r.delivered_fields, ['firma_name', 'umsatz']);
  assert.equal(r.research_status, 'needs_review'); // no_match on a requested field → review
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.data.firma_name, 'ACME GmbH');
  assert.deepEqual(lead.field_status.firma_name.revision, { writeback_id: 'wb_1', command_id: 'research_1', attempt: 2, written_at_ms: 1_700_000_000_000 });
  assert.equal(lead.field_status.firma_name.review, null);
  assert.equal(lead.payload.native_research_rejections_count, 0);
  assert.deepEqual(lead.payload.native_research_requested_fields, ['firma_name', 'umsatz']);
  assert.equal(lead.research_phase, null);
  // Witnesses: app collection, written in the same transaction as the lead (store.rs:12753).
  const issued = t.doc(WITNESSES, 'lead_1');
  assert.deepEqual(issued.witnesses.map((w) => [w.scope, w.person_key, w.field]), [['lead', '', 'firma_name'], ['lead', '', 'umsatz']]);
  assert.deepEqual(issued.witnesses[0].status, lead.field_status.firma_name);
  assert.equal(t.calls.caps.filter((c) => c.name.startsWith('outbound.')).length, 0);
  assert.equal(t.fences.size, 0);
  // OLG3: Kontaktprüfung als App-Befehl statt Core-Fähigkeit mail.validate_contacts.
  assert.deepEqual(t.calls.commands.map((c) => c.type), ['outbound.contacts.email_validate']);
  assert.equal(t.calls.caps.some((c) => c.name === 'mail.validate_contacts'), false);
});

test('a rejected source leaves the field open; undelivered fields are open, not rejected', async () => {
  const t = setup();
  const r = await handle(writeback({ field_status: {
    firma_name: { status: 'verified', value: 'ACME GmbH', sources: [{ source_id: 'ex', url: 'https://example.com/x', quote: 'ACME GmbH' }] },
  } }), t.ctx);
  assert.deepEqual(r.rejections, ['field_status.firma_name: Beleg vom Dokumentations-Host `example.com` beweist nichts']);
  assert.deepEqual(r.open_fields, ['firma_name', 'umsatz']);
  assert.equal(r.research_status, 'needs_review');
  assert.match(r.summary, /^0 Feld\(er\) gespeichert, 1 Beleg\(e\) verworfen, 2 Feld\(er\) noch offen\./);
});

test('guards: module, record, original command, fence, optimistic write', async () => {
  const t = setup();
  const fs = { firma_name: { status: 'verified', value: 'A', sources: [{ url: 'https://a.de', quote: 'A' }] } };
  assert.match((await rejects(handle(writeback({ field_status: fs }, { module: 'outbound' }), t.ctx))).message, /module must be outbound-lead-generation/);
  assert.match((await rejects(handle(writeback({ field_status: fs }, { record_id: 'x' }), t.ctx))).message, /record_id does not match command record_id/);
  t.fences.add('outbound-research:lead_1');
  assert.equal((await rejects(handle(writeback({ field_status: fs }), t.ctx))).code, 'conflict');
  t.fences.clear();
  const other = setup({ original: { command_type: 'business_os.chat.task', module: MODULE, record_id: 'lead_2', payload: {} } });
  assert.match((await rejects(handle(writeback({ field_status: fs }), other.ctx))).message, /same module and record_id/);
  const gap = setup();
  assert.match((await rejects(handle(writeback({ field_status: fs, gap_task_id: 'g1' }), gap.ctx))).message, /not in research_phase gap_closure/);
  // Lead changes between read and write → conflict, nothing stored.
  const race = setup();
  const realGet = race.ctx.get;
  let first = true;
  race.ctx.get = async (c, id) => {
    const doc = await realGet(c, id);
    if (first && c === LEADS) {
      first = false;
      await race.ctx.put(LEADS, { ...doc, research_status: 'running' });
    }
    return doc;
  };
  const e = await rejects(handle(writeback({ field_status: fs }), race.ctx));
  assert.equal(e.code, 'conflict');
  assert.equal(race.doc(LEADS, 'lead_1').field_status, undefined);
  assert.equal(race.fences.size, 0);
});

test('gap task writeback checks correlation, workspace and attempt artifacts', async () => {
  const contract = { record_id: 'lead_1', research_command_id: 'research_1', module: MODULE, gap_task_id: 'g1', requested_fields: ['umsatz'] };
  const task = { attempt: 1, metadata: { person_research_gap_closure: contract }, workspace_root_rel: 'runtime/research/person/research_1' };
  const attempt = (n, kind) => ({ kind, query_or_url: 'q', at: 1, artifact_path: `gap_closure/attempts/umsatz/${n}.json` });
  const files = Object.fromEntries([1, 2, 3].map((n) => [`gap_closure/attempts/umsatz/${n}.json`, { text: '{ }', sha256: `s${n}` }]));
  const t = setup({ lead: { research_phase: 'gap_closure', gap_task_id: 'g1' }, task, files, original: { command_type: 'web_stack.person_research', module: MODULE, record_id: 'lead_1', payload: {} } });
  const r = await handle(writeback({ gap_task_id: 'g1', field_status: { umsatz: { status: 'no_match', reason: 'keine Angabe',
    attempts: [attempt(1, 'web_search'), attempt(2, 'web_read'), attempt(3, 'browser_capture')] } } }), t.ctx);
  assert.deepEqual(r.accepted_fields, ['umsatz']);
  const lead = t.doc(LEADS, 'lead_1');
  assert.equal(lead.gap_task_id, 'g1');
  assert.equal(lead.field_status.umsatz.revision.attempt, 1);
  const bad = setup({ lead: { research_phase: 'gap_closure', gap_task_id: 'g1' }, task, files: {}, original: { command_type: 'web_stack.person_research', module: MODULE, record_id: 'lead_1', payload: {} } });
  assert.match((await rejects(handle(writeback({ gap_task_id: 'g1', field_status: { umsatz: { status: 'no_match', reason: 'x',
    attempts: [attempt(1, 'web_search')] } } }), bad.ctx))).message, /artifact_path `gap_closure\/attempts\/umsatz\/1.json` does not exist/);
});

test('INV-SCR2 witnesses: a changed status loses its witness, a reviewed status without issuance gets none (outbound_field_review.rs:218)', async () => {
  const stale = { status: 'verified', value: 'Alt', revision: { writeback_id: 'old', command_id: 'research_0', attempt: 1, written_at_ms: 1 } };
  const reviewed = { status: 'no_match', revision: { writeback_id: 'old', command_id: 'research_0', attempt: 1, written_at_ms: 1 }, review: { verdict: 'refuted' } };
  const t = setup({
    lead: { field_status: { firma_name: stale, umsatz: reviewed } },
    witnesses: [{ scope: 'lead', person_key: '', field: 'firma_name', status: stale }],
  });
  await handle(writeback({ field_status: {
    firma_name: { status: 'verified', value: 'ACME GmbH', sources: [{ source_id: 'nd', url: 'https://www.northdata.de/acme', quote: 'ACME GmbH, Hamburg' }] },
  } }), t.ctx);
  const lead = t.doc(LEADS, 'lead_1');
  const doc = t.doc(WITNESSES, 'lead_1');
  // firma_name: newly issued with the new revision (old witness replaced); umsatz: stamped but never issued → no witness.
  assert.deepEqual(doc.witnesses.map((w) => w.field), ['firma_name']);
  assert.equal(doc.witnesses[0].status.revision.writeback_id, 'wb_1');
  assert.deepEqual(doc.witnesses[0].status, lead.field_status.firma_name);
});

test('INV-SCR2 witness write is atomic with the lead: an ambiguous identity stores neither', async () => {
  const stamped = { status: 'verified', value: 'x', revision: { writeback_id: 'a', command_id: 'b', attempt: 1, written_at_ms: 1 } };
  const t = setup({ lead: { person_field_status: { ' p1 ': { telefon: stamped } } } });
  const e = await rejects(handle(writeback({ field_status: {
    firma_name: { status: 'verified', value: 'ACME GmbH', sources: [{ source_id: 'nd', url: 'https://www.northdata.de/acme', quote: 'ACME GmbH, Hamburg' }] },
  } }), t.ctx));
  assert.equal(e.message, 'ambiguous native field-status identity');
  assert.equal(t.doc(LEADS, 'lead_1').field_status, undefined);
  assert.equal(t.doc(WITNESSES, 'lead_1'), null);
});
