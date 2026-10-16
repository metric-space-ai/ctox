// Tests: thread.watch|unwatch|archive|snooze|state.update|assign|claim, handoff.create, ai.request,
// link.create|remove, note.update|delete, notification.preferences.update.
import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, cmd, USERS, rejects } from './_ctx.mjs';
import watch from '../threads.thread.watch.js';
import unwatch from '../threads.thread.unwatch.js';
import archive from '../threads.thread.archive.js';
import snooze from '../threads.thread.snooze.js';
import stateUpdate from '../threads.thread.state.update.js';
import assign from '../threads.thread.assign.js';
import claim from '../threads.thread.claim.js';
import handoff from '../threads.handoff.create.js';
import aiRequest from '../threads.ai.request.js';
import linkCreate from '../threads.link.create.js';
import linkRemove from '../threads.link.remove.js';
import noteCreate from '../threads.note.create.js';
import noteUpdate from '../threads.note.update.js';
import noteDelete from '../threads.note.delete.js';
import prefs, { validQuietTime } from '../threads.notification.preferences.update.js';
import { threadStateId } from '../_lib.js';

const ALICE = { id: 'alice', role: 'user', display_name: 'Alice' };
const BOB = { id: 'bob', role: 'user', display_name: 'Bob' };
const CAROL = { id: 'carol', role: 'user' };
const ADMIN = { id: 'admin', role: 'chef' };

function seedThread(extra = {}) {
  return { user_threads: [{ id: 't1', thread_id: 't1', title: 'Fall', kind: 'case', status: 'open', participant_ids: ['alice', 'bob'],
    owner_user_id: 'alice', assigned_user_id: '', source_module: 'support', source_record_type: 'conversation',
    source_record_id: 'c1', source_label: 'Kunde', created_at_ms: 10, updated_at_ms: 10, ...extra }] };
}

test('watch/unwatch: Beobachter + Teilnehmer, sortiert; Nicht-Teilnehmer abgewiesen', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  await rejects(watch(cmd('threads.thread.watch', { thread_id: 't1' }, CAROL), h.ctx), 'forbidden');
  const r = await watch(cmd('threads.thread.watch', { thread_id: 't1' }, BOB), h.ctx);
  assert.equal(r.watching, true);
  assert.deepEqual(h.doc('user_threads', 't1').watcher_user_ids, ['bob']);
  await watch(cmd('threads.thread.watch', { thread_id: 't1' }, ADMIN), h.ctx);
  assert.deepEqual(h.doc('user_threads', 't1').participant_ids, ['admin', 'alice', 'bob']);
  assert.deepEqual(h.doc('user_threads', 't1').watcher_user_ids, ['admin', 'bob']);
  await unwatch(cmd('threads.thread.unwatch', {}, BOB, { record_id: 't1' }), h.ctx);
  assert.deepEqual(h.doc('user_threads', 't1').watcher_user_ids, ['admin']);
  assert.ok(h.doc('user_thread_states', threadStateId('admin', 't1')));
});

test('archive: Status archived, Übergabe-Gründe entfallen', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread({ assigned_user_id: 'bob' }) });
  await archive(cmd('threads.thread.archive', { thread_id: 't1' }, ALICE), h.ctx);
  const t = h.doc('user_threads', 't1');
  assert.equal(t.status, 'archived');
  assert.equal(t.archived_at_ms, h.ctx.now());
  assert.deepEqual(h.doc('user_thread_states', threadStateId('bob', 't1')).attention_reasons, []);
  await rejects(archive(cmd('threads.thread.archive', { thread_id: 't1' }, CAROL), h.ctx), 'forbidden', 'only participants or admins can archive this thread');
});

