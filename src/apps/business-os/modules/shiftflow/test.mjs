import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { __shiftflowTestHooks as hooks } from './index.js';

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
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
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
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));

  assert.match(html, /class="ctox-pane shiftflow-left-pane"[^>]*data-left-content/);
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-view="cards"[\s\S]*data-pg-view="list"/);
  assert.match(html, /data-pg-tray-toggle[\s\S]*data-pg-tray hidden/);
  assert.match(html, /data-pg-reset/);
  assert.equal((html.match(/data-pg-band=/g) || []).length, 2);
  assert.match(html, /class="ctox-pane-body ctox-well"/);
  assert.match(html, /class="ctox-pane-footer"/);
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
  assert.doesNotMatch(js, /window\.dispatchEvent|ctox-business-os-chat-submit/);
  assert.match(js, /markupUrl\.searchParams\.set\('v', MOD_BUILD\)/);
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
