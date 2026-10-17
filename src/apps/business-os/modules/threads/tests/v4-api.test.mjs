// threads gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel, Pin 67229d2) in echtem Chromium.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/threads/tests/v4-api.test.mjs
// Daten: tests/fixtures/ops-fixture.mjs (Ops-Ansicht mit Kennungs-Titeln, workjet#594). Screenshots → tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { APP, HERE, startHarness, mountMain, rowTexts } from './_v4-harness.mjs';

const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i;
let h, page;
before(async () => { h = await startHarness(); page = h.page; });
after(async () => { await h?.close(); });
const settle = () => h.settle();

test('mount: Live-Daten, gescoptes Root, Titel über api.shell, je Collection genau ein Abo', async () => {
  await mountMain(page);
  await settle();
  const rows = await rowTexts(page);
  assert.equal(rows.length, 4, 'Handeln: t1, t2, t4, t5 (t3 ist AI-Arbeit ohne Blockade)');
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate(() => window.__k.shell.window('threads').state.get().title), 'Threads');
  for (const c of ['user_threads', 'user_thread_states', 'ctox_task_approval_requests', 'user_notifications', 'business_users', 'business_apps', 'workjet_projects']) {
    assert.equal(await page.evaluate((c) => window.__c.subscribers(c), c), 1, `ein Abo auf ${c}`);
  }
  // Zähler sind exakt (lokale Abfrage vollständig): kein „≥“, keine Zustandszeile.
  const band = await page.$eval('.threads-view-switch', (el) => el.innerText.replace(/\s+/g, ' '));
  assert.match(band, /Handeln \(4\)/);
  assert.doesNotMatch(band, /≥/);
  assert.equal(await page.$$eval('[role="status"], .ctox-syncing', (n) => n.length), 0);
  // Reiter im Originalwortlaut; keine Fußzeile/Zählzeile (Owner-Regel), nur die Tab-Zähler.
  assert.match(band, /Wartet/);
  assert.equal(await page.$$eval('.threads-footer, [data-pg-footer], .ctox-pane-footer', (n) => n.length), 0);
  const left = await page.$eval('.threads-left', (el) => el.innerText);
  assert.doesNotMatch(left, /\d+ Threads? ·|Jetzt handeln| als /);
  assert.deepEqual(page.__errors, []);
});

test('linker Kopf: Filterknopf und Suche liegen ganz in der Spalte (nicht abgeschnitten)', async () => {
  const geo = await page.evaluate(() => {
    const pane = document.querySelector('.threads-left').getBoundingClientRect();
    const head = document.querySelector('.threads-left > .ctox-pane-header').getBoundingClientRect();
    const rect = (s) => document.querySelector(s).getBoundingClientRect();
    return { paneRight: pane.right, headRight: head.right, filterRight: rect('.threads-filter-toggle').right,
      toggleRight: rect('.threads-view-toggle').right, searchWidth: rect('.threads-left .ctox-pane-search').width };
  });
  assert.ok(geo.headRight <= geo.paneRight + 0.5, `Kopf ragt über die Spalte: ${JSON.stringify(geo)}`);
  assert.ok(geo.filterRight <= geo.paneRight - 4, `Filterknopf angeschnitten: ${JSON.stringify(geo)}`);
  assert.ok(geo.toggleRight < geo.filterRight && geo.searchWidth >= 100, JSON.stringify(geo));
});

test('workjet#594: Threads heißen nach Projekt und Aufgabe, Kennung nur in den Details', async () => {
  const rows = await rowTexts(page);
  const { t1, t2, t4, t5, C1 } = await page.evaluate(() => window.__fx.ids);
  const byId = Object.fromEntries(rows.map((r) => [r.id, r.text]));
  for (const row of rows) assert.doesNotMatch(row.text, UUID, `keine Kennung in der Zeile: ${row.text}`);
  assert.match(byId[t1], /^. ?CTOX Shell v4 · Login-Seite auf App-API 4\.0 umstellen und Abnahme fahren/);
  assert.match(byId[t2], /Workjet · Projektliste/);
  for (const row of rows) assert.doesNotMatch(row.text, /ctox\.workjet\./, `kein technischer Befehlstyp: ${row.text}`);
  assert.match(byId[t4], /Workjet Desktop · Supervisor/);
  assert.match(byId[t5], /Angebot Müller GmbH prüfen/);
  // Kopf des ausgewählten Threads (t1, höchste Aufmerksamkeit): Projekt als Kicker, Aufgabe als Titel, keine Kennung.
  const head = await page.evaluate(() => ({
    kicker: document.querySelector('[data-thread-source]').textContent,
    title: document.querySelector('[data-thread-title]').textContent,
    status: document.querySelector('[data-thread-status]').textContent,
  }));
  assert.equal(head.kicker, 'CTOX Shell v4');
  assert.equal(head.title, 'Login-Seite auf App-API 4.0 umstellen und Abnahme fahren');
  assert.doesNotMatch(JSON.stringify(head), UUID);
  assert.equal(head.status, 'Abstimmung: Blockiert · Fachvorgang: Fehlgeschlagen');
  // Kennung steht in den Details (rechte Spalte).
  const context = await page.$eval('[data-thread-context]', (el) => el.innerText);
  assert.match(context, /Kennung/);
  assert.ok(context.includes(t1));
  assert.ok(context.includes(C1), 'Record-Kennung in den Details');
});

