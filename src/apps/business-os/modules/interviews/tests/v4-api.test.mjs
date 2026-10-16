// interviews gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/68b83a3), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/interviews/tests/v4-api.test.mjs
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

const APP = 'interviews';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
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
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Interviews' }]);
    const day = 86_400_000;
    const future = Date.now() + 3 * day;
    await kernel.sim.push('interview_meetings', [
      { id: 'imeet_1', candidate_id: 'cand-alice', candidate_name: 'Alice Adler', vacancy_id: 'VAC-7', parties: [{ name: 'HR' }, { name: 'Fachbereich' }], start: future, end: future + 45 * 60_000, location_mode: 'video', video_link: 'https://meet.example.org/x', state: 'proposed', attended: false, created_at_ms: 1, updated_at_ms: 1 },
      { id: 'imeet_2', candidate_id: 'cand-bob', candidate_name: 'Bob Berger', state: 'completed', attended: true, start: Date.now() - 5 * day, end: Date.now() - 5 * day + 30 * 60_000, location_mode: 'onsite', parties: [{ name: 'CEO' }], created_at_ms: 2, updated_at_ms: 2 },
      { id: 'imeet_3', candidate_id: 'cand-carla-diaz-montenegro-von-hohenberg', candidate_name: 'Carla Diaz-diaz-montenegro-von-hohenberg', vacancy_id: 'VAC-12', state: 'confirmed', start: future + day, location_mode: 'phone', parties: [], created_at_ms: 3, updated_at_ms: 3 },
    ]);
    await kernel.sim.push('interview_scorecards', [
      { id: 'isc_1', candidate_id: 'cand-alice', candidate_name: 'Alice Adler', vacancy_id: 'VAC-7', role_template: 'engineer', criteria: [{ id: 'c1', label: 'Technik', weight: 2 }, { id: 'c2', label: 'Team', weight: 1 }], ratings: { c1: 4, c2: 5 }, interviewer: 'Ines', updated_at_ms: 1 },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const listText = () => page.$eval('[data-ats-list]', (el) => el.textContent);

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
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'interviews');
  assert.equal(await page.evaluate(() => window.__k.shell.window('interviews').state.get().title), 'Interviews');
  assert.equal(await page.evaluate(() => window.__c.subscribers('interview_meetings')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('interview_scorecards')), 1);
  assert.equal(await page.$eval('[data-pg-count="all"]', (el) => el.textContent), ' (3)');
  assert.equal(await page.$eval('.ats-list-pane', (el) => el.dataset.pgWired), 'true', 'App verdrahtet die Spalten-Grammatik selbst');
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('interview_meetings', { id: 'imeet_live', candidate_id: 'cand-dora-live', candidate_name: 'Dora Live', state: 'proposed', created_at_ms: 9, updated_at_ms: 9 }));
  await settle();
  assert.match(await listText(), /Dora Live/);
  await page.evaluate(() => window.__k.sim.push('interview_meetings', { id: 'imeet_live', _deleted: true }));
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

test('leere Collections rendern keinen Text und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('interviews', '1'), {});
    await new Promise((r) => setTimeout(r, 250));
    const out = {
      listText: host.querySelector('[data-ats-list]').textContent.trim(),
      hasStateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], .ctox-callout')),
      zone: k2.statusZone.forApp('interviews').get().map((e) => e.code),
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

test('Schreiben: Termin anlegen → api.put lokal sofort, angenommen, Auswahl gebunden', async () => {
  await page.click('[data-action="new"]');
  await page.fill('[name="candidate_id"]', 'cand-emil-neu');
  await page.fill('[name="parties"]', 'HR, Team');
  await page.click('[data-ats-submit]');
  // kein Warten: das Dokument steht sofort lokal in der Liste
  // Über das Formular angelegt (nur Kandidaten-ID, kein Name): „Unbekannt“, nie die ID.
  assert.match(await listText(), /Unbekannt/);
  assert.doesNotMatch(await listText(), /cand-emil-neu/);
  await settle();
  const doc = await page.evaluate(() => window.__api.query('interview_meetings', { selector: { candidate_id: 'cand-emil-neu' } }).get()[0]);
  assert.equal(doc.state, 'proposed');
  assert.deepEqual(doc.parties, [{ name: 'HR' }, { name: 'Team' }]);
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedId), doc.id);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
});

test('Zustandsübergang: Bestätigen patcht den Termin; Ablehnung des Core → Zustandszone (Kernel)', async () => {
  await page.click('[data-ats-select="imeet_1"]');
  await page.click('[data-meeting-action="confirmed"][data-meeting-id="imeet_1"]');
  await settle();
  assert.equal(await page.evaluate(() => window.__api.get('interview_meetings', 'imeet_1').get().state), 'confirmed');
  await page.evaluate(() => window.__k.sim.rejectNext('conflict'));
  await page.click('[data-meeting-action="no_show"][data-meeting-id="imeet_1"]');
  await settle();
  const zone = await page.evaluate(() => window.__k.statusZone.forApp('interviews').get());
  assert.equal(zone.filter((e) => e.code.startsWith('write.rejected:')).length, 1, 'genau ein Kernel-Eintrag');
  assert.equal(await page.evaluate(() => window.__api.get('interview_meetings', 'imeet_1').get().state), 'confirmed', 'Serverstand zurück');
  assert.deepEqual(await page.evaluate(() => window.__zone()), [], 'App selbst meldet nichts doppelt');
  await page.evaluate(() => { for (const e of window.__k.statusZone.forApp('interviews').get()) window.__k.statusZone.clear('interviews', e.code); });
});

