import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const { runAllTests } = await import('../test.js');

const [html, js, css, manifest] = await Promise.all([
  readFile(new URL('../index.html', import.meta.url), 'utf8'),
  readFile(new URL('../index.js', import.meta.url), 'utf8'),
  readFile(new URL('../index.css', import.meta.url), 'utf8'),
  readFile(new URL('../module.json', import.meta.url), 'utf8').then(JSON.parse),
]);

test('existing accounting core suite remains green', () => {
  assert.deepEqual(runAllTests(), { passed: 9, failed: 0 });
});

test('left booking selector uses the complete shell-owned pane grammar', () => {
  assert.match(html, /data-booking-pane/);
  assert.match(html, /data-pg-search/);
  // Betreiber-Direktive 31.08.2026: die Darstellung wird von EINEM Aktionsknopf
  // umgeschaltet, nicht von einem aria-pressed-Paar. Der Wächter misst jetzt
  // genau diesen Vertrag statt des abgeloesten Zwei-Knopf-Umschalters.
  assert.doesNotMatch(html, /data-pg-view=/, 'kein Zwei-Knopf-Umschalter mehr');
  assert.doesNotMatch(html, /ctox-view-toggle/, 'die Umschaltgruppe entfaellt');
  assert.equal(html.match(/data-fibu-view-toggle/g)?.length, 1, 'genau ein Umschalt-Knopf');
  assert.match(html, /data-fibu-view-toggle[^>]*data-fibu-view="cards"/);
  assert.doesNotMatch(html, /data-fibu-view-toggle[^>]*aria-pressed/, 'der Knopf ist Aktion, kein Zustand');
  assert.match(html, /data-fibu-view-toggle[^>]*aria-label="Als Liste anzeigen"[^>]*title="Als Liste anzeigen"/);
  assert.match(js, /const label = view === 'list' \? 'Als Karten anzeigen' : 'Als Liste anzeigen'/);
  assert.match(js, /setBookingView\(state\.bookingView === 'list' \? 'cards' : 'list'\)/);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-tray\b[^>]*hidden/);
  assert.match(html, /data-pg-reset/);
  assert.match(html, /class="ctox-pane-body ctox-well"/);
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt');
  assert.doesNotMatch(html, /data-action="refresh|manual-refresh|neu laden/i);
});

test('booking status band exposes real counted views including zero-capable spans', () => {
  for (const key of ['all', 'draft', 'posted']) {
    assert.match(html, new RegExp(`data-pg-band="${key}"`));
    assert.match(html, new RegExp(`data-pg-count="${key}"`));
  }
  assert.match(js, /draft:\s*scoped\.filter\(\(record\) => record\.status === 'draft'\)\.length/);
  assert.match(js, /posted:\s*scoped\.filter\(\(record\) => record\.status === 'posted'\)\.length/);
  assert.match(js, /grammarHandle\?\.setCounts\?\.\(counts\)/);
  assert.match(js, /node\.textContent = ` \(\$\{value\}\)`/);
});

