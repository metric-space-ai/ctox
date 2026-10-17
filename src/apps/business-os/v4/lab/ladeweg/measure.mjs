// Messskript Ladeweg (Shell v4, Strang P1): Service Worker vs. Blob-URLs, Playwright headless.
// Aufruf: node measure.mjs [chromium,webkit] [sw,blob]
// Schreibt results.json und den Messwerte-Block in RESULTS.md nach JEDER (Browser, Weg)-Messung.
import { readFile, writeFile, readdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startServer } from './server.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const PW = '/Users/michaelwelsch/Documents/ctox/src/apps/business-os/node_modules/playwright/index.mjs';
const pw = await import(PW);
const browsers = (process.argv[2] || 'chromium,webkit').split(',');
const modes = (process.argv[3] || 'sw,blob').split(',');
const REPS = 20;
const BIN = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'woff2', 'woff', 'ttf', 'wasm', 'ico']);

async function walk(dir, base = dir, out = {}) {
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) await walk(full, base, out);
    else {
      const rel = path.relative(base, full).split(path.sep).join('/');
      const ext = rel.split('.').pop();
      out[rel] = BIN.has(ext) ? { b64: (await readFile(full)).toString('base64') } : { text: await readFile(full, 'utf8') };
    }
  }
  return out;
}
const fx = (app, ver) => walk(path.join(here, 'fixtures', app, ver));
/** Frische Version aus v2 ableiten (neue Hashes, neue URLs): VERSION und Titel ersetzen. */
function derive(v2files, ver) {
  const f = structuredClone(v2files);
  f['util.js'].text = f['util.js'].text.replace(`'v2'`, `'${ver}'`);
  f['locales/de.json'].text = f['locales/de.json'].text.replace('Demo v2', `Demo ${ver}`);
  return { app_id: 'demo', app_ver: ver, files: f };
}

// Last-Probe (h): synthetische App in realistischer Groesse (heute: outbound 654 KB / research 569 KB,
// shared 9,8 MB inkl. Vendor). APP_MODS Module je ~25 KB, SHARED_MODS shared-Module je ~50 KB.
const APP_MODS = 40, SHARED_MODS = 30;
function bulk(prefix, kb) {
  const lines = [];
  let i = 0;
  while (lines.join('\n').length < kb * 1024) { lines.push(`export function ${prefix}_${i}(a) { const o = { k${i}: a + ${i}, s: 'x${i}'.repeat(2) }; return o.k${i} * 2 + o.s.length; }`); i++; }
  return lines.join('\n');
}
function heavyShell(base) {
  const files = structuredClone(base);
  for (let i = 0; i < SHARED_MODS; i++) files[`shared/h${i}.js`] = { text: `${bulk(`h${i}`, 50)}\nexport function hsum() { return h${i}_0(1); }\n` };
  return { app_id: 'shell', app_ver: 'vh', files };
}
function heavyApp(ver) {
  const files = { 'util.js': { text: `export const VERSION = '${ver}';\n` } };
  const imps = [];
  for (let i = 0; i < APP_MODS; i++) {
    files[`m${i}.js`] = { text: `import { hsum } from 'ctox:shared/h${i % SHARED_MODS}.js';\nimport { VERSION } from './util.js';\n${bulk(`m${i}`, 25)}\nexport function sum() { return m${i}_0(1) + hsum() + VERSION.length; }\n` };
    imps.push(`import * as m${i} from './m${i}.js';`);
  }
  files['index.js'] = { text: `${imps.join('\n')}\nimport { VERSION } from './util.js';\nimport { greet } from 'ctox:shared/greet.js';\nconst mods = [${Array.from({ length: APP_MODS }, (_, i) => `m${i}`).join(', ')}];\nexport async function mount(root) {\n  const sum = mods.reduce((a, m) => a + m.sum(), 0);\n  root.textContent = greet(String(sum));\n  return { unmount() { root.textContent = ''; }, snapshot() { return {}; }, probe() { return { version: VERSION, sum, greet: root.textContent, note: '' }; } };\n}\n` };
  return { app_id: 'heavy', app_ver: ver, files };
}
const sizeKB = (spec) => Math.round(Object.values(spec.files).reduce((a, f) => a + (f.text ? Buffer.byteLength(f.text) : f.b64.length * 0.75), 0) / 1024);
const pct = (arr, p) => { const s = [...arr].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)]; };
const r1 = (x) => (typeof x === 'number' ? Math.round(x * 10) / 10 : x);

