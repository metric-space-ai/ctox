// Strang SKL-A1: „CTOX fragen“ als App-Automatisierung (Kern §16.30/§16.34), geprüft mit der agent-task-Simulation
// (v4/tools/agent-sim): Knopf → support.agent.task → enqueue (Skill support/support-workflow aus dem Paket,
// Template prompts/agent-task.md), complete → Vorschläge in support_agent_suggestions, fail/cancel → Auftrag
// failed/cancelled (UI meldet ihn in der Zone), Doppelzustellung ohne Wirkung.
import test from 'node:test';
import assert from 'node:assert/strict';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { command, loadHandler, makeCtx } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';
import { compactSnapshot, parseAgentResult } from '../_agent-task.js';

const APP_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const NOW = 1_760_000_000_000;
const SKILL = 'support/support-workflow';
const conv = { id: 'conv_1', status: 'pending', priority: 'high', inbox_id: 'inbox-mail', search_text: 'Acme Login', created_at_ms: NOW - 10, updated_at_ms: NOW - 10, is_deleted: false };

function world() {
  const env = makeCtx({ now: NOW, collections: { support_conversations: [conv] } });
  const ctx = env.ctx;
  ctx.cap = { agent: {} };
  const sim = createAgentTaskSim({ appId: 'support', appDir: APP_DIR, ctxFactory: () => ctx });
  const send = async (type, payload, id) => {
    const cmd = command(type, payload, { id, record_id: payload.conversation_id });
    return (await loadHandler(type))(cmd, sim.attach(ctx, cmd));
  };
  const request = () => env.docs('support_agent_requests')[0];
  return { env, ctx, sim, send, request };
}

const snapshot = { conversation: conv, recent_messages: [{ id: 'm1', body_text: 'Login geht nicht' }], notes: [], events: [], suggestions: [] };

test('Knopf → support.agent.task → enqueue: App-Skill aus dem Paket, Template gefüllt, Auftrag queued mit task_id', async () => {
  const w = world();
  const out = await w.send('support.agent.task', { conversation_id: 'conv_1', title: 'Support: Acme', status: 'open', priority: 'urgent', record_snapshot: snapshot }, 'cmd_ask_1');
  assert.equal(out.ok, true);
  const [e] = w.sim.enqueued;
  assert.equal(e.args.skill, SKILL);
  assert.deepEqual(e.skill, { skill: SKILL, scope: 'app', path: 'skills/support-workflow/SKILL.md' });
  assert.equal(e.args.on_complete, 'support.agent.task.complete');
  assert.equal(e.args.on_failed, 'support.agent.task.failed');
  assert.equal(e.args.thread_key, 'business-os/support/conv_1');
  assert.equal(e.args.idempotency_key, 'support-agent:cmd_ask_1');
  assert.equal(e.args.title, 'Support: Acme');
  // Zeilen 1–2 und 4–6 wörtlich aus buildAgentInstruction (index.js @ b16b40a9e), Status/Priorität aus den Bedienelementen.
  assert.equal(e.args.instruction, [
    'Analysiere diese CTOX Support-Konversation.',
    'Liefere eine knappe Zusammenfassung, Risiko-/SLA-Einschaetzung, Antwortentwurf und naechste Aktion.',
    'Gib die strukturierten Vorschlaege als Ergebnis im Format des Skills support/support-workflow zurueck.',
    'Conversation: conv_1',
    'Status: open',
    'Priority: urgent',
    '',
    'Kontext (Datensatz-Schnappschuss der Support-App, JSON):',
    JSON.stringify(snapshot),
  ].join('\n'));
  assert.deepEqual(e.args.payload.record_snapshot, snapshot);
  assert.equal(e.args.payload.request_id, out.request_id);
  const req = w.request();
  assert.equal(req.id, out.request_id);
  assert.equal(req.status, 'queued');
  assert.equal(req.task_id, e.task_id);
  assert.equal(req.conversation_id, 'conv_1');
  assert.deepEqual(req.required_skills, [SKILL]);
  // Großer Schnappschuss: Anweisung bleibt unter der 8 000-Zeichen-Kappung des Queue-Speichers, Payload vollständig.
  const big = { conversation: conv, recent_messages: Array.from({ length: 40 }, (_, i) => ({ id: `m${i}`, body_text: 'y'.repeat(500) })) };
  const g = world();
  await g.send('support.agent.task', { conversation_id: 'conv_1', record_snapshot: big }, 'cmd_big');
  assert.ok(g.sim.enqueued[0].args.instruction.length < 8000, `${g.sim.enqueued[0].args.instruction.length}`);
  assert.equal(g.sim.enqueued[0].args.payload.record_snapshot.recent_messages.length, 40);
  // Ohne Status/Priorität im Befehl gelten die Werte der Konversation.
  const v = world();
  await v.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_ask_2');
  assert.match(v.sim.enqueued[0].args.instruction, /\nStatus: pending\nPriority: high\n/);
});