test('snooze: Zeitpunkt muss in der Zukunft liegen; until_ms als Alias', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  const now = h.ctx.now();
  await rejects(snooze(cmd('threads.thread.snooze', { thread_id: 't1', snoozed_until_ms: now }, ALICE), h.ctx), 'invalid', 'future snoozed_until_ms is required');
  await rejects(snooze(cmd('threads.thread.snooze', { thread_id: 't1', snoozed_until_ms: null, until_ms: now + 5 }, ALICE), h.ctx), 'invalid');
  const r = await snooze(cmd('threads.thread.snooze', { thread_id: 't1', until_ms: now + 5000 }, ALICE), h.ctx);
  assert.equal(r.snoozed_until_ms, now + 5000);
  assert.equal(h.doc('user_threads', 't1').status, 'snoozed');
});

test('state.update: übernimmt Felder, prüft Priorität, refresh überschreibt Zähler', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  await rejects(stateUpdate(cmd('threads.thread.state.update', { thread_id: 't1', priority: 'mega' }, ALICE), h.ctx), 'invalid', 'invalid priority');
  const r = await stateUpdate(cmd('threads.thread.state.update', { thread_id: 't1', pinned: true, priority: 'high', unread_count: 7, follow_up_at_ms: 99 }, ALICE), h.ctx);
  const st = h.doc('user_thread_states', r.state_id);
  assert.equal(r.state_id, 'thread_state_alice_t1');
  assert.equal(st.pinned, true);
  assert.equal(st.priority, 'high');
  assert.equal(st.follow_up_at_ms, 99);
  assert.equal(st.unread_count, 0);
});

test('assign/claim: aktiver Nutzer, Versionsprüfung, schon beansprucht', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  await rejects(assign(cmd('threads.thread.assign', { thread_id: 't1', assigned_user_id: 'ghost' }, ALICE), h.ctx), 'invalid', 'not an active Business OS user');
  await rejects(assign(cmd('threads.thread.assign', { thread_id: 't1', assigned_user_id: 'carol', expected_updated_at_ms: 9 }, ALICE), h.ctx), 'conflict', 'thread changed; refresh before assigning');
  await assign(cmd('threads.thread.assign', { thread_id: 't1', assigned_user_id: 'carol', expected_updated_at_ms: 10, due_at_ms: -5, next_step: ' Anrufen ' }, ALICE), h.ctx);
  let t = h.doc('user_threads', 't1');
  assert.equal(t.assigned_user_id, 'carol');
  assert.deepEqual(t.participant_ids, ['alice', 'bob', 'carol']);
  assert.equal(t.due_at_ms, 0);
  assert.equal(t.next_step, 'Anrufen');
  assert.deepEqual(h.doc('user_thread_states', threadStateId('carol', 't1')).attention_reasons, ['Zugewiesen']);
  await rejects(claim(cmd('threads.thread.claim', { thread_id: 't1' }, BOB), h.ctx), 'conflict', 'thread was already claimed');
  await claim(cmd('threads.thread.claim', { thread_id: 't1' }, CAROL), h.ctx);
  t = h.doc('user_threads', 't1');
  assert.equal(t.assigned_user_id, 'carol');
});

// handoff_rejects_invalid_target_and_stale_thread_before_writing_message (threads.rs:5730)
test('handoff: ungültiges Ziel / veraltete Version schreiben nichts', async () => {
  const h = makeCtx({ users: USERS });
  await noteCreate(cmd('threads.note.create', { thread_id: 'handoff-thread', body: 'Initial note',
    source_context: { module: 'threads', record_type: 'thread', record_id: 'handoff-thread' } }, ALICE, { record_id: 'handoff-thread' }), h.ctx);
  const version = h.doc('user_threads', 'handoff-thread').updated_at_ms;
  h.tick(10);
  for (const [messageId, target, expected] of [['msg-handoff-unknown', 'unknown', version], ['msg-handoff-stale', 'bob', version - 1]]) {
    await assert.rejects(handoff(cmd('threads.handoff.create', { thread_id: 'handoff-thread', message_id: messageId,
      target_user_id: target, expectation: 'Follow up', expected_updated_at_ms: expected }, ALICE), h.ctx));
    assert.equal(h.doc('user_thread_messages', messageId), null);
    const t = h.doc('user_threads', 'handoff-thread');
    assert.equal(t.updated_at_ms, version);
    assert.equal(t.assigned_user_id, '');
  }
});

