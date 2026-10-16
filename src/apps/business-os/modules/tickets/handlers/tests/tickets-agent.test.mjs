// Strang SKL-A1: Rückschrieb an zammad über agent.task mit App-Skill tickets/zammad-writeback und Prompt-Templates
// (Kern §16.30/§16.34), geprüft mit der agent-task-Simulation (v4/tools/agent-sim): Befehl → enqueue (Skill-Auflösung
// gegen Paket, Template wörtlich wie die frühere remoteInstruction), Callback complete → Ergebnis in den eigenen
// Collections, fail/cancel → Zeile failed/cancelled, Doppelzustellung ohne Wirkung.
import test from 'node:test';
import assert from 'node:assert/strict';
import { APP_DIR, createCtx, makeCommand, load } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';

const NOW = Date.parse('2026-10-15T10:00:00.000Z');
const ISO = '2026-10-15T10:00:00.000Z';
const SKILL = 'tickets/zammad-writeback';

const zammadTicket = { id: 'zammad:4711', ticket_key: 'zammad:4711', source_system: 'zammad', remote_ticket_id: '4711', title: 'Drucker', body_text: 'kaputt', remote_status: 'open', created_at: ISO, updated_at: ISO, metadata: {}, updated_at_ms: NOW, is_deleted: false };
const ticketCase = (state) => ({ id: 'case-1', case_id: 'case-1', ticket_key: 'zammad:4711', label: 'printer', bundle_label: 'printer-bundle', bundle_version: 1, state, approval_mode: 'human_approval_required', autonomy_level: 'A1', support_mode: 'support_case', risk_level: 'low', opened_at: ISO, updated_at: ISO, closed_at: null, updated_at_ms: NOW, is_deleted: false });
const passed = { id: 'v', verification_id: 'v', case_id: 'case-1', status: 'passed', created_at: ISO, updated_at_ms: NOW };

// Eine Welt: ctx mit Speicher + Audit-Protokoll; die Simulation hängt agent.task und asset an denselben ctx, und die
// Callback-Handler laufen gegen denselben Speicher (der Core stellt den Befehl an dieselbe App zu).
function world(state = 'writeback_pending') {
  const ctx = createCtx({ now: NOW, seed: { ctox_ticket_items: [zammadTicket], ctox_ticket_cases: [ticketCase(state)], ctox_ticket_verifications: [passed] } });
  const audits = [];
  ctx.cap = { agent: {}, audit: { event: async (event) => { audits.push(event); return { ok: true }; } } };
  const sim = createAgentTaskSim({ appId: 'tickets', appDir: APP_DIR, ctxFactory: () => ctx });
  const send = async (type, payload, id) => {
    const command = makeCommand(type, payload, { id });
    const { default: handle } = await load(type);
    return handle(command, sim.attach(ctx, command));
  };
  return { ctx, sim, audits, send, row: (id) => ctx.$.doc('ctox_ticket_writebacks', id) };
}

// Wörtliche Ausgabe der früheren remoteInstruction (_tickets.js @ b16b40a9e) als Referenz.
function legacyInstruction(request) {
  const head = 'Schreibe im Ticketsystem zammad auf Ticket 4711';
  if (request.operation === 'transition') {
    const note = request.note_body ? ` und hänge ${request.internal_note ? 'intern' : 'öffentlich'} diese Notiz an:\n${request.note_body}` : '';
    return `${head} den Status „${request.state}“${note}`;
  }
  return `${head} ${request.internal ? 'einen internen' : 'einen öffentlichen'} Kommentar:\n${request.body}`;
}

