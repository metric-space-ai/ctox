import assert from 'node:assert/strict';
import test from 'node:test';
import handle from '../kundenpipeline.decision.answer.js';
import request from '../kundenpipeline.decision.request.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup, vorgang } from './_fixtures.mjs';

const decision = (overrides = {}) => ({
  id: 'kpl-e-1', vorgang_id: 'kpl-v-1', typ: 'mailfreigabe', status: 'offen', antwort_json: {}, updated_at_ms: NOW - 5, ...overrides,
});

test('annehmen einer Mailfreigabe versendet genau einmal und auditiert', async () => {
  const sent = [];
  const t = await setup({
    seed: {
      kundenpipeline_vorgaenge: [vorgang({ triage_json: { antwort_vorschlag: 'Gern.' } })],
      kundenpipeline_entscheidungen: [decision()],
    },
    caps: { 'mail.send': async (req) => (sent.push(req), { message_id: 'm' }) },
  });
  const r = await handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: 'kpl-e-1', wert: 'annehmen', kanal: 'brille' }), t.ctx);
  assert.equal(r.status, 'entschieden');
  assert.deepEqual(r.folge, { ok: true, queued: true, to: 'kunde@acme.example', subject: 'Re: Export anpassen' });
  assert.equal(sent.length, 1);
  const d = t.doc('kundenpipeline_entscheidungen', 'kpl-e-1');
  assert.deepEqual(d.antwort_json, { wert: 'annehmen', kanal: 'brille', zeit_ms: NOW, akteur: 'user_owner' });
  const v = t.doc('kundenpipeline_vorgaenge', 'kpl-v-1');
  assert.deepEqual(v.audit_json.map((a) => a.aktion), ['mailfreigabe:annehmen', 'mail:bestaetigung:gesendet']);
});

test('Mailfreigabe bei ergebnisVorliegend sendet art=ergebnis; Folgefehler rollt Antwort nicht zurueck', async () => {
  const t = await setup({
    seed: {
      kundenpipeline_vorgaenge: [vorgang({ status: 'ergebnisVorliegend', run_json: { zusammenfassung: 'Fertig' } })],
      kundenpipeline_entscheidungen: [decision()],
    },
  });
  const r = await handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: 'kpl-e-1', wert: 'annehmen' }), t.ctx);
  assert.equal(r.status, 'entschieden');
  assert.match(r.folge.error, /mail.send not available/);
  assert.equal(t.doc('kundenpipeline_entscheidungen', 'kpl-e-1').status, 'entschieden');
});

test('Triage annehmen delegiert; ohne Aufgabe skipped; ablehnen ohne Folge', async () => {
  const tasks = [];
  const t = await setup({
    seed: {
      kundenpipeline_vorgaenge: [
        vorgang({ triage_json: { aufgabe: { beschreibung: 'Fix' } } }),
        vorgang({ id: 'kpl-v-2', triage_json: { aufgabe: { beschreibung: '  ' } } }),
      ],
      kundenpipeline_entscheidungen: [
        decision({ typ: 'triage' }),
        decision({ id: 'kpl-e-2', typ: 'triage', vorgang_id: 'kpl-v-2' }),
        decision({ id: 'kpl-e-3', typ: 'triage' }),
      ],
    },
    caps: { 'agent.task.enqueue': async (req) => (tasks.push(req), { task_id: 'q', thread_key: req.thread_key }) },
  });
  const a = await handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: 'kpl-e-1', wert: 'annehmen' }), t.ctx);
  assert.equal(a.folge.task_key, 'q');
  const b = await handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: 'kpl-e-2', wert: 'annehmen' }), t.ctx);
  assert.deepEqual(b.folge, { skipped: 'triage without a task description' });
  const c = await handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: 'kpl-e-3', wert: ' ablehnen ' }), t.ctx);
  assert.equal(c.status, 'abgelehnt');
  assert.equal(c.folge, null);
  assert.equal(tasks.length, 1);
});

test('agent_escalation wird ueber resolve beantwortet (wert = option_id)', async () => {
  const t = await setup();
  const r = await request(cmd('kundenpipeline.decision.request', {
    decision_key: 'k', title: 'T', question: 'Q?',
    options: [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }],
    source: { environment_id: 'e', thread_id: 't' }, correlation: { idempotency_key: 'k' },
  }), t.ctx);
  const answered = await handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: r.decision_id, wert: 'b', comment: 'ok' }), t.ctx);
  assert.equal(answered.status, 'resolved');
  assert.equal(answered.resolution.option_id, 'b');
  assert.equal(answered.resolution.comment, 'ok');
});

test('unbekannte Antwort und unbekannte Entscheidung', async () => {
  const t = await setup({ seed: { kundenpipeline_entscheidungen: [decision()] } });
  await rejects(handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: 'kpl-e-1', wert: 'vielleicht' }), t.ctx), 'invalid', /unknown decision answer/);
  await rejects(handle(cmd('kundenpipeline.decision.answer', { entscheidung_id: 'x', wert: 'annehmen' }), t.ctx), 'not_found');
});
