// FIX10 (Shell v4): Messwerte-Tabellen scrollen nie seitlich – feste Spalten mit Ellipse und Tooltip, in der
// einspaltigen Stufe (Fenster ≤ 768 px) gestapelte Karten „Beschriftung: Wert“ (Container-Query business-app-window).
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac \
//       node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/research/tests/fix10.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { chromium, startServer, seedKernel, pageHtml } from './v4-fixture.mjs';

let server, browser, page;
before(async () => {
  const r = await startServer(pageHtml(1280, 720, { shell: true }));
  server = r.server;
  browser = await chromium.launch();
  page = await browser.newPage();
  page.__errors = [];
  page.on('pageerror', (e) => page.__errors.push(e.message));
  await page.goto(r.base);
  await seedKernel(page);
  await page.evaluate(async () => { window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {}); });
  await page.evaluate(() => window.__k.sim.settle());
  await page.waitForTimeout(300);
});
after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const LONG_SOURCE = 'uiuc-propeller-database-volume-4-apc-thin-electric-9x5-static-and-wind-tunnel-2026';
const ROWS = [
  { source_id: LONG_SOURCE, propeller_size: '9x5', rpm: 6012, thrust_coefficient_CT: 0.1123, power_coefficient_CP: 0.0491,
    confidence: 'hoch – aus der veröffentlichten Tabelle extrahiert, zweifach gegengelesen und mit der Rohdatei abgeglichen',
    propeller_size_original: '9x5', rpm_input: 6012, thrust_N_derived: 4.31, torque_Nm_derived: 0.052, shaft_power_W_derived: 32.7, air_density_kg_m3_input: 1.225 },
  { source_id: 'src-b', propeller_size: '10x4.7', rpm: 4500, thrust_coefficient_CT: 0.098, power_coefficient_CP: 0.041, confidence: 'mittel' },
];

// Tabelle in einen Host wie im Hauptbereich setzen (research-table-host im Fenster-Container) und messen.
async function measure(kind, width) {
  return page.evaluate(({ kind, width, rows }) => {
    const win = document.getElementById('win');
    win.style.width = `${width}px`;
    let probe = document.getElementById('fix10-probe');
    if (!probe) {
      probe = document.createElement('div');
      probe.id = 'fix10-probe';
      probe.setAttribute('data-app', 'research');
      probe.style.cssText = 'position:absolute;inset:0;background:#fff;z-index:9';
      win.appendChild(probe);
    }
    const hooks = window.__mod.__researchTestHooks;
    const html = kind === 'direct' ? hooks.renderDirectMeasurements(rows) : hooks.renderDerivedMeasurements(rows);
    // Wie im Hauptbereich: Breite der mittleren Spalte, im Rahmen gemessen (1280 → 656 px, 900 → 562 px).
    const hostWidth = { 1280: 656, 900: 562 }[width] ?? width - 24;
    probe.innerHTML = `<div class="research-table-host" style="width:${hostWidth}px;height:600px">${html}</div>`;
    const host = probe.querySelector('.research-table-host');
    const firstCell = host.querySelector('tbody tr td');
    const cs = getComputedStyle(firstCell);
    return {
      overflow: host.scrollWidth - host.clientWidth,
      headShown: getComputedStyle(host.querySelector('thead')).display !== 'none',
      firstTitle: firstCell.getAttribute('title'),
      ellipsis: cs.textOverflow === 'ellipsis' && cs.whiteSpace === 'nowrap',
      firstLabel: getComputedStyle(firstCell, '::before').content,
      labels: [...host.querySelectorAll('tbody tr:first-child td')].map((td) => td.dataset.label),
      // Zahlen werden nie abgeschnitten (nur Text-Spalten tragen die Ellipse).
      cutNumbers: [...host.querySelectorAll('tbody td.is-num')].filter((td) => td.scrollWidth > td.clientWidth + 1).map((td) => `${td.dataset.label}=${td.textContent}`),
    };
  }, { kind, width, rows: ROWS });
}

test('FIX10: Messwerte ohne Seitwärts-Scroll – breit Ellipse + Tooltip, bis 768 px Karten mit Beschriftung', async () => {
  for (const kind of ['direct', 'derived']) {
    for (const width of [1280, 900]) {
      const wide = await measure(kind, width);
      assert.ok(wide.overflow <= 1, `${kind} ${width}: scrollt seitlich um ${wide.overflow}px`);
      assert.equal(wide.headShown, true, `${kind} ${width}: Tabellenkopf sichtbar`);
      assert.equal(wide.ellipsis, true, `${kind} ${width}: einzeilig mit Ellipse`);
      assert.equal(wide.firstTitle, LONG_SOURCE, `${kind} ${width}: voller Wert im Tooltip`);
      assert.deepEqual(wide.cutNumbers, [], `${kind} ${width}: Zahlen vollständig`);
    }
    const narrow = await measure(kind, 420);
    assert.ok(narrow.overflow <= 1, `${kind} 420: scrollt seitlich um ${narrow.overflow}px`);
    assert.equal(narrow.headShown, false, `${kind} 420: Karten statt Kopfzeile`);
    assert.equal(narrow.firstLabel, '"Quelle"', `${kind} 420: Beschriftung vor dem Wert`);
    assert.ok(narrow.labels.every(Boolean), `${kind}: jede Zelle trägt ihre Beschriftung`);
  }
  await page.evaluate(() => { document.getElementById('fix10-probe')?.remove(); document.getElementById('win').style.width = '1280px'; });
  assert.deepEqual(page.__errors, []);
});
