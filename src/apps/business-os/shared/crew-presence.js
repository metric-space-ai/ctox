// Crew presence: how a crew creature looks for a task-like subject (a chat, a
// queue task, a member) and which expression a crew member wears right now.
// Generic over the subject object: no collections, no app ids, no I/O. Used by
// the ctox app (map, members) and by the old shell chat bar (business-chat.js).
import { renderCrewCreature, crewModeForTaskState } from './crew-renderer.js';

// How long a member visibly reads its memory (attempt start) or learns
// (after the learning tick). Both are stamps from the projection, not guesses.
export const CREW_READING_WINDOW_MS = 20 * 1000;
export const CREW_LEARNING_WINDOW_MS = 90 * 1000;

// The one expression a member wears right now: reading (on duty, memory just
// read), learning (at home, just learned), running, failed or idle.
export function crewMemberExpression(member, nowMs = Date.now()) {
  const readAt = Number(member?.last_memory_read_at_ms) || 0;
  const learnedAt = Number(member?.last_learning_at_ms) || 0;
  if (member?.state === 'on_duty') {
    return readAt && nowMs - readAt >= 0 && nowMs - readAt < CREW_READING_WINDOW_MS ? 'reading' : 'running';
  }
  if (member?.state === 'resting_after_failure') return 'failed';
  return learnedAt && nowMs - learnedAt >= 0 && nowMs - learnedAt < CREW_LEARNING_WINDOW_MS ? 'learning' : 'idle';
}

// When the current expression ends (ms from now), or 0 when it does not decay.
export function crewMemberExpressionTtlMs(member, nowMs = Date.now()) {
  const expression = crewMemberExpression(member, nowMs);
  if (expression === 'reading') return Math.max(1, Number(member.last_memory_read_at_ms) + CREW_READING_WINDOW_MS - nowMs);
  if (expression === 'learning') return Math.max(1, Number(member.last_learning_at_ms) + CREW_LEARNING_WINDOW_MS - nowMs);
  return 0;
}

export function canonicalTrackingStatus(status) {
  const value = String(status || '').trim().toLowerCase();
  if (value === 'handled' || value === 'success' || value === 'done' || value === 'passed') return 'completed';
  if (value === 'leased' || value === 'processing' || value === 'executing' || value === 'active') return 'running';
  if (['waiting', 'pending_sync', 'pending', 'retry_wait', 'retry-wait', 'review_rework', 'review-rework'].includes(value)) return 'queued';
  return value;
}

// Every other Business OS surface (ctox, conversations, outbound) reports
// these as their own "blocked" state. The chat used to fold them into
// failure, which is how a waiting command came to read as a dead one.
export function isBlockedTrackingStatus(status) {
  return ['blocked', 'stale_missing_native'].includes(String(status || '').toLowerCase());
}

export function trackingIdFromMessage(message, kind) {
  if (!message || typeof message !== 'object') return '';
  const value = kind === 'command'
    ? message.commandId || message.command_id
    : message.taskId || message.task_id;
  return String(value || '').trim();
}

export function latestTrackingMessage(chat) {
  const messages = Array.isArray(chat?.messages) ? chat.messages : [];
  return [...messages].reverse().find((message) => (
    message?.executionProgress
    || message?.taskId
    || message?.commandId
  )) || null;
}

export function normalizeExecutionProgress(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const steps = Array.isArray(value.steps)
    ? value.steps.map((step, index) => ({
        position: Number(step?.position) || index + 1,
        label: String(step?.label || `Schritt ${index + 1}`),
        status: String(step?.status || 'pending').toLowerCase(),
        activity_turns: Math.max(0, Number(step?.activity_turns ?? step?.activityTurns) || 0),
      }))
    : [];
  if (!steps.length) return null;
  const rawActivity = value.activity_turns || value.activityTurns;
  const activity = rawActivity && typeof rawActivity === 'object'
    ? rawActivity
    : {};
  return {
    version: Number(value.version) || 1,
    revision: Math.max(1, Number(value.revision) || 1),
    phase: String(value.phase || 'work'),
    percent: Math.max(0, Math.min(100, Number(value.percent) || 0)),
    current_step: Number(value.current_step ?? value.currentStep) || null,
    completed_steps: Math.max(0, Number(value.completed_steps ?? value.completedSteps) || 0),
    total_steps: Math.max(steps.length, Number(value.total_steps ?? value.totalSteps) || 0),
    steps,
    review: value.review && typeof value.review === 'object'
      ? { status: String(value.review.status || 'pending').toLowerCase() }
      : { status: 'pending' },
    activity_turns: {
      total: Math.max(0, Number(activity.total) || 0),
      thinking: Math.max(0, Number(activity.thinking) || 0),
      tools: Math.max(0, Number(activity.tools) || 0),
      last_kind: String(activity.last_kind || activity.lastKind || value.lastActivityKind || '').toLowerCase(),
    },
    updated_at_ms: Math.max(0, Number(value.updated_at_ms ?? value.updatedAtMs) || 0),
  };
}

