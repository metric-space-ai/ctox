// node --test src/apps/business-os/v4/tools/geometry/geometry-v4.test.mjs
// Strang VIS4: G7 (Geschwister-Pane-Köpfe gleich hoch, Fixtures + Mutant), Rahmen-Angleichung v4/frame/pane-heads.js
// samt theme.css-Regel (wachsen, schrumpfen, Breitenwechsel; Mutant ohne Markierung), Zonen-Fall-Bewertung.
// Labor-Tests Strang GEO2: G1-Ausnahme [data-scroll-x] (Positiv-/Negativ-Fixtures + Mutanten), ctox:lib im
// Labor-Server wie v4/sw.js, G6-Vergleichsfenster in der echten Fensterbreite (Integrationslauf über eine App mit
// min_width 640 bei 420 px) und spreadsheets lädt ctox:lib/ctox-office@1 ohne Seitenfehler.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir, homedir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { measureInPage } from './measure.mjs';
import { judgeZoneCase, ZONE_CASES } from './zone-cases.mjs';
import { buildFixtures, BASE_PATCHES } from './fixtures.mjs';
import { serve, resolveLabPath, labBody, labBinding, LIB_DIRS, LAB_LIB_BINDING, loadSwRewrite, LAB_SHELL, runG11 } from './geometry-v4.mjs';
import { g11Static } from './g11.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = join(HERE, '..', '..', '..');
const require = createRequire(join(homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const EXE = join(homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');

// ---------------------------------------------------------------------------------------------------------------
// G1-Fixtures: Fenster 400 px, root scrollt vertikal; Inhalt je Fall. Gemessen wird mit der echten measureInPage.
// ---------------------------------------------------------------------------------------------------------------
const TABLE = '<table style="width:900px;border-collapse:collapse"><tr><td>Firma</td><td>Ort</td><td>Umsatz</td></tr></table>';
const FIXTURES = {
  // positiv: Datentabelle scrollt in ihrem eigenen Container mit [data-scroll-x]
  pos: { root: '', html: `<div data-scroll-x style="overflow-x:auto">${TABLE}</div>` },
  // positiv: verschachtelter Scroller innerhalb eines [data-scroll-x]-Containers
  posNested: { root: '', html: `<section data-scroll-x style="overflow-x:auto"><div style="overflow-x:auto">${TABLE}</div></section>` },
  // negativ: derselbe Container ohne Attribut
  negNoAttr: { root: '', html: `<div style="overflow-x:auto">${TABLE}</div>` },
  // negativ: Attribut an root selbst, root scrollt seitlich (App-Fläche bleibt streng)
  negRoot: { root: 'data-scroll-x', html: TABLE },
  // negativ: Attribut am Fenster (Vorfahr von root) – gilt nicht für den Inhalt
  negWindow: { win: 'data-scroll-x', root: '', html: `<div style="overflow-x:auto">${TABLE}</div>` },
  // negativ: der [data-scroll-x]-Container ist selbst breiter als root und ragt heraus
  negContainerOut: { root: 'style="overflow-x:hidden;overflow-y:auto"', html: `<div data-scroll-x style="overflow-x:auto;width:700px">${TABLE}</div>` },
};
function page(fx) {
  return `<!doctype html><html><head><meta charset="utf-8"><style>
    body{margin:0;font:14px system-ui}
    .shell-window{position:absolute;left:10px;top:10px;width:400px;height:300px;display:flex;flex-direction:column}
    .shell-window-header{height:30px;flex:0 0 auto}
    .shell-window-content{flex:1 1 auto;min-height:0;display:flex}
    .v4-app-root{flex:1 1 auto;min-width:0;overflow:auto}
  </style></head><body>
  <section class="shell-window" ${fx.win || ''}><header class="shell-window-header"><span class="shell-window-title-text">Fixture</span></header>
  <div class="shell-window-content"><div class="v4-app-root" ${fx.root || ''}>${fx.html}</div></div></section>
  <script>
    const win = document.querySelector('.shell-window');
    window.__geo = { titles: ['fx'], bodyBefore: [...document.body.children], sheetsBefore: new Set([...document.styleSheets].map((s) => s.ownerNode)),
      kernel: { status: {} }, frame: { shellHandle: () => ({ win: () => ({ handle: { element: win } }) }), describeShell: () => ({}) } };
  </script></body></html>`;
}

let browser;
test.before(async () => { browser = await chromium.launch({ headless: true, executablePath: EXE }); });
test.after(async () => { await browser?.close(); });

async function g1(fn, fx) {
  const ctx = await browser.newContext({ viewport: { width: 800, height: 600 } });
  const p = await ctx.newPage();
  await p.setContent(page(fx));
  const r = await p.evaluate(fn, { app: 'fx', vocab: [], allow: [], owner: 'fx', withG5: false });
  await ctx.close();
  return r;
}
// Mutanten aus measure.mjs (keine Importe, daher in einem Temp-Ordner lauffähig).
const SRC = readFileSync(join(HERE, 'measure.mjs'), 'utf8');
const MUT_DIR = mkdtempSync(join(tmpdir(), 'geo2-mut-'));
test.after(() => rmSync(MUT_DIR, { recursive: true, force: true }));
async function mutant(name, from, to) {
  assert.ok(SRC.includes(from), `Mutationsstelle fehlt in measure.mjs: ${from}`);
  const f = join(MUT_DIR, `${name}.mjs`);
  writeFileSync(f, SRC.replace(from, to));
  return (await import(pathToFileURL(f).href)).measureInPage;
}

test('G1 positiv: Überstand im [data-scroll-x]-Container zählt nicht, steht aber als Hinweis', async () => {
  for (const k of ['pos', 'posNested']) {
    const r = await g1(measureInPage, FIXTURES[k]);
    assert.deepEqual(r.G1, [], `${k}: ${JSON.stringify(r.G1)}`);
    assert.ok(r.notes.some((n) => n.startsWith('G1 erlaubt:')), `${k}: Hinweis fehlt ${JSON.stringify(r.notes)}`);
  }
});

test('G1 negativ: ohne Attribut, am root, am Fenster, Container ragt heraus – weiterhin G1', async () => {
  for (const k of ['negNoAttr', 'negRoot', 'negWindow', 'negContainerOut']) {
    const r = await g1(measureInPage, FIXTURES[k]);
    assert.ok(r.G1.length > 0, `${k} müsste G1 sein: ${JSON.stringify(r)}`);
  }
});

test('Mutant 1 (Ausnahme entfernt): positive Fixture wird rot – die Ausnahme ist tragend', async () => {
  const m = await mutant('m1', '(inScrollX(el) ? hscrollAllowed : hscroll).push', 'hscroll.push');
  const r = await g1(m, FIXTURES.pos);
  assert.ok(r.G1.length > 0, JSON.stringify(r.G1));
});

test('Mutant 2 (Attribut überall gültig): Fenster-Attribut würde durchgewunken – die root-Grenze ist tragend', async () => {
  const m = await mutant('m2', 'return !!c && c !== root && root.contains(c);', 'return !!c;');
  const r = await g1(m, FIXTURES.negWindow);
  assert.deepEqual(r.G1, [], 'Mutant 2 sollte negWindow fälschlich grün messen');
  const real = await g1(measureInPage, FIXTURES.negWindow);
  assert.ok(real.G1.length > 0);
});

// ---------------------------------------------------------------------------------------------------------------
// ctox:lib im Labor-Server = v4/sw.js
// ---------------------------------------------------------------------------------------------------------------
test('Labor-Server schreibt mit der rewriteShellRefs aus v4/sw.js um (JS und CSS, ctox:shared und ctox:lib mit Minor)', () => {
  const sw = loadSwRewrite();
  const js = "const m = await import('ctox:lib/ctox-office@1/ctox-office-spreadsheet.mjs'); import { t } from 'ctox:shared/i18n.js'; const x = 'ctox:lib/evil@1/a.js';";
  const outJs = labBody('/x/a.js', Buffer.from(js), LAB_LIB_BINDING).toString();
  assert.equal(outJs, sw.rewriteShellRefs(js, 'js', LAB_SHELL, LAB_LIB_BINDING));
  // Form folgt sw.js (§16.39: /~a/lib:<name>@<major>.<minor>[~lib:…]/…); der Server muss sie auflösen.
  const libUrl = /'(\/~a\/lib:ctox-office@1\.0~lib:monaco@0\.0\/ctox-office-spreadsheet\.mjs)'/.exec(outJs)?.[1];
  assert.ok(libUrl, outJs);
  assert.ok(resolveLabPath(libUrl)?.endsWith('vendor/ctox-office/ctox-office-spreadsheet.mjs'), libUrl);
  assert.ok(outJs.includes("'ctox:lib/evil@1/a.js'"), 'nicht freigegebene Bibliothek bleibt unverändert');
  const css = "@import url('ctox:shared/office-workspace.css');";
  assert.ok(labBody('/x/a.css', Buffer.from(css)).toString().includes(`/~a/shell@${LAB_SHELL}/shared/office-workspace.css`));
  // Bindung aus der URL: gebundene App-URL, Bibliotheks-URL (Kopf + Rest), ungebunden → {} (kein Ersatz, L-34), mehrdeutig → null
  assert.deepEqual(labBinding('/~a/spreadsheets@1~shell@x~lib:ctox-office@1.7/index.js'), { 'ctox-office@1': '1.7' });
  assert.deepEqual(labBinding('/~a/lib:monaco@0.32~lib:ctox-office@1.4/vs/a.js'), { 'ctox-office@1': '1.4', 'monaco@0': '0.32' });
  assert.deepEqual(labBinding('/~a/spreadsheets@1.0.0/index.js'), {});
  // die Wurzel, die der Labor-Kernel per artifacts.bind baut, trägt genau LAB_LIB_BINDING
  assert.deepEqual(labBinding(`/~a/spreadsheets@1.0.0~shell@${LAB_SHELL}~lib:ctox-office@1.0~lib:monaco@0.0/index.js`), { ...LAB_LIB_BINDING });
  assert.ok(!labBody('/x/a.js', Buffer.from(js), labBinding('/~a/spreadsheets@1.0.0/index.js')).toString().includes('/~a/lib:'), 'ungebunden: ctox:lib bleibt unaufgelöst');
  assert.equal(labBinding('/~a/lib:monaco@0.32~lib:monaco@0.33/vs/a.js'), null);
  const bound = labBody('/x/a.js', Buffer.from(js), labBinding('/~a/spreadsheets@1~lib:ctox-office@1.7/index.js')).toString();
  assert.ok(bound.includes("'/~a/lib:ctox-office@1.7/ctox-office-spreadsheet.mjs'"), bound);
});

test('Labor-Server liefert nur freigegebene Bibliotheken aus ihrem Quellordner (standard-libs.json), nur mit Minor', async () => {
  assert.deepEqual(Object.keys(LIB_DIRS).sort(), ['ctox-office@1', 'monaco@0']);
  // Form §16.39: eigenes Plattform-Artefakt lib:<name>@<major>.<minor>, optional mit Rest-Bindung
  assert.ok(resolveLabPath('/~a/lib:ctox-office@1.4/ctox-office-spreadsheet.mjs')?.endsWith('vendor/ctox-office/ctox-office-spreadsheet.mjs'));
  assert.ok(resolveLabPath('/~a/lib:monaco@0.32~lib:ctox-office@1.4/vs/loader.js')?.endsWith('vendor/monaco/vs/loader.js'));
  assert.equal(resolveLabPath('/~a/lib:evil@1.0/x.js'), null);
  assert.equal(resolveLabPath('/~a/lib:ctox-office@2.0/ctox-office-spreadsheet.mjs'), null);
  assert.equal(resolveLabPath('/~a/lib:ctox-office@1.4/../../app.css'), null);
  // alte Formen gibt es nicht mehr
  assert.equal(resolveLabPath('/~a/lib:ctox-office@1/ctox-office-spreadsheet.mjs'), null, 'ohne Minor');
  assert.equal(resolveLabPath(`/~a/shell@${LAB_SHELL}/lib/ctox-office@1/ctox-office-spreadsheet.mjs`), null, '/~a/shell@<v>/lib/');
  const server = await serve();
  try {
    const base = `http://127.0.0.1:${server.address().port}`;
    // Wurzel wie artifacts.bind des Labor-Kernels (veröffentlichte Shell lab + lib:<name>@<major>.0)
    const app = await fetch(`${base}/~a/spreadsheets@1.0.0~shell@${LAB_SHELL}~lib:ctox-office@1.0~lib:monaco@0.0/index.js`).then((r) => r.text());
    assert.ok(!app.includes('ctox:lib/'), 'spreadsheets/index.js enthält nach dem Umschreiben kein ctox:lib/ mehr');
    assert.ok(app.includes('/~a/lib:ctox-office@1.0'), 'gebundene Kernel-URL → deren Minor');
    const unbound = await fetch(`${base}/~a/spreadsheets@1.0.0/index.js`).then((r) => r.text());
    assert.ok(unbound.includes('ctox:lib/'), 'ungebunden: keine Ersatzbindung (L-34), ctox:lib/ bleibt');
    const pinned = await fetch(`${base}/~a/spreadsheets@1.0.0~shell@x~lib:ctox-office@1.7/index.js`).then((r) => r.text());
    assert.ok(pinned.includes('/~a/lib:ctox-office@1.7/'), 'gebundene URL → deren Minor');
    assert.equal((await fetch(`${base}/~a/lib:ctox-office@1.0~lib:monaco@0.0/ctox-office-spreadsheet.mjs`)).status, 200);
    assert.equal((await fetch(`${base}/~a/lib:ctox-office@1/ctox-office-spreadsheet.mjs`)).status, 404);
    assert.equal((await fetch(`${base}/~a/lib:evil@1.0/x.js`)).status, 404);
    assert.equal((await fetch(`${base}/~a/shell@${LAB_SHELL}/lib/evil@1/x.js`)).status, 404);
  } finally { server.close(); }
});

// ---------------------------------------------------------------------------------------------------------------
// Integration: echter Rahmen, eine App mit min_width 640 (tickets) und spreadsheets, 1280 + 420.
// ---------------------------------------------------------------------------------------------------------------
test('Integration: G6-Vergleichsfenster ist bei 420 genau 420 px breit; spreadsheets lädt ctox:lib ohne Fehler', { timeout: 240000 }, () => {
  const out = mkdtempSync(join(tmpdir(), 'geo2-int-'));
  try {
    execFileSync(process.execPath, [join(HERE, 'geometry-v4.mjs'), '--app', 'tickets,spreadsheets', '--widths', '1280,420', '--out', out], { cwd: BOS, stdio: 'pipe', timeout: 230000 });
    const r = JSON.parse(readFileSync(join(out, 'results.json'), 'utf8'));
    const t420 = r.results.find((x) => x.app === 'tickets' && x.width === 420);
    assert.equal(t420.g6.viewportWindowW, 420);
    assert.equal(t420.g6.compareWindowW, 420, `Vergleichsfenster ${t420.g6.compareWindowW} px`);
    for (const x of r.results.filter((y) => y.app === 'spreadsheets')) {
      const libErr = x.errors.filter((e) => /ctox:lib|ERR_UNKNOWN_URL_SCHEME|\/lib\/ctox-office/.test(e));
      assert.deepEqual(libErr, [], `spreadsheets ${x.width}: ${JSON.stringify(x.errors)}`);
      assert.ok(!x.G1.some((m) => /Labor-Fehler|Fenster nach dem Öffnen wieder weg/.test(m)), JSON.stringify(x.G1));
    }
  } finally { rmSync(out, { recursive: true, force: true }); }
});

// ---------------------------------------------------------------------------------------------------------------
// G7: Geschwister-Pane-Köpfe eines Fensters gleich hoch (±1 px) – Strang VIS4
// ---------------------------------------------------------------------------------------------------------------
const head = (title, h = 0) => `<header class="ctox-pane-header" style="padding:8px;${h ? `height:${h}px;` : ''}"><h2 class="ctox-pane-title" style="margin:0;font-size:16px">${title}</h2></header>`;
const cols = (a, b) => `<div class="cols" style="display:flex;align-items:flex-start"><section class="col-a" style="width:150px">${a}</section><section class="col-b" style="width:150px">${b}</section></div>`;
const G7FX = {
  // positiv: zwei Köpfe nebeneinander, gleich hoch
  posEqual: { root: '', html: cols(head('Liste', 40), head('Detail', 40)) },
  // positiv: Köpfe untereinander (andere Oberkante) dürfen verschieden hoch sein
  posStacked: { root: '', html: `${head('Oben', 40)}${head('Unten', 60)}` },
  // negativ: nebeneinander, 40 vs 60 px
  negUnequal: { root: '', html: cols(head('Liste', 40), head('Detail', 60)) },
  // negativ: 2 px Unterschied (über der Toleranz von 1 px)
  negTwoPx: { root: '', html: cols(head('Liste', 40), head('Detail', 42)) },
};

test('G7 positiv: gleich hohe Geschwisterköpfe und gestapelte Köpfe sind grün', async () => {
  for (const k of ['posEqual', 'posStacked']) {
    const r = await g1(measureInPage, G7FX[k]);
    assert.deepEqual(r.G7, [], `${k}: ${JSON.stringify(r.G7)}`);
  }
});

test('G7 negativ: ungleich hohe Geschwisterköpfe (20 px, 2 px) sind rot', async () => {
  for (const k of ['negUnequal', 'negTwoPx']) {
    const r = await g1(measureInPage, G7FX[k]);
    assert.equal(r.G7.length, 1, `${k}: ${JSON.stringify(r.G7)}`);
  }
});

test('Mutant 3 (G7-Toleranz aufgehoben): ungleiche Köpfe würden grün – der Höhenvergleich ist tragend', async () => {
  const m = await mutant('m3', 'Math.max(...hs) - Math.min(...hs) > 1', 'Math.max(...hs) - Math.min(...hs) > 1e9');
  assert.deepEqual((await g1(m, G7FX.negUnequal)).G7, []);
  assert.equal((await g1(measureInPage, G7FX.negUnequal)).G7.length, 1);
});

// Rahmen-Angleichung: v4/frame/pane-heads.js + Kit-Regel aus theme.css an einer Fixture mit zwei Spalten, deren
// Köpfe von Natur aus verschieden hoch sind (langer Titel bricht um).
const PANE_HEADS_SRC = readFileSync(join(BOS, 'v4', 'frame', 'pane-heads.js'), 'utf8');
const THEME_CSS = readFileSync(join(BOS, 'v4', 'frame', 'theme', 'theme.css'), 'utf8');
const LONG = 'Rahmenvertrag über Wartung und Instandhaltung der Produktionsanlagen';
async function syncPage(src = PANE_HEADS_SRC) {
  const ctx = await browser.newContext({ viewport: { width: 800, height: 600 } });
  const p = await ctx.newPage();
  await p.setContent(page({ root: '', html: cols(head(LONG), head('Detail')) }));
  await p.addStyleTag({ content: THEME_CSS });
  await p.addScriptTag({ type: 'module', content: `${src}\nwindow.__ph = createPaneHeadSync(); window.__phReady = true;` });
  await p.waitForFunction(() => window.__phReady === true);
  const frames = () => p.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(r)))));
  const g7 = async () => (await p.evaluate(measureInPage, { app: 'fx', vocab: [], allow: [], owner: 'fx', withG5: false })).G7;
  const heights = () => p.evaluate(() => [...document.querySelectorAll('.ctox-pane-header')].map((h) => Math.round(h.getBoundingClientRect().height)));
  return { ctx, p, frames, g7, heights };
}

