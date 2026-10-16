// Regel (vor dem Port: „authoritative rescan/deletion/cancellation“): Suche deckt alle zugänglichen Threads ab,
// eine Löschung (Tombstone/_deleted oder is_deleted) entfernt den Thread sofort – auch den ausgewählten –, und keine
// spätere Emission bringt ihn zurück. In App-API 4.0 ist die lokale Abfrage vollständig und live; die frühere
// Scan-/Abbruch-Maschinerie (Seiten, Generationen, Sichtbarkeitsprüfung) entfällt, die Regel bleibt.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/threads/tests/search-rescan.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startHarness, mountMain, rowTexts } from './_v4-harness.mjs';

let h, page;
before(async () => { h = await startHarness(); page = h.page; await mountMain(page); await h.settle(); });
after(async () => { await h?.close(); });

const search = async (text) => {
  await page.fill('[data-pg-search]', text);
  await page.dispatchEvent('[data-pg-search]', 'input');
  await h.settle();
};

test('Suche über den angezeigten Namen und die Quelle, über alle zugänglichen Threads', async () => {
  // „Alle“ statt Handeln, damit auch t3 (AI-Arbeit) zur Menge gehört.
  await page.click('[data-pg-tray-toggle]');
  await page.selectOption('[data-pg-filter][data-pg-name="view"]', 'all');
  await search('nachtlauf');
  const rows = await rowTexts(page);
  assert.equal(rows.length, 1);
  assert.match(rows[0].text, /CTOX Shell v4 · Nachtlauf/);
  await search('müller');
  assert.deepEqual((await rowTexts(page)).map((r) => r.id), [await page.evaluate(() => window.__fx.ids.t5)]);
});

test('Tombstone entfernt den ausgewählten Treffer sofort; spätere Emissionen bringen ihn nicht zurück', async () => {
  const { t5 } = await page.evaluate(() => window.__fx.ids);
  await page.click(`[data-thread-id="${t5}"]`);
  await h.settle();
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedId), t5);
  await page.evaluate((id) => window.__k.sim.push('user_threads', { id, _deleted: true }), t5);
  await h.settle();
  assert.deepEqual(await rowTexts(page), []);
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedId), '');
  assert.equal(await page.$eval('[data-thread-title]', (el) => el.textContent), '');
  // Fremde Änderungen (andere Threads, Zustände) lösen neue Emissionen aus – der gelöschte bleibt weg.
  await page.evaluate(() => window.__k.sim.push('user_thread_states', { id: 'state_noise', thread_id: 'x', user_id: 'michael', attention_score: 5, updated_at_ms: Date.now() }));
  await h.settle();
  assert.deepEqual(await rowTexts(page), []);
});

test('is_deleted:true gilt wie gelöscht; Suche zurücksetzen zeigt den Rest', async () => {
  const { t1 } = await page.evaluate(() => window.__fx.ids);
  await page.evaluate((id) => window.__k.sim.push('user_threads', { ...window.__fx.collections.user_threads.find((t) => t.id === id), is_deleted: true, updated_at_ms: Date.now() }), t1);
  await search('');
  const ids = (await rowTexts(page)).map((r) => r.id);
  assert.ok(!ids.includes(t1));
  assert.ok(ids.length >= 3);
  assert.deepEqual(page.__errors, []);
});
