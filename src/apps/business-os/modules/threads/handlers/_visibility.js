// Sichtbarkeitsregel der threads-eigenen Collections je Nutzer (Kandidat für Wunsch W-5).
// Port des threads-Zweigs von actor_may_replicate_document_using_reader (threads.rs:575-620) +
// thread_record_visible_to_user (:4803) / thread_document_visible_with_reader (:4787).
// NICHT portiert (Core/andere Apps): guest_frames, Browser-, communication_*-, business_commands-,
// ctox_queue_tasks-Sichtbarkeit, Workjet-Privatsphäre (private_visibility), Rollen-/Grant-Prüfung.
// Aufrufvertrag (Vorschlag): Core ruft visible(...) erst NACH seinen eigenen Prüfungen
// (Admin/Chef → sichtbar; collection_read_allowed) und nur für die sechs Collections unten.
import { arrayStrings, valueString } from './_lib.js';

export const THREADS_OWNED_COLLECTIONS = [
  'user_threads', 'user_thread_messages', 'user_thread_links', 'user_notifications', 'user_thread_states',
  'ctox_task_approval_requests',
];

/** thread_record_visible_to_user (:4803). */
export function threadRecordVisibleToUser(thread, userId) {
  if (String(userId ?? '').trim() === '') return false;
  return valueString(thread, 'owner_user_id') === userId
    || valueString(thread, 'created_by_id') === userId
    || valueString(thread, 'assigned_user_id') === userId
    || arrayStrings(thread?.participant_ids).includes(userId)
    || arrayStrings(thread?.watcher_user_ids).includes(userId);
}

async function threadVisible(read, document, userId) {
  const threadId = valueString(document, 'thread_id');
  if (threadId === '') return false;
  try {
    const thread = await read('user_threads', threadId);
    return thread ? threadRecordVisibleToUser(thread, userId) : false;
  } catch {
    return false;
  }
}

/**
 * visible(collection, document, userId, read) → boolean
 * read(collection, id) → Promise<doc|null>. Persönliche Projektionen (user_notifications,
 * user_thread_states) nur für ihren Besitzer – in Rust auch vor dem Collection-Grant (:589-591).
 */
export async function visible(collection, document, userId, read) {
  switch (collection) {
    case 'user_notifications':
    case 'user_thread_states':
      return valueString(document, 'user_id') === userId;
    case 'ctox_task_approval_requests':
      return valueString(document, 'requester_user_id') === userId
        || valueString(document, 'reviewer_user_id') === userId
        || valueString(document, 'decision_by_id') === userId
        || threadVisible(read, document, userId);
    case 'user_threads':
      return threadRecordVisibleToUser(document, userId);
    case 'user_thread_messages':
      return valueString(document, 'author_user_id') === userId
        || arrayStrings(document?.target_user_ids).includes(userId)
        || threadVisible(read, document, userId);
    case 'user_thread_links':
      return threadVisible(read, document, userId);
    default:
      return false;
  }
}

export default visible;
