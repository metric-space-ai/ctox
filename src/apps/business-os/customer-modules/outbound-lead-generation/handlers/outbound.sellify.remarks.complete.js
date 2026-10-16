// outbound.sellify.remarks.complete — Rückruf (on_complete): Antwort lesen und das Urteil je Lead schreiben (vorher im Browser).
import { handleRemarksComplete } from './_lib/sellify-remarks.js';

export default async function handle(command, ctx) {
  return handleRemarksComplete(command, ctx);
}