test('Pflichtfeld: fehlende Kandidat-ID → genau ein Zoneneintrag, kein Write', async () => {
  const before = await page.evaluate(() => window.__api.query('interview_meetings', {}).get().length);
  await page.click('[data-action="new"]');
  await page.$eval('[name="candidate_id"]', (el) => { el.removeAttribute('required'); el.value = ''; });
  await page.click('[data-ats-submit]');
  await settle();
  const zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => e.code), ['interviews.form.candidate_required']);
  assert.equal(await page.evaluate(() => window.__api.query('interview_meetings', {}).get().length), before);
  await page.evaluate(() => window.__api.status.clear('interviews.form.candidate_required'));
});

test('snapshot/restored: offene Eingabe, Auswahl und Filter überleben den Remount', async () => {
  await page.click('[data-ats-select="imeet_3"]');
  await page.fill('[name="video_link"]', 'https://halb.fertig/link');
  await page.click('[data-pg-tray-toggle]');
  await page.selectOption('[data-pg-filter][data-pg-name="status"]', 'confirmed');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'imeet_3');
  assert.equal(snap.form.video_link, 'https://halb.fertig/link');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    link: document.querySelector('[name="video_link"]').value,
    cand: document.querySelector('[name="candidate_id"]').value,
    status: document.querySelector('[data-pg-filter][data-pg-name="status"]').value,
    trayHidden: document.querySelector('[data-pg-tray]').hidden,
    wbHidden: document.querySelector('[data-ats-root]').classList.contains('is-workbench-hidden'),
    list: document.querySelector('[data-ats-list]').textContent,
    selected: window.__app.snapshot().selectedId,
  }));
  assert.equal(after.link, 'https://halb.fertig/link');
  assert.equal(after.cand, 'cand-carla-diaz-montenegro-von-hohenberg');
  assert.equal(after.status, 'confirmed');
  assert.equal(after.trayHidden, false);
  assert.equal(after.wbHidden, false);
  assert.doesNotMatch(after.list, /Bob Berger/, 'Statusfilter wiederhergestellt');
  assert.equal(after.selected, 'imeet_3');
  assert.equal(await page.evaluate(() => window.__c.subscribers('interview_meetings')), 1, 'alte Abos abgemeldet');
  await page.click('[data-pg-reset]');
  await page.click('[data-pg-tray-toggle]');
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('FIX9a: leere oder unplausible Daten erscheinen nie als „01.01.70“ – fehlt ein Datum, steht nichts da; Sekunden werden richtig gelesen', async () => {
  await page.evaluate(() => window.__k.sim.push('interview_meetings', [
    { id: 'imeet_z0', candidate_id: 'cand-null', candidate_name: 'Nora Null', state: 'proposed', start: 1, end: 2, created_at_ms: 20, updated_at_ms: 20 },
    { id: 'imeet_z1', candidate_id: 'cand-text', candidate_name: 'Tim Text', state: 'proposed', start: '0', created_at_ms: 21, updated_at_ms: 21 },
    { id: 'imeet_z2', candidate_id: 'cand-sek', candidate_name: 'Sina Sekunden', state: 'proposed', start: 1787047200, end: 1787049000, created_at_ms: 22, updated_at_ms: 22 },
  ]));
  await settle();
  const rowText = (id) => page.$eval(`[data-ats-list] [data-context-record-id="${id}"]`, (el) => el.textContent.replace(/\s+/g, ' ').trim());
  for (const id of ['imeet_z0', 'imeet_z1']) {
    const t = await rowText(id);
    assert.doesNotMatch(t, /1970|\.70\b|01\.01\./, `${id}: ${t}`);
    assert.doesNotMatch(t, /\d{2}\.\d{2}\./, `${id} zeigt kein Datum: ${t}`);
  }
  assert.match(await rowText('imeet_z2'), /\.08\.26/, 'Sekunden als Datum 2026');
  assert.match(await rowText('imeet_z2'), /30 Min/, 'Dauer aus Sekunden');
  // Listenansicht (dichte Zeile) ebenso.
  await page.click('[data-ats-view-toggle]');
  await settle();
  for (const id of ['imeet_z0', 'imeet_z1']) assert.doesNotMatch(await rowText(id), /1970|\.70\b|01\.01/, id);
  await page.click('[data-ats-view-toggle]');
  await page.evaluate(() => window.__k.sim.push('interview_meetings', ['imeet_z0', 'imeet_z1', 'imeet_z2'].map((id) => ({ id, _deleted: true }))));
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.click('[data-ats-select="imeet_1"]');
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
    await window.__k.sim.push('interview_meetings', { id: 'imeet_late', candidate_id: 'late', updated_at_ms: 11 });
    await window.__k.sim.settle();
    return { subs: ['interview_meetings', 'interview_scorecards'].map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  });
  assert.deepEqual(res.subs, [0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
