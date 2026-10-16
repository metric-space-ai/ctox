// Projektion business_commands + ctox_queue_tasks → user_threads/… („CTOX-Relevanz“).
// Port von project_ctox_relevance (threads.rs:966-1042), project_ctox_command_document (:2508-2604),
// project_ctox_task_document (:2606-2687) und Helfern (:4064-4508). KEIN Befehlstyp: läuft heute in der
// Projektionsschleife des nativen Peers (rxdb_peer.rs:5637-5661). Vertragswunsch W-4 (V4-HANDLERS.md §3/§4):
// Core ruft `project(input, ctx)` bei Änderungen an den Quell-Collections bzw. mit Cursor auf.
// Braucht foreign_collections read: business_commands, ctox_queue_tasks (Eigentümer ctox).
import {
  clone, documentIsDeleted, documentUpdatedAtMs, firstNonEmpty, firstPresent, firstStringField, isObject,
  loadRecord, nestedString, nonEmptyString, participantSet, refreshThreadStates, slugPart, sourceString,
  supervisorBindingForThread, threadOwnerForBinding, truncate, upsertIfChanged, upsertMessage, upsertThread,
  valueString,
} from './_lib.js';

const commandIdOf = (command) => firstNonEmpty([valueString(command, 'command_id'), valueString(command, 'id')]);

/** is_threads_internal_command (:4311). */
export function isThreadsInternalCommand(command) {
  return valueString(command, 'module') === 'threads' || valueString(command, 'command_type').startsWith('threads.');
}

/** actor_from_ctox_documents (:4316) → Pseudo-Befehl {actor} für die gemeinsamen Upserts. */
export function actorFromCtoxDocuments(command, task) {
  const cc = command?.client_context;
  const actor = isObject(cc) ? firstPresent(cc, ['actor', 'user']) : undefined;
  const id = (isObject(actor) ? nonEmptyString(actor, 'id') : undefined)
    ?? nestedString(command, ['client_context', 'user_id'])
    ?? nestedString(command, ['payload', 'approval', 'requester_user_id'])
    ?? (task ? nonEmptyString(task, 'owner_user_id') : undefined)
    ?? (task ? nonEmptyString(task, 'lease_owner') : undefined);
  if (id === undefined) return null;
  const displayName = (isObject(actor) ? firstStringField(actor, ['display_name', 'name']) : undefined) ?? id;
  const role = ((isObject(actor) ? nonEmptyString(actor, 'role') : undefined)
    ?? nestedString(command, ['client_context', 'role']) ?? 'user').trim();
  return { actor: { id, display_name: displayName, role, is_admin: role === 'chef' || role === 'admin' } };
}

