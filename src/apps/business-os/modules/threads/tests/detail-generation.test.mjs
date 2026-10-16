// Regel (vor dem Port: „ältere Detail-Antworten dürfen nicht gewinnen“): Nach einem Auswahlwechsel zeigt die
// Zeitleiste nur Daten des neuen Threads; Änderungen am vorher gewählten Thread erreichen sie nicht mehr, und die
// alten Detail-Abos sind abgemeldet. In App-API 4.0 gibt es keine Anfragen im Flug mehr – die Abos werden je Auswahl
// gewechselt (index.js syncDetailSubscription).
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/8265bd7 node --test src/apps/business-os/modules/threads/tests/detail-generation.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startHarness, mountMain } from './_v4-harness.mjs';

let h, page;
before(async () => { h = await startHarness(); page = h.page; await mountMain(page); await h.settle(); });
after(async () => { await h?.close(); });

const timeline = () => page.$eval('[data-thread-timeline]', (el) => el.innerText);

test('Auswahlwechsel: späte Änderung am alten Thread erreicht die neue Zeitleiste nicht', async () => {
  const { t1, t5 } = await page.evaluate(() => window.__fx.ids);
  assert.match(await timeline(), /CTOX Arbeit blockiert/);
  await page.click(`[data-thread-id="${t5}"]`);
  await h.settle();
  await page.evaluate((id) => window.__k.sim.push('user_thread_messages', { id: 'msg_late_old', thread_id: id, kind: 'note', body: 'Späte Nachricht des alten Threads', author_user_id: 'anna', created_at_ms: Date.now(), updated_at_ms: Date.now() }), t1);
  await h.settle();
  const text = await timeline();
  assert.doesNotMatch(text, /Späte Nachricht des alten Threads|CTOX Arbeit blockiert/);
  assert.match(text, /bitte das Angebot vor Freitag freigeben/);
  assert.equal(await page.evaluate(() => window.__c.subscribers('user_thread_messages')), 1, 'nur das Abo des neuen Threads');
  assert.equal(await page.evaluate(() => window.__c.subscribers('user_thread_links')), 1);
});

test('Gelöschter und neu angelegter Thread mit derselben ID zeigt nur frische Detaildaten', async () => {
  const { t5 } = await page.evaluate(() => window.__fx.ids);
  await page.evaluate((id) => window.__k.sim.push('user_thread_messages', { id: `msg_${id}_1`, _deleted: true }), t5);
  await h.settle();
  assert.doesNotMatch(await timeline(), /bitte das Angebot/);
  await page.evaluate((id) => window.__k.sim.push('user_thread_messages', { id: 'msg_fresh', thread_id: id, kind: 'note', body: 'Frische Nachricht', author_user_id: 'anna', created_at_ms: Date.now(), updated_at_ms: Date.now() }), t5);
  await h.settle();
  assert.match(await timeline(), /Frische Nachricht/);
  assert.deepEqual(page.__errors, []);
});
