// Tests Strang WJAPP: Workjet-Privatchat-Zugriffsschutz (privacy.rs:473 command_access_check) und
// Supervisor-Bindung (supervisor_binding.rs:133 for_thread) als threads-Fachlogik über foreign_collections.
import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, cmd, USERS, rejects } from './_ctx.mjs';
import messageCreate from '../threads.message.create.js';
import noteCreate from '../threads.note.create.js';
import assign from '../threads.thread.assign.js';
import approvalRequest from '../threads.ctox_approval.request.js';
import { commandAccessCheck, commandVisibleToActor, commandDocument } from '../_chat_access.js';
import { supervisorBindingForThread } from '../_lib.js';

const OWNER = { id: 'owner', role: 'user', display_name: 'Owner' };
const OTHER = { id: 'other-user', role: 'user', display_name: 'Other' };
const OTHER_ADMIN = { id: 'other-admin', role: 'admin', display_name: 'Other Admin' };
const CHAT = 'workjet_private_p1_worker1';
const GROUP = 'workjet_group_p1';
const PROFILE = 'workjet_profile_owner_w1';
const users = { ...USERS, owner: { display_name: 'Owner' }, 'other-user': { display_name: 'Other' }, 'other-admin': {} };

function workjetSeed(extra = {}) {
  return {
    workjet_projects: [
      { id: 'p1', name: 'Projekt', owner_user_id: 'owner', status: 'active' },
      { id: 'p-archived', name: 'Alt', owner_user_id: 'owner', status: 'archived' },
      { id: 'p-foreign', name: 'Fremd', owner_user_id: 'other-user', status: 'active' },
      { id: 'p-deleted', name: 'Weg', owner_user_id: 'owner', status: 'active', is_deleted: true },
      // Existieren, damit nur `required` (project_chats.rs:406) sie abweisen kann:
      { id: 'ä'.repeat(65), name: 'Lang', owner_user_id: 'owner', status: 'active' },
      { id: 'p1\u0007', name: 'Steuer', owner_user_id: 'owner', status: 'active' },
      { id: 'ä'.repeat(64), name: 'Grenze', owner_user_id: 'owner', status: 'active' },
      { id: 'p-anon', name: 'Anon', owner_user_id: '', status: 'active' },
    ],
    workjet_project_chats: [
      { id: CHAT, thread_id: CHAT, project_id: 'p1', owner_user_id: 'owner', kind: 'private' },
      { id: GROUP, thread_id: GROUP, project_id: 'p1', owner_user_id: 'owner', kind: 'group' },
      { id: 'workjet_private_archived', project_id: 'p-archived', owner_user_id: 'owner', kind: 'private' },
      { id: 'workjet_private_foreignproject', project_id: 'p-foreign', owner_user_id: 'owner', kind: 'private' },
      { id: 'workjet_private_deletedproject', project_id: 'p-deleted', owner_user_id: 'owner', kind: 'private' },
      { id: 'workjet_private_deletedchat', project_id: 'p1', owner_user_id: 'owner', kind: 'private', is_deleted: true },
      { id: 'workjet_private_noproject', owner_user_id: 'owner', kind: 'private' },
      { id: 'workjet_private_anon', project_id: 'p-anon', owner_user_id: '', kind: 'private' },
    ],
    workjet_worker_profile_bindings: [
      { id: PROFILE, owner_user_id: 'owner', worker_profile_id: 'w1', status: 'active' },
    ],
    ...extra,
  };
}

function privateThread(id = CHAT) {
  return { id, thread_id: id, title: 'Privat', kind: 'chat', status: 'open', participant_ids: ['owner'],
    owner_user_id: 'owner', source_module: 'ctox', source_record_type: 'workjet_project', source_record_id: 'p1', updated_at_ms: 1 };
}

// ---- Rust-Testfall project_chats/tests.rs:846 private_threads_reject_foreign_human_mentions_and_admin_mutation