test('header provides compact create, JSON import and JSON export flows', () => {
  assert.match(html, /class="ctox-pane-actions"[\s\S]*data-action="new-booking"[\s\S]*data-action="import-bookings"[\s\S]*data-action="export-bookings"/);
  assert.match(html, /data-booking-import-input[^>]*accept="application\/json,\.json"[^>]*hidden/);
  assert.match(js, /addEventListener\('change', importBookingRecords\)/);
  assert.match(js, /function exportBookingRecords\(\)/);
  assert.match(js, /new Blob\(\[JSON\.stringify\(value, null, 2\)\]/);
  assert.match(js, /format: 'ctox-buchhaltung-records-v1'/);
  assert.match(js, /insertMissingRecords\('accounting_journal_entries'/);
  assert.match(js, /insertMissingRecords\('accounting_journal_entry_lines'/);
  assert.match(js, /insertMissingRecords\('accounting_receipts'/);
});

test('booking selection flips existing rows in place and never rebuilds the list', () => {
  const helper = js.match(/function setSelectedBookingRow\(selectedKey\) \{([\s\S]*?)\n\}/)?.[1] || '';
  const selector = js.match(/function selectBookingRecord\(key\) \{([\s\S]*?)\n\}/)?.[1] || '';
  assert.match(helper, /querySelectorAll\('\[data-booking-key\]'\)/);
  assert.match(helper, /classList\.toggle\('is-selected', selected\)/);
  assert.match(helper, /setAttribute\('aria-selected', String\(selected\)\)/);
  assert.doesNotMatch(helper, /innerHTML|replaceChildren|renderBookingList/);
  assert.match(selector, /setSelectedBookingRow\(key\)/);
  assert.doesNotMatch(selector, /renderBookingList|innerHTML|replaceChildren/);
});

test('selection-driven evidence pane follows visible = hasSelection && !userCollapsed', () => {
  assert.match(html, /class="ctox-workspace fibu-module is-evidence-hidden"/);
  assert.match(html, /data-toggle-evidence[^>]*hidden/);
  assert.match(html, /data-collapse-evidence/);
  assert.match(js, /const visible = hasSelection && !state\.evidenceUserCollapsed/);
  assert.match(js, /classList\.toggle\('is-evidence-hidden', !visible\)/);
  assert.match(js, /state\.els\.evidenceToggle\.hidden = !hasSelection/);
  assert.match(js, /selectReceipt[\s\S]*updateEvidencePaneVisibility\(\)/);
  assert.match(js, /selectJournalEntry[\s\S]*updateEvidencePaneVisibility\(\)/);
});

test('L-class pane tracks are explicit and the center keeps a hard minimum', () => {
  assert.match(css, /grid-template-columns:\s*var\(--ctox-left-width\) 12px minmax\(360px, 1fr\) 12px var\(--ctox-right-width\)/);
  assert.match(css, /\.fibu-left\s*\{[\s\S]*grid-column:\s*1/);
  assert.match(css, /\[data-resizer="left"\]\s*\{\s*grid-column:\s*2/);
  assert.match(css, /\.fibu-center\s*\{[\s\S]*grid-column:\s*3[\s\S]*min-width:\s*360px/);
  assert.match(css, /\[data-resizer="right"\]\s*\{\s*grid-column:\s*4/);
  assert.match(css, /\.fibu-right\s*\{[\s\S]*grid-column:\s*5/);
  assert.equal(typeof manifest.layout.third_pane_justification, 'string');
  assert.ok(manifest.layout.third_pane_justification.length > 20);
});

test('primary and secondary accounting rows carry the full context trio', () => {
  assert.match(js, /data-context-record-id=/);
  assert.match(js, /data-context-record-type=/);
  assert.match(js, /data-context-label=/);
  assert.match(js, /data-travel-click-id=[^\n]+data-context-record-type="accounting_journal_entry"[^\n]+data-context-label=/);
  assert.match(js, /data-mileage-click-id=[^\n]+data-context-record-type="accounting_journal_entry"[^\n]+data-context-label=/);
});

test('v4: empty states render nothing and live data comes from api.query (App-API 4.0)', () => {
  // Readiness/Ladefunktion entfällt (PORTING.md R4/R11): keine Sync-API, kein Ladeblock, leere Listen bleiben leer.
  assert.doesNotMatch(js, /ctx\?\.sync|collectionReadiness|subscribeCollectionReadiness/);
  assert.doesNotMatch(js, /ctox-syncing|syncingData/);
  assert.match(js, /export async function mount\(root, api, \{ restored \} = \{\}\)/);
  for (const name of ['accounting_accounts', 'accounting_journal_entries', 'accounting_receipts', 'accounting_bank_statement_lines']) {
    assert.ok(js.includes(`'${name}'`), `live query wired for ${name}`);
  }
  assert.match(js, /disposers\.push\(api\.query\(name, \{\}\)\.subscribe\(scheduleFibuReload\)\)/);
  // Selection- and match-driven empties stay.
  assert.match(js, /Keine Belegevidenz für diese Banktransaktion verknüpft\./);
  assert.match(js, /Keine Hauptbuchungen für dieses Sachkonto vorhanden\./);
});

test('v4 accounting overlays stay inside the module and context actions clamp locally', () => {
  // v4: Shell-Vorfahr entfällt, Regel liegt unter dem App-Scope (scope-css.mjs).
  assert.match(css, /\[data-app="buchhaltung"\] \.fibu-module \{ position: relative;[\s\S]*overflow: hidden;/);
  assert.match(js, /\(state\.els\.root \|\| state\.root\)\?\.append\(menu\)/);
  assert.match(js, /const rootRect = state\.els\.root\?\.getBoundingClientRect/);
});

// FIX5: Die In-App-E2E-Suite (core/ui_e2e_tests.js, Ansicht „System & QS“) ist gelöscht, nicht stillgelegt. Sie schrieb
// Testdaten in die echten Bücher (Kontenrahmen neu, Hetzner-Beleg, festgeschriebene Buchung) und griff auf
// ctx.db/ctx.permissions zu, die es unter App-API 4.0 nicht gibt. Die Fachregeln prüft ../test.js gegen die Kernmodule.
test('v4: no in-app E2E suite, no QS view writing test bookings into the live ledger', async () => {
  const { access } = await import('node:fs/promises');
  await assert.rejects(access(new URL('../core/ui_e2e_tests.js', import.meta.url)), { code: 'ENOENT' });
  assert.doesNotMatch(js, /ui_e2e_tests|uiTestCases|uiTestResults|runAllUiTests|runSingleUiTest|buchhaltung\.uitest/);
  assert.doesNotMatch(html, /data-panel="tests"|value="tests"|run-all-ui-tests|e2e-live-log|data-ui-tests-list/);
  assert.doesNotMatch(css, /fibu-e2e|e2e-summary-bar|fibu-tests-|fibu-terminal/);
  const workbenchSelect = html.match(/<select[^>]*data-workbench-view[^>]*>([\s\S]*?)<\/select>/)?.[1] || '';
  assert.deepEqual([...workbenchSelect.matchAll(/<option value="([a-z]+)"/g)].map((m) => m[1]),
    ['skr', 'journal', 'receipts', 'banking', 'travel', 'mileage', 'reports', 'assets']);
});