test('Rahmen pane-heads.js: Geschwisterköpfe wachsen mit dem höchsten, schrumpfen mit, folgen der Breite', async () => {
  const { ctx, p, frames, g7, heights } = await syncPage();
  try {
    assert.equal((await g7()).length, 1, 'ohne Angleichung sind die Köpfe verschieden hoch (Fixture trägt)');
    const natural = await heights();
    await p.evaluate(() => window.__ph.attach(document.querySelector('.v4-app-root')));
    await frames();
    assert.deepEqual(await g7(), [], 'nach attach gleich hoch');
    const h1 = await heights();
    assert.equal(h1[0], h1[1]);
    assert.equal(h1[0], Math.max(...natural), 'Höhe = höchster natürlicher Kopf');
    // Titel im rechten Kopf wird noch länger → beide wachsen (MutationObserver, Textänderung)
    await p.evaluate((t) => { document.querySelector('.col-b .ctox-pane-title').textContent = `${t} ${t}`; }, LONG);
    await frames();
    const h2 = await heights();
    assert.deepEqual(await g7(), []);
    assert.ok(h2[0] > h1[0], `gewachsen: ${h1} → ${h2}`);
    // wieder kurz → beide schrumpfen zurück (gesetzte Mindesthöhe darf nicht kleben bleiben)
    await p.evaluate(() => { document.querySelector('.col-b .ctox-pane-title').textContent = 'Detail'; });
    await frames();
    assert.deepEqual(await heights(), h1, 'zurück auf die vorige Höhe');
    // Spalte breiter → langer Titel bricht seltener um → beide kleiner (ResizeObserver)
    await p.evaluate(() => { document.querySelector('.col-a').style.width = '600px'; });
    await frames();
    const h3 = await heights();
    assert.deepEqual(await g7(), []);
    assert.ok(h3[0] < h1[0], `bei breiter Spalte kleiner: ${h1} → ${h3}`);
    // detach entfernt die Markierungen: natürliche Höhen kehren zurück
    await p.evaluate(() => window.__ph.detach());
    assert.equal(await p.evaluate(() => document.querySelectorAll('[data-v4-pane-row]').length), 0);
  } finally { await ctx.close(); }
});