/** ctox_source_context (:4162). */
export function ctoxSourceContext(command, task, fallbackId) {
  const payload = isObject(command?.payload) ? command.payload : undefined;
  const rawContext = payload ? firstPresent(payload, ['source_context', 'context']) : undefined;
  const pc = isObject(rawContext) ? rawContext : {};
  const cc = isObject(command?.client_context) ? command.client_context : undefined;
  const scope = cc && isObject(cc.scope) ? cc.scope : undefined;
  const selection = scope ? scope.selection : undefined;
  const selStr = (key) => (isObject(selection) ? nonEmptyString(selection, key) : undefined);
  const ccStr = (key) => (cc ? nonEmptyString(cc, key) : undefined);
  const module = sourceString(pc, 'module')
    ?? nestedString(command, ['payload', 'source_module'])
    ?? (cc ? firstStringField(cc, ['source_module', 'module', 'module_id', 'app_id']) : undefined)
    ?? nonEmptyString(command, 'module')
    ?? (task ? nonEmptyString(task, 'source_module') : undefined)
    ?? 'ctox';
  const hasIdentity = commandIdOf(command) !== '';
  const recordType = sourceString(pc, 'record_type') ?? selStr('record_type') ?? ccStr('record_type')
    ?? (hasIdentity ? 'command' : task ? 'queue_task' : 'command');
  const recordId = sourceString(pc, 'record_id') ?? selStr('record_id') ?? ccStr('record_id')
    ?? (module === 'ctox' && recordType === 'command' && hasIdentity
      ? firstStringField(command, ['command_id', 'id'])
      : nonEmptyString(command, 'record_id'))
    ?? (task ? nonEmptyString(task, 'id') : undefined)
    ?? fallbackId;
  const promptLabel = nestedString(command, ['payload', 'prompt']);
  const label = sourceString(pc, 'label') ?? selStr('label') ?? ccStr('label')
    ?? nestedString(command, ['payload', 'title'])
    ?? (task ? nonEmptyString(task, 'title') : undefined)
    ?? (promptLabel !== undefined ? truncate(promptLabel, 120) : undefined)
    ?? fallbackId;
  let deepLink = sourceString(pc, 'deep_link') ?? ccStr('deep_link');
  if (deepLink === undefined) {
    if (module === 'ctox' && recordType === 'command') deepLink = `#ctox?command_id=${slugPart(recordId)}`;
    else if (module === 'ctox' && ['queue_task', 'ctox_task', 'task'].includes(recordType)) deepLink = `#ctox?task_id=${slugPart(recordId)}`;
    else deepLink = `#${module}?record=${slugPart(recordId)}&record_type=${slugPart(recordType)}`;
  }
  return { module, record_type: recordType, record_id: recordId, label, deep_link: deepLink };
}

/** ctox_thread_id (:4075) inkl. project_existing_approval_thread (:4064). */
export async function ctoxThreadId(store, command, task, source, fallbackId) {
  const approvalId = nestedString(command, ['payload', 'approval', 'approval_request_id']);
  if (approvalId !== undefined) {
    const approval = await loadRecord(store, 'ctox_task_approval_requests', approvalId);
    const threadId = approval ? valueString(approval, 'thread_id') : '';
    if (threadId !== '') return threadId;
  }
  for (const key of [
    nestedString(command, ['payload', 'thread_key']),
    nestedString(command, ['client_context', 'thread_key']),
    task ? nonEmptyString(task, 'thread_key') : undefined,
  ]) {
    if (key !== undefined && key.startsWith('business-os/threads/')) {
      const id = key.slice('business-os/threads/'.length).trim();
      if (id !== '') return id;
    }
  }
  const module = sourceString(source, 'module') ?? 'ctox';
  const recordType = sourceString(source, 'record_type') ?? 'ctox_task';
  const recordId = sourceString(source, 'record_id') ?? fallbackId;
  return truncate(`thread_${slugPart(module)}_${slugPart(recordType)}_${slugPart(recordId)}`, 220);
}

/** existing_ctox_command_source_thread (:4114): Altverknüpfung reparieren statt Verlauf verschieben. */
async function existingCtoxCommandSourceThread(store, command, task, source, commandId) {
  if (sourceString(source, 'module') !== 'ctox' || sourceString(source, 'record_type') !== 'command'
    || sourceString(source, 'record_id') !== commandId) return null;
  const legacyRecordId = nonEmptyString(command, 'record_id') ?? (task ? nonEmptyString(task, 'id') : undefined);
  if (legacyRecordId === undefined || legacyRecordId === commandId) return null;
  const legacySource = clone(source);
  legacySource.record_id = legacyRecordId;
  const legacyThreadId = await ctoxThreadId(store, command, task, legacySource, commandId);
  const link = await loadRecord(store, 'user_thread_links', `link_${slugPart(legacyThreadId)}_business_commands_${slugPart(commandId)}`);
  if (!link) return null;
  if (valueString(link, 'command_id') !== commandId || valueString(link, 'thread_id') !== legacyThreadId
    || valueString(link, 'link_role') !== 'ctox_command') return null;
  const thread = await loadRecord(store, 'user_threads', legacyThreadId);
  if (!thread) return null;
  return valueString(thread, 'kind') === 'ctox_task' && valueString(thread, 'source_module') === 'ctox'
    && valueString(thread, 'source_record_type') === 'command' ? legacyThreadId : null;
}

