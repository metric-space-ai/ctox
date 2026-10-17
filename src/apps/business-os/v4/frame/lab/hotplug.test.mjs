// Hot-Plug-Probe (Owner-Regel „Hot-Plug für alle Apps“, Kernauftrag 17.10.2026): eine offene Live-Abfrage der Shell auf
// eine Fremd-Collection füllt sich, sobald das Eigentümer-Manifest (bzw. der App-Grant, E55) kommt – ohne Neuladen.
//
// Fall: Code-Ansicht von „Notizen“ mit Reiter „Meldungen“ ist offen, BEVOR die Eigentümer-App ctox
// (business_module_reports, business_users) veröffentlicht ist → Liste leer. Dann ctox veröffentlichen und eine Meldung
// für „notes“ schreiben → die Zeile muss in der schon offenen Liste erscheinen.
// Gegenprobe: auf Stub-Pin 4b0d0ac rot (Liste bleibt leer – Anlass des Kernauftrags), auf 64acc2c (Hot-Plug-Pin) grün.
//
// Lauf: node src/apps/business-os/v4/frame/lab/hotplug.test.mjs            (Pin aus CTOX_APP_API_STUB, sonst Default unten)
import { createRequire } from 'node:module';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { realManifestRoutes } from '../../tools/port/real-manifest.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
process.env.CTOX_APP_API_STUB ||= path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const { startServer } = await import('./serve.mjs');
const require = createRequire(path.join(here, '../../../package.json'));
const { chromium } = require('playwright');

const results = [];
const check = (name, ok, detail) => { results.push({ name, ok: !!ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` – ${detail}` : ''}`); };

const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ headless: true });
const P = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
P.on('pageerror', (e) => errors.push(String(e)));
try {
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?inst=hotplug-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  const H = (fn, arg) => P.evaluate(fn, arg);
  const notes = await H(async () => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow('notes', { geometry: { x: 20, y: 30, w: 860, h: 560 } }); await h.win(o).host.ready; return o; });
  const NW = '.shell-window[data-window-app="notes"]';
  const ownerBefore = await H(async () => !!(await window.__lab.kernel.sim.inspect('business_apps', 'ctox'))?.data?.active_ver);
  await H((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource('notes', { tab: 'reports' }), notes);
  await P.waitForSelector(`${NW} [data-v4-reports]`, { timeout: 8000 });
  await P.waitForTimeout(500);
  const rowsBefore = await P.$$eval(`${NW} [data-v4-reports] [data-report-id]`, (e) => e.length);

  // Eigentümer kommt nach der Shell (wie eine später installierte App), dann eine Meldung für „notes“.
  const owner = (realManifestRoutes('shell')._fixture_owners ?? []).find((o) => o.app === 'ctox');
  const id = `rep-hotplug-${Date.now().toString(36)}`;
  const put = await H(async ([o, id]) => {
    const k = window.__lab.kernel;
    await k.sim.publishAppVersion(o.app, o.ver, o.publish);
    const now = Date.now();
    const h = k.appApi('ctox', o.ver).put('business_module_reports', { id, report_id: id, module_id: 'notes', kind: 'bug', title: 'Hot-Plug-Probe', summary: 'Hot-Plug-Probe', status: 'open', reporter_id: 'owner', created_at_ms: now, updated_at_ms: now });
    try { await (h?.local ?? h); return 'ok'; } catch (e) { return String(e?.code || e); }
  }, [owner, id]);
  const t0 = Date.now();
  const appeared = await P.waitForSelector(`${NW} [data-v4-reports] [data-report-id="${id}"]`, { timeout: 5000 }).then(() => Date.now() - t0).catch(() => null);
  check('Vorbedingung: Eigentümer ctox fehlt beim Öffnen, Liste leer, Schreiben als Eigentümer klappt', !ownerBefore && rowsBefore === 0 && put === 'ok', JSON.stringify({ ownerBefore, rowsBefore, put }));
  check('HOTPLUG offene Meldungsliste füllt sich nach später veröffentlichtem Eigentümer ohne Neuladen (≤ 5 s)', appeared != null, JSON.stringify({ appearedMs: appeared, pin: path.basename(process.env.CTOX_APP_API_STUB) }));
  check('keine Seitenfehler', errors.length === 0, errors.slice(0, 3).join(' | '));
} catch (e) {
  check('Lauf ohne Ausnahme', false, String(e?.message || e).slice(0, 300));
} finally {
  await browser.close();
  server.close();
  const failed = results.filter((r) => !r.ok).length;
  console.log(`${results.length - failed}/${results.length} bestanden`);
  process.exitCode = failed ? 1 : 0;
}