test('Konversation fehlt → not_found, kein enqueue; Wiederholung desselben Befehls → derselbe Auftrag', async () => {
  const w = world();
  await assert.rejects(w.send('support.agent.task', { conversation_id: 'nope' }, 'cmd_x'), { code: 'not_found' });
  assert.equal(w.sim.enqueued.length, 0);
  const a = await w.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_same');
  const b = await w.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_same');
  assert.equal(a.request_id, b.request_id);
  assert.equal(a.task_id, b.task_id);
  assert.equal(w.sim.enqueued.length, 1);
  assert.equal(w.env.count('support_agent_requests'), 1);
});

test('complete: Skill-Ergebnis (JSON im Antworttext) → Vorschläge proposed + Ereignis je Vorschlag, Auftrag completed', async () => {
  const w = world();
  const out = await w.send('support.agent.task', { conversation_id: 'conv_1', record_snapshot: snapshot }, 'cmd_ask');
  const text = 'Analyse fertig.\n```json\n' + JSON.stringify({ suggestions: [
    { suggestion_kind: 'summary', summary: 'Login-Problem', confidence: 0.82, payload: { text: 'Kunde kann sich nicht anmelden.' } },
    { suggestion_kind: 'draft_reply', summary: 'Antwortentwurf', confidence: 0.6, required_human_action: 'approve', payload: { text: 'Hallo …' } },
    { suggestion_kind: 'bogus', summary: 'x' },
  ] }) + '\n```';
  const d = await w.sim.complete(out.task_id, { text });
  assert.equal(d.command.type, 'support.agent.task.complete');
  assert.equal(d.result.status, 'completed');
  const suggestions = w.env.docs('support_agent_suggestions');
  assert.equal(suggestions.length, 2);
  const byKind = Object.fromEntries(suggestions.map((s) => [s.suggestion_kind, s]));
  assert.equal(byKind.summary.status, 'proposed');
  assert.equal(byKind.summary.confidence, 0.82);
  assert.equal(byKind.summary.task_id, out.task_id);
  assert.equal(byKind.summary.source_command_id, 'cmd_ask');
  assert.equal(byKind.summary.conversation_id, 'conv_1');
  assert.deepEqual(byKind.summary.payload, { text: 'Kunde kann sich nicht anmelden.' });
  assert.equal(byKind.draft_reply.required_human_action, 'approve');
  const events = w.env.docs('support_conversation_events').filter((e) => e.event_type === 'support.agent.writeback');
  assert.equal(events.length, 2);
  assert.ok(events.every((e) => e.source_task_id === out.task_id));
  const req = w.request();
  assert.equal(req.status, 'completed');
  assert.deepEqual(req.payload.suggestion_ids.sort(), suggestions.map((s) => s.id).sort());
  assert.equal(req.payload.dropped_suggestions, 1);
});

