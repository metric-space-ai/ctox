// Fall-Zustandsautomat der Tickets-App (Strang TICKAPP): Handler gegen die Rust-Referenz
// mission/tickets/cases.rs, case_state.rs, ticket_local_native.rs, core_state/mod.rs:1172 (Ticket-Katalog).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createCtx, run } from './_ctx.mjs';

const NOW = Date.parse('2026-10-15T10:00:00.000Z');
const ISO = '2026-10-15T10:00:00.000Z';

const ticket = (system, remoteId, extra = {}) => ({
  id: `${system}:${remoteId}`, ticket_key: `${system}:${remoteId}`, source_system: system, remote_ticket_id: remoteId,
  title: 'Drucker', body_text: 'kaputt', remote_status: 'open', priority: null, requester: null, created_at: ISO,
  updated_at: ISO, last_synced_at: ISO, metadata: {}, updated_at_ms: NOW, is_deleted: false, ...extra,
});
const ticketCase = (caseId, ticketKey, state) => ({
  id: caseId, case_id: caseId, ticket_key: ticketKey, label: 'printer', bundle_label: 'printer-bundle', bundle_version: 1,
  state, approval_mode: 'human_approval_required', autonomy_level: 'A1', support_mode: 'support_case', risk_level: 'low',
  opened_at: ISO, updated_at: ISO, closed_at: null, updated_at_ms: NOW, is_deleted: false,
});
const inbound = (key, ticketKey, at) => ({
  id: key, event_key: key, ticket_key: ticketKey, source_system: 'local', remote_event_id: key.split(':')[1], direction: 'inbound',
  event_type: 'comment', summary: 'comment', body_text: 'x', metadata: {}, external_created_at: at, observed_at: at,
  updated_at_ms: NOW, is_deleted: false,
});

function world({ state = 'approval_pending', system = 'local', extraSeed = {} } = {}) {
  const t = ticket(system, system === 'local' ? 'LT-aaa' : '4711');
  return createCtx({
    now: NOW,
    seed: { ctox_ticket_items: [t], ctox_ticket_cases: [ticketCase('case-1', t.ticket_key, state)], ...extraSeed },
    caps: { 'agent.task.enqueue': (() => { let n = 0; return () => ({ task_id: `task-${++n}` }); })() },
  });
}
const caseOf = (ctx) => ctx.$.doc('ctox_ticket_cases', 'case-1');
const audits = (ctx) => ctx.$.capsTo('audit.event').map((c) => c.args[0]);

// ------------------------------------------------------------------------------------------- lokale Tickets

test('local.create: Ticket + Ereignis in der App, ticket_key = LT-Id wie Rust, Pflichtfeld/Alias', async () => {
  const ctx = createCtx({ now: NOW });
  const out = await run(ctx, 'ctox.ticket.local.create', { title: ' Drucker ', body_text: 'kaputt', priority: 'high' });
  assert.match(out.ticket_key, /^LT-[0-9a-f]{12}$/);
  assert.equal(out.source_system, 'local');
  assert.deepEqual(out.ticket, {
    ticket_id: out.ticket_key, title: 'Drucker', body_text: 'kaputt', status: 'open', priority: 'high', requester: null,
    created_at: ISO, updated_at: ISO, metadata: {},
  });
  const item = ctx.$.doc('ctox_ticket_items', `local:${out.ticket_key}`);
  assert.equal(item.remote_status, 'open');
  const [event] = ctx.$.all('ctox_ticket_events');
  assert.equal(event.event_type, 'created');
  assert.equal(event.direction, 'inbound');
  assert.equal(event.ticket_key, `local:${out.ticket_key}`);
  await assert.rejects(run(ctx, 'ctox.ticket.local.create', { title: '  ' }), { code: 'invalid', message: 'title is required' });
  assert.equal(ctx.$.capCalls.length, 0, 'keine Capability, insbesondere kein ctox.ticket.run');
});

