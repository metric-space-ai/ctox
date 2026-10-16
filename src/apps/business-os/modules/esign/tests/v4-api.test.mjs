// esign gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/68b83a3), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/esign/tests/v4-api.test.mjs
// Playwright: aus PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
// Screenshots (1280 und 420) landen in tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const APP = 'esign';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const COLLECTIONS = ['signature_requests'];

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0} #win{position:relative;overflow:hidden;background:var(--bg)}</style>
</head><body><div id="win" style="width:1280px;height:760px"></div></body></html>`;

let server, browser, page, base;

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (Import-Map greift für ctox: nicht).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 760 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'E-Signatur' }]);
    await kernel.sim.push('signature_requests', [
      { id: 'sr-1', document_id: 'DOC-ALPHA', document_title: 'Vertrag Alpha', subject_kind: 'arbeitsvertrag', status: 'sent', signers: [{ id: 'cand-alpha', name: 'Alpha Person', state: 'pending' }, { id: 'hr-1', state: 'signed' }], created_at_ms: 1, updated_at_ms: 2 },
      { id: 'sr-2', document_id: 'DOC-BETA', document_title: 'Vertrag Beta', subject_kind: 'vermittlungsvertrag', status: 'completed', signers: [{ id: 'cand-beta', state: 'signed' }], created_at_ms: 1, updated_at_ms: 1 },
    ]);
    await kernel.sim.push('documents', [
      { id: 'doc-delta', title: 'Arbeitsvertrag Dora Delta', filename: 'arbeitsvertrag-delta.docx', updated_at_ms: 5 },
      { id: 'doc-eps', title: 'Rahmenvertrag Epsilon', updated_at_ms: 4 },
    ]);
    await kernel.sim.push('customer_contacts', [{ id: 'ct-hr', first_name: 'Hanna', last_name: 'Richter', email: 'hanna.richter@example.de', updated_at_ms: 1 }]);
    await kernel.sim.push('applications', [{ id: 'app-delta', candidate: { id: 'cand-delta', name: 'Dora Delta', email: 'dora@delta.example' }, status: 'new', updated_at_ms: 1 }]);
    kernel.sim.onCommand('ats.signature.request', (p) => ({ ok: true, request_id: 'sr-new', status: 'created', document_id: p.document_id }));
    kernel.sim.onCommand('ats.signature.sign', (p) => ({ ok: true, request_id: p.request_id, status: 'partially_signed' }));
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(200); };
const listText = () => page.$eval('[data-esign-list]', (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Vertrag Alpha/);
  assert.match(list, /Vertrag Beta/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate((APP) => window.__k.shell.window(APP).state.get().title, APP), 'E-Signatur');
  assert.equal(await page.evaluate(() => window.__c.subscribers('signature_requests')), 1);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('signature_requests', { id: 'sr-live', document_id: 'DOC-GAMMA-LIVE', document_title: 'Vertrag Gamma', status: 'created', signers: [], updated_at_ms: 3 }));
  await settle();
  assert.match(await listText(), /Vertrag Gamma/);
  await page.evaluate(() => window.__k.sim.push('signature_requests', { id: 'sr-live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Vertrag Gamma/);
});

test('leere Collections: kein Lade-/Leertext, keine Zone', async () => {
  const res = await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi(APP, '1'), {});
    await new Promise((r) => setTimeout(r, 150));
    const listText = host.querySelector('[data-esign-list]').textContent.trim();
    const stateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"]'));
    const zone = k2.statusZone.forApp(APP).get().filter((e) => e.source === 'app').map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { listText, stateBox, zone };
  }, APP);
  assert.equal(res.listText, '');
  assert.equal(res.stateBox, false);
  assert.deepEqual(res.zone, []);
});

test('Befehl: Signatur anfordern → business_commands-Dokument, Ergebnis im Prüfbereich; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => document.querySelector('[data-esign-root] [data-action="new"]').click());
  // FIX7: keine ID-Felder mehr – Dokument aus der Liste, Unterzeichner über die Personenauswahl.
  assert.equal(await page.$('[data-esign-form] input[type="text"][name="document_id"], [data-esign-form] [name="signers"]'), null);
  await page.fill('[data-esign-doc-search]', 'delta');
  assert.match(await page.$eval('[data-esign-doc-options]', (el) => el.textContent), /Arbeitsvertrag Dora Delta/);
  await page.click('[data-esign-doc-option="doc-delta"]');
  assert.match(await page.$eval('[data-esign-doc-picked]', (el) => el.textContent), /Arbeitsvertrag Dora Delta/);
  await page.fill('[data-esign-person-search]', 'Richter');
  await page.click('[data-esign-person-option="ct-hr"]');
  await page.fill('[data-esign-person-search]', 'dora');
  await page.keyboard.press('Enter');
  const rows = await page.$$eval('[data-esign-recipient]', (els) => els.map((el) => el.querySelector('[data-esign-r-field="name"]').value));
  assert.deepEqual(rows, ['Hanna Richter', 'Dora Delta']);
  // Reihenfolge ändern: Dora nach oben, Rolle von Hanna auf „Erhält Kopie“.
  await page.click('[data-esign-recipient="1"] [data-esign-move="up"]');
  await page.selectOption('[data-esign-recipient="1"] [data-esign-r-field="role"]', 'cc');
  const formText = await page.$eval('[data-esign-form]', (el) => el.innerText);
  assert.doesNotMatch(formText, /doc-delta|cand-delta|ct-hr|Dokument-ID|IDS/);
  await page.evaluate(() => document.querySelector('[data-esign-form]').requestSubmit());
  await settle();
  let cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ats.signature.request');
  assert.equal(cmd.payload.document_id, 'doc-delta');
  assert.deepEqual(cmd.payload.signers, [
    { id: 'cand-delta', state: 'pending', order: 1, role: 'signer', name: 'Dora Delta', email: 'dora@delta.example' },
    { id: 'ct-hr', state: 'pending', order: 2, role: 'cc', name: 'Hanna Richter', email: 'hanna.richter@example.de' },
  ]);
  assert.equal(await page.$$eval('[data-esign-recipient]', (els) => els.length), 0, 'Formular nach dem Anlegen leer');
  assert.equal(cmd.status, 'done');
  // Ergebnis als Meldung in der Zone statt Kasten im Inhalt.
  assert.equal(await page.$('[data-esign-gate], .ctox-callout'), null);
  assert.match((await page.evaluate(() => window.__zone())).find((e) => e.code === 'esign.result')?.text || '', /sr-new/);
  await page.evaluate(() => window.__k.sim.failCommand('ats.signature.request', 'kaputt'));
  await page.fill('[data-esign-doc-search]', 'Epsilon');
  await page.click('[data-esign-doc-option="doc-eps"]');
  await page.evaluate(() => document.querySelector('[data-esign-form]').requestSubmit());
  await settle();
  cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'esign.command.ats.signature.request');
  assert.match(zone[0].text, /kaputt/);
  await page.evaluate(() => window.__api.status.clear('esign.command.ats.signature.request'));
});

test('Schreiben: Import über api.put → angenommen, live sichtbar, Meldung in der Zone', async () => {
  const [chooser] = await Promise.all([
    page.waitForEvent('filechooser'),
    page.evaluate(() => document.querySelector('[data-esign-root] [data-action="import"]').click()),
  ]);
  await chooser.setFiles({ name: 'sr.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify([{ document_id: 'DOC-IMPORT-ZETA', subject_kind: 'arbeitsvertrag', signers: [] }])) });
  await settle();
  // Import ohne Dokumenttitel: Zeile mit dem Vertragstyp, nie die Dokument-ID.
  assert.doesNotMatch(await listText(), /DOC-IMPORT-ZETA/);
  assert.ok(await page.evaluate(() => window.__api.query('signature_requests', { selector: { document_id: 'DOC-IMPORT-ZETA' } }).get().length) === 1);
  const res = await page.evaluate(async () => ({ outbox: (await window.__k.sim.outbox()).length, zone: window.__zone().map((e) => e.code) }));
  assert.equal(res.outbox, 0);
  assert.ok(res.zone.includes('esign.import.done'));
});

test('snapshot/restored: Auswahl und offene Eingabe überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-esign-row="sr-1"]').click());
  await page.fill('[data-esign-recipient="0"] [data-esign-r-field="email"]', 'halb@fertig.example');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'sr-1');
  assert.equal(snap.form.signers[0].email, 'halb@fertig.example');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    signers: document.querySelector('[data-esign-recipient="0"] [data-esign-r-field="email"]').value,
    selected: window.__app.snapshot().selectedId,
    detail: document.querySelector('[data-esign-detail]').textContent,
  }));
  assert.equal(after.signers, 'halb@fertig.example');
  assert.equal(after.selected, 'sr-1');
  assert.match(after.detail, /Alpha Person/);
  assert.doesNotMatch(after.detail, /cand-alpha|DOC-ALPHA|sr-1/, 'keine technischen IDs im Detail');
  assert.equal(await page.evaluate(() => window.__c.subscribers('signature_requests')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Signieren: ats.signature.sign aus dem Detail → done-Ergebnis im Prüfbereich', async () => {
  await page.evaluate(() => document.querySelector('[data-esign-detail] [data-sign]')?.click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ats.signature.sign');
  assert.equal(cmd.payload.request_id, 'sr-1');
  assert.equal(cmd.status, 'done');
  // Status deutsch statt Rohwert, als Meldung in der Zone.
  const result = (await page.evaluate(() => window.__zone())).find((e) => e.code === 'esign.result')?.text || '';
  assert.match(result, /Teilweise signiert/);
  assert.doesNotMatch(result, /partially_signed/);
});

// FIX8 Mangel 2: Status `done`/`failed` erschienen roh; der Detailbereich war im dunklen Thema dunkel auf dunkel.
test('FIX8: done/failed deutsch wie DocuSign; Kopf- und Detailbereich im Shell-v2-Fenster hell und dunkel lesbar (Kontrast ≥ 3)', async () => {
  await page.evaluate(() => window.__k.sim.push('signature_requests', [
    { id: 'sr-done', document_id: 'DOC-DONE', document_title: 'Vertrag Fertig', subject_kind: 'arbeitsvertrag', status: 'done', signers: [{ id: 's-d', name: 'Dana Done', state: 'signed' }], updated_at_ms: 20 },
    { id: 'sr-fail', document_id: 'DOC-FAIL', document_title: 'Vertrag Fehler', subject_kind: 'arbeitsvertrag', status: 'failed', signers: [{ id: 's-f', name: 'Fritz Fail', state: 'pending' }], updated_at_ms: 19 },
  ]));
  await settle();
  const list = await listText();
  assert.match(list, /Abgeschlossen/);
  assert.match(list, /Fehlgeschlagen/);
  assert.doesNotMatch(list, /\bdone\b|\bfailed\b/);
  const badge = await page.$eval('[data-esign-row="sr-fail"] .ctox-badge', (el) => el.className);
  assert.match(badge, /is-danger/);
  // Im echten Rahmen liegt die App in .shell-window[data-shell-contract="v2"] (Shell-v2-Flächentokens aus app.css).
  await page.evaluate(() => { const w = document.getElementById('win'); w.classList.add('shell-window', 'is-focused'); w.dataset.shellContract = 'v2'; document.querySelector('[data-esign-row="sr-done"]').click(); });
  await settle();
  const detail = await page.$eval('[data-esign-detail]', (el) => el.textContent);
  assert.match(detail, /Abgeschlossen/);
  assert.doesNotMatch(detail, /\bdone\b/);
  await mkdir(join(HERE, 'shots'), { recursive: true });
  const lowContrast = () => page.evaluate(() => {
    // Farben über ein Canvas normalisieren (color-mix/color(srgb …) inklusive), Hintergründe von oben nach unten mischen.
    const cv = document.createElement('canvas'); cv.width = cv.height = 1; const cx = cv.getContext('2d', { willReadFrequently: true });
    const rgb = (c) => { cx.clearRect(0, 0, 1, 1); cx.fillStyle = '#000'; cx.fillStyle = c; cx.fillRect(0, 0, 1, 1); const [r, g, b, a] = cx.getImageData(0, 0, 1, 1).data; return { r, g, b, a: a / 255 }; };
    const over = (top, under) => ({ r: top.r * top.a + under.r * (1 - top.a), g: top.g * top.a + under.g * (1 - top.a), b: top.b * top.a + under.b * (1 - top.a), a: 1 });
    const lum = ({ r, g, b }) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
    const bgOf = (el) => {
      const layers = [];
      for (let n = el; n; n = n.parentElement) { const c = rgb(getComputedStyle(n).backgroundColor); if (c.a > 0) layers.push(c); if (c.a >= 0.99) break; }
      return layers.reverse().reduce((acc, c) => over(c, acc), { r: 255, g: 255, b: 255, a: 1 });
    };
    const out = [];
    for (const el of document.querySelectorAll('#win *')) {
      const own = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
      // Primärknöpfe tragen die Akzentfarbe des Rahmens (Fensterpalette), nicht der App.
      if (!own || !el.getClientRects().length || getComputedStyle(el).visibility === 'hidden' || el.closest('.is-primary')) continue;
      const bg = bgOf(el);
      const [a, b] = [lum(over(rgb(getComputedStyle(el).color), bg)), lum(bg)].sort((x, y) => y - x);
      const ratio = (a + 0.05) / (b + 0.05);
      if (ratio < 3) out.push(`${el.tagName}.${el.className} "${el.textContent.trim().slice(0, 30)}" ${ratio.toFixed(2)}`);
    }
    return out;
  });
  const low = {};
  for (const theme of ['light', 'dark']) {
    await page.evaluate((t) => { document.documentElement.dataset.theme = t; }, theme);
    await page.waitForTimeout(100);
    await page.screenshot({ path: join(HERE, 'shots', `${APP}-v2-${theme}-1280.png`) });
    low[theme] = await lowContrast();
  }
  await page.evaluate(() => { delete document.documentElement.dataset.theme; const w = document.getElementById('win'); w.classList.remove('shell-window', 'is-focused'); delete w.dataset.shellContract; document.querySelector('[data-esign-row="sr-1"]').click(); });
  await page.evaluate(() => window.__k.sim.push('signature_requests', [{ id: 'sr-done', _deleted: true }, { id: 'sr-fail', _deleted: true }]));
  await settle();
  assert.deepEqual(low, { light: [], dark: [] }, 'Text mit zu wenig Kontrast im Shell-v2-Fenster');
});

test('Screenshots 1280/420', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 1, `kein horizontaler Überlauf (${overflow}px)`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (COLLECTIONS) => {
    await window.__app.unmount();
    await window.__k.sim.push('signature_requests', { id: 'sr-late', document_id: 'LATE', updated_at_ms: 10 });
    await window.__k.sim.settle();
    return { subs: COLLECTIONS.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, COLLECTIONS);
  assert.deepEqual(res.subs, COLLECTIONS.map(() => 0));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
