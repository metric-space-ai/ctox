// threads.ctox_approval.approve — Port von approve_approval (threads.rs:1644-1752)
// + enqueue_approved_ctox_command (:3246-3392). Strang SKL-A2 (Kern §16.30), THR2 (Kern §16.40):
// - einziges Ziel business_os.chat.task (Agentenarbeit): der Handler reiht den Agenten-Task selbst ein
//   (ctx.cap.agent.task.enqueue, Anweisung aus prompts/chat-task.md) mit Rückruf threads.ctox_approval.complete/.failed.
// - Strang LOOP Runde 2 (Kern §16.44): kein fremdes Ziel mehr. ctox.coding.turn als Reviewer ist entfernt
//   (_approval_targets.js); Altbestände mit diesem Ziel lehnt approve als invalid ab, nichts wird gesendet.
// Ergebnis → Nachricht im Thread und, falls angefragt, Weiterleitung an den Rückruf der anfragenden App (callback).
import {
  actorId, actorPayload, approvalIdFromCommand, approvalSource, approvalTitle, clone, ensureApprovalTargetPolicy,
  ensureApprovalVersion, ensurePendingApproval, ensureReviewerOrAdmin, fail, firstStringField, has, isObject,
  loadApproval, newUuid, nonEmptyString, payloadOf, preflight, projectionValues, recordApprovalDecisionEvent,
  refreshThreadStates, sourceString, upsertMessage, upsertNotifications, upsertRecord, upsertThreadStatusDelta,
  valueString,
} from './_lib.js';
import { APPROVAL_COMPLETE, APPROVAL_FAILED, chatTaskInstruction } from './_agent.js';
import { AGENT_TASK_TARGET, ensureAllowedApprovalTarget } from './_approval_targets.js';

export { AGENT_TASK_TARGET };

/** Zieltyp der Freigabe (Vorgabe Agentenarbeit). */
const targetCommandTypeOf = (approval) => nonEmptyString(approval, 'target_command_type') ?? AGENT_TASK_TARGET;

