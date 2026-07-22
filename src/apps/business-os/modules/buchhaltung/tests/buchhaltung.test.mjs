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
  assert.match(html, /data-pg-view="cards"/);
  assert.match(html, /data-pg-view="list"/);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-tray\b[^>]*hidden/);
  assert.match(html, /data-pg-reset/);
  assert.match(html, /class="ctox-pane-body ctox-well"/);
  assert.match(html, /class="ctox-pane-footer"/);
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