test('handoff: Nachricht + Zuweisung + Übergabe an dich (65)', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  const r = await handoff(cmd('threads.handoff.create', { thread_id: 't1', message_id: 'msg-h', target_user_id: 'bob',
    expectation: 'Kunde anrufen', return_reason: 'nicht erreichbar', due_at_ms: 500, expected_updated_at_ms: 10 }, ALICE), h.ctx);
  assert.equal(r.ok, true);
  const m = h.doc('user_thread_messages', 'msg-h');
  assert.equal(m.kind, 'handoff');
  assert.equal(m.body, 'Kunde anrufen\n\nRückgabe wenn: nicht erreichbar');
  assert.deepEqual(m.target_user_ids, ['bob']);
  const t = h.doc('user_threads', 't1');
  assert.equal(t.assigned_user_id, 'bob');
  assert.equal(t.next_step, 'Kunde anrufen');
  assert.equal(t.due_at_ms, 500);
  assert.equal(r.assignment.assigned_user_id, 'bob');
  const bob = h.doc('user_thread_states', threadStateId('bob', 't1'));
  assert.deepEqual(bob.attention_reasons, ['Übergabe an dich']);
  assert.equal(bob.attention_score, 65);
});

// SKL-A2: kein business_os.chat.task mehr, eigener Task mit Rückruf (Details agent.test.mjs).
test('ai.request: Nachricht ai_request + agent.task.enqueue mit Rückruf', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  const r = await aiRequest(cmd('threads.ai.request', { thread_id: 't1', goal: 'Fasse den Fall zusammen', target_user_ids: ['bob'] }, ALICE), h.ctx);
  const m = h.doc('user_thread_messages', r.message.message_id);
  assert.equal(m.kind, 'ai_request');
  assert.equal(m.execution_status, 'requested');
  assert.deepEqual(m.target_user_ids, []);
  assert.equal(h.calls.commands.length, 0);
  assert.equal(h.sim.enqueued.length, 1);
  const { args } = h.sim.enqueued[0];
  assert.equal(args.thread_key, 'business-os/threads/t1');
  assert.equal(args.title, 'AI: Fasse den Fall zusammen');
  assert.equal(args.payload.context.module, 'support');
  assert.equal(args.payload.module, 'support');
  assert.equal(args.payload.record_id, 'c1');
  assert.equal(args.on_complete, 'threads.ai.complete');
  assert.equal(args.on_failed, 'threads.ai.failed');
  // risk_class ≠ internal: Fehler NACH dem Schreiben der Nachricht (Rust-Verhalten)
  await rejects(aiRequest(cmd('threads.ai.request', { thread_id: 't1', goal: 'Mail senden', message_id: 'msg-risk', risk_class: 'external' }, ALICE), h.ctx), 'invalid', 'non-internal AI actions');
  assert.ok(h.doc('user_thread_messages', 'msg-risk'));
  assert.equal(h.sim.enqueued.length, 1);
});

test('link.create/remove: Standard-ID, Rolle, Soft-Delete', async () => {
  const h = makeCtx({ users: USERS, seed: seedThread() });
  const r = await linkCreate(cmd('threads.link.create', { thread_id: 't1', source_context: { module: 'documents', record_type: 'document', record_id: 'd 7' }, task_id: 'task-1' }, ALICE), h.ctx);
  assert.equal(r.link_id, 'link_t1_documents_d_7');
  const l = h.doc('user_thread_links', r.link_id);
  assert.equal(l.link_role, 'related');
  assert.equal(l.task_id, 'task-1');
  assert.deepEqual(h.calls.policy, [{ permission: 'data.read', module: 'documents' }]);
  await rejects(linkRemove(cmd('threads.link.remove', { link_id: r.link_id }, CAROL), h.ctx), 'forbidden');
  await linkRemove(cmd('threads.link.remove', { link_id: r.link_id }, BOB), h.ctx);
  assert.equal(h.doc('user_thread_links', r.link_id).is_deleted, true);
  await rejects(linkRemove(cmd('threads.link.remove', { link_id: 'nope' }, BOB), h.ctx), 'not_found', 'thread link nope not found');
});