/** ctox_thread_title (:4252). */
export function ctoxThreadTitle(command, task, source, fallbackId) {
  const prompt = nestedString(command, ['payload', 'prompt']);
  return (task ? nonEmptyString(task, 'title') : undefined)
    ?? nestedString(command, ['payload', 'title'])
    ?? sourceString(source, 'label')
    ?? (prompt !== undefined ? truncate(prompt, 120) : undefined)
    ?? `CTOX Arbeit ${fallbackId}`;
}

/** ctox_thread_status (:4267). */
export function ctoxThreadStatus(command, task) {
  const status = ((task ? firstStringField(task, ['status', 'task_status', 'route_status']) : undefined)
    ?? firstStringField(command, ['status', 'task_status']) ?? 'open').toLowerCase();
  if (['queued', 'pending', 'pending_sync', 'accepted', 'leased', 'running', 'in_progress'].includes(status)) return 'running';
  if (['failed', 'blocked', 'error'].includes(status)) return 'blocked';
  if (['completed', 'handled', 'done', 'success'].includes(status)) return 'completed';
  if (['cancelled', 'canceled'].includes(status)) return 'archived';
  return status !== '' ? status : 'open';
}

export const ctoxStatusDeservesEvent = (status) => ['completed', 'blocked', 'failed', 'archived'].includes(status);

/** ctox_status_body (:4288). */
export function ctoxStatusBody(command, task, status) {
  const note = (task ? firstStringField(task, ['status_note', 'error']) : undefined)
    ?? firstStringField(command, ['status_note', 'error']) ?? '';
  const prompt = (task ? nonEmptyString(task, 'prompt') : undefined) ?? nestedString(command, ['payload', 'prompt']) ?? '';
  const prefix = status === 'completed' ? 'CTOX Arbeit abgeschlossen.'
    : status === 'blocked' || status === 'failed' ? 'CTOX Arbeit blockiert oder fehlgeschlagen.'
      : status === 'archived' ? 'CTOX Arbeit wurde beendet.' : 'CTOX Status aktualisiert.';
  return [prefix, note, prompt].map((p) => p.trim()).filter((p) => p !== '').join('\n\n');
}

async function upsertCtoxTaskLink(store, threadId, task, source, now, projections) {
  const taskId = valueString(task, 'id');
  if (taskId === '') return;
  const linkId = `link_${slugPart(threadId)}_ctox_queue_tasks_${slugPart(taskId)}`;
  await upsertIfChanged(store, 'user_thread_links', linkId, now, {
    id: linkId,
    thread_id: threadId,
    source_module: 'ctox',
    source_record_type: 'queue_task',
    source_record_id: taskId,
    source_label: nonEmptyString(task, 'title') ?? sourceString(source, 'label') ?? '',
    link_role: 'ctox_task',
    link_type: 'queue_task',
    command_id: valueString(task, 'command_id'),
    task_id: valueString(task, 'id'),
    deep_link: sourceString(source, 'deep_link') ?? '',
    context: source,
    created_at_ms: now,
    updated_at_ms: now,
  }, projections);
}

async function upsertCtoxCommandLink(store, threadId, commandId, task, source, now, projections) {
  const linkId = `link_${slugPart(threadId)}_business_commands_${slugPart(commandId)}`;
  await upsertIfChanged(store, 'user_thread_links', linkId, now, {
    id: linkId,
    thread_id: threadId,
    source_module: sourceString(source, 'module') ?? 'ctox',
    source_record_type: sourceString(source, 'record_type') ?? 'command',
    source_record_id: sourceString(source, 'record_id') ?? commandId,
    source_label: sourceString(source, 'label') ?? commandId,
    link_role: 'ctox_command',
    link_type: 'business_command',
    command_id: commandId,
    task_id: task ? valueString(task, 'id') : '',
    deep_link: sourceString(source, 'deep_link') ?? '',
    context: source,
    created_at_ms: now,
    updated_at_ms: now,
  }, projections);
  if (task) await upsertCtoxTaskLink(store, threadId, task, source, now, projections);
}

