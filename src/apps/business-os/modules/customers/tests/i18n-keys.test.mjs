// FIX7: jeder statisch benutzte Textschlüssel (state.t/t/tr/text./data-copy/data-i18n) existiert im eingebetteten
// Wörterbuch (de/en) UND in locales/de.json/en.json – sonst rendert der Rahmen den rohen Schlüssel
// („searchOpportunities“). Prüft customers, esign und support (Prüfskript v4/tools/textdiaet/check-i18n-keys.mjs).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkApp } from '../../../v4/tools/textdiaet/check-i18n-keys.mjs';

const MODULES = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

for (const app of ['customers', 'esign', 'support']) {
  test(`${app}: keine fehlenden Übersetzungsschlüssel`, () => {
    const r = checkApp(join(MODULES, app));
    assert.ok(r.used.length > 10, `Schlüssel gefunden (${r.used.length})`);
    assert.deepEqual(r.missing, { 'embedded.de': [], 'embedded.en': [], 'locale.de': [], 'locale.en': [] });
  });
}

test('customers: searchOpportunities hat einen deutschen Text (Fund aus der welsch-Stichprobe)', () => {
  const r = checkApp(join(MODULES, 'customers'));
  assert.ok(r.used.includes('searchOpportunities'));
  assert.equal(r.sources['embedded.de'].searchOpportunities, 'Verkaufschancen suchen');
});
