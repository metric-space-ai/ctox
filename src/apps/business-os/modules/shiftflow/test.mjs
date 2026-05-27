import assert from 'node:assert/strict';
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
