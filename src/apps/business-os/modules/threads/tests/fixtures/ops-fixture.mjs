// Realistische Ops-Fixture für threads (workjet#594 „Threads heißen wie Kennungen“).
// Form der Datensätze = Ausgabe der Projektionen handlers/_projection.ctox_relevance.js (upsertThread, upsertMessage)
// und der Workjet-Befehle (payload.title = Befehls-ID wie in handlers/tests/projection.test.mjs:52,
// payload.project_id wie in src/core/business_os/project_chats/*). Wird im Browser per JSON übergeben.
const NOW = Date.UTC(2026, 9, 15, 11, 20);
const ME = 'michael';
const P1 = '0f3c9a52-7d1e-4b8a-9c21-5e6f7a8b9c0d';
const P2 = '7b2e4d19-3a6c-4f80-8e15-2c9d0b7a6f34';
const C1 = 'a0796d32-2ca9-4f1e-9b7a-3c5d8e2f1a46';
const C2 = 'e41b7c05-9d3a-4e62-b8f1-6a2c0d9e7b13';
const C3 = '5c1e8a27-b4d6-4c93-a0f5-8e7d2b1c9a60';
const T3 = '9d4f2b81-6e0a-4c57-b3d2-1f8e7a6c5b09';

function ctoxThread(id, recordId, recordType, title, status, label, extra = {}) {
  const source = { module: 'ctox', record_type: recordType, record_id: recordId, label,
    deep_link: recordType === 'command' ? `#ctox?command_id=${recordId}` : `#ctox?task_id=${recordId}` };
  return {
    id, thread_id: id, title, kind: 'ctox_task', status, participant_ids: [ME], watcher_user_ids: [],
    owner_user_id: ME, created_by_id: ME, assigned_user_id: '',
    source_module: 'ctox', source_record_type: recordType, source_record_id: recordId, source_label: label,
    source_deep_link: source.deep_link, last_message_id: `msg_${id}`, last_message_at_ms: NOW - 60_000 * 7,
    pending_approval_count: 0, snoozed_until_ms: 0, archived_at_ms: 0, last_seen_by_user: {},
    metadata: { source_context: source }, created_at_ms: NOW - 3_600_000, updated_at_ms: NOW - 60_000 * 7, ...extra,
  };
}

