// Tests: threads.ctox_approval.request|approve|reject|cancel|edit.
import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, cmd, USERS, rejects } from './_ctx.mjs';
import request from '../threads.ctox_approval.request.js';
import approve from '../threads.ctox_approval.approve.js';
import reject from '../threads.ctox_approval.reject.js';
import cancel from '../threads.ctox_approval.cancel.js';
import edit from '../threads.ctox_approval.edit.js';
import { threadStateId } from '../_lib.js';

const JUNIOR = { id: 'junior', role: 'user', display_name: 'Junior' };
const LEAD = { id: 'lead', role: 'user', display_name: 'Lead' };
const ADMIN = { id: 'admin', role: 'admin', display_name: 'Admin' };
const MALLORY = { id: 'mallory', role: 'user' };
const users = { ...USERS, junior: { display_name: 'Junior' }, lead: { display_name: 'Lead' }, mallory: {} };

function requestCmd(extra = {}) {
  return cmd('threads.ctox_approval.request', {
    approval_request_id: 'approval-1',
    prompt: 'CTOX soll den Fall zusammenfassen.',
    reviewer_user_id: 'lead',
    target_module: 'ctox',
    target_record_id: 'case-3',
    target_command_type: 'business_os.chat.task',
    source_context: { module: 'threads', record_type: 'case', record_id: 'case-3', label: 'Case 3' },
    target_payload: { mode: 'data', target: 'data' },
    ...extra,
  }, JUNIOR, { record_id: 'case-3' });
}

// approval_request_is_not_a_queue_task_until_reviewed (threads.rs:5850) + attention (5915)
test('request: pending, keine Queue, Freigabe nötig 80 nur für Reviewer', async () => {
  const h = makeCtx({ users });
  const out = await request(requestCmd(), h.ctx);
  assert.equal(out.status, 'pending');
  assert.equal(h.calls.commands.length, 0);
  const a = h.doc('ctox_task_approval_requests', 'approval-1');
  assert.equal(a.status, 'pending');
  assert.equal(a.requester_user_id, 'junior');
  assert.equal(a.reviewer_user_id, 'lead');
  assert.equal(a.reviewer_display_name, 'Lead');
  assert.equal(a.target_record_id, 'case-3');
  assert.deepEqual(a.target_payload, { mode: 'data', target: 'data' });
  assert.equal(a.client_context, null);
  const t = h.doc('user_threads', out.thread_id);
  assert.equal(out.thread_id, 'thread_threads_case_case-3');
  assert.equal(t.kind, 'approval');
  assert.equal(t.title, 'Case 3');
  assert.equal(t.assigned_user_id, 'lead');
  assert.equal(t.pending_approval_count, 1);
  const lead = h.doc('user_thread_states', threadStateId('lead', out.thread_id));
  assert.deepEqual(lead.attention_reasons, ['Freigabe nötig']);
  assert.equal(lead.attention_score, 80);
  const junior = h.doc('user_thread_states', threadStateId('junior', out.thread_id));
  assert.deepEqual(junior.attention_reasons, []);
  assert.equal(junior.attention_score, 0);
  const msg = h.all('user_thread_messages')[0];
  assert.equal(msg.kind, 'approval_request');
  assert.deepEqual(msg.target_user_ids, ['lead']);
  assert.equal(h.all('user_notifications')[0].notification_type, 'approval_request');
});

// approval_request_requires_active_reviewer_and_blocks_self_review (threads.rs:6222)
test('request: aktiver Reviewer nötig, keine Selbstfreigabe (außer Admin-Override)', async () => {
  const h = makeCtx({ users });
  await rejects(request(requestCmd({ reviewer_user_id: 'ghost' }), h.ctx), 'invalid', 'is not an active Business OS user');
  await rejects(request(requestCmd({ reviewer_user_id: 'gone' }), h.ctx), 'invalid', 'is inactive');
  await rejects(request(requestCmd({ reviewer_user_id: 'junior' }), h.ctx), 'invalid', 'cannot assign themselves');
  assert.equal(h.all('ctox_task_approval_requests').length, 0);
  const adminSelf = cmd('threads.ctox_approval.request', { prompt: 'p', reviewer_user_id: 'admin', allow_self_approval_admin_override: true, thread_id: 'ta' }, ADMIN);
  const out = await request(adminSelf, h.ctx);
  assert.equal(out.status, 'pending');
  await rejects(request(cmd('threads.ctox_approval.request', { prompt: 'p' }, JUNIOR), h.ctx), 'invalid', 'required text field is missing');
});

