/* src/apps/business-os/modules/shiftflow/index.js */
import { loadModuleMessages } from '../../shared/i18n.js';
import { accumulateUeberlassung, checkDailyHours, checkRestPeriods } from './core/arbzg.js';

const MOD_BUILD = '20260722-ia-karte-v1';
const PLANNING_COLLECTIONS = Object.freeze([
  'planning_employees',
  'planning_projects',
  'planning_shifts',
  'planning_time_records',
  'planning_absences',
]);
const SEED_WRITE_COLLECTIONS = Object.freeze([
  'planning_employees',
  'planning_projects',
  'planning_shifts',
  'planning_time_records',
]);

let activeSubscriptions = [];
let currentWeekStart = getMondayOfCurrentWeek();
let currentView = 'scheduler'; // 'scheduler', 'timesheets', or 'billing'
let currentTimelineFocus = 'employees'; // 'employees' or 'projects'
let selectedShiftId = null;
let selectedEmployeeId = null;
let detailUserCollapsed = false;
let currentDeptFilter = 'all';
let shiftListState = { search: '', view: 'cards', band: 'week', filters: { department: 'all', status: 'all' } };
let latestEmployees = [];
let latestProjects = [];
let latestShifts = [];
let latestTimeRecords = [];
let latestConflicts = [];
let lang = 'de';
let t = (key, fallback) => fallback ?? key;

function shiftflowCollection(ctx, name) {
  const facade = ctx?.db;
  if (!facade || !name) return null;
  return facade.collection?.(name) || null;
}

function shiftflowDb(ctx) {
  const entries = PLANNING_COLLECTIONS.map((name) => [name, shiftflowCollection(ctx, name)]);
  if (entries.some(([, collection]) => !collection)) return null;
  return Object.fromEntries(entries);
}

function canReadCollection(ctx, name) {
  const permissionCheck = ctx?.permissions?.canReadCollection;
  return typeof permissionCheck !== 'function' || permissionCheck(name) === true;
}

function canWriteCollection(ctx, name) {
  const permissionCheck = ctx?.permissions?.canWriteCollection;
  return typeof permissionCheck !== 'function' || permissionCheck(name) === true;
}

function canWriteSeedData(ctx) {
  return SEED_WRITE_COLLECTIONS.every((name) => canWriteCollection(ctx, name));
}

export async function mount(ctx) {
  lang = ctx.locale === 'en' ? 'en' : 'de';
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, {});
  t = (key, fallback, ...args) => {
    let val = messages[key] ?? fallback ?? key;
    if (args.length) {
      args.forEach((arg, i) => {
        val = val.replace(`{${i}}`, arg);
      });
    }
    return val;
  };

  await ensureStyles();

  // Load markup
  ctx.host.innerHTML = await loadModuleMarkup();
  applyStaticLabels(ctx.host, t);
  ctx.host.dataset.shiftflowModule = 'native';

  // Static chrome is mounted once. Reactive refreshes replace only list/board
  // data nodes so search focus and the shell-owned pane grammar stay intact.
  const els = {
    app: ctx.host.querySelector('.shiftflow-app'),
    leftPane: ctx.host.querySelector('#shiftflow-left'),
    shiftList: ctx.host.querySelector('[data-shift-list]'),
    addShiftBtn: ctx.host.querySelector('#addShiftBtn'),
    importShiftsBtn: ctx.host.querySelector('#importShiftsBtn'),
    exportShiftsBtn: ctx.host.querySelector('#exportShiftsBtn'),
    shiftImportInput: ctx.host.querySelector('#shiftImportInput'),
    schedulerView: ctx.host.querySelector('#schedulerView'),
    conflictsView: ctx.host.querySelector('#conflictsView'),
    timesheetsView: ctx.host.querySelector('#timesheetsView'),
    billingView: ctx.host.querySelector('#billingView'),
    toggleViewEmployeesBtn: ctx.host.querySelector('#toggleViewEmployeesBtn'),
    toggleViewProjectsBtn: ctx.host.querySelector('#toggleViewProjectsBtn'),
    schedulerCornerCell: ctx.host.querySelector('#schedulerCornerCell'),
    schedulerGridHeader: ctx.host.querySelector('#schedulerGridHeader'),
    schedulerGridBody: ctx.host.querySelector('#schedulerGridBody'),
    schedulerWeekRange: ctx.host.querySelector('#schedulerWeekRange'),
    centerPaneTitle: ctx.host.querySelector('#centerPaneTitle'),
    prevWeekBtn: ctx.host.querySelector('#prevWeekBtn'),
    nextWeekBtn: ctx.host.querySelector('#nextWeekBtn'),
    approveAllTimesheetsBtn: ctx.host.querySelector('#approveAllTimesheetsBtn'),
    timesheetsList: ctx.host.querySelector('#timesheetsList'),
    btnPublishSchedule: ctx.host.querySelector('#btnPublishSchedule'),
    publishScheduleStatus: ctx.host.querySelector('#publishScheduleStatus'),
    btnAutoGenerateSchedule: ctx.host.querySelector('#btnAutoGenerateSchedule'),
    btnCheckConflicts: ctx.host.querySelector('#btnCheckConflicts'),
    conflictsList: ctx.host.querySelector('#conflictsList'),
    detailInspectorSection: ctx.host.querySelector('#detailInspectorSection'),
    closeInspectorBtn: ctx.host.querySelector('#closeInspectorBtn'),
    inspectorContent: ctx.host.querySelector('#inspectorContent'),
    inspectorTitle: ctx.host.querySelector('#inspectorTitle'),
    billingStartDate: ctx.host.querySelector('#billingStartDate'),
    billingEndDate: ctx.host.querySelector('#billingEndDate'),
    billingFilterApplyBtn: ctx.host.querySelector('#billingFilterApplyBtn'),
    exportInvoiceDraftBtn: ctx.host.querySelector('#exportInvoiceDraftBtn'),
    billingTotalRevenue: ctx.host.querySelector('#billingTotalRevenue'),
    billingTotalCost: ctx.host.querySelector('#billingTotalCost'),
    billingTotalMargin: ctx.host.querySelector('#billingTotalMargin'),
    billingAggregationBody: ctx.host.querySelector('#billingAggregationBody'),
    viewSchedulerTabBtn: ctx.host.querySelector('#viewSchedulerTabBtn'),
    viewConflictsTabBtn: ctx.host.querySelector('#viewConflictsTabBtn'),
    viewTimesheetsTabBtn: ctx.host.querySelector('#viewTimesheetsTabBtn'),
    viewBillingTabBtn: ctx.host.querySelector('#viewBillingTabBtn'),
  };

  applyActionIcons(ctx, els);

  // Seed default dates in Billing selector (current month)
  const today = new Date();
  const firstDay = new Date(today.getFullYear(), today.getMonth(), 1);
  const lastDay = new Date(today.getFullYear(), today.getMonth() + 1, 0);
  els.billingStartDate.value = firstDay.toISOString().split('T')[0];
  els.billingEndDate.value = lastDay.toISOString().split('T')[0];

  currentWeekStart = getMondayOfCurrentWeek();
  currentView = 'scheduler';
  currentTimelineFocus = 'employees';
  currentDeptFilter = 'all';
  selectedShiftId = null;
  selectedEmployeeId = null;
  detailUserCollapsed = false;
  shiftListState = { search: '', view: 'cards', band: 'week', filters: { department: 'all', status: 'all' } };
  latestEmployees = [];
  latestProjects = [];
  latestShifts = [];
  latestTimeRecords = [];
  latestConflicts = [];

  // Setup reactive RxDB subscriptions
  setupSubscriptions(ctx, els);

  // Bind Event Listeners
  bindEventListeners(ctx, els);

  // Column resizing is owned by the shell: the `.ctox-column-resizer
  // [data-resizer-var]` handles in index.html inside the `[data-resize-frame]`
  // root get drag/keyboard/persistence from the shell resizer for free.

  // Initial UI updates
  updateWeekRangeDisplay(els);
  renderGridHeader(els);
  applyCenterViewState(els);
  applyTimelineState(els);

  // Seeding is optional bootstrap work, not a prerequisite for mounting the
  // planner. The reactive queries above paint both the empty and populated
  // states and will pick up inserted seed records as they arrive.
  let disposed = false;
  Promise.resolve()
    .then(() => seedMockDataIfEmpty(ctx))
    .catch((error) => {
      if (!disposed) console.error('[shiftflow] seed failed:', error);
    });

  // 5. Initialize CTOX unified context menu

  // Return unmount function
  return () => {
    disposed = true;
    activeSubscriptions.forEach(sub => sub.unsubscribe?.());
    activeSubscriptions = [];
    ctx.host.replaceChildren();
    delete ctx.host.dataset.shiftflowModule;
  };
}