test('workjet#594: Status ausgeschrieben, nie abgeschnitten (Zeile bricht um statt Ellipse)', async () => {
  for (const width of [1280, 420]) {
    await page.setViewportSize({ width, height: 860 });
    await page.$eval('#win', (el, w) => { el.style.width = `${w}px`; el.style.height = '860px'; }, width);
    await page.waitForTimeout(150);
    const cut = await page.$$eval('.threads-attention, .threads-meta-facts, [data-thread-status]', (nodes) => nodes
      .filter((n) => n.offsetParent !== null)
      .map((n) => ({ text: n.textContent, over: n.scrollWidth - n.clientWidth, ellipsis: getComputedStyle(n).textOverflow }))
      .filter((n) => n.over > 0 || n.ellipsis === 'ellipsis'));
    assert.deepEqual(cut, [], `abgeschnitten bei ${width}px`);
    const attention = await page.$$eval('.threads-attention', (nodes) => nodes.map((n) => n.textContent));
    assert.ok(attention.includes('Blockiert · Fehlgeschlagen'));
  }
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('Rundgang RUND2: Aufklapp-Köpfe rechts – Pfeil und Überschrift in einer Zeile', async () => {
  const toggle = '#win [data-toggle-actions]';
  const wasOpen = await page.$eval(toggle, (b) => b.getAttribute('aria-pressed') === 'true');
  if (!wasOpen) { await page.click(toggle); await settle(); }
  const rows = await page.$$eval('#win .threads-right .threads-panel > summary', (nodes) => nodes
    .filter((n) => n.offsetParent !== null)
    .map((n) => { const h3 = n.querySelector('h3'); const s = n.getBoundingClientRect(); const r = h3.getBoundingClientRect(); return { text: h3.textContent, oneLine: r.top - s.top <= 16 && s.height < 44, h: Math.round(s.height) }; }));
  assert.ok(rows.length > 0, 'Aufklapp-Köpfe sichtbar');
  assert.deepEqual(rows.filter((r) => !r.oneLine), []);
  if (!wasOpen) { await page.click(toggle); await settle(); }
});

test('Rundgang RUND2: langer Projekt-Kicker bleibt einzeilig, Aufgabentitel bleibt im Kopf (1280)', async () => {
  // Wie im Rundgang: Aktionsspalte offen, die Timeline-Spalte ist dann ~590 px breit.
  const toggle = '#win [data-toggle-actions]';
  const wasOpen = await page.$eval(toggle, (b) => b.getAttribute('aria-pressed') === 'true');
  if (!wasOpen) { await page.click(toggle); await settle(); }
  const r = await page.evaluate(() => {
    const head = document.querySelector('#win .threads-center > .ctox-pane-header');
    const k = head?.querySelector('[data-thread-source]');
    const t = head?.querySelector('[data-thread-title]');
    if (!k || !t) return null;
    const keep = k.textContent;
    k.textContent = 'ANGEBOT WARTUNGSVERTRAG HALLE 4 MIT SEHR LANGEM PROJEKTNAMEN';
    const kb = k.getBoundingClientRect(); const tb = t.getBoundingClientRect(); const hb = head.getBoundingClientRect();
    const out = { kickerH: Math.round(kb.height), titleInside: tb.top >= hb.top - 1 && tb.bottom <= hb.bottom + 1, ws: getComputedStyle(k).whiteSpace };
    k.textContent = keep;
    return out;
  });
  if (!wasOpen) { await page.click(toggle); await settle(); }
  assert.ok(r, 'Timeline-Kopf mit Kicker und Titel vorhanden');
  assert.ok(r.kickerH <= 22, `Kicker einzeilig (Höhe ${r.kickerH}) ${JSON.stringify(r)}`);
  assert.equal(r.titleInside, true);
  // Im Test-Harness verhindert schon die Kit-CSS (app.css/base.css) den Umbruch; der v4-Rahmen lädt sie nicht.
  // Deshalb muss die App-eigene CSS die Regel tragen.
  const own = await page.evaluate(() => [...document.styleSheets]
    .filter((sh) => /\/modules\/threads\/index\.css/.test(sh.href || ''))
    .flatMap((sh) => [...sh.cssRules])
    .some((rule) => /threads-center/.test(rule.selectorText || '') && /ctox-pane-kicker\s*$/.test(rule.selectorText || '') && rule.style.whiteSpace === 'nowrap'));
  assert.equal(own, true, 'index.css setzt white-space: nowrap am Timeline-Kicker');
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('user_threads', { id: 'thread_live', thread_id: 'thread_live', title: 'Rückruf Kunde Live',
    kind: 'note', status: 'open', participant_ids: ['michael'], assigned_user_id: 'michael', updated_at_ms: Date.now() }));
  await page.evaluate(() => window.__k.sim.push('user_thread_states', { id: 'state_live', thread_id: 'thread_live', user_id: 'michael',
    attention_score: 99, attention_reasons: ['Erwähnung'], updated_at_ms: Date.now() }));
  await settle();
  assert.ok((await rowTexts(page)).some((r) => /Rückruf Kunde Live/.test(r.text)));
  await page.evaluate(() => window.__k.sim.push('user_threads', { id: 'thread_live', _deleted: true }));
  await settle();
  assert.ok(!(await rowTexts(page)).some((r) => /Rückruf Kunde Live/.test(r.text)));
});

test('leere Collections rendern keinen Text und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const { mount } = await import('/modules/threads/index.js?second');
    const app = await mount(host, k2.appApi('threads', '0.2.4'), {});
    await new Promise((r) => setTimeout(r, 250));
    const result = {
      list: host.querySelector('[data-thread-list]').textContent.trim(),
      timeline: host.querySelector('[data-thread-timeline]').textContent.trim(),
      title: host.querySelector('[data-thread-title]').textContent.trim(),
      stateBoxes: host.querySelectorAll('.ctox-syncing, .ctox-empty, [role="status"]').length,
      zone: k2.statusZone.forApp('threads').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return result;
  });
  assert.deepEqual(empty, { list: '', timeline: '', title: '', stateBoxes: 0, zone: [] });
});

