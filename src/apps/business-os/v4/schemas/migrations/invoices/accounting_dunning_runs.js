// Migration invoices/accounting_dunning_runs: Basis main@4794af03d (Schema v0) → Schema v1, neue Generation (§16.4).
// Erzeugt von v4/schemas/build-schemas.mjs (Strang SCH). Im App-Paket: migrations/accounting_dunning_runs.js. Gründe:
//   - Typ von „invoices_total“: number → array
//   - Feld „updated_at_ms“ neu ohne Default
export default function migrate(doc) {
  // invoices_total: number → string[] (Rust schreibt die Liste der gemahnten Rechnungs-IDs, invoices.rs:1693, :1704).
  // Einziger Schreiber ist Rust (immer Array); ein Zahl-Altwert ist nicht verlustfrei abbildbar und bleibt als
  // invoices_total_count erhalten.
  if (Array.isArray(doc.invoices_total)) return doc;
  const next = { ...doc, invoices_total: [] };
  if (typeof doc.invoices_total === 'number') next.invoices_total_count = doc.invoices_total;
  return next;
}
