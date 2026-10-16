// invoices.dunning.letter.send – Port aus invoices.rs invoices_dunning_letter_send_stub :1750.
// Rust-Name "_stub" und HALBER STUB (treu übernommen): markiert den Brief als per E-Mail gesendet und
// zaehlt den Mahnlauf hoch – OHNE Rendern und OHNE Versand. Kein ctx.cap.mail.send-Aufruf, weil Rust
// keinen Versand macht (Wunsch fuer spaeter: ctx.cap.office.render + ctx.cap.mail.send, siehe V4-HANDLERS.md).
import { COL, asI64, ensureModule, fail, loadRecord, recordRequirement, str, upsertRecord } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const letterId = recordRequirement(command, ['letter_id', 'id']);
  return ctx.transaction(async (tx) => {
    const letter = await loadRecord(tx, COL.dunningLetters, letterId);
    if (!letter) fail('not_found', `dunning letter ${letterId} not found`);
    const now = ctx.now();
    letter.sent_via = 'email';
    letter.sent_at_ms = now;
    letter.status = 'sent';
    letter.updated_at_ms = now;
    await upsertRecord(tx, COL.dunningLetters, letterId, now, letter);
    const runId = str(letter.dunning_run_id);
    if (runId !== undefined) {
      const run = await loadRecord(tx, COL.dunningRuns, runId);
      if (run) {
        run.letters_sent = (asI64(run.letters_sent) ?? 0) + 1;
        run.state = 'executed';
        run.updated_at_ms = now;
        await upsertRecord(tx, COL.dunningRuns, runId, now, run);
      }
    }
    return { ok: true, letter };
  });
}
