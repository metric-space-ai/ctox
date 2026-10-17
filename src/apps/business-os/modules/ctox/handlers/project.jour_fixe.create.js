// project.jour_fixe.create – legt einen geplanten Jour-fixe-Termin an (Strang JF Runde 2, E17/E23).
// Zustellung nur durch den Kern (Zeitplan der Vorbereitung, AppRuntimeIngress); kein UI-Aufruf, kein Agentenauftrag.
// Vertrag: { id: stabile Operations-ID, record_id: neue meeting_id (vom Absender vergeben),
//   payload: Meeting-Kopf nach workjet-jour-fixe-v1.json types.Meeting { project_id, owner_user_id,
//   supervisor {workjet_thread_id, ctox_thread_key}, scheduled_at_ms, prepare_at_ms, timezone, previous_goal?,
//   id?/meeting_id? (= record_id) } }.
// Zustand, Revisionen, Folien, Kommentare, Transkript und To-dos setzt nie der Absender (Wire: „Commands cannot
// supply owner/confirming-user identity“; Anlage wie jour_fixe_preparation.rs:95-119: planned, revision 0).
// Idempotenz wie jour_fixe_preparation.rs:124-143: gleicher Kopf → bestehender Termin (unverändert, auch wenn er
// inzwischen vorbereitet wurde), anderer Kopf unter derselben meeting_id → conflict.
import { fail, invalid, isPlainObject } from './_lib/errors.js';
import { MAX_ID, MEETINGS, PROJECTS } from './_lib/jour-fixe.js';

const PAYLOAD_KEYS = new Set([
  'id',
  'meeting_id',
  'project_id',
  'owner_user_id',
  'supervisor',
  'scheduled_at_ms',
  'prepare_at_ms',
  'timezone',
  'previous_goal',
  'trigger',
]);

function text(value, field, max) {
  if (typeof value !== 'string' || value.length === 0 || [...value].length > max) throw invalid(`payload_invalid:${field}`);
  return value;
}

function millis(value, field) {
  if (!Number.isSafeInteger(value) || value < 0) throw invalid(`payload_invalid:${field}`);
  return value;
}

function onlyKeys(value, keys, field) {
  if (!isPlainObject(value)) throw invalid(`payload_invalid:${field}`);
  for (const key of Object.keys(value)) if (!keys.includes(key)) throw invalid(`payload_invalid:${field}.${key}`);
  return value;
}

// Kopf des Wire-Meetings, geprüft wie die generierten Grenzen (Meeting/SupervisorRef/GoalRef).
function meetingHead(command) {
  const id = text(command?.id, 'id', MAX_ID);
  const recordId = text(command?.record_id, 'record_id', MAX_ID);
  const p = command?.payload;
  if (!isPlainObject(p)) throw invalid('payload_invalid:payload');
  for (const key of Object.keys(p)) if (!PAYLOAD_KEYS.has(key)) throw invalid(`payload_invalid:${key}`);
  for (const key of ['id', 'meeting_id']) if (p[key] !== undefined && p[key] !== recordId) throw invalid(`payload_invalid:${key}`);
  const supervisor = onlyKeys(p.supervisor, ['workjet_thread_id', 'ctox_thread_key'], 'supervisor');
  const head = {
    id: recordId,
    project_id: text(p.project_id, 'project_id', 128),
    owner_user_id: text(p.owner_user_id, 'owner_user_id', 256),
    supervisor: {
      workjet_thread_id: text(supervisor.workjet_thread_id, 'supervisor.workjet_thread_id', 128),
      ctox_thread_key: text(supervisor.ctox_thread_key, 'supervisor.ctox_thread_key', 512),
    },
    scheduled_at_ms: millis(p.scheduled_at_ms, 'scheduled_at_ms'),
    prepare_at_ms: millis(p.prepare_at_ms, 'prepare_at_ms'),
    timezone: text(p.timezone, 'timezone', 128),
  };
  if (head.prepare_at_ms > head.scheduled_at_ms) throw invalid('payload_invalid:prepare_at_ms');
  if (p.previous_goal !== undefined) {
    const goal = onlyKeys(p.previous_goal, ['goal_id', 'revision'], 'previous_goal');
    head.previous_goal = { goal_id: text(goal.goal_id, 'previous_goal.goal_id', 128), revision: millis(goal.revision, 'previous_goal.revision') };
  }
  return { operationId: id, head };
}

const sameGoal = (a, b) => (a === undefined && b === undefined) || (a && b && a.goal_id === b.goal_id && a.revision === b.revision);

function sameHead(existing, head) {
  return (
    existing.project_id === head.project_id &&
    existing.owner_user_id === head.owner_user_id &&
    existing.supervisor?.workjet_thread_id === head.supervisor.workjet_thread_id &&
    existing.supervisor?.ctox_thread_key === head.supervisor.ctox_thread_key &&
    existing.scheduled_at_ms === head.scheduled_at_ms &&
    existing.prepare_at_ms === head.prepare_at_ms &&
    existing.timezone === head.timezone &&
    Boolean(sameGoal(existing.previous_goal, head.previous_goal))
  );
}

const receipt = (operationId, meeting, replayed) => ({
  operation_id: operationId,
  meeting_id: meeting.id,
  project_id: meeting.project_id,
  revision: meeting.revision,
  state: meeting.state,
  replayed,
});

export default async function handle(command, ctx) {
  const { operationId, head } = meetingHead(command);
  return ctx.transaction(async (tx) => {
    const existing = await tx.get(MEETINGS, head.id);
    if (existing) {
      if (existing.is_deleted === true || !sameHead(existing, head)) throw fail('conflict', 'meeting_id_conflict');
      return receipt(operationId, existing, true);
    }
    // Dieselbe Operations-ID darf keinen zweiten Termin anlegen.
    const [reused] = await tx.read(MEETINGS, { selector: { created_operation_id: operationId }, limit: 1 });
    if (reused) throw fail('conflict', 'operation_id_conflict');
    const project = await tx.get(PROJECTS, head.project_id);
    if (!project || project.is_deleted === true) throw fail('not_found', 'project not found');
    if (project.owner_user_id !== head.owner_user_id) throw invalid('payload_invalid:owner_user_id');
    if (project.status !== 'active') throw fail('conflict', 'project_not_active');
    const now = ctx.now();
    const meeting = {
      ...head,
      state: 'planned',
      revision: 0,
      deck_revision: 0,
      created_operation_id: operationId,
      created_at_ms: now,
      updated_at_ms: now,
      is_deleted: false,
    };
    await tx.put(MEETINGS, meeting);
    return receipt(operationId, meeting, false);
  });
}
