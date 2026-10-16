// Tests: threads.note.create, threads.message.create, threads.notification.mark_read|dismiss.
import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, cmd, USERS, rejects } from './_ctx.mjs';
import noteCreate from '../threads.note.create.js';
import messageCreate from '../threads.message.create.js';
import markRead from '../threads.notification.mark_read.js';
import dismiss from '../threads.notification.dismiss.js';
import { threadStateId, slugPart, truncate } from '../_lib.js';

const BOB = { id: 'bob', role: 'user', display_name: 'Bob' };
const ALICE = { id: 'alice', role: 'user', display_name: 'Alice' };

test('slug_part und truncate wie Rust', () => {
  assert.equal(slugPart('Ä b__c/'), 'b_c');
  assert.equal(slugPart('***'), 'item');
  assert.equal(slugPart('x'.repeat(100)).length, 80);
  assert.equal(truncate('  äöü  ', 2), 'äö');
});

// Rust-Test mark_read_recomputes_unread_count_and_attention_score (threads.rs:6048)
test('note.create (mention) → Erwähnung 60; mark_read → 0', async () => {
  const h = makeCtx({ users: USERS });
  const out = await noteCreate(cmd('threads.note.create', {
    thread_id: 'thread-attention-mention', message_id: 'msg-attention-mention', message_type: 'mention',
    body: '@Alice bitte prüfen.', target_user_ids: ['alice'],
    source_context: { module: 'threads', record_type: 'thread', record_id: 'thread-attention-mention' },
  }, BOB, { record_id: 'thread-attention-mention' }), h.ctx);
  assert.equal(out.ok, true);
  assert.equal(out.thread_id, 'thread-attention-mention');
  const sid = threadStateId('alice', 'thread-attention-mention');
  let st = h.doc('user_thread_states', sid);
  assert.equal(st.unread_count, 1);
  assert.deepEqual(st.attention_reasons, ['Erwähnung']);
  assert.equal(st.attention_score, 60);
  const notif = h.doc('user_notifications', 'notif_msg-attention-mention_alice');
  assert.equal(notif.notification_type, 'mention');
  assert.equal(notif.status, 'unread');
  const thread = h.doc('user_threads', 'thread-attention-mention');
  assert.deepEqual(thread.participant_ids, ['alice', 'bob']);
  assert.equal(thread.kind, 'mention');
  assert.equal(thread.owner_user_id, 'bob');
  assert.equal(h.doc('user_thread_messages', 'msg-attention-mention').author_display_name, 'Bob');
  // Quelle module=threads → keine Policy-Abfrage
  assert.equal(h.calls.policy.length, 0);
  assert.ok(h.doc('user_thread_links', 'link_thread-attention-mention_threads_thread-attention-mention'));

  h.tick(5);
  const r = await markRead(cmd('threads.notification.mark_read', { notification_id: 'notif_msg-attention-mention_alice' }, ALICE), h.ctx);
  assert.equal(r.status, 'read');
  st = h.doc('user_thread_states', sid);
  assert.equal(st.unread_count, 0);
  assert.deepEqual(st.attention_reasons, []);
  assert.equal(st.attention_score, 0);
  assert.equal(st.last_seen_at_ms, h.ctx.now());
});

// Rust-Test addressed_note_preserves_thread_assignment_and_source_state (threads.rs:6126)
test('note.create erhält Titel/Art/Status/Zuständigen; Blockiert für Zuständigen', async () => {
  const id = 'thread-note-preserved-source';
  const h = makeCtx({
    users: USERS,
    seed: { user_threads: [{ id, thread_id: id, title: 'Existing case', kind: 'case', status: 'blocked',
      participant_ids: ['bob', 'alice'], owner_user_id: 'bob', assigned_user_id: 'bob', created_at_ms: 42, updated_at_ms: 42 }] },
  });
  await noteCreate(cmd('threads.note.create', {
    thread_id: id, message_id: 'msg-note-preserved-source', message_type: 'mention',
    body: '@Alice please check the existing case.', target_user_ids: ['alice'],
    source_context: { module: 'threads', record_type: 'case', record_id: 'case-note-preserved' },
  }, BOB, { record_id: id }), h.ctx);
  const thread = h.doc('user_threads', id);
  assert.equal(thread.title, 'Existing case');
  assert.equal(thread.kind, 'case');
  assert.equal(thread.status, 'blocked');
  assert.equal(thread.assigned_user_id, 'bob');
  assert.equal(thread.created_at_ms, 42);
  const aliceId = threadStateId('alice', id);
  assert.deepEqual(h.doc('user_thread_states', aliceId).attention_reasons, ['Erwähnung']);
  await markRead(cmd('threads.notification.mark_read', { notification_id: 'notif_msg-note-preserved-source_alice' }, ALICE), h.ctx);
  const alice = h.doc('user_thread_states', aliceId);
  assert.deepEqual(alice.attention_reasons, []);
  assert.equal(alice.attention_score, 0);
  const bob = h.doc('user_thread_states', threadStateId('bob', id));
  assert.deepEqual(bob.attention_reasons, ['Blockiert']);
  assert.equal(bob.attention_score, 70);
});

