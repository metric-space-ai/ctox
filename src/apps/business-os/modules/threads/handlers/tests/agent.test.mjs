// Strang SKL-A2 (Kern §16.30): threads startet Agentenarbeit selbst (agent.task.enqueue) und schließt sie über
// eigene Rückruf-Befehle ab. Geprüft mit der agent-task-Simulation (v4/tools/agent-sim): Knopf → Befehl → enqueue
// (Template + Rückrufe), complete → Ergebnis in eigenen Collections, fail/cancel → Ereigniszeile + Benachrichtigung,
// Doppelzustellung ohne Wirkung. Strang FIX10: kein Rückruf an eine anfragende App mehr (Liste leer), nichts wird
// weitergeleitet.
import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, cmd, USERS, rejects } from './_ctx.mjs';
import aiRequest from '../threads.ai.request.js';
import aiFailed from '../threads.ai.failed.js';
import aiComplete from '../threads.ai.complete.js';
import request from '../threads.ctox_approval.request.js';
import approve from '../threads.ctox_approval.approve.js';
import approvalFailed from '../threads.ctox_approval.failed.js';
import { aiResultMessageId, approvalResultMessageId } from '../_agent.js';

const ALICE = { id: 'alice', role: 'user', display_name: 'Alice' };
const JUNIOR = { id: 'junior', role: 'user', display_name: 'Junior' };
const LEAD = { id: 'lead', role: 'user', display_name: 'Lead' };
const users = { ...USERS, junior: { display_name: 'Junior' }, lead: { display_name: 'Lead' } };

function seedThread() {
  return { user_threads: [{ id: 't1', thread_id: 't1', title: 'Fall', kind: 'case', status: 'open', participant_ids: ['alice', 'bob'],
    owner_user_id: 'alice', assigned_user_id: '', source_module: 'support', source_record_type: 'conversation',
    source_record_id: 'c1', source_label: 'Kunde', created_at_ms: 10, updated_at_ms: 10 }] };
}

const callback = (type, payload) => ({ id: `cb-${type}-${payload.task_id}`, type, app_id: 'threads', module: 'threads', record_id: null, payload });