const resultsPath = path.join(here, 'results.json');
const all = existsSync(resultsPath) ? JSON.parse(await readFile(resultsPath, 'utf8')) : {};

async function persist() {
  await writeFile(resultsPath, JSON.stringify(all, null, 2));
  const rows = [];
  for (const [key, run] of Object.entries(all)) {
    const [browser, mode] = key.split('/');
    if (run.fatal) { rows.push(`| alle | ${mode} | ${browser} | NICHT GEMESSEN | ${run.fatal.replace(/\|/g, '/').slice(0, 200)} |`); continue; }
    for (const s of run.scenarios) rows.push(`| ${s.id} | ${mode} | ${browser} | ${s.pass ? 'ok' : 'FEHLER'} | ${s.summary.replace(/\|/g, '/')} |`);
  }
  const block = ['<!-- MESSWERTE:BEGIN (automatisch von measure.mjs, nicht von Hand aendern) -->',
    `Stand: ${new Date().toISOString()} · Rohdaten: \`results.json\``, '',
    '| Szenario | Weg | Browser | Ergebnis | Messwerte / Befund |', '|---|---|---|---|---|', ...rows,
    '<!-- MESSWERTE:END -->'].join('\n');
  const mdPath = path.join(here, 'RESULTS.md');
  let md = existsSync(mdPath) ? await readFile(mdPath, 'utf8') : '# Ladeweg-Messung Shell v4\n\n## Messwerte\n\n<!-- MESSWERTE:BEGIN -->\n<!-- MESSWERTE:END -->\n';
  md = md.replace(/<!-- MESSWERTE:BEGIN[\s\S]*?<!-- MESSWERTE:END -->/, block);
  await writeFile(mdPath, md);
}

async function openLab(context, url, mode) {
  const page = await context.newPage();
  page.on('pageerror', (e) => console.log(`[pageerror] ${e.message}`));
  await page.goto(`${url}/?mode=${mode}`);
  await page.waitForFunction(() => document.documentElement.dataset.labReady === '1');
  return page;
}