test('Rust tests.rs:846: fremde Erwähnung verweigert, eigene erlaubt, fremder Mensch (auch Admin) sieht den Chat nicht', async () => {
  const h = makeCtx({ users, seed: workjetSeed({ user_threads: [privateThread()] }) });
  const msg = (payload, actor) => cmd('threads.message.create', { thread_id: CHAT, body: 'My private message', ...payload }, actor);
  await rejects(messageCreate(msg({ target_user_ids: ['other-user'] }, OWNER), h.ctx), 'forbidden',
    'Workjet chat cannot add another human participant');
  assert.equal(h.calls.writes.length, 0, 'Verweigerung vor dem ersten Schreiben');
  await messageCreate(msg({ target_user_ids: ['owner'] }, OWNER), h.ctx);
  await rejects(messageCreate(msg({ target_user_ids: ['owner'] }, OTHER), h.ctx), 'forbidden',
    'Workjet chat is unavailable to this user or instance');
  await rejects(messageCreate(msg({}, OTHER_ADMIN), h.ctx), 'forbidden', 'Workjet chat is unavailable to this user or instance');
});

test('alle vier Listenfelder und vier Einzelfelder werden geprüft; leerer Einzelwert ist erlaubt', async () => {
  const h = makeCtx({ seed: workjetSeed() });
  for (const key of ['target_user_ids', 'participant_ids', 'watcher_user_ids', 'assignee_user_ids']) {
    await rejects(commandAccessCheck(h.ctx, cmd('threads.thread.watch', { thread_id: CHAT, [key]: ['owner', 'bob'] }, OWNER), 'owner'),
      'forbidden', 'cannot add another human participant');
    await commandAccessCheck(h.ctx, cmd('threads.thread.watch', { thread_id: CHAT, [key]: ['owner'] }, OWNER), 'owner');
  }
  for (const key of ['target_user_id', 'assigned_user_id', 'user_id', 'reviewer_user_id']) {
    await rejects(commandAccessCheck(h.ctx, cmd('threads.thread.assign', { thread_id: CHAT, [key]: 'bob' }, OWNER), 'owner'),
      'forbidden', 'cannot assign another human participant');
    await commandAccessCheck(h.ctx, cmd('threads.thread.assign', { thread_id: CHAT, [key]: '' }, OWNER), 'owner');
    await commandAccessCheck(h.ctx, cmd('threads.thread.assign', { thread_id: CHAT, [key]: 'owner' }, OWNER), 'owner');
  }
  // Nicht-String/Nicht-Array wird wie in Rust ignoriert (`as_array`/`as_str`).
  await commandAccessCheck(h.ctx, cmd('threads.thread.assign', { thread_id: CHAT, assigned_user_id: 7, target_user_ids: 'bob' }, OWNER), 'owner');
  // Array-Element ohne String (`value.as_str() == Some(user)`) verweigert.
  await rejects(commandAccessCheck(h.ctx, cmd('threads.thread.watch', { thread_id: CHAT, watcher_user_ids: [null] }, OWNER), 'owner'),
    'forbidden', 'cannot add another human participant');
});

test('Handler: assign und Freigabe-Antrag im Privatchat an anderen Menschen verweigert', async () => {
  const h = makeCtx({ users, seed: workjetSeed({ user_threads: [privateThread()] }) });
  await rejects(assign(cmd('threads.thread.assign', { thread_id: CHAT, assigned_user_id: 'bob' }, OWNER), h.ctx),
    'forbidden', 'cannot assign another human participant');
  await rejects(approvalRequest(cmd('threads.ctox_approval.request', { thread_id: CHAT, prompt: 'p', reviewer_user_id: 'bob' }, OWNER), h.ctx),
    'forbidden', 'cannot assign another human participant');
  assert.equal(h.calls.writes.length, 0);
});

test('Threads ohne Workjet-Verweis bleiben uneingeschränkt (Rust: None)', async () => {
  const h = makeCtx({ users, seed: workjetSeed() });
  const plain = cmd('threads.note.create', { thread_id: 'support-t1', body: 'Text nennt workjet_private_p1_worker1', target_user_ids: ['bob'] }, OTHER);
  assert.equal(await commandVisibleToActor(h.ctx, commandDocument(plain), 'other-user'), null);
  await noteCreate(plain, h.ctx);
  assert.deepEqual(h.doc('user_threads', 'support-t1').participant_ids, ['bob', 'other-user']);
});