test('local.create: vorgegebene ticket_id (support über ctx.command), idempotent je Befehl, sonst conflict', async () => {
  const ctx = createCtx({ now: NOW });
  const first = await run(ctx, 'ctox.ticket.local.create', { title: 'A', ticket_id: 'LT-support1' }, { id: 'c1' });
  assert.equal(first.ticket_key, 'LT-support1');
  const again = await run(ctx, 'ctox.ticket.local.create', { title: 'A', ticket_id: 'LT-support1' }, { id: 'c1' });
  assert.equal(again.ticket_key, 'LT-support1');
  assert.equal(ctx.$.all('ctox_ticket_items').length, 1);
  await assert.rejects(run(ctx, 'ctox.ticket.local.create', { title: 'B', ticket_id: 'LT-support1' }, { id: 'c2' }), { code: 'conflict' });
  await assert.rejects(run(ctx, 'ctox.ticket.local.create', { title: 'B', ticket_id: 'x/../y' }), { code: 'invalid' });
});

test('local.comment / local.transition: Aliase, Meldungen wie Rust (or_else → zweiter Schlüssel), Ticket fehlt', async () => {
  const ctx = world();
  const comment = await run(ctx, 'ctox.ticket.local.comment', { ticket_key: 'LT-aaa', body: 'hi' });
  assert.equal(comment.ticket_key, 'LT-aaa');
  assert.equal(comment.event.event_type, 'comment');
  await assert.rejects(run(ctx, 'ctox.ticket.local.comment', { body: 'hi' }), { code: 'invalid', message: 'ticket_key is required' });
  await assert.rejects(run(ctx, 'ctox.ticket.local.comment', { ticket_id: 'LT-zzz', body: 'hi' }), { code: 'not_found', message: 'local ticket not found: LT-zzz' });
  const moved = await run(ctx, 'ctox.ticket.local.transition', { ticket_id: 'LT-aaa', state: 'done' });
  assert.equal(moved.ticket.status, 'done');
  const statusEvent = ctx.$.all('ctox_ticket_events').find((e) => e.event_type === 'status_changed');
  assert.equal(statusEvent.summary, 'status -> done');
  assert.equal(statusEvent.body_text, 'Ticket moved to status done');
  await assert.rejects(run(ctx, 'ctox.ticket.local.transition', { ticket_id: 'LT-aaa' }), { code: 'invalid', message: 'state is required' });
});

// ------------------------------------------------------------------------------------------- Fall-Lebenszyklus

