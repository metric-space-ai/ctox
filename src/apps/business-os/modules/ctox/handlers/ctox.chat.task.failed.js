// ctox.chat.task.failed – on_failed-Rückruf von ctox.chat.task und ctox.app.modify.request (Kern §16.30 Nr. 3), auch
// Abbruch (status 'cancelled'). Schreibt den Fehlschlag als Nachricht (role 'ctox', failureFor = task_id) in
// business_chats. Idempotent.
import { failContextTask } from './_lib/context-chat.js';

export default async function handle(command, ctx) {
  return failContextTask(command, ctx);
}