test('Verweise: Chat-, Gruppen-, Profil-IDs in typisierten Feldern, record_id und verschachtelten Kontexten', async () => {
  const h = makeCtx({ seed: workjetSeed() });
  const vis = (command, user = 'owner') => commandVisibleToActor(h.ctx, commandDocument(command), user);
  assert.equal(await vis(cmd('threads.message.create', { chat_id: GROUP })), true);
  assert.equal(await vis(cmd('threads.message.create', { binding_id: PROFILE })), true);
  assert.equal(await vis(cmd('threads.message.create', { binding_id: PROFILE }), 'other-user'), false);
  assert.equal(await vis(cmd('threads.thread.watch', {}, OWNER, { record_id: CHAT })), true);
  assert.equal(await vis(cmd('threads.thread.watch', {}, OTHER, { record_id: CHAT }), 'other-user'), false);
  const nested = cmd('threads.note.create', { body: 'x' }, OTHER, { client_context: { source: { metadata: { parent_thread_id: CHAT } } } });
  assert.equal(await vis(nested, 'other-user'), false);
  // Nicht verfolgte Felder (z. B. payload.items[]) sind kein Verweis (Rust folgt nur den sechs Hüllenfeldern).
  assert.equal(await vis(cmd('threads.note.create', { items: [{ thread_id: CHAT }] }), 'other-user'), null);
});

test('Sichtbarkeit: gelöschter Chat, fehlender Chat, fremdes/gelöschtes Projekt verweigern; archiviertes Projekt genügt', async () => {
  const h = makeCtx({ seed: workjetSeed() });
  const vis = (thread) => commandVisibleToActor(h.ctx, commandDocument(cmd('threads.message.create', { thread_id: thread }, OWNER)), 'owner');
  assert.equal(await vis('workjet_private_archived'), true, 'owned_project(require_active=false)');
  assert.equal(await vis('workjet_private_foreignproject'), false);
  assert.equal(await vis('workjet_private_deletedproject'), false);
  assert.equal(await vis('workjet_private_deletedchat'), false);
  assert.equal(await vis('workjet_private_noproject'), false);
  assert.equal(await vis('workjet_private_missing'), false);
  assert.equal(await commandVisibleToActor(h.ctx, commandDocument(cmd('threads.message.create', { thread_id: CHAT })), ''), false);
  // Leerer Nutzer sieht nie etwas, auch keinen Chat mit leerem Eigentümer (privacy.rs:385).
  assert.equal(await commandVisibleToActor(h.ctx, commandDocument(cmd('threads.message.create', { thread_id: 'workjet_private_anon' })), ''), false);
});

test('Projektbefehl-ID und Abbruch-Quittung: Zielbefehl wird aufgelöst, fehlendes Ziel verweigert', async () => {
  const h = makeCtx({ seed: workjetSeed({ business_commands: [
    { id: 'workjet_project_native_ok', command_type: 'ctox.workjet.project.run', payload: { project_id: 'p1' } },
    { id: 'workjet_project_native_foreign', command_type: 'ctox.workjet.project.run', payload: { project_id: 'p-foreign' } },
    { id: 'ctox_delegate_public', command_type: 'business_os.chat.task', payload: {} },
  ] }) });
  const vis = (id, payload, user = 'owner') => commandVisibleToActor(h.ctx, commandDocument({ ...cmd('threads.thread.watch', payload, OWNER), id }), user);
  assert.equal(await vis('workjet_project_native_x', { project_id: 'p1' }), true);
  assert.equal(await vis('workjet_project_native_x', { project_id: 'p1' }, 'other-user'), false);
  assert.equal(await vis('workjet_project_native_x', {}), false);
  assert.equal(await vis('workjet_project_native_x', { project_id: `p1\u0007` }), false, 'Steuerzeichen (required)');
  assert.equal(await vis('workjet_project_native_x', { project_id: 'ä'.repeat(65) }), false, '130 Byte > 128 (required zählt Bytes)');
  assert.equal(await vis('workjet_project_native_x', { project_id: 'ä'.repeat(64) }), true, '128 Byte = Grenze');
  assert.equal(await vis('workjet_project_native_x', { project_id: '  p1  ' }), true, 'required trimmt');
  assert.equal(await vis('workjet_project_cancel_1', { target_command_id: 'workjet_project_native_ok' }), true);
  assert.equal(await vis('workjet_project_cancel_1', { target_command_id: 'workjet_project_native_foreign' }), false);
  assert.equal(await vis('workjet_project_cancel_1', { target_command_id: 'workjet_project_native_missing' }), false);
  assert.equal(await vis('workjet_project_cancel_1', { target_command_id: 'other_prefix' }), false);
  // Ziel ohne eigene Einschränkung → keine Einschränkung (Rust: constraints leer → None).
  assert.equal(await vis('workjet_project_cancel_1', { target_command_id: 'ctox_delegate_public' }), null);
});