test('Befehl: Antwort senden → business_commands-Dokument mit record_id; failed → genau ein Zoneneintrag', async () => {
  const { t1 } = await page.evaluate(() => window.__fx.ids);
  await page.evaluate(() => window.__k.sim.failCommand('threads.message.create', 'Thread geändert'));
  await page.fill('[data-message-body]', 'Bitte Build-Lane neu starten @Anna');
  await page.click('[data-message-form] button[type="submit"]');
  assert.equal(await page.$eval('[data-message-body]', (el) => el.value), '', 'Eingabe sofort frei, kein Warten');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'threads.message.create');
  assert.equal(cmd.record_id, t1);
  assert.equal(cmd.payload.thread_id, t1);
  assert.deepEqual(cmd.payload.target_user_ids, ['anna']);
  assert.equal(cmd.payload.record_id, undefined, 'record_id nicht im payload (V4-HANDLERS U-3)');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'threads.command.threads.message.create');
  assert.match(zone[0].text, /Thread geändert/);
  // Nächster Befehl desselben Typs gelingt → Zoneneintrag geräumt.
  await page.fill('[data-message-body]', 'Zweiter Versuch');
  await page.click('[data-message-form] button[type="submit"]');
  await settle();
  assert.equal((await page.evaluate(() => window.__commands().at(-1))).status, 'done');
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
});

test('Dialog im Fenster: Snooze fragt über api.shell.dialog (Stub-Antwort = default 24 h), kein window.prompt', async () => {
  const { t1 } = await page.evaluate(() => window.__fx.ids);
  const before = await page.evaluate(() => window.__k.sim.dialogs.length);
  await page.click('.threads-center [data-thread-snooze]');
  await settle();
  assert.equal(await page.evaluate(() => window.__k.sim.dialogs.length), before + 1);
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'threads.thread.snooze');
  assert.equal(cmd.record_id, t1);
  assert.ok(cmd.payload.snoozed_until_ms > Date.now() + 23 * 3600 * 1000);
});

