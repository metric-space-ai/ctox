// outbound.sellify.remarks.failed — Rückruf (on_failed, auch Abbruch): Prüfvermerk mit Fehler am Lead.
import { handleRemarksFailed } from './_lib/sellify-remarks.js';

export default async function handle(command, ctx) {
  return handleRemarksFailed(command, ctx);
}
