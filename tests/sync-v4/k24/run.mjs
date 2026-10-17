// Shell v4 / Strang K24PREP – Abnahmewerkzeug für K24 („fremde App ohne Core-Wissen“, negativ-abnahme.md K24).
//
//   node src/apps/business-os/v4/k24/run.mjs --target bind
//   node src/apps/business-os/v4/k24/run.mjs --target url=<shell-url> --user2-url=<url-des-nicht-admin> \
//        [--core-fingerprint-cmd "<befehl, dessen Ausgabe Core-Binary + Konfiguration fingerprintet>"]
//   Optionen: --trigger-wait-ms N (bind 65000, url 150000), --agent-wait-ms N (bind 10000, url 600000), --headed
//
// Prüft die Kriterien (1)–(7) aus K24 mit der Test-App app/v1/k24-probe (danach app/v2/k24-probe) in einem Lauf:
// Upload als Ordner über die Upload-Fläche des Quelltext-Modus der Shell (v4/source/upload.js aus dem Shell-Artefakt,
// echter <input webkitdirectory>), zwei Tabs Admin + ein zweiter Nutzer, Befehl, Trigger, v2 mit Migration,
// Entfernen; dazu Wächter check-caps.mjs --no-baseline und check-isolation.mjs für die App.
//
// Bewertung je Teilprüfung: PASS = gemessen und erfüllt; FAIL = gemessen und nicht erfüllt; OFFEN(Grund) = auf
// diesem Ziel nicht leistbar, weil der Kern (K-1) die Fähigkeit nachweislich nicht hat (Beleg im Grund). OFFEN wird
// NUR für --target bind und nur mit einer benannten Lücke vergeben; auf --target url wird dieselbe Messung FAIL.
// Nichts wird simuliert, was das Kriterium entscheidet. Kriterium = schlechteste Teilprüfung (FAIL > OFFEN > PASS).
//
// Ausgaben: v4/k24/out/results.json, Block in v4/k24/RESULTS.md, Bilder v4/k24/shots/*.png. Exit 1 bei FAIL.
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join, dirname, relative } from 'node:path';
import { mkdir, writeFile, readFile, readdir, rm, cp, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync, execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const HERE = dirname(fileURLToPath(import.meta.url));
// tests/sync-v4/k24 → repository root; the shell under test is this checkout's src/apps/business-os.
const REPO = join(HERE, '../../..');
const BOS = join(REPO, 'src/apps/business-os');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(BOS, 'node_modules/playwright'));
const OUT = join(HERE, 'out');
const SHOTS = join(HERE, 'shots');
const APP_ID = 'k24-probe';
const COLL = 'k24_ticks';
const APP_V1 = join(HERE, 'app/v1', APP_ID);
const APP_V2 = join(HERE, 'app/v2', APP_ID);
// Guards run from the same checkout by default (read-only); K24_GUARD_BOS can point elsewhere.
const GUARD_BOS = process.env.K24_GUARD_BOS || BOS;
const LIMIT_MS = 2000;

// ------------------------------------------------------------------ Argumente
const argv = process.argv.slice(2);
const opt = (name) => { const a = argv.find((x) => x === `--${name}` || x.startsWith(`--${name}=`)); if (!a) return null; if (a.includes('=')) return a.slice(a.indexOf('=') + 1); const i = argv.indexOf(a); return argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[i + 1] : true; };
const targetArg = String(opt('target') || 'bind');
const TARGET = targetArg === 'bind' ? { kind: 'bind' } : targetArg.startsWith('url=') ? { kind: 'url', url: targetArg.slice(4) } : null;
if (!TARGET) { console.error('--target bind | --target url=<…>'); process.exit(2); }
const BIND = TARGET.kind === 'bind';
const TRIGGER_WAIT_MS = Number(opt('trigger-wait-ms') || (BIND ? 65000 : 150000));
const CORE_FP_CMD = opt('core-fingerprint-cmd');
const AGENT_WAIT_MS = Number(opt('agent-wait-ms') || (BIND ? 10000 : 600000));

// ------------------------------------------------------------------ Lücken von K-1 (nur für --target bind)
// Jede Lücke nennt den Beleg im Code; der Lauf prüft den Beleg zusätzlich statisch (gapEvidence).
const GAPS = {
  handler: 'K-1/K-1b hat keine App-Laufzeit: kernel/fake-core.mjs runCommand ruft nur per sim.onCommand registrierte Testfunktionen und setzt sonst result {ok:true}; Handler-Dateien aus dem Manifest werden nie ausgeführt → K-2/S8',
  trigger: 'K-1/K-1b hat keinen Trigger-Lauf: triggers wird nur als Manifestfeld geerbt (kernel.mjs „Befund B4“), es gibt keinen Aufrufer (kein runTrigger/trigger_id in kernel/*.mjs) → K-2/S8',
  user2: 'K-1-Fake-Core lebt im Leiter-Tab je (Instanz, Nutzer) (MemoryCoreTransport namespace = dbName): ein zweiter Nutzer hat einen eigenen leeren Core; v4/boot/boot.js setzt zudem jede Identität als role admin → echter Nicht-Admin erst mit K-2/S8',
  workjet: 'Workjet Ops ist kein Client des Bind-Aufbaus → S8 auf welsch',
  migration: 'Kernel (K-1) publishDraft → publishInternal ohne incompatible/migrate: keine neue Generation, migrations aus collections.schema.json werden nicht ausgeführt → K-2/S8',
  remove: 'Weder die K-1-Shell-API (kernel.shell) noch die Quelltext-API kennen Entfernen/Archivieren einer App → K-2/S8',
  archive: 'Archiv einer entfernten App gibt es in K-1 nicht (kein remove/archive in kernel.mjs) → K-2/S8',
  peer: 'Bind-Aufbau hat keinen nativen Peer (MemoryCoreTransport im Tab); Peer-Respawn erst auf welsch messbar → S8',
  core: 'Bind-Aufbau hat keinen laufenden Rust-Core; Binary-/Konfigurations-Fingerprint erst auf welsch (--core-fingerprint-cmd) → S8',
  inject: 'Fehler-Injektion (sim.failCommand) gibt es nur im Fake-Core',
  frameEntry: 'Rahmen ohne Einstieg „Neue App“: Strang FR3 baut ihn (Launcher → „Neue App“, nur Admins → Quelltext-Modus mit neuem Entwurf); bis FR3 im Prototyp-Klon gelandet ist, hängt das Werkzeug die Upload-Fläche der Shell selbst ein',
  caps: 'Bind-Boot (v4/boot/boot.js) übergibt createLoader keine capabilities; die Aktivierung prüft capabilities_required (audit.event, agent.task.enqueue) gegen diese leere Liste → needs_update. Der Fake-Core bietet kein ctx.cap → K-2/S8',
  skills: 'Kernel übernimmt das Manifestfeld skills nicht (K-1: publishInternal ohne setManifest) → K-2/S8',
  agent: 'K-1 hat weder agent.task.enqueue noch Harness noch Rückruf-Befehle (§16.30 Nr. 3); ohne App-Laufzeit startet kein Agentenlauf → K-2/S8',
};

