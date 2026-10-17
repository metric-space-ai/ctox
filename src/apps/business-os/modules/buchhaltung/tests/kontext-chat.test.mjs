// Strang BUCH (16.10.2026): Fibu-Agent-Attrappe entfernt, Fragen an den Agenten laufen über den Standard-Kontext-Chat.
//
// 1. Statisch: keine festen Antworttexte, kein simuliertes Tippen, kein Reiter „Agentenprotokoll“ mehr in
//    index.js/index.html/index.css/locales. Gegenprobe: derselbe Detektor schlägt auf der Attrappe an (eingebettete
//    Originalzeilen aus dem Stand vor BUCH, index.js:3207-3245 / index.html:456-481).
// 2. Echter v4-Rahmen (Muster rundgang2.mjs / Geometrie-Labor): App über die Desktop-Kachel mit echter Maus öffnen,
//    Buchung anklicken, Rechtsklick auf die Buchung → Rahmen-Kontextmenü „CTOX fragen…“ → Eingabeblatt → Senden.
//    Erwartet: genau ein Befehl ctox.chat.task unter der App-Identität buchhaltung mit Datensatzbezug, keine
//    Antwortanzeige in der App-Fläche. Gegenproben: Esc im Eingabeblatt sendet nichts; Rechtsklick ohne Datensatz
//    sendet ohne record_ref.
//
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/buchhaltung/tests/kontext-chat.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFileSync, existsSync, mkdirSync } from 'node:fs';
import { join, dirname, extname, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';

process.env.CTOX_APP_API_STUB ||= join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const { appConfig, configs, resolveLabPath, labBinding, labBody } = await import('../../../v4/tools/geometry/geometry-v4.mjs');
const { realManifestRoutes } = await import('../../../v4/tools/port/real-manifest.mjs');

const APP = 'buchhaltung';
const HERE = dirname(fileURLToPath(import.meta.url));
const MOD = normalize(join(HERE, '..'));
const SHOTS = join(HERE, 'shots');

// ---------- 1. Statisch ----------
// Erkennungsmerkmale der Attrappe: Antworttexte, erfundene Rechnung im Chat, Tipp-Simulation, Reiter, Bausteine.
const FAKE_MARKERS = [
  /Analysiere Anfrage/,
  /Fibu-Agent fragen/,
  /Agentenprotokoll/,
  /Rechnung HET-2026-98127\)/,
  /Vorkontierung:<br>/,
  /Bankabgleich abgeschlossen\./,
  /Bilanz aktualisiert\./,
  /Nicht verstanden – z\. B\./,
  /handleSendAgentMsg/,
  /appendChatMessage/,
  /data-chat-messages|data-chat-input|send-chat-msg/,
  /data-right-tab="agent"|data-right-subpanel="agent"/,
  /fibu-agent-chat|\.chat-message|\.message-bubble/,
];
const fakeHits = (text) => FAKE_MARKERS.filter((re) => re.test(text)).map(String);

// Originalzeilen der Attrappe (Stand vor BUCH, gekürzt) – nur für die Gegenprobe des Detektors.
const OLD_SNIPPET = `
<button type="button" class="ctox-pane-tab" role="tab" data-right-tab="agent" aria-selected="false">Agentenprotokoll</button>
<div class="fibu-right-subpanel" data-right-subpanel="agent" hidden>
<input type="text" class="ctox-input" data-chat-input placeholder="Fibu-Agent fragen..." />
async function handleSendAgentMsg() {
  appendChatMessage('🤖', 'Analysiere Anfrage...', 'system');
  setTimeout(() => {
      appendChatMessage('🤖', \`Vorkontierung:<br>
      <strong>Hetzner Online GmbH (Rechnung HET-2026-98127)</strong><br>
        appendChatMessage('🤖', \`Bankabgleich abgeschlossen.\`, 'system');
      appendChatMessage('🤖', \`Bilanz aktualisiert.\`, 'system');
      appendChatMessage('🤖', \`Nicht verstanden – z. B. „Bank abgleichen“ oder „Bilanz erstellen“.\`, 'system');
  }, 1000);
}`;