// approved_request_creates_command_task_and_audit_linkage (threads.rs:6364)
// SKL-A2/THR2 + LOOP Runde 2: einziges Ziel ist die Agentenarbeit (eigener Task mit Rückruf an threads, kein Befehl an
// eine fremde App); ctox.coding.turn ist entfernt (targets.test.mjs). Rückruf/Weiterleitung siehe agent.test.mjs.
test('approve: reiht Agenten-Task ein, setzt approved, Audit-Ereignis, Zähler -1', async () => {
  const h = makeCtx({ users });
  const out = await request(requestCmd(), h.ctx);
  const version = h.doc('ctox_task_approval_requests', 'approval-1').updated_at_ms;
  h.tick(10);
  const decision = cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-1', expected_updated_at_ms: version, decision_note: 'Passt.' }, ADMIN, { record_id: 'approval-1' });
  const r = await approve(decision, h.ctx);
  assert.equal(r.status, 'approved');
  assert.equal(h.calls.commands.length, 0, 'kein Befehl an eine fremde App');
  assert.equal(h.sim.enqueued.length, 1);
  const [{ task_id, args }] = h.sim.enqueued;
  assert.equal(r.approved_task_id, task_id);
  assert.deepEqual(r.approved_command, { ok: true, command_id: r.approved_command_id, status: 'accepted', execution_mode: 'agent_task', task_id, task_status: 'queued' });
  assert.equal(args.on_complete, 'threads.ctox_approval.complete');
  assert.equal(args.on_failed, 'threads.ctox_approval.failed');
  assert.equal(args.title, 'Freigabe: Case 3');
  assert.equal(args.thread_key, `business-os/threads/${out.thread_id}`);
  assert.equal(args.payload.approved_command_id, r.approved_command_id);
  assert.equal(args.payload.record_id, 'case-3');
  assert.ok(args.instruction.startsWith('CTOX soll den Fall zusammenfassen.'));
  assert.match(args.instruction, new RegExp(`"decision_command_id": "${decision.id}"`));
  assert.match(args.instruction, /"requester_user_id": "junior"/);
  const a = h.doc('ctox_task_approval_requests', 'approval-1');
  assert.equal(a.status, 'approved');
  assert.equal(a.decision_by_id, 'admin');
  assert.equal(a.decision_note, 'Passt.');
  assert.equal(a.approved_command_id, r.approved_command_id);
  assert.equal(h.doc('user_threads', out.thread_id).pending_approval_count, 0);
  assert.equal(h.calls.audit.length, 1);
  assert.equal(h.calls.audit[0].event_type, 'business_os.external_approval.decided');
  assert.equal(h.calls.audit[0].payload.decision, 'approved');
  assert.equal(h.calls.audit[0].payload.approved_task_id, task_id);
  assert.deepEqual(h.calls.policy, [{ permission: 'data.write', module: 'ctox' }]);
  const lead = h.doc('user_thread_states', threadStateId('lead', out.thread_id));
  assert.ok(!lead.attention_reasons.includes('Freigabe nötig'));
  const msg = h.all('user_thread_messages').find((m) => m.kind === 'approval_approved');
  assert.equal(msg.body, 'Freigegeben: CTOX soll den Fall zusammenfassen.');
  assert.equal(msg.command_id, r.approved_command_id);
  assert.equal(h.doc('user_notifications', `notif_${msg.id}_junior`).title, 'CTOX Freigabe erteilt');
  // angenommen, noch nicht abgeschlossen: das Ergebnis kommt per Rückruf des Agenten-Tasks
  assert.equal(r.execution, undefined);
  assert.equal(h.doc('ctox_task_approval_requests', 'approval-1').execution_status, undefined);
});