test('fail → Auftrag failed mit Fehlertext; cancel (on_failed status cancelled) → cancelled; {error} im Ergebnis → failed', async () => {
  const f = world();
  const a = await f.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_f');
  const d = await f.sim.fail(a.task_id, 'model quota exhausted');
  assert.equal(d.command.type, 'support.agent.task.failed');
  assert.equal(f.request().status, 'failed');
  assert.equal(f.request().payload.error, 'model quota exhausted');
  assert.equal(f.env.count('support_agent_suggestions'), 0);

  const c = world();
  const b = await c.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_c');
  const out = await c.send('support.agent.task.failed', { task_id: b.task_id, status: 'cancelled', result: null }, 'core_cancel');
  assert.equal(out.status, 'cancelled');
  assert.equal(c.request().status, 'cancelled');
  assert.equal(c.request().payload.error, 'support agent task cancelled');

  const e = world();
  const x = await e.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_e');
  await e.sim.complete(x.task_id, { error: 'Schnappschuss leer' });
  assert.equal(e.request().status, 'failed');
  assert.equal(e.request().payload.error, 'Schnappschuss leer');
});

test('Doppelzustellung: Simulation verwirft die zweite; direkter zweiter Abschluss ändert nichts', async () => {
  const w = world();
  const a = await w.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_d');
  await w.sim.complete(a.task_id, { suggestions: [{ suggestion_kind: 'next_action', summary: 'Rückruf', payload: { text: 'anrufen' } }] });
  assert.deepEqual(await w.sim.complete(a.task_id, { suggestions: [] }), { delivered: false, reason: 'duplicate' });
  const writes = w.env.writes;
  const before = w.env.docs('support_agent_suggestions');
  const again = await w.send('support.agent.task.complete', { task_id: a.task_id, status: 'completed', result: { suggestions: [{ suggestion_kind: 'summary', summary: 'neu' }] } }, 'core_2');
  assert.equal(again.status, 'ignored');
  const late = await w.send('support.agent.task.failed', { task_id: a.task_id, status: 'failed', error: { code: 'failed', message: 'spät' } }, 'core_3');
  assert.equal(late.status, 'ignored');
  assert.equal(w.env.writes, writes, 'keine Schreibzugriffe');
  assert.deepEqual(w.env.docs('support_agent_suggestions'), before);
  assert.equal(w.request().status, 'completed');
});

test('Abschluss vor dem Setzen der task_id findet den Auftrag über agent.task.get (payload.request_id)', async () => {
  const w = world();
  const a = await w.send('support.agent.task', { conversation_id: 'conv_1' }, 'cmd_r');
  await w.ctx.patch('support_agent_requests', a.request_id, { task_id: '' });
  await w.sim.complete(a.task_id, { suggestions: [{ suggestion_kind: 'summary', summary: 's' }] });
  assert.equal(w.request().status, 'completed');
  assert.equal(w.request().task_id, a.task_id);
  await assert.rejects(w.send('support.agent.task.complete', { status: 'completed' }, 'core_x'), { code: 'invalid' });
});

test('Antwortauswertung: Freitext ohne JSON → ein summary-Vorschlag; Schnappschuss bleibt unter der Kappung', () => {
  const plain = parseAgentResult({ text: 'Kunde wartet auf Vertragsdaten.\nBitte nachfassen.' });
  assert.equal(plain.suggestions.length, 1);
  assert.equal(plain.suggestions[0].suggestion_kind, 'summary');
  assert.equal(plain.suggestions[0].summary, 'Kunde wartet auf Vertragsdaten.');
  assert.deepEqual(parseAgentResult(null), { error: null, suggestions: [], dropped: 0 });
  const big = { conversation: conv, recent_messages: Array.from({ length: 50 }, (_, i) => ({ id: `m${i}`, body_text: 'x'.repeat(400) })), notes: [], events: [], suggestions: [] };
  const text = compactSnapshot(big);
  assert.ok(text.length <= 6000, `${text.length}`);
  const kept = JSON.parse(text).recent_messages;
  assert.equal(kept.at(-1).id, 'm49', 'neueste Nachricht bleibt');
});
