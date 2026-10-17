import { createCtx, loadSchema } from './_ctx.mjs';

export const NOW = 1_760_000_000_000; // 2025-10-09
export const schemaUrl = new URL('../../collections.schema.json', import.meta.url);

// Entwurf wie im Rust-Test post_writes_journal_entry_and_marks_invoice_posted (invoices.rs:1964)
export const line = (o = {}) => ({
  id: 'l1', position: 1, description: 'Beratung', quantity: 1000, unit: 'h', unit_price_cents: 12000, tax_rate: 0.19, account_code: '8400', ...o,
});
export const draftPayload = (o = {}) => ({
  invoice_id: 'inv_post', invoice_type: 'sale_out', party_id: 'cust_p', invoice_date_ms: 1_700_000_000_000, currency: 'EUR', lines: [line()], ...o,
});

// strict=false schaltet die Schemapruefung ab: noetig, wo Rust-Datensaetze vom Schema abweichen
// (Befunde I4–I6, belegt in schema-befund.test.mjs).
export async function setup({ seed = {}, caps = {}, now = NOW, strict = true } = {}) {
  return createCtx({ seed, caps, now, schema: strict ? await loadSchema(schemaUrl) : null });
}