// unauthorized_actor_cannot_approve_and_request_stays_pending (5979), stale (6527), policy (6667)
test('approve: Unbefugter, veraltete Version, Policy, zentrale Ablehnung', async () => {
  const h = makeCtx({ users });
  await request(requestCmd(), h.ctx);
  const v = h.doc('ctox_task_approval_requests', 'approval-1').updated_at_ms;
  await rejects(approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-1', expected_updated_at_ms: v }, MALLORY), h.ctx), 'forbidden', 'Nur der eingetragene Reviewer');
  await rejects(approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-1', expected_updated_at_ms: v - 1 }, LEAD), h.ctx), 'conflict', 'approval request changed');
  await rejects(approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-1' }, LEAD), h.ctx), 'invalid', 'expected_updated_at_ms is required');
  assert.equal(h.doc('ctox_task_approval_requests', 'approval-1').status, 'pending');
  assert.equal(h.sim.enqueued.length, 0);
  const h2 = makeCtx({ users, policy: (perm) => ({ allowed: perm !== 'data.write', display_reason: 'no write' }) });
  await request(requestCmd(), h2.ctx);
  const v2 = h2.doc('ctox_task_approval_requests', 'approval-1').updated_at_ms;
  await rejects(approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-1', expected_version: v2 }, LEAD), h2.ctx), 'forbidden', 'reviewer cannot approve target `business_os.chat.task` for module `ctox`: no write');
  assert.equal(h2.calls.commands.length, 0);
  assert.equal(h2.sim.enqueued.length, 0);
  // app-Ziel → apps.modify
  const h3 = makeCtx({ users });
  await request(requestCmd({ target_payload: { target: 'APP ' } }), h3.ctx);
  const v3 = h3.doc('ctox_task_approval_requests', 'approval-1').updated_at_ms;
  await approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-1', expected_updated_at_ms: v3 }, LEAD), h3.ctx);
  assert.deepEqual(h3.calls.policy.at(-1), { permission: 'apps.modify', module: 'ctox' });
  // Nicht mehr pending
  await rejects(approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'approval-1', expected_updated_at_ms: v3 }, LEAD), h3.ctx), 'conflict', 'not pending');
  await rejects(approve(cmd('threads.ctox_approval.approve', { approval_request_id: 'nope', expected_updated_at_ms: 1 }, LEAD), h3.ctx), 'not_found', 'approval request nope not found');
});

// rejected_approval_creates_no_queue_task (threads.rs:6291)
test('reject: kein Befehl, Nachricht mit Begründung, Audit', async () => {
  const h = makeCtx({ users });
  const out = await request(requestCmd(), h.ctx);
  const v = h.doc('ctox_task_approval_requests', 'approval-1').updated_at_ms;
  await rejects(reject(cmd('threads.ctox_approval.reject', { approval_request_id: 'approval-1', expected_updated_at_ms: v }, JUNIOR), h.ctx), 'forbidden');
  const r = await reject(cmd('threads.ctox_approval.reject', { approval_request_id: 'approval-1', expected_updated_at_ms: v, decision_note: 'Zu riskant' }, LEAD), h.ctx);
  assert.equal(r.status, 'rejected');
  assert.equal(h.calls.commands.length, 0);
  const msg = h.all('user_thread_messages').find((m) => m.kind === 'approval_rejected');
  assert.equal(msg.body, 'rejected: Zu riskant\n\nCTOX soll den Fall zusammenfassen.');
  assert.equal(h.doc('user_notifications', `notif_${msg.id}_junior`).title, 'CTOX Freigabe abgelehnt');
  assert.equal(h.doc('user_threads', out.thread_id).pending_approval_count, 0);
  assert.equal(h.calls.audit[0].payload.decision, 'rejected');
});

test('cancel: nur Antragsteller oder Admin; Reviewer darf nicht stornieren', async () => {
  const h = makeCtx({ users });
  await request(requestCmd(), h.ctx);
  const v = h.doc('ctox_task_approval_requests', 'approval-1').updated_at_ms;
  await rejects(cancel(cmd('threads.ctox_approval.cancel', { approval_request_id: 'approval-1', expected_updated_at_ms: v }, LEAD), h.ctx), 'forbidden', 'only requester or admin can cancel');
  const r = await cancel(cmd('threads.ctox_approval.cancel', { expected_updated_at_ms: v }, JUNIOR, { record_id: 'approval-1' }), h.ctx);
  assert.equal(r.status, 'cancelled');
  const msg = h.all('user_thread_messages').find((m) => m.kind === 'approval_cancelled');
  assert.equal(msg.body, 'cancelled: CTOX soll den Fall zusammenfassen.');
  assert.equal(h.doc('user_notifications', `notif_${msg.id}_junior`).title, 'CTOX Freigabe storniert');
});

// approval_edit_cannot_change_target (threads.rs:6591)
test('edit: Ziel unveränderlich, Prompt änderbar, Gegenseite benachrichtigt', async () => {
  const h = makeCtx({ users });
  const out = await request(requestCmd(), h.ctx);
  const v = h.doc('ctox_task_approval_requests', 'approval-1').updated_at_ms;
  await rejects(edit(cmd('threads.ctox_approval.edit', { approval_request_id: 'approval-1', expected_updated_at_ms: v, target_module: 'outbound' }, JUNIOR), h.ctx), 'invalid', 'approval target cannot be edited');
  await rejects(edit(cmd('threads.ctox_approval.edit', { approval_request_id: 'approval-1', expected_updated_at_ms: v, target_payload: { target: 'app' } }, JUNIOR), h.ctx), 'invalid', 'target payload cannot be edited');
  await rejects(edit(cmd('threads.ctox_approval.edit', { approval_request_id: 'approval-1', expected_updated_at_ms: v, prompt: 'x' }, MALLORY), h.ctx), 'forbidden');
  h.tick(3);
  const r = await edit(cmd('threads.ctox_approval.edit', { approval_request_id: 'approval-1', expected_updated_at_ms: v, prompt: 'Neu', target_module: 'ctox', target_payload: { mode: 'data', target: 'data' } }, JUNIOR), h.ctx);
  assert.equal(r.status, 'pending');
  const a = h.doc('ctox_task_approval_requests', 'approval-1');
  assert.equal(a.prompt, 'Neu');
  assert.equal(a.instruction, 'Neu');
  assert.equal(a.updated_at_ms, h.ctx.now());
  const msg = h.all('user_thread_messages').find((m) => m.kind === 'approval_edited');
  assert.deepEqual(msg.target_user_ids, ['lead']);
  assert.equal(h.doc('user_notifications', `notif_${msg.id}_lead`).title, 'CTOX Freigabe aktualisiert');
  assert.equal(h.doc('user_threads', out.thread_id).pending_approval_count, 1);
  // alte Version jetzt veraltet
  await rejects(edit(cmd('threads.ctox_approval.edit', { approval_request_id: 'approval-1', expected_updated_at_ms: v, prompt: 'x' }, LEAD), h.ctx), 'conflict');
});