test('note.create ohne thread_id leitet Thread-ID aus Quelle ab und prüft Quell-Policy', async () => {
  const h = makeCtx({ users: USERS });
  const out = await noteCreate(cmd('threads.note.create', {
    body: 'Hinweis', source_context: { module: 'outbound', record_type: 'lead', record_id: 'L 1', label: 'ACME' },
  }, ALICE), h.ctx);
  assert.equal(out.thread_id, 'thread_outbound_lead_L_1');
  assert.deepEqual(h.calls.policy, [{ permission: 'data.read', module: 'outbound' }]);
  const t = h.doc('user_threads', out.thread_id);
  assert.equal(t.title, 'ACME');
  assert.equal(t.kind, 'note');
  assert.equal(t.status, 'open');
  assert.equal(t.source_label, 'ACME');
  // Ersteller ohne Adressaten: kein Zustand außer für alice selbst (Teilnehmer), Grund leer
  const st = h.doc('user_thread_states', threadStateId('alice', out.thread_id));
  assert.equal(st.unread_count, 0);
  assert.deepEqual(st.attention_reasons, []);
});

test('note.create: Policy verweigert Quelle → forbidden, nichts geschrieben', async () => {
  const h = makeCtx({ users: USERS, policy: () => ({ allowed: false, display_reason: 'no data.read' }) });
  await rejects(noteCreate(cmd('threads.note.create', {
    body: 'x', source_context: { module: 'outbound', record_id: 'L1' },
  }, ALICE), h.ctx), 'forbidden', 'not readable by actor: no data.read');
  assert.equal(h.all('user_threads').length, 0);
});

test('note.create: Nicht-Teilnehmer an bestehendem Thread → forbidden', async () => {
  const h = makeCtx({ users: USERS, seed: { user_threads: [{ id: 't1', title: 'T', status: 'open', participant_ids: ['bob'], updated_at_ms: 1 }] } });
  await rejects(noteCreate(cmd('threads.note.create', { thread_id: 't1', body: 'x' }, ALICE), h.ctx),
    'forbidden', 'only participants or admins can update this thread');
  // Admin darf
  await noteCreate(cmd('threads.note.create', { thread_id: 't1', body: 'x' }, { id: 'admin', role: 'admin' }), h.ctx);
  assert.deepEqual(h.doc('user_threads', 't1').participant_ids, ['admin', 'bob']);
});

test('note.create: Pflichtfelder und Modul', async () => {
  const h = makeCtx({ users: USERS });
  await rejects(noteCreate(cmd('threads.note.create', {}, ALICE), h.ctx), 'invalid', 'required text field is missing');
  await rejects(noteCreate(cmd('threads.note.create', { body: 'x' }, ALICE, { module: 'notes', app_id: 'notes' }), h.ctx),
    'invalid', 'threads commands require module=threads');
});

test('preflight: Workjet-Privatchat verweigert → forbidden (threads-eigene Prüfung, keine Capability)', async () => {
  const h = makeCtx({ users: USERS, seed: {
    workjet_projects: [{ id: 'p1', owner_user_id: 'alice', status: 'active', name: 'P' }],
    workjet_project_chats: [{ id: 'workjet_private_p1_w1', project_id: 'p1', owner_user_id: 'alice', kind: 'private' }],
  } });
  await rejects(noteCreate(cmd('threads.note.create', { thread_id: 'workjet_private_p1_w1', body: 'x', target_user_ids: ['bob'] }, ALICE), h.ctx),
    'forbidden', 'Workjet chat cannot add another human participant');
  assert.equal(h.calls.writes.length, 0);
});

