// Tests: _projection.ctox_relevance.js (Port von project_ctox_relevance, kein Befehlstyp).
import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx } from './_ctx.mjs';
import project, { ctoxSourceContext, ctoxThreadStatus, ctoxStatusBody } from '../_projection.ctox_relevance.js';

const ALICE_CC = { actor: { id: 'alice', display_name: 'Alice', role: 'user' } };

// ctox_relevance_projection_joins_command_and_task_into_one_thread (threads.rs:7763)
test('Befehl + Task → ein Thread, zwei Links, Abschluss-Benachrichtigung, Wiederholung ändert nichts', async () => {
  const now = 1_700_000_000_000;
  const h = makeCtx({ seed: {
    business_commands: [{ id: 'cmd-relevance', command_id: 'cmd-relevance', module: 'support', command_type: 'business_os.chat.task',
      record_id: 'case-1', status: 'completed', task_id: 'task-relevance',
      payload: { title: 'Pruefe Antwortentwurf', prompt: 'Bitte pruefe den Antwortentwurf fuer case-1.' },
      client_context: ALICE_CC, updated_at_ms: now }],
    ctox_queue_tasks: [{ id: 'task-relevance', command_id: 'cmd-relevance', source_module: 'support', status: 'completed',
      title: 'Pruefe Antwortentwurf', updated_at_ms: now + 1 }],
  } });
  const out = await project({ command_since_ms: 0, task_since_ms: 0, limit: 50 }, h.ctx);
  assert.ok(out.changed_count > 0);
  assert.ok(out.projections.some((p) => p.collection === 'user_threads' && p.record_id === 'thread_support_command_case-1'));
  assert.equal(h.doc('user_threads', 'thread_support_queue_task_case-1'), null);
  const t = h.doc('user_threads', 'thread_support_command_case-1');
  assert.equal(t.source_record_type, 'command');
  assert.equal(t.status, 'completed');
  assert.equal(t.kind, 'ctox_task');
  assert.equal(t.updated_at_ms, now + 1);
  const cl = h.doc('user_thread_links', 'link_thread_support_command_case-1_business_commands_cmd-relevance');
  assert.equal(cl.link_role, 'ctox_command');
  assert.equal(cl.task_id, 'task-relevance');
  assert.equal(h.doc('user_thread_links', 'link_thread_support_command_case-1_ctox_queue_tasks_task-relevance').link_role, 'ctox_task');
  const n = h.doc('user_notifications', 'notif_ctox_completed_cmd-relevance_alice');
  assert.equal(n.thread_id, 'thread_support_command_case-1');
  assert.equal(n.user_id, 'alice');
  assert.equal(n.notification_type, 'ctox_completed');
  const m = h.doc('user_thread_messages', 'msg_ctox_cmd-relevance_completed');
  assert.equal(m.body, 'CTOX Arbeit abgeschlossen.\n\nBitte pruefe den Antwortentwurf fuer case-1.');
  assert.equal(out.source_cursors[1].since_ms, now + 1);
  const again = await project({ command_since_ms: 0, task_since_ms: 0, limit: 50 }, h.ctx);
  assert.equal(again.changed_count, 0);
  // gelesene Benachrichtigung bleibt gelesen bei erneuter Projektion
  await h.ctx.put('user_notifications', { ...h.doc('user_notifications', 'notif_ctox_completed_cmd-relevance_alice'), status: 'read' });
  await project({ commands: [{ ...h.doc('business_commands', 'cmd-relevance'), updated_at_ms: now + 5 }], tasks: [] }, h.ctx);
  assert.equal(h.doc('user_notifications', 'notif_ctox_completed_cmd-relevance_alice').status, 'read');
});