// ---- Supervisor-Bindung (supervisor_binding.rs:133 for_thread, threads.rs:3546)

const SUP = '6f1c2d3e-4a5b-4c6d-8e7f-001122334455';
function supervisorSeed(bindings) {
  return { workjet_supervisor_bindings: bindings, user_threads: [{ ...privateThread(SUP), title: 'Supervisor' }] };
}

test('Bindung wird aus workjet_supervisor_bindings gelesen und in die Thread-Metadaten geschrieben', async () => {
  const h = makeCtx({ users, seed: supervisorSeed([{ id: 'p1', project_id: 'p1', owner_user_id: 'owner', thread_id: SUP, created_at_ms: 5 }]) });
  await messageCreate(cmd('threads.message.create', { thread_id: SUP, body: 'Status?' }, OWNER), h.ctx);
  const meta = h.doc('user_threads', SUP).metadata;
  assert.equal(meta.workjet_supervisor_contract, 'ctox.workjet.supervisor_binding.v1');
  assert.deepEqual(meta.workjet_supervisor, { project_id: 'p1', thread_id: SUP, thread_key: `business-os/threads/${SUP}` });
});

test('Bindung gilt nur für den Thread-Eigentümer; gelöschte Zeile zählt nicht', async () => {
  const h = makeCtx({ seed: { workjet_supervisor_bindings: [
    { id: 'p1', project_id: 'p1', owner_user_id: 'owner', thread_id: SUP },
    { id: 'p2', project_id: 'p2', owner_user_id: 'owner', thread_id: 'gone', is_deleted: true },
  ] } });
  assert.equal(await supervisorBindingForThread(h.ctx, 'other-user', SUP), null);
  assert.equal(await supervisorBindingForThread(h.ctx, 'owner', 'gone'), null);
  assert.equal(await supervisorBindingForThread(h.ctx, 'owner', ''), null);
  assert.deepEqual(await supervisorBindingForThread(h.ctx, 'owner', SUP),
    { contract: 'ctox.workjet.supervisor_binding.v1', project_id: 'p1', thread_id: SUP, thread_key: `business-os/threads/${SUP}` });
  // Fremder Thread-Eigentümer: Notiz in einen Thread, den 'other-user' besitzt → keine Bindung, keine Metadaten.
  const h2 = makeCtx({ users, seed: { workjet_supervisor_bindings: [{ id: 'p1', project_id: 'p1', owner_user_id: 'owner', thread_id: SUP }],
    user_threads: [{ ...privateThread(SUP), owner_user_id: 'other-user', participant_ids: ['other-user'] }] } });
  await noteCreate(cmd('threads.note.create', { thread_id: SUP, body: 'x' }, OTHER), h2.ctx);
  assert.equal(h2.doc('user_threads', SUP).metadata.workjet_supervisor, undefined);
});

test('Quellkontext widerspricht der Bindung → conflict, nichts geschrieben (threads.rs:3548)', async () => {
  const h = makeCtx({ users, seed: { workjet_supervisor_bindings: [{ id: 'p9', project_id: 'p9', owner_user_id: 'owner', thread_id: SUP }],
    user_threads: [privateThread(SUP)] } });
  await rejects(messageCreate(cmd('threads.message.create', { thread_id: SUP, body: 'x' }, OWNER), h.ctx),
    'conflict', 'supervisor source context conflicts with its native project binding');
  assert.equal(h.calls.writes.length, 0);
});

test('Handler laufen ohne jede Workjet-Capability (Attrappe bietet keine an)', async () => {
  const h = makeCtx({ users, seed: workjetSeed({ user_threads: [privateThread()] }) });
  assert.deepEqual(Object.keys(h.ctx.cap).sort(), ['agent', 'audit', 'directory', 'policy']); // agent = agent.task (SKL-A2), keine Workjet-Capability
  await messageCreate(cmd('threads.message.create', { thread_id: CHAT, body: 'ok' }, OWNER), h.ctx);
});