// -------------------------------------------------------------
// Core UI Setup & Helper Methods
// -------------------------------------------------------------

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${MOD_BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

async function loadModuleMarkup() {
  const markupUrl = new URL('./index.html', import.meta.url);
  markupUrl.searchParams.set('v', MOD_BUILD);
  const html = await fetch(markupUrl).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function applyActionIcons(ctx, els) {
  if (typeof ctx?.getActionIcon !== 'function') return;
  const icons = [
    [els.addShiftBtn, 'add'],
    [els.importShiftsBtn, 'download'],
    [els.exportShiftsBtn, 'export'],
    [els.btnAutoGenerateSchedule, 'play'],
    [els.prevWeekBtn, 'chevronLeft'],
    [els.nextWeekBtn, 'chevronRight'],
    [els.closeInspectorBtn, 'close'],
  ];
  for (const [button, name] of icons) {
    const glyph = button ? ctx.getActionIcon(name) : '';
    if (glyph) button.innerHTML = glyph;
  }
}

function getMondayOfCurrentWeek(date = new Date()) {
  const day = date.getDay();
  const diff = date.getDate() - day + (day === 0 ? -6 : 1); // adjust when day is sunday
  const monday = new Date(date.setDate(diff));
  monday.setHours(0, 0, 0, 0);
  return monday;
}

function updateWeekRangeDisplay(els) {
  const monday = new Date(currentWeekStart);
  const sunday = new Date(monday);
  sunday.setDate(monday.getDate() + 6);

  const options = { day: '2-digit', month: '2-digit', year: 'numeric' };
  const localeStr = lang === 'en' ? 'en-US' : 'de-DE';
  const prefix = lang === 'en' ? 'W' : 'KW';
  els.schedulerWeekRange.textContent = `${prefix} ${getWeekNumber(monday)}: ${monday.toLocaleDateString(localeStr, options)} – ${sunday.toLocaleDateString(localeStr, options)}`;
}

function getWeekNumber(d) {
  d = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  d.setUTCDate(d.getUTCDate() + 4 - (d.getUTCDay() || 7));
  const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
  const weekNo = Math.ceil((((d - yearStart) / 86400000) + 1) / 7);
  return weekNo;
}

function renderGridHeader(els) {
  const days = [
    t('dayMonday', 'Montag'),
    t('dayTuesday', 'Dienstag'),
    t('dayWednesday', 'Mittwoch'),
    t('dayThursday', 'Donnerstag'),
    t('dayFriday', 'Freitag'),
    t('daySaturday', 'Samstag'),
    t('daySunday', 'Sonntag')
  ];

  if (currentTimelineFocus === 'employees') {
    els.schedulerCornerCell.textContent = t('employees', 'Mitarbeiter');
  } else {
    els.schedulerCornerCell.textContent = t('projects', 'Projekte');
  }

  const baseHeader = `<div class="shiftflow-grid-corner-cell">${escapeHtml(els.schedulerCornerCell.textContent)}</div>`;
  const monday = new Date(currentWeekStart);
  const todayStr = new Date().toDateString();

  const dayCells = days.map((day, index) => {
    const currentDay = new Date(monday);
    currentDay.setDate(monday.getDate() + index);
    const isToday = currentDay.toDateString() === todayStr;

    return `
      <div class="grid-day-cell ${isToday ? 'today' : ''}">
        <div>${day}</div>
        <div class="day-number">${currentDay.getDate()}</div>
      </div>
    `;
  }).join('');

  els.schedulerGridHeader.innerHTML = baseHeader + dayCells;
}

export function filterShiftflowEmployeesForPlanner(employees, { department = 'all', search = '' } = {}) {
  const query = String(search || '').toLowerCase().trim();
  return employees.filter((emp) => {
    const matchesDept = department === 'all' || emp.departments?.includes(department);
    if (!matchesDept) return false;
    if (!query) return true;
    const haystack = `${emp.name || ''} ${emp.role || ''} ${(emp.departments || []).join(' ')}`.toLowerCase();
    return haystack.includes(query);
  });
}

export function getShiftflowPressedState(view = 'scheduler', timelineFocus = 'employees') {
  return {
    tabs: {
      scheduler: view === 'scheduler',
      conflicts: view === 'conflicts',
      timesheets: view === 'timesheets',
      billing: view === 'billing'
    },
    timeline: {
      employees: timelineFocus === 'employees',
      projects: timelineFocus === 'projects'
    }
  };
}

export function getWeekBoundsMs(weekStart) {
  const start = new Date(weekStart);
  start.setHours(0, 0, 0, 0);
  const end = new Date(start);
  end.setDate(start.getDate() + 7);
  return { startMs: start.getTime(), endMs: end.getTime() - 1 };
}

function getEmployeeSearchQuery() {
  return String(shiftListState.search || '').trim();
}

function setPressedState(button, pressed) {
  if (!button) return;
  button.classList.toggle('active', pressed);
  button.classList.toggle('is-active', pressed);
  if (button.getAttribute('role') === 'tab') button.setAttribute('aria-selected', String(Boolean(pressed)));
  else button.setAttribute('aria-pressed', String(Boolean(pressed)));
}

function applyCenterViewState(els) {
  const state = getShiftflowPressedState(currentView, currentTimelineFocus);
  setPressedState(els.viewSchedulerTabBtn, state.tabs.scheduler);
  setPressedState(els.viewConflictsTabBtn, state.tabs.conflicts);
  setPressedState(els.viewTimesheetsTabBtn, state.tabs.timesheets);
  setPressedState(els.viewBillingTabBtn, state.tabs.billing);

  els.schedulerView.classList.toggle('hidden', !state.tabs.scheduler);
  els.conflictsView.classList.toggle('hidden', !state.tabs.conflicts);
  els.timesheetsView.classList.toggle('hidden', !state.tabs.timesheets);
  els.billingView.classList.toggle('hidden', !state.tabs.billing);

  if (state.tabs.scheduler) {
    els.centerPaneTitle.textContent = t('schedulePlanning', 'Einsatzplanung');
  } else if (state.tabs.conflicts) {
    els.centerPaneTitle.textContent = t('conflictsAndWarnings', 'Konflikte & Warnungen');
  } else if (state.tabs.timesheets) {
    els.centerPaneTitle.textContent = t('tabTimesheets', 'Zeiterfassung');
  } else {
    els.centerPaneTitle.textContent = t('billingTitle', 'Leistungsabrechnung & Aggregation');
  }
}

function applyTimelineState(els) {
  const state = getShiftflowPressedState(currentView, currentTimelineFocus);
  setPressedState(els.toggleViewEmployeesBtn, state.timeline.employees);
  setPressedState(els.toggleViewProjectsBtn, state.timeline.projects);
}

function showInspectorSection(els) {
  const visible = Boolean(selectedShiftId || selectedEmployeeId) && !detailUserCollapsed;
  els.detailInspectorSection.hidden = !visible;
  els.detailInspectorSection.classList.toggle('is-open', visible);
  els.detailInspectorSection.parentElement?.classList.toggle('has-detail', visible);
}

function hideInspectorSection(els) {
  detailUserCollapsed = true;
  showInspectorSection(els);
}

function setPublishStatus(els, message = '') {
  if (els.publishScheduleStatus) {
    els.publishScheduleStatus.textContent = message;
  }
}

// -------------------------------------------------------------
// Database Operations & Data Seeding
// -------------------------------------------------------------

async function seedMockDataIfEmpty(ctx) {
  if (!canReadCollection(ctx, 'planning_employees') || !canWriteSeedData(ctx)) return;
  const db = shiftflowDb(ctx);
  if (!db) return;

  const empCount = await db.planning_employees.find().exec();
  if (empCount.length > 0) return; // already has data

  console.log('[shiftflow] Seeding advanced mock database records...');

  // 1. Seed Projects
  const mockProjects = [
    {
      id: 'proj_intersolar',
      kind: 'project',
      name: 'Intersolar Standbau',
      client: 'Messe München GmbH',
      location: 'Halle A5, Stand 120',
      hourly_rate: 85.00,
      color: '#06b6d4',
      status: 'active',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'proj_catering',
      kind: 'project',
      name: 'Sommerfest Catering',
      client: 'Allianz Arena AG',
      location: 'VIP Lounge',
      hourly_rate: 95.00,
      color: '#22c55e',
      status: 'active',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'proj_office',
      kind: 'project',
      name: 'Office Administration',
      client: 'CTOX GmbH',
      location: 'Hauptbüro',
      hourly_rate: 0.00, // non-billable internal work
      color: '#6366f1',
      status: 'active',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    }
  ];

  for (const proj of mockProjects) {
    await db.planning_projects.insert(proj);
  }

  // 2. Seed Employees with internal hourly wage rate
  const mockEmployees = [
    {
      id: 'emp_michael',
      kind: 'employee',
      name: 'Michael Welsch',
      email: 'michael.welsch@ctox.dev',
      role: 'Service & Barista',
      weekly_target_hours: 40,
      status: 'active',
      avatar_color: 'hsl(250, 70%, 50%)',
      internal_hourly_rate: 28.50,
      departments: ['Service', 'Bar'],
      skills: ['Barista-certified', 'POS-operator'],
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'emp_lisa',
      kind: 'employee',
      name: 'Lisa Schmidt',
      email: 'lisa.schmidt@ctox.dev',
      role: 'Serviceleitung',
      weekly_target_hours: 40,
      status: 'active',
      avatar_color: 'hsl(168, 80%, 40%)',
      internal_hourly_rate: 35.00,
      departments: ['Service', 'Verwaltung'],
      skills: ['Shift-lead', 'Complaints-handling'],
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'emp_christian',
      kind: 'employee',
      name: 'Christian Meyer',
      email: 'christian.meyer@ctox.dev',
      role: 'Küchenchef',
      weekly_target_hours: 40,
      status: 'active',
      avatar_color: 'hsl(30, 90%, 50%)',
      internal_hourly_rate: 32.00,
      departments: ['Küche'],
      skills: ['HACCP-certified', 'Menu-designer'],
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'emp_sarah',
      kind: 'employee',
      name: 'Sarah Becker',
      email: 'sarah.becker@ctox.dev',
      role: 'Verwaltung & Admin',
      weekly_target_hours: 20,
      status: 'active',
      avatar_color: 'hsl(200, 80%, 45%)',
      internal_hourly_rate: 24.00,
      departments: ['Verwaltung'],
      skills: ['Payroll', 'Office-mgmt'],
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    }
  ];

  for (const emp of mockEmployees) {
    await db.planning_employees.insert(emp);
  }

  // Seed standard planned shifts for the current week linked to projects
  const monday = getMondayOfCurrentWeek();

  const getTimestamp = (dayOffset, hourStr) => {
    const d = new Date(monday);
    d.setDate(monday.getDate() + dayOffset);
    const [h, m] = hourStr.split(':').map(Number);
    d.setHours(h, m, 0, 0);
    return d.getTime();
  };

  const mockShifts = [
    // Monday
    {
      id: 'shift_1',
      kind: 'shift',
      employee_id: 'emp_lisa',
      project_id: 'proj_intersolar',
      title: 'Bauleitung Intersolar',
      start_time: getTimestamp(0, '08:00'),
      end_time: getTimestamp(0, '16:00'),
      location: 'Intersolar Standbau',
      department: 'Service',
      status: 'published',
      notes: 'Schichtleitung übernehmen',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'shift_2',
      kind: 'shift',
      employee_id: 'emp_michael',
      project_id: 'proj_catering',
      title: 'Service Sommerfest',
      start_time: getTimestamp(0, '16:00'),
      end_time: getTimestamp(0, '23:30'),
      location: 'Sommerfest Catering',
      department: 'Bar',
      status: 'published',
      notes: 'Closing erledigen',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'shift_3',
      kind: 'shift',
      employee_id: 'emp_christian',
      project_id: 'proj_catering',
      title: 'Küchenleitung Catering',
      start_time: getTimestamp(0, '11:00'),
      end_time: getTimestamp(0, '20:00'),
      location: 'Sommerfest Catering',
      department: 'Küche',
      status: 'published',
      notes: 'Menüabstimmung vor Ort',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    // Tuesday
    {
      id: 'shift_4',
      kind: 'shift',
      employee_id: 'emp_michael',
      project_id: 'proj_intersolar',
      title: 'Standbetreuung Kaffeespezialitäten',
      start_time: getTimestamp(1, '08:00'),
      end_time: getTimestamp(1, '16:00'),
      location: 'Intersolar Standbau',
      department: 'Bar',
      status: 'published',
      notes: 'Barista Service',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    {
      id: 'shift_5',
      kind: 'shift',
      employee_id: 'emp_lisa',
      project_id: 'proj_office',
      title: 'Dienstplanerstellung',
      start_time: getTimestamp(1, '09:00'),
      end_time: getTimestamp(1, '17:00'),
      location: 'Office Administration',
      department: 'Verwaltung',
      status: 'published',
      notes: 'Dienstplanvorbereitung im Büro',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    },
    // Wednesday
    {
      id: 'shift_6',
      kind: 'shift',
      employee_id: 'emp_sarah',
      project_id: 'proj_office',
      title: 'Backoffice Buchhaltung',
      start_time: getTimestamp(2, '09:00'),
      end_time: getTimestamp(2, '14:00'),
      location: 'Office Administration',
      department: 'Verwaltung',
      status: 'published',
      notes: 'Rechnungswesen',
      created_at_ms: Date.now(),
      updated_at_ms: Date.now()
    }
  ];

  for (const shift of mockShifts) {
    await db.planning_shifts.insert(shift);
  }

  // Seed completed / active timesheets

  // 1. Active clock-in (Michael is currently working at Intersolar)
  const activeRecordStart = new Date();
  activeRecordStart.setHours(activeRecordStart.getHours() - 3); // clocked in 3 hours ago

  await db.planning_time_records.insert({
    id: 'tr_michael_active',
    kind: 'time_record',
    employee_id: 'emp_michael',
    project_id: 'proj_intersolar',
    start_time: activeRecordStart.getTime(),
    end_time: null,
    breaks: [],
    notes: 'Kaffeebar Schicht gestartet',
    approval_status: 'pending',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  // 2. Completed timesheet needing approval
  const completedRecordStart = new Date(monday);
  completedRecordStart.setDate(monday.getDate() - 1); // Sunday
  completedRecordStart.setHours(8, 0, 0, 0);
  const completedRecordEnd = new Date(completedRecordStart);
  completedRecordEnd.setHours(16, 30, 0, 0); // 8.5 hours

  await db.planning_time_records.insert({
    id: 'tr_lisa_approval',
    kind: 'time_record',
    employee_id: 'emp_lisa',
    project_id: 'proj_intersolar',
    start_time: completedRecordStart.getTime(),
    end_time: completedRecordEnd.getTime(),
    breaks: [{ start: completedRecordStart.getTime() + 4 * 3600000, end: completedRecordStart.getTime() + 4.5 * 3600000 }], // 30 min break
    notes: 'Aufbauarbeiten Intersolar',
    approval_status: 'pending',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  // 3. Seed historical APPROVED records so billing aggregation shows value immediately
  const histStart1 = new Date(monday);
  histStart1.setDate(monday.getDate() - 5); // Wednesday last week
  histStart1.setHours(9, 0, 0, 0);
  const histEnd1 = new Date(histStart1);
  histEnd1.setHours(17, 0, 0, 0); // 8 hours

  await db.planning_time_records.insert({
    id: 'tr_hist_michael',
    kind: 'time_record',
    employee_id: 'emp_michael',
    project_id: 'proj_catering',
    start_time: histStart1.getTime(),
    end_time: histEnd1.getTime(),
    breaks: [],
    notes: 'Sommerfest Catering Allianz Arena',
    approval_status: 'approved',
    approved_by: 'manager',
    billing_status: 'uninvoiced',
    billing_rate_applied: 95.00,
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  const histStart2 = new Date(monday);
  histStart2.setDate(monday.getDate() - 4); // Thursday last week
  histStart2.setHours(8, 0, 0, 0);
  const histEnd2 = new Date(histStart2);
  histEnd2.setHours(16, 0, 0, 0); // 8 hours

  await db.planning_time_records.insert({
    id: 'tr_hist_lisa',
    kind: 'time_record',
    employee_id: 'emp_lisa',
    project_id: 'proj_intersolar',
    start_time: histStart2.getTime(),
    end_time: histEnd2.getTime(),
    breaks: [],
    notes: 'Kundenberatung Messe',
    approval_status: 'approved',
    approved_by: 'manager',
    billing_status: 'uninvoiced',
    billing_rate_applied: 85.00,
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });
}

// -------------------------------------------------------------
// Live Observables & UI Subscriptions
// -------------------------------------------------------------

function setupSubscriptions(ctx, els) {
  const db = shiftflowDb(ctx);
  if (!db || !PLANNING_COLLECTIONS.every((name) => canReadCollection(ctx, name))) return;

  const refresh = () => refreshPlanningSurfaces(els, ctx);
  activeSubscriptions.push(db.planning_employees.find().$.subscribe((employees) => {
    latestEmployees = employees || [];
    refresh();
  }));
  activeSubscriptions.push(db.planning_projects.find().$.subscribe((projects) => {
    latestProjects = projects || [];
    refresh();
  }));
  activeSubscriptions.push(db.planning_shifts.find().$.subscribe((shifts) => {
    latestShifts = shifts || [];
    if (selectedShiftId && !latestShifts.some((shift) => shift.id === selectedShiftId)) {
      selectedShiftId = null;
      detailUserCollapsed = false;
    }
    refresh();
  }));
  activeSubscriptions.push(db.planning_time_records.find().$.subscribe((records) => {
    latestTimeRecords = records || [];
    refresh();
  }));
}

function refreshPlanningSurfaces(els, ctx) {
  renderShiftList(els);
  renderSchedulerGrid(latestEmployees, latestProjects, latestShifts, els, ctx);
  renderTimesheets(latestEmployees, latestProjects, latestShifts, latestTimeRecords, els, ctx);
  renderBillingAggregation(latestEmployees, latestProjects, latestTimeRecords, els, ctx);
  latestConflicts = collectPlanningConflicts(latestShifts, latestEmployees, currentWeekStart);
  renderConflicts(latestConflicts, els);
  renderMainViewCounts(els);
  renderSelectedShiftDetail(els, ctx);
}

// -------------------------------------------------------------
// Renders
// -------------------------------------------------------------

function shiftListBandCounts(shifts = latestShifts) {
  const { startMs, endMs } = getWeekBoundsMs(currentWeekStart);
  return {
    week: shifts.filter((shift) => shift.start_time >= startMs && shift.start_time <= endMs).length,
    drafts: shifts.filter((shift) => shift.status === 'draft').length,
  };
}

function visibleShiftListRecords() {
  const { startMs, endMs } = getWeekBoundsMs(currentWeekStart);
  const query = String(shiftListState.search || '').trim().toLowerCase();
  const department = shiftListState.filters?.department || 'all';
  const status = shiftListState.filters?.status || 'all';
  const employeesById = new Map(latestEmployees.map((employee) => [employee.id, employee]));
  const projectsById = new Map(latestProjects.map((project) => [project.id, project]));

  return latestShifts
    .filter((shift) => shiftListState.band === 'drafts'
      ? shift.status === 'draft'
      : shift.start_time >= startMs && shift.start_time <= endMs)
    .filter((shift) => department === 'all' || shift.department === department)
    .filter((shift) => status === 'all' || shift.status === status)
    .filter((shift) => {
      if (!query) return true;
      const employee = employeesById.get(shift.employee_id);
      const project = projectsById.get(shift.project_id);
      return `${shift.title || ''} ${shift.department || ''} ${employee?.name || ''} ${employee?.role || ''} ${project?.name || ''} ${project?.client || ''}`.toLowerCase().includes(query);
    })
    .sort((a, b) => Number(a.start_time || 0) - Number(b.start_time || 0));
}

function renderShiftList(els) {
  if (!els.shiftList || !els.leftPane) return;
  const records = visibleShiftListRecords();
  const previousScrollTop = els.shiftList.parentElement?.scrollTop || 0;
  els.shiftList.classList.toggle('is-cards', shiftListState.view !== 'list');
  els.shiftList.classList.toggle('is-list', shiftListState.view === 'list');
  els.shiftList.innerHTML = records.map((shift) => renderShiftListItem(shift)).join('')
    || `<div class="ctox-empty">${escapeHtml(t('noShiftsForView', 'Keine Schichten für diese Ansicht.'))}</div>`;
  if (previousScrollTop && els.shiftList.parentElement) els.shiftList.parentElement.scrollTop = previousScrollTop;
  renderShiftListCountsAndFooter(els.leftPane, records.length);
}

function renderShiftListItem(shift) {
  const employee = latestEmployees.find((item) => item.id === shift.employee_id);
  const project = latestProjects.find((item) => item.id === shift.project_id);
  const start = new Date(shift.start_time);
  const end = new Date(shift.end_time);
  const locale = lang === 'en' ? 'en-US' : 'de-DE';
  const date = start.toLocaleDateString(locale, { weekday: 'short', day: '2-digit', month: '2-digit' });
  const title = shift.title || project?.name || t('shift', 'Dienst');
  const meta = `${date} · ${formatTime(start)}–${formatTime(end)} · ${employee?.name || t('unassigned', 'Unbesetzt')} · ${project?.name || shift.location || t('noProject', 'Ohne Projekt')}`;
  const selected = shift.id === selectedShiftId;
  return `
    <article class="ctox-list-item shiftflow-shift-item${selected ? ' is-selected' : ''}" role="option" aria-selected="${selected}" data-shift-list-id="${escapeHtml(shift.id)}" data-context-record-id="${escapeHtml(shift.id)}" data-context-record-type="planning_shift" data-context-label="${escapeHtml(title)}">
      <button type="button" class="shiftflow-shift-select" data-select-shift-id="${escapeHtml(shift.id)}" aria-label="${escapeHtml(`${t('openShift', 'Schicht öffnen')}: ${title}`)}">
        <strong>${escapeHtml(title)}</strong>
        <span>${escapeHtml(meta)}</span>
      </button>
      <button type="button" class="ctox-pane-icon" data-edit-shift-id="${escapeHtml(shift.id)}" title="${escapeHtml(t('edit', 'Bearbeiten'))}" aria-label="${escapeHtml(`${t('edit', 'Bearbeiten')}: ${title}`)}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h4l11-11-4-4L4 16v4z"/><path d="M13.5 6.5l4 4"/></svg></button>
    </article>
  `;
}

function renderShiftListCountsAndFooter(pane, visibleCount) {
  const counts = shiftListBandCounts();
  const bandLabel = shiftListState.band === 'drafts' ? t('viewDrafts', 'Entwürfe') : t('viewWeek', 'Woche');
  const footer = `${visibleCount} ${t('entries', 'Einträge')} · ${bandLabel}`;
  const pg = pane.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(counts);
  else for (const [key, value] of Object.entries(counts)) {
    const node = pane.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
  if (pg?.setFooter) pg.setFooter(footer);
  else {
    const node = pane.querySelector('[data-pg-footer]');
    if (node) node.textContent = footer;
  }
}

export function applyShiftListSelection(list, selectedId) {
  if (!list?.querySelectorAll) return;
  list.querySelectorAll('[data-shift-list-id]').forEach((row) => {
    const selected = String(row.dataset.shiftListId || '') === String(selectedId || '');
    row.classList.toggle('is-selected', selected);
    row.setAttribute('aria-selected', String(selected));
  });
}

function applyBoardShiftSelection(els) {
  els.schedulerGridBody?.querySelectorAll?.('[data-shift-id]').forEach((card) => {
    const selected = String(card.dataset.shiftId || '') === String(selectedShiftId || '');
    card.classList.toggle('is-selected', selected);
    card.setAttribute('aria-pressed', String(selected));
  });
}

function selectShiftInPlace(shiftId, els, ctx) {
  selectedShiftId = String(shiftId || '') || null;
  selectedEmployeeId = null;
  detailUserCollapsed = false;
  applyShiftListSelection(els.shiftList, selectedShiftId);
  applyBoardShiftSelection(els);
  currentView = 'scheduler';
  applyCenterViewState(els);
  renderSelectedShiftDetail(els, ctx);
}

function selectEmployeeInPlace(employeeId, els, ctx) {
  selectedEmployeeId = String(employeeId || '') || null;
  selectedShiftId = null;
  detailUserCollapsed = false;
  applyShiftListSelection(els.shiftList, null);
  applyBoardShiftSelection(els);
  currentView = 'scheduler';
  applyCenterViewState(els);
  renderSelectedShiftDetail(els, ctx);
}

function renderSelectedShiftDetail(els, ctx) {
  const shift = latestShifts.find((item) => item.id === selectedShiftId);
  if (shift) {
    void openShiftDetails(shift.id, latestShifts, latestEmployees, els, ctx);
    return;
  }
  selectedShiftId = null;
  if (selectedEmployeeId) {
    void openEmployeeDetailsInspector(selectedEmployeeId, latestEmployees, els, ctx);
    return;
  }
  showInspectorSection(els);
}

function renderMainViewCounts(els) {
  const { startMs, endMs } = getWeekBoundsMs(currentWeekStart);
  const counts = {
    scheduler: latestShifts.filter((shift) => shift.start_time >= startMs && shift.start_time <= endMs).length,
    conflicts: latestConflicts.length,
    timesheets: latestTimeRecords.filter((record) => record.approval_status === 'pending' && record.end_time !== null).length,
    billing: latestTimeRecords.filter((record) => record.approval_status === 'approved' && record.end_time !== null).length,
  };
  for (const [key, value] of Object.entries(counts)) {
    const node = els.app?.querySelector(`[data-main-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function renderEmployeesList(employees, timeRecords, els, ctx) {
  const filtered = filterShiftflowEmployeesForPlanner(employees, {
    department: currentDeptFilter,
    search: getEmployeeSearchQuery(els)
  });

  const activeEmpIds = new Set(
    timeRecords
      .filter(rec => rec.end_time === null)
      .map(rec => rec.employee_id)
  );

  const activeSection = [];
  const inactiveSection = [];

  filtered.forEach(emp => {
    const initials = emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase();
    const isActive = activeEmpIds.has(emp.id);

    const cardHtml = `
      <div class="ctox-list-item employee-card ${isActive ? 'active' : ''}" data-emp-id="${escapeHtml(emp.id)}" data-context-record-id="${escapeHtml(emp.id)}" data-context-record-type="planning_employee" data-context-label="${escapeHtml(emp.name || emp.id)}" draggable="true">
        <div class="ctox-avatar emp-avatar" style="background: ${emp.avatar_color || 'var(--accent)'}">${initials}</div>
        <div class="emp-info">
          <div class="emp-name">${escapeHtml(emp.name)}</div>
          <div class="emp-meta">${escapeHtml(emp.role)}</div>
        </div>
      </div>
    `;

    if (isActive) {
      activeSection.push(cardHtml);
    } else {
      inactiveSection.push(cardHtml);
    }
  });

  els.activeEmployeeList.innerHTML = activeSection.length ? activeSection.join('') : '<div class="ctox-empty">Niemand im Dienst</div>';
  els.inactiveEmployeeList.innerHTML = inactiveSection.length ? inactiveSection.join('') : '<div class="ctox-empty">Keine weiteren Mitarbeiter</div>';

  els.activeEmployeesCount.textContent = activeSection.length;
  els.inactiveEmployeesCount.textContent = inactiveSection.length;

  // Bind employee selection / detail triggers
  const cards = [...els.activeEmployeeList.querySelectorAll('.employee-card'), ...els.inactiveEmployeeList.querySelectorAll('.employee-card')];
  cards.forEach(card => {
    card.addEventListener('click', (e) => {
      e.stopPropagation();
      const emp = employees.find(e => e.id === card.dataset.empId);
      if (emp) openEmployeeDrawer(emp, els, ctx);
    });

    card.addEventListener('dblclick', (e) => {
      e.stopPropagation();
      const emp = employees.find(e => e.id === card.dataset.empId);
      if (emp) openEmployeeDrawer(emp, els, ctx);
    });

    // Setup HTML5 Drag and Drop triggers
    card.addEventListener('dragstart', (e) => {
      e.dataTransfer.setData('text/plain', card.dataset.empId);
      card.classList.add('is-dragging');
    });
    card.addEventListener('dragend', () => {
      card.classList.remove('is-dragging');
    });
  });
}

function renderProjectsList(projects, els, ctx) {
  const projectCards = projects.map(proj => {
    const activeBadge = proj.status === 'active' ? `<span class="project-card-badge" style="background:${proj.color || 'var(--accent)'};"></span>` : '';

    return `
      <div class="ctox-list-item project-card" data-proj-id="${escapeHtml(proj.id)}" data-context-record-id="${escapeHtml(proj.id)}" data-context-record-type="planning_project" data-context-label="${escapeHtml(proj.name || proj.title || proj.id)}">
        <div class="project-card-info">
          <div class="project-card-name">${escapeHtml(proj.name)}</div>
          <div class="project-card-client">${escapeHtml(proj.client)} · ${escapeHtml(proj.location || '')}</div>
        </div>
        <div class="project-card-meta">
          ${activeBadge}
          <span class="ctox-badge">${proj.hourly_rate.toFixed(2)} €/h</span>
        </div>
      </div>
    `;
  }).join('');

  els.projectList.innerHTML = projectCards || '<div class="ctox-empty">Keine Projekte angelegt</div>';

  // Bind edit dialog click listeners
  els.projectList.querySelectorAll('.project-card').forEach(card => {
    card.addEventListener('click', () => {
      const proj = projects.find(p => p.id === card.dataset.projId);
      if (proj) openProjectDrawer(proj, els, ctx);
    });
  });
}

function renderSchedulerGrid(employees, projects, shifts, els, ctx) {
  const monday = new Date(currentWeekStart);
  const searchQuery = getEmployeeSearchQuery(els);
  const employeesMatchingSearch = filterShiftflowEmployeesForPlanner(employees, {
    department: 'all',
    search: searchQuery
  });
  const visibleEmployeeIdsBySearch = new Set(employeesMatchingSearch.map(emp => emp.id));

  if (currentTimelineFocus === 'employees') {
    // 1. Classic Employee-Centric Timeline View
    const filteredEmployees = filterShiftflowEmployeesForPlanner(employees, {
      department: currentDeptFilter,
      search: searchQuery
    });

    const rows = filteredEmployees.map(emp => {
      const initials = emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase();

      const rowHeader = `
        <button type="button" class="row-employee-cell" data-open-employee-id="${escapeHtml(emp.id)}" draggable="true" aria-label="${escapeHtml(`${t('employeeDetails', 'Mitarbeiter-Details')}: ${emp.name}`)}">
          <div class="ctox-avatar emp-avatar" style="background: ${emp.avatar_color || 'var(--accent)'}">${initials}</div>
          <div class="emp-info">
            <div class="emp-name">${escapeHtml(emp.name)}</div>
            <div class="emp-meta">${escapeHtml(emp.role)}</div>
          </div>
        </button>
      `;

      const dayCells = Array.from({ length: 7 }).map((_, index) => {
        const currentDay = new Date(monday);
        currentDay.setDate(monday.getDate() + index);
        const dayStart = currentDay.getTime();

        const currentDayEnd = new Date(currentDay);
        currentDayEnd.setHours(23, 59, 59, 999);
        const dayEnd = currentDayEnd.getTime();

        // Find shifts for this employee and day
        const dayShifts = shifts.filter(shift => {
          return shift.employee_id === emp.id &&
                 shift.start_time >= dayStart &&
                 shift.start_time <= dayEnd;
        });

        const shiftCards = dayShifts.map(shift => {
          const startStr = formatTime(new Date(shift.start_time));
          const endStr = formatTime(new Date(shift.end_time));
          const duration = ((shift.end_time - shift.start_time) / 3600000).toFixed(1);

          // Resolve project details
          const proj = projects.find(p => p.id === shift.project_id);
          const projName = proj ? proj.name : (shift.location || 'Sonstiges');
          const projColor = proj ? proj.color : 'var(--accent)';

          return `
            <button type="button" class="shift-card dept-${shift.department?.toLowerCase() || 'service'} ${shift.status || 'published'}" data-shift-id="${escapeHtml(shift.id)}" data-context-record-id="${escapeHtml(shift.id)}" data-context-record-type="planning_shift" data-context-label="${escapeHtml(shift.title || 'Schicht')}" aria-label="${escapeHtml(shift.title || 'Schicht')} ${startStr} - ${endStr}">
              <div class="shift-time">
                <span>${startStr} - ${endStr}</span>
                <span class="shift-tag">${shift.department}</span>
              </div>
              <div class="shift-title">${escapeHtml(shift.title || 'Schicht')}</div>
              <div class="shift-meta">${duration} Std · ${escapeHtml(projName)}</div>
              <div class="project-strip" style="background:${projColor};"></div>
            </button>
          `;
        }).join('');

        const cellDateStr = currentDay.toISOString().split('T')[0];

        return `
          <div class="grid-shift-cell shiftflow-grid-cell" data-emp-id="${emp.id}" data-date="${cellDateStr}">
            ${shiftCards}
            <button class="add-shift-hover-btn" data-emp-id="${emp.id}" data-date="${cellDateStr}">+ Schicht</button>
          </div>
        `;
      }).join('');

      return `
        <div class="scheduler-row" data-context-record-id="${escapeHtml(emp.id)}" data-context-record-type="planning_employee" data-context-label="${escapeHtml(emp.name)}">
          ${rowHeader}
          ${dayCells}
        </div>
      `;
    }).join('');

    els.schedulerGridBody.innerHTML = rows;

  } else {
    // 2. Project-Centric Timeline View
    let filteredProjects = projects;
    if (currentDeptFilter !== 'all') {
      // Filter projects that have shifts in this department, or show active ones
      filteredProjects = projects.filter(p => p.status === 'active');
    }

    const rows = filteredProjects.map(proj => {
      const rowHeader = `
        <div class="row-employee-cell" style="border-color: color-mix(in srgb, ${proj.color || 'var(--accent)'} 45%, var(--line)); background: color-mix(in srgb, ${proj.color || 'var(--accent)'} 8%, transparent);">
          <div class="emp-info">
            <div class="emp-name" style="font-weight:800;">${escapeHtml(proj.name)}</div>
            <div class="emp-meta">${escapeHtml(proj.client)}</div>
          </div>
        </div>
      `;

      const dayCells = Array.from({ length: 7 }).map((_, index) => {
        const currentDay = new Date(monday);
        currentDay.setDate(monday.getDate() + index);
        const dayStart = currentDay.getTime();

        const currentDayEnd = new Date(currentDay);
        currentDayEnd.setHours(23, 59, 59, 999);
        const dayEnd = currentDayEnd.getTime();

        // Find shifts for this project and day
        const dayShifts = shifts.filter(shift => {
          const matchesSearch = !searchQuery || visibleEmployeeIdsBySearch.has(shift.employee_id);
          const matchesDept = currentDeptFilter === 'all' || shift.department === currentDeptFilter;
          return matchesSearch &&
                 matchesDept &&
                 shift.project_id === proj.id &&
                 shift.start_time >= dayStart &&
                 shift.start_time <= dayEnd;
        });

        const shiftCards = dayShifts.map(shift => {
          const startStr = formatTime(new Date(shift.start_time));
          const endStr = formatTime(new Date(shift.end_time));
          const duration = ((shift.end_time - shift.start_time) / 3600000).toFixed(1);

          // Resolve employee details
          const emp = employees.find(e => e.id === shift.employee_id);
          const empName = emp ? emp.name : 'Unbesetzt';
          const avatarColor = emp ? emp.avatar_color : 'var(--muted)';
          const initials = emp ? emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase() : '?';

          return `
            <button type="button" class="shift-card dept-${shift.department?.toLowerCase() || 'service'} ${shift.status || 'published'}" data-shift-id="${escapeHtml(shift.id)}" data-context-record-id="${escapeHtml(shift.id)}" data-context-record-type="planning_shift" data-context-label="${escapeHtml(shift.title || 'Schicht')}" aria-label="${escapeHtml(shift.title || 'Schicht')} ${startStr} - ${endStr}" style="display:flex; flex-direction:column;">
              <div style="display:flex; align-items:center; gap:6px; margin-bottom:4px;">
                <div class="ctox-avatar emp-avatar" style="width:16px; height:16px; font-size:7px; background:${avatarColor};">${initials}</div>
                <span style="font-weight:700; font-size:11px;">${escapeHtml(empName)}</span>
              </div>
              <div class="shift-time" style="font-size:10px;">
                <span>${startStr} - ${endStr} (${duration}h)</span>
              </div>
              <div class="shift-meta">${escapeHtml(shift.title || 'Schicht')}</div>
            </button>
          `;
        }).join('');

        const cellDateStr = currentDay.toISOString().split('T')[0];

        return `
          <div class="grid-shift-cell shiftflow-grid-cell" data-proj-id="${proj.id}" data-date="${cellDateStr}">
            ${shiftCards}
            <button class="add-shift-hover-btn" data-proj-id="${proj.id}" data-date="${cellDateStr}">+ Schicht</button>
          </div>
        `;
      }).join('');

      return `
        <div class="scheduler-row" data-context-record-id="${escapeHtml(proj.id)}" data-context-record-type="planning_project" data-context-label="${escapeHtml(proj.name)}">
          ${rowHeader}
          ${dayCells}
        </div>
      `;
    }).join('');

    els.schedulerGridBody.innerHTML = rows || `<div class="ctox-empty">${escapeHtml(t('noActiveProjects', 'Keine aktiven Projekte vorhanden.'))}</div>`;
  }

  // Bind click & double-click handlers on shift cards
  els.schedulerGridBody.querySelectorAll('.shift-card').forEach(card => {
    card.addEventListener('click', (e) => {
      e.stopPropagation();
      const shift = shifts.find(s => s.id === card.dataset.shiftId);
      if (shift) selectShiftInPlace(shift.id, els, ctx);
    });

    card.addEventListener('dblclick', (e) => {
      e.stopPropagation();
      const shift = shifts.find(s => s.id === card.dataset.shiftId);
      if (shift) selectShiftInPlace(shift.id, els, ctx);
    });
  });

  // Employee row headers remain the drag source for the one-step booking path;
  // clicking one opens the existing quick-assign matrix in the in-board detail.
  els.schedulerGridBody.querySelectorAll('[data-open-employee-id]').forEach((button) => {
    button.addEventListener('click', () => selectEmployeeInPlace(button.dataset.openEmployeeId, els, ctx));
    button.addEventListener('dragstart', (event) => {
      event.dataTransfer.setData('text/plain', button.dataset.openEmployeeId);
      button.classList.add('is-dragging');
    });
    button.addEventListener('dragend', () => button.classList.remove('is-dragging'));
  });

  // Bind click handlers on "add shift" hover buttons
  els.schedulerGridBody.querySelectorAll('.add-shift-hover-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      openShiftDrawer(null, btn.dataset.date, btn.dataset.empId || '', btn.dataset.projId || '', els, ctx);
    });
  });

  // Setup HTML5 Drag over & Drop target receivers and double click cell creation
  const gridCells = els.schedulerGridBody.querySelectorAll('.shiftflow-grid-cell');
  gridCells.forEach(cell => {
    cell.addEventListener('dblclick', (e) => {
      // Avoid opening cell modal if double clicking on card inside cell
      if (e.target.closest('.shift-card')) return;
      openShiftDrawer(null, cell.dataset.date, cell.dataset.empId || '', cell.dataset.projId || '', els, ctx);
    });

    cell.addEventListener('dragover', (e) => {
      e.preventDefault();
      cell.classList.add('drag-over');
    });

    cell.addEventListener('dragleave', () => {
      cell.classList.remove('drag-over');
    });

    cell.addEventListener('drop', async (e) => {
      e.preventDefault();
      cell.classList.remove('drag-over');

      const empId = e.dataTransfer.getData('text/plain');
      if (!empId) return;

      const dateStr = cell.dataset.date;
      const targetProjId = cell.dataset.projId;
      const targetEmpId = cell.dataset.empId;

      const finalEmpId = targetEmpId || empId;
      let finalProjId = targetProjId;

      // If we are in employee-timeline focus, we don't have a direct project context. Let's find first active project.
      if (!finalProjId) {
        const activeProjects = projects.filter(p => p.status === 'active');
        if (activeProjects.length > 0) {
          finalProjId = activeProjects[0].id;
        } else {
          alert('Bitte erstelle zuerst ein aktives Projekt in der linken Spalte!');
          return;
        }
      }

      // Automatically create a scheduled shift
      const db = shiftflowDb(ctx);
      if (!db) return;

      const proj = projects.find(p => p.id === finalProjId);
      const projName = proj ? proj.name : 'Einsatz';

      const startStr = '08:00';
      const endStr = '16:00';
      const getTimestamp = (dStr, tStr) => {
        const d = new Date(dStr);
        const [h, m] = tStr.split(':').map(Number);
        d.setHours(h, m, 0, 0);
        return d.getTime();
      };

      const startTime = getTimestamp(dateStr, startStr);
      const endTime = getTimestamp(dateStr, endStr);

      const id = 'shift_' + Date.now();
      await db.planning_shifts.insert({
        id,
        kind: 'shift',
        employee_id: finalEmpId,
        project_id: finalProjId,
        title: `Einsatz ${projName}`,
        start_time: startTime,
        end_time: endTime,
        location: projName,
        department: 'Service',
        status: 'published',
        notes: 'Geplant per Drag & Drop',
        created_at_ms: Date.now(),
        updated_at_ms: Date.now()
      });
    });
  });

  applyBoardShiftSelection(els);
}

function renderTimesheets(employees, projects, shifts, records, els, ctx) {
  // Only show records with 'pending' status for approvals
  const pendingRecords = records.filter(rec => rec.approval_status === 'pending' && rec.end_time !== null);

  if (pendingRecords.length === 0) {
    els.timesheetsList.innerHTML = `
      <div class="ctox-empty">
        ${t('allTimesheetsApproved', 'Alle eingereichten Stundenzettel wurden freigegeben! 🎉')}
      </div>
    `;
    els.approveAllTimesheetsBtn.style.display = 'none';
    return;
  }

  els.approveAllTimesheetsBtn.style.display = 'inline-flex';

  const listHtml = pendingRecords.map(rec => {
    const emp = employees.find(e => e.id === rec.employee_id);
    const initials = emp ? emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase() : '?';
    const localeStr = lang === 'en' ? 'en-US' : 'de-DE';
    const dateStr = new Date(rec.start_time).toLocaleDateString(localeStr, { weekday: 'short', day: '2-digit', month: '2-digit', year: 'numeric' });
    const startStr = formatTime(new Date(rec.start_time));
    const endStr = formatTime(new Date(rec.end_time));

    // Calculate breaks and duration
    let breakMin = 0;
    if (rec.breaks && rec.breaks.length) {
      rec.breaks.forEach(b => {
        breakMin += Math.round((b.end - b.start) / 60000);
      });
    }
    const workedHours = ((rec.end_time - rec.start_time - breakMin * 60000) / 3600000);
    const workedHoursStr = workedHours.toFixed(2);

    // 1. Cross-reference Soll (Planned shift on that day)
    const recDayStr = new Date(rec.start_time).toISOString().split('T')[0];
    const dayStart = new Date(recDayStr).getTime();
    const dayEnd = dayStart + 24 * 3600000 - 1;

    const dayShifts = shifts.filter(s =>
      s.employee_id === rec.employee_id &&
      s.start_time >= dayStart &&
      s.start_time <= dayEnd
    );

    let sollHtml = `<span class="timesheet-secondary">${t('noShiftPlanned', 'Keine Schicht eingeplant (Überstunden)')}</span>`;
    let warningBadge = '';

    if (dayShifts.length > 0) {
      const s = dayShifts[0];
      const sStart = formatTime(new Date(s.start_time));
      const sEnd = formatTime(new Date(s.end_time));
      const sHours = ((s.end_time - s.start_time) / 3600000);

      sollHtml = t('sollPlannedText', 'Geplant (Soll): <strong>{0} - {1}</strong> ({2} Std)', sStart, sEnd, sHours.toFixed(1));

      const diff = workedHours - sHours;
      if (Math.abs(diff) >= 0.25) {
        const sign = diff > 0 ? '+' : '';
        const badgeClass = diff > 0 ? 'is-success' : 'is-danger';
        warningBadge = `<span class="ctox-badge ${badgeClass}">${t('hoursDeviationText', '{0}{1} Std Abweichung', sign, diff.toFixed(1))}</span>`;
      }
    }

    // 2. Resolve Project context
    const proj = projects.find(p => p.id === rec.project_id);
    const projName = proj ? proj.name : t('noProject', 'Ohne Projekt');
    const projColor = proj ? proj.color : 'var(--line)';

    return `
      <div class="timesheet-card" data-rec-id="${rec.id}" data-context-record-id="${escapeHtml(rec.id)}" data-context-record-type="planning_time_record" data-context-label="${escapeHtml(`${emp ? emp.name : t('employee', 'Mitarbeiter')} · ${dateStr}`)}">
        <div class="timesheet-card-main">
          <div class="ctox-avatar emp-avatar" style="background: ${emp ? emp.avatar_color : 'var(--muted)'}">${initials}</div>
          <div class="timesheet-details">
            <div class="timesheet-primary">${escapeHtml(emp ? emp.name : t('employee', 'Mitarbeiter'))} <span class="timesheet-secondary">(${escapeHtml(emp ? emp.role : '')})</span></div>
            <div class="timesheet-meta-row">
              <span>${t('dateText', 'Datum: <strong>{0}</strong>', dateStr)}</span>
              <span class="ctox-badge" style="background:color-mix(in srgb, ${projColor} 15%, transparent); color:var(--text);">${t('projects', 'Projekt')}: ${escapeHtml(projName)}</span>
            </div>
            <div class="timesheet-secondary">
              ${sollHtml}
            </div>
            ${rec.notes ? `<div class="timesheet-notes">"${escapeHtml(rec.notes)}"</div>` : ''}
          </div>
          <div class="timesheet-hours-block">
            <div class="timesheet-hours">${workedHoursStr} ${lang === 'en' ? 'hrs.' : 'Std.'}</div>
            <div class="timesheet-time-range">${startStr} - ${endStr} ${breakMin ? `(${t('pauseText', 'Pause: {0}m', breakMin)})` : ''}</div>
            <div class="timesheet-warning-row">${warningBadge}</div>
          </div>
        </div>
        <div class="timesheet-card-actions">
          <button class="ctox-button ctox-button--sm is-danger btn-reject" data-rec-id="${rec.id}">${t('reject', 'Ablehnen')}</button>
          <button class="ctox-button ctox-button--sm is-primary btn-approve" data-rec-id="${rec.id}">${t('approveAndBook', 'Genehmigen & Buchen')}</button>
        </div>
      </div>
    `;
  }).join('');

  els.timesheetsList.innerHTML = listHtml;

  // Bind individual timesheet approvals / rejections
  els.timesheetsList.querySelectorAll('.btn-approve').forEach(btn => {
    btn.addEventListener('click', () => {
      approveSingleRecord(btn.dataset.recId, ctx);
    });
  });

  els.timesheetsList.querySelectorAll('.btn-reject').forEach(btn => {
    btn.addEventListener('click', () => {
      rejectSingleRecord(btn.dataset.recId, ctx);
    });
  });
}

function renderBillingAggregation(employees, projects, records, els, ctx) {
  // 1. Gather all approved and completed records
  const approvedRecords = records.filter(rec => rec.approval_status === 'approved' && rec.end_time !== null);

  // 2. Date Filtering Bounds
  const startMs = els.billingStartDate.value ? new Date(els.billingStartDate.value).getTime() : 0;
  const endMs = els.billingEndDate.value ? new Date(els.billingEndDate.value + 'T23:59:59.999').getTime() : Infinity;

  const filtered = approvedRecords.filter(rec => rec.start_time >= startMs && rec.start_time <= endMs);

  // 3. Group by Project
  const aggregation = {};

  projects.forEach(p => {
    aggregation[p.id] = {
      project: p,
      hours: 0,
      cost: 0,
      revenue: 0,
      details: []
    };
  });

  // Safe fallback for office/other if no project was set
  const fallbackId = 'proj_office';
  if (!aggregation[fallbackId]) {
    aggregation[fallbackId] = {
      project: { id: fallbackId, name: 'Interne Verwaltung', client: 'CTOX OS', hourly_rate: 0.0, color: '#6366f1' },
      hours: 0,
      cost: 0,
      revenue: 0,
      details: []
    };
  }

  filtered.forEach(rec => {
    const projId = rec.project_id || fallbackId;

    if (!aggregation[projId]) {
      aggregation[projId] = {
        project: { id: projId, name: 'Unbekanntes Projekt', client: 'Sonstige', hourly_rate: 85.0, color: '#94a3b8' },
        hours: 0,
        cost: 0,
        revenue: 0,
        details: []
      };
    }

    // Calculate worked hours minus pauses
    let breakMin = 0;
    if (rec.breaks) {
      rec.breaks.forEach(b => {
        breakMin += Math.round((b.end - b.start) / 60000);
      });
    }
    const hours = (rec.end_time - rec.start_time - breakMin * 60000) / 3600000;

    // Resolve employee rates
    const emp = employees.find(e => e.id === rec.employee_id);
    const wageRate = emp ? (emp.internal_hourly_rate || 25.00) : 25.00;
    const cost = hours * wageRate;

    // Apply billing rate
    const billRate = rec.billing_rate_applied !== undefined ? rec.billing_rate_applied : (aggregation[projId].project.hourly_rate || 0.0);
    const revenue = hours * billRate;

    aggregation[projId].hours += hours;
    aggregation[projId].cost += cost;
    aggregation[projId].revenue += revenue;
    aggregation[projId].details.push({
      employee_name: emp ? emp.name : 'Mitarbeiter',
      hours,
      cost,
      revenue
    });
  });

  // 4. Render Aggregated Ledger Grid Rows
  let totalHours = 0;
  let totalCost = 0;
  let totalRevenue = 0;

  const rowsHtml = Object.values(aggregation)
    .filter(data => data.hours > 0) // only show projects with hours in that range
    .map(data => {
      const p = data.project;
      totalHours += data.hours;
      totalCost += data.cost;
      totalRevenue += data.revenue;

      const grossMargin = data.revenue - data.cost;
      const marginPercent = data.revenue > 0 ? (grossMargin / data.revenue) * 100 : 0.0;

      let badgeClass = 'is-success';
      if (marginPercent < 30) badgeClass = 'is-danger';
      else if (marginPercent < 55) badgeClass = 'is-warning';

      const billingDisplay = p.hourly_rate > 0 ? `${p.hourly_rate.toFixed(2)} €/h` : 'Nicht abrechenbar';

      return `
        <tr data-context-record-id="${escapeHtml(p.id)}" data-context-record-type="planning_project" data-context-label="${escapeHtml(p.name)}">
          <td class="billing-project-cell">
            <span class="project-card-badge" style="background:${p.color || 'var(--muted)'}"></span>
            <strong>${escapeHtml(p.name)}</strong>
          </td>
          <td>${escapeHtml(p.client || '')}</td>
          <td class="is-num" style="font-weight:600;">${data.hours.toFixed(1)} Std</td>
          <td class="is-num">${billingDisplay}</td>
          <td class="is-num" style="font-weight:700;">${data.revenue.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} €</td>
          <td class="is-num text-cost">${data.cost.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} €</td>
          <td class="is-num">
            <span class="ctox-badge ${badgeClass}">${marginPercent.toFixed(1)}% (${grossMargin.toLocaleString('de-DE', { maximumFractionDigits: 0 })} €)</span>
          </td>
          <td class="is-num">
            <button class="ctox-button ctox-button--sm btn-billing-inspect" data-proj-id="${p.id}">Details</button>
          </td>
        </tr>
      `;
    }).join('');

  els.billingAggregationBody.innerHTML = rowsHtml || `
    <tr>
      <td colspan="8">
        <div class="ctox-empty">Keine freigegebenen Zeiterfassungen im gewählten Zeitraum vorhanden.</div>
      </td>
    </tr>
  `;

  // 5. Update KPI Financial summary cards
  const totalMarginVal = totalRevenue - totalCost;
  const totalMarginPercentVal = totalRevenue > 0 ? (totalMarginVal / totalRevenue) * 100 : 0.0;

  els.billingTotalRevenue.textContent = `${totalRevenue.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} €`;
  els.billingTotalCost.textContent = `${totalCost.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} €`;
  els.billingTotalMargin.textContent = `${totalMarginVal.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} € (${totalMarginPercentVal.toFixed(1)}%)`;

  // Bind inspect aggregation details
  els.billingAggregationBody.querySelectorAll('.btn-billing-inspect').forEach(btn => {
    btn.addEventListener('click', () => {
      openBillingDetailsInspector(btn.dataset.projId, aggregation[btn.dataset.projId], els, ctx);
    });
  });

  // Save the grouped aggregation object globally for export payload access
  globalThis.CTOX_LAST_AGGREGATION = {
    aggregation,
    totals: { totalHours, totalCost, totalRevenue, totalMarginVal, totalMarginPercentVal },
    dateRange: { start: els.billingStartDate.value, end: els.billingEndDate.value }
  };
}

function openBillingDetailsInspector(projId, projectData, els, ctx) {
  const body = document.createElement('div');
  body.className = 'drawer-body shiftflow-drawer-body';

  const itemsHtml = projectData.details.map(item => {
    return `
      <div class="shiftflow-billing-detail-row">
        <div>
          <div class="shiftflow-billing-detail-name">${escapeHtml(item.employee_name)}</div>
          <div class="shiftflow-billing-detail-meta">${t('billingDetailsCostLabel', 'Umsatz: {0} € · Lohnkosten: {1} €', item.revenue.toFixed(2), item.cost.toFixed(2))}</div>
        </div>
        <div class="shiftflow-billing-detail-hours">${item.hours.toFixed(1)} ${t('hoursShort', 'Std.')}</div>
      </div>
    `;
  }).join('');

  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span class="ctox-pane-kicker">${t('billingEvaluation', 'Auswertung')}</span>
        <h2>${escapeHtml(projectData.project.name)}</h2>
      </div>
      <button class="ctox-pane-icon" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
    </header>
    <div class="shiftflow-drawer-form">
      <dl class="ctox-fields">
        <dt>${t('colCustomer', 'Kunde')}</dt>
        <dd>${escapeHtml(projectData.project.client)}</dd>
        <dt>${t('location', 'Einsatzort')}</dt>
        <dd>${escapeHtml(projectData.project.location || t('noInfo', 'Keine Angabe'))}</dd>
      </dl>

      <hr class="shiftflow-separator" />

      <div>
        <span class="ctox-field-label">${t('employeeDistribution', 'Mitarbeiter Aufteilung')}</span>
        <div class="shiftflow-billing-detail-list os-scrollbar">
          ${itemsHtml || `<div class="ctox-empty">${t('noEntries', 'Keine Einträge')}</div>`}
        </div>
      </div>
    </div>
  `;

  body.querySelector('[data-drawer-close]').addEventListener('click', () => ctx.closeDrawers());
  ctx.openRightDrawer(body);
}

// -------------------------------------------------------------
// Timesheet Approvals actions
// -------------------------------------------------------------

async function approveSingleRecord(recId, ctx) {
  const db = shiftflowDb(ctx);
  if (!db) return;

  const doc = await db.planning_time_records.findOne(recId).exec();
  if (doc) {
    // Resolve project billing rate to apply permanently to timesheet record
    const proj = await db.planning_projects.findOne(doc.project_id || 'proj_office').exec();
    const rate = proj ? proj.hourly_rate : 0.0;

    await doc.incrementalPatch({
      approval_status: 'approved',
      approved_by: 'manager',
      billing_status: 'uninvoiced',
      billing_rate_applied: rate,
      updated_at_ms: Date.now()
    });
  }
}

async function rejectSingleRecord(recId, ctx) {
  const db = shiftflowDb(ctx);
  if (!db) return;

  const doc = await db.planning_time_records.findOne(recId).exec();
  if (doc) {
    await doc.incrementalPatch({
      approval_status: 'rejected',
      updated_at_ms: Date.now()
    });
  }
}

async function approveAllTimesheets(ctx) {
  const db = shiftflowDb(ctx);
  if (!db) return;

  const records = await db.planning_time_records.find({ selector: { approval_status: 'pending' } }).exec();
  for (const rec of records) {
    if (rec.end_time !== null) {
      const proj = await db.planning_projects.findOne(rec.project_id || 'proj_office').exec();
      const rate = proj ? proj.hourly_rate : 0.0;

      await rec.incrementalPatch({
        approval_status: 'approved',
        approved_by: 'manager',
        billing_status: 'uninvoiced',
        billing_rate_applied: rate,
        updated_at_ms: Date.now()
      });
    }
  }
}

async function publishCurrentWeekSchedule(ctx, els) {
  if (!ctx.db) return;

  const { startMs, endMs } = getWeekBoundsMs(currentWeekStart);
  const weekShifts = await shiftflowCollection(ctx, 'planning_shifts').find({
    selector: {
      start_time: { $gte: startMs, $lte: endMs }
    }
  }).exec();

  if (!weekShifts.length) {
    setPublishStatus(els, t('publishNoShifts', 'Keine Schichten in dieser Woche.'));
    return;
  }

  const draftShifts = weekShifts.filter(shift => shift.status !== 'published');
  if (!draftShifts.length) {
    setPublishStatus(els, t('publishAlreadyDone', 'Woche ist bereits veröffentlicht.'));
    return;
  }

  const confirmMessage = t(
    'confirmPublishSchedule',
    'Möchtest du {0} Schicht(en) für KW {1} veröffentlichen?',
    String(draftShifts.length),
    String(getWeekNumber(currentWeekStart))
  );
  if (!confirm(confirmMessage)) return;

  els.btnPublishSchedule.disabled = true;
  setPublishStatus(els, t('publishInProgress', 'Veröffentliche...'));
  try {
    for (const shift of draftShifts) {
      await shift.incrementalPatch({
        status: 'published',
        published_at_ms: Date.now(),
        updated_at_ms: Date.now()
      });
    }
    setPublishStatus(els, t('publishSuccess', '{0} Schicht(en) veröffentlicht.', String(draftShifts.length)));
  } catch (error) {
    console.error('[shiftflow] Failed to publish schedule', error);
    setPublishStatus(els, t('publishFailed', 'Veröffentlichung fehlgeschlagen.'));
  } finally {
    els.btnPublishSchedule.disabled = false;
  }
}

// Shift planning forms are now built dynamically inside the side drawer via openShiftDrawer

async function openShiftDetails(shiftId, shifts, employees, els, ctx) {
  const shift = shifts.find(s => s.id === shiftId);
  if (!shift) return;
  selectedShiftId = shift.id;

  const emp = employees.find(e => e.id === shift.employee_id);
  const db = shiftflowDb(ctx);
  let projName = shift.location || t('location', 'Einsatzort');

  if (db && shift.project_id) {
    const proj = await db.planning_projects.findOne(shift.project_id).exec();
    if (proj) projName = proj.name;
  }

  els.inspectorTitle.textContent = t('shiftDetails', 'Schicht-Details');

  const start = new Date(shift.start_time);
  const end = new Date(shift.end_time);
  const options = { weekday: 'short', day: '2-digit', month: '2-digit', year: 'numeric' };

  els.inspectorContent.innerHTML = `
    <div class="shiftflow-inspector">
      <div class="shiftflow-inspector-person">
        <div class="ctox-avatar ctox-avatar--lg emp-avatar" style="background: ${emp ? emp.avatar_color : 'var(--muted)'};">
          ${emp ? emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase() : '?'}
        </div>
        <div>
          <h4 class="shiftflow-inspector-name">${escapeHtml(emp ? emp.name : t('employee', 'Mitarbeiter'))}</h4>
          <span class="shiftflow-inspector-sub">${escapeHtml(emp ? emp.role : '')}</span>
        </div>
      </div>

      <hr class="shiftflow-separator" />

      <dl class="ctox-fields">
        <dt>${t('dateTime', 'Datum & Zeit')}</dt>
        <dd>
          ${start.toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', options)}<br>
          <span class="shiftflow-accent-text">${formatTime(start)} - ${formatTime(end)} (${((shift.end_time - shift.start_time)/3600000).toFixed(1)} ${t('hoursShort', 'Std')})</span>
        </dd>
        <dt>${t('colProject', 'Projekt / Einsatzort')}</dt>
        <dd>${escapeHtml(projName)}</dd>
        <dt>${t('department', 'Abteilung')}</dt>
        <dd>${t('dept' + (shift.department === 'Küche' ? 'Kitchen' : shift.department === 'Verwaltung' ? 'Admin' : shift.department), shift.department)}</dd>
      </dl>

      ${shift.notes ? `
        <div>
          <span class="ctox-field-label">${t('notes', 'Notizen')}</span>
          <div class="ctox-callout">"${escapeHtml(shift.notes)}"</div>
        </div>
      ` : ''}

      <div class="shiftflow-inspector-actions">
        <button class="ctox-button" id="btnEditShiftInspector">${t('edit', 'Bearbeiten')}</button>
      </div>
    </div>
  `;

  showInspectorSection(els);

  // Edit in inspector trigger
  els.inspectorContent.querySelector('#btnEditShiftInspector').addEventListener('click', () => {
    openShiftDrawer(shift, null, null, null, els, ctx);
  });
}



// -------------------------------------------------------------
// Employee Details Inspector & Form Operations
// -------------------------------------------------------------

async function openEmployeeDetailsInspector(empId, employees, els, ctx) {
  const emp = employees.find(e => e.id === empId);
  if (!emp) {
    selectedEmployeeId = null;
    hideInspectorSection(els);
    return;
  }
  selectedEmployeeId = empId;

  const db = shiftflowDb(ctx);
  if (!db) return;

  // Fetch shifts for this employee in the current week (from currentWeekStart)
  const mondayStart = new Date(currentWeekStart);
  mondayStart.setHours(0, 0, 0, 0);
  const sundayEnd = new Date(mondayStart);
  sundayEnd.setDate(sundayEnd.getDate() + 7);

  const allShifts = await db.planning_shifts.find({
    selector: { employee_id: empId }
  }).exec();

  const currentWeekShifts = allShifts.filter(s => {
    return s.start_time >= mondayStart.getTime() && s.start_time < sundayEnd.getTime();
  });

  const totalHours = currentWeekShifts.reduce((sum, s) => {
    return sum + (s.end_time - s.start_time) / 3600000;
  }, 0);

  const projects = await db.planning_projects.find().exec();
  const activeProjects = projects.filter(p => p.status === 'active');

  els.inspectorTitle.textContent = t('employeeDetails', 'Mitarbeiter-Details');

  els.inspectorContent.innerHTML = `
    <div class="shiftflow-inspector">
      <!-- Header Info -->
      <div class="shiftflow-inspector-person">
        <div class="ctox-avatar ctox-avatar--lg emp-avatar" style="background: ${emp.avatar_color || 'var(--accent)'};">
          ${emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase()}
        </div>
        <div>
          <h4 class="shiftflow-inspector-name">${escapeHtml(emp.name)}</h4>
          <span class="shiftflow-inspector-sub">${escapeHtml(emp.role || t('employee', 'Mitarbeiter'))}</span>
        </div>
      </div>

      <hr class="shiftflow-separator" />

      <!-- Stammdaten -->
      <dl class="ctox-fields">
        <dt>${t('billingWagesLabel', 'Lohnkosten')}</dt>
        <dd>${emp.internal_hourly_rate ? emp.internal_hourly_rate.toFixed(2) : '25.00'} €/${t('hoursShort', 'Std')}</dd>
        <dt>${t('weeklyTarget', 'Wochen-Soll')}</dt>
        <dd>${emp.weekly_target_hours || '40'} ${t('hoursShort', 'Std')}</dd>
      </dl>

      <!-- Weekly Planned Progress -->
      <div>
        <div class="shiftflow-progress-head">
          <span class="ctox-field-label">${t('plannedThisWeek', 'Eingeplant (diese Woche)')}</span>
          <span class="shiftflow-progress-value" style="color:${totalHours > (emp.weekly_target_hours || 40) ? 'var(--danger)' : 'var(--accent)'};">
            ${totalHours.toFixed(1)} / ${emp.weekly_target_hours || '40'} ${t('hoursShort', 'Std')}
          </span>
        </div>
        <div class="shiftflow-progress">
          <div class="shiftflow-progress-bar" style="width:${Math.min(100, (totalHours / (emp.weekly_target_hours || 40)) * 100)}%; background:${totalHours > (emp.weekly_target_hours || 40) ? 'var(--danger)' : 'var(--accent)'};"></div>
        </div>
      </div>

      <div>
        <span class="ctox-field-label">${t('departments', 'Abteilungen')}</span>
        <div class="shiftflow-badge-row">
          ${(emp.departments || ['Service']).map(dept => {
            const dMap = { 'Service': t('deptService', 'Service'), 'Küche': t('deptKitchen', 'Küche'), 'Bar': t('deptBar', 'Bar'), 'Verwaltung': t('deptAdmin', 'Verwaltung') };
            return `<span class="ctox-badge">${escapeHtml(dMap[dept] || dept)}</span>`;
          }).join('')}
        </div>
      </div>

        <p class="shiftflow-inspector-hint">${t('quickAssignInstructions', 'Klicke auf einen Tag, um den Mitarbeiter direkt für das Projekt einzuteilen (8:00 - 16:00 Uhr):')}</p>
        <div class="shiftflow-quick-assign">
          ${activeProjects.length === 0 ? `<div class="ctox-empty">${t('noActiveProjects', 'Keine aktiven Projekte vorhanden.')}</div>` : activeProjects.map(proj => {
            const weekdays = lang === 'en'
              ? ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
              : ['Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa', 'So'];
            const weekdayButtons = weekdays.map((dayName, idx) => {
              const targetDate = new Date(currentWeekStart);
              targetDate.setDate(targetDate.getDate() + idx);
              const dateStr = targetDate.toISOString().split('T')[0];

              return `
                <button
                  class="quick-assign-day-btn"
                  data-proj-id="${proj.id}"
                  data-emp-id="${emp.id}"
                  data-date="${dateStr}"
                  title="${dayName}, ${targetDate.toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', {day:'2-digit', month:'2-digit'})}"
                >
                  ${dayName}
                </button>
              `;
            }).join('');

            return `
              <div class="shiftflow-quick-assign-card">
                <div class="shiftflow-quick-assign-title">
                  <span class="project-card-badge" style="background:${proj.color || 'var(--accent)'};"></span>
                  <span>${escapeHtml(proj.name)}</span>
                </div>
                <div class="shiftflow-quick-assign-days">
                  ${weekdayButtons}
                </div>
              </div>
            `;
          }).join('')}
        </div>

      <hr class="shiftflow-separator" />

      <!-- Inspector Profile Actions -->
      <div class="shiftflow-inspector-actions">
        <button class="ctox-button" id="btnEditEmployeeInspector">${t('editProfile', 'Profil bearbeiten')}</button>
        <button class="ctox-button is-danger" id="btnDeleteEmployeeInspector">${t('deleteEmployee', 'Mitarbeiter löschen')}</button>
      </div>
    </div>
  `;

  showInspectorSection(els);

  // Bind edit profile action inside Inspector
  els.inspectorContent.querySelector('#btnEditEmployeeInspector').addEventListener('click', () => {
    openEmployeeDrawer(emp, els, ctx);
  });

  // Bind delete profile action inside Inspector
  els.inspectorContent.querySelector('#btnDeleteEmployeeInspector').addEventListener('click', async () => {
    const confirmDelete = confirm(t('confirmDeleteEmployee', 'Möchtest du den Mitarbeiter "{0}" wirklich löschen? Alle zugeordneten Schichten werden ebenfalls gelöscht.', emp.name));
    if (!confirmDelete) return;

    const doc = await db.planning_employees.findOne(emp.id).exec();
    if (doc) {
      await doc.remove();
    }

    const associatedShifts = await db.planning_shifts.find({ selector: { employee_id: emp.id } }).exec();
    for (const s of associatedShifts) {
      await s.remove();
    }

    selectedEmployeeId = null;
    hideInspectorSection(els);
  });

  // Bind quick assign day button actions
  els.inspectorContent.querySelectorAll('.quick-assign-day-btn').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const projId = btn.dataset.projId;
      const empId = btn.dataset.empId;
      const dateStr = btn.dataset.date;

      const proj = activeProjects.find(p => p.id === projId);
      const projName = proj ? proj.name : 'Einsatz';

      const startStr = '08:00';
      const endStr = '16:00';

      const getTimestamp = (dStr, tStr) => {
        const d = new Date(dStr);
        const [h, m] = tStr.split(':').map(Number);
        d.setHours(h, m, 0, 0);
        return d.getTime();
      };

      const startTime = getTimestamp(dateStr, startStr);
      const endTime = getTimestamp(dateStr, endStr);

      const id = 'shift_' + Date.now();
      await db.planning_shifts.insert({
        id,
        kind: 'shift',
        employee_id: empId,
        project_id: projId,
        title: `${t('location', 'Einsatz')} ${projName}`,
        start_time: startTime,
        end_time: endTime,
        location: projName,
        department: 'Service',
        status: 'published',
        notes: t('quickAssignNotes', 'Schnell-Zuweisung im Inspector'),
        created_at_ms: Date.now(),
        updated_at_ms: Date.now()
      });
    });
  });
}

function openEmployeeDrawer(emp, els, ctx) {
  const isEdit = !!emp;
  const body = document.createElement('div');
  body.className = 'drawer-body shiftflow-drawer-body';

  const title = isEdit ? t('employeeEdit', 'Mitarbeiter bearbeiten') : t('employeeCreate', 'Mitarbeiter anlegen');
  const kicker = isEdit ? t('employeeBasicsKicker', 'Stammdaten') : t('employeeNewKicker', 'Neuaufnahme');
  const submitText = isEdit ? t('save', 'Speichern') : t('create', 'Anlegen');

  const nameVal = isEdit ? escapeHtml(emp.name) : '';
  const emailVal = isEdit ? escapeHtml(emp.email || '') : '';
  const rateVal = isEdit ? (emp.internal_hourly_rate || 25.00).toFixed(2) : '25.00';
  const roleVal = isEdit ? escapeHtml(emp.role || '') : '';
  const hoursVal = isEdit ? (emp.weekly_target_hours || 40) : '40';
  const depts = isEdit ? (emp.departments || []) : ['Service'];

  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span class="ctox-pane-kicker">${kicker}</span>
        <h2>${title}</h2>
      </div>
      <button class="ctox-pane-icon" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
    </header>
    <form class="shiftflow-drawer-form">
      <label>
        <span class="ctox-field-label">${t('name', 'Name')}</span>
        <input type="text" name="name" class="ctox-input" value="${nameVal}" required placeholder="z.B. Max Mustermann" />
      </label>
      <label>
        <span class="ctox-field-label">${t('email', 'E-Mail')}</span>
        <input type="email" name="email" class="ctox-input" value="${emailVal}" placeholder="z.B. max@ctox.dev" />
      </label>
      <div class="form-row">
        <label>
          <span class="ctox-field-label">${t('wagesPerHr', 'Lohnkosten (€/h)')}</span>
          <input type="number" step="0.01" name="internal_hourly_rate" class="ctox-input" value="${rateVal}" required />
        </label>
        <label>
          <span class="ctox-field-label">${t('weeklyTargetHrs', 'Wochen-Soll (Std)')}</span>
          <input type="number" name="weekly_target_hours" class="ctox-input" value="${hoursVal}" required />
        </label>
      </div>
      <label>
        <span class="ctox-field-label">${t('rolePosition', 'Rolle / Position')}</span>
        <input type="text" name="role" class="ctox-input" value="${roleVal}" placeholder="${t('rolePlaceholder', 'z.B. Serviceleitung')}" required />
      </label>
      <label>
        <span class="ctox-field-label">${t('departments', 'Abteilungen')}</span>
        <div class="ctox-choice-group">
          <label class="ctox-choice"><input type="checkbox" name="departments" value="Service" ${depts.includes('Service') ? 'checked' : ''} /><span>${t('deptService', 'Service')}</span></label>
          <label class="ctox-choice"><input type="checkbox" name="departments" value="Küche" ${depts.includes('Küche') ? 'checked' : ''} /><span>${t('deptKitchen', 'Küche')}</span></label>
          <label class="ctox-choice"><input type="checkbox" name="departments" value="Bar" ${depts.includes('Bar') ? 'checked' : ''} /><span>${t('deptBar', 'Bar')}</span></label>
          <label class="ctox-choice"><input type="checkbox" name="departments" value="Verwaltung" ${depts.includes('Verwaltung') ? 'checked' : ''} /><span>${t('deptAdmin', 'Verwaltung')}</span></label>
        </div>
      </label>
      <div class="shiftflow-drawer-actions ${isEdit ? 'has-danger' : ''}">
        <button type="button" class="ctox-button" data-drawer-cancel>${t('cancel', 'Abbrechen')}</button>
        ${isEdit ? `<button type="button" class="ctox-button is-danger" data-drawer-delete>${t('delete', 'Löschen')}</button>` : ''}
        <button type="submit" class="ctox-button is-primary">${submitText}</button>
      </div>
    </form>
  `;

  body.querySelector('[data-drawer-close]').addEventListener('click', () => ctx.closeDrawers());
  body.querySelector('[data-drawer-cancel]').addEventListener('click', () => ctx.closeDrawers());

  if (isEdit) {
    body.querySelector('[data-drawer-delete]').addEventListener('click', async () => {
      const confirmDelete = confirm(t('confirmDeleteEmployee', 'Möchtest du den Mitarbeiter "{0}" wirklich löschen? Alle zugeordneten Schichten werden ebenfalls gelöscht.', emp.name));
      if (!confirmDelete) return;

      const doc = await shiftflowCollection(ctx, 'planning_employees').findOne(emp.id).exec();
      if (doc) {
        await doc.remove();
      }

      const associatedShifts = await shiftflowCollection(ctx, 'planning_shifts').find({ selector: { employee_id: emp.id } }).exec();
      for (const s of associatedShifts) {
        await s.remove();
      }

      ctx.closeDrawers();
      selectedEmployeeId = null;
      hideInspectorSection(els);
    });
  }

  body.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const form = e.target;
    const name = form.name.value;
    const email = form.email.value;
    const rate = Number(form.internal_hourly_rate.value);
    const role = form.role.value;
    const hours = Number(form.weekly_target_hours.value);

    const selectedDepts = [];
    form.querySelectorAll('input[name="departments"]:checked').forEach(cb => {
      selectedDepts.push(cb.value);
    });

    if (isEdit) {
      const doc = await shiftflowCollection(ctx, 'planning_employees').findOne(emp.id).exec();
      if (doc) {
        await doc.incrementalPatch({
          name,
          email,
          role,
          weekly_target_hours: hours,
          internal_hourly_rate: rate,
          departments: selectedDepts,
          updated_at_ms: Date.now()
        });
      }
    } else {
      const colors = ['hsl(250, 70%, 50%)', 'hsl(168, 80%, 40%)', 'hsl(30, 90%, 50%)', 'hsl(200, 80%, 45%)', 'hsl(340, 75%, 50%)'];
      const randomColor = colors[Math.floor(Math.random() * colors.length)];

      const id = 'emp_' + Date.now();
      await shiftflowCollection(ctx, 'planning_employees').insert({
        id,
        kind: 'employee',
        name,
        email,
        role,
        weekly_target_hours: hours,
        status: 'active',
        avatar_color: randomColor,
        internal_hourly_rate: rate,
        departments: selectedDepts,
        skills: ['New-hire'],
        created_at_ms: Date.now(),
        updated_at_ms: Date.now()
      });
    }
    ctx.closeDrawers();
  });

  ctx.openLeftDrawer(body);
}

function openProjectDrawer(proj, els, ctx) {
  const isEdit = !!proj;
  const body = document.createElement('div');
  body.className = 'drawer-body shiftflow-drawer-body';

  const title = isEdit ? t('projectEdit', 'Projekt bearbeiten') : t('projectCreate', 'Projekt anlegen');
  const kicker = isEdit ? t('projectDetailsKicker', 'Projekt-Eckdaten') : t('projectNewKicker', 'Neues Projekt');
  const submitText = isEdit ? t('save', 'Speichern') : t('create', 'Anlegen');

  const nameVal = isEdit ? escapeHtml(proj.name) : '';
  const clientVal = isEdit ? escapeHtml(proj.client) : '';
  const locationVal = isEdit ? escapeHtml(proj.location || '') : '';
  const rateVal = isEdit ? (proj.hourly_rate || 0.00).toFixed(2) : '85.00';
  const colorVal = isEdit ? proj.color : '#06b6d4';

  const colors = ['#06b6d4', '#22c55e', '#6366f1', '#8b5cf6', '#f97316', '#f43f5e'];
  const colorPickerHtml = colors.map(c => `
    <label class="color-option-label" style="background: ${c};">
      <input type="radio" name="color" value="${c}" ${colorVal === c ? 'checked' : ''} />
    </label>
  `).join('');

  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span class="ctox-pane-kicker">${kicker}</span>
        <h2>${title}</h2>
      </div>
      <button class="ctox-pane-icon" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
    </header>
    <form class="shiftflow-drawer-form">
      <label>
        <span class="ctox-field-label">${t('projectName', 'Projektname')}</span>
        <input type="text" name="name" class="ctox-input" value="${nameVal}" required placeholder="${t('projectNamePlaceholder', 'z.B. Intersolar Standbau')}" />
      </label>
      <label>
        <span class="ctox-field-label">${t('customer', 'Kunde')}</span>
        <input type="text" name="client" class="ctox-input" value="${clientVal}" required placeholder="${t('customerPlaceholder', 'z.B. Messe München GmbH')}" />
      </label>
      <label>
        <span class="ctox-field-label">${t('location', 'Einsatzort')}</span>
        <input type="text" name="location" class="ctox-input" value="${locationVal}" placeholder="${t('locationPlaceholder', 'z.B. Halle A5, Stand 120')}" />
      </label>
      <label>
        <span class="ctox-field-label">${t('hourlyRateExt', 'Stundensatz (externer Umsatz)')}</span>
        <input type="number" step="0.01" name="hourly_rate" class="ctox-input" value="${rateVal}" required />
      </label>
      <label>
        <span class="ctox-field-label">${t('color', 'Farbe')}</span>
        <div class="shiftflow-color-picker">
          ${colorPickerHtml}
        </div>
      </label>
      <div class="shiftflow-drawer-actions ${isEdit ? 'has-danger' : ''}">
        <button type="button" class="ctox-button" data-drawer-cancel>${t('cancel', 'Abbrechen')}</button>
        ${isEdit ? `<button type="button" class="ctox-button is-danger" data-drawer-delete>${t('delete', 'Löschen')}</button>` : ''}
        <button type="submit" class="ctox-button is-primary">${submitText}</button>
      </div>
    </form>
  `;

  body.querySelector('[data-drawer-close]').addEventListener('click', () => ctx.closeDrawers());
  body.querySelector('[data-drawer-cancel]').addEventListener('click', () => ctx.closeDrawers());

  if (isEdit) {
    body.querySelector('[data-drawer-delete]').addEventListener('click', async () => {
      const shiftCount = await shiftflowCollection(ctx, 'planning_shifts').find({ selector: { project_id: proj.id } }).exec();
      if (shiftCount.length > 0) {
        const confirmDelete = confirm(t('confirmDeleteProjectWithShifts', 'Es sind noch {0} Schichten für das Projekt "{1}" geplant. Willst du das Projekt trotzdem löschen?', shiftCount.length, proj.name));
        if (!confirmDelete) return;
      } else {
        const confirmDelete = confirm(t('confirmDeleteProject', 'Möchtest du das Projekt "{0}" wirklich löschen?', proj.name));
        if (!confirmDelete) return;
      }

      const doc = await shiftflowCollection(ctx, 'planning_projects').findOne(proj.id).exec();
      if (doc) {
        await doc.remove();
      }
      ctx.closeDrawers();
    });
  }

  body.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const form = e.target;
    const name = form.name.value;
    const client = form.client.value;
    const location = form.location.value;
    const rate = Number(form.hourly_rate.value);
    const color = form.querySelector('input[name="color"]:checked').value;

    const patch = {
      name,
      client,
      location,
      hourly_rate: rate,
      color,
      status: 'active',
      updated_at_ms: Date.now()
    };

    if (isEdit) {
      const doc = await shiftflowCollection(ctx, 'planning_projects').findOne(proj.id).exec();
      if (doc) {
        await doc.incrementalPatch(patch);
      }
    } else {
      const id = 'proj_' + Date.now();
      await shiftflowCollection(ctx, 'planning_projects').insert({
        id,
        kind: 'project',
        created_at_ms: Date.now(),
        ...patch
      });
    }
    ctx.closeDrawers();
  });

  ctx.openLeftDrawer(body);
}

async function openShiftDrawer(shift, dateStr, empId, projId, els, ctx) {
  const isEdit = !!shift;
  const body = document.createElement('div');
  body.className = 'drawer-body shiftflow-drawer-body';

  const title = isEdit ? t('shiftEdit', 'Schicht bearbeiten') : t('shiftCreate', 'Schicht planen');
  const kicker = isEdit ? t('shiftDetailsKicker', 'Dienst-Details') : t('shiftNewKicker', 'Neue Einteilung');
  const submitText = isEdit ? t('save', 'Speichern') : t('tabScheduler', 'Planen');

  const employees = await shiftflowCollection(ctx, 'planning_employees').find().exec();
  const projects = await shiftflowCollection(ctx, 'planning_projects').find().exec();

  const activeProj = projects.filter(p => p.status === 'active');

  const selectedEmpId = isEdit ? shift.employee_id : empId;
  const selectedProjId = isEdit ? (shift.project_id || 'proj_office') : (projId || (activeProj[0]?.id || ''));

  const deptVal = isEdit ? (shift.department || 'Service') : 'Service';

  let shiftDateStr = dateStr || '';
  let startVal = '08:00';
  let endVal = '16:00';
  let notesVal = '';

  if (isEdit) {
    const start = new Date(shift.start_time);
    const end = new Date(shift.end_time);
    shiftDateStr = start.toISOString().split('T')[0];
    startVal = start.toTimeString().slice(0, 5);
    endVal = end.toTimeString().slice(0, 5);
    notesVal = escapeHtml(shift.notes || '');
  }

  const empOptions = employees.map(emp => `
    <option value="${emp.id}" ${selectedEmpId === emp.id ? 'selected' : ''}>
      ${escapeHtml(emp.name)} (${escapeHtml(emp.role)})
    </option>
  `).join('');

  const projOptions = projects.map(p => `
    <option value="${p.id}" ${selectedProjId === p.id ? 'selected' : ''}>
      ${escapeHtml(p.name)} (${p.hourly_rate.toFixed(0)} €/h)
    </option>
  `).join('');

  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span class="ctox-pane-kicker">${kicker}</span>
        <h2>${title}</h2>
      </div>
      <button class="ctox-pane-icon" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
    </header>
    <form class="shiftflow-drawer-form">
      <label>
        <span class="ctox-field-label">${t('employee', 'Mitarbeiter')}</span>
        <select name="employee_id" class="ctox-select" required>
          ${empOptions}
        </select>
      </label>
      <label>
        <span class="ctox-field-label">${t('colProject', 'Projekt / Einsatzort')}</span>
        <select name="project_id" class="ctox-select" required>
          ${projOptions}
        </select>
      </label>
      <div class="form-row">
        <label>
          <span class="ctox-field-label">${t('date', 'Datum')}</span>
          <input type="date" name="date" class="ctox-input" value="${shiftDateStr}" required />
        </label>
        <label>
          <span class="ctox-field-label">${t('department', 'Abteilung')}</span>
          <select name="department" class="ctox-select" required>
            <option value="Service" ${deptVal === 'Service' ? 'selected' : ''}>${t('deptService', 'Service')}</option>
            <option value="Küche" ${deptVal === 'Küche' ? 'selected' : ''}>${t('deptKitchen', 'Küche')}</option>
            <option value="Bar" ${deptVal === 'Bar' ? 'selected' : ''}>${t('deptBar', 'Bar')}</option>
            <option value="Verwaltung" ${deptVal === 'Verwaltung' ? 'selected' : ''}>${t('deptAdmin', 'Verwaltung')}</option>
          </select>
        </label>
      </div>
      <div class="form-row">
        <label>
          <span class="ctox-field-label">${t('begin', 'Beginn')}</span>
          <input type="time" name="start_time" class="ctox-input" value="${startVal}" required />
        </label>
        <label>
          <span class="ctox-field-label">${t('end', 'Ende')}</span>
          <input type="time" name="end_time" class="ctox-input" value="${endVal}" required />
        </label>
      </div>
      <label>
        <span class="ctox-field-label">${t('specialNotes', 'Besondere Notizen')}</span>
        <textarea name="notes" class="ctox-textarea" placeholder="${t('notesPlaceholder', 'z.B. Schichtleitung übernehmen, Barista Service...')}">${notesVal}</textarea>
      </label>
      <div class="shiftflow-drawer-actions ${isEdit ? 'has-danger' : ''}">
        <button type="button" class="ctox-button" data-drawer-cancel>${t('cancel', 'Abbrechen')}</button>
        ${isEdit ? `<button type="button" class="ctox-button is-danger" data-drawer-delete>${t('delete', 'Löschen')}</button>` : ''}
        <button type="submit" class="ctox-button is-primary">${submitText}</button>
      </div>
    </form>
  `;

  body.querySelector('[data-drawer-close]').addEventListener('click', () => ctx.closeDrawers());
  body.querySelector('[data-drawer-cancel]').addEventListener('click', () => ctx.closeDrawers());

  if (isEdit) {
    body.querySelector('[data-drawer-delete]').addEventListener('click', async () => {
      const confirmDelete = confirm(t('confirmDeleteShift', 'Möchtest du diese Schicht wirklich löschen?'));
      if (!confirmDelete) return;

      const doc = await shiftflowCollection(ctx, 'planning_shifts').findOne(shift.id).exec();
      if (doc) {
        await doc.remove();
      }
      ctx.closeDrawers();
      hideInspectorSection(els);
    });
  }

  body.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const form = e.target;
    const empId = form.employee_id.value;
    const projId = form.project_id.value;
    const dateVal = form.date.value;
    const dept = form.department.value;
    const startStr = form.start_time.value;
    const endStr = form.end_time.value;
    const notes = form.notes.value;

    const getTimestamp = (dStr, tStr) => {
      const d = new Date(dStr);
      const [h, m] = tStr.split(':').map(Number);
      d.setHours(h, m, 0, 0);
      return d.getTime();
    };

    const startTime = getTimestamp(dateVal, startStr);
    const endTime = getTimestamp(dateVal, endStr);

    const proj = await shiftflowCollection(ctx, 'planning_projects').findOne(projId).exec();
    const projName = proj ? proj.name : t('shift', 'Dienst');

    const patch = {
      employee_id: empId,
      project_id: projId,
      title: `${t('shift', 'Dienst')} ${projName}`,
      start_time: startTime,
      end_time: endTime,
      department: dept,
      location: projName,
      notes,
      updated_at_ms: Date.now()
    };

    if (isEdit) {
      const doc = await shiftflowCollection(ctx, 'planning_shifts').findOne(shift.id).exec();
      if (doc) {
        await doc.incrementalPatch(patch);
      }
    } else {
      const id = 'shift_' + Date.now();
      await shiftflowCollection(ctx, 'planning_shifts').insert({
        id,
        kind: 'shift',
        status: 'published',
        created_at_ms: Date.now(),
        ...patch
      });
    }
    ctx.closeDrawers();
    hideInspectorSection(els);
  });

  ctx.openRightDrawer(body);
}

// -------------------------------------------------------------
// Bind Event Listeners
// -------------------------------------------------------------

function bindEventListeners(ctx, els) {
  const changeWeek = (days) => {
    currentWeekStart.setDate(currentWeekStart.getDate() + days);
    updateWeekRangeDisplay(els);
    renderGridHeader(els);
    latestConflicts = collectPlanningConflicts(latestShifts, latestEmployees, currentWeekStart);
    refreshPlanningSurfaces(els, ctx);
  };
  els.prevWeekBtn.addEventListener('click', () => changeWeek(-7));
  els.nextWeekBtn.addEventListener('click', () => changeWeek(7));

  const setMainView = (view) => {
    currentView = view;
    applyCenterViewState(els);
    if (view === 'billing') renderBillingAggregation(latestEmployees, latestProjects, latestTimeRecords, els, ctx);
  };
  els.viewSchedulerTabBtn.addEventListener('click', () => setMainView('scheduler'));
  els.viewConflictsTabBtn.addEventListener('click', () => setMainView('conflicts'));
  els.viewTimesheetsTabBtn.addEventListener('click', () => setMainView('timesheets'));
  els.viewBillingTabBtn.addEventListener('click', () => setMainView('billing'));

  els.toggleViewEmployeesBtn.addEventListener('click', () => {
    currentTimelineFocus = 'employees';
    applyTimelineState(els);
    renderGridHeader(els);
    renderSchedulerGrid(latestEmployees, latestProjects, latestShifts, els, ctx);
  });
  els.toggleViewProjectsBtn.addEventListener('click', () => {
    currentTimelineFocus = 'projects';
    applyTimelineState(els);
    renderGridHeader(els);
    renderSchedulerGrid(latestEmployees, latestProjects, latestShifts, els, ctx);
  });

  els.leftPane.addEventListener('ctox-pane-grammar-change', (event) => {
    const detail = event.detail || {};
    shiftListState = {
      search: String(detail.search || ''),
      view: detail.view === 'list' ? 'list' : 'cards',
      band: detail.band === 'drafts' ? 'drafts' : 'week',
      filters: {
        department: detail.filters?.department || 'all',
        status: detail.filters?.status || 'all',
      },
    };
    currentDeptFilter = shiftListState.filters.department;
    renderShiftList(els);
    renderSchedulerGrid(latestEmployees, latestProjects, latestShifts, els, ctx);
  });

  els.shiftList.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const edit = target?.closest('[data-edit-shift-id]');
    if (edit) {
      const shift = latestShifts.find((item) => item.id === edit.dataset.editShiftId);
      if (shift) void openShiftDrawer(shift, null, null, null, els, ctx);
      return;
    }
    const select = target?.closest('[data-select-shift-id]');
    if (select) selectShiftInPlace(select.dataset.selectShiftId, els, ctx);
  });

  els.addShiftBtn.addEventListener('click', () => {
    const date = new Date(currentWeekStart).toISOString().split('T')[0];
    void openShiftDrawer(null, date, '', '', els, ctx);
  });
  els.importShiftsBtn.addEventListener('click', () => els.shiftImportInput.click());
  els.shiftImportInput.addEventListener('change', async () => {
    const [file] = els.shiftImportInput.files || [];
    try {
      await importShiftRecords(file, ctx);
    } catch (error) {
      ctx.notifications?.show?.({ type: 'error', title: t('shiftPlanList', 'Schichten & Pläne'), message: error?.message || String(error) });
    } finally {
      els.shiftImportInput.value = '';
    }
  });
  els.exportShiftsBtn.addEventListener('click', exportShiftRecords);

  els.approveAllTimesheetsBtn.addEventListener('click', () => approveAllTimesheets(ctx));
  els.btnPublishSchedule.addEventListener('click', () => publishCurrentWeekSchedule(ctx, els));
  els.billingFilterApplyBtn.addEventListener('click', () => renderBillingAggregation(latestEmployees, latestProjects, latestTimeRecords, els, ctx));
  els.exportInvoiceDraftBtn.addEventListener('click', exportInvoiceDraftPayload);
  els.closeInspectorBtn.addEventListener('click', () => hideInspectorSection(els));
  els.btnAutoGenerateSchedule.addEventListener('click', () => autoGenerateSchedule(ctx, els));
  els.btnCheckConflicts.addEventListener('click', () => runConflictsAnalysis(ctx, els));
}

function triggerScheduleGridRefresh(ctx, els) {
  Promise.all([
    shiftflowCollection(ctx, 'planning_shifts').find().exec(),
    shiftflowCollection(ctx, 'planning_employees').find().exec(),
    shiftflowCollection(ctx, 'planning_projects').find().exec(),
  ]).then(([shifts, employees, projects]) => {
    latestShifts = shifts;
    latestEmployees = employees;
    latestProjects = projects;
    refreshPlanningSurfaces(els, ctx);
  });
}

function exportShiftRecords() {
  const shifts = visibleShiftListRecords().map((shift) => typeof shift.toJSON === 'function' ? shift.toJSON() : { ...shift });
  const payload = { format: 'ctox-shiftflow-shifts-v1', exported_at_ms: Date.now(), shifts };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `shiftflow-shifts-${new Date().toISOString().slice(0, 10)}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}

function validShiftImportRecord(record) {
  return record && typeof record === 'object'
    && typeof record.id === 'string' && record.id.length > 0
    && Number.isFinite(Number(record.start_time))
    && Number.isFinite(Number(record.end_time))
    && Number(record.end_time) > Number(record.start_time)
    && typeof record.status === 'string'
    && Number.isFinite(Number(record.created_at_ms))
    && Number.isFinite(Number(record.updated_at_ms));
}

async function importShiftRecords(file, ctx) {
  if (!file) return;
  const parsed = JSON.parse(await file.text());
  const records = (Array.isArray(parsed) ? parsed : parsed?.shifts || []).filter(validShiftImportRecord);
  if (!records.length) throw new Error(t('shiftImportEmpty', 'Die JSON-Datei enthält keine gültigen Schichten.'));
  const collection = shiftflowCollection(ctx, 'planning_shifts');
  if (!collection || !canWriteCollection(ctx, 'planning_shifts')) throw new Error(t('shiftImportDenied', 'Schichten können mit den aktuellen Rechten nicht importiert werden.'));
  for (const record of records) {
    const normalized = {
      ...record,
      kind: record.kind || 'shift',
      start_time: Number(record.start_time),
      end_time: Number(record.end_time),
      created_at_ms: Number(record.created_at_ms),
      updated_at_ms: Number(record.updated_at_ms),
    };
    if (typeof collection.upsert === 'function') await collection.upsert(normalized);
    else {
      const existing = await collection.findOne(normalized.id).exec();
      if (existing) await existing.incrementalPatch(normalized);
      else await collection.insert(normalized);
    }
  }
  ctx.notifications?.show?.({ type: 'success', title: t('shiftPlanList', 'Schichten & Pläne'), message: t('shiftImportDone', '{0} Schichten importiert.', String(records.length)) });
}

// -------------------------------------------------------------
// Advanced Planning Automations (Mock intelligence)
// -------------------------------------------------------------

async function autoGenerateSchedule(ctx, els) {
  const employeesCollection = shiftflowCollection(ctx, 'planning_employees');
  const projectsCollection = shiftflowCollection(ctx, 'planning_projects');
  const shiftsCollection = shiftflowCollection(ctx, 'planning_shifts');
  if (!employeesCollection || !projectsCollection || !shiftsCollection) return;

  const employees = await employeesCollection.find({ selector: { status: 'active' } }).exec();
  const projects = await projectsCollection.find({ selector: { status: 'active' } }).exec();

  if (employees.length === 0 || projects.length === 0) {
    alert(t('aiSetupRequirement', 'Es müssen mindestens ein aktiver Mitarbeiter und ein aktives Projekt existieren!'));
    return;
  }

  // Clear existing draft shifts for the current week first
  const monday = new Date(currentWeekStart);
  const dayStart = monday.getTime();
  const dayEnd = dayStart + 7 * 24 * 3600000 - 1;

  const weekShifts = await shiftsCollection.find({
    selector: {
      start_time: { $gte: dayStart, $lte: dayEnd }
    }
  }).exec();

  for (const s of weekShifts) {
    await s.remove();
  }

  // Generate a basic balanced plan: assign each employee to a project for Mo-Fr (08:00 - 16:00)
  const getTimestamp = (dayOffset, hourStr) => {
    const d = new Date(monday);
    d.setDate(monday.getDate() + dayOffset);
    const [h, m] = hourStr.split(':').map(Number);
    d.setHours(h, m, 0, 0);
    return d.getTime();
  };

  let shiftIndex = 1;
  for (let day = 0; day < 5; day++) { // Mon-Fri
    for (const [empIdx, emp] of employees.entries()) {
      // Rotate projects among employees
      const proj = projects[ (empIdx + day) % projects.length ];

      const startTime = getTimestamp(day, '08:00');
      const endTime = getTimestamp(day, '16:00');

      await shiftsCollection.insert({
        id: `shift_auto_${Date.now()}_${shiftIndex++}`,
        kind: 'shift',
        employee_id: emp.id,
        project_id: proj.id,
        title: `${t('shift', 'Dienst')} ${proj.name}`,
        start_time: startTime,
        end_time: endTime,
        location: proj.name,
        department: emp.departments[0] || 'Service',
        status: 'draft',
        notes: t('autoPlannedNotes', 'Automatisch geplant vom CTOX Assistenten'),
        created_at_ms: Date.now(),
        updated_at_ms: Date.now()
      });
    }
  }

  ctx.notifications?.show?.({
    type: 'success',
    title: t('schedulePlanning', 'Einsatzplanung'),
    message: t('aiGenerateSuccessPlain', 'Dienstplanentwurf für Montag bis Freitag wurde angelegt.'),
  });
}

export function collectPlanningConflicts(shifts, employees, weekStart = currentWeekStart) {
  const monday = new Date(weekStart);
  const weekStartMs = monday.getTime();
  const weekEndMs = weekStartMs + 7 * 24 * 3600000 - 1;
  const weekShifts = shifts.filter((shift) => shift.start_time >= weekStartMs && shift.start_time <= weekEndMs);
  const conflicts = [];

  // The established Shiftflow rules remain unchanged: >42 weekly hours,
  // overlapping assignments, 11h rest, 10h extended daily cap, AÜG duration.
  employees.forEach((employee) => {
    const employeeShifts = weekShifts.filter((shift) => shift.employee_id === employee.id);
    const totalHours = employeeShifts.reduce((sum, shift) => sum + (shift.end_time - shift.start_time) / 3600000, 0);
    if (totalHours > 42) {
      conflicts.push({
        type: 'overtime',
        employeeId: employee.id,
        message: t('conflictMaxHours', '<strong>{0}</strong> überschreitet die wöchentliche Höchstarbeitszeit ({1} Std. geplant, Soll: {2} Std.)', employee.name, totalHours.toFixed(1), employee.weekly_target_hours || 40),
      });
    }
  });

  for (let i = 0; i < weekShifts.length; i += 1) {
    for (let j = i + 1; j < weekShifts.length; j += 1) {
      const first = weekShifts[i];
      const second = weekShifts[j];
      if (first.employee_id === second.employee_id && first.start_time < second.end_time && second.start_time < first.end_time) {
        const employee = employees.find((item) => item.id === first.employee_id);
        conflicts.push({
          type: 'overlap',
          employeeId: first.employee_id,
          shiftId: second.id,
          message: t('conflictDoubleBooking', 'Doppelbuchung für <strong>{0}</strong> am {1} erkannt.', employee ? employee.name : t('employee', 'Mitarbeiter'), new Date(first.start_time).toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE')),
        });
      }
    }
  }

  const employeesById = new Map(employees.map((employee) => [employee.id, employee]));
  const employeeName = (id) => employeesById.get(id)?.name || t('employee', 'Mitarbeiter');
  const toPlain = (shift) => ({ id: shift.id, employee_id: shift.employee_id, project_id: shift.project_id, start_time: shift.start_time, end_time: shift.end_time });
  const weekShiftObjects = weekShifts.map(toPlain);
  for (const violation of checkRestPeriods(weekShiftObjects)) {
    conflicts.push({
      type: 'rest_period',
      employeeId: violation.employeeId,
      shiftId: violation.shiftId,
      message: t('conflictRestPeriod', '<strong>{0}</strong>: Ruhezeit unter 11 Std. ({1} Std.) zwischen zwei Schichten.', employeeName(violation.employeeId), violation.restHours),
    });
  }
  for (const violation of checkDailyHours(weekShiftObjects, { extended: true })) {
    conflicts.push({
      type: 'daily_hours',
      employeeId: violation.employeeId,
      message: t('conflictDailyHours', '<strong>{0}</strong>: Tägliche Höchstarbeitszeit überschritten ({1} Std., max {2} Std.).', employeeName(violation.employeeId), violation.hours, violation.capHours),
    });
  }
  for (const assignment of accumulateUeberlassung(shifts.map(toPlain))) {
    if (assignment.overCap) {
      conflicts.push({
        type: 'ueberlassung',
        employeeId: assignment.employeeId,
        projectId: assignment.projectId,
        message: t('conflictUeberlassung', '<strong>{0}</strong>: Höchstüberlassungsdauer überschritten ({1} Tage, max {2}).', employeeName(assignment.employeeId), assignment.days, assignment.capDays),
      });
    } else if (assignment.nearCap) {
      conflicts.push({
        type: 'ueberlassung',
        employeeId: assignment.employeeId,
        projectId: assignment.projectId,
        message: t('conflictUeberlassungNear', '<strong>{0}</strong>: Höchstüberlassungsdauer fast erreicht ({1} Tage).', employeeName(assignment.employeeId), assignment.days),
      });
    }
  }
  return conflicts;
}

function renderConflicts(conflicts, els) {
  if (!els.conflictsList) return;
  els.conflictsList.innerHTML = conflicts.length ? conflicts.map((conflict, index) => {
    const recordId = conflict.shiftId || conflict.employeeId || conflict.projectId || `conflict-${index}`;
    return `
      <article class="ctox-callout is-danger shiftflow-conflict-item" data-context-record-id="${escapeHtml(recordId)}" data-context-record-type="planning_conflict" data-context-label="${escapeHtml(`${t('tabConflicts', 'Konflikt')} ${index + 1}`)}">
        <span data-conflict-symbol aria-hidden="true">!</span>
        <div>${conflict.message}</div>
      </article>
    `;
  }).join('') : `<div class="ctox-empty">${t('noConflictsDetected', 'Keine aktiven Konflikte erkannt. Der Dienstplan erfüllt alle Vorgaben.')}</div>`;
}

async function runConflictsAnalysis(ctx, els) {
  const shiftsCollection = shiftflowCollection(ctx, 'planning_shifts');
  const employeesCollection = shiftflowCollection(ctx, 'planning_employees');
  if (!shiftsCollection || !employeesCollection) return;
  latestShifts = await shiftsCollection.find().exec();
  latestEmployees = await employeesCollection.find().exec();
  latestConflicts = collectPlanningConflicts(latestShifts, latestEmployees, currentWeekStart);
  renderConflicts(latestConflicts, els);
  renderMainViewCounts(els);
  ctx.notifications?.show?.({
    type: latestConflicts.length ? 'warning' : 'success',
    title: t('conflictsAndWarnings', 'Konflikte & Warnungen'),
    message: latestConflicts.length
      ? t('conflictCountResult', '{0} Konflikte gefunden.', String(latestConflicts.length))
      : t('noConflictsDetected', 'Keine aktiven Konflikte erkannt. Der Dienstplan erfüllt alle Vorgaben.'),
  });
}

function exportInvoiceDraftPayload() {
  const data = globalThis.CTOX_LAST_AGGREGATION;
  if (!data || !data.totals || data.totals.totalRevenue === 0) {
    alert(t('noBillableHours', 'Es sind keine freigegebenen Stunden im ausgewählten Zeitraum vorhanden, die abgerechnet werden können.'));
    return;
  }

  const draft = {
    metadata: {
      document_type: 'INVOICE_AGGREGATION_DRAFT',
      creator: 'CTOX Shiftflow Module',
      created_at: new Date().toISOString(),
      billing_period: {
        start_date: data.dateRange.start,
        end_date: data.dateRange.end
      }
    },
    financial_summary: {
      total_billable_hours: Number(data.totals.totalHours.toFixed(2)),
      gross_revenue_eur: Number(data.totals.totalRevenue.toFixed(2)),
      internal_labor_cost_eur: Number(data.totals.totalCost.toFixed(2)),
      gross_margin_eur: Number(data.totals.totalMarginVal.toFixed(2)),
      margin_percent: Number(data.totals.totalMarginPercentVal.toFixed(1))
    },
    projects: Object.values(data.aggregation)
      .filter(pData => pData.hours > 0)
      .map(pData => {
        const p = pData.project;
        const revenue = pData.revenue;
        const cost = pData.cost;
        const margin = revenue - cost;

        return {
          project_id: p.id,
          project_name: p.name,
          client_name: p.client,
          location: p.location,
          hours_logged: Number(pData.hours.toFixed(2)),
          billing_rate: p.hourly_rate,
          total_revenue: Number(revenue.toFixed(2)),
          total_cost: Number(cost.toFixed(2)),
          margin: Number(margin.toFixed(2)),
          margin_percent: Number((revenue > 0 ? (margin / revenue) * 100 : 0.0).toFixed(1)),
          details: pData.details.map(d => ({
            employee: d.employee_name,
            hours: Number(d.hours.toFixed(2)),
            revenue_share: Number(d.revenue.toFixed(2))
          }))
        };
      })
  };

  // Trigger file download of JSON invoice draft payload
  const blob = new Blob([JSON.stringify(draft, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `ctox_rechnungsentwurf_${data.dateRange.start}_zu_${data.dateRange.end}.json`;
  a.click();
  URL.revokeObjectURL(url);

  alert(t('invoiceDraftDownloadSuccess', 'Rechnungsentwurf erfolgreich erstellt und heruntergeladen.\n\nDu kannst diese Datei direkt im CTOX Rechnungs-Modul einlesen.'));
}

// -------------------------------------------------------------
// Utilities
// -------------------------------------------------------------

function escapeHtml(str) {
  if (!str) return '';
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function formatTime(date) {
  const localeStr = lang === 'en' ? 'en-US' : 'de-DE';
  return date.toLocaleTimeString(localeStr, { hour: '2-digit', minute: '2-digit' });
}

function applyStaticLabels(root, t) {
  root.querySelectorAll('[data-t]').forEach(el => el.textContent = t(el.dataset.t));
  root.querySelectorAll('[data-t-title]').forEach(el => el.title = t(el.dataset.tTitle));
  root.querySelectorAll('[data-t-aria]').forEach(el => el.setAttribute('aria-label', t(el.dataset.tAria)));
  root.querySelectorAll('[data-t-placeholder]').forEach(el => el.placeholder = t(el.dataset.tPlaceholder));
}

// Right-click context is shell-owned. Record containers expose the explicit
// data-context-record-* trio; Shiftflow does not dispatch shell events or own a
// context menu.

export const __shiftflowTestHooks = {
  filterShiftflowEmployeesForPlanner,
  getShiftflowPressedState,
  getWeekBoundsMs,
  applyShiftListSelection,
  collectPlanningConflicts,
};
