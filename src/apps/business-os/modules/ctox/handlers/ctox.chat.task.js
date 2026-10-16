// ctox.chat.task – Standardbefehl der ctox-App „Kontext-Auftrag an CTOX“ (Strang SKL-D). Sender: der Baustein
// ctox:shared/context-chat.js in jeder App (api.command), mail „An CTOX übergeben“, ctox „Aufgaben aus Datei“ und
// „Aufgabe fortsetzen“. Template prompts/chat-data.md bzw. chat-ask.md, kein Skill, Rückruf ctox.chat.task.complete /
// .failed; der Chat-Turn steht in business_chats. Ersetzt das rohe business_os.chat.task (store.rs:26700, record_command).
import { enqueueContextTask } from './_lib/context-chat.js';

export default async function handle(command, ctx) {
  return enqueueContextTask(command, ctx);
}
