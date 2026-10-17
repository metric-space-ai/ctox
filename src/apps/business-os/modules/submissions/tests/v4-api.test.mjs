// submissions gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/64acc2c), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/submissions/tests/v4-api.test.mjs
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
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const APP = 'submissions';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
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
  // FIXT-R: echte Routen/Rechte/Collections aus v4/manifests/submissions.manifest.json (strikter Kern seit E26/E48).
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Vorstellungen' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version; nach der business_apps-Zeile.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Vorstellungen', manifestExtra: window.__routes });
    await kernel.sim.push('submissions', [
      { id: 'sub_1', candidate_id: 'cand-alice', candidate_name: 'Alice Adler', client_account_id: 'acme', vacancy_id: 'VAC-7', client_contact_id: 'ct-1', status: 'sent', consent: { status: 'granted' }, sent_at_ms: Date.UTC(2026, 9, 10), created_at_ms: 1, updated_at_ms: 3 },
      { id: 'sub_2', candidate_id: 'cand-bob', candidate_name: 'Bob Berger', client_account_id: 'globex', status: 'hired', feedback: { outcome: 'accepted' }, created_at_ms: 2, updated_at_ms: 2 },
      { id: 'sub_3', candidate_id: 'cand-carla-diaz-montenegro-von-hohenberg', candidate_name: 'Carla Diaz-diaz-montenegro-von-hohenberg', client_account_id: 'initech-international-holding', status: 'sent', created_at_ms: 3, updated_at_ms: 1 },
    ]);
    await kernel.sim.settle();
    kernel.sim.onCommand('ats.submission.present', () => ({ ok: true, submission_id: 'sub_new' }));
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const listText = () => page.$eval('[data-subs-list]', (el) => el.textContent);

test('mount: zeigt Live-Daten, nur api.*, Root gescoped, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Alice Adler/);
  assert.match(list, /Bob Berger/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'submissions');
  assert.equal(await page.evaluate(() => window.__k.shell.window('submissions').state.get().title), 'Vorstellungen');
  assert.equal(await page.evaluate(() => window.__c.subscribers('submissions')), 1);
  assert.equal(await page.$eval('[data-pg-count="all"]', (el) => el.textContent), ' (3)');
  assert.equal(await page.$eval('[data-pg-count="done"]', (el) => el.textContent), ' (1)');
  assert.equal(await page.$eval('[data-subs-list-pane]', (el) => el.dataset.pgWired), 'true', 'App verdrahtet die Spalten-Grammatik selbst');
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('submissions', { id: 'sub_live', candidate_id: 'cand-dora-live', candidate_name: 'Dora Live', client_account_id: 'x', status: 'sent', created_at_ms: 9, updated_at_ms: 9 }));
  await settle();
  assert.match(await listText(), /Dora Live/);
  await page.evaluate(() => window.__k.sim.push('submissions', { id: 'sub_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Dora Live/);
});

test('Filter: Band „Erledigt“ und Suche wirken über die selbst verdrahtete Grammatik', async () => {
  await page.click('[data-pg-band="done"]');
  await settle();
  assert.match(await listText(), /Bob Berger/);
  assert.doesNotMatch(await listText(), /Alice Adler/);
  await page.click('[data-pg-band="all"]');
  await page.fill('[data-pg-search]', 'carla');
  await settle();
  assert.match(await listText(), /Carla Diaz/);
  assert.doesNotMatch(await listText(), /Bob Berger/);
  await page.fill('[data-pg-search]', '');
  await settle();
});

test('leere Collection rendert keinen Text und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('submissions', '1'), {});
    await new Promise((r) => setTimeout(r, 250));
    const out = {
      listText: host.querySelector('[data-subs-list]').textContent.trim(),
      hasStateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], .ctox-callout')),
      zone: k2.statusZone.forApp('submissions').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.listText, '');
  assert.equal(empty.hasStateBox, false);
  assert.deepEqual(empty.zone, []);
});

