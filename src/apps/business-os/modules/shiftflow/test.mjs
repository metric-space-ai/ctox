import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { __shiftflowTestHooks as hooks } from './index.js';

const {
  filterShiftflowEmployeesForPlanner,
  getShiftflowPressedState,
  getWeekBoundsMs,
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
    tabs: { scheduler: false, timesheets: true, billing: false },
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
  assert.match(css, /--shiftflow-radius:\s*var\(--control-radius\)/);
  assert.match(css, /--shiftflow-panel-radius:\s*var\(--surface-radius\)/);
  assert.match(css, /\.shiftflow-panel\s*\{[\s\S]*?box-shadow:\s*none;/);
  assert.match(css, /\.shiftflow-grid-cell\.drag-over\s*\{[\s\S]*?outline:\s*2px solid var\(--shiftflow-accent\)/);
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