export function opsFixture() {
  const t1 = `thread_ctox_command_${C1}`;
  const t2 = `thread_ctox_command_${C2}`;
  const t3 = `thread_ctox_queue_task_${T3}`;
  const t4 = `thread_ctox_workjet_project_${P2}`;
  const t5 = 'thread_kundenpipeline_lead_lead-4711';
  const user_threads = [
    // Supervisor-Zug: Titel und Quelllabel = Befehls-ID (payload.title = id), Projekt nur über payload.project_id.
    ctoxThread(t1, C1, 'command', C1, 'blocked', C1, { updated_at_ms: NOW - 60_000 * 3, last_message_at_ms: NOW - 60_000 * 3 }),
    // Steuerbefehl ohne Prompt und ohne Projekt.
    ctoxThread(t2, C2, 'command', C2, 'blocked', C2),
    // Queue-Aufgabe: Projektions-Rückfall „CTOX Arbeit <id>“, Aufgabe hat inzwischen einen Titel.
    ctoxThread(t3, T3, 'queue_task', `CTOX Arbeit ${C3}`, 'running', T3),
    // Supervisor-Thread eines Workjet-Projekts (source record_type workjet_project, label = Projektname).
    { ...ctoxThread(t4, P2, 'workjet_project', P2, 'open', 'Workjet Desktop'), kind: 'thread',
      metadata: { source_context: { module: 'ctox', record_type: 'workjet_project', record_id: P2, label: 'Workjet Desktop', deep_link: `#ctox?record=${P2}&record_type=workjet_project` },
        workjet_supervisor: { project_id: P2, thread_id: t4, thread_key: `business-os/threads/${t4}` } } },
    // Fachthread mit sprechendem Titel (darf unverändert bleiben).
    { id: t5, thread_id: t5, title: 'Angebot Müller GmbH prüfen', kind: 'note', status: 'open', participant_ids: [ME, 'anna'],
      watcher_user_ids: [], owner_user_id: 'anna', created_by_id: 'anna', assigned_user_id: ME,
      source_module: 'kundenpipeline', source_record_type: 'lead', source_record_id: 'lead-4711', source_label: 'Müller GmbH',
      source_deep_link: '#kundenpipeline?record=lead-4711&record_type=lead', last_message_at_ms: NOW - 3_600_000 * 2,
      pending_approval_count: 1, metadata: {}, created_at_ms: NOW - 86_400_000, updated_at_ms: NOW - 3_600_000 * 2 },
  ];
  const state = (threadId, score, reasons) => ({ id: `state_${threadId}_${ME}`, thread_id: threadId, user_id: ME,
    attention_score: score, attention_reasons: reasons, unread_count: 1, updated_at_ms: NOW - 60_000 });
  const user_thread_states = [
    state(t1, 90, ['Blockiert', 'Fehlgeschlagen']),
    state(t2, 80, ['Blockiert', 'Fehlgeschlagen']),
    state(t3, 40, ['AI arbeitet']),
    state(t4, 30, ['Erwähnung']),
    state(t5, 60, ['Freigabe nötig']),
  ];
  const user_thread_messages = [
    { id: `msg_${t1}`, message_id: `msg_${t1}`, thread_id: t1, kind: 'ctox_status', event_type: 'ctox_status', actor_type: 'ai',
      actor_id: ME, body: 'CTOX Arbeit blockiert oder fehlgeschlagen.\n\nBuild-Lane Exit 75: Sperre belegt\n\nLogin-Seite auf App-API 4.0 umstellen und Abnahme fahren',
      command_id: C1, created_at_ms: NOW - 60_000 * 3, updated_at_ms: NOW - 60_000 * 3 },
    { id: `msg_${t5}_1`, message_id: `msg_${t5}_1`, thread_id: t5, kind: 'note', event_type: 'note', actor_type: 'user',
      author_user_id: 'anna', author_display_name: 'Anna Berg', body: '@Michael bitte das Angebot vor Freitag freigeben.',
      created_at_ms: NOW - 3_600_000 * 2, updated_at_ms: NOW - 3_600_000 * 2 },
  ];
  const user_thread_links = [
    { id: `link_${t1}_business_commands_${C1}`, thread_id: t1, link_role: 'ctox_command', link_type: 'ctox_command', command_id: C1,
      source_module: 'ctox', source_record_type: 'command', source_record_id: C1, source_label: C1, deep_link: `#ctox?command_id=${C1}`, updated_at_ms: NOW - 60_000 * 3 },
    { id: `link_${t3}_ctox_queue_tasks_${T3}`, thread_id: t3, link_role: 'ctox_task', link_type: 'ctox_task', task_id: T3, command_id: C3,
      source_module: 'ctox', source_record_type: 'queue_task', source_record_id: T3, source_label: T3, deep_link: `#ctox?task_id=${T3}`, updated_at_ms: NOW - 60_000 * 7 },
  ];
  const business_commands = [
    { id: C1, command_id: C1, module: 'ctox', command_type: 'ctox.workjet.project.supervisor.turn.submit', status: 'failed',
      record_id: P1, payload: { title: C1, project_id: P1, prompt: 'Login-Seite auf App-API 4.0 umstellen und Abnahme fahren' }, updated_at_ms: NOW - 60_000 * 3 },
    { id: C2, command_id: C2, module: 'ctox', command_type: 'ctox.workjet.project.list', status: 'failed', record_id: ME,
      payload: { title: C2 }, updated_at_ms: NOW - 60_000 * 7 },
    { id: C3, command_id: C3, module: 'ctox', command_type: 'ctox.workjet.project.supervisor.turn.submit', status: 'running', task_id: T3,
      record_id: P1, payload: { title: C3, project_id: P1 }, updated_at_ms: NOW - 60_000 * 7 },
  ];
  const ctox_queue_tasks = [
    { id: T3, command_id: C3, title: 'Nachtlauf: Abnahme Kundenpipeline auf gpu3', status: 'running', module: 'ctox', updated_at_ms: NOW - 60_000 * 7 },
  ];
  const ctox_task_approval_requests = [
    { id: 'approval_mueller', approval_request_id: 'approval_mueller', thread_id: t5, status: 'pending', requester_user_id: 'anna',
      requester_display_name: 'Anna Berg', reviewer_user_id: ME, reviewer_display_name: 'Michael Welsch',
      prompt: 'Angebot an Müller GmbH versenden', instruction: 'Angebot an Müller GmbH versenden', target_module: 'kundenpipeline',
      requested_at_ms: NOW - 3_600_000, created_at_ms: NOW - 3_600_000, updated_at_ms: NOW - 3_600_000 },
  ];
  const user_notifications = [
    { id: `notif_${t1}_${ME}`, thread_id: t1, user_id: ME, notification_type: 'ctox_failed', status: 'unread', title: 'CTOX Arbeit blockiert', updated_at_ms: NOW - 60_000 * 3 },
  ];
  const workjet_projects = [
    { id: P1, name: 'CTOX Shell v4', status: 'active', owner_user_id: ME, updated_at_ms: NOW - 86_400_000 },
    { id: P2, name: 'Workjet Desktop', status: 'active', owner_user_id: ME, updated_at_ms: NOW - 86_400_000 },
  ];
  const business_users = [
    { id: ME, display_name: 'Michael Welsch', updated_at_ms: 1 },
    { id: 'anna', display_name: 'Anna Berg', updated_at_ms: 1 },
  ];
  const business_apps = [
    { id: 'threads', title: 'Threads' }, { id: 'ctox', title: 'CTOX' }, { id: 'kundenpipeline', title: 'Kundenpipeline' },
  ];
  return {
    now: NOW, me: ME, ids: { t1, t2, t3, t4, t5, C1, C2, C3, T3, P1, P2 },
    collections: { business_apps, business_users, workjet_projects, user_threads, user_thread_states, user_thread_messages,
      user_thread_links, business_commands, ctox_queue_tasks, ctox_task_approval_requests, user_notifications },
  };
}