// ctox_command_projection_uses_command_identity_for_actor_scoped_targets (threads.rs:7560)
test('ctox-Steuerbefehle: je Befehl eigener Thread über Befehls-ID; Threads-Befehle werden übersprungen', async () => {
  const now = 1_700_000_000_000;
  const cmds = ['cmd-workjet-list-1', 'cmd-workjet-list-2'].map((id) => ({ id, command_id: id, module: 'ctox',
    command_type: 'ctox.workjet.project.list', record_id: 'alice', status: 'completed', payload: { title: id },
    client_context: { source: 'workjet-project-control', ...ALICE_CC }, updated_at_ms: now }));
  cmds.push({ id: 'cmd-thread-note', command_id: 'cmd-thread-note', module: 'threads', command_type: 'threads.note.create', status: 'completed', client_context: ALICE_CC, updated_at_ms: now });
  const h = makeCtx({ seed: { business_commands: cmds } });
  await project({ command_since_ms: 0, task_since_ms: 0, limit: 50 }, h.ctx);
  for (const id of ['cmd-workjet-list-1', 'cmd-workjet-list-2']) {
    const t = h.doc('user_threads', `thread_ctox_command_${id}`);
    assert.equal(t.source_record_id, id);
    assert.equal(t.source_record_type, 'command');
    assert.equal(t.source_deep_link, `#ctox?command_id=${id}`);
    const l = h.doc('user_thread_links', `link_thread_ctox_command_${id}_business_commands_${id}`);
    assert.equal(l.command_id, id);
  }
  assert.equal(h.doc('user_threads', 'thread_ctox_command_alice'), null);
  assert.equal(h.all('user_threads').length, 2);
  assert.equal((await project({ command_since_ms: 0, task_since_ms: 0, limit: 50 }, h.ctx)).changed_count, 0);
});

// ctox_command_source_preserves_explicit_app_and_task_context (threads.rs:7614)
test('Quellkontext: explizit, Auswahl, Fachbefehl', () => {
  let s = ctoxSourceContext({ id: 'cmd-control', module: 'ctox', record_id: 'alice', payload: { source_context: { module: 'ctox', record_type: 'queue_task', record_id: 'task-source' } } }, null, 'cmd-control');
  assert.equal(s.record_id, 'task-source');
  assert.equal(s.record_type, 'queue_task');
  assert.equal(s.deep_link, '#ctox?task_id=task-source');
  s = ctoxSourceContext({ id: 'cmd-control', module: 'ctox', record_id: 'alice', client_context: { scope: { selection: { record_type: 'command', record_id: 'cmd-selected' } } } }, null, 'cmd-control');
  assert.equal(s.record_id, 'cmd-selected');
  assert.equal(s.deep_link, '#ctox?command_id=cmd-selected');
  s = ctoxSourceContext({ id: 'cmd-business', module: 'support', record_id: 'case-1' }, null, 'cmd-business');
  assert.equal(s.record_id, 'case-1');
  assert.equal(s.deep_link, '#support?record=case-1&record_type=command');
});

test('Status-Abbildung und Text', () => {
  assert.equal(ctoxThreadStatus({ status: 'pending_sync' }, null), 'running');
  assert.equal(ctoxThreadStatus({ status: 'x' }, { status: 'FAILED' }), 'blocked');
  assert.equal(ctoxThreadStatus({}, { route_status: 'canceled' }), 'archived');
  assert.equal(ctoxThreadStatus({}, null), 'open');
  assert.equal(ctoxThreadStatus({ status: 'waiting' }, null), 'waiting');
  assert.equal(ctoxStatusBody({}, { status_note: ' Timeout ' }, 'blocked'), 'CTOX Arbeit blockiert oder fehlgeschlagen.\n\nTimeout');
});

test('Task ohne Befehl: eigener Thread über thread_key, Fehlgeschlagen für Zuständigen', async () => {
  const h = makeCtx({ seed: {
    user_threads: [{ id: 'tk', thread_id: 'tk', title: 'Alt', status: 'open', participant_ids: ['bob'], updated_at_ms: 1 }],
    ctox_queue_tasks: [{ id: 'task-x', command_id: '', owner_user_id: 'bob', thread_key: 'business-os/threads/tk', status: 'failed', error: 'boom', title: 'Lauf', updated_at_ms: 50 }],
  } });
  await project({ command_since_ms: 0, task_since_ms: 0 }, h.ctx);
  const t = h.doc('user_threads', 'tk');
  assert.equal(t.status, 'blocked');
  assert.equal(t.assigned_user_id, 'bob');
  assert.equal(h.doc('user_notifications', 'notif_ctox_blocked_task-x_bob').notification_type, 'ctox_failed');
  const st = h.doc('user_thread_states', 'thread_state_bob_tk');
  // kind=ctox_task (Maschinenarbeit) → nur konkrete Gründe
  assert.deepEqual(st.attention_reasons, ['Blockiert', 'Fehlgeschlagen']);
  assert.equal(st.attention_score, 70);
});