test('Schreiben: api.put erscheint live und wird angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('submissions', { id: 'sub_put', candidate_id: 'cand-emil-put', candidate_name: 'Emil Put', client_account_id: 'y', status: 'sent', created_at_ms: 10, updated_at_ms: 10 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await listText(), /Emil Put/);
});

test('Befehl Vorstellen: Dokument sofort mit unveränderter Nutzlast, Erfolg still, blockiert → Zone', async () => {
  await page.click('[data-action="new"]');
  await page.fill('[data-subs-form] [name="candidate_id"]', 'cand-frieda');
  await page.fill('[data-subs-form] [name="client_account_id"]', 'acme');
  await page.click('[data-subs-form] button[type="submit"]');
  const immediate = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(immediate.type, 'ats.submission.present');
  assert.deepEqual(immediate.payload, { candidate_id: 'cand-frieda', client_account_id: 'acme', vacancy_id: null, client_contact_id: null });
  await settle();
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.equal(await page.inputValue('[data-subs-form] [name="candidate_id"]'), '', 'Formular nach done geleert');
  await page.evaluate(() => window.__k.sim.onCommand('ats.submission.present', () => ({ ok: false, blockers: [{ reason: 'Doppel-Vorstellung', conflicting_submission_id: 'sub_1' }] })));
  await page.fill('[data-subs-form] [name="candidate_id"]', 'cand-alice');
  await page.fill('[data-subs-form] [name="client_account_id"]', 'acme');
  await page.click('[data-subs-form] button[type="submit"]');
  await settle();
  const zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => [e.code, e.level]), [['submissions.present.blocked', 'warn']]);
  assert.match(zone[0].text, /Doppel-Vorstellung.*sub_1/);
  assert.equal(await page.inputValue('[data-subs-form] [name="candidate_id"]'), 'cand-alice', 'Eingabe bleibt bei Blockade');
  await page.evaluate(() => window.__api.status.clear('submissions.present.blocked'));
});

test('Befehl failed → genau ein Zoneneintrag; Pflichtfelder → Zone, kein Befehl', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ats.submission.present', 'kaputt'));
  await page.click('[data-subs-form] button[type="submit"]');
  await settle();
  let zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => e.code), ['submissions.command.ats.submission.present']);
  assert.match(zone[0].text, /kaputt/);
  await page.evaluate(() => window.__api.status.clear('submissions.command.ats.submission.present'));
  const before = await page.evaluate(() => window.__commands().length);
  await page.$eval('[data-subs-form] [name="client_account_id"]', (el) => { el.removeAttribute('required'); el.value = ''; });
  await page.click('[data-subs-form] button[type="submit"]');
  await settle();
  zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => e.code), ['submissions.form.ids_required']);
  assert.equal(await page.evaluate(() => window.__commands().length), before);
  await page.evaluate(() => window.__api.status.clear('submissions.form.ids_required'));
});

