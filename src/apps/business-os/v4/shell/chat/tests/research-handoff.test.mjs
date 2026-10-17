// RCH2: Web Research übergibt den systematischen Lauf an den Business Chat (E37, E39 Nr. 4), Rahmen gegen den Kern-Stub,
// Playwright mit echter Maus/Tastatur. Gleiche Bedienbeobachtung wie shared/business-chat.test.mjs:3564 für V2.
// Lauf (aus src/apps/business-os): node v4/shell/chat/tests/research-handoff.test.mjs [counter]
//   counter = Gegenprobe: research läuft mit dem Stand vor RCH2 (git HEAD:modules/research/index.js); (a)/(b) müssen rot sein.
// Fälle:
//   (a) „Research starten“ → Chatleiste offen, Eingabefeld = vollständige Anweisung, KEIN Befehl vor dem Senden
//   (b) „Senden“ → genau ein research.systematic.run mit genau den Befehlsparametern des direkten Wegs
//   (c) ohne ctox/Chatleiste → direkter Weg, genau ein Befehl, kein Fehler-/navigate-Eintrag in der Zone
//   (d) Entwurf byte-gleich zur Anweisung, die der Handler research.systematic.run aus dem gesendeten Befehl rendert
// Exit 0 = alles grün, sonst 1 (auch in der Gegenprobe; dort ist Exit 1 das erwartete Ergebnis).
// Umgebung: CTOX_APP_API_STUB (Standard ~/.local/state/shell-v4/stub-pins/64acc2c), PLAYWRIGHT_MODULE_PATH,
// RCH2_SHOTS (Bildordner, Standard v4/shell/chat/tests/shots).
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOS = path.resolve(here, '../../../..');
const RESEARCH = path.join(BOS, 'modules/research');
process.env.CTOX_APP_API_STUB ||= path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
// ctox:shared/ für den Handler in Node (wie --import v4/tools/node/ctox-shared.mjs), vor dem Laden von enqueue.js.
await import(pathToFileURL(path.join(BOS, 'v4/tools/node/ctox-shared.mjs')).href);
const { enqueueSystematicRun } = await import(pathToFileURL(path.join(RESEARCH, 'handlers/_lib/enqueue.js')).href);
const { startServer } = await import(pathToFileURL(path.join(BOS, 'v4/frame/lab/serve.mjs')).href);
const { realManifestRoutes } = await import(pathToFileURL(path.join(BOS, 'v4/tools/port/real-manifest.mjs')).href);