// ------------------------------------------------------------------ Ergebnis
const results = { target: TARGET, started: new Date().toISOString(), criteria: {}, subs: [], measures: {}, evidence: {}, files: {} };
const CRIT = {
  V: 'Vorbedingung: Upload als Ordner über den Quelltext-Modus (Admin), Collections mit Index + Pflichtfeld',
  1: '(1) null Änderung an Rust, Core-Konfiguration oder Shell-Artefakt',
  2: '(2) kein Neustart, kein Peer-Respawn, kein Reload (je Tab genau eine Navigation)',
  3: '(3) App erscheint in allen Clients (Tab 2, zweiter Nutzer Nicht-Admin, Workjet Ops) ≤ 2 s',
  4: '(4) Befehl läuft durch den Handler, Ergebnis live in allen Clients',
  5: '(5) Trigger feuert',
  6: '(6) inkompatible Schemaänderung mit Migration → alle Clients remounten ohne Eingabeverlust',
  7: '(7) App entfernen → Archiv, verschwindet überall',
  8: '(8) App-Skill + Prompt-Template: Knopf → Befehl → Handler → agent.task.enqueue(skill) → Rückruf genau einmal, live in allen Clients; Skill-Änderung im Quelltext-Modus wirkt ohne Neustart',
  W: 'Wächter-Gegenprobe: check-caps.mjs --no-baseline und check-isolation.mjs ohne Treffer',
};
function sub(crit, name, measured, detail, gap = null) {
  let status;
  if (measured === true) status = 'PASS';
  else if (gap && BIND && GAPS[gap]) status = 'OFFEN';
  else status = 'FAIL';
  const row = { crit: String(crit), name, status, detail, ...(status === 'OFFEN' ? { reason: GAPS[gap], gap } : {}), ...(measured === null ? { measured: false } : {}) };
  results.subs.push(row);
  console.log(`${status.padEnd(5)} (${crit}) ${name}${detail !== undefined ? ` – ${typeof detail === 'string' ? detail : JSON.stringify(detail)}` : ''}${status === 'OFFEN' ? `\n        OFFEN: ${GAPS[gap]}` : ''}`);
  return status;
}
const rank = { PASS: 0, OFFEN: 1, FAIL: 2 };
function finalize() {
  if (DIAG) for (const k of ['3', '4', '5', '6', '7', '8']) sub(k, 'gilt für die K24-App selbst (mit capabilities_required)? Messungen dieses Kriteriums liefen auf der Diagnose-Version', null, 'K24-App aktiviert auf bind nicht', 'caps');
  results.diag = DIAG;
  for (const k of Object.keys(CRIT)) {
    const rows = results.subs.filter((s) => s.crit === k);
    const status = rows.length ? rows.reduce((a, s) => (rank[s.status] > rank[a] ? s.status : a), 'PASS') : 'FAIL';
    results.criteria[k] = { title: CRIT[k], status, subs: rows.length, reasons: [...new Set(rows.filter((s) => s.status !== 'PASS').map((s) => (s.status === 'OFFEN' ? `OFFEN: ${s.reason}` : `FAIL: ${s.name}`)))] };
  }
}
const save = async () => { await mkdir(OUT, { recursive: true }); await writeFile(join(OUT, 'results.json'), `${JSON.stringify(results, null, 2)}\n`); };

// ------------------------------------------------------------------ Hilfen (Node)
async function walk(dir, skip = () => false) {
  const out = [];
  let ents = [];
  try { ents = await readdir(dir, { withFileTypes: true }); } catch { return out; }
  for (const e of ents) {
    const p = join(dir, e.name);
    if (skip(p)) continue;
    if (e.isDirectory()) out.push(...await walk(p, skip)); else if (e.isFile()) out.push(p);
  }
  return out.sort();
}
async function treeHash(dirs, skip) {
  const h = createHash('sha256');
  let n = 0;
  for (const d of dirs) for (const f of await walk(d, skip)) { h.update(relative(REPO, f)); h.update(await readFile(f)); n++; }
  return { sha256: h.digest('hex'), files: n };
}
const sh = (cmd, args, o = {}) => execFileSync(cmd, args, { encoding: 'utf8', maxBuffer: 64 << 20, ...o });
function gitState() {
  // Nur lesend: Arbeitsbaum-Zustand außerhalb v4/k24 und Diff-Hash aller *.rs/Cargo/Konfiguration gegen HEAD.
  const status = sh('git', ['-C', REPO, 'status', '--porcelain', '--untracked-files=all']).split('\n').filter((l) => l && !l.includes('src/apps/business-os/v4/k24/') && !l.includes('/tests/') && !/\.(md|png|jpg)$/.test(l));
  const rust = createHash('sha256').update(sh('git', ['-C', REPO, 'diff', 'HEAD', '--', '*.rs', '*Cargo.toml', '*Cargo.lock', '*.toml'])).digest('hex');
  return { head: sh('git', ['-C', REPO, 'rev-parse', 'HEAD']).trim(), statusOutsideK24: status, rustDiffSha256: rust };
}
async function exists(p) { try { await stat(p); return true; } catch { return false; } }
async function readJson(p) { return JSON.parse(await readFile(p, 'utf8')); }