test('Mutant 4 (pane-heads ohne Markierung): G7 bleibt rot – Markierung + theme.css-Regel sind tragend', async () => {
  const from = "el.setAttribute(ROW_ATTR, '');";
  assert.ok(PANE_HEADS_SRC.includes(from));
  const { ctx, p, frames, g7 } = await syncPage(PANE_HEADS_SRC.replace(from, ''));
  try {
    await p.evaluate(() => window.__ph.attach(document.querySelector('.v4-app-root')));
    await frames();
    assert.equal((await g7()).length, 1);
  } finally { await ctx.close(); }
  assert.ok(THEME_CSS.includes('.ctox-pane-header[data-v4-pane-row]') && THEME_CSS.includes('min-height: var(--v4-pane-head-h) !important'), 'Kit-Regel in theme.css');
});

// ---------------------------------------------------------------------------------------------------------------
// Fixtures: gültiger Grundzustand; Fehlerfälle als Zonen-Fälle (Strang VIS4)
// ---------------------------------------------------------------------------------------------------------------
test('Fixtures: erste Zeile gesund, keine Fehlerfelder, Verweise auf vorhandene Zeilen', () => {
  const news = buildFixtures('news').seed.news_issues;
  assert.notEqual(news[0].status, 'failed');
  assert.ok(news.every((r) => !r.error), JSON.stringify(news.map((r) => r.error)));
  const docs = buildFixtures('documents').seed;
  const versionIds = new Set(docs.document_versions.map((v) => v.id));
  assert.ok(docs.documents.every((d) => versionIds.has(d.current_version_id)), 'current_version_id zeigt auf document_versions');
  assert.ok(docs.document_versions.every((v) => v.blob_id === ''), 'keine hängenden Datei-Verweise');
  assert.ok(docs.documents.every((d) => d.document_type === 'markdown_document'), 'documents-Grundzustand ohne native DOCX-Vorbereitung');
  assert.equal(typeof BASE_PATCHES.documents, 'function');
  const b = buildFixtures('browser').seed.browser_sessions[0];
  assert.ok(!['failed', 'error', 'blocked', 'disconnected', 'offline'].includes(String(b.status)), b.status);
  assert.equal(b.error, '');
});

