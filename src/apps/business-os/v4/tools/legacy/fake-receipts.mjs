// Origin: CTOX
// License: AGPL-3.0-only
// Prüfliste: Belege, die der alte Beleg-Upload der Buchhaltung erfunden hat (bis 9130067bf). Er legte jede Datei als
// Hetzner-Rechnung HET-2026-98127 (bzw. Telekom TEL-7816-12, wenn der Dateiname „telekom“/„phone“ enthielt) vom
// 22.05.2026 an, mit Ablagepfad runtime/business-os/buchhaltung/storage/<datei> ohne gespeicherte Datei.
// Nur lesen: das Skript nennt Belege und daraus erzeugte Buchungen (ref_type 'receipt'), es ändert und löscht nichts.
//
// Aufruf: node fake-receipts.mjs <export.json>
//   export.json = { accounting_receipts: [...], accounting_journal_entries: [...], accounting_journal_entry_lines: [...] }

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const LEGACY_INVOICE_NUMBERS = Object.freeze(['HET-2026-98127', 'TEL-7816-12']);
export const LEGACY_INVOICE_DATE = '2026-05-22';
export const LEGACY_STORAGE_PREFIX = 'runtime/business-os/buchhaltung/storage/';

/** Ein Beleg gilt als erfunden, wenn Rechnungsnummer, Datum UND Ablagepfad dem alten Upload entsprechen. */
export function isLegacyFakeReceipt(receipt) {
  return Boolean(receipt)
    && LEGACY_INVOICE_NUMBERS.includes(receipt.invoice_number)
    && receipt.invoice_date === LEGACY_INVOICE_DATE
    && String(receipt.file_storage_url || '').startsWith(LEGACY_STORAGE_PREFIX);
}

export function legacyFakeReceiptReport(data) {
  const receipts = (data.accounting_receipts || []).filter((r) => !r?.deleted && isLegacyFakeReceipt(r));
  const ids = new Set(receipts.map((r) => r.id));
  const entries = (data.accounting_journal_entries || []).filter((e) => e?.ref_type === 'receipt' && ids.has(e.ref_id));
  const entryIds = new Set(entries.map((e) => e.id));
  const lines = (data.accounting_journal_entry_lines || []).filter((l) => entryIds.has(l.journal_entry_id));
  return {
    receipts: receipts.map((r) => ({ id: r.id, filename: r.filename || null, invoice_number: r.invoice_number, status: r.status || null })),
    journal_entries: entries.map((e) => ({ id: e.id, receipt_id: e.ref_id, number: e.number || null, reversed_by_id: e.reversed_by_id || null })),
    journal_entry_lines: lines.length,
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const file = process.argv[2];
  if (!file) { console.error('Aufruf: node fake-receipts.mjs <export.json>'); process.exit(64); }
  console.log(JSON.stringify(legacyFakeReceiptReport(JSON.parse(readFileSync(file, 'utf8'))), null, 2));
}