// ------------------------------------------------------------------ Hilfen (Browser)
function instrument(page, label) {
  const log = { label, errors: [], console: [], navigations: 0 };
  page.on('pageerror', (e) => log.errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') log.console.push(`${m.type()}: ${m.text().slice(0, 300)}`); });
  page.on('framenavigated', (f) => { if (f === page.mainFrame()) log.navigations++; });
  return log;
}
const READY = () => !!document.querySelector('.v4f[data-v4-ready]') && !!globalThis.__ctoxShellV4?.bridge && document.querySelectorAll('.v4f [data-v4-app]').length > 0;
async function bootPage(ctx, url, label) {
  const page = await ctx.newPage();
  const log = instrument(page, label);
  await page.goto(url);
  await page.waitForFunction(READY, null, { timeout: 45000 });
  // Marker gegen Reload: verschwindet bei jeder Navigation.
  const marker = await page.evaluate(() => (window.__k24Marker = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`));
  return { page, log, marker, label };
}
const tileSel = `.v4f-launcher [data-v4-app="${APP_ID}"]`;
async function untilMs(page, fn, arg, timeout) {
  const t0 = Date.now();
  try { await page.waitForFunction(fn, arg, { timeout, polling: 'raf' }); return Date.now() - t0; } catch { return null; }
}
async function openApp(page, appId) {
  if (await page.evaluate(() => document.querySelectorAll('.v4f [data-window-id]').length)) await page.click('[data-v4-home]');
  await page.waitForTimeout(250);
  await page.click(`[data-v4-app="${appId}"] [data-v4-open]`);
  await page.waitForFunction((a) => { const w = [...document.querySelectorAll('.v4f [data-window-id]')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === a); return w && ['mounted', 'failed', 'rolled-back'].includes(w.dataset.hostState); }, appId, { timeout: 15000 });
  await page.waitForTimeout(500);
}
const WIN = (a) => [...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === a);
async function winInfo(page) {
  return page.evaluate((a) => {
    const w = [...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === a);
    const r = w?.querySelector('.v4-app-root');
    return w ? { state: w.dataset.hostState, ver: r?.dataset.appVer ?? null, schema: r?.dataset.k24Schema ?? null, input: r?.querySelector('[data-k24-input]')?.value ?? null, ticks: [...(r?.querySelectorAll('[data-k24-tick]') ?? [])].map((li) => ({ id: li.dataset.k24Tick, source: li.dataset.k24Source, weight: li.dataset.k24Weight ?? null, text: li.textContent })) } : null;
  }, APP_ID);
}
const activeVer = (page) => page.evaluate((a) => globalThis.__ctoxShellV4.bridge.apps.get().find((x) => x.id === a)?.active_ver ?? null, APP_ID);

// Upload-Fläche der Shell (v4/source/upload.js aus dem laufenden Shell-Artefakt) – der Rahmen hat keinen eigenen
// Einstieg für „neue App“ (Befund V-Einstieg). Das Werkzeug hängt die Fläche nur ein; Ordnerwahl, Entwurf und
// Veröffentlichen laufen über den Shell-Code (publishUpload → kernel.source(app).draft.* → publish).
async function mountUploadSurface(page) {
  return page.evaluate(async () => {
    document.querySelector('[data-k24-harness]')?.remove();
    const sv = document.querySelector('.v4f[data-v4-ready]').dataset.shellVer;
    const mod = await import(`/~a/shell@${sv}/v4/source/upload.js`);
    const bridge = globalThis.__ctoxShellV4.bridge;
    const host = document.createElement('div');
    host.dataset.k24Harness = 'upload';
    host.style.cssText = 'position:fixed;right:12px;bottom:56px;width:380px;max-height:300px;overflow:auto;z-index:2147483000;background:#fff;border:1px solid #888;padding:6px';
    document.body.append(host);
    window.__k24UploadZone = [];
    const zone = { report: (i) => window.__k24UploadZone.push(i), clear: () => {} };
    const actor = bridge.identity?.actor ?? bridge.appApi('shell', sv).actor; // K-1: identity.actor, K-1b: App-API der Shell
    window.__k24Upload = mod.openAppUpload({ root: host, kernel: bridge, api: { actor }, zone });
    return { shellVer: sv, module: `/~a/shell@${sv}/v4/source/upload.js` };
  });
}
async function uploadFolder(page, dir) {
  await page.setInputFiles('[data-k24-harness] input[type=file]', dir);
  await page.waitForFunction(() => { const i = document.querySelector('[data-k24-harness] [data-app-id]'); const b = document.querySelector('[data-k24-harness] [data-publish]'); return i && !i.hidden && i.value && b && !b.disabled; }, null, { timeout: 10000 });
  return page.evaluate(() => ({ appId: document.querySelector('[data-k24-harness] [data-app-id]').value, files: [...document.querySelectorAll('[data-k24-harness] [data-files] li')].map((l) => l.textContent) }));
}
async function clickPublish(page) {
  const t = Date.now();
  await page.click('[data-k24-harness] [data-publish]');
  return t;
}

// ------------------------------------------------------------------ Lauf
await mkdir(SHOTS, { recursive: true });
await mkdir(OUT, { recursive: true });
let server = null;
let browser = null;
const pages = [];
let exitCode = 0;
let DIAG = false; // bind: K24-App aktiviert nicht (capabilities), Messkette mit Diagnose-Version
try {
  // ---- statische Belege der K-1-Lücken (bind) ----
  if (BIND) {
    const kernelSrc = (await Promise.all((await walk(join(BOS, 'kernel'))).filter((f) => f.endsWith('.mjs')).map((f) => readFile(f, 'utf8')))).join('\n');
    results.evidence.k1 = {
      triggerRunner: /runTrigger|trigger_id|runTriggers/.test(kernelSrc),
      handlerRuntime: /handlers\.get\(doc\.type\)/.test(kernelSrc) && !/import\(.*handlers\//.test(kernelSrc),
      removeApi: /uninstall|removeApp|archiveApp/.test(kernelSrc),
      publishDraftMigrations: /publishDraft[\s\S]{0,900}incompatible/.test(kernelSrc),
      bootActorFixedAdmin: /role: 'admin'/.test(await readFile(join(BOS, 'v4/boot.js'), 'utf8')),
    };
  }
  const frameSrc = (await Promise.all((await walk(join(BOS, 'v4/frame'), (p) => p.includes('/lab'))).concat(await walk(join(BOS, 'v4/shell'))).filter((f) => f.endsWith('.js')).map((f) => readFile(f, 'utf8')))).join('\n');
  results.evidence.frameUploadEntry = /openAppUpload|publishUpload|source\/upload\.js/.test(frameSrc);

  // ---- (1) Ausgangszustand ----
  const SHELL_SRC = ['kernel', 'v4/frame', 'v4/shell', 'v4/source', 'shared'].map((d) => join(BOS, d));
  // Nur Code/Konfiguration: Tests, Bilder und Berichte anderer Stränge im selben Klon (z. B. bind.test.mjs-Läufe) zählen nicht.
  const skipVolatile = (p) => p.includes('/tests/') || p.includes('/node_modules') || p.includes('/lab/') || /\.(md|png|jpg)$/.test(p);
  // Stand des Bind-Klons (andere Stränge arbeiten darin): HEAD + ungesicherte Pfade außerhalb v4/k24.
  results.tree = { head: sh('git', ['-C', REPO, 'rev-parse', '--short', 'HEAD']).trim(), dirty: sh('git', ['-C', REPO, 'status', '--porcelain']).split('\n').filter((l) => l && !l.includes('v4/k24/') && !l.includes('/tests/shots/') && !l.includes('/tests/out/')) };
  const before = { files: await treeHash(SHELL_SRC, skipVolatile), git: gitState(), core: CORE_FP_CMD && CORE_FP_CMD !== true ? execSync(String(CORE_FP_CMD), { encoding: 'utf8' }) : null };

  // ---- Ziel starten ----
  let urlAdmin, urlUser2;
  if (BIND) {
    const { startServer } = await import('../boot/serve.mjs');
    server = await startServer();
    urlAdmin = server.url;
    urlUser2 = `${server.url}?user=k24user2`;
    results.measures.seedBuildMs = server.seedBuildMs;
  } else {
    urlAdmin = TARGET.url;
    urlUser2 = opt('user2-url') || null;
  }
  browser = await chromium.launch({ headless: !opt('headed') });
  const ctxAdmin = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const ctxUser2 = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const A = await bootPage(ctxAdmin, urlAdmin, 'A'); pages.push(A);
  const B = await bootPage(ctxAdmin, urlAdmin, 'B'); pages.push(B);
  const U2 = urlUser2 ? await bootPage(ctxUser2, urlUser2, 'U2') : null; if (U2) pages.push(U2);
  const shellFingerprint = (page) => page.evaluate(async () => {
    const b = globalThis.__ctoxShellV4.bridge;
    const ver = document.querySelector('.v4f[data-v4-ready]').dataset.shellVer;
    const m = await b.artifacts.manifest('shell', ver);
    return { ver, files: m?.files ? Object.keys(m.files).length : null, hash: m?.files ? JSON.stringify(Object.entries(m.files).sort()) : null, apps: b.apps.get().map((a) => `${a.id}@${a.active_ver}`) };
  });
  const shellBefore = await shellFingerprint(A.page);
  const actorInfo = async (P) => P.page.evaluate(() => { const b = globalThis.__ctoxShellV4.bridge; const a = b.identity?.actor ?? b.appApi('shell', document.querySelector('.v4f[data-v4-ready]').dataset.shellVer).actor; return { id: a.id, role: a.role, permissions: a.permissions.get() }; });
  results.measures.actors = { A: await actorInfo(A), B: await actorInfo(B), U2: U2 ? await actorInfo(U2) : null };
  results.measures.appsBefore = shellBefore.apps;
  await A.page.screenshot({ path: join(SHOTS, '0-launcher-before-A.png') });

  // =========================================================== V: Upload v1 über den Quelltext-Modus
  sub('V', 'Rahmen bietet einen Einstieg „neue App hochladen“ (Quelltext-Modus)', results.evidence.frameUploadEntry,
    results.evidence.frameUploadEntry ? 'gefunden' : 'v4/frame + v4/shell rufen openAppUpload/upload.js nirgends auf; openSource() verlangt ein offenes Fenster der App – für eine neue App gibt es keins. Werkzeug hängt die Upload-Fläche der Shell selbst ein.', 'frameEntry');
  const surf = await mountUploadSurface(A.page);
  const picked = await uploadFolder(A.page, APP_V1);
  sub('V', 'Ordner gewählt (webkitdirectory), App-ID aus manifest.json', picked.appId === APP_ID && picked.files.includes('handlers/k24.tick.add.js'), { ...surf, ...picked });
  await A.page.screenshot({ path: join(SHOTS, '1-upload-v1-picked-A.png') });
  const wait3 = (P) => untilMs(P.page, (sel) => !!document.querySelector(sel), tileSel, 5000);
  const tClick = await clickPublish(A.page);
  let [tA, tB, tU2] = await Promise.all([wait3(A), wait3(B), U2 ? wait3(U2) : Promise.resolve(null)]);
  const sinceClick = (t) => (t == null ? null : t);
  results.measures.v1 = { appearMs: { A: sinceClick(tA), B: sinceClick(tB), U2: sinceClick(tU2) }, clickAt: tClick };
  const pub1 = await A.page.waitForFunction(() => document.querySelector('[data-k24-harness] [data-v4-source-upload]')?.dataset.lastPublished || (window.__k24UploadZone.length ? `ZONE ${JSON.stringify(window.__k24UploadZone)}` : null), null, { timeout: 10000 }).then((h) => h.jsonValue()).catch(() => null);
  let ver1 = await activeVer(A.page);
  results.measures.v1.published = pub1;
  results.measures.v1.activeVer = ver1;
  sub('V', 'Veröffentlichen über draft.publish({base_app_ver:null}) angenommen', typeof pub1 === 'string' && pub1.startsWith(`${APP_ID}@`), pub1);
  const appRec = await A.page.evaluate((a) => globalThis.__ctoxShellV4.bridge.apps.get().find((x) => x.id === a) ?? null, APP_ID);
  results.measures.v1.activation = appRec;
  const verOrig = ver1;
  const capsBlocked = appRec?.state === 'failed' && /^needs_update:/.test(appRec?.error ?? '');
  sub('V', 'App aktiviert (activation state = activated)', appRec?.state === 'activated' && appRec?.activated_ver === ver1, appRec, capsBlocked ? 'caps' : null);
  // Diagnose (nur bind): Aktivierung scheitert an capabilities_required, weil der Bind-Boot dem Kernel keine Fähigkeiten
  // meldet. Damit der Rest der Kette trotzdem gemessen wird, veröffentlicht das Werkzeug DIESELBE App als nächste Version
  // ohne capabilities_required („Diagnose-Version“, in jeder Teilprüfung so benannt). Die K24-App selbst bleibt OFFEN.
  if (BIND && capsBlocked) {
    DIAG = true;
    const dir = join(OUT, 'upload/diag-v1', APP_ID);
    await rm(dirname(dir), { recursive: true, force: true });
    await cp(APP_V1, dir, { recursive: true });
    const m = await readJson(join(dir, 'manifest.json'));
    m.capabilities_required = [];
    await writeFile(join(dir, 'manifest.json'), `${JSON.stringify(m, null, 2)}\n`);
    await uploadFolder(A.page, dir);
    const tDiag = await clickPublish(A.page);
    const winTile = (P) => untilMs(P.page, (sel) => !!document.querySelector(sel), tileSel, 5000);
    [tA, tB, tU2] = await Promise.all([winTile(A), winTile(B), U2 ? winTile(U2) : Promise.resolve(null)]);
    await A.page.waitForFunction((v) => globalThis.__ctoxShellV4.bridge.apps.get().find((x) => x.id === 'k24-probe')?.active_ver !== v, ver1, { timeout: 10000 }).catch(() => {});
    ver1 = await activeVer(A.page);
    const rec = await A.page.evaluate((a) => globalThis.__ctoxShellV4.bridge.apps.get().find((x) => x.id === a) ?? null, APP_ID);
    results.measures.v1.diag = { clickAt: tDiag, appearMs: { A: tA, B: tB, U2: tU2 }, activation: rec };
    sub('V', 'Diagnose-Version (ohne capabilities_required) aktiviert', rec?.state === 'activated' && rec?.activated_ver === ver1, rec);
  }
  const D = DIAG ? ' [Diagnose-Version]' : '';
  const man1 = await A.page.evaluate(async ({ a, v }) => globalThis.__ctoxShellV4.bridge.artifacts.manifest(a, v), { a: APP_ID, v: ver1 });
  const c1 = man1?.collections?.[COLL];
  results.measures.v1.manifest = { files: Object.keys(man1?.files ?? {}), collections: man1?.collections, handlers: man1?.handlers, capabilities_required: man1?.capabilities_required, generations: man1?.generations };
  sub('V', `Collection ${COLL} mit Index und Pflichtfeld im veröffentlichten Manifest`, !!c1 && (c1.indexes ?? c1.schema?.indexes)?.length > 0 && (c1.schema?.required ?? []).includes('label'), { indexes: c1?.indexes ?? c1?.schema?.indexes, required: c1?.schema?.required });
  const strictReq = await A.page.evaluate(async ({ a, v, c }) => {
    const h = globalThis.__ctoxShellV4.bridge.appApi(a, v).put(c, { id: 'k24-ohne-label', source: 'ui', created_at_ms: 1 });
    try { await h.local; return { rejected: false }; } catch (e) { return { rejected: true, code: e?.code, message: e?.message }; }
  }, { a: APP_ID, v: ver1, c: COLL });
  sub('V', 'Pflichtfeld wird erzwungen (put ohne label → invalid)', strictReq.rejected && strictReq.code === 'invalid', strictReq);
  // Handler/Trigger/Capability: Kern übernimmt sie beim Veröffentlichen ins Manifest? (Version der K24-App selbst)
  const manOrig = verOrig === ver1 ? man1 : await A.page.evaluate(async ({ a, v }) => globalThis.__ctoxShellV4.bridge.artifacts.manifest(a, v), { a: APP_ID, v: verOrig });
  sub('V', 'Handler, Trigger und capabilities_required vom Kern aus manifest.json übernommen', !!manOrig?.handlers?.['k24.tick.add'] && (manOrig?.capabilities_required ?? []).includes('audit.event') && Array.isArray(manOrig?.triggers) && manOrig.triggers.length === 1,
    { ver: verOrig, handlers: manOrig?.handlers, triggers: manOrig?.triggers ?? null, capabilities_required: manOrig?.capabilities_required }, 'handler');

  // =========================================================== (3) sichtbar in allen Clients ≤ 2 s
  sub(3, `Tab A (Admin, Uploader): Kachel ≤ 2 s${D}`, tA != null && tA <= LIMIT_MS, tA);
  sub(3, `Tab B (Admin, zweiter Tab): Kachel ≤ 2 s${D}`, tB != null && tB <= LIMIT_MS, tB);
  sub(3, 'zweiter Nutzer ohne Admin-Rechte: Kachel ≤ 2 s', U2 ? (tU2 != null && tU2 <= LIMIT_MS && results.measures.actors.U2?.role !== 'admin') : false,
    U2 ? { appearMs: tU2, actor: results.measures.actors.U2 } : 'kein --user2-url', 'user2');
  sub(3, 'Workjet Ops: App sichtbar ≤ 2 s', null, 'nicht Teil dieses Ziels', 'workjet');
  await Promise.all([A, B, U2].filter(Boolean).map((P) => P.page.screenshot({ path: join(SHOTS, `2-after-upload-v1-${P.label}.png`) })));

  // =========================================================== (4) Befehl → Handler → live in allen Clients
  await A.page.evaluate(() => document.querySelector('[data-k24-harness]')?.style.setProperty('display', 'none'));
  await openApp(A.page, APP_ID);
  await openApp(B.page, APP_ID);
  const mountA = await winInfo(A.page); const mountB = await winInfo(B.page);
  sub('V', `App in Tab A und B montiert (mount → {unmount, snapshot})${D}`, mountA?.state === 'mounted' && mountB?.state === 'mounted', { A: mountA?.state, B: mountB?.state, ver: mountA?.ver });
  const LABEL_CMD = 'K24 Befehl A';
  await A.page.fill(`.v4f [data-app="${APP_ID}"] [data-k24-input]`, LABEL_CMD);
  const tCmd = Date.now();
  await A.page.click(`.v4f [data-app="${APP_ID}"] [data-k24-command]`);
  const tickVisible = (P, src, label, timeout) => untilMs(P.page, ({ a, src, label }) => {
    const w = [...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === a);
    return [...(w?.querySelectorAll(`[data-k24-source="${src}"]`) ?? [])].some((li) => !label || li.textContent.includes(label));
  }, { a: APP_ID, src, label }, timeout);
  const [hA, hB] = await Promise.all([tickVisible(A, 'command', LABEL_CMD, 5000), tickVisible(B, 'command', LABEL_CMD, 5000)]);
  const cmdDoc = async (P) => P.page.evaluate(async ({ a, v }) => {
    const rows = globalThis.__ctoxShellV4.bridge.appApi(a, v).query('business_commands', { selector: { app_id: a, type: 'k24.tick.add' } }).get();
    return rows.map((r) => ({ id: r.id, status: r.status, result: r.result ?? null, error: r.error ?? null }));
  }, { a: APP_ID, v: ver1 });
  const cmdB = await untilMs(B.page, ({ a, v }) => globalThis.__ctoxShellV4.bridge.appApi(a, v).query('business_commands', { selector: { app_id: a, type: 'k24.tick.add', status: 'done' } }).get().length > 0, { a: APP_ID, v: ver1 }, 5000);
  results.measures.command = { sentAt: tCmd, handlerTickMs: { A: hA, B: hB }, commandDocDoneInB_ms: cmdB, docsA: await cmdDoc(A), docsB: await cmdDoc(B) };
  sub(4, 'Befehlsdokument k24.tick.add erreicht „done“ und ist in Tab B sichtbar ≤ 2 s', cmdB != null && cmdB <= LIMIT_MS, { ms: cmdB, docs: results.measures.command.docsB });
  sub(4, 'Handler hat geschrieben (ctx.put): Tick source=command in Tab A ≤ 2 s', hA != null && hA <= LIMIT_MS, { ms: hA, result: results.measures.command.docsA[0]?.result }, 'handler');
  sub(4, 'Handler-Ergebnis live in Tab B ≤ 2 s', hB != null && hB <= LIMIT_MS, hB, 'handler');
  sub(4, 'Handler-Ergebnis beim zweiten Nutzer', null, 'App beim zweiten Nutzer nicht vorhanden (siehe 3)', 'user2');
  sub(4, 'audit.event-Primitiv im Handler ausgeführt', null, 'ohne App-Laufzeit kein ctx.cap', 'handler');
  // Gegenstück ohne Handler: Datenweg App → Kern → alle Tabs (api.put), damit (4) bei S8 nur noch am Handler hängt.
  const LABEL_PUT = 'K24 Direkt A';
  await A.page.fill(`.v4f [data-app="${APP_ID}"] [data-k24-input]`, LABEL_PUT);
  await A.page.click(`.v4f [data-app="${APP_ID}"] [data-k24-put]`);
  const [pA, pB] = await Promise.all([tickVisible(A, 'ui', LABEL_PUT, 5000), tickVisible(B, 'ui', LABEL_PUT, 5000)]);
  const LABEL_PUTB = 'K24 Direkt B';
  await B.page.fill(`.v4f [data-app="${APP_ID}"] [data-k24-input]`, LABEL_PUTB);
  await B.page.click(`.v4f [data-app="${APP_ID}"] [data-k24-put]`);
  const [qA, qB] = await Promise.all([tickVisible(A, 'ui', LABEL_PUTB, 5000), tickVisible(B, 'ui', LABEL_PUTB, 5000)]);
  results.measures.put = { fromA: { A: pA, B: pB }, fromB: { A: qA, B: qB } };
  sub(4, 'Gegenprobe Datenweg: api.put aus Tab A live in A und B ≤ 2 s (ohne Handler)', pA != null && pB != null && Math.max(pA, pB) <= LIMIT_MS, { A: pA, B: pB });
  sub(4, 'Gegenprobe Datenweg: api.put aus Tab B (Leser) live in A ≤ 2 s', qA != null && qA <= LIMIT_MS, { A: qA, B: qB });
  // api.status.report-Weg der App (Befehl scheitert → genau ein Zoneneintrag der App).
  if (BIND) {
    await A.page.evaluate(() => window.__boot.kernel.sim.failCommand('k24.tick.add', 'K24-Gegenprobe'));
    await A.page.fill(`.v4f [data-app="${APP_ID}"] [data-k24-input]`, 'K24 Fehlerprobe');
    await A.page.click(`.v4f [data-app="${APP_ID}"] [data-k24-command]`);
    // App-Einträge gehen über api.status des Hosts in die Zone des Fensters (app-host.js: code 'app:<code>').
    const zoneOf = (a) => globalThis.__ctoxShellV4.shellHandle()?.windows().find((r) => r.appId === a)?.host?.zone?.entries() ?? [];
    const zMs = await untilMs(A.page, (src) => (0, eval)(src)('k24-probe').some((e) => e.code === 'app:k24.command.tick_add'), zoneOf.toString(), 5000);
    const z = await A.page.evaluate((src) => (0, eval)(src)('k24-probe').map((e) => ({ code: e.code, level: e.level, text: e.text, source: e.source })), zoneOf.toString());
    z.push(...await A.page.evaluate(() => [...document.querySelectorAll('.v4-status-zone:not([hidden])')].map((e) => ({ dom: e.textContent.trim().slice(0, 120), level: e.dataset.level }))));
    results.measures.statusReport = { ms: zMs, zone: z };
    sub('V', 'api.status.report: gescheiterter Befehl → Zoneneintrag der App (Fehler-Injektion des Fake-Core)', zMs != null && z.filter((e) => e.code === 'app:k24.command.tick_add').length === 1, { ms: zMs, zone: z });
    await A.page.screenshot({ path: join(SHOTS, '3-app-after-commands-A.png') });
  } else sub('V', 'api.status.report: gescheiterter Befehl → Zoneneintrag', null, 'Fehler-Injektion nur im Fake-Core', 'inject');
  await B.page.screenshot({ path: join(SHOTS, '3-app-after-commands-B.png') });

  // =========================================================== (8) App-Skill, Prompt-Template, Agentenlauf mit Rückruf
  await runSkillChecks({ A, B, ver1, man1 });

  // =========================================================== (6) v2 inkompatibel + Migration → Remount ohne Eingabeverlust
  const DRAFT = { A: 'Halb getippt in A – nicht abgeschickt', B: 'Halb getippt in B – nicht abgeschickt' };
  for (const P of [A, B]) {
    await P.page.click(`.v4f [data-app="${APP_ID}"] [data-k24-input]`);
    await P.page.keyboard.type(DRAFT[P.label]);
  }
  const base = await activeVer(A.page);
  // Ordner v2 für den Upload: from_ver der Migration = aktive Basisversion (Core vergibt sie beim Veröffentlichen).
  const upV2 = join(OUT, 'upload/v2', APP_ID);
  await rm(join(OUT, 'upload'), { recursive: true, force: true });
  await cp(APP_V2, upV2, { recursive: true });
  const schemaV2 = (await readFile(join(upV2, 'collections.schema.json'), 'utf8')).replace('__BASE_APP_VER__', base);
  await writeFile(join(upV2, 'collections.schema.json'), schemaV2);
  // Die Shell selbst stuft die Änderung ein (schema-diff.js aus dem Shell-Artefakt) – Spiegel der Core-Prüfung.
  const cls = await A.page.evaluate(async ({ v1, v2, base, files }) => {
    const sv = document.querySelector('.v4f[data-v4-ready]').dataset.shellVer;
    const m = await import(`/~a/shell@${sv}/v4/source/schema-diff.js`);
    const diff = m.classifyCollections(m.parseCollectionsFile(v1), m.parseCollectionsFile(v2), { baseVer: base });
    const noMig = m.classifyCollections(m.parseCollectionsFile(v1), { ...m.parseCollectionsFile(v2), migrations: {} }, { baseVer: base });
    return { diff, blockers: m.publishBlockers(diff, files), blockersWithoutMigration: m.publishBlockers(noMig, files) };
  }, { v1: await readFile(join(APP_V1, 'collections.schema.json'), 'utf8'), v2: schemaV2, base, files: (await walk(upV2)).map((f) => relative(upV2, f)) });
  results.measures.v2classify = cls;
  const d0 = cls.diff.find((d) => d.name === COLL);
  sub(6, 'Shell stuft v2 als neue Generation ein, Migration erkannt, ohne Migration blockiert', d0?.kind === 'generation' && !!d0?.migration && cls.blockers.length === 0 && cls.blockersWithoutMigration.length === 1, { kind: d0?.kind, reasons: d0?.reasons, migration: d0?.migration, blockers: cls.blockers, blockersWithoutMigration: cls.blockersWithoutMigration });
  await A.page.evaluate(() => document.querySelector('[data-k24-harness]')?.style.removeProperty('display'));
  let upV2Upload = upV2;
  if (DIAG) {
    upV2Upload = join(OUT, 'upload/diag-v2', APP_ID);
    await rm(dirname(upV2Upload), { recursive: true, force: true });
    await cp(upV2, upV2Upload, { recursive: true });
    const m = await readJson(join(upV2Upload, 'manifest.json'));
    m.capabilities_required = [];
    await writeFile(join(upV2Upload, 'manifest.json'), `${JSON.stringify(m, null, 2)}\n`);
  }
  const picked2 = await uploadFolder(A.page, upV2Upload);
  results.measures.v2 = { base, picked: picked2 };
  const remount = (P) => untilMs(P.page, ({ a, base, d }) => {
    const w = [...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === a);
    const r = w?.querySelector('.v4-app-root');
    return !!r && r.dataset.appVer !== base && r.dataset.k24Schema === '2' && w.dataset.hostState === 'mounted' && r.querySelector('[data-k24-input]')?.value === d;
  }, { a: APP_ID, base, d: DRAFT[P.label] }, 10000);
  // „busy“ = eine vorige Veröffentlichung dieser App hält die Sperre noch (gemessen nach dem Quelltext-Publish in (8)):
  // erneut versuchen, Versuche und Wartezeit mitschreiben; gemessen wird ab dem angenommenen Klick.
  const busy = { retries: 0, waitedMs: 0 };
  const tFirst = Date.now();
  let accepted = false;
  for (let i = 0; i < 40 && !accepted; i++) {
    await A.page.evaluate(() => { window.__k24UploadZone.length = 0; delete document.querySelector('[data-k24-harness] [data-v4-source-upload]').dataset.lastPublished; });
    if (i > 0) { const h = await A.page.$('[data-k24-harness] [data-publish]'); if (await h.isDisabled()) await uploadFolder(A.page, upV2Upload); }
    busy.acceptedClickAt = await clickPublish(A.page);
    const outcome = await A.page.waitForFunction(() => document.querySelector('[data-k24-harness] [data-v4-source-upload]')?.dataset.lastPublished || (window.__k24UploadZone.length ? JSON.stringify(window.__k24UploadZone) : null), null, { timeout: 10000 }).then((h) => h.jsonValue()).catch(() => null);
    if (outcome && !String(outcome).includes('(busy)')) { accepted = true; busy.outcome = outcome; break; }
    busy.retries++;
    busy.last = outcome;
    await A.page.waitForTimeout(500);
  }
  busy.waitedMs = Date.now() - tFirst;
  results.measures.v2busy = busy;
  const tWait = Date.now();
  let [rA, rB] = await Promise.all([remount(A), remount(B)]);
  // ab angenommenem Klick: Zeit bis zur Annahme-Rückmeldung der Fläche plus Wartezeit (obere Schranke)
  const lag = tWait - busy.acceptedClickAt;
  if (rA != null) rA += lag; if (rB != null) rB += lag;
  busy.lagMs = lag;
  const ver2 = await activeVer(A.page);
  const infoA = await winInfo(A.page); const infoB = await winInfo(B.page);
  results.measures.v2 = { ...results.measures.v2, ver: ver2, remountMs: { A: rA, B: rB }, A: infoA, B: infoB, zone: await A.page.evaluate(() => window.__k24UploadZone) };
  sub(6, 'Tab A: v2 montiert, nicht abgeschickte Eingabe erhalten ≤ 2 s', rA != null && rA <= LIMIT_MS, { ms: rA, ver: infoA?.ver, input: infoA?.input });
  sub(6, 'Tab B: v2 montiert, nicht abgeschickte Eingabe erhalten ≤ 2 s', rB != null && rB <= LIMIT_MS, { ms: rB, ver: infoB?.ver, input: infoB?.input });
  const mig = await A.page.evaluate(async ({ a, v, c }) => {
    const b = globalThis.__ctoxShellV4.bridge;
    const m = await b.artifacts.manifest(a, v);
    const rows = b.appApi(a, v).query(c, {}).get();
    return { generations: m?.generations ?? null, rows: rows.map((r) => ({ id: r.id, source: r.source, weight: r.weight ?? null })) };
  }, { a: APP_ID, v: ver2, c: COLL });
  results.measures.v2.migration = mig;
  sub(6, `neue Generation ${COLL}@g2 angelegt`, (mig.generations?.[COLL] ?? 1) === 2, mig.generations, 'migration');
  sub(6, 'Migration ausgeführt: alle Bestandszeilen tragen weight', mig.rows.length > 0 && mig.rows.every((r) => r.weight === 1), mig.rows, 'migration');
  sub(6, 'Remount beim zweiten Nutzer', null, 'App beim zweiten Nutzer nicht vorhanden (siehe 3)', 'user2');
  await A.page.screenshot({ path: join(SHOTS, '4-after-v2-A.png') });
  await B.page.screenshot({ path: join(SHOTS, '4-after-v2-B.png') });

  // =========================================================== (5) Trigger feuert
  const remaining = Math.max(0, tClick + TRIGGER_WAIT_MS - Date.now());
  const trg = await tickVisible(A, 'trigger', null, remaining + 1000);
  results.measures.trigger = { waitedMsSincePublish: Date.now() - tClick, firstSeenMs: trg, every: 'PT1M', evidence: { triggerRunner: results.evidence.k1?.triggerRunner } };
  sub(5, `every-Trigger PT1M schreibt einen Tick (gewartet ${Math.round((Date.now() - tClick) / 1000)} s ab Veröffentlichung)`, trg != null, results.measures.trigger, 'trigger');

  // =========================================================== (7) Entfernen → Archiv, verschwindet überall
  const removeApi = await A.page.evaluate((a) => {
    const b = globalThis.__ctoxShellV4.bridge;
    const src = b.source(a);
    const shellKeys = window.__boot ? Object.keys(window.__boot.kernel.shell) : [];
    const appsKeys = Object.keys(b.apps ?? {});
    // draft.remove/rename betreffen Dateien im Entwurf, nicht die App – gesucht wird ein App-Weg in Shell-API/Quelltext-API.
    return { sourceKeys: Object.keys(src), draftKeys: Object.keys(src.draft), shellKeys, appsKeys, found: [...Object.keys(src), ...shellKeys, ...appsKeys].filter((k) => /remove|uninstall|archive|delete/i.test(k)) };
  }, APP_ID);
  results.measures.remove = { api: removeApi };
  sub(7, 'Admin-Weg „App entfernen“ in Shell/Kern vorhanden', removeApi.found.length > 0, removeApi, 'remove');
  if (BIND) {
    // Client-Hälfte: der Core meldet die App als entfernt (Grabstein business_apps über die K-1-Testnaht = Core-Rolle).
    // Prüft nur, dass alle Clients ohne Reload folgen; das Entfernen selbst und das Archiv bleiben OFFEN.
    const gone = (P) => untilMs(P.page, ({ sel, a }) => !document.querySelector(sel) && ![...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].some((e) => (e.dataset.windowApp ?? e.dataset.app) === a), { sel: tileSel, a: APP_ID }, 5000);
    await A.page.evaluate((a) => window.__boot.kernel.sim.push('business_apps', [{ id: a, _deleted: true }]), APP_ID);
    const [gA, gB] = await Promise.all([gone(A), gone(B)]);
    const left = await Promise.all([A, B].map((P) => P.page.evaluate(({ sel, a }) => ({ tile: !!document.querySelector(sel), window: [...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].filter((e) => (e.dataset.windowApp ?? e.dataset.app) === a).map((e) => e.dataset.hostState) }), { sel: tileSel, a: APP_ID })));
    results.measures.remove.clientHalf = { A: gA, B: gB, left };
    sub(7, 'Client-Hälfte: Grabstein der App → Kachel und Fenster verschwinden in A und B ≤ 2 s (Core-Rolle über K-1-Testnaht)', gA != null && gB != null && Math.max(gA, gB) <= LIMIT_MS, { A: gA, B: gB, left });
    await A.page.screenshot({ path: join(SHOTS, '5-after-remove-A.png') });
  } else sub(7, 'App verschwindet in allen Clients ≤ 2 s', null, 'Entfernen-Weg fehlt', 'remove');
  sub(7, 'entfernte App im Archiv (Daten/Versionen wiederherstellbar)', null, 'kein Archiv in K-1', 'archive');
  sub(7, 'verschwindet beim zweiten Nutzer und in Workjet Ops', null, 'siehe (3)', 'user2');

  // =========================================================== (2) kein Neustart, kein Respawn, kein Reload
  for (const P of pages) {
    const still = await P.page.evaluate(() => window.__k24Marker ?? null);
    const navEntries = await P.page.evaluate(() => performance.getEntriesByType('navigation').length);
    sub(2, `Tab ${P.label}: genau eine Navigation, Marker unverändert (kein Reload)`, P.log.navigations === 1 && navEntries === 1 && still === P.marker, { navigations: P.log.navigations, navEntries, marker: still === P.marker });
  }
  if (BIND) {
    const lead = await A.page.evaluate(() => ({ leader: window.__boot.leader, seeded: !!window.__boot.seeded }));
    sub(2, 'kein Neustart: derselbe Server-/Kernel-Lauf (Tab A bleibt Leiter, kein zweites Seed)', lead.leader === true && server.requests.filter((r) => r.path === '/boot/seed.json').length === (U2 ? 2 : 1), { ...lead, seedFetches: server.requests.filter((r) => r.path === '/boot/seed.json').length });
  }
  sub(2, 'kein Peer-Respawn', null, 'kein nativer Peer', 'peer');

  // =========================================================== (1) keine Änderung an Rust/Core/Shell-Artefakt
  const shellAfter = await shellFingerprint(A.page);
  const after = { files: await treeHash(SHELL_SRC, skipVolatile), git: gitState(), core: before.core != null ? execSync(String(CORE_FP_CMD), { encoding: 'utf8' }) : null };
  results.measures.noChange = { before: { files: before.files, git: before.git, shell: { ...shellBefore, hash: undefined } }, after: { files: after.files, git: after.git, shell: { ...shellAfter, hash: undefined } } };
  sub(1, 'Shell-Artefakt unverändert (Version und Datei-Hashes vor = nach)', shellBefore.ver === shellAfter.ver && shellBefore.hash === shellAfter.hash, { before: shellBefore.ver, after: shellAfter.ver, files: shellAfter.files });
  sub(1, 'Quellen von Kernel, Shell, Boot, shared unverändert (sha256 über Baum vor = nach)', before.files.sha256 === after.files.sha256, { before: before.files, after: after.files });
  sub(1, 'kein Rust-/Cargo-/Konfigurations-Diff entstanden, Arbeitsbaum außerhalb v4/k24 unverändert', before.git.rustDiffSha256 === after.git.rustDiffSha256 && JSON.stringify(before.git.statusOutsideK24) === JSON.stringify(after.git.statusOutsideK24), { headBefore: before.git.head, headAfter: after.git.head, statusOutsideK24: after.git.statusOutsideK24 });
  sub(1, 'laufender Core: Binary + Konfiguration unverändert', before.core != null ? before.core === after.core : null, before.core != null ? { same: before.core === after.core } : 'kein --core-fingerprint-cmd', 'core');

  // =========================================================== W: Wächter
  const guardRun = async (label, appDir, mutate) => {
    const root = join(OUT, `guard-root-${label}`);
    await rm(root, { recursive: true, force: true });
    await mkdir(join(root, 'modules'), { recursive: true });
    await mkdir(join(root, 'v4/manifests'), { recursive: true });
    await mkdir(join(root, 'caps'), { recursive: true });
    await cp(appDir, join(root, 'modules', APP_ID), { recursive: true });
    // Shell- und shared-Code der laufenden Shell mitprüfen (R3/R4: nennen sie die fremde App?).
    for (const p of ['v4/frame', 'v4/shell', 'v4/source', 'shared']) await cp(join(BOS, p), join(root, p), { recursive: true });
    for (const f of ['v4/sw.js', 'v4/blob-loader.js']) if (await exists(join(BOS, f))) await cp(join(BOS, f), join(root, f));
    const m = await readJson(join(appDir, 'manifest.json'));
    m.collections = (await readJson(join(appDir, 'collections.schema.json'))).collections; // Registry-Form (A9 → Manifest)
    await writeFile(join(root, 'v4/manifests', `${APP_ID}.manifest.json`), JSON.stringify(m, null, 2));
    await writeFile(join(root, 'caps', `${APP_ID}.manifest.json`), JSON.stringify(m, null, 2));
    if (mutate) await mutate(root);
    const runTool = (args) => { try { return { code: 0, out: sh(process.execPath, args) }; } catch (e) { return { code: e.status, out: `${e.stdout ?? ''}${e.stderr ?? ''}` }; } };
    const caps = runTool([join(GUARD_BOS, 'v4/os-primitives/check-caps.mjs'), '--no-baseline', '--manifests', join(root, 'caps')]);
    const iso = runTool([join(GUARD_BOS, 'v4/tools/isolation/check-isolation.mjs'), '--root', root, '--json']);
    let isoJson = null; try { isoJson = JSON.parse(iso.out); } catch { /* Text */ }
    return { caps: { code: caps.code, out: caps.out.trim().slice(0, 600) }, iso: { code: iso.code, stats: isoJson?.stats, hits: isoJson?.hits?.slice(0, 10) ?? iso.out.slice(0, 600) } };
  };
  const gV1 = await guardRun('v1', APP_V1);
  const gV2 = await guardRun('v2', upV2);
  // Gegenprobe der Gegenprobe: ein Ausbruch aus dem App-Ordner und eine App-Logik-Capability müssen anschlagen.
  const gBad = await guardRun('mutant', APP_V1, async (root) => {
    const f = join(root, 'modules', APP_ID, 'index.js');
    await writeFile(f, `import '../../shared/base.js';\n${await readFile(f, 'utf8')}`);
    const mf = join(root, 'caps', `${APP_ID}.manifest.json`);
    const m = await readJson(mf); m.capabilities_required = [...m.capabilities_required, 'k24.tick.add']; await writeFile(mf, JSON.stringify(m));
  });
  for (const l of ['v1', 'v2', 'mutant']) await rm(join(OUT, `guard-root-${l}`), { recursive: true, force: true }); // reproduzierbar, 40 MB
  results.measures.guards = { v1: gV1, v2: gV2, mutant: gBad, guardBos: GUARD_BOS, guardHead: (() => { try { return sh('git', ['-C', GUARD_BOS, 'rev-parse', '--short', 'HEAD']).trim(); } catch { return null; } })() };
  sub('W', 'check-caps.mjs --no-baseline: v1 und v2 ohne Treffer', gV1.caps.code === 0 && gV2.caps.code === 0, { v1: gV1.caps.out, v2: gV2.caps.out });
  sub('W', 'check-isolation.mjs (App + Shell + shared): v1 und v2 ohne Treffer', gV1.iso.code === 0 && gV2.iso.code === 0 && !gV1.iso.hits?.length && !gV2.iso.hits?.length, { v1: gV1.iso.stats, v2: gV2.iso.stats });
  sub('W', 'Wächter schlagen bei Mutante an (Import aus ../../shared, App-Capability)', gBad.caps.code === 1 && gBad.iso.code === 1, { caps: gBad.caps.code, iso: gBad.iso.code, isoHits: Array.isArray(gBad.iso.hits) ? gBad.iso.hits.map((h) => h.rule ?? h).slice(0, 3) : gBad.iso.hits });

  // ---- Fehler/Konsole ----
  results.measures.pageErrors = Object.fromEntries(pages.map((P) => [P.label, P.log.errors]));
  results.measures.console = Object.fromEntries(pages.map((P) => [P.label, P.log.console.slice(0, 20)]));
  sub('V', 'keine pageerror in allen Tabs', pages.every((P) => P.log.errors.length === 0), results.measures.pageErrors);
} catch (err) {
  results.fatal = String(err?.stack || err);
  console.error(err);
  exitCode = 1;
} finally {
  finalize();
  results.finished = new Date().toISOString();
  await save();
  await writeResultsBlock().catch((e) => console.error('RESULTS.md', e));
  await browser?.close().catch(() => {});
  await server?.close().catch(() => {});
}
const fails = Object.values(results.criteria).filter((c) => c.status === 'FAIL').length;
console.log('\nKriterien:');
for (const [k, c] of Object.entries(results.criteria)) console.log(`  ${c.status.padEnd(5)} ${c.title}`);
process.exit(exitCode || (fails ? 1 : 0));

// (8) §16.30: Skill/Prompt im Paket, Knopf → api.command → Handler → agent.task.enqueue → Rückruf in den eigenen Handler.
async function runSkillChecks({ A, B, ver1, man1 }) {
  const SKILL_FILE = 'skills/k24-summary/SKILL.md';
  const files = Object.keys(man1?.files ?? {});
  sub(8, 'Skill und Prompt-Template als Dateien im veröffentlichten App-Artefakt', files.includes(SKILL_FILE) && files.includes('prompts/k24-summary.md'), files.filter((f) => /^(skills|prompts)\//.test(f)));
  sub(8, 'Kern übernimmt Manifest skills:[{name,path}] (Namensraum k24-probe/k24-summary)', Array.isArray(man1?.skills) && man1.skills.some((s) => s.name === 'k24-summary'), { skills: man1?.skills ?? null }, 'skills');
  const LABEL = 'K24 Agent A';
  await A.page.fill(`.v4f [data-app="${APP_ID}"] [data-k24-input]`, LABEL);
  await A.page.click(`.v4f [data-app="${APP_ID}"] [data-k24-agent]`);
  const cmdDone = await untilMs(B.page, ({ a, v }) => globalThis.__ctoxShellV4.bridge.appApi(a, v).query('business_commands', { selector: { app_id: a, type: 'k24.agent.start', status: 'done' } }).get().length > 0, { a: APP_ID, v: ver1 }, 5000);
  const runRow = (P, sel, timeout) => untilMs(P.page, ({ a, sel }) => { const w = [...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === a); return !!w?.querySelector(sel); }, { a: APP_ID, sel }, timeout);
  const [qA, qB] = await Promise.all([runRow(A, '[data-k24-run]', 5000), runRow(B, '[data-k24-run]', 5000)]);
  const [cA, cB] = await Promise.all([runRow(A, '[data-k24-run][data-k24-run-status="done"][data-k24-callbacks="1"]', AGENT_WAIT_MS), runRow(B, '[data-k24-run][data-k24-run-status="done"][data-k24-callbacks="1"]', AGENT_WAIT_MS)]);
  const docs = await A.page.evaluate(({ a, v }) => globalThis.__ctoxShellV4.bridge.appApi(a, v).query('business_commands', { selector: { app_id: a } }).get().filter((d) => d.type.startsWith('k24.agent.')).map((d) => ({ type: d.type, status: d.status, result: d.result ?? null })), { a: APP_ID, v: ver1 });
  results.measures.agent = { commandDoneInB_ms: cmdDone, runRowMs: { A: qA, B: qB }, callbackOnceMs: { A: cA, B: cB }, commandDocs: docs };
  sub(8, 'Knopf → Befehlsdokument k24.agent.start erreicht „done“, in Tab B sichtbar ≤ 2 s', cmdDone != null && cmdDone <= LIMIT_MS, { ms: cmdDone, docs });
  sub(8, 'Handler hat agent.task.enqueue aufgerufen und den Lauf in k24_runs geschrieben (A ≤ 2 s)', qA != null && qA <= LIMIT_MS, { A: qA, B: qB }, 'handler');
  sub(8, 'Rückruf k24.agent.done genau einmal im eigenen Handler (callback_count = 1)', cA != null, { A: cA }, 'agent');
  sub(8, 'Ergebnis des Agentenlaufs live in Tab B', cB != null, { B: cB }, 'agent');
  sub(8, 'Ergebnis beim zweiten Nutzer', null, 'App beim zweiten Nutzer nicht vorhanden (siehe 3)', 'user2');
  // Skill im Quelltext-Modus der Shell ändern (UI: Datei wählen, Text ändern, Veröffentlichen) → neue Version ohne Reload.
  const MARK_OLD = 'K24-Skill-Marke: v1';
  const MARK_NEW = 'K24-Skill-Marke: quelltext';
  const opened = await A.page.evaluate(async () => { try { await globalThis.__ctoxShellV4.openSource('k24-probe'); return 'ok'; } catch (e) { return String(e.message); } });
  await A.page.waitForTimeout(600);
  const pickedSkill = await A.page.evaluate((f) => { const sel = document.querySelector('[data-v4-source] select'); if (!sel || ![...sel.options].some((o) => o.value === f)) return [...(sel?.options ?? [])].map((o) => o.value); sel.value = f; sel.dispatchEvent(new Event('change', { bubbles: true })); return 'ok'; }, SKILL_FILE);
  await A.page.waitForTimeout(600);
  const edited = await A.page.evaluate(({ o, n }) => { const t = document.querySelector('[data-v4-source] textarea'); if (!t || !t.value.includes(o)) return false; t.value = t.value.split(o).join(n); t.dispatchEvent(new Event('input', { bubbles: true })); return true; }, { o: MARK_OLD, n: MARK_NEW });
  await A.page.waitForTimeout(600);
  let skillPub = { opened, pickedSkill, edited };
  if (edited) {
    await A.page.screenshot({ path: join(SHOTS, '3b-source-skill-edit-A.png') });
    await A.page.click('[data-v4-source] [data-publish]', { timeout: 5000 }).catch((e) => { skillPub.clickError = e.message.split('\n')[0]; });
    const newVer = (P) => untilMs(P.page, ({ a, v }) => { const w = [...document.querySelectorAll('.v4f[data-v4-ready] [data-window-id]')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === a); const r = w?.querySelector('.v4-app-root'); return !!r && r.dataset.appVer !== v && w.dataset.hostState === 'mounted'; }, { a: APP_ID, v: ver1 }, 10000);
    const [sA, sB] = await Promise.all([newVer(A), newVer(B)]);
    const ver = await activeVer(A.page);
    const text = await A.page.evaluate(async ({ a, v, f }) => { const b = globalThis.__ctoxShellV4.bridge; const m = await b.artifacts.manifest(a, v); return m?.files?.[f] ? (await b.artifacts.read(m.files[f])).text() : null; }, { a: APP_ID, v: ver, f: SKILL_FILE });
    skillPub = { ...skillPub, ver, remountMs: { A: sA, B: sB }, newMarker: !!text?.includes(MARK_NEW) };
    sub(8, 'Skill im Quelltext-Modus geändert → neue App-Version mit geändertem SKILL.md, A und B folgen ohne Reload ≤ 2 s', ver !== ver1 && skillPub.newMarker && sA != null && sB != null && Math.max(sA, sB) <= LIMIT_MS, skillPub);
  } else sub(8, 'Skill im Quelltext-Modus geändert → neue App-Version mit geändertem SKILL.md', false, skillPub);
  await A.page.evaluate(() => { for (const h of document.querySelectorAll('[data-v4-source-host]')) h.remove(); });
  results.measures.skillEdit = skillPub;
  sub(8, 'geänderter Skill wirkt beim nächsten Agentenlauf', null, 'kein Agentenlauf in K-1', 'agent');
}

async function writeResultsBlock() {
  const file = join(HERE, 'RESULTS.md');
  const esc = (s) => String(s).replace(/\|/g, '\\|').replace(/\n/g, ' ');
  const lines = [
    '<!-- BEGIN K24 RESULT (von run.mjs geschrieben) -->',
    `Lauf ${results.started} → ${results.finished}, Klon-HEAD ${results.tree?.head ?? '?'}${results.tree?.dirty?.length ? ` + ${results.tree.dirty.length} ungesicherte Pfade anderer Stränge (${results.tree.dirty.map((l) => l.slice(3).replace('src/apps/business-os/', '')).join(', ')})` : ''}, Ziel \`${TARGET.kind === 'bind' ? 'bind' : `url=${TARGET.url}`}\`${DIAG ? ' – **Diagnose-Version**: die K24-App aktiviert auf bind nicht (needs_update, capabilities), Messkette ab (3) lief auf derselben App ohne capabilities_required' : ''}${results.fatal ? ' – **ABBRUCH**: ' + esc(results.fatal.split('\n')[0]) : ''}`,
    '',
    '| Kriterium | Ergebnis | Grund (bei FAIL/OFFEN) |',
    '|---|---|---|',
    ...Object.entries(results.criteria).map(([, c]) => `| ${esc(c.title)} | **${c.status}** | ${esc(c.reasons.join(' · ') || '–')} |`),
    '',
    '<details><summary>Teilprüfungen</summary>',
    '',
    '| Krit. | Teilprüfung | Ergebnis | Messung |',
    '|---|---|---|---|',
    ...results.subs.map((s) => `| ${s.crit} | ${esc(s.name)} | ${s.status} | ${esc(typeof s.detail === 'string' ? s.detail : JSON.stringify(s.detail ?? '')).slice(0, 260)} |`),
    '',
    '</details>',
    '<!-- END K24 RESULT -->',
  ];
  let text = (await exists(file)) ? await readFile(file, 'utf8') : '# K24 – Abnahmewerkzeug\n\n';
  const i = text.indexOf('<!-- BEGIN K24 RESULT');
  const j = text.indexOf('<!-- END K24 RESULT -->');
  const block = lines.join('\n');
  text = i >= 0 && j > i ? text.slice(0, i) + block + text.slice(j + '<!-- END K24 RESULT -->'.length) : `${text.trimEnd()}\n\n${block}\n`;
  await writeFile(file, text);
}