test('statisch: keine Attrappe mehr in buchhaltung (index.js/html/css, locales)', () => {
  const files = ['index.js', 'index.html', 'index.css', 'locales/de.json', 'locales/en.json'];
  const hits = files.flatMap((f) => fakeHits(readFileSync(join(MOD, f), 'utf8')).map((h) => `${f}: ${h}`));
  assert.deepEqual(hits, []);
});

test('statisch Gegenprobe: der Detektor erkennt die alte Attrappe', () => {
  const hits = fakeHits(OLD_SNIPPET);
  assert.ok(hits.length >= 10, `nur ${hits.length} Merkmale erkannt: ${hits.join(', ')}`);
});

test('statisch: Datensätze tragen den Kontext für das Rahmen-Kontextmenü (data-context-record-id/-type/-label)', () => {
  const js = readFileSync(join(MOD, 'index.js'), 'utf8');
  // Buchungsliste, Konten, Journal, Belege, Bankzeilen, Anlagen, Reisen, Fahrten
  assert.ok((js.match(/data-context-record-id=/g) || []).length >= 8);
  assert.ok((js.match(/data-context-record-type=/g) || []).length >= 8);
});

// ---------- 2. Echter Rahmen ----------
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2', '.wasm': 'application/wasm' };
const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>BUCH Kontext-Chat</title><style>html,body{margin:0;height:100%}</style></head><body><div id="ctox-shell"></div>
<script type="module">
import { createStubKernel } from '/~stub/ctox-app-api-stub.mjs';
// PIN-ALL: Rahmen wie vom Kernel unter /~a/shell@<ver>/ laden – er leitet seine Shell-Version (für shell_preferences über
// die App-API der Shell) aus der eigenen URL ab; unter /v4/ wäre das 'dev' ohne veröffentlichtes Manifest.
import * as frame from '/~a/shell@lab/v4/frame/index.js';
import { createWindowHost } from '/v4/shell/app-host.js';
const app = ${JSON.stringify(APP)};
const cfg = await (await fetch('/__bt/cfg.json')).json();
const bt = window.__bt = { commands: [], writes: [], zone: [] };
const kernel = createStubKernel({ instanceId: 'bt-' + Date.now(), userId: 'owner', storage: 'memory', latencyMs: 1, multiTab: false, locale: 'de-DE' });
await kernel.ready;
for (const [c, rows] of Object.entries(cfg.seed)) { try { await kernel.sim.push(c, rows); } catch {} }
// PIN-ALL (E48): echte Routen/Rechte/Collections aus v4/manifests/{shell,buchhaltung}.manifest.json; ohne Deklaration sperrt
// der Kern einer veröffentlichten App Fremd- und eigene Collections (der Rahmen liest shell_preferences als Fremdlesen).
for (const o of [...(cfg.shellRoutes?._fixture_owners ?? []), ...(cfg.routes?._fixture_owners ?? [])]) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
try { await kernel.sim.publishAppVersion('shell', 'lab', { entry: 'v4/frame/index.js', manifestExtra: cfg.shellRoutes }); } catch {}
await kernel.sim.publishAppVersion(app, '1.0.0', { entry: cfg.entry, layout: cfg.layout, styles: cfg.styles, manifestExtra: cfg.routes });
await kernel.sim.settle();
const wrapData = (data) => {
  const out = { ...data };
  out.command = function (type, payload, opts) { bt.commands.push(JSON.parse(JSON.stringify({ type, payload, opts: opts || null }))); return data.command.call(data, type, payload, opts); };
  for (const m of ['put', 'patch', 'remove']) out[m] = function (...a) { bt.writes.push(m + ' ' + a[0]); return data[m].apply(data, a); };
  return out;
};
const wrapHost = (o) => {
  const k = o.api.kernel;
  if (k && !k.__btWrapped) { const orig = k.appApi.bind(k); k.appApi = (id, ver) => (id === app ? wrapData(orig(id, ver)) : orig(id, ver)); k.__btWrapped = true; }
  return createWindowHost(o);
};
await kernel.sim.startShell({ startShell: (k, o = {}) => frame.startShell(k, { ...o, createWindowHost: wrapHost }), stopShell: () => frame.stopShell() });
bt.kernel = kernel;
window.__btReady = true;
</script></body></html>`;

let server, browser, page;
const errors = [];
const W = `.shell-window[data-window-app="${APP}"]`;

async function realClick(x, y, opts = {}) {
  await page.mouse.move(x, y);
  await page.mouse.click(x, y, opts);
}
async function clickEl(selector, opts = {}) {
  const b = await page.locator(selector).first().boundingBox();
  assert.ok(b, `nicht sichtbar: ${selector}`);
  await realClick(b.x + Math.min(b.width / 2, 60), b.y + b.height / 2, opts);
}

before(async () => {
  const cfg = appConfig(APP);
  for (const rows of Object.values(cfg.seed)) for (const r of rows) delete r._deleted;
  cfg.routes = realManifestRoutes(APP);
  cfg.shellRoutes = realManifestRoutes('shell');
  configs.set(APP, cfg);
  server = createServer((req, res) => {
    const p = decodeURIComponent(new URL(req.url, 'http://bt').pathname).replace(/^(\/~a\/[^/]+?)~shell@[^/~]+/, '$1');
    if (p === '/__bt/page.html') { res.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' }); res.end(PAGE); return; }
    if (p === '/__bt/cfg.json') { res.writeHead(200, { 'content-type': 'application/json' }); res.end(JSON.stringify(cfg)); return; }
    const file = resolveLabPath(p);
    const binding = labBinding(p);
    if (!file || !binding) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
    res.end(labBody(file, undefined, binding));
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const require = createRequire(join(homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
  const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
  const exe = join(homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
  browser = await chromium.launch({ headless: true, ...(existsSync(exe) ? { executablePath: exe } : {}) });
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/__bt/page.html`);
  await page.waitForFunction(() => window.__btReady === true, null, { timeout: 30000 });
  await page.waitForTimeout(300);
  await clickEl(`[data-v4-desk-app="${APP}"]`);
  await page.waitForSelector(`${W} .v4-app-root`, { timeout: 15000 });
  await page.waitForTimeout(900);
  for (const sel of ['[data-window-layout-control="toggle"]', '[data-window-layout-control="maximize"]']) {
    const b = await page.locator(sel).first().boundingBox({ timeout: 2000 }).catch(() => null);
    if (b) { await realClick(b.x + b.width / 2, b.y + b.height / 2); await page.waitForTimeout(350); }
  }
  await page.waitForTimeout(400);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const chatCommands = () => page.evaluate(() => window.__bt.commands.filter((c) => c.type === 'ctox.chat.task' || c.type === 'ctox.app.modify.request'));