test('ai.request: enqueue mit Template prompts/chat-task.md, eigenen Rückrufen, Idempotenzschlüssel, Verknüpfung', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  const command = cmd('threads.ai.request', { thread_id: 't1', goal: 'Fasse den Fall zusammen', api_key: 'geheim' }, ALICE);
  const r = await aiRequest(command, h.ctx);
  assert.equal(h.sim.enqueued.length, 1);
  const [{ task_id, args, skill }] = h.sim.enqueued;
  assert.equal(r.task_id, task_id);
  assert.equal(skill, null); // threads bringt keinen Skill mit (F §5)
  assert.equal(args.on_complete, 'threads.ai.complete');
  assert.equal(args.on_failed, 'threads.ai.failed');
  assert.equal(args.idempotency_key, `threads.ai.request:${command.id}`);
  // Template wörtlich aus command_prompt (store.rs:27097): Auftrag, Befehlsblock, JSON-Vorschauen
  assert.ok(args.instruction.startsWith('Fasse den Fall zusammen\nBusiness OS command:\n- command_id: '));
  assert.match(args.instruction, /\n- module: support\n- type: threads\.ai\.request\n- record_id: c1\n/);
  assert.match(args.instruction, /Full payload and client context are stored on the Business OS command record\./);
  assert.match(args.instruction, /Payload JSON:\n\{\n {2}"title": "AI: Fasse den Fall zusammen"/);
  assert.match(args.instruction, /Client context JSON:\n\{\n {2}"actor": \{/);
  assert.doesNotMatch(args.instruction, /geheim|\{\{/);
  const link = h.all('user_thread_links').find((l) => l.task_id === task_id);
  assert.equal(link.link_role, 'ai_task');
  assert.equal(link.status, 'queued');
  assert.equal(link.requester_user_id, 'alice');
  assert.equal(link.thread_id, 't1');
  // Wiederholung desselben Befehls reiht nichts Neues ein (idempotency_key)
  await aiRequest(command, h.ctx);
  assert.equal(h.sim.tasks.size, 1);
  assert.ok(h.sim.log.some((e) => e.event === 'idempotent_hit'));
});

test('ai.complete: Antwort als Nachricht von CTOX, Benachrichtigung, Verknüpfung abgeschlossen; Doppelzustellung wirkungslos', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  const r = await aiRequest(cmd('threads.ai.request', { thread_id: 't1', goal: 'Fasse den Fall zusammen' }, ALICE), h.ctx);
  h.tick(50);
  const d = await h.sim.complete(r.task_id, { text: 'Der Kunde wartet auf Rückruf.' });
  assert.equal(d.delivered, true);
  assert.equal(d.command.type, 'threads.ai.complete');
  assert.equal(d.result.status, 'completed');
  const msg = h.doc('user_thread_messages', aiResultMessageId(r.task_id));
  assert.equal(msg.kind, 'ai_result');
  assert.equal(msg.actor_type, 'ai');
  assert.equal(msg.author_user_id, 'ctox');
  assert.equal(msg.body, 'Der Kunde wartet auf Rückruf.');
  assert.deepEqual(msg.target_user_ids, ['alice']);
  assert.equal(h.doc('user_notifications', `notif_${msg.id}_alice`).notification_type, 'ctox_completed');
  assert.equal(h.doc('user_threads', 't1').last_message_id, msg.id);
  assert.equal(h.all('user_thread_links').find((l) => l.task_id === r.task_id).status, 'completed');
  // Doppelzustellung: Simulation verwirft, direkter zweiter Aufruf schreibt nichts
  assert.deepEqual(await h.sim.complete(r.task_id, { text: 'anders' }), { delivered: false, reason: 'duplicate' });
  const writes = h.calls.writes.length;
  const again = await aiComplete(callback('threads.ai.complete', { task_id: r.task_id, status: 'completed', result: { text: 'anders' } }), h.ctx);
  assert.equal(again.status, 'ignored');
  assert.equal(h.calls.writes.length, writes);
  assert.equal(h.doc('user_thread_messages', msg.id).body, 'Der Kunde wartet auf Rückruf.');
});

test('ai.failed: Fehler als Ereigniszeile (ctox_status) + ctox_failed; Abbruch (cancelled) als beendete Arbeit', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  const r = await aiRequest(cmd('threads.ai.request', { thread_id: 't1', goal: 'Fasse zusammen' }, ALICE), h.ctx);
  const d = await h.sim.fail(r.task_id, { code: 'failed', message: 'Modell nicht erreichbar' });
  assert.equal(d.command.type, 'threads.ai.failed');
  const msg = h.doc('user_thread_messages', aiResultMessageId(r.task_id));
  assert.equal(msg.kind, 'ctox_status');
  assert.equal(msg.body, 'CTOX Arbeit blockiert oder fehlgeschlagen.\n\nModell nicht erreichbar');
  assert.equal(h.doc('user_notifications', `notif_${msg.id}_alice`).notification_type, 'ctox_failed');
  const link = h.all('user_thread_links').find((l) => l.task_id === r.task_id);
  assert.equal(link.status, 'failed');
  assert.equal(link.error, 'Modell nicht erreichbar');
  // Abbruch: der Core stellt on_failed mit status 'cancelled' zu
  const r2 = await aiRequest(cmd('threads.ai.request', { thread_id: 't1', goal: 'Noch einmal' }, ALICE), h.ctx);
  await h.sim.api().cancel({ task_id: r2.task_id });
  const out = await aiFailed(callback('threads.ai.failed', { task_id: r2.task_id, status: 'cancelled', result: null, error: null }), h.ctx);
  assert.equal(out.status, 'cancelled');
  const cancelled = h.doc('user_thread_messages', aiResultMessageId(r2.task_id));
  assert.equal(cancelled.body, 'CTOX Arbeit wurde beendet.');
  assert.equal(h.doc('user_notifications', `notif_${cancelled.id}_alice`).notification_type, 'ctox_finished');
  assert.equal((await aiFailed(callback('threads.ai.failed', { task_id: r2.task_id, status: 'failed' }), h.ctx)).status, 'ignored');
  // Rückruf ohne passenden Auftrag / fremdes Modul
  await rejects(aiComplete(callback('threads.ai.complete', { task_id: 'task_unknown', status: 'completed' }), h.ctx), 'not_found');
  await rejects(aiComplete({ ...callback('threads.ai.complete', { task_id: r.task_id }), module: 'support', app_id: 'support' }, h.ctx), 'invalid');
  await rejects(aiComplete(callback('threads.ai.complete', { status: 'completed' }), h.ctx), 'invalid', 'task_id is required');
});

test('ai.request: zu lange Anweisung → invalid, nichts eingereiht', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  await rejects(aiRequest(cmd('threads.ai.request', { thread_id: 't1', goal: 'x'.repeat(8001) }, ALICE), h.ctx), 'invalid', 'business_command_instruction_too_large');
  assert.equal(h.sim.enqueued.length, 0);
});

function delegationRequest(extra = {}) {
  return cmd('threads.ctox_approval.request', {
    approval_request_id: 'approval-7',
    prompt: 'Bitte die Rechnungsübersicht prüfen und zusammenfassen.',
    reviewer_user_id: 'lead',
    target_module: 'reports',
    target_record_id: 'rep-1',
    target_command_type: 'business_os.chat.task',
    source_context: { module: 'reports', record_type: 'bug_report', record_id: 'rep-1', label: 'Bug 1' },
    ...extra,
  }, JUNIOR, { record_id: 'rep-1' });
}

async function approved(h) {
  await request(delegationRequest(), h.ctx);
  const v = h.doc('ctox_task_approval_requests', 'approval-7').updated_at_ms;
  return approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-7', expected_updated_at_ms: v }, LEAD), h.ctx);
}

test('approve (Agentenziel): eigener Task statt ctx.command; complete → Ergebnis, keine Weiterleitung (FIX10)', async () => {
  const h = makeCtx({ users });
  const r = await approved(h);
  assert.equal(h.calls.commands.length, 0);
  assert.equal(h.sim.enqueued.length, 1);
  const [{ task_id, args }] = h.sim.enqueued;
  assert.equal(r.approved_task_id, task_id);
  assert.equal(r.approved_command.execution_mode, 'agent_task');
  assert.equal(args.on_complete, 'threads.ctox_approval.complete');
  assert.equal(args.on_failed, 'threads.ctox_approval.failed');
  assert.equal(args.idempotency_key, 'threads.ctox_approval:approval-7');
  assert.equal(args.title, 'Freigabe: Bug 1');
  assert.ok(args.instruction.startsWith('Bitte die Rechnungsübersicht prüfen und zusammenfassen.\nBusiness OS command:\n'));
  assert.match(args.instruction, /- type: business_os\.chat\.task\n- record_id: rep-1\n/);
  assert.match(args.instruction, /"approval_request_id": "approval-7"/);
  const a = h.doc('ctox_task_approval_requests', 'approval-7');
  assert.equal(a.status, 'approved');
  assert.equal(a.callback, null);
  h.tick(5);
  const d = await h.sim.complete(task_id, { text: 'Zusammenfassung liegt vor.' });
  assert.equal(d.result.status, 'completed');
  const done = h.doc('ctox_task_approval_requests', 'approval-7');
  assert.equal(done.status, 'approved');
  assert.equal(done.execution_status, 'completed');
  assert.equal(done.result_text, 'Zusammenfassung liegt vor.');
  const msg = h.doc('user_thread_messages', approvalResultMessageId('approval-7'));
  assert.equal(msg.kind, 'ai_result');
  assert.equal(msg.body, 'Zusammenfassung liegt vor.');
  assert.deepEqual(msg.target_user_ids, ['junior']);
  assert.equal(h.calls.commands.length, 0, 'keine Weiterleitung');
  assert.equal(d.result.forwarded, null);
  // Doppelzustellung: kein zweites Schreiben
  assert.equal((await h.sim.complete(task_id, {})).reason, 'duplicate');
  const writes = h.calls.writes.length;
  const again = await (await import('../threads.ctox_approval.complete.js')).default(callback('threads.ctox_approval.complete', { task_id, status: 'completed', result: 'x' }), h.ctx);
  assert.equal(again.status, 'ignored');
  assert.equal(h.calls.writes.length, writes);
  assert.equal(h.calls.commands.length, 0);
});

test('approve (Agentenziel): failed/cancelled → Ereigniszeile, keine Weiterleitung (FIX10)', async () => {
  const h = makeCtx({ users });
  await approved(h);
  const [{ task_id }] = h.sim.enqueued;
  const out = await approvalFailed(callback('threads.ctox_approval.failed', { task_id, status: 'cancelled', result: null }), h.ctx);
  assert.equal(out.status, 'cancelled');
  assert.equal(h.doc('ctox_task_approval_requests', 'approval-7').execution_status, 'cancelled');
  assert.equal(h.doc('user_thread_messages', approvalResultMessageId('approval-7')).body, 'CTOX Arbeit wurde beendet.');
  assert.equal(h.calls.commands.length, 0);
});

test('request: Rückruf nur im Namensraum der Quell-App, sonst invalid ohne Schreiben', async () => {
  const h = makeCtx({ users });
  // FIX10: der frühere Rückruf an reports ist kein deklarierter Rückruf mehr.
  await rejects(request(delegationRequest({ callback: { on_complete: 'reports.delegation.complete', on_failed: 'reports.delegation.failed' } }), h.ctx), 'invalid', 'not a declared approval callback');
  await rejects(request(delegationRequest({ callback: { on_complete: 'tickets.x.complete', on_failed: 'reports.delegation.failed' } }), h.ctx), 'invalid', 'must be a command type of app reports');
  await rejects(request(delegationRequest({ callback: { on_complete: 'reports.delegation.complete' } }), h.ctx), 'invalid', 'callback.on_failed is required');
  await rejects(request(delegationRequest({ callback: 'reports.delegation.complete' }), h.ctx), 'invalid', 'callback must be an object');
  await rejects(request(delegationRequest({ source_context: { module: 'threads', record_id: 'x' }, callback: { on_complete: 'threads.ai.complete', on_failed: 'threads.ai.failed' } }), h.ctx), 'invalid', 'another app');
  assert.equal(h.calls.writes.length, 0);
  // ohne callback bleibt alles wie bisher (callback: null)
  await request(delegationRequest({ callback: undefined }), h.ctx);
  assert.equal(h.doc('ctox_task_approval_requests', 'approval-7').callback, null);
});