async function runOne(browserName, mode) {
  const key = `${browserName}/${mode}`;
  const run = { started: new Date().toISOString(), scenarios: [] };
  all[key] = run;
  const add = async (id, pass, summary, data) => { run.scenarios.push({ id, pass: !!pass, summary, data }); console.log(`${key} ${id} ${pass ? 'ok' : 'FEHLER'} ${summary}`); await persist(); };
  let browser, server;
  try {
    browser = await pw[browserName].launch({ headless: true });
    run.browserVersion = browser.version();
    server = await startServer();
    const context = await browser.newContext({ serviceWorkers: mode === 'blob' ? 'block' : 'allow' });
    const page = await openLab(context, server.url, mode);
    const lab = (fn, ...args) => page.evaluate(fn, ...args);
    const fixtures = { shell1: await fx('shell', 'v1'), shell2: await fx('shell', 'v2'), demo1: await fx('demo', 'v1'), demo2: await fx('demo', 'v2'), dyn: await fx('demo-dyn', 'v1') };
    const install = (spec) => lab((s) => window.lab.install(s), spec);

    // (a) Erstinstallation v1
    {
      const init = await lab(() => window.lab.init());
      const iShell = await install({ app_id: 'shell', app_ver: 'v1', files: fixtures.shell1 });
      const iDemo = await install({ app_id: 'demo', app_ver: 'v1', files: fixtures.demo1 });
      await lab(async () => { await window.lab.setActive('shell', 'v1'); await window.lab.setActive('demo', 'v1'); });
      const m = await lab(() => window.lab.mount('main', 'demo', 'v1'));
      const keep = await lab(() => window.lab.mount('keep', 'demo', 'v1'));
      const p = m.probe;
      const pass = p.version === 'v1' && p.greet === 'Hallo Welt (shell v1)' && p.cssColor === 'rgb(200, 0, 0)' && p.imgWidth === 16 && p.title === 'Demo v1' && /url\(/.test(p.cssBg);
      await add('a Erstinstallation', pass,
        `init (SW-Registrierung+Kontrolle bzw. Loader) ${r1(init.initMs)} ms; IDB-Schreiben shell ${r1(iShell.ms)} ms + demo ${r1(iDemo.ms)} ms; Montage (import+fetch+css+img) ${r1(m.totalMs)} ms (davon import ${r1(m.importMs)} ms); 2. Fenster gleiche Version ${r1(keep.totalMs)} ms; css/img/locale/greet/relativer Import ok=${pass}`,
        { init, iShell: iShell.ms, iDemo: iDemo.ms, mount: m, keep });
    }

    // (b) Aktivierung v2 im selben Dokument + 20 frische Versionen + 20 warme Wechsel
    {
      await install({ app_id: 'demo', app_ver: 'v2', files: fixtures.demo2 });
      await lab(() => window.lab.setNote('main', 'halb ausgefuellt'));
      await lab(() => window.lab.setActive('demo', 'v2'));
      const s = await lab(() => window.lab.swap('main', 'demo', 'v2'));
      const firstOk = s.probe.version === 'v2' && s.probe.note === 'halb ausgefuellt' && s.probe.cssColor === 'rgb(0, 0, 200)' && s.oldModule.version === 'v1';
      const fresh = [];
      const freshInstall = [];
      for (let i = 0; i < REPS; i++) {
        const ver = `v${3 + i}`;
        freshInstall.push((await install(derive(fixtures.demo2, ver))).ms);
        await lab((v) => window.lab.setActive('demo', v), ver);
        const r = await lab((v) => window.lab.swap('main', 'demo', v), ver);
        if (r.probe.version !== ver || r.probe.note !== 'halb ausgefuellt') throw new Error(`swap ${ver} falsch: ${JSON.stringify(r.probe)}`);
        fresh.push(r.ms);
      }
      const keepMod = await lab(() => window.lab.moduleState('keep'));
      const keepProbe = await lab(() => window.lab.probe('keep'));
      const warm = [];
      for (let i = 0; i < REPS; i++) {
        const ver = i % 2 ? 'v2' : 'v1';
        const r = await lab((v) => window.lab.swap('main', 'demo', v), ver);
        if (r.probe.version !== ver) throw new Error(`warm ${ver} falsch`);
        warm.push(r.ms);
      }
      const untouched = keepMod.version === 'v1' && keepMod.mountCount === 2 && keepMod.greet === 'Hallo x (shell v1)' && keepProbe.version === 'v1';
      await add('b Aktivierung v2 (Live-Remount)', firstOk && untouched,
        `erster Wechsel v1->v2 ${r1(s.ms)} ms, Eingabe erhalten=${s.probe.note === 'halb ausgefuellt'}; frische Versionen (neue URLs, ${REPS}x): p50 ${r1(pct(fresh, 50))} ms / p95 ${r1(pct(fresh, 95))} ms / max ${r1(Math.max(...fresh))} ms; warmer Wechsel v1<->v2 (Modul-Map, ${REPS}x): p50 ${r1(pct(warm, 50))} / p95 ${r1(pct(warm, 95))} ms; IDB-Installation je Version p50 ${r1(pct(freshInstall, 50))} ms; altes v1-Modul unberuehrt=${untouched} (mountCount ${keepMod.mountCount}, greet '${keepMod.greet}'); CSS-Farbe des alten v1-Fensters neben v-neu: ${keepProbe.cssColor}`,
        { first: s, fresh, warm, freshInstall, keepMod, keepProbe });
    }

    // (c) Shell-Version-Wechsel
    {
      await install({ app_id: 'shell', app_ver: 'v2', files: fixtures.shell2 });
      await lab(() => window.lab.setActive('shell', 'v2'));
      const sameVer = await lab(() => window.lab.mount('c-same', 'demo', 'v1'));
      await install(derive(fixtures.demo2, 'vc'));
      const newVer = await lab(() => window.lab.mount('c-new', 'demo', 'vc'));
      const pinned = mode === 'sw' ? await lab(() => window.lab.mount('c-pin', 'demo', 'v1', { pinShell: true })) : null;
      const keepMod = await lab(() => window.lab.moduleState('keep'));
      const page2 = await openLab(context, server.url, mode);
      await page2.evaluate(() => window.lab.init());
      const reloaded = await page2.evaluate(() => window.lab.mount('main', 'demo', 'v1'));
      await page2.close();
      const g = (x) => x && x.probe.greet.replace('Hallo Welt ', '');
      const pass = g(newVer) === '(shell v2)' && g(reloaded) === '(shell v2)' && keepMod.greet === 'Hallo x (shell v1)' && (mode === 'blob' ? g(sameVer) === '(shell v2)' : g(pinned) === '(shell v2)');
      await add('c Shell-Wechsel v1->v2', pass,
        `neuer Mount gleiche App-Version (demo@v1, schon im Dokument geladen): ${g(sameVer)}; neuer Mount neue App-Version: ${g(newVer)}${pinned ? `; Vorschlag URL mit ~shell@v2: ${g(pinned)}` : ''}; neues Dokument demo@v1: ${g(reloaded)}; altes Fenster bleibt: ${keepMod.greet.replace('Hallo x ', '')}`,
        { sameVer: g(sameVer), newVer: g(newVer), pinned: g(pinned), reloaded: g(reloaded), keep: keepMod });
    }

    // (e) Manipulierte Datei + unbekannte Version
    {
      // eigener Inhalt fuer util.js, sonst teilt 'bad' den Hash mit demo@v1 (Inhaltsadressierung) und die
      // Manipulation trifft auch v1 – genau das passierte im ersten Lauf, siehe RESULTS.md.
      const badFiles = structuredClone(fixtures.demo1);
      badFiles['util.js'].text += '\n// bad\n';
      const inst = await install({ app_id: 'demo', app_ver: 'bad', files: badFiles });
      await lab((sha) => window.lab.tamper(sha, "globalThis.__pwned = 1; export const VERSION = 'pwned'; export function helper() { return 'x'; }"), inst.manifest.files['util.js']);
      const tm = await lab(() => window.lab.tryMount('bad', 'demo', 'bad'));
      const pwned = await lab(() => globalThis.__pwned === 1);
      const stBad = mode === 'sw' ? await lab(() => window.lab.status('/~a/demo@bad/util.js')) : 'n/a';
      const stUnknown = mode === 'sw' ? await lab(() => window.lab.status('/~a/demo@gibtsnicht/index.js')) : 'n/a';
      const stUnknownFile = mode === 'sw' ? await lab(() => window.lab.status('/~a/demo@v1/fehlt.js')) : 'n/a';
      const pass = !tm.ok && !pwned && (mode === 'blob' || (stBad === 404 && stUnknown === 404 && stUnknownFile === 404));
      await add('e Manipulierte Datei', pass,
        `Montage abgelehnt=${!tm.ok} (${(tm.error || '').slice(0, 90)}); Code ausgefuehrt=${pwned}; Status manipuliert=${stBad}, unbekannte Version=${stUnknown}, unbekannte Datei=${stUnknownFile}`,
        { tm, pwned, stBad, stUnknown, stUnknownFile });
    }

    // (g) Berechnete Pfade (heutiges App-Muster) – Zusatzprobe
    {
      await install({ app_id: 'demo-dyn', app_ver: 'v1', files: fixtures.dyn });
      const r = await lab(() => window.lab.tryMount('dyn', 'demo-dyn', 'v1'));
      const p = r.ok ? r.probe : {};
      await add('g Berechnete Pfade (Template-Literal, import.meta.url als Wert)', r.ok && p.templateLiteral === 'dyn-ok' && p.metaUrlAsValue === 'dyn-ok',
        `new URL(\`./locales/\${lang}.json\`, import.meta.url): ${String(p.templateLiteral).slice(0, 80)}; loadModuleMessages(import.meta.url): ${String(p.metaUrlAsValue).slice(0, 80)}${r.ok ? '' : `; Montage: ${r.error}`}; Loader-Warnungen: ${(await lab(() => window.lab.warnings())).length}`,
        { r });
    }

    // (h) Last: App ~1 MB in 41 Modulen + Shell-shared ~1,5 MB in 30 Modulen, kalt montiert
    {
      const hs = heavyShell(fixtures.shell2);
      const iS = await install(hs);
      await lab(() => window.lab.setActive('shell', 'vh'));
      const times = [];
      let ia = 0;
      for (let i = 0; i < 6; i++) {
        const spec = heavyApp(`h${i}`);
        ia = sizeKB(spec);
        await install(spec);
        const r = await lab((v) => window.lab.mount(`heavy${v}`, 'heavy', v), `h${i}`);
        if (r.probe.version !== `h${i}`) throw new Error('heavy falsch');
        times.push(r.totalMs);
        await lab((v) => window.lab.unmount(`heavy${v}`), `h${i}`);
      }
      const rest = times.slice(1);
      await add('h Last (App 41 Module, Shell 30 shared-Module)', true,
        `App ${ia} KB, Shell ${sizeKB(hs)} KB (IDB-Schreiben ${r1(iS.ms)} ms); erster Mount (Shell+App kalt) ${r1(times[0])} ms; weitere 5 frische App-Versionen (App kalt, Shell warm): p50 ${r1(pct(rest, 50))} ms / max ${r1(Math.max(...rest))} ms`,
        { times, appKB: ia, shellKB: sizeKB(hs) });
    }

    // (f) Zwei Tabs gleichzeitig
    {
      const tab2 = await openLab(context, server.url, mode);
      await tab2.evaluate(() => window.lab.init());
      const cur = await lab(() => window.lab.probe('main'));
      await tab2.evaluate((v) => window.lab.mount('main', 'demo', v), cur.version);
      await tab2.evaluate(() => window.lab.listen('main'));
      const lat = [];
      const both = [];
      for (let i = 0; i < 10; i++) {
        const ver = `vt${i}`;
        await install(derive(fixtures.demo2, ver));
        const sentAt = await lab((v) => window.lab.activateAndAnnounce('demo', v), ver);
        const own = await lab((v) => window.lab.swap('main', 'demo', v), ver);
        await tab2.waitForFunction((n) => window.lab.events.length >= n, i + 1, { timeout: 15000 });
        const ev = await tab2.evaluate((n) => window.lab.events[n], i);
        if (ev.version !== ver || own.probe.version !== ver) throw new Error(`Tab-Wechsel ${ver} falsch`);
        lat.push(ev.doneAt - sentAt);
        both.push(ev.greet === own.probe.greet);
      }
      // gleichzeitiges Montieren derselben neuen Version in beiden Tabs
      await install(derive(fixtures.demo2, 'vpar'));
      const [x, y] = await Promise.all([lab(() => window.lab.mount('par', 'demo', 'vpar')), tab2.evaluate(() => window.lab.mount('par', 'demo', 'vpar'))]);
      await tab2.close();
      const pass = both.every(Boolean) && x.probe.version === 'vpar' && y.probe.version === 'vpar';
      await add('f Zwei Tabs', pass,
        `Tab 2 folgt Aktivierung (BroadcastChannel -> Remount), 10x: p50 ${r1(pct(lat, 50))} ms / p95 ${r1(pct(lat, 95))} ms; gleichzeitiges Montieren neue Version in beiden Tabs: ${r1(x.totalMs)} / ${r1(y.totalMs)} ms, beide ok=${pass}`,
        { lat, both, x: x.totalMs, y: y.totalMs });
    }

    // (d) Server stoppen -> Wechsel nur aus IDB
    {
      const before = server.stats.artifactHits;
      await server.stop();
      const toggles = [];
      for (const v of ['v1', 'v2', 'v1', 'v2']) { const r = await lab((x) => window.lab.swap('main', 'demo', x), v); toggles.push(r.probe.version === v ? r1(r.ms) : 'FEHLER'); }
      await install(derive(fixtures.demo2, 'voff'));
      const off = await lab(() => window.lab.swap('main', 'demo', 'voff'));
      let swRestart = 'n/a';
      if (mode === 'sw' && browserName === 'chromium') {
        const cdp = await context.newCDPSession(page);
        await cdp.send('ServiceWorker.enable');
        await cdp.send('ServiceWorker.stopAllWorkers');
        await install(derive(fixtures.demo2, 'voff2'));
        const r = await lab(() => window.lab.swap('main', 'demo', 'voff2'));
        swRestart = r.probe.version === 'voff2' ? `ok ${r1(r.ms)} ms` : 'FEHLER';
      }
      const pass = toggles.every((t) => t !== 'FEHLER') && off.probe.version === 'voff' && swRestart !== 'FEHLER';
      await add('d Server gestoppt', pass,
        `Wechsel v1/v2/v1/v2: ${toggles.join(' / ')} ms; neue Version nach Stopp installiert+montiert: ${r1(off.ms)} ms; SW gestoppt+Neustart ohne Netz: ${swRestart}; Server-Treffer unter /~a/ waehrend des Laufs: ${before}`,
        { toggles, off: off.ms, swRestart, artifactHits: before, paths: server.stats.artifactPaths.slice(0, 10) });
    }
    run.artifactHitsTotal = server.stats.artifactHits;
    await context.close();
  } catch (e) {
    console.log(`${key} FATAL ${e.stack || e}`);
    if (!run.scenarios.length) run.fatal = String(e.message || e);
    else run.scenarios.push({ id: 'Abbruch', pass: false, summary: String(e.message || e).slice(0, 300) });
    await persist();
  } finally {
    try { await server?.stop(); } catch {}
    await browser?.close();
  }
}

for (const b of browsers) for (const m of modes) await runOne(b, m);
await persist();