test('Zonen-Fälle: Bewertung verlangt Zonen-Eintrag + sichtbare Zone und keinen Zustandstext im Inhalt', () => {
  const c = ZONE_CASES.find((x) => x.app === 'news');
  const shown = ['Zustandszone sichtbar: „Ausgabe fehlgeschlagen“'];
  assert.equal(judgeZoneCase(c, { G3: shown, info: { zoneEntries: ['app:news.issue.failed'] } }).ok, true);
  assert.equal(judgeZoneCase(c, { G3: [], info: { zoneEntries: [] } }).ok, false, 'Zone fehlt');
  assert.equal(judgeZoneCase(c, { G3: shown, info: { zoneEntries: ['app:news.other'] } }).ok, false, 'falscher Code');
  assert.equal(judgeZoneCase(c, { G3: [...shown, '1 Zustandstexte: „Fehler“'], info: { zoneEntries: ['app:news.issue.failed'] } }).ok, false, 'Text im Inhalt');
  assert.ok(ZONE_CASES.every((z) => typeof z.patch === 'function' && z.expect && z.name));
});

// ---------------------------------------------------------------------------------------------------------------
// G8: reservierte Zonen unter dem schwebenden Icon-Header und Knopfcluster (FR6a)
// ---------------------------------------------------------------------------------------------------------------
const ZONES = 'style="--shell-icon-inset:82px;--shell-icon-zone-height:74px;--shell-controls-inset:96px;--shell-controls-zone-height:44px"';
const G8FX = {
  // positiv: Werkzeugleiste rechts eingerückt (wie das Kit), Suchzeile nur links (unter dem Icon), Filter darunter voll
  pos: { win: ZONES, root: '', html: '<div style="display:flex;justify-content:space-between;height:44px;padding:0 96px 0 82px"><b>Titel</b><button>PNG speichern</button></div><div style="padding:4px 0 0 82px;height:30px"><input style="width:100%;box-sizing:border-box" placeholder="Suche"></div><div><input style="width:100%;box-sizing:border-box" placeholder="Filter unter beiden Zonen, volle Breite"></div>' },
  // negativ: Knopf oben rechts unter dem Cluster (Black-Hole-Fall)
  negRight: { win: ZONES, root: '', html: '<div style="display:flex;justify-content:flex-end;height:40px"><button>Vollbild</button></div>' },
  // negativ: Knopf oben links unter dem Icon-Header
  negLeft: { win: ZONES, root: '', html: '<div style="height:40px"><button>Menü</button></div>' },
};
test('G8 positiv: Bedienelemente außerhalb der Zonen sind grün', async () => {
  const r = await g1(measureInPage, G8FX.pos);
  assert.deepEqual(r.G8, [], JSON.stringify(r.G8));
});
test('G8 negativ: Knopf unter Knopfcluster bzw. Icon-Header ist rot', async () => {
  for (const k of ['negRight', 'negLeft']) {
    const r = await g1(measureInPage, G8FX[k]);
    assert.equal(r.G8.length, 1, `${k}: ${JSON.stringify(r.G8)}`);
  }
});
test('Mutant 5 (Zonenschnitt aufgehoben): Knopf unter dem Cluster würde grün – der Rechteckvergleich ist tragend', async () => {
  const m = await mutant('g8-off', 'if (hits.length) out.G8.push(', 'if (false && hits.length) out.G8.push(');
  assert.deepEqual((await g1(m, G8FX.negRight)).G8, []);
  assert.equal((await g1(measureInPage, G8FX.negRight)).G8.length, 1);
});

