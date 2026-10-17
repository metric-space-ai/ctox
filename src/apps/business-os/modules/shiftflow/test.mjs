import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Buffer } from 'node:buffer';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';
import { join } from 'node:path';

// v4: index.js importiert `ctox:shared/…` (nur über den Service Worker auflösbar, §16.15) → esbuild-Bundle mit
// onResolve-Plugin. esbuild aus src/apps/business-os/node_modules, sonst ESBUILD_MODULE bzw. dem Haupt-Checkout.
async function loadEsbuild() {
  try { return await import('esbuild'); } catch (error) {
    if (error?.code !== 'ERR_MODULE_NOT_FOUND') throw error;
    const require = createRequire(import.meta.url);
    return require(process.env.ESBUILD_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/esbuild'));
  }
}
const { build } = await loadEsbuild();
const bundled = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
  plugins: [{
    name: 'ctox-shared',
    setup(b) {
      b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
    },
  }],
});
const { __shiftflowTestHooks: hooks } = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);

const {
  filterShiftflowEmployeesForPlanner,
  getShiftflowPressedState,
  getWeekBoundsMs,
  applyShiftListSelection,
  collectPlanningConflicts,
} = hooks;

const tests = [];
function test(name, fn) {
  tests.push({ name, fn });
}

const employees = [
  { id: 'emp-lisa', name: 'Lisa Schmidt', role: 'Serviceleitung', departments: ['Service', 'Verwaltung'] },
  { id: 'emp-chris', name: 'Christian Meyer', role: 'Küchenchef', departments: ['Küche'] },
  { id: 'emp-michael', name: 'Michael Welsch', role: 'Service & Barista', departments: ['Service', 'Bar'] },
];

test('employee search clears back to the department-scoped schedule set', () => {
  assert.deepEqual(
    filterShiftflowEmployeesForPlanner(employees, { department: 'Service', search: 'lisa' }).map(emp => emp.id),
    ['emp-lisa'],
  );

  assert.deepEqual(
    filterShiftflowEmployeesForPlanner(employees, { department: 'Service', search: '' }).map(emp => emp.id),
    ['emp-lisa', 'emp-michael'],
  );
});

test('employee search includes role and department text', () => {
  assert.deepEqual(
    filterShiftflowEmployeesForPlanner(employees, { department: 'all', search: 'barista' }).map(emp => emp.id),
    ['emp-michael'],
  );
  assert.deepEqual(
    filterShiftflowEmployeesForPlanner(employees, { department: 'all', search: 'küche' }).map(emp => emp.id),
    ['emp-chris'],
  );
});

test('pressed state reflects active center tab and timeline toggle', () => {
  assert.deepEqual(getShiftflowPressedState('timesheets', 'projects'), {
    tabs: { scheduler: false, conflicts: false, timesheets: true, billing: false },
    timeline: { employees: false, projects: true },
  });
});

test('week publish bounds cover exactly seven days', () => {
  const { startMs, endMs } = getWeekBoundsMs(new Date(2026, 4, 25, 11, 30, 0));
  const start = new Date(startMs);
  assert.equal(start.getFullYear(), 2026);
  assert.equal(start.getMonth(), 4);
  assert.equal(start.getDate(), 25);
  assert.equal(start.getHours(), 0);
  assert.equal(start.getMinutes(), 0);
  assert.equal(endMs - startMs, (7 * 24 * 60 * 60 * 1000) - 1);
});