test('Lebenszyklus lokal: approve → execute → verify → writeback_comment → writeback_transition (closed)', async () => {
  const ctx = world();
  let out = await run(ctx, 'ctox.ticket.approve', { case_id: 'case-1', status: 'approved' }, { id: 'a1' });
  assert.equal(out.case.state, 'executable');
  assert.equal(out.ticket_key, 'local:LT-aaa');
  const [approval] = ctx.$.all('ctox_ticket_approvals');
  assert.equal(approval.decided_by, 'owner');
  assert.match(approval.approval_id, /^approval:case-1:[0-9a-f]{12}$/);
  out = await run(ctx, 'ctox.ticket.execute', { case_id: 'case-1', summary: 'Treiber neu' }, { id: 'e1' });
  assert.equal(out.case.state, 'executing');
  assert.equal(out.case.last_execution_action.summary, 'Treiber neu');
  out = await run(ctx, 'ctox.ticket.verify', { case_id: 'case-1', status: 'passed' }, { id: 'v1' });
  assert.equal(out.case.state, 'writeback_pending');
  out = await run(ctx, 'ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'Erledigt' }, { id: 'w1' });
  assert.equal(out.case.state, 'writeback_pending');
  const outbound = ctx.$.all('ctox_ticket_events').filter((e) => e.direction === 'outbound');
  assert.equal(outbound.length, 1);
  assert.equal(outbound[0].metadata.origin, 'ctox-writeback');
  out = await run(ctx, 'ctox.ticket.writeback_transition', { case_id: 'case-1', state: 'closed', body: 'Danke' }, { id: 'w2' });
  assert.equal(out.case.state, 'closed');
  assert.equal(out.case.closed_at, ISO);
  assert.equal(ctx.$.doc('ctox_ticket_items', 'local:LT-aaa').remote_status, 'closed');
  const writebacks = ctx.$.all('ctox_ticket_writebacks');
  assert.deepEqual(writebacks.map((w) => [w.operation, w.status]).sort(), [['comment', 'ok'], ['transition', 'ok']]);
  assert.deepEqual(audits(ctx).map((a) => a.payload.action_type), [
    'approval_decision', 'execution_case', 'verification_record', 'writeback_record', 'writeback_record', 'case_closed',
  ]);
  assert.deepEqual(audits(ctx)[0].payload.details, { status: 'approved', decided_by: 'owner', rationale: null });
  assert.equal(ctx.$.capsTo('ctox.ticket.run').length, 0);
});

test('approve rejected → blocked, verify failed → blocked; unbekannte Status wie Rust', async () => {
  const ctx = world();
  assert.equal((await run(ctx, 'ctox.ticket.approve', { case_id: 'case-1', status: 'rejected', rationale: ' nein ' })).case.state, 'blocked');
  await assert.rejects(run(ctx, 'ctox.ticket.approve', { case_id: 'case-1', status: 'maybe' }), { code: 'invalid', message: 'unsupported approval status: maybe' });
  await assert.rejects(run(ctx, 'ctox.ticket.approve', { case_id: 'nope', status: 'approved' }), { code: 'not_found', message: 'ticket case not found' });
  await assert.rejects(run(ctx, 'ctox.ticket.approve', { case_id: 'case-1' }), { code: 'invalid', message: 'status is required' });
  const exec = world({ state: 'executing' });
  assert.equal((await run(exec, 'ctox.ticket.verify', { case_id: 'case-1', status: 'failed', summary: 'rot' })).case.state, 'blocked');
  await assert.rejects(run(exec, 'ctox.ticket.verify', { case_id: 'case-1', status: 'ok' }), { code: 'invalid', message: 'unsupported verification status: ok' });
});

// ------------------------------------------------------------------------------------------- Invarianten

test('INV-1 Übergangskatalog: approve aus writeback_pending (Verified→Planned) scheitert atomar', async () => {
  const ctx = world({ state: 'writeback_pending' });
  await assert.rejects(run(ctx, 'ctox.ticket.approve', { case_id: 'case-1', status: 'approved' }), (err) => {
    assert.equal(err.code, 'conflict');
    assert.match(err.message, /^Diese Aktion wurde noch nicht ausgefuehrt, weil der abgesicherte Arbeitsablauf unvollstaendig ist\. Bleib im erlaubten Arbeitsablauf/);
    assert.deepEqual(err.violations, ['invalid_transition']);
    return true;
  });
  assert.equal(caseOf(ctx).state, 'writeback_pending');
  assert.equal(ctx.$.all('ctox_ticket_approvals').length, 0, 'keine Freigabe ohne Übergang');
  assert.equal(audits(ctx).length, 0);
});

test('INV-2 execute nur aus executable/executing', async () => {
  const ctx = world({ state: 'approval_pending' });
  await assert.rejects(run(ctx, 'ctox.ticket.execute', { case_id: 'case-1', summary: 'los' }), {
    code: 'conflict', message: 'case case-1 is not executable; current state is approval_pending',
  });
  assert.equal(caseOf(ctx).state, 'approval_pending');
});

test('INV-3 close braucht bestandene Prüfung (closure_requires_verification) und Review-Beleg', async () => {
  const ctx = world({ state: 'verified' });
  await assert.rejects(run(ctx, 'ctox.ticket.close', { case_id: 'case-1' }), (err) => {
    assert.equal(err.code, 'conflict');
    assert.ok(err.violations.includes('closure_requires_verification'));
    assert.ok(err.violations.includes('owner_visible_completion_requires_review'));
    assert.ok(err.message.includes('Schliesse die Aufgabe noch nicht. Verifiziere das Ergebnis zuerst'));
    return true;
  });
  assert.equal(caseOf(ctx).state, 'verified');
  const ok = world({
    state: 'verified',
    extraSeed: { ctox_ticket_verifications: [{ id: 'verification:case-1:x', verification_id: 'verification:case-1:x', case_id: 'case-1', status: 'passed', summary: null, created_at: ISO, updated_at_ms: NOW }] },
  });
  const closed = await run(ok, 'ctox.ticket.close', { case_id: 'case-1', summary: ' fertig ' });
  assert.equal(closed.case.state, 'closed');
  assert.deepEqual(audits(ok)[0].payload.details, { summary: 'fertig' });
});

test('INV-4 close aus executing verletzt den Katalog (Executing→Closed nicht erlaubt)', async () => {
  const ctx = world({
    state: 'executing',
    extraSeed: { ctox_ticket_verifications: [{ id: 'v', verification_id: 'v', case_id: 'case-1', status: 'passed', created_at: ISO, updated_at_ms: NOW }] },
  });
  await assert.rejects(run(ctx, 'ctox.ticket.close', { case_id: 'case-1' }), (err) => err.code === 'conflict' && err.violations.includes('invalid_transition'));
});

test('INV-5 Rückschrieb nur aus writeback_pending; unbekannter Zustand scheitert geschlossen', async () => {
  const ctx = world({ state: 'executing' });
  await assert.rejects(run(ctx, 'ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'x' }), {
    code: 'conflict', message: 'case case-1 is not ready for writeback; current state is executing',
  });
  const weird = world({ state: 'limbo' });
  await assert.rejects(run(weird, 'ctox.ticket.approve', { case_id: 'case-1', status: 'approved' }), {
    code: 'invalid', message: 'ticket case state is not recognized: limbo',
  });
  const unknown = world({ state: 'writeback_pending', system: 'jira' });
  await assert.rejects(run(unknown, 'ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'x' }), {
    code: 'invalid', message: 'unsupported ticket system for writeback: jira',
  });
});

// ------------------------------------------------------------------------------------------- Rückfragen

test('Rückfrage: anlegen (Fall blockiert), veröffentlichen (lokal), auflösen (Fall zurück nach executable)', async () => {
  const ctx = world({
    state: 'executable',
    extraSeed: { ctox_ticket_events: [inbound('local:LE-2', 'local:LT-aaa', '2026-10-15T09:00:00.000Z'), inbound('local:LE-1', 'local:LT-aaa', '2026-10-15T08:00:00.000Z')] },
  });
  const created = await run(ctx, 'ctox.ticket.request_clarification', {
    case_id: 'case-1', question: 'Welche Version?', missing_inputs_csv: 'Version, ,version,Seriennr',
  }, { id: 'q1' });
  assert.equal(created.clarification.status, 'draft');
  assert.deepEqual(created.clarification.missing_inputs, ['Version', 'Seriennr']);
  assert.equal(created.clarification.target_type, 'requester');
  assert.equal(created.clarification.resume_state, 'executable');
  assert.equal(caseOf(ctx).state, 'blocked_needs_clarification');
  const id = created.clarification_id;
  const published = await run(ctx, 'ctox.ticket.publish_clarification', { clarification_id: id }, { id: 'p1' });
  assert.equal(published.clarification.status, 'waiting_for_response');
  assert.match(published.clarification.outbound_message_key, /^LE-[0-9a-f]{12}$/);
  assert.deepEqual(published.clarification.metadata.response_baseline_event_keys, ['local:LE-1', 'local:LE-2']);
  assert.equal(published.clarification.metadata.review_summary, 'Clarification question reviewed for this ticket.');
  await assert.rejects(run(ctx, 'ctox.ticket.publish_clarification', { clarification_id: id }), {
    code: 'conflict', message: `clarification ${id} cannot be published from status waiting_for_response`,
  });
  const resolved = await run(ctx, 'ctox.ticket.resolve_clarification', { clarification_id: id, response_key: 'local:LE-9', body: ' 1.2 ' });
  assert.equal(resolved.clarification.status, 'resolved');
  assert.equal(resolved.clarification.inbound_response_body, '1.2');
  assert.equal(caseOf(ctx).state, 'executable');
  assert.deepEqual(audits(ctx).map((a) => a.payload.action_type), ['clarification_requested', 'clarification_published', 'clarification_resolved']);
});

test('Rückfrage: Prüfungen wie Rust (Schlüssel passt nicht, Endzustand, Ziel, Kanal, fehlende Angaben)', async () => {
  const ctx = world({ state: 'writeback_pending' });
  await assert.rejects(run(ctx, 'ctox.ticket.request_clarification', { case_id: 'case-1', ticket_key: 'local:LT-other', question: 'q' }), {
    code: 'invalid', message: 'clarification ticket_key local:LT-other does not match case case-1 ticket_key local:LT-aaa',
  });
  await assert.rejects(run(ctx, 'ctox.ticket.request_clarification', { case_id: 'case-1', question: 'q' }), {
    code: 'conflict', message: 'case case-1 cannot request clarification from terminal/writeback state writeback_pending',
  });
  await assert.rejects(run(ctx, 'ctox.ticket.request_clarification', { question: 'q' }), { code: 'invalid', message: 'case_id or ticket_key is required for clarification' });
  await assert.rejects(run(ctx, 'ctox.ticket.request_clarification', { ticket_key: 'local:LT-zzz', question: 'q' }), { code: 'not_found', message: 'ticket not found for clarification' });
  await assert.rejects(run(ctx, 'ctox.ticket.request_clarification', { ticket_key: 'local:LT-aaa', question: 'q', target_type: 'boss' }), {
    code: 'invalid', message: "unsupported clarification target_type 'boss' (expected requester|owner|internal)",
  });
  await assert.rejects(run(ctx, 'ctox.ticket.request_clarification', { ticket_key: 'local:LT-aaa', question: 'q', target_channel: 'fax' }), {
    code: 'invalid', message: "unsupported clarification target_channel 'fax' (expected ticket|email|jami|tui|teams|whatsapp)",
  });
  const owner = await run(ctx, 'ctox.ticket.request_clarification', { ticket_key: 'local:LT-aaa', question: 'q', target_type: 'founder', missing_inputs: ['x', 3] });
  assert.equal(owner.clarification.target_type, 'owner');
  assert.deepEqual(owner.clarification.missing_inputs, ['x']);
  assert.equal(owner.case_id, null);
  await assert.rejects(run(ctx, 'ctox.ticket.publish_clarification', { clarification_id: owner.clarification_id }), {
    code: 'invalid', message: 'automatic clarification publish currently supports requester ticket comments only',
  });
  await assert.rejects(run(ctx, 'ctox.ticket.request_clarification', { case_id: 'case-1' }), { code: 'invalid', message: 'question is required' });
});

// ------------------------------------------------------------------------------------------- Fremdsystem über agent.task

const complete = (ctx, taskId, result = {}, id = `done-${taskId}`) => run(ctx, 'ctox.ticket.writeback.complete', { task_id: taskId, status: 'completed', result }, { id });
const failed = (ctx, taskId, error, id = `fail-${taskId}`) => run(ctx, 'ctox.ticket.writeback.failed', { task_id: taskId, status: 'failed', result: null, error }, { id });

test('zammad: Rückschrieb als agent.task, Abschlussbefehle (Kern §16.30): Kommentar ok, Übergang closed, Fehler failed', async () => {
  const ctx = world({
    state: 'writeback_pending', system: 'zammad',
    extraSeed: { ctox_ticket_verifications: [{ id: 'v', verification_id: 'v', case_id: 'case-1', status: 'passed', created_at: ISO, updated_at_ms: NOW }] },
  });
  const comment = await run(ctx, 'ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'Hallo', internal: true }, { id: 'w1' });
  assert.equal(comment.writeback_status, 'queued');
  assert.equal(comment.task_id, 'task-1');
  const [enqueue] = ctx.$.capsTo('agent.task.enqueue').map((c) => c.args[0]);
  assert.equal(enqueue.on_complete, 'ctox.ticket.writeback.complete');
  assert.equal(enqueue.on_failed, 'ctox.ticket.writeback.failed');
  assert.equal(enqueue.idempotency_key, `ticket-writeback:${comment.writeback_id}`);
  assert.equal(enqueue.payload.remote_ticket_id, '4711');
  assert.equal(ctx.$.doc('ctox_ticket_writebacks', comment.writeback_id).status, 'queued');
  assert.equal((await complete(ctx, 'task-1', { remote_event_ids: ['99'] })).status, 'ok');
  const okRow = ctx.$.doc('ctox_ticket_writebacks', comment.writeback_id);
  assert.equal(okRow.status, 'ok');
  assert.deepEqual(okRow.payload, { body: 'Hallo', internal: true, remote_event_ids: ['99'] });

  const transition = await run(ctx, 'ctox.ticket.writeback_transition', { case_id: 'case-1', state: 'closed' }, { id: 'w2' });
  assert.equal(transition.case.state, 'writeback_pending', 'Fall schließt erst nach dem Rückschrieb');
  assert.equal(transition.task_id, 'task-2');
  await complete(ctx, 'task-2');
  assert.equal(caseOf(ctx).state, 'closed');
  assert.deepEqual(audits(ctx).map((a) => a.payload.action_type), ['writeback_record', 'writeback_record', 'case_closed']);

  const failing = world({ state: 'writeback_pending', system: 'zammad' });
  const queued = await run(failing, 'ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'x' });
  const out = await failed(failing, 'task-1', 'HTTP 502');
  assert.equal(out.status, 'failed');
  const failedRow = failing.$.doc('ctox_ticket_writebacks', queued.writeback_id);
  assert.equal(failedRow.status, 'failed');
  assert.deepEqual(failedRow.payload, { payload: { body: 'x', internal: false }, error: 'HTTP 502' });
  assert.equal(audits(failing)[0].payload.action_type, 'writeback_failed');
  await assert.rejects(complete(failing, 'task-unknown'), { code: 'not_found', message: 'no ticket writeback for task task-unknown' });
  await assert.rejects(run(failing, 'ctox.ticket.writeback.complete', { status: 'completed' }), { code: 'invalid', message: 'task_id is required' });
});

test('Doppelzustellung: zweiter Abschlussbefehl je task_id ändert nichts (kein Schreibzugriff, kein Audit)', async () => {
  const ctx = world({
    state: 'writeback_pending', system: 'zammad',
    extraSeed: { ctox_ticket_verifications: [{ id: 'v', verification_id: 'v', case_id: 'case-1', status: 'passed', created_at: ISO, updated_at_ms: NOW }] },
  });
  const transition = await run(ctx, 'ctox.ticket.writeback_transition', { case_id: 'case-1', state: 'closed' }, { id: 'w1' });
  assert.equal((await complete(ctx, 'task-1', {}, 'core-1')).status, 'ok');
  const writes = ctx.$.writes.length;
  const auditCount = audits(ctx).length;
  const before = ctx.$.doc('ctox_ticket_writebacks', transition.writeback_id);
  assert.deepEqual(await complete(ctx, 'task-1', { remote_event_ids: ['other'] }, 'core-2'), { status: 'ignored', writeback_id: transition.writeback_id, task_id: 'task-1' });
  assert.deepEqual(await failed(ctx, 'task-1', 'spät', 'core-3'), { status: 'ignored', writeback_id: transition.writeback_id, task_id: 'task-1' });
  assert.equal(ctx.$.writes.length, writes, 'keine Schreibvorgänge bei Doppelzustellung');
  assert.equal(audits(ctx).length, auditCount, 'kein zweites Audit');
  assert.deepEqual(ctx.$.doc('ctox_ticket_writebacks', transition.writeback_id), before);
  assert.equal(caseOf(ctx).state, 'closed');

  const f = world({ state: 'writeback_pending', system: 'zammad' });
  await run(f, 'ctox.ticket.writeback_comment', { case_id: 'case-1', body: 'x' });
  await failed(f, 'task-1', 'HTTP 502', 'core-1');
  const fWrites = f.$.writes.length;
  assert.equal((await failed(f, 'task-1', 'HTTP 502', 'core-2')).status, 'ignored');
  assert.equal(f.$.writes.length, fWrites);
  assert.equal(audits(f).length, 1);
});

test('zammad: Rückfrage-Veröffentlichung über agent.task, Fehler → send_failed, erneut → waiting_for_response', async () => {
  const ctx = world({ state: 'executable', system: 'zammad' });
  const created = await run(ctx, 'ctox.ticket.request_clarification', { case_id: 'case-1', question: 'Version?' });
  const published = await run(ctx, 'ctox.ticket.publish_clarification', { clarification_id: created.clarification_id }, { id: 'p1' });
  assert.equal(published.writeback_status, 'queued');
  assert.equal(published.clarification.status, 'draft');
  await failed(ctx, published.task_id, 'timeout');
  assert.equal(ctx.$.doc('ctox_ticket_clarification_requests', created.clarification_id).status, 'send_failed');
  const retry = await run(ctx, 'ctox.ticket.publish_clarification', { clarification_id: created.clarification_id }, { id: 'p2' });
  assert.equal(retry.writeback_status, 'queued');
  await complete(ctx, retry.task_id, { remote_event_ids: ['77'] });
  const sent = ctx.$.doc('ctox_ticket_clarification_requests', created.clarification_id);
  assert.equal(sent.status, 'waiting_for_response');
  assert.equal(sent.outbound_message_key, '77');
  assert.equal(audits(ctx).at(-1).payload.action_type, 'clarification_published');
});

// ------------------------------------------------------------------------------------------- Abnahme-Grep

test('kein Handler ruft ctox.ticket.run (Capability ist in die App verlagert)', () => {
  const dir = fileURLToPath(new URL('..', import.meta.url));
  for (const file of readdirSync(dir).filter((f) => f.endsWith('.js'))) {
    assert.ok(!readFileSync(`${dir}/${file}`, 'utf8').includes('ctox.ticket.run'), file);
  }
});