// ---------------------------------------------------------------------------------------------------------------
// G11 schmal (Strang KIT1): Rahmen-Kit v4/frame/pane-narrow.js + frame.css an einer Fixture-App (Workspace mit Liste und
// Detail, acht Reiter mit Zählern) in einem 420 px breiten Fenster. Ohne Kit rot, mit Kit grün (statisch und mit echtem
// Mausklick über runG11), Mutanten am Kit (Zurück, Scrollstand, Reiterüberlauf) und am Kriterium.
// ---------------------------------------------------------------------------------------------------------------
const PANE_NARROW_SRC = readFileSync(join(BOS, 'v4', 'frame', 'pane-narrow.js'), 'utf8');
const FRAME_CSS = readFileSync(join(BOS, 'v4', 'frame', 'frame.css'), 'utf8');
const G11_SRC = readFileSync(join(HERE, 'g11.mjs'), 'utf8');
const TAB_NAMES = ['Übersicht', 'Kontakte', 'Verkaufschancen', 'Aktivitäten', 'Aufgaben', 'Notizen', 'Dateien', 'Dubletten'];
function kitPage(width) {
  const rows = Array.from({ length: 40 }, (_, i) => `<button type="button" class="ctox-list-item" data-id="r${i}" style="display:block;width:100%;height:44px;text-align:left">Kunde ${i}</button>`).join('');
  const tabs = TAB_NAMES.map((t, i) => `<button type="button" class="ctox-pane-tab${i === 0 ? ' is-active' : ''}" data-tab="${t}" style="padding:0 10px;height:24px;white-space:nowrap;min-width:52px">${t}(${i + 1})</button>`).join('');
  return `<!doctype html><html><head><meta charset="utf-8"><style>
    body{margin:0;font:13px system-ui}
    .ctox-pane{display:grid;grid-template-rows:auto minmax(0,1fr);min-width:0;min-height:0;overflow:hidden}
    .ctox-pane-tabs{display:inline-flex;flex-wrap:wrap;gap:2px;padding:2px;min-width:max-content}
    .ctox-pane-title-row{display:flex;align-items:center;min-height:32px}
  </style></head><body><div class="v4f">
  <section class="shell-window" style="position:absolute;left:0;top:0;width:${width}px;height:700px;display:flex;flex-direction:column">
  <header class="shell-window-header" style="height:30px;flex:0 0 auto"></header>
  <div class="shell-window-content" style="position:relative;flex:1 1 auto;min-height:0;--shell-icon-inset:82px;--shell-icon-zone-height:74px;--shell-controls-inset:96px;--shell-controls-zone-height:44px">
  <div class="v4-app-root" style="position:absolute;inset:0;overflow:auto">
  <main class="ctox-workspace fx-ws" style="display:grid;grid-template-columns:${width < 600 ? 'minmax(0,1fr)' : '300px minmax(0,1fr)'};height:100%">
    <aside class="ctox-pane fx-list"><header class="ctox-pane-header"><div class="ctox-pane-title-row" style="padding-left:82px"><h2 style="margin:0">Liste</h2></div></header>
      <div class="fx-scroll" style="overflow:auto;min-height:0">${rows}</div></aside>
    <section class="ctox-pane fx-detail"><header class="ctox-pane-header"><div class="ctox-pane-title-row"><h2 class="fx-title" style="margin:0">Detail</h2></div>
      <nav class="fx-band" style="display:flex;flex-wrap:wrap"><div class="ctox-pane-tabs">${tabs}</div></nav></header>
      <div class="fx-body">Inhalt</div></section>
  </main></div></div></section></div>
  <script>
    const win = document.querySelector('.shell-window');
    window.__geo = { frame: { shellHandle: () => ({ win: () => ({ handle: { element: win } }) }) } };
    // „App“: Auswahl setzt aria-selected und rendert das Detail neu; Reiter setzen is-active.
    document.querySelector('.fx-ws').addEventListener('click', (ev) => {
      const row = ev.target.closest('.ctox-list-item');
      // wie viele Apps: Auswahl rendert die Liste neu (neues Scroll-Element) und das Detail
      if (row) { const sc = document.querySelector('.fx-scroll'); const fresh = sc.cloneNode(true); for (const r of fresh.querySelectorAll('.ctox-list-item')) r.setAttribute('aria-selected', String(r.dataset.id === row.dataset.id)); sc.replaceWith(fresh); document.querySelector('.fx-title').textContent = row.textContent; }
      const tab = ev.target.closest('.ctox-pane-tab');
      if (tab) { for (const t of document.querySelectorAll('.ctox-pane-tab')) t.classList.toggle('is-active', t === tab); window.__tab = tab.dataset.tab; }
    });
  </script></body></html>`;
}
async function kitFixture({ width = 420, kitSrc = PANE_NARROW_SRC, kit = true } = {}) {
  const ctx = await browser.newContext({ viewport: { width: Math.max(width, 420) + 20, height: 760 } });
  const p = await ctx.newPage();
  await p.setContent(kitPage(width));
  await p.addStyleTag({ content: FRAME_CSS });
  if (kit) {
    // wie im Rahmen (column-resizers.js): pane-heads.js rückt Kopfzeilen aus den reservierten Zonen, pane-narrow.js daneben
    await p.addScriptTag({ type: 'module', content: `${PANE_HEADS_SRC}\ncreatePaneHeadSync().attach(document.querySelector('.v4-app-root'));` });
    await p.addScriptTag({ type: 'module', content: `${kitSrc}\nwindow.__pn = createPaneNarrow(); window.__pn.attach(document.querySelector('.v4-app-root')); window.__pnReady = true;` });
    await p.waitForFunction(() => window.__pnReady === true);
  }
  await p.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(r)))));
  return { ctx, p };
}
const g11Of = (p) => p.evaluate(g11Static, { owner: 'fx' });

