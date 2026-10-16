// ctox.app.modify.request – Standardbefehl der ctox-App „App ändern“ (Strang SKL-D). Sender: der Baustein
// ctox:shared/context-chat.js (Modus app). Rechteprüfung hier (Rolle owner/admin/chef oder apps.source.edit), Template
// prompts/app-modify.md, generischer System-Skill business-os-app-module-development (src/skills/system/
// product_engineering/), Rückruf ctox.chat.task.complete / .failed. Ersetzt ctox.business_os.app.modify ohne Handler.
import { enqueueContextTask } from './_lib/context-chat.js';

export default async function handle(command, ctx) {
  return enqueueContextTask(command, ctx);
}