const menuLabels = () => page.evaluate((w) => [...document.querySelectorAll(`${w} [data-v4-menu] [data-v4-menu-item]`)].map((b) => b.textContent.trim()), W);

test('Rahmen: kein Reiter „Agentenprotokoll“, keine Chat-Attrappe im Fenster', async () => {
  await clickEl(`${W} [data-booking-key]`);
  await page.waitForTimeout(400);
  const res = await page.evaluate((w) => {
    const win = document.querySelector(w);
    return {
      tabs: [...win.querySelectorAll('[role="tab"]')].map((t) => t.textContent.trim()),
      fake: Boolean(win.querySelector('[data-chat-messages], [data-chat-input], [data-action="send-chat-msg"], [data-right-tab]')),
      evidenceVisible: !win.querySelector('.fibu-module').classList.contains('is-evidence-hidden'),
    };
  }, W);
  assert.ok(!res.tabs.includes('Agentenprotokoll'), JSON.stringify(res.tabs));
  assert.equal(res.fake, false);
  assert.equal(res.evidenceVisible, true, 'Evidenzspalte nach Auswahl sichtbar');
  mkdirSync(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, 'buch-evidenz-1280.png') });
});

test('Rechtsklick auf Buchung → „CTOX fragen…“ → ctox.chat.task mit Buchhaltungskontext, keine Antwort in der App', async () => {
  const row = page.locator(`${W} [data-booking-key]`).first();
  const rec = await row.evaluate((el) => ({ id: el.dataset.contextRecordId, type: el.dataset.contextRecordType, label: el.dataset.contextLabel }));
  const before = (await chatCommands()).length;
  const textBefore = await page.$eval(`${W} .v4-app-root`, (el) => el.innerText);
  await clickEl(`${W} [data-booking-key]`, { button: 'right' });
  await page.waitForSelector(`${W} [data-v4-menu]`);
  assert.deepEqual((await menuLabels()).at(-1), 'CTOX fragen…');
  await page.screenshot({ path: join(SHOTS, 'buch-kontextmenue-1280.png') });
  await clickEl(`${W} [data-v4-menu-item="ctox.ask"]`);
  await page.waitForSelector(`${W} [data-v4-sheet-input]`);
  await page.keyboard.type('Ist diese Buchung richtig kontiert?');
  await page.screenshot({ path: join(SHOTS, 'buch-frage-1280.png') });
  await clickEl(`${W} [data-v4-sheet-ok]`);
  await page.waitForFunction((n) => window.__bt.commands.filter((c) => c.type === 'ctox.chat.task').length > n, before);
  const cmds = await chatCommands();
  assert.equal(cmds.length, before + 1);
  const cmd = cmds.at(-1);
  assert.equal(cmd.type, 'ctox.chat.task');
  assert.equal(cmd.payload.source_app, APP);
  assert.equal(cmd.payload.text, 'Ist diese Buchung richtig kontiert?');
  assert.equal(cmd.payload.record_ref, rec.id);
  assert.equal(cmd.payload.context.record_type, rec.type);
  assert.equal(cmd.payload.context.record_id, rec.id);
  assert.equal(cmd.payload.context.label, rec.label);
  assert.equal(cmd.opts?.record_id, rec.id);
  await page.waitForTimeout(1300); // länger als das frühere simulierte Tippen (1 s)
  const textAfter = await page.$eval(`${W} .v4-app-root`, (el) => el.innerText);
  assert.equal(textAfter, textBefore, 'keine Antwort/Tippanzeige in der App-Fläche');
  assert.deepEqual(fakeHits(textAfter), []);
  await page.screenshot({ path: join(SHOTS, 'buch-gesendet-1280.png') });
});