test('G11 negativ: ohne Kit sind Liste+Detail gestapelt und die Reiter mehrzeilig – rot', async () => {
  const { ctx, p } = await kitFixture({ kit: false });
  try {
    const r = await g11Of(p);
    assert.ok(r.fails.some((f) => /2 Panes zugleich sichtbar/.test(f)), JSON.stringify(r.fails));
    assert.ok(r.fails.some((f) => /Reiter (mehrzeilig|abgeschnitten)/.test(f)), JSON.stringify(r.fails));
  } finally { await ctx.close(); }
});

test('G11 positiv: mit Kit eine Spalte, Reiter einzeilig mit „Mehr“, Tipp öffnet Detail, Zurück stellt Liste und Scrollstand her', async () => {
  const { ctx, p } = await kitFixture();
  try {
    assert.deepEqual((await g11Of(p)).fails, []);
    const r = await runG11(p, 'fx');
    assert.deepEqual(r.G11, [], JSON.stringify(r));
    assert.equal(r.g11.list.scrolled, 60, 'Liste war gescrollt (Zurück muss den Stand beweisen)');
    assert.equal(r.g11.back.scrollTop, 60);
    assert.equal(r.g11.detail.backInZone, false, 'Zurück außerhalb der reservierten Zonen');
  } finally { await ctx.close(); }
});