function loadPlaywright() {
  const tries = [process.env.PLAYWRIGHT_MODULE_PATH, path.join(BOS, 'package.json'), path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json')].filter(Boolean);
  for (const t of tries) {
    try {
      if (/package\.json$/.test(t)) return createRequire(t)('playwright');
      return createRequire(import.meta.url)(t);
    } catch { /* nächster */ }
  }
  throw new Error('Playwright nicht gefunden (PLAYWRIGHT_MODULE_PATH setzen)');
}
const { chromium } = loadPlaywright();
const macShell = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');

const COUNTER = process.argv.includes('counter');
const MODE = COUNTER ? 'rch2-counter' : 'rch2';
const shots = process.env.RCH2_SHOTS || path.join(here, 'shots');
fs.mkdirSync(shots, { recursive: true });
const shot = (name) => path.join(shots, `${MODE}-${name}.png`);
const RUN = 'research.systematic.run';

const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} [${MODE}] ${name}${detail ? ` – ${String(detail).slice(0, 600)}` : ''}`);
}

// Saat wie modules/research/tests/v4-fixture.mjs (dort mit Playwright-Import verbunden, darum hier wörtlich).
const DOMAIN = 'research/drone-bearings';
const TASK = {
  id: 'research_drone_bearings', title: 'Drohnen-Lagerlasten', prompt: 'Lagerlasten für Drohnenantriebe recherchieren', criteria: '',
  status: 'ready', knowledge_domain: DOMAIN, source_catalog_key: 'source_catalog', payload: { user_created: true }, created_at_ms: 1, updated_at_ms: 2,
};
const TABLES = [
  { id: `table:${DOMAIN}:source_catalog`, domain: DOMAIN, table_key: 'source_catalog', title: 'Verified Source Registry', description: 'Research source catalog', row_count: 2,
    rows: [
      { source_id: 'src-a', title: 'Wälzlager unter Propellerlast', source_url: 'https://example.test/a', source_type: 'paper', publisher: 'Example Press' },
      { source_id: 'src-b', title: 'Messreihe Motorlager 2025', source_url: 'https://example.test/b', source_type: 'dataset', publisher: 'Lab B' },
    ], updated_at_ms: 1 },
  { id: `table:${DOMAIN}:source_candidates`, domain: DOMAIN, table_key: 'source_candidates', title: 'Discovery Candidates', row_count: 1,
    rows: [{ source_id: 'cand-1', title: 'Kandidat Lagerreibung', source_url: 'https://example.test/c' }], updated_at_ms: 1 },
];

// research-Artefakt: /~a/research@<ver>[~…]/<pfad> aus modules/research, ctox:shared/ wie der Service Worker umgeschrieben.
// Gegenprobe: index.js im Stand vor RCH2 (HEAD).
const counterIndex = COUNTER ? execFileSync('git', ['show', 'HEAD:src/apps/business-os/modules/research/index.js'], { cwd: BOS, encoding: 'utf8', maxBuffer: 64 << 20 }) : null;
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.md': 'text/markdown; charset=utf-8', '.svg': 'image/svg+xml' };
async function serveResearch(route) {
  const m = /\/~a\/research@[^/]+\/(.+)$/.exec(decodeURIComponent(new URL(route.request().url()).pathname));
  const rel = m ? path.normalize(m[1]) : '';
  const file = path.join(RESEARCH, rel);
  if (!m || !file.startsWith(RESEARCH + path.sep) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) return route.fulfill({ status: 404, body: '' });
  let body = rel === 'index.js' && counterIndex != null ? counterIndex : fs.readFileSync(file, /\.(m?js|css|json|md|svg)$/.test(rel) ? 'utf8' : null);
  if (typeof body === 'string' && /\.m?js$/.test(rel)) body = body.replace(/(['"])ctox:shared\//g, '$1/shared/');
  return route.fulfill({ status: 200, contentType: TYPES[path.extname(rel)] || 'application/octet-stream', body });
}

// Anweisung, die der Handler research.systematic.run aus einem Befehl rendert (echter Handler, Template von der Platte).
async function handlerInstruction(command) {
  let instruction = null;
  const ctx = {
    asset: async (p) => fs.readFileSync(path.join(RESEARCH, p), 'utf8'),
    cap: { agent: { task: { enqueue: async (t) => { instruction = t.instruction; return { task_id: 'q-1' }; } } } },
  };
  await enqueueSystematicRun(command, ctx);
  return instruction;
}

async function clickCenter(P, sel) {
  const el = await P.$(sel);
  await el?.scrollIntoViewIfNeeded().catch(() => {});
  const b = await el?.boundingBox();
  if (!b) return false;
  await P.mouse.click(b.x + b.width / 2, b.y + b.height / 2);
  return true;
}

const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ headless: true, ...(fs.existsSync(macShell) ? { executablePath: macShell } : {}) });
const bctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
const P = await bctx.newPage();
const perr = [];
P.on('pageerror', (e) => perr.push(String(e)));
P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
await P.route(/\/~a\/research@[^/]+\//, serveResearch);

const snapshot = () => P.evaluate((run) => {
  const k = window.__lab.kernel;
  const cmds = (app) => k.appApi(app, app === 'shell' ? 'dev' : '1.0.0').query('business_commands', {}).get()
    .filter((d) => d.type === run || d.type === 'business_os.chat.task')
    .map((d) => ({ id: d.id, type: d.type, app_id: d.app_id, record_id: d.record_id ?? null, status: d.status, payload: d.payload }));
  const seen = new Map();
  for (const d of [...cmds('research'), ...cmds('shell')]) seen.set(d.id, d);
  return { handled: window.__rch2.handled, docs: [...seen.values()] };
}, RUN);

let researchWin = null;
try {
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?inst=rch2-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await P.evaluate(async ({ run, task, tables, routes }) => {
    const { kernel } = window.__lab;
    window.__rch2 = { handled: [] };
    // Empfänger: research.systematic.run (Handler der research-App) und – zur Kontrolle – der Standard-Chatbefehl.
    for (const type of [run, 'business_os.chat.task']) {
      kernel.sim.onCommand(type, (payload, c) => {
        window.__rch2.handled.push({ type, payload, app_id: c.app_id, command_id: c.command_id });
        return { ok: true, task_id: `queue-rch2-${window.__rch2.handled.length}`, status: 'queued' };
      });
    }
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#0f766e"/></svg>';
    // E48 (Stub 8265bd7): echte Manifeste (Routen, Rechte, collections + exports) für ctox (Eigentümer der Shell-Chatlesungen)
    // und research samt deren Eigentümern (knowledge, documents); business_apps-Zeilen davor, sonst überschreiben sie active_ver.
    await kernel.sim.push('business_apps', [{ id: 'knowledge', title: 'Knowledge' }]);
    for (const o of routes.research._fixture_owners) if (o.app !== 'ctox' && !(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('ctox', '1.0.0', { title: 'CTOX', layout: { category: 'Crew', icon_svg: svg }, styles: [], manifestExtra: routes.ctox });
    // research wie ihr Manifest (eigener Handler für research.systematic.run berechtigt den Befehlstyp im Entwurf).
    await kernel.sim.publishAppVersion('research', '1.0.0', { title: 'Web Research',
      layout: { category: 'Wissen', icon_svg: svg, default_width: 1180, default_height: 680 }, styles: ['index.css'],
      manifestExtra: routes.research });
    await kernel.sim.push('research_tasks', [task]);
    await kernel.sim.push('knowledge_tables', tables);
    await kernel.sim.settle?.();
  }, { run: RUN, task: TASK, tables: TABLES, routes: { ctox: realManifestRoutes('ctox'), research: realManifestRoutes('research') } });
  await P.waitForSelector('[data-ctox-chat-root] [data-chat-dock]', { timeout: 8000 }).catch(() => {});
  researchWin = await P.evaluate(() => window.__lab.frame.shellHandle().openWindow('research'));
  const taskSel = `.shell-window[data-window-app="research"] .research-task-item[data-task-id="${TASK.id}"]`;
  const runSel = '.shell-window[data-window-app="research"] [data-action="run-research"]';
  await P.waitForSelector(taskSel, { timeout: 10000 }).catch(() => {});
  await clickCenter(P, taskSel);
  await P.waitForFunction((sel) => { const b = document.querySelector(sel); return b && !b.disabled; }, runSel, { timeout: 8000 }).catch(() => {});
  await P.screenshot({ path: shot('1280-research') });

  // ---------- (a) Research starten → Chat mit Entwurf, kein Befehl ----------
  const clicked = await clickCenter(P, runSel);
  await P.waitForFunction(() => [...document.querySelectorAll('[data-ctox-chat-root] textarea[name="message"]')].some((t) => t.value.length > 200 && t.getBoundingClientRect().width > 0), null, { timeout: 8000 }).catch(() => {});
  await P.waitForTimeout(900); // ein (falscher) Direktbefehl hätte jetzt seinen Weg genommen
  const a = await P.evaluate(async () => {
    const ta = [...document.querySelectorAll('[data-ctox-chat-root] textarea[name="message"]')].find((t) => t.getBoundingClientRect().width > 0);
    const chatWin = ta?.closest('[data-chat-id]');
    const { currentBusinessChat } = await import('/v4/shell/chat/index.js');
    const draft = currentBusinessChat()?.lastDraft || null;
    const runs = window.__lab.kernel.appApi('research', '1.0.0').query('research_runs', {}).get();
    return { value: ta?.value ?? null, chatId: chatWin?.getAttribute('data-chat-id') ?? null, visible: !!ta, draft, runStatus: runs.map((r) => r.status) };
  });
  const before = await snapshot();
  check('(a) Klick auf „Research starten“ (echte Maus)', clicked);
  check('(a) Chatleiste offen, Eingabefeld sichtbar mit Entwurf', a.visible && typeof a.value === 'string' && a.value.length > 200, JSON.stringify({ visible: a.visible, len: a.value?.length ?? null }));
  check('(a) Entwurf = vollständige Anweisung (Kopf, Aufgabe, Abschluss des Templates)', !!a.value
    && a.value.startsWith(`Führe die systematische Recherche für "${TASK.title}" mit dem System-Skill systematic-research fort.`)
    && a.value.includes(`Research Task ID: ${TASK.id}`) && a.value.includes(`Knowledge domain: ${DOMAIN}`)
    && a.value.endsWith('starte keine Child Agents.'), (a.value || '').slice(0, 160));
  check('(a) vor dem Senden KEIN Befehl (weder research.systematic.run noch Chatbefehl)', before.handled.length === 0 && before.docs.length === 0, JSON.stringify({ handled: before.handled.length, docs: before.docs.map((d) => [d.type, d.app_id]) }));
  const winTitle = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o)?.title ?? null, researchWin);
  check('(a) Rahmen setzt Quelle aus dem Fenster und merkt Befehlstyp + Parameter', a.draft?.source_module === 'research' && !!winTitle && a.draft?.source_title === winTitle
    && a.draft?.command_type === RUN && a.draft?.record_id === TASK.id && a.draft?.payload?.record_id === TASK.id && a.draft?.draft === a.value,
    JSON.stringify({ source: a.draft?.source_module, title: a.draft?.source_title, winTitle, type: a.draft?.command_type, record: a.draft?.record_id }));
  check('(a) Lauf-Kopf im Chat-Weg: status chat (nichts läuft vor dem Senden)', a.runStatus.length === 1 && a.runStatus[0] === 'chat', JSON.stringify(a.runStatus));
  await P.screenshot({ path: shot('1280-dock-draft') });

  // ---------- (b) Senden → genau ein research.systematic.run ----------
  const sendSel = a.chatId ? `[data-ctox-chat-root] [data-chat-id="${a.chatId}"] [data-chat-send]` : '[data-ctox-chat-root] [data-chat-send]';
  const sentClick = await clickCenter(P, sendSel);
  await P.waitForFunction(() => window.__rch2.handled.length > 0, null, { timeout: 10000 }).catch(() => {});
  await P.waitForTimeout(900); // Doppelsendung abwarten
  const b = await snapshot();
  const runCmds = b.handled.filter((h) => h.type === RUN);
  const doc = b.docs.find((d) => d.type === RUN) || null;
  check('(b) Klick auf Senden (echte Maus)', sentClick);
  check('(b) genau ein Befehl research.systematic.run, kein Chat-Standardbefehl', runCmds.length === 1 && b.handled.length === 1 && b.docs.length === 1 && doc?.type === RUN,
    JSON.stringify({ handled: b.handled.map((h) => [h.type, h.app_id]), docs: b.docs.map((d) => [d.type, d.app_id, d.status]) }));
  const sent = runCmds[0]?.payload || {};
  const expected = a.draft?.payload || {};
  // Felder, die der Chat (wie V2 submitChatMessage) selbst setzt; alle anderen müssen exakt den Parametern des
  // direkten Wegs entsprechen. title/thread_key/priority setzt der Chat aus genau diesen Parametern.
  const diff = Object.keys(expected).filter((k) => JSON.stringify(sent[k]) !== JSON.stringify(expected[k]));
  check('(b) Nutzlast trägt jede Befehlsgröße des direkten Wegs unverändert', Object.keys(expected).length > 10 && diff.length === 0, JSON.stringify({ keys: Object.keys(expected).length, diff }));
  check('(b) Absender Chat (App shell), Datensatz = Research-Aufgabe, Anweisung = Entwurf', runCmds[0]?.app_id === 'shell' && doc?.record_id === TASK.id
    && sent.instruction === a.value && sent.research_command_id === expected.research_command_id, JSON.stringify({ app: runCmds[0]?.app_id, record: doc?.record_id }));
  await P.waitForTimeout(300);
  await P.screenshot({ path: shot('1280-dock-sent') });

  // ---------- (d) Entwurf byte-gleich zur Handler-Anweisung ----------
  let rendered = null;
  try { rendered = doc ? await handlerInstruction({ id: doc.id, record_id: doc.record_id, payload: doc.payload }) : null; } catch (e) { rendered = `ERROR ${e?.message || e}`; }
  check('(d) Entwurf byte-gleich zur Anweisung des Handlers research.systematic.run (gleicher Befehl)', rendered != null && a.value != null && Buffer.from(rendered).equals(Buffer.from(a.value)),
    JSON.stringify({ handler: rendered?.length ?? null, draft: a.value?.length ?? null, firstDiff: rendered && a.value ? [...rendered].findIndex((ch, i) => ch !== a.value[i]) : null }));

  // ---------- (c) ohne ctox/Chatleiste → direkter Weg ----------
  await P.evaluate(() => window.__lab.remove('ctox'));
  await P.waitForFunction(() => !document.querySelector('[data-ctox-chat-root]'), null, { timeout: 5000 }).catch(() => {});
  await P.evaluate(() => { window.__rch2.handled.length = 0; });
  const docsBefore = (await snapshot()).docs.length;
  await clickCenter(P, taskSel);
  const directClick = await clickCenter(P, runSel);
  await P.waitForFunction(() => window.__rch2.handled.length > 0, null, { timeout: 8000 }).catch(() => {});
  await P.waitForTimeout(900);
  const c = await snapshot();
  const zone = await P.evaluate(({ owner }) => {
    const rec = window.__lab.frame.shellHandle().win(owner);
    const entries = rec?.host?.zone?.entries?.() || [];
    const kernelZone = window.__lab.kernel.statusZone?.forApp?.('research')?.get?.() || [];
    return { frame: entries.map((e) => ({ code: e.code, level: e.level, source: e.source })), kernel: kernelZone.map((e) => ({ code: e.code, level: e.level, source: e.source })) };
  }, { owner: researchWin });
  const newDocs = c.docs.length - docsBefore;
  check('(c) Klick auf „Research starten“ ohne Chatleiste (echte Maus)', directClick);
  check('(c) direkter Weg: genau ein Befehl research.systematic.run von der App research', c.handled.length === 1 && c.handled[0].type === RUN && c.handled[0].app_id === 'research' && newDocs === 1,
    JSON.stringify({ handled: c.handled.map((h) => [h.type, h.app_id]), newDocs }));
  check('(c) direkter Weg: dieselben Befehlsgrößen wie die Chat-Übergabe (bis auf Lauf-Kennungen)', (() => {
    const p = c.handled[0]?.payload || {};
    const perRun = new Set(['research_run_id', 'research_command_id', 'knowledge_contract', 'client_context']);
    return Object.keys(expected).length > 10 && Object.keys(expected).every((k) => perRun.has(k) || JSON.stringify(p[k]) === JSON.stringify(expected[k]))
      && Object.keys(p).every((k) => k in expected);
  })(), '');
  const bad = [...zone.frame, ...zone.kernel].filter((e) => e.level === 'error' || /navigate/.test(e.code));
  check('(c) kein Fehler- oder navigate-Eintrag in der Zone', bad.length === 0, JSON.stringify(zone));
  check('keine Seitenfehler', perr.length === 0, perr.slice(0, 3).join(' | '));
} finally {
  await browser.close();
  server.close();
}

const failed = results.filter((r) => !r.ok);
console.log(`\n[${MODE}] ${results.length - failed.length}/${results.length} PASS`);
fs.writeFileSync(path.join(shots, `${MODE}-results.json`), JSON.stringify(results, null, 2));
process.exit(failed.length ? 1 : 0);