test('presentation follows compact Business OS planning contract', async () => {
  // v4: Selektoren sind unter [data-app="shiftflow"] gescoped; die Regeln dahinter prüfen wir unverändert.
  const css = (await readFile(new URL('./index.css', import.meta.url), 'utf8')).replaceAll('[data-app="shiftflow"] ', '');
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');

  assert.doesNotMatch(css, /Premium/);
  assert.doesNotMatch(css, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
  assert.doesNotMatch(css, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
  assert.doesNotMatch(css, /border-(?:left|right):\s*(?:[2-9]|[0-9]{2,})px/);
  assert.doesNotMatch(css, /@keyframes\s+shiftflow-pulse-active/);
  assert.doesNotMatch(js, /border-radius:12px/);
  // No kit-migration residue: removed custom radius variables and panel classes
  // are gone — the kit supplies those tokens now.
  assert.doesNotMatch(css, /--shiftflow-radius:/);
  assert.doesNotMatch(css, /--shiftflow-panel-radius:/);
  assert.doesNotMatch(css, /--shiftflow-accent:/);
  assert.doesNotMatch(css, /\.shiftflow-panel\b/);
  // Drag-over highlight uses the kit's --accent token, not a stale local copy.
  assert.match(
    css,
    /\.shiftflow-grid-cell\.drag-over\s*\{[\s\S]*?outline:\s*2px solid var\(--accent\)/,
  );
});

test('IA is a canonical shift list plus planning board with conflicts in the main band', async () => {
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  // v4: Selektoren sind unter [data-app="shiftflow"] gescoped; die Regeln dahinter prüfen wir unverändert.
  const css = (await readFile(new URL('./index.css', import.meta.url), 'utf8')).replaceAll('[data-app="shiftflow"] ', '');
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));

  assert.match(html, /class="ctox-pane shiftflow-left-pane"[^>]*data-left-content/);
  assert.match(html, /data-pg-search/);
  // Ein-Knopf-Umschalter (Betreiber-Direktive 31.08.2026): genau EIN
  // Bedienelement, das die Ansicht togglet - kein Paar aus zwei Knoepfen und
  // kein aria-pressed, weil der Knopf eine Aktion ist und kein Zustand.
  assert.equal((html.match(/data-shift-view-toggle/g) || []).length, 1);
  assert.doesNotMatch(html, /data-pg-view=/);
  assert.doesNotMatch(html, /data-shift-view-toggle[^>]*aria-pressed/);
  assert.match(html, /data-shift-view-icon="cards"[\s\S]*data-shift-view-icon="list"/);
  assert.match(html, /data-pg-tray-toggle[\s\S]*data-pg-tray hidden/);
  assert.match(html, /data-pg-reset/);
  assert.equal((html.match(/data-pg-band=/g) || []).length, 2);
  assert.match(html, /class="ctox-pane-body ctox-well"/);
  assert.doesNotMatch(html, /data-pg-footer/, 'keine Fußzeile, die nur den Reiterzähler wiederholt');
  assert.match(html, /id="addShiftBtn"[\s\S]*id="importShiftsBtn"[\s\S]*id="exportShiftsBtn"/);
  assert.match(html, /data-main-view="conflicts"[\s\S]*data-main-count="conflicts"/);
  assert.doesNotMatch(html, /shiftflow-right-pane|data-resizer="right"/);
  assert.equal(Object.hasOwn(manifest.layout, 'right'), false);
  assert.match(manifest.version, /^\d+\.\d+\.\d+$/);
  assert.match(css, /grid-template-columns:\s*var\(--ctox-left-width\) 12px minmax\(360px, 1fr\)/);
  assert.match(css, /\.shiftflow-left-pane\s*\{[\s\S]*?grid-column:\s*1[\s\S]*?grid-template-rows:\s*auto auto minmax\(0, 1fr\) auto/);
  assert.match(css, /\.shiftflow-center-pane\s*\{[\s\S]*?grid-column:\s*3/);
  assert.match(js, /addEventListener\('ctox-pane-grammar-change'/);
  assert.match(js, /const pg = pane\.__ctoxPaneGrammar/);
  // Der Umschalter ist modul-eigen verdrahtet; die Pane-Grammar kennt die
  // Ansicht nicht mehr und darf sie bei Suche/Filter nicht zuruecksetzen.
  assert.match(js, /els\.viewToggleBtn\?\.addEventListener\('click'/);
  assert.match(js, /view: shiftListState\.view === 'list' \? 'list' : 'cards'/);
  assert.match(js, /button\.removeAttribute\('aria-pressed'\)/);
  // Karten und Liste sind zwei Dichten derselben Daten, nicht dieselbe Zeile.
  assert.match(js, /data-shift-meta="when"[\s\S]*data-shift-meta="who"[\s\S]*data-shift-meta="brief"/);
  assert.match(css, /\.shiftflow-shift-list\.is-cards \[data-shift-meta="brief"\]\s*\{[\s\S]*?display:\s*none/);
  assert.match(css, /\.shiftflow-shift-list\.is-list \[data-shift-meta="when"\],\s*\.shiftflow-shift-list\.is-list \[data-shift-meta="who"\]\s*\{[\s\S]*?display:\s*none/);
  assert.doesNotMatch(js, /window\.dispatchEvent|ctox-business-os-chat-submit/);
  // v4 (R10): Markup relativ ohne Cache-Buster, CSS lädt die Shell aus dem Manifest.
  assert.match(js, /fetch\(new URL\('\.\/index\.html', import\.meta\.url\)\)/);
  assert.doesNotMatch(js, /\?v=|MOD_BUILD|ensureStyles/);
  assert.match(js, /data-open-employee-id/);
  assert.match(js, /addEventListener\('dragstart'/);
  assert.match(js, /quick-assign-day-btn/);
});

test('list selection flips existing rows in place without rebuilding the list', () => {
  const makeRow = (id) => ({
    dataset: { shiftListId: id },
    selected: false,
    aria: '',
    classList: { toggle(name, on) { if (name === 'is-selected') this.owner.selected = on; }, owner: null },
    setAttribute(name, value) { if (name === 'aria-selected') this.aria = value; },
  });
  const first = makeRow('shift-a');
  const second = makeRow('shift-b');
  first.classList.owner = first;
  second.classList.owner = second;
  const rows = [first, second];
  const list = { querySelectorAll() { return rows; } };

  applyShiftListSelection(list, 'shift-b');
  assert.equal(first.selected, false);
  assert.equal(first.aria, 'false');
  assert.equal(second.selected, true);
  assert.equal(second.aria, 'true');
});

test('selection handler is pinned to detail-only updates and never calls the list renderer', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const body = js.match(/function selectShiftInPlace\([\s\S]*?\n\}/)?.[0] || '';
  assert.match(body, /applyShiftListSelection/);
  assert.match(body, /renderSelectedShiftDetail/);
  assert.doesNotMatch(body, /renderShiftList/);
  assert.match(body, /detailUserCollapsed = false/);
  const reveal = js.match(/function showInspectorSection\([\s\S]*?\n\}/)?.[0] || '';
  assert.match(reveal, /Boolean\(selectedShiftId \|\| selectedEmployeeId\) && !detailUserCollapsed/);
});

test('reactive data refreshes preserve the header, filterbar, and search input nodes', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const refresh = js.match(/function refreshPlanningSurfaces\([\s\S]*?\n\}/)?.[0] || '';
  const listRender = js.match(/function renderShiftList\([\s\S]*?\n\}/)?.[0] || '';
  assert.doesNotMatch(refresh, /host\.innerHTML|leftPane\.innerHTML|replaceChildren/);
  assert.match(listRender, /els\.shiftList\.innerHTML/);
  assert.doesNotMatch(listRender, /leftPane\.innerHTML|data-pg-search/);
});

test('conflict collection preserves overlap, rest, daily and assignment rules', () => {
  const hour = 60 * 60 * 1000;
  const monday = new Date(2026, 4, 25, 0, 0, 0, 0);
  const shifts = [
    { id: 'a', employee_id: 'emp-1', project_id: 'p1', start_time: monday.getTime() + 8 * hour, end_time: monday.getTime() + 17 * hour },
    { id: 'b', employee_id: 'emp-1', project_id: 'p2', start_time: monday.getTime() + 12 * hour, end_time: monday.getTime() + 20 * hour },
  ];
  const conflicts = collectPlanningConflicts(shifts, [{ id: 'emp-1', name: 'Alex', weekly_target_hours: 40 }], monday);
  assert.ok(conflicts.some((entry) => entry.type === 'overlap'));
  assert.ok(conflicts.some((entry) => entry.type === 'daily_hours'));
});

// v4-Vertrag (PORTING.md R4/R11): keine Readiness, kein Synchronisierungs- oder Leertext im Inhalt.
test('no readiness path and empty sources render no state text', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  assert.doesNotMatch(js, /ctox-syncing|ctox-empty|subscribeCollectionReadiness|collectionReadiness|dataDrivenEmptyHtml|ctx\.db|commandBus/);
  assert.equal('dataDrivenEmptyHtml' in hooks, false);
});

let passed = 0;
for (const entry of tests) {
  try {
    await entry.fn();
    passed += 1;
    console.log(`ok - ${entry.name}`);
  } catch (error) {
    console.error(`not ok - ${entry.name}`);
    throw error;
  }
}

console.log(`${passed} shiftflow tests passed`);