test('Kit-Reiter: „Mehr ▾“ zeigt die ausgeblendeten Reiter mit Zählern, Auswahl schaltet über den Original-Reiter, aktiver bleibt sichtbar', async () => {
  const { ctx, p } = await kitFixture();
  try {
    // ins Detail, dort stehen die Reiter
    await p.click('.ctox-list-item[data-id="r1"]');
    await p.waitForFunction(() => document.querySelector('.fx-ws').getAttribute('data-v4-md') === 'detail');
    await p.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const hidden = await p.evaluate(() => [...document.querySelectorAll('.ctox-pane-tab[data-v4-tab-overflow]')].map((t) => t.dataset.tab));
    assert.ok(hidden.length >= 2, `ausgeblendet: ${hidden}`);
    assert.ok(!hidden.includes('Übersicht'), 'aktiver Reiter bleibt sichtbar');
    await p.click('[data-v4-tabs-more]');
    const items = await p.evaluate(() => [...document.querySelectorAll('[data-v4-tabs-menu] .v4-menu-item')].map((b) => b.textContent));
    assert.equal(items.length, hidden.length);
    assert.match(items.at(-1), /Dubletten\(8\)/, 'Zähler bleibt am Reiter');
    await p.click('[data-v4-tabs-menu] .v4-menu-item:last-child');
    assert.equal(await p.evaluate(() => window.__tab), 'Dubletten', 'Original-Reiter geklickt (App schaltet um)');
    await p.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const st = await p.evaluate(() => { const t = document.querySelector('[data-tab="Dubletten"]'); return { hidden: t.hasAttribute('data-v4-tab-overflow'), shown: t.getClientRects().length > 0, menu: !!document.querySelector('[data-v4-tabs-menu]') }; });
    assert.deepEqual(st, { hidden: false, shown: true, menu: false }, 'neuer aktiver Reiter sichtbar, Menü zu');
    assert.deepEqual((await g11Of(p)).fails, []);
  } finally { await ctx.close(); }
});