test('note.update/delete: nur Autor/Admin; Erwähnung neu; Soft-Delete fällt aus Zählung', async () => {
  const h = makeCtx({ users: USERS });
  await noteCreate(cmd('threads.note.create', { thread_id: 'tn', message_id: 'm1', body: 'alt' }, ALICE), h.ctx);
  await rejects(noteUpdate(cmd('threads.note.update', { message_id: 'm1', body: 'x' }, BOB), h.ctx), 'forbidden', 'only message author or admin');
  await rejects(noteUpdate(cmd('threads.note.update', { message_id: 'zz', body: 'x' }, ALICE), h.ctx), 'not_found', 'thread message zz not found');
  await noteUpdate(cmd('threads.note.update', { note_id: 'm1', body: 'neu', target_user_ids: ['bob'] }, ALICE), h.ctx);
  const m = h.doc('user_thread_messages', 'm1');
  assert.equal(m.body, 'neu');
  assert.deepEqual(m.target_user_ids, ['bob']);
  const n = h.doc('user_notifications', 'notif_m1_bob');
  assert.equal(n.notification_type, 'mention');
  assert.equal(n.title, 'Thread aktualisiert');
  const bobState = h.doc('user_thread_states', threadStateId('bob', 'tn'));
  assert.deepEqual(bobState.attention_reasons, ['Erwähnung']);
  await noteDelete(cmd('threads.note.delete', {}, ADMIN, { record_id: 'm1' }), h.ctx);
  assert.equal(h.doc('user_thread_messages', 'm1').is_deleted, true);
  // Rust löscht die Benachrichtigung zur gelöschten Notiz nicht.
  assert.equal(h.doc('user_notifications', 'notif_m1_bob').status, 'unread');
});

test('preferences.update: Schwelle, Ruhezeiten, eigener Zustand', async () => {
  const h = makeCtx({ users: USERS });
  assert.equal(validQuietTime(''), true);
  assert.equal(validQuietTime('22:00'), true);
  assert.equal(validQuietTime('24:00'), false);
  assert.equal(validQuietTime('7:00'), false);
  assert.equal(validQuietTime('+7:00'), true);
  assert.equal(validQuietTime('07:60'), false);
  await rejects(prefs(cmd('threads.notification.preferences.update', { priority_threshold: 101 }, ALICE), h.ctx), 'invalid', 'priority threshold must be 0..100');
  await rejects(prefs(cmd('threads.notification.preferences.update', { quiet_start: '25:00' }, ALICE), h.ctx), 'invalid', 'invalid quiet_start');
  const r = await prefs(cmd('threads.notification.preferences.update', { priority_threshold: 50, quiet_start: '22:00', quiet_end: '07:00', notification_types: ['mention', 3, ' note '] }, ALICE), h.ctx);
  assert.equal(r.state_id, 'thread_state_alice_preferences');
  const st = h.doc('user_thread_states', r.state_id);
  assert.equal(st.thread_id, '__preferences__');
  assert.deepEqual(st.notification_preferences, { priority_threshold: 50, quiet_start: '22:00', quiet_end: '07:00', notification_types: ['mention', 'note'] });
  const r2 = await prefs(cmd('threads.notification.preferences.update', { priority_threshold: 1.5 }, ALICE), h.ctx);
  assert.equal(h.doc('user_thread_states', r2.state_id).notification_preferences.priority_threshold, 20);
});