/** upsert_status_notification (:4450): Lesestatus einer bestehenden Benachrichtigung bleibt. */
async function upsertStatusNotification(store, a, projections) {
  if (a.userId.trim() === '') return;
  const type = a.status === 'completed' ? 'ctox_completed'
    : a.status === 'blocked' || a.status === 'failed' ? 'ctox_failed'
      : a.status === 'archived' ? 'ctox_finished' : 'ctox_status';
  const id = `notif_ctox_${slugPart(a.status)}_${slugPart(a.statusSourceId)}_${slugPart(a.userId)}`;
  const existing = await loadRecord(store, 'user_notifications', id);
  const existingStatus = existing ? valueString(existing, 'status') : '';
  await upsertIfChanged(store, 'user_notifications', id, a.now, {
    id,
    notification_id: id,
    user_id: a.userId,
    thread_id: a.threadId,
    message_id: a.messageId,
    approval_request_id: '',
    notification_type: type,
    status: existingStatus !== '' ? existingStatus : 'unread',
    title: a.title,
    body_preview: truncate(a.body, 180),
    source_module: sourceString(a.source, 'module') ?? '',
    source_record_id: sourceString(a.source, 'record_id') ?? '',
    created_at_ms: a.now,
    updated_at_ms: a.now,
  }, projections);
}

/** project_ctox_command_document (:2508) / project_ctox_task_document (:2606) – gemeinsamer Ablauf. */
async function projectDocument(ctx, command, task, projections) {
  const isTaskOnly = command === null;
  const cmdDoc = command ?? {};
  const sourceId = isTaskOnly ? valueString(task, 'id') : commandIdOf(cmdDoc);
  if (sourceId === '') return;
  if (!isTaskOnly && isThreadsInternalCommand(cmdDoc)) return;
  const pseudo = actorFromCtoxDocuments(cmdDoc, task);
  if (!pseudo) return;
  const actor = pseudo.actor.id;
  const now = isTaskOnly ? documentUpdatedAtMs(task) : Math.max(documentUpdatedAtMs(cmdDoc), task ? documentUpdatedAtMs(task) : 0);
  const source = ctoxSourceContext(cmdDoc, task, sourceId);
  const threadId = isTaskOnly
    ? await ctoxThreadId(ctx, cmdDoc, task, source, sourceId)
    : (await existingCtoxCommandSourceThread(ctx, cmdDoc, task, source, sourceId))
      ?? await ctoxThreadId(ctx, cmdDoc, task, source, sourceId);
  const participants = await participantSet(ctx, threadId, [actor], []);
  if (!isTaskOnly) {
    const reviewer = nestedString(cmdDoc, ['payload', 'approval', 'reviewer_user_id']);
    if (reviewer !== undefined && !participants.includes(reviewer)) participants.push(reviewer);
  }
  const status = ctoxThreadStatus(cmdDoc, task);
  const title = ctoxThreadTitle(cmdDoc, task, source, sourceId);
  const messageId = `msg_ctox_${slugPart(sourceId)}_${slugPart(status)}`;
  const binding = await supervisorBindingForThread(ctx, await threadOwnerForBinding(ctx, pseudo, threadId), threadId);
  await ctx.transaction(async (tx) => {
    await upsertThread(tx, {
      threadId, title, kind: 'ctox_task', status, participants, source, command: pseudo,
      assignedUserId: actor, lastMessageId: messageId, now, pendingDelta: 0, binding,
    }, projections);
    if (isTaskOnly) await upsertCtoxTaskLink(tx, threadId, task, source, now, projections);
    else await upsertCtoxCommandLink(tx, threadId, sourceId, task, source, now, projections);
    if (ctoxStatusDeservesEvent(status)) {
      const body = ctoxStatusBody(cmdDoc, task, status);
      await upsertMessage(tx, {
        threadId, messageId, kind: 'ctox_status', command: pseudo, targetUserIds: [actor], body, source,
        approvalRequestId: '', commandId: isTaskOnly ? '' : sourceId, now,
      }, projections);
      await upsertStatusNotification(tx, {
        threadId, messageId, statusSourceId: sourceId, status, userId: actor, title, body, source, now,
      }, projections);
    }
    await refreshThreadStates(tx, threadId, now, projections);
  });
}

