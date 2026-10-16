// Gemeinsame Teile von project.jour_fixe.prepare (Strang JF): Collections, Quelle der offenen Themen, Reihenfolge.
import { isPlainObject } from './errors.js';

export const MEETINGS = 'workjet_jour_fixe_meetings';
export const PROJECTS = 'workjet_projects';
export const AGENDA_SCHEMA = 'ctox.workjet.jour_fixe.agenda.v1';
export const MAX_ID = 128;
const MAX_ITEMS = 100;
export const PAYLOAD_KEYS = new Set(['project_id', 'meeting_id', 'expected_revision', 'trigger']);
// Vorbereiten nur vor dem Termin bzw. nach einer benannten Vorbereitungs-Störung (MeetingState im Wire-Vertrag).
export const PREPARABLE = new Set(['planned', 'preparing', 'ready', 'failed']);
const PRIORITY_RANK = { P0: 0, P1: 1, P2: 2 };
const rankOf = (priority) => (typeof priority === 'string' && Object.hasOwn(PRIORITY_RANK, priority) ? PRIORITY_RANK[priority] : 3);

const finiteOr = (value, fallback) => (Number.isFinite(value) ? value : fallback);

// Deterministische Reihenfolge: Priorität (P0 vor P1 vor P2), dann Fälligkeit (früher zuerst, ohne Datum zuletzt),
// dann Alter = Position in der bestätigten Liste, zuletzt die Themen-ID.
export function buildAgendaItems(sourceMeeting) {
  const items = Array.isArray(sourceMeeting?.todos?.items) ? sourceMeeting.todos.items : [];
  return items
    .map((todo, index) => ({ todo, index }))
    .filter(({ todo }) => isPlainObject(todo) && typeof todo.id === 'string' && todo.id && typeof todo.title === 'string' && todo.title)
    .sort((a, b) => {
      const pa = rankOf(a.todo.priority);
      const pb = rankOf(b.todo.priority);
      if (pa !== pb) return pa - pb;
      const da = finiteOr(a.todo.due_at_ms, Number.POSITIVE_INFINITY);
      const db = finiteOr(b.todo.due_at_ms, Number.POSITIVE_INFINITY);
      if (da !== db) return da - db;
      if (a.index !== b.index) return a.index - b.index;
      return a.todo.id < b.todo.id ? -1 : a.todo.id > b.todo.id ? 1 : 0;
    })
    .slice(0, MAX_ITEMS)
    .map(({ todo }, position) => {
      const item = {
        position,
        topic_id: todo.id,
        source_meeting_id: sourceMeeting.id,
        title: todo.title,
        priority: rankOf(todo.priority) < 3 ? todo.priority : 'P2',
      };
      if (Number.isInteger(todo.due_at_ms) && todo.due_at_ms >= 0) item.due_at_ms = todo.due_at_ms;
      return item;
    });
}

// Quelle der offenen Themen: jüngster früherer, nicht gelöschter Termin desselben Projekts und Owners mit
// bestätigter To-do-Liste. Spätere Bestätigungen ersetzen frühere Ziele (Wire-Vertrag: „A stale confirmation
// cannot replace a newer goal“), deshalb zählt nur die jüngste.
export function openTopicSource(meetings, meeting) {
  const candidates = meetings.filter(
    (m) =>
      m &&
      m.id !== meeting.id &&
      m.is_deleted !== true &&
      m.project_id === meeting.project_id &&
      m.owner_user_id === meeting.owner_user_id &&
      finiteOr(m.scheduled_at_ms, Number.POSITIVE_INFINITY) < finiteOr(meeting.scheduled_at_ms, 0) &&
      m.todos?.status === 'confirmed',
  );
  candidates.sort((a, b) => b.scheduled_at_ms - a.scheduled_at_ms || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  return candidates[0] ?? null;
}

export function receipt(meeting, agenda) {
  return {
    operation_id: agenda.operation_id,
    meeting_id: meeting.id,
    project_id: meeting.project_id,
    revision: meeting.revision,
    state: meeting.state,
    source_meeting_id: agenda.source_meeting_id,
    agenda_items: agenda.items.length,
  };
}

