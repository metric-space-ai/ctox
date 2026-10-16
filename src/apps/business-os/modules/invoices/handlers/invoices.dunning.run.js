// invoices.dunning.run – Port aus invoices.rs invoices_dunning_run_stub :1677.
// Rust-Name "_stub", aber mit Logik: Mahnlauf je run_id (idempotent), faellige Rechnungen
// (state = filter.state|overdue, due_date_ms < jetzt, open_cents > filter.min_open_cents|0),
// je Rechnung ein Brief Stufe 1 (Erinnerung) als Entwurf, Gebuehren/Zinsen 0.
import { COL, ensureModule, listDueInvoices, loadRecord, recordRequirement, upsertRecord } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const runId = recordRequirement(command, ['run_id', 'id']);
  return ctx.transaction(async (tx) => {
    const existingRun = await loadRecord(tx, COL.dunningRuns, runId);
    if (existingRun) return { ok: true, idempotent: true, run: existingRun };
    const now = ctx.now();
    const filter = command.payload?.filter !== undefined ? command.payload.filter : {};
    const invoicesTotal = await listDueInvoices(tx, filter, now);
    const run = {
      id: runId,
      run_date_ms: now,
      // Rust: client_context.actor.id -> v4 command.actor.id
      run_by: typeof command.actor?.id === 'string' ? command.actor.id : 'system',
      filter,
      invoices_total: invoicesTotal,
      letters_sent: 0,
      state: 'draft',
      created_at_ms: now,
      updated_at_ms: now,
    };
    await upsertRecord(tx, COL.dunningRuns, runId, now, run);
    const letters = [];
    for (const invoiceId of invoicesTotal) {
      const letterId = `let_${runId}_${invoiceId}`;
      const letter = {
        id: letterId,
        dunning_run_id: runId,
        invoice_id: invoiceId,
        level: 1,
        letter_date_ms: now,
        fee_cents: 0,
        interest_cents: 0,
        total_cents: 0,
        pdf_attachment_id: null,
        sent_via: null,
        sent_at_ms: null,
        status: 'draft',
        created_at_ms: now,
        updated_at_ms: now,
      };
      await upsertRecord(tx, COL.dunningLetters, letterId, now, letter);
      letters.push(letter);
    }
    return { ok: true, run, letters };
  });
}