test('Kit breit: ab 600 px Fensterbreite bleibt der Workspace zweispaltig (keine Liste→Detail-Markierung)', async () => {
  const { ctx, p } = await kitFixture({ width: 900 });
  try {
    const st = await p.evaluate(() => ({ md: document.querySelector('.fx-ws').getAttribute('data-v4-md'), vis: [...document.querySelectorAll('.ctox-pane')].filter((x) => x.getClientRects().length).length }));
    assert.deepEqual(st, { md: null, vis: 2 });
  } finally { await ctx.close(); }
});

test('Mutanten am Kit: ohne Zurück-Handler, ohne Scroll-Wiederherstellung, ohne Reiterüberlauf wird G11 rot', async () => {
  const cases = [
    ['zurück', "if (ws) setMode(ws, 'list');", '', /führt nicht zur Liste/],
    ['scroll', 'if (el) el.scrollTop = x.top;', '', /Listenposition nicht wiederhergestellt/],
    ['reiter', 'for (const t of root.querySelectorAll(TABS)) fitTabs(t);', '', /Reiter (mehrzeilig|abgeschnitten)/],
  ];
  for (const [name, from, to, expect] of cases) {
    assert.ok(PANE_NARROW_SRC.includes(from), `Mutationsstelle fehlt (${name}): ${from}`);
    const { ctx, p } = await kitFixture({ kitSrc: PANE_NARROW_SRC.replace(from, to) });
    try {
      let fails = (await g11Of(p)).fails;
      if (name === 'reiter') {
        // Reiter stehen im Detail: dorthin wechseln, dann statisch messen
        await p.click('.ctox-list-item[data-id="r1"]');
        await p.waitForTimeout(400);
        fails = (await g11Of(p)).fails;
      } else fails = (await runG11(p, 'fx')).G11;
      assert.ok(fails.some((f) => expect.test(f)), `${name}: ${JSON.stringify(fails)}`);
    } finally { await ctx.close(); }
  }
});

test('Mutant am Kriterium: ohne Spaltenzählung würde die gestapelte Fixture ohne Kit durchgehen – die Zählung ist tragend', async () => {
  const from = 'if (vis.length > 1) out.fails.push(';
  assert.ok(G11_SRC.includes(from));
  const f = join(MUT_DIR, 'g11-cols.mjs');
  writeFileSync(f, G11_SRC.replace(from, 'if (false) out.fails.push('));
  const m = (await import(pathToFileURL(f).href)).g11Static;
  const { ctx, p } = await kitFixture({ kit: false });
  try {
    const r = await p.evaluate(m, { owner: 'fx' });
    assert.ok(!r.fails.some((x) => /Panes zugleich sichtbar/.test(x)));
    assert.ok((await g11Of(p)).fails.some((x) => /Panes zugleich sichtbar/.test(x)));
  } finally { await ctx.close(); }
});
