// project.jour_fixe.prepare – Tagesordnung eines bestehenden Jour-fixe-Termins (Strang JF, Owner-Regel E17).
// Vertrag: ~/.local/state/ctox-sync-v4/docs/evidence/harness-10-20261016/interfaces-open.json „jour_fixe“:
//   { id: stabile Operations-ID, record_id: bestehende Meeting-ID, payload: { project_id, meeting_id = record_id,
//     expected_revision? } } (expected_revision = Wire-Name aus PrepareRequest, Runde 2).
// Wirkung: Die Tagesordnung entsteht aus den offenen Themen genau dieses Projekts und wird in den BESTEHENDEN
// Datensatz workjet_jour_fixe_meetings/<record_id> geschrieben; kein neuer Datensatz, ID unverändert.
// Offene Themen = die Punkte der zuletzt vom Owner bestätigten To-do-Liste (TodoList.status 'confirmed') eines
// früheren Termins desselben Projekts – das ist das laufende Projektziel (Wire-Vertrag workjet-jour-fixe-v1.json,
// Todo/TodoList; Herleitung in v4/tools/textdiaet/JF.md). Keine KPI-/Brief-/Skill-Logik, kein Agentenauftrag.
import { fail, invalid, isPlainObject } from './_lib/errors.js';
import {
  AGENDA_SCHEMA,
  MAX_ID,
  MEETINGS,
  PAYLOAD_KEYS,
  PREPARABLE,
  PROJECTS,
  buildAgendaItems,
  openTopicSource,
  receipt,
} from './_lib/jour-fixe.js';

function requireId(value, field) {
  if (typeof value !== 'string' || value.length === 0 || value.length > MAX_ID) {
    throw invalid(`payload_invalid:${field}`);
  }
  return value;
}

export default async function handle(command, ctx) {
  const operationId = requireId(command?.id, 'id');
  const recordId = requireId(command?.record_id, 'record_id');
  const p = command?.payload;
  if (!isPlainObject(p)) throw invalid('payload_invalid:payload');
  for (const key of Object.keys(p)) if (!PAYLOAD_KEYS.has(key)) throw invalid(`payload_invalid:${key}`);
  const projectId = requireId(p.project_id, 'project_id');
  const meetingId = requireId(p.meeting_id, 'meeting_id');
  if (meetingId !== recordId) throw invalid('payload_invalid:meeting_id');
  const expectedRevision = p.expected_revision;
  if (expectedRevision !== undefined && !(Number.isSafeInteger(expectedRevision) && expectedRevision >= 0)) {
    throw invalid('payload_invalid:expected_revision');
  }

  return ctx.transaction(async (tx) => {
    const meeting = await tx.get(MEETINGS, recordId);
    if (!meeting || meeting.is_deleted === true) throw fail('not_found', 'meeting not found');
    if (meeting.id !== recordId) throw invalid('payload_invalid:record_id');
    if (meeting.project_id !== projectId) throw invalid('payload_invalid:project_id');
    // Wiederholung derselben Operation liefert den gespeicherten Beleg und schreibt nichts – vorrangig vor
    // expected_revision, auch wenn die Revision inzwischen gestiegen ist (Runde 2, Vertrag „Retrying an operation
    // returns its saved receipt“).
    if (meeting.prepared_operation_id === operationId && isPlainObject(meeting.agenda)) {
      return { ...receipt(meeting, meeting.agenda), replayed: true };
    }
    // Wire PrepareRequest.expected_revision (workjet-jour-fixe-v1.json:20): „stale revisions fail before effects“.
    const currentRevision = Number.isInteger(meeting.revision) ? meeting.revision : 0;
    if (expectedRevision !== undefined && expectedRevision !== currentRevision) {
      throw fail('stale_base', `expected_revision ${expectedRevision} != ${currentRevision}`);
    }
    const project = await tx.get(PROJECTS, projectId);
    if (!project || project.is_deleted === true) throw fail('not_found', 'project not found');
    if (project.owner_user_id !== meeting.owner_user_id) throw invalid('payload_invalid:project_id');
    if (project.status !== 'active') throw fail('conflict', 'project_not_active');
    if (!PREPARABLE.has(meeting.state)) throw fail('conflict', `meeting_state:${meeting.state}`);

    const projectMeetings = await tx.read(MEETINGS, { selector: { project_id: projectId } });
    const source = openTopicSource(projectMeetings, meeting);
    const now = ctx.now();
    const agenda = {
      schema: AGENDA_SCHEMA,
      operation_id: operationId,
      prepared_at_ms: now,
      source_meeting_id: source ? source.id : null,
      items: source ? buildAgendaItems(source) : [],
    };
    const goal = source?.todos?.goal;
    if (goal && typeof goal.goal_id === 'string' && goal.goal_id && Number.isInteger(goal.revision) && goal.revision >= 0) {
      agenda.source_goal = { goal_id: goal.goal_id, revision: goal.revision };
    }
    const revision = currentRevision + 1;
    await tx.patch(MEETINGS, recordId, {
      agenda,
      prepared_at_ms: now,
      prepared_operation_id: operationId,
      revision,
      updated_at_ms: now,
    });
    return { ...receipt({ ...meeting, revision }, agenda), replayed: false };
  });
}