test('Knopf → Befehl → enqueue: Skill tickets/zammad-writeback aus dem Paket, Template wörtlich, Callbacks der App', async () => {
  const w = world();
  const out = await w.send('ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'Hallo {{x}}', internal: true }, 'w1');
  assert.equal(out.writeback_status, 'queued');
  const [e] = w.sim.enqueued;
  assert.equal(e.args.skill, SKILL);
  assert.deepEqual(e.skill, { skill: SKILL, scope: 'app', path: 'skills/zammad-writeback/SKILL.md' });
  assert.equal(e.args.instruction, legacyInstruction({ operation: 'comment', body: 'Hallo {{x}}', internal: true }));
  assert.equal(e.args.on_complete, 'ctox.ticket.writeback.complete');
  assert.equal(e.args.on_failed, 'ctox.ticket.writeback.failed');
  assert.equal(e.args.idempotency_key, `ticket-writeback:${out.writeback_id}`);
  assert.equal(w.row(out.writeback_id).task_id, e.task_id);

  for (const [i, request] of [
    { operation: 'transition', state: 'closed', note_body: 'Danke', internal_note: true },
    { operation: 'transition', state: 'closed', note_body: 'Danke', internal_note: false },
    { operation: 'transition', state: 'closed' },
  ].entries()) {
    const t = world();
    await t.send('ctox.ticket.writeback_transition', { case_id: 'case-1', state: request.state, ...(request.note_body ? { body: request.note_body, internal: request.internal_note } : {}) }, `t${i}`);
    assert.equal(t.sim.enqueued[0].args.instruction, legacyInstruction(request), `transition ${i}`);
    assert.equal(t.sim.enqueued[0].args.skill, SKILL);
  }
});

test('Rückfrage-Veröffentlichung nutzt dasselbe Template (öffentlicher Kommentar) und denselben Skill', async () => {
  const w = world('executable');
  const created = await w.send('ctox.ticket.request_clarification', { case_id: 'case-1', question: 'Welche Version?' }, 'rq');
  const published = await w.send('ctox.ticket.publish_clarification', { clarification_id: created.clarification_id }, 'p1');
  const [e] = w.sim.enqueued;
  assert.equal(e.task_id, published.task_id);
  assert.equal(e.args.instruction, legacyInstruction({ operation: 'comment', body: 'Welche Version?', internal: false }));
  assert.equal(e.args.skill, SKILL);
});

test('complete: Ergebnis des Skills (JSON im Antworttext) → Writeback ok mit Artikel-IDs, Fall geschlossen', async () => {
  const w = world();
  const tr = await w.send('ctox.ticket.writeback_transition', { case_id: 'case-1', state: 'closed', body: 'Danke' }, 't1');
  const d = await w.sim.complete(tr.task_id, { text: 'Status gesetzt, Notiz angelegt.\n```json\n{"remote_event_ids": [99]}\n```' });
  assert.equal(d.delivered, true);
  assert.equal(d.command.type, 'ctox.ticket.writeback.complete');
  assert.equal(d.result.status, 'ok');
  assert.equal(w.row(tr.writeback_id).status, 'ok');
  assert.deepEqual(w.row(tr.writeback_id).payload.remote_event_ids, ['99']);
  assert.equal(w.ctx.$.doc('ctox_ticket_cases', 'case-1').state, 'closed');
  assert.deepEqual(w.audits.map((a) => a.payload.action_type), ['writeback_record', 'case_closed']);
});

test('complete mit {error} ohne IDs → Rückschrieb failed (Antwortauswertung), Rückfrage send_failed', async () => {
  const w = world();
  const c = await w.send('ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'x' }, 'w1');
  const d = await w.sim.complete(c.task_id, { error: 'POST /api/v1/ticket_articles HTTP 401' });
  assert.equal(d.result.status, 'failed');
  assert.equal(w.row(c.writeback_id).status, 'failed');
  assert.equal(w.row(c.writeback_id).payload.error, 'POST /api/v1/ticket_articles HTTP 401');
  assert.equal(w.audits.at(-1).payload.action_type, 'writeback_failed');

  const q = world('executable');
  const created = await q.send('ctox.ticket.request_clarification', { case_id: 'case-1', question: 'V?' }, 'rq');
  const pub = await q.send('ctox.ticket.publish_clarification', { clarification_id: created.clarification_id }, 'p1');
  await q.sim.complete(pub.task_id, 'Zugang fehlt.\n{"error": "ZAMMAD_API_TOKEN fehlt"}');
  assert.equal(q.ctx.$.doc('ctox_ticket_clarification_requests', created.clarification_id).status, 'send_failed');
});

test('fail → failed (Fehlertext aus {code,message}); cancel → cancelled; Rückfrage bleibt wiederholbar', async () => {
  const w = world();
  const c = await w.send('ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'x' }, 'w1');
  const d = await w.sim.fail(c.task_id, 'HTTP 502');
  assert.equal(d.command.type, 'ctox.ticket.writeback.failed');
  assert.equal(w.row(c.writeback_id).status, 'failed');
  assert.equal(w.row(c.writeback_id).payload.error, 'HTTP 502');

  const k = world('executable');
  const created = await k.send('ctox.ticket.request_clarification', { case_id: 'case-1', question: 'V?' }, 'rq');
  const pub = await k.send('ctox.ticket.publish_clarification', { clarification_id: created.clarification_id }, 'p1');
  // Abbruch: der Core stellt on_failed mit status 'cancelled' zu (Festlegung 15.10.).
  const out = await k.send('ctox.ticket.writeback.failed', { task_id: pub.task_id, status: 'cancelled', result: null }, 'core-cancel');
  assert.equal(out.status, 'cancelled');
  const wb = k.ctx.$.all('ctox_ticket_writebacks').find((r) => r.task_id === pub.task_id);
  assert.equal(wb.status, 'cancelled');
  assert.equal(wb.payload.error, 'writeback task cancelled');
  assert.equal(k.ctx.$.doc('ctox_ticket_clarification_requests', created.clarification_id).status, 'send_failed');
  const retry = await k.send('ctox.ticket.publish_clarification', { clarification_id: created.clarification_id }, 'p2');
  assert.equal(retry.writeback_status, 'queued');
});

test('Doppelzustellung: Simulation verwirft die zweite, direkter zweiter Befehl ändert nichts', async () => {
  const w = world();
  const c = await w.send('ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'x' }, 'w1');
  await w.sim.complete(c.task_id, { remote_event_ids: ['7'] });
  assert.deepEqual(await w.sim.complete(c.task_id, { remote_event_ids: ['8'] }), { delivered: false, reason: 'duplicate' });
  assert.ok(w.sim.log.some((l) => l.event === 'callback_dropped' && l.task_id === c.task_id));
  const writes = w.ctx.$.writes.length;
  const audits = w.audits.length;
  const again = await w.send('ctox.ticket.writeback.failed', { task_id: c.task_id, status: 'failed', error: { code: 'failed', message: 'spät' } }, 'core-2');
  assert.equal(again.status, 'ignored');
  assert.equal(w.ctx.$.writes.length, writes);
  assert.equal(w.audits.length, audits);
  assert.deepEqual(w.row(c.writeback_id).payload.remote_event_ids, ['7']);
});