test('snapshot/restored: offene Eingabe, Auswahl und Filter überleben den Remount', async () => {
  // Offene Eingabe gibt es nur im Anlegemodus „+ Neu“ (FIX9a): erst Formular, dann Auswahl je eigener Remount.
  await page.click('[data-action="new"]');
  await page.click('details.subs-more > summary');
  await page.fill('[data-subs-form] [name="client_contact_id"]', 'ct-halb');
  const composeSnap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(composeSnap.composing, true);
  assert.equal(composeSnap.form.client_contact_id, 'ct-halb');
  await settle();
  assert.deepEqual(await page.evaluate(() => ({
    req: document.querySelector('[data-subs-form] [name="client_contact_id"]').value,
    more: document.querySelector('details.subs-more').open,
    shown: !document.querySelector('[data-subs-form]').hidden,
  })), { req: 'ct-halb', more: true, shown: true });
  await page.click('[data-subs-form] [data-action="cancel-new"]');
  await page.click('[data-subs-row="sub_3"]');
  await page.click('[data-pg-tray-toggle]');
  await page.selectOption('[data-pg-filter][data-pg-name="status"]', 'sent');
  await page.click('[data-subs-view-toggle]');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'sub_3');
  assert.equal(snap.composing, false);
  assert.equal(snap.view, 'list');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    formHidden: document.querySelector('[data-subs-form]').hidden,
    type: document.querySelector('[data-pg-filter][data-pg-name="status"]').value,
    trayHidden: document.querySelector('[data-pg-tray]').hidden,
    compact: document.querySelectorAll('[data-subs-list] .is-compact').length,
    detail: document.querySelector('[data-subs-detail]').textContent,
    list: document.querySelector('[data-subs-list]').textContent,
    selected: window.__app.snapshot().selectedId,
  }));
  assert.equal(after.formHidden, true, 'kein Anlegeformular über dem Datensatz');
  assert.equal(after.type, 'sent');
  assert.equal(after.trayHidden, false);
  assert.ok(after.compact > 0, 'Listenansicht wiederhergestellt');
  assert.match(after.detail, /Kunde\s*Unbekannt/);
  assert.doesNotMatch(after.detail, /initech/);
  assert.doesNotMatch(after.list, /Bob Berger/, 'Statusfilter wiederhergestellt');
  assert.equal(after.selected, 'sub_3');
  assert.equal(await page.evaluate(() => window.__c.subscribers('submissions')), 1, 'alte Abos abgemeldet');
  await page.click('[data-pg-reset]');
  await page.click('[data-pg-tray-toggle]');
  await page.click('[data-subs-view-toggle]');
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('FIX9a: Detail zeigt den Datensatz ohne vorbefülltes „Vorstellen“-Formular; „+ Neu“ öffnet ein leeres Formular, Abbrechen schließt es', async () => {
  await page.click('[data-subs-row="sub_1"]');
  await settle();
  const sel = await page.evaluate(() => {
    const form = document.querySelector('[data-subs-form]');
    return { hidden: form.hidden || getComputedStyle(form).display === 'none', candidate: form.querySelector('[name="candidate_id"]').value,
      submitVisible: Boolean(form.querySelector('button[type="submit"]')?.offsetParent), detail: document.querySelector('[data-subs-detail]').textContent.length };
  });
  assert.equal(sel.hidden, true, 'kein Anlegeformular über dem gewählten Datensatz');
  assert.equal(sel.submitVisible, false, 'kein „Vorstellen“ neben dem Datensatz');
  assert.equal(sel.candidate, '', 'nichts vorbefüllt');
  assert.ok(sel.detail > 0, 'Detail zeigt den Datensatz');
  const before = await page.evaluate(() => window.__commands().length);
  await page.click('[data-action="new"]');
  const fresh = await page.evaluate(() => {
    const form = document.querySelector('[data-subs-form]');
    return { shown: !form.hidden && getComputedStyle(form).display !== 'none', values: [...form.querySelectorAll('input')].map((i) => i.value),
      detail: document.querySelector('[data-subs-detail]').textContent.trim(), selected: window.__app.snapshot().selectedId };
  });
  assert.equal(fresh.shown, true);
  assert.deepEqual([...new Set(fresh.values)], [''], 'leeres Formular');
  assert.equal(fresh.detail, '', 'kein Datensatz daneben');
  assert.equal(fresh.selected, '');
  await page.click('[data-subs-form] [data-action="cancel-new"]');
  assert.equal(await page.$eval('[data-subs-form]', (el) => el.hidden), true, 'Abbrechen schließt');
  assert.equal(await page.evaluate(() => window.__commands().length), before, 'kein Befehl');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.click('[data-subs-row="sub_1"]');
  await settle();
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(200);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 0, `kein horizontaler Überlauf bei 420 (scrollWidth-clientWidth=${overflow})`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('submissions', { id: 'sub_late', candidate_id: 'late', updated_at_ms: 11 });
    await window.__k.sim.settle();
    return { subs: window.__c.subscribers('submissions'), children: document.getElementById('win').children.length };
  });
  assert.equal(res.subs, 0);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
