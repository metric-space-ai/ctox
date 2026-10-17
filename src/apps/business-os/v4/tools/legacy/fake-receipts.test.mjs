import test from 'node:test';
import assert from 'node:assert/strict';
import { legacyFakeReceiptReport, isLegacyFakeReceipt } from './fake-receipts.mjs';

const fake = { id: 'r1', filename: 'scan.pdf', invoice_number: 'HET-2026-98127', invoice_date: '2026-05-22', file_storage_url: 'runtime/business-os/buchhaltung/storage/scan.pdf', status: 'posted' };
const fakeTel = { id: 'r2', invoice_number: 'TEL-7816-12', invoice_date: '2026-05-22', file_storage_url: 'runtime/business-os/buchhaltung/storage/telekom.pdf' };
// Echte Hetzner-Rechnung mit derselben Nummer, aber gespeicherter Datei: nicht erfunden.
const real = { id: 'r3', invoice_number: 'HET-2026-98127', invoice_date: '2026-05-22', file_storage_url: 'sha256:abc' };
const other = { id: 'r4', invoice_number: 'RE-4711', invoice_date: '2026-10-14', file_storage_url: 'sha256:def' };

test('erkennt nur Belege mit Nummer, Datum und Ablagepfad des alten Uploads', () => {
  assert.equal(isLegacyFakeReceipt(fake), true);
  assert.equal(isLegacyFakeReceipt(fakeTel), true);
  assert.equal(isLegacyFakeReceipt(real), false);
  assert.equal(isLegacyFakeReceipt(other), false);
  assert.equal(isLegacyFakeReceipt({ ...fake, invoice_date: '2026-05-23' }), false);
});

test('nennt Belege und ihre Buchungen, ignoriert gelöschte und fremde', () => {
  const report = legacyFakeReceiptReport({
    accounting_receipts: [fake, fakeTel, real, other, { ...fake, id: 'r5', deleted: true }],
    accounting_journal_entries: [
      { id: 'e1', ref_type: 'receipt', ref_id: 'r1', number: 'J-2026-0001' },
      { id: 'e2', ref_type: 'receipt', ref_id: 'r3', number: 'J-2026-0002' },
      { id: 'e3', ref_type: 'bank', ref_id: 'r1' },
    ],
    accounting_journal_entry_lines: [
      { id: 'l1', journal_entry_id: 'e1' }, { id: 'l2', journal_entry_id: 'e1' }, { id: 'l3', journal_entry_id: 'e2' },
    ],
  });
  assert.deepEqual(report.receipts.map((r) => r.id), ['r1', 'r2']);
  assert.deepEqual(report.journal_entries.map((e) => e.id), ['e1']);
  assert.equal(report.journal_entry_lines, 2);
});
