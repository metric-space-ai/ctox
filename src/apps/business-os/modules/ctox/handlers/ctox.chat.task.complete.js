// ctox.chat.task.complete – on_complete-Rückruf von ctox.chat.task und ctox.app.modify.request (Kern §16.30 Nr. 3),
// kein UI-Sender. Schreibt die Antwort als Nachricht (role 'ctox', replyFor = task_id) in business_chats; live bei
// allen Clients. Idempotent: eine zweite Zustellung derselben task_id schreibt nichts.
import { completeContextTask } from './_lib/context-chat.js';

export default async function handle(command, ctx) {
  return completeContextTask(command, ctx);
}