test('Freigabe ablehnen: Begründung über api.shell.dialog, Entscheidung ohne Nachlade-Schleife', async () => {
  const { t5 } = await page.evaluate(() => window.__fx.ids);
  await page.click(`[data-thread-id="${t5}"]`);
  await settle();
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, 'Preis fehlt'), { restored: { v: 1, selectedId: window.__fx.ids.t5 } });
  });
  await settle();
  await page.click('[data-reject-approval="approval_mueller"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'threads.ctox_approval.reject');
  assert.equal(cmd.record_id, 'approval_mueller');
  assert.equal(cmd.payload.decision_note, 'Preis fehlt');
  assert.equal(cmd.payload.approval_request_id, 'approval_mueller');
  // Die Entscheidung kommt vom Core als Live-Änderung; die Karte verliert ihre Entscheidungsknöpfe.
  await page.evaluate(() => window.__k.sim.push('ctox_task_approval_requests', { ...window.__fx.collections.ctox_task_approval_requests[0], status: 'rejected', updated_at_ms: Date.now() }));
  await settle();
  assert.equal(await page.$$eval('[data-reject-approval]', (n) => n.length), 0);
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('user_threads', { id: 'thread_put', thread_id: 'thread_put', title: 'Lieferung Charge 12 prüfen', kind: 'note',
      status: 'open', participant_ids: ['michael'], updated_at_ms: Date.now() });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await page.click('[data-pg-tray-toggle]');
  await page.selectOption('[data-pg-filter][data-pg-name="view"]', 'all');
  await settle();
  assert.ok((await rowTexts(page)).some((r) => /Lieferung Charge 12 prüfen/.test(r.text)));
  await page.click('[data-pg-reset]');
  await page.click('[data-pg-tray-toggle]');
  await settle();
});

test('snapshot/restored: offene Antwort, Notiz-Entwurf und Auswahl überleben den Remount', async () => {
  const { t5 } = await page.evaluate(() => window.__fx.ids);
  await page.click(`[data-thread-id="${t5}"]`);
  await settle();
  await page.fill('[data-message-body]', 'halb fertige Antwort an Anna');
  await page.evaluate(() => document.querySelector('[data-action="create-note"]').click());
  await page.fill('[data-note-body]', 'Notiz-Entwurf');
  await page.selectOption('[data-note-target]', 'anna');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, t5);
  assert.equal(snap.drafts.messageBody, 'halb fertige Antwort an Anna');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    message: document.querySelector('[data-message-body]').value,
    note: document.querySelector('[data-note-body]').value,
    target: document.querySelector('[data-note-target]').value,
    panelOpen: document.querySelector('[data-action-panel="note"]').open,
    selected: window.__app.snapshot().selectedId,
    title: document.querySelector('[data-thread-title]').textContent,
  }));
  assert.deepEqual(after, { message: 'halb fertige Antwort an Anna', note: 'Notiz-Entwurf', target: 'anna', panelOpen: true, selected: t5, title: 'Angebot Müller GmbH prüfen' });
  assert.equal(await page.evaluate(() => window.__c.subscribers('user_threads')), 1, 'alte Abos abgemeldet');
  assert.equal(await page.evaluate(() => window.__c.subscribers('user_thread_messages')), 1);
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub (nachher)', async () => {
  // Fixture-Stand der Vorher-Aufnahme wiederherstellen (Freigabe offen, vgl. Test „Freigabe ablehnen“).
  await page.evaluate(() => window.__k.sim.push('ctox_task_approval_requests', { ...window.__fx.collections.ctox_task_approval_requests[0], updated_at_ms: Date.now() }));
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-after-1280.png`) });
  // Details (rechte Spalte) mit der Kennung – einziger Ort, an dem sie steht (workjet#594).
  await page.click('[data-toggle-actions]');
  await page.waitForTimeout(150);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-after-1280-details.png`) });
  await page.click('[data-toggle-actions]');
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.evaluate(() => document.querySelector('[data-mobile-back]').click());
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-after-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 0, `kein horizontaler Überlauf bei 420 (${overflow})`);
  const { t1 } = await page.evaluate(() => window.__fx.ids);
  await page.click(`[data-thread-id="${t1}"]`);
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-after-420-detail.png`) });
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('user_threads', { id: 'thread_late', title: 'Late', status: 'open', participant_ids: [], updated_at_ms: Date.now() });
    await window.__k.sim.settle();
    const names = ['user_threads', 'user_thread_states', 'ctox_task_approval_requests', 'user_notifications', 'business_users',
      'business_apps', 'workjet_projects', 'user_thread_messages', 'user_thread_links', 'business_commands', 'ctox_queue_tasks'];
    return { subs: Object.fromEntries(names.map((c) => [c, window.__c.subscribers(c)])), children: document.getElementById('win').children.length };
  });
  assert.ok(Object.values(res.subs).every((n) => n === 0), JSON.stringify(res.subs));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