async function pullProjectionDocuments(ctx, collection, sinceMs, limit) {
  // pull_collection_records_for_projection: updated_at_ms ≥ since, aufsteigend, dann id.
  return ctx.read(collection, {
    selector: { updated_at_ms: { $gte: sinceMs } },
    sort: [{ updated_at_ms: 'asc' }, { id: 'asc' }],
    limit,
  });
}

/**
 * project_ctox_relevance (:966). input = {command_since_ms, task_since_ms, limit} (Cursor-Betrieb wie heute)
 * oder {commands, tasks} (Änderungs-Betrieb, W-4). Ergebnis wie Rust: {changed_count, source_cursors, projections}.
 */
export default async function project(input, ctx) {
  const limit = Math.min(Math.max(Number(input?.limit) || 500, 1), 2000);
  const commandSince = Number(input?.command_since_ms) || 0;
  const taskSince = Number(input?.task_since_ms) || 0;
  const commands = Array.isArray(input?.commands) ? input.commands : await pullProjectionDocuments(ctx, 'business_commands', commandSince, limit);
  const tasks = Array.isArray(input?.tasks) ? input.tasks : await pullProjectionDocuments(ctx, 'ctox_queue_tasks', taskSince, limit);
  const commandById = new Map();
  const taskByCommandId = new Map();
  let maxCommand = commandSince;
  for (const c of commands) {
    maxCommand = Math.max(maxCommand, documentUpdatedAtMs(c));
    if (documentIsDeleted(c)) continue;
    const id = commandIdOf(c);
    if (id !== '') commandById.set(id, c);
  }
  let maxTask = taskSince;
  for (const t of tasks) {
    maxTask = Math.max(maxTask, documentUpdatedAtMs(t));
    const id = valueString(t, 'command_id');
    if (id !== '') taskByCommandId.set(id, t);
  }
  const projections = [];
  const projected = new Set();
  for (const c of commands.filter((d) => !documentIsDeleted(d))) {
    const id = commandIdOf(c);
    await projectDocument(ctx, c, taskByCommandId.get(id) ?? null, projections);
    if (id !== '') projected.add(id);
  }
  for (const t of tasks.filter((d) => !documentIsDeleted(d))) {
    const commandId = valueString(t, 'command_id');
    if (projected.has(commandId)) continue;
    if (valueString(t, 'id') === '') continue;
    const command = commandById.get(commandId) ?? (await loadRecord(ctx, 'business_commands', commandId));
    await projectDocument(ctx, command ?? null, t, projections);
  }
  const seen = new Set();
  const pairs = [];
  for (const p of projections) {
    const key = `${p.collection}/${p.record_id}`;
    if (!seen.has(key)) { seen.add(key); pairs.push({ collection: p.collection, record_id: p.record_id }); }
  }
  return {
    changed_count: pairs.length,
    source_cursors: [{ collection: 'business_commands', since_ms: maxCommand }, { collection: 'ctox_queue_tasks', since_ms: maxTask }],
    projections: pairs,
  };
}