test('Gegenprobe: Esc im Eingabeblatt sendet nichts', async () => {
  const before = (await chatCommands()).length;
  await clickEl(`${W} [data-booking-key]`, { button: 'right' });
  await page.waitForSelector(`${W} [data-v4-menu]`);
  await clickEl(`${W} [data-v4-menu-item="ctox.ask"]`);
  await page.waitForSelector(`${W} [data-v4-sheet-input]`);
  await page.keyboard.type('nicht senden');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);
  assert.equal((await chatCommands()).length, before);
});

test('Gegenprobe: Rechtsklick ohne Datensatz → Auftrag ohne record_ref (nur App-Bezug)', async () => {
  const before = (await chatCommands()).length;
  await clickEl(`${W} .fibu-center .ctox-pane-title`, { button: 'right' });
  await page.waitForSelector(`${W} [data-v4-menu]`);
  assert.deepEqual(await menuLabels(), ['CTOX fragen…']);
  await clickEl(`${W} [data-v4-menu-item="ctox.ask"]`);
  await page.waitForSelector(`${W} [data-v4-sheet-input]`);
  await page.keyboard.type('Was ist in dieser Ansicht offen?');
  await page.keyboard.press('Enter');
  await page.waitForFunction((n) => window.__bt.commands.filter((c) => c.type === 'ctox.chat.task').length > n, before);
  const cmd = (await chatCommands()).at(-1);
  assert.equal(cmd.payload.source_app, APP);
  assert.equal(cmd.payload.record_ref, '');
  assert.deepEqual(errors, []);
});
