// Test für check-raw-ids.mjs (reine Erkennung, ohne Browser).
// Lauf: node --test src/apps/business-os/v4/tools/rundgang/check-raw-ids.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { findRawIds } from './check-raw-ids.mjs';

const values = (text) => findRawIds(text).map((h) => h.value);
const kinds = (text) => findRawIds(text).map((h) => h.kind);

test('meldet Roh-IDs: Slug mit Ziffern, UUID, langes Hex, Hash-Kürzel, snake_case', () => {
  assert.deepEqual(values('2026-01040 Gebucht party-001 · 12,74 €'), ['party-001']);
  assert.deepEqual(values('candidate-002 → client-account-002'), ['candidate-002', 'client-account-002']);
  assert.deepEqual(values('Anfrage: manualreq_1729_ab12cd'), ['manualreq_1729_ab12cd']);
  assert.deepEqual(values('Person cand__42 vorgestellt'), ['cand__42']);
  assert.deepEqual(findRawIds('Datensatz 3f2b8c1e-9a4d-4e7b-8f00-12ab34cd56ef geöffnet').map((h) => h.kind), ['uuid']);
  assert.deepEqual(kinds('Blob 9f86d081884c7d659a2feaa0c55ad015 geladen'), ['hex']);
  assert.deepEqual(values('Version a65ff419f · Freigegeben'), ['a65ff419f']);
  assert.deepEqual(values('Status: partially_signed'), ['partially_signed']);
  assert.deepEqual(values('Datenzugriff auf customer_accounts gesperrt'), ['customer_accounts']);
});

test('meldet keine Nutzertexte: Belegnummern, Datum, Beträge, Normen, Wörter', () => {
  for (const ok of [
    'RE-2026-0001 · 119,00 €',
    '14.10.2026, 08:00',
    '2026-10-14',
    'Kontenrahmen SKR03 · ISO 9001 · G25',
    'Alle(6) Offen(2) Bezahlt(1) Überfällig(1)',
    'Tel. 0151 2345678 · KW 42 · Seite 1/3',
    'Festanstellung (Personalvermittlung) · Honorar: 18.000,00 €',
    'deadbeef und facade', // Hex-Buchstaben ohne Ziffern sind Wörter
    'Termin 1234567 bestätigt', // reine Zahl ist kein Hash
    'E-Mail an max.mustermann@example.com',
  ]) assert.deepEqual(values(ok), [], ok);
});

test('jede Fundstelle nur einmal, mit Umfeld', () => {
  const hits = findRawIds('Kunde party-001 hat Rechnung inv_2026_001 und Hash abc1234def');
  assert.deepEqual(hits.map((h) => [h.kind, h.value]), [['slug-id', 'party-001'], ['slug-id', 'inv_2026_001'], ['hash', 'abc1234def']]);
  assert.match(hits[0].context, /Kunde party-001 hat/);
});