export async function enqueueApprovedCommand(ctx, command, approval) {
  const source = approvalSource(approval);
  const prompt = valueString(approval, 'prompt');
  const instruction = valueString(approval, 'instruction');
  const targetModule = valueString(approval, 'target_module');
  const module = targetModule === '' ? (sourceString(source, 'module') ?? 'ctox') : targetModule;
  const recordId = valueString(approval, 'target_record_id');
  const commandType = ensureAllowedApprovalTarget(targetCommandTypeOf(approval));
  const approvalId = valueString(approval, 'approval_request_id');
  const threadId = valueString(approval, 'thread_id');
  const approvedCommandId = `cmd_${newUuid(ctx, command)}`;
  const payload = isObject(approval.target_payload) ? clone(approval.target_payload) : {};
  // serde Map::entry().or_insert: nur fehlende Schlüssel (null zählt als vorhanden).
  const orInsert = (key, value) => { if (!has(payload, key)) payload[key] = value; };
  orInsert('prompt', prompt);
  orInsert('instruction', instruction === '' ? prompt : instruction);
  orInsert('user_message', prompt);
  orInsert('title', approvalTitle(approval, prompt));
  orInsert('thread_key', threadId === '' ? `business-os/${module}` : `business-os/threads/${threadId}`);
  orInsert('context', clone(source));
  payload.approval = {
    approval_request_id: approvalId,
    approved_by: actorPayload(command),
    decision_command_id: typeof command.id === 'string' ? command.id : null,
    requester_user_id: has(approval, 'requester_user_id') ? approval.requester_user_id : null,
    reviewer_user_id: has(approval, 'reviewer_user_id') ? approval.reviewer_user_id : null,
  };
  const clientContext = isObject(approval.client_context) ? clone(approval.client_context) : {};
  Object.assign(clientContext, {
    actor: actorPayload(command),
    action: 'thread-approval-approved',
    approval_request_id: approvalId,
    source_module: module,
    module,
    module_id: module,
    app_id: module,
  });
  // Kern §16.30: Agentenarbeit als eigener Task der App mit Rückruf (statt Kern-Alias agent.task{action:'approve'}).
  const instructionText = await chatTaskInstruction(ctx, {
    instruction: typeof payload.instruction === 'string' ? payload.instruction : prompt,
    commandId: approvedCommandId, module, type: commandType, recordId, payload, clientContext,
  });
  const queued = await ctx.cap.agent.task.enqueue({
    title: typeof payload.title === 'string' && payload.title.trim() !== '' ? payload.title : approvalTitle(approval, prompt),
    instruction: instructionText,
    thread_key: typeof payload.thread_key === 'string' && payload.thread_key.trim() !== '' ? payload.thread_key : `business-os/threads/${threadId}`,
    payload: { approval_request_id: approvalId, thread_id: threadId, approved_command_id: approvedCommandId, module, record_id: recordId },
    idempotency_key: `threads.ctox_approval:${approvalId}`,
    on_complete: APPROVAL_COMPLETE,
    on_failed: APPROVAL_FAILED,
  });
  const queuedTaskId = typeof queued?.task_id === 'string' ? queued.task_id.trim() : '';
  if (queuedTaskId === '') fail('unavailable', 'agent task enqueue did not return task_id');
  return { ok: true, command_id: approvedCommandId, status: 'accepted', execution_mode: 'agent_task', task_id: queuedTaskId, task_status: 'queued' };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const approvalId = approvalIdFromCommand(command);
  const approval = await loadApproval(ctx, approvalId);
  ensurePendingApproval(approval);
  ensureApprovalVersion(command, approval);
  ensureReviewerOrAdmin(command, approval);
  ensureAllowedApprovalTarget(targetCommandTypeOf(approval)); // Altbestand aus der Zeit vor der festen Liste
  await ensureApprovalTargetPolicy(ctx, approval);

  // Rust: zuerst einreihen, dann Freigabe schreiben (ohne Transaktion um beides).
  const approved = await enqueueApprovedCommand(ctx, command, approval);
  const approvedCommandId = approved.command_id;
  const approvedTaskId = approved.task_id ?? '';

  const threadId = valueString(approval, 'thread_id');
  const source = approvalSource(approval);
  const prompt = valueString(approval, 'prompt');
  const decisionNote = firstStringField(payloadOf(command), ['decision_note', 'note']) ?? '';
  const messageId = `msg_${newUuid(ctx, command)}`;
  const requester = valueString(approval, 'requester_user_id');
  const projections = [];
  await ctx.transaction(async (tx) => {
    const next = clone(approval);
    next.status = 'approved';
    next.decided_at_ms = now;
    next.decision_by_id = actorId(command);
    next.decision_note = decisionNote;
    next.approved_command_id = approvedCommandId;
    next.approved_task_id = approvedTaskId;
    await upsertRecord(tx, 'ctox_task_approval_requests', approvalId, now, next);
    projections.push({ collection: 'ctox_task_approval_requests', record_id: approvalId });
    await upsertThreadStatusDelta(tx, threadId, 'open', messageId, now, -1, projections);
    await upsertMessage(tx, {
      threadId, messageId, kind: 'approval_approved', command, targetUserIds: [requester],
      body: `Freigegeben: ${prompt}`, source, approvalRequestId: approvalId, commandId: approvedCommandId, now,
    }, projections);
    await upsertNotifications(tx, {
      threadId, messageId, approvalRequestId: approvalId, notificationType: 'approval_approved',
      userIds: [requester], title: 'CTOX Freigabe erteilt', body: prompt, source, now,
    }, projections);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  await recordApprovalDecisionEvent(ctx, command, approval, 'approved', approvedCommandId, approvedTaskId, decisionNote, now);
  return {
    ok: true,
    approval_request_id: approvalId,
    thread_id: threadId,
    status: 'approved',
    approved_command_id: approvedCommandId,
    approved_task_id: approvedTaskId,
    approved_command: approved,
    projections: projectionValues(projections),
  };
}