test('message.create: Thread muss existieren; Zuständiger = erster Adressat; Titel bleibt', async () => {
  const h = makeCtx({ users: USERS, seed: { user_threads: [{ id: 't1', thread_id: 't1', title: 'Fall', kind: 'case', status: 'blocked',
    participant_ids: ['alice'], assigned_user_id: 'alice', source_module: 'support', source_record_id: 'c1', updated_at_ms: 1 }] } });
  await rejects(messageCreate(cmd('threads.message.create', { thread_id: 'nope', body: 'x' }, ALICE), h.ctx), 'not_found', 'thread nope not found');
  await rejects(messageCreate(cmd('threads.message.create', { thread_id: 't1', body: 'x', kind: 'note' }, ALICE), h.ctx), 'invalid', 'invalid thread message kind');
  await rejects(messageCreate(cmd('threads.message.create', { body: 'x' }, ALICE), h.ctx), 'invalid', 'thread_id is required');
  const out = await messageCreate(cmd('threads.message.create', { thread_id: 't1', body: 'Bitte ansehen', target_user_ids: ['carol', 'bob'] }, ALICE), h.ctx);
  const t = h.doc('user_threads', 't1');
  assert.equal(t.kind, 'thread');
  assert.equal(t.status, 'open');
  assert.equal(t.assigned_user_id, 'bob');
  assert.deepEqual(t.participant_ids, ['alice', 'bob', 'carol']);
  assert.equal(t.title, 'Fall');
  assert.equal(t.metadata.source_context.module, 'support');
  assert.deepEqual(h.calls.policy, [{ permission: 'data.read', module: 'support' }]);
  const m = h.doc('user_thread_messages', out.message_id);
  assert.equal(m.kind, 'message');
  assert.equal(m.execution_status, 'completed');
  assert.deepEqual(m.target_user_ids, ['bob', 'carol']);
  assert.ok(out.message_id.startsWith('msg_'));
  assert.equal(h.doc('user_notifications', `notif_${slugPart(out.message_id)}_bob`).notification_type, 'message');
  assert.deepEqual(h.doc('user_thread_states', threadStateId('bob', 't1')).attention_reasons, ['Zugewiesen']);
  assert.deepEqual(h.doc('user_thread_states', threadStateId('carol', 't1')).attention_reasons, ['Ungelesen']);
  // Nachricht ohne Adressaten löscht die Zuständigkeit (Rust-Verhalten)
  await messageCreate(cmd('threads.message.create', { thread_id: 't1', body: 'Notiz' }, ALICE), h.ctx);
  assert.equal(h.doc('user_threads', 't1').assigned_user_id, '');
});

test('message.create: record_id als Thread-Rückfall (UI sendet recordId)', async () => {
  const h = makeCtx({ users: USERS, seed: { user_threads: [{ id: 't9', title: '', status: 'open', participant_ids: ['alice'], updated_at_ms: 1 }] } });
  const out = await messageCreate(cmd('threads.message.create', { body: 'x' }, ALICE, { record_id: 't9' }), h.ctx);
  assert.equal(out.thread_id, 't9');
  assert.equal(h.doc('user_threads', 't9').title, '');
});

test('notification: nur Besitzer oder Admin; dismiss setzt dismissed', async () => {
  const h = makeCtx({ users: USERS });
  await noteCreate(cmd('threads.note.create', { thread_id: 'tn', message_id: 'm1', body: 'x', target_user_ids: ['bob'] }, ALICE), h.ctx);
  await rejects(dismiss(cmd('threads.notification.dismiss', { notification_id: 'notif_m1_bob' }, ALICE), h.ctx), 'forbidden');
  await rejects(dismiss(cmd('threads.notification.dismiss', { notification_id: 'nope' }, BOB), h.ctx), 'not_found', 'notification nope not found');
  const r = await dismiss(cmd('threads.notification.dismiss', {}, BOB, { record_id: 'notif_m1_bob' }), h.ctx);
  assert.equal(r.status, 'dismissed');
  assert.equal(h.doc('user_notifications', 'notif_m1_bob').status, 'dismissed');
  assert.equal(h.doc('user_thread_states', threadStateId('bob', 'tn')).unread_count, 0);
});

test('refresh_thread_states liest alle Seiten (>200 Benachrichtigungen)', async () => {
  const notes = [];
  for (let i = 0; i < 450; i += 1) notes.push({ id: `n${String(i).padStart(4, '0')}`, thread_id: 'big', user_id: 'bob', status: 'unread', notification_type: 'message', updated_at_ms: 1 });
  const h = makeCtx({ users: USERS, readCap: 200, seed: {
    user_threads: [{ id: 'big', title: 'B', status: 'open', participant_ids: ['alice', 'bob'], updated_at_ms: 1 }],
    user_notifications: notes,
  } });
  await messageCreate(cmd('threads.message.create', { thread_id: 'big', body: 'x' }, ALICE), h.ctx);
  assert.equal(h.doc('user_thread_states', threadStateId('bob', 'big')).unread_count, 450);
});