export function executionProgressForChat(chat) {
  const latest = latestTrackingMessage(chat);
  const taskId = trackingIdFromMessage(latest, 'task');
  const commandId = trackingIdFromMessage(latest, 'command');
  // Status and plan projections arrive independently. A new status without a
  // plan must not hide the last plan for this task, nor inherit another task's
  // plan when the conversation receives a follow-up command.
  const previousPlan = [...(chat?.messages || [])].reverse().find(message => (
    message.executionProgress && (taskId
      ? trackingIdFromMessage(message, 'task') === taskId
      : commandId && trackingIdFromMessage(message, 'command') === commandId)
  ));
  return normalizeExecutionProgress(
    chat?.executionProgress
    || chat?.execution_progress
    || latest?.executionProgress
    || previousPlan?.executionProgress,
  );
}

export function getTaskState(chat) {
  const isFuture = chat.createdAt > Date.now();
  const hasScheduledMessage = Array.isArray(chat.messages) && chat.messages.some(m => m.status === 'scheduled');
  if (isFuture && hasScheduledMessage) return 'scheduled';

  if (!chat.lastTrackingId) return 'idle';
  const trackingMsg = [...chat.messages].reverse().find(m => 
    (m.commandId && m.commandId === chat.lastTrackingId) || 
    (m.taskId && m.taskId === chat.lastTrackingId)
  );
  if (!trackingMsg) return 'idle';
  const status = canonicalTrackingStatus(trackingMsg.status);
  if (status === 'scheduled') return 'scheduled';
  if (!status) return 'idle';
  if (status === 'success' || status === 'completed' || status === 'handled' || status === 'done' || status === 'erledigt') return 'success';
  if (isBlockedTrackingStatus(status)) return 'blocked';
  if (['failed', 'error'].includes(status)) return 'failed';
  if (['queued', 'pending', 'pending_sync', 'waiting', 'terminal'].includes(status)) return 'queued';
  if (['running', 'processing', 'executing', 'active'].includes(status)) return 'running';
  return 'idle';
}

export function crewIdentityKey(subject = {}) {
  const tracking = Array.isArray(subject.messages) ? latestTrackingMessage(subject) : null;
  return subject.crewKey
    || subject.commandId
    || subject.command_id
    || tracking?.commandId
    || tracking?.command_id
    || subject.taskId
    || subject.task_id
    || tracking?.taskId
    || tracking?.task_id
    || subject.lastTrackingId
    || subject.id
    || subject.createdAt
    || subject.title
    || 'ctox-crew';
}

export function crewCreatureMode(chat, taskState = getTaskState(chat)) {
  const phase = taskState === 'running'
    ? executionProgressForChat(chat)?.phase || chat?.executionPhase || chat?.execution_phase || chat?.routeStatus || chat?.status || ''
    : '';
  return crewModeForTaskState(taskState, phase);
}

export function executionActivityTelemetry(chat) {
  const progress = executionProgressForChat(chat);
  return {
    total: Math.max(0, Number(progress?.activity_turns?.total) || 0),
    thinking: Math.max(0, Number(progress?.activity_turns?.thinking) || 0),
    tools: Math.max(0, Number(progress?.activity_turns?.tools) || 0),
    lastKind: ['thinking', 'tool'].includes(progress?.activity_turns?.last_kind)
      ? progress.activity_turns.last_kind
      : '',
    updatedAt: Math.max(0, Number(progress?.updated_at_ms) || 0),
  };
}

export function crewCreatureHtml(chat, taskState = getTaskState(chat), placement = 'dock') {
  return renderCrewCreature({
    // The raw identity: the renderer decides member vs. neutral ghost. The
    // normalized one already carries the neutral name "Crew" and would turn
    // every unassigned task into a grey member called Crew.
    appearance: chat?.crewIdentity || null,
    animationKey: crewIdentityKey(chat),
    taskState,
    mode: crewCreatureMode(chat, taskState),
    placement,
    progressPercent: executionProgressForChat(chat)?.percent || 0,
    activity: executionActivityTelemetry(chat),
  });
}
