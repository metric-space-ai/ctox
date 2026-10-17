// Tests: _projection.app_relevance.js (Port von project_app_relevance, kein Befehlstyp).
import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx } from './_ctx.mjs';
import project, { appThreadStatus, APP_RELEVANCE_SPECS } from '../_projection.app_relevance.js';
import { threadStateId } from '../_lib.js';

const now = 1_700_000_000_000;

test('14 Quell-Collections wie APP_RELEVANCE_SPECS (threads.rs:44-150)', () => {
  assert.equal(APP_RELEVANCE_SPECS.length, 14);
});

// app_relevance_projection_surfaces_ticket_approval_for_reviewer (threads.rs:8006)
test('Ticket-Freigabe → Thread needs_review beim Reviewer, nur Reviewer benachrichtigt', async () => {
  const h = makeCtx({ seed: { ctox_ticket_approvals: [{ id: 'approval-1', case_id: 'case-7', title: 'Antwortentwurf freigeben',
    status: 'pending', requester_user_id: 'bob', reviewer_user_id: 'alice', comment: 'Bitte vor Versand pruefen.', updated_at_ms: now }] } });
  const out = await project({ source_cursors: [{ collection: 'ctox_ticket_approvals', since_ms: 0 }], limit: 50 }, h.ctx);
  assert.ok(out.changed_count > 0);
  const t = h.doc('user_threads', 'thread_tickets_ticket_case_case-7');
  assert.equal(t.kind, 'approval');
  assert.equal(t.status, 'needs_review');
  assert.equal(t.assigned_user_id, 'alice');
  assert.deepEqual(t.participant_ids, ['alice', 'bob']);
  assert.equal(t.title, 'Ticket: Antwortentwurf freigeben');
  const l = h.doc('user_thread_links', 'link_thread_tickets_ticket_case_case-7_ctox_ticket_approvals_approval-1');
  assert.equal(l.link_role, 'app_record');
  assert.equal(l.link_type, 'ticket_case');
  assert.equal(l.source_record_id, 'approval-1');
  const n = h.doc('user_notifications', 'notif_app_needs_review_ctox_ticket_approvals_approval-1_alice');
  assert.equal(n.notification_type, 'approval_requested');
  assert.equal(h.doc('user_notifications', 'notif_app_needs_review_ctox_ticket_approvals_approval-1_bob'), null);
  assert.deepEqual(h.doc('user_thread_states', threadStateId('alice', t.id)).attention_reasons, ['Freigabe nötig']);
  assert.equal(out.source_cursors[0].since_ms, now);
  assert.equal((await project({ source_cursors: [{ collection: 'ctox_ticket_approvals', since_ms: 0 }] }, h.ctx)).changed_count, 0);
});

// app_relevance_projection_surfaces_pipeline_decision_for_owner (7870) + agent_escalation (7939/7957)
test('Kundenpipeline: offen → open, entschieden → completed; agent_escalation → needs_review + ungelesen', async () => {
  const h = makeCtx({});
  const doc = { id: 'kpl-e-triage-1', vorgang_id: 'kpl-v-1', typ: 'triage', titel: 'Portal-Login defekt', title: 'Portal-Login defekt',
    status: 'offen', owner_user_id: 'michael', assigned_user_id: 'michael', updated_at_ms: now };
  await project({ collection: 'kundenpipeline_entscheidungen', docs: [doc] }, h.ctx);
  const tid = 'thread_kundenpipeline_entscheidung_kpl-e-triage-1';
  let t = h.doc('user_threads', tid);
  assert.equal(t.kind, 'approval');
  assert.equal(t.status, 'open');
  assert.equal(t.assigned_user_id, 'michael');
  await project({ collection: 'kundenpipeline_entscheidungen', docs: [{ ...doc, status: 'entschieden', updated_at_ms: now + 1 }] }, h.ctx);
  t = h.doc('user_threads', tid);
  assert.equal(t.status, 'completed');
  assert.equal(appThreadStatus('kundenpipeline_entscheidungen', { typ: 'agent_escalation', status: 'offen' }), 'needs_review');
  assert.equal(appThreadStatus('kundenpipeline_entscheidungen', { typ: 'triage', status: 'offen' }), 'open');
  await project({ collection: 'kundenpipeline_entscheidungen', docs: [{ id: 'kpl-e-agent-1', typ: 'agent_escalation', titel: 'Architektur freigeben',
    status: 'offen', owner_user_id: 'michael', assigned_user_id: 'michael', participant_ids: ['michael'], updated_at_ms: now }] }, h.ctx);
  const n = h.all('user_notifications').find((x) => x.thread_id === 'thread_kundenpipeline_entscheidung_kpl-e-agent-1');
  assert.equal(n.user_id, 'michael');
  assert.equal(n.status, 'unread');
  assert.equal(n.notification_type, 'approval_requested');
  assert.equal(n.source_module, 'kundenpipeline');
  assert.equal(n.source_record_id, 'kpl-e-agent-1');
});

// app_relevance_projection_ignores_records_without_user_relevance (threads.rs:8085)
test('Datensatz ohne Nutzerbezug wird ignoriert; unbekannte Collection ebenso', async () => {
  const h = makeCtx({ seed: { support_conversations: [{ id: 'conv-public', status: 'open', priority: 'normal', search_text: 'Unassigned support conversation', updated_at_ms: now }] } });
  const out = await project({ source_cursors: [{ collection: 'support_conversations', since_ms: 0 }, { collection: 'mail', since_ms: 0 }] }, h.ctx);
  assert.equal(out.changed_count, 0);
  assert.equal(h.all('user_threads').length, 0);
  assert.equal(out.source_cursors.length, 1);
});
