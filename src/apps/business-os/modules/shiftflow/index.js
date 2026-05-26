/* src/apps/business-os/modules/shiftflow/index.js */
import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';

const MOD_BUILD = '20260520-shiftflow-v2';

let activeSubscriptions = [];
let currentWeekStart = getMondayOfCurrentWeek();
let currentView = 'scheduler'; // 'scheduler', 'timesheets', or 'billing'
let currentTimelineFocus = 'employees'; // 'employees' or 'projects'
let selectedEmployeeId = null;
let currentDeptFilter = 'all';
let lang = 'de';
let t = (key, fallback) => fallback ?? key;
let contextMenu = null;
let contextMenuCleanup = null;

export async function mount(ctx) {
  if (ctx.db && ctx.db.raw) {
    ctx.db = ctx.db.raw;
  }

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

  // Initialize UI variables & elements
  const els = {
    app: ctx.host.querySelector('.shiftflow-app'),
    activeEmployeeList: ctx.host.querySelector('#activeEmployeeList'),
    inactiveEmployeeList: ctx.host.querySelector('#inactiveEmployeeList'),
    activeEmployeesCount: ctx.host.querySelector('#activeEmployeesCount'),
    inactiveEmployeesCount: ctx.host.querySelector('#inactiveEmployeesCount'),

    // Project management elements
    projectList: ctx.host.querySelector('#projectList'),
    addProjectBtn: ctx.host.querySelector('#addProjectBtn'),

    // View content panes
    schedulerView: ctx.host.querySelector('#schedulerView'),
    timesheetsView: ctx.host.querySelector('#timesheetsView'),
    billingView: ctx.host.querySelector('#billingView'),

    // Dual timeline toggles
    toggleViewEmployeesBtn: ctx.host.querySelector('#toggleViewEmployeesBtn'),
    toggleViewProjectsBtn: ctx.host.querySelector('#toggleViewProjectsBtn'),
    schedulerCornerCell: ctx.host.querySelector('#schedulerCornerCell'),

    // Table / Grid elements
    schedulerGridHeader: ctx.host.querySelector('#schedulerGridHeader'),
    schedulerGridBody: ctx.host.querySelector('#schedulerGridBody'),
    schedulerWeekRange: ctx.host.querySelector('#schedulerWeekRange'),
    centerPaneTitle: ctx.host.querySelector('#centerPaneTitle'),
    prevWeekBtn: ctx.host.querySelector('#prevWeekBtn'),
    nextWeekBtn: ctx.host.querySelector('#nextWeekBtn'),
    approveAllTimesheetsBtn: ctx.host.querySelector('#approveAllTimesheetsBtn'),
    timesheetsList: ctx.host.querySelector('#timesheetsList'),
    departmentFilterSelect: ctx.host.querySelector('#departmentFilterSelect'),
    employeeSearchInput: ctx.host.querySelector('#employeeSearchInput'),
    addEmployeeBtn: ctx.host.querySelector('#addEmployeeBtn'),

    // AI and Inspector
    aiPlannerChatBody: ctx.host.querySelector('#aiPlannerChatBody'),
    btnAutoGenerateSchedule: ctx.host.querySelector('#btnAutoGenerateSchedule'),
    btnCheckConflicts: ctx.host.querySelector('#btnCheckConflicts'),
    btnFindReplacements: ctx.host.querySelector('#btnFindReplacements'),
    conflictsList: ctx.host.querySelector('#conflictsList'),
    detailInspectorSection: ctx.host.querySelector('#detailInspectorSection'),
    aiPlannerSection: ctx.host.querySelector('#aiPlannerSection'),
    closeInspectorBtn: ctx.host.querySelector('#closeInspectorBtn'),
    inspectorContent: ctx.host.querySelector('#inspectorContent'),
    inspectorTitle: ctx.host.querySelector('#inspectorTitle'),


    // Billing Workbench elements
    billingStartDate: ctx.host.querySelector('#billingStartDate'),
    billingEndDate: ctx.host.querySelector('#billingEndDate'),
    billingFilterApplyBtn: ctx.host.querySelector('#billingFilterApplyBtn'),
    exportInvoiceDraftBtn: ctx.host.querySelector('#exportInvoiceDraftBtn'),
    billingTotalRevenue: ctx.host.querySelector('#billingTotalRevenue'),
    billingTotalCost: ctx.host.querySelector('#billingTotalCost'),
    billingTotalMargin: ctx.host.querySelector('#billingTotalMargin'),
    billingAggregationBody: ctx.host.querySelector('#billingAggregationBody'),

    // Center tabs
    viewSchedulerTabBtn: ctx.host.querySelector('#viewSchedulerTabBtn'),
    viewTimesheetsTabBtn: ctx.host.querySelector('#viewTimesheetsTabBtn'),
    viewBillingTabBtn: ctx.host.querySelector('#viewBillingTabBtn')
  };

  // Seed default dates in Billing selector (current month)
  const today = new Date();
  const firstDay = new Date(today.getFullYear(), today.getMonth(), 1);
  const lastDay = new Date(today.getFullYear(), today.getMonth() + 1, 0);
  els.billingStartDate.value = firstDay.toISOString().split('T')[0];
  els.billingEndDate.value = lastDay.toISOString().split('T')[0];

  // Ensure DB collections exist and seed mock data if empty
  await seedMockDataIfEmpty(ctx.db);

  // Setup reactive RxDB subscriptions
  setupSubscriptions(ctx, els);

  // Bind Event Listeners
  bindEventListeners(ctx, els);

  // Set up column resizing
  const resizeCleanup = setupShiftflowColumnResizing(els.app);

  // Initial UI updates
  updateWeekRangeDisplay(els);
  renderGridHeader(els);

  // 5. Initialize CTOX unified context menu
  contextMenuCleanup = initShiftflowContextMenu(els, ctx);

  // Return unmount function
  return () => {
    activeSubscriptions.forEach(sub => sub.unsubscribe?.());
    activeSubscriptions = [];
    resizeCleanup?.();
    contextMenuCleanup?.();
    contextMenu?.remove();
    contextMenu = null;
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
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
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

  const baseHeader = `<div class="grid-corner-cell">${escapeHtml(els.schedulerCornerCell.textContent)}</div>`;
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

// -------------------------------------------------------------
// Database Operations & Data Seeding
// -------------------------------------------------------------

async function seedMockDataIfEmpty(db) {
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
  if (!ctx.db) return;

  // 1. Reactive Employees Subscription
  const empSub = ctx.db.planning_employees.find().$.subscribe(async (employees) => {
    const timeRecords = await ctx.db.planning_time_records.find().exec();
    renderEmployeesList(employees, timeRecords, els, ctx);

    const projects = await ctx.db.planning_projects.find().exec();
    const shifts = await ctx.db.planning_shifts.find().exec();
    renderSchedulerGrid(employees, projects, shifts, els, ctx);

    if (selectedEmployeeId) {
      openEmployeeDetailsInspector(selectedEmployeeId, employees, els, ctx);
    }
  });
  activeSubscriptions.push(empSub);

  // 2. Reactive Projects Subscription
  const projSub = ctx.db.planning_projects.find().$.subscribe(async (projects) => {
    renderProjectsList(projects, els, ctx);

    const employees = await ctx.db.planning_employees.find().exec();
    const shifts = await ctx.db.planning_shifts.find().exec();
    renderSchedulerGrid(employees, projects, shifts, els, ctx);
    renderBillingAggregation(employees, projects, await ctx.db.planning_time_records.find().exec(), els);

    if (selectedEmployeeId) {
      openEmployeeDetailsInspector(selectedEmployeeId, employees, els, ctx);
    }
  });
  activeSubscriptions.push(projSub);

  // 3. Reactive Shifts Subscription
  const shiftSub = ctx.db.planning_shifts.find().$.subscribe(async (shifts) => {
    const employees = await ctx.db.planning_employees.find().exec();
    const projects = await ctx.db.planning_projects.find().exec();
    renderSchedulerGrid(employees, projects, shifts, els, ctx);

    if (selectedEmployeeId) {
      openEmployeeDetailsInspector(selectedEmployeeId, employees, els, ctx);
    }
  });
  activeSubscriptions.push(shiftSub);

  // 4. Reactive Time Records Subscription
  const timeSub = ctx.db.planning_time_records.find().$.subscribe(async (records) => {
    const employees = await ctx.db.planning_employees.find().exec();
    const projects = await ctx.db.planning_projects.find().exec();
    const shifts = await ctx.db.planning_shifts.find().exec();

    renderTimesheets(employees, projects, shifts, records, els);
    renderEmployeesList(employees, records, els, ctx);
    renderBillingAggregation(employees, projects, records, els);

    if (selectedEmployeeId) {
      openEmployeeDetailsInspector(selectedEmployeeId, employees, els, ctx);
    }
  });
  activeSubscriptions.push(timeSub);
}

// -------------------------------------------------------------
// Renders
// -------------------------------------------------------------

function renderEmployeesList(employees, timeRecords, els, ctx) {
  const searchQuery = els.employeeSearchInput.value.toLowerCase().trim();

  // Filter by dept & search query
  let filtered = employees;
  if (currentDeptFilter !== 'all') {
    filtered = filtered.filter(emp => emp.departments?.includes(currentDeptFilter));
  }
  if (searchQuery) {
    filtered = filtered.filter(emp => emp.name.toLowerCase().includes(searchQuery) || emp.role.toLowerCase().includes(searchQuery));
  }

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
      <div class="employee-card ${isActive ? 'active' : ''} ${selectedEmployeeId === emp.id ? 'selected' : ''}" data-emp-id="${emp.id}" draggable="true">
        <div class="emp-avatar" style="background: ${emp.avatar_color || '#6366f1'}">${initials}</div>
        <div class="emp-info">
          <div class="emp-name">${escapeHtml(emp.name)}</div>
          <div class="emp-meta">${escapeHtml(emp.role)}</div>
        </div>
        ${isActive ? '<span class="status-indicator active"></span>' : ''}
      </div>
    `;

    if (isActive) {
      activeSection.push(cardHtml);
    } else {
      inactiveSection.push(cardHtml);
    }
  });

  els.activeEmployeeList.innerHTML = activeSection.length ? activeSection.join('') : '<div class="pane-subtitle" style="padding: 4px 8px;">Niemand im Dienst</div>';
  els.inactiveEmployeeList.innerHTML = inactiveSection.length ? inactiveSection.join('') : '<div class="pane-subtitle" style="padding: 4px 8px;">Keine weiteren Mitarbeiter</div>';

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
    const activeBadge = proj.status === 'active' ? `<span class="project-card-badge" style="background:${proj.color || '#6366f1'};"></span>` : '';

    return `
      <div class="project-card" data-proj-id="${proj.id}">
        <div class="project-card-info">
          <div class="project-card-name">${escapeHtml(proj.name)}</div>
          <div class="project-card-client">${escapeHtml(proj.client)} · ${escapeHtml(proj.location || '')}</div>
        </div>
        <div class="project-card-meta">
          ${activeBadge}
          <span class="project-card-rate">${proj.hourly_rate.toFixed(2)} €/h</span>
        </div>
      </div>
    `;
  }).join('');

  els.projectList.innerHTML = projectCards || '<div class="pane-subtitle" style="padding: 4px 8px;">Keine Projekte angelegt</div>';

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

  if (currentTimelineFocus === 'employees') {
    // 1. Classic Employee-Centric Timeline View
    let filteredEmployees = employees;
    if (currentDeptFilter !== 'all') {
      filteredEmployees = filteredEmployees.filter(emp => emp.departments?.includes(currentDeptFilter));
    }

    const rows = filteredEmployees.map(emp => {
      const initials = emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase();

      const rowHeader = `
        <div class="row-employee-cell">
          <div class="emp-avatar" style="background: ${emp.avatar_color || '#6366f1'}">${initials}</div>
          <div class="emp-info">
            <div class="emp-name">${escapeHtml(emp.name)}</div>
            <div class="emp-meta">${escapeHtml(emp.role)}</div>
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
          const projColor = proj ? proj.color : 'var(--shiftflow-accent)';

          return `
            <div class="shift-card dept-${shift.department?.toLowerCase() || 'service'} ${shift.status || 'published'}" data-shift-id="${shift.id}">
              <div class="shift-time">
                <span>${startStr} - ${endStr}</span>
                <span class="shift-tag">${shift.department}</span>
              </div>
              <div style="font-weight:700; margin-top:2px;">${escapeHtml(shift.title || 'Schicht')}</div>
              <div class="pane-subtitle" style="margin-top:2px; font-size:9px; color:inherit;">${duration} Std · ${escapeHtml(projName)}</div>
              <div class="project-strip" style="background:${projColor};"></div>
            </div>
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
        <div class="scheduler-row">
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
        <div class="row-employee-cell" style="border-left: 4px solid ${proj.color || '#6366f1'}; padding-left: 8px;">
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
          return shift.project_id === proj.id &&
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
          const avatarColor = emp ? emp.avatar_color : '#94a3b8';
          const initials = emp ? emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase() : '?';

          return `
            <div class="shift-card dept-${shift.department?.toLowerCase() || 'service'} ${shift.status || 'published'}" data-shift-id="${shift.id}" style="display:flex; flex-direction:column;">
              <div style="display:flex; align-items:center; gap:6px; margin-bottom:4px;">
                <div class="emp-avatar" style="width:16px; height:16px; font-size:7px; background:${avatarColor};">${initials}</div>
                <span style="font-weight:700; font-size:11px;">${escapeHtml(empName)}</span>
              </div>
              <div class="shift-time" style="font-size:10px;">
                <span>${startStr} - ${endStr} (${duration}h)</span>
              </div>
              <div class="pane-subtitle" style="font-size:9.5px; color:inherit; margin-top:2px;">${escapeHtml(shift.title || 'Schicht')}</div>
            </div>
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
        <div class="scheduler-row">
          ${rowHeader}
          ${dayCells}
        </div>
      `;
    }).join('');

    els.schedulerGridBody.innerHTML = rows || '<div class="conflict-empty-state" style="padding: 24px;">Keine aktiven Projekte gefunden. Lege links ein neues an!</div>';
  }

  // Bind click & double-click handlers on shift cards
  els.schedulerGridBody.querySelectorAll('.shift-card').forEach(card => {
    card.addEventListener('click', (e) => {
      e.stopPropagation();
      const shift = shifts.find(s => s.id === card.dataset.shiftId);
      if (shift) openShiftDrawer(shift, null, null, null, els, ctx);
    });

    card.addEventListener('dblclick', (e) => {
      e.stopPropagation();
      const shift = shifts.find(s => s.id === card.dataset.shiftId);
      if (shift) openShiftDrawer(shift, null, null, null, els, ctx);
    });
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
      const db = globalThis.CTOX_ACTIVE_DB || els.activeEmployeeList.__ctx__db;
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
}

function renderTimesheets(employees, projects, shifts, records, els) {
  // Only show records with 'pending' status for approvals
  const pendingRecords = records.filter(rec => rec.approval_status === 'pending' && rec.end_time !== null);

  if (pendingRecords.length === 0) {
    els.timesheetsList.innerHTML = `
      <div class="conflict-empty-state" style="padding: 40px;">
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

    let sollHtml = `<span style="color:var(--shiftflow-muted); font-size:11px;">${t('noShiftPlanned', 'Keine Schicht eingeplant (Überstunden)')}</span>`;
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
        const badgeClass = diff > 0 ? 'badge-success' : 'badge-danger';
        warningBadge = `<span class="shiftflow-badge ${badgeClass}" style="font-weight:700; margin-left:8px;">${t('hoursDeviationText', '{0}{1} Std Abweichung', sign, diff.toFixed(1))}</span>`;
      }
    }

    // 2. Resolve Project context
    const proj = projects.find(p => p.id === rec.project_id);
    const projName = proj ? proj.name : t('noProject', 'Ohne Projekt');
    const projColor = proj ? proj.color : 'var(--shiftflow-line)';

    return `
      <div class="timesheet-card" data-rec-id="${rec.id}">
        <div class="timesheet-card-main">
          <div class="emp-avatar" style="background: ${emp ? emp.avatar_color : '#94a3b8'}">${initials}</div>
          <div class="timesheet-details">
            <div style="font-weight:800; font-size:14px; color:var(--shiftflow-text);">${escapeHtml(emp ? emp.name : t('employee', 'Mitarbeiter'))} <span style="font-weight:400; font-size:12px; color:var(--shiftflow-muted);">(${escapeHtml(emp ? emp.role : '')})</span></div>
            <div style="margin-top:4px; font-size:12px; display:flex; align-items:center; gap:8px;">
              <span>${t('dateText', 'Datum: <strong>{0}</strong>', dateStr)}</span>
              <span class="shiftflow-badge" style="background:color-mix(in srgb, ${projColor} 15%, transparent); color:var(--shiftflow-text); padding:1px 6px;">${t('projects', 'Projekt')}: ${escapeHtml(projName)}</span>
            </div>
            <div style="margin-top:4px; font-size:12px; color:var(--shiftflow-muted);">
              ${sollHtml}
            </div>
            ${rec.notes ? `<div class="timesheet-notes" style="margin-top:6px; font-style:italic;">"${escapeHtml(rec.notes)}"</div>` : ''}
          </div>
          <div class="timesheet-hours-block" style="text-align:right;">
            <div class="timesheet-hours">${workedHoursStr} ${lang === 'en' ? 'hrs.' : 'Std.'}</div>
            <div class="timesheet-time-range">${startStr} - ${endStr} ${breakMin ? `(${t('pauseText', 'Pause: {0}m', breakMin)})` : ''}</div>
            <div style="margin-top:4px;">${warningBadge}</div>
          </div>
        </div>
        <div class="timesheet-card-actions">
          <button class="os-btn os-btn-danger btn-reject" data-rec-id="${rec.id}">${t('reject', 'Ablehnen')}</button>
          <button class="os-btn os-btn-primary btn-approve" data-rec-id="${rec.id}">${t('approveAndBook', 'Genehmigen & Buchen')}</button>
        </div>
      </div>
    `;
  }).join('');

  els.timesheetsList.innerHTML = listHtml;

  // Bind individual timesheet approvals / rejections
  els.timesheetsList.querySelectorAll('.btn-approve').forEach(btn => {
    btn.addEventListener('click', () => {
      approveSingleRecord(btn.dataset.recId, els);
    });
  });

  els.timesheetsList.querySelectorAll('.btn-reject').forEach(btn => {
    btn.addEventListener('click', () => {
      rejectSingleRecord(btn.dataset.recId, els);
    });
  });
}

function renderBillingAggregation(employees, projects, records, els) {
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

      let badgeClass = 'high';
      if (marginPercent < 30) badgeClass = 'low';
      else if (marginPercent < 55) badgeClass = 'mid';

      const billingDisplay = p.hourly_rate > 0 ? `${p.hourly_rate.toFixed(2)} €/h` : 'Nicht abrechenbar';

      return `
        <tr>
          <td style="font-weight:700; display:flex; align-items:center; gap:8px; height:48px;">
            <span style="width:8px; height:8px; border-radius:50%; background:${p.color || '#94a3b8'}"></span>
            ${escapeHtml(p.name)}
          </td>
          <td>${escapeHtml(p.client || '')}</td>
          <td class="text-right" style="font-weight:600;">${data.hours.toFixed(1)} Std</td>
          <td class="text-right">${billingDisplay}</td>
          <td class="text-right" style="font-weight:700;">${data.revenue.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} €</td>
          <td class="text-right text-cost" style="color:#ef4444;">${data.cost.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} €</td>
          <td class="text-right">
            <span class="margin-badge ${badgeClass}">${marginPercent.toFixed(1)}% (${grossMargin.toLocaleString('de-DE', { maximumFractionDigits: 0 })} €)</span>
          </td>
          <td class="text-right">
            <button class="os-btn btn-billing-inspect" data-proj-id="${p.id}" style="padding:4px 8px; font-size:11px;">Details</button>
          </td>
        </tr>
      `;
    }).join('');

  els.billingAggregationBody.innerHTML = rowsHtml || `
    <tr>
      <td colspan="8" style="text-align:center; padding:32px; color:var(--shiftflow-muted);">
        Keine freigegebenen Zeiterfassungen im gewählten Zeitraum vorhanden.
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
      <div style="display:flex; justify-content:space-between; padding:12px 0; border-bottom:1px solid var(--shiftflow-line);">
        <div>
          <div style="font-weight:700; color:var(--shiftflow-text);">${escapeHtml(item.employee_name)}</div>
          <div style="font-size:11px; color:var(--shiftflow-muted); margin-top:2px;">${t('billingDetailsCostLabel', 'Umsatz: {0} € · Lohnkosten: {1} €', item.revenue.toFixed(2), item.cost.toFixed(2))}</div>
        </div>
        <div style="font-weight:700; align-self:center; color:var(--shiftflow-text);">${item.hours.toFixed(1)} ${t('hoursShort', 'Std.')}</div>
      </div>
    `;
  }).join('');

  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span class="shiftflow-kicker">${t('billingEvaluation', 'Auswertung')}</span>
        <h2>${escapeHtml(projectData.project.name)}</h2>
      </div>
      <button class="icon-button" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}">×</button>
    </header>
    <div style="padding:16px; display:flex; flex-direction:column; gap:16px;">
      <div style="display:grid; grid-template-columns:1fr 1fr; gap:12px;">
        <div style="background:var(--shiftflow-surface-2); padding:10px; border-radius:8px; border:1px solid var(--shiftflow-line);">
          <span class="shiftflow-kicker">${t('colCustomer', 'Kunde')}</span>
          <div style="font-weight:700; font-size:13px; margin-top:2px; color:var(--shiftflow-text);">${escapeHtml(projectData.project.client)}</div>
        </div>
        <div style="background:var(--shiftflow-surface-2); padding:10px; border-radius:8px; border:1px solid var(--shiftflow-line);">
          <span class="shiftflow-kicker">${t('location', 'Einsatzort')}</span>
          <div style="font-weight:700; font-size:13px; margin-top:2px; color:var(--shiftflow-text);">${escapeHtml(projectData.project.location || t('noInfo', 'Keine Angabe'))}</div>
        </div>
      </div>

      <hr style="border:0; height:1px; background:var(--shiftflow-line); margin:4px 0;" />

      <div style="display:flex; flex-direction:column; gap:4px;">
        <span class="shiftflow-kicker">${t('employeeDistribution', 'Mitarbeiter Aufteilung')}</span>
        <div style="display:flex; flex-direction:column; max-height:400px; overflow-y:auto;" class="os-scrollbar">
          ${itemsHtml || `<div style="color:var(--shiftflow-muted); padding:12px; text-align:center; font-style:italic;">${t('noEntries', 'Keine Einträge')}</div>`}
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

async function approveSingleRecord(recId, els) {
  const db = globalThis.CTOX_ACTIVE_DB || els.activeEmployeeList.__ctx__db;
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

async function rejectSingleRecord(recId, els) {
  const db = globalThis.CTOX_ACTIVE_DB || els.activeEmployeeList.__ctx__db;
  if (!db) return;

  const doc = await db.planning_time_records.findOne(recId).exec();
  if (doc) {
    await doc.incrementalPatch({
      approval_status: 'rejected',
      updated_at_ms: Date.now()
    });
  }
}

async function approveAllTimesheets(els) {
  const db = globalThis.CTOX_ACTIVE_DB || els.activeEmployeeList.__ctx__db;
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

// Shift planning forms are now built dynamically inside the side drawer via openShiftDrawer

async function openShiftDetails(shiftId, shifts, employees, els, ctx) {
  const shift = shifts.find(s => s.id === shiftId);
  if (!shift) return;

  const emp = employees.find(e => e.id === shift.employee_id);
  const db = globalThis.CTOX_ACTIVE_DB || els.activeEmployeeList.__ctx__db;
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
    <div style="padding: 12px; display:flex; flex-direction:column; gap:12px;">
      <div style="display:flex; align-items:center; gap:10px;">
        <div class="emp-avatar" style="background: ${emp ? emp.avatar_color : '#94a3b8'}; width:36px; height:36px; font-size:12px;">
          ${emp ? emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase() : '?'}
        </div>
        <div>
          <h4 style="margin:0; font-size:14px;">${escapeHtml(emp ? emp.name : t('employee', 'Mitarbeiter'))}</h4>
          <span style="font-size:11px; color:var(--shiftflow-muted);">${escapeHtml(emp ? emp.role : '')}</span>
        </div>
      </div>

      <hr style="border:0; height:1px; background:var(--shiftflow-line); margin:4px 0;" />

      <div>
        <span class="shiftflow-kicker">${t('dateTime', 'Datum & Zeit')}</span>
        <div style="font-weight:700; font-size:13px; margin-top:2px;">${start.toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', options)}</div>
        <div style="font-size:13px; color:var(--shiftflow-accent); font-weight:700; margin-top:2px;">
          ${formatTime(start)} - ${formatTime(end)} (${((shift.end_time - shift.start_time)/3600000).toFixed(1)} ${t('hoursShort', 'Std')})
        </div>
      </div>

      <div>
        <span class="shiftflow-kicker">${t('colProject', 'Projekt / Einsatzort')}</span>
        <div style="font-weight:700; font-size:12.5px; margin-top:2px;">${escapeHtml(projName)}</div>
      </div>

      <div>
        <span class="shiftflow-kicker">${t('department', 'Abteilung')}</span>
        <div style="font-weight:700; font-size:12.5px; margin-top:2px;">${t('dept' + (shift.department === 'Küche' ? 'Kitchen' : shift.department === 'Verwaltung' ? 'Admin' : shift.department), shift.department)}</div>
      </div>

      ${shift.notes ? `
        <div>
          <span class="shiftflow-kicker">${t('notes', 'Notizen')}</span>
          <div style="font-size:12px; font-style:italic; margin-top:2px; background:var(--shiftflow-surface-2); padding:6px; border-radius:6px;">"${escapeHtml(shift.notes)}"</div>
        </div>
      ` : ''}

      <div style="display:flex; gap:8px; margin-top:12px;">
        <button class="os-btn" id="btnEditShiftInspector" style="flex:1;">${t('edit', 'Bearbeiten')}</button>
      </div>
    </div>
  `;

  els.aiPlannerSection.classList.add('hidden');
  els.detailInspectorSection.classList.add('is-open');

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
    els.detailInspectorSection.classList.remove('is-open');
    els.aiPlannerSection.classList.remove('hidden');
    return;
  }

  const db = globalThis.CTOX_ACTIVE_DB || els.activeEmployeeList.__ctx__db;
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
    <div style="padding: 12px; display:flex; flex-direction:column; gap:14px;">
      <!-- Header Info -->
      <div style="display:flex; align-items:center; gap:10px;">
        <div class="emp-avatar" style="background: ${emp.avatar_color || '#6366f1'}; width:40px; height:40px; font-size:13px; font-weight:700; display:flex; align-items:center; justify-content:center; border-radius:50%; color:#fff;">
          ${emp.name.split(' ').map(n => n[0]).join('').slice(0, 2).toUpperCase()}
        </div>
        <div style="flex: 1;">
          <h4 style="margin:0; font-size:15px; font-weight:700; color:var(--shiftflow-text);">${escapeHtml(emp.name)}</h4>
          <span style="font-size:11.5px; color:var(--shiftflow-muted);">${escapeHtml(emp.role || t('employee', 'Mitarbeiter'))}</span>
        </div>
      </div>

      <hr style="border:0; height:1px; background:var(--shiftflow-line); margin:2px 0;" />

      <!-- Stammdaten & Stats -->
      <div style="display:grid; grid-template-columns:1fr 1fr; gap:10px; background:var(--shiftflow-surface-2); padding:10px; border-radius:8px;">
        <div>
          <span class="shiftflow-kicker">${t('billingWagesLabel', 'Lohnkosten')}</span>
          <div style="font-weight:700; font-size:13px; margin-top:2px; color:var(--shiftflow-text);">${emp.internal_hourly_rate ? emp.internal_hourly_rate.toFixed(2) : '25.00'} €/${t('hoursShort', 'Std')}</div>
        </div>
        <div>
          <span class="shiftflow-kicker">${t('weeklyTarget', 'Wochen-Soll')}</span>
          <div style="font-weight:700; font-size:13px; margin-top:2px; color:var(--shiftflow-text);">${emp.weekly_target_hours || '40'} ${t('hoursShort', 'Std')}</div>
        </div>
      </div>

      <!-- Weekly Planned Progress -->
      <div>
        <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:4px;">
          <span class="shiftflow-kicker">${t('plannedThisWeek', 'Eingeplant (diese Woche)')}</span>
          <span style="font-size:12px; font-weight:700; color:${totalHours > (emp.weekly_target_hours || 40) ? 'var(--shiftflow-danger)' : 'var(--shiftflow-accent)'};">
            ${totalHours.toFixed(1)} / ${emp.weekly_target_hours || '40'} ${t('hoursShort', 'Std')}
          </span>
        </div>
        <div style="width:100%; height:6px; background:var(--shiftflow-surface-3); border-radius:3px; overflow:hidden;">
          <div style="width:${Math.min(100, (totalHours / (emp.weekly_target_hours || 40)) * 100)}%; height:100%; background:${totalHours > (emp.weekly_target_hours || 40) ? 'var(--shiftflow-danger)' : 'var(--shiftflow-accent)'}; border-radius:3px;"></div>
        </div>
      </div>

      <div>
        <span class="shiftflow-kicker">${t('departments', 'Abteilungen')}</span>
        <div style="display:flex; flex-wrap:wrap; gap:4px; margin-top:4px;">
          ${(emp.departments || ['Service']).map(dept => {
            const dMap = { 'Service': t('deptService', 'Service'), 'Küche': t('deptKitchen', 'Küche'), 'Bar': t('deptBar', 'Bar'), 'Verwaltung': t('deptAdmin', 'Verwaltung') };
            return `<span style="background:var(--shiftflow-surface-3); font-size:11px; padding:3px 8px; border-radius:12px; font-weight:600; color:var(--shiftflow-text);">${escapeHtml(dMap[dept] || dept)}</span>`;
          }).join('')}
        </div>
      </div>

        <p style="font-size:11px; color:var(--shiftflow-muted); margin:0 0 8px 0; line-height:1.3;">${t('quickAssignInstructions', 'Klicke auf einen Tag, um den Mitarbeiter direkt für das Projekt einzuteilen (8:00 - 16:00 Uhr):')}</p>
        <div style="display:flex; flex-direction:column; gap:6px;">
          ${activeProjects.length === 0 ? `<div style="color:var(--shiftflow-muted); font-size:11px; font-style:italic;">${t('noActiveProjects', 'Keine aktiven Projekte vorhanden.')}</div>` : activeProjects.map(proj => {
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
                  style="padding:3px 5px; min-width:26px; font-size:10px; font-weight:700; border-radius:4px; border:1px solid var(--shiftflow-line); background:var(--shiftflow-surface-2); color:var(--shiftflow-text); cursor:pointer;"
                  title="${dayName}, ${targetDate.toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', {day:'2-digit', month:'2-digit'})}"
                >
                  ${dayName}
                </button>
              `;
            }).join('');

            return `
              <div style="display:flex; flex-direction:column; gap:4px; padding:6px; border-radius:6px; background:var(--shiftflow-surface-2); border:1px solid var(--shiftflow-line);">
                <div style="display:flex; align-items:center; gap:6px; font-size:11.5px; font-weight:700; color:var(--shiftflow-text);">
                  <span style="background:${proj.color || '#6366f1'}; width:8px; height:8px; border-radius:50%; display:inline-block;"></span>
                  <span style="white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:180px;">${escapeHtml(proj.name)}</span>
                </div>
                <div style="display:flex; gap:2.5px; margin-top:2px;">
                  ${weekdayButtons}
                </div>
              </div>
            `;
          }).join('')}
        </div>
      </div>

      <hr style="border:0; height:1px; background:var(--shiftflow-line); margin:2px 0;" />

      <!-- Inspector Profile Actions -->
      <div style="display:flex; gap:8px; margin-top:4px;">
        <button class="os-btn" id="btnEditEmployeeInspector" style="flex:1;">${t('editProfile', 'Profil bearbeiten')}</button>
        <button class="os-btn os-btn-danger" id="btnDeleteEmployeeInspector" style="flex:1;">${t('deleteEmployee', 'Mitarbeiter löschen')}</button>
      </div>
    </div>
  `;

  els.aiPlannerSection.classList.add('hidden');
  els.detailInspectorSection.classList.add('is-open');

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
    els.detailInspectorSection.classList.remove('is-open');
    els.aiPlannerSection.classList.remove('hidden');
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
        <span class="shiftflow-kicker">${kicker}</span>
        <h2>${title}</h2>
      </div>
      <button class="icon-button" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}">×</button>
    </header>
    <form class="shiftflow-drawer-form">
      <label>
        <span>${t('name', 'Name')}</span>
        <input type="text" name="name" class="os-input" value="${nameVal}" required placeholder="z.B. Max Mustermann" />
      </label>
      <label>
        <span>${t('email', 'E-Mail')}</span>
        <input type="email" name="email" class="os-input" value="${emailVal}" placeholder="z.B. max@ctox.dev" />
      </label>
      <div class="form-row">
        <label>
          <span>${t('wagesPerHr', 'Lohnkosten (€/h)')}</span>
          <input type="number" step="0.01" name="internal_hourly_rate" class="os-input" value="${rateVal}" required />
        </label>
        <label>
          <span>${t('weeklyTargetHrs', 'Wochen-Soll (Std)')}</span>
          <input type="number" name="weekly_target_hours" class="os-input" value="${hoursVal}" required />
        </label>
      </div>
      <label>
        <span>${t('rolePosition', 'Rolle / Position')}</span>
        <input type="text" name="role" class="os-input" value="${roleVal}" placeholder="${t('rolePlaceholder', 'z.B. Serviceleitung')}" required />
      </label>
      <label>
        <span>${t('departments', 'Abteilungen')}</span>
        <div class="shiftflow-checkbox-group">
          <label><input type="checkbox" name="departments" value="Service" ${depts.includes('Service') ? 'checked' : ''} /> ${t('deptService', 'Service')}</label>
          <label><input type="checkbox" name="departments" value="Küche" ${depts.includes('Küche') ? 'checked' : ''} /> ${t('deptKitchen', 'Küche')}</label>
          <label><input type="checkbox" name="departments" value="Bar" ${depts.includes('Bar') ? 'checked' : ''} /> ${t('deptBar', 'Bar')}</label>
          <label><input type="checkbox" name="departments" value="Verwaltung" ${depts.includes('Verwaltung') ? 'checked' : ''} /> ${t('deptAdmin', 'Verwaltung')}</label>
        </div>
      </label>
      <div class="shiftflow-drawer-actions ${isEdit ? 'has-danger' : ''}">
        <button type="button" class="os-btn" data-drawer-cancel>${t('cancel', 'Abbrechen')}</button>
        ${isEdit ? `<button type="button" class="os-btn os-btn-danger" data-drawer-delete>${t('delete', 'Löschen')}</button>` : ''}
        <button type="submit" class="os-btn os-btn-primary">${submitText}</button>
      </div>
    </form>
  `;

  body.querySelector('[data-drawer-close]').addEventListener('click', () => ctx.closeDrawers());
  body.querySelector('[data-drawer-cancel]').addEventListener('click', () => ctx.closeDrawers());

  if (isEdit) {
    body.querySelector('[data-drawer-delete]').addEventListener('click', async () => {
      const confirmDelete = confirm(t('confirmDeleteEmployee', 'Möchtest du den Mitarbeiter "{0}" wirklich löschen? Alle zugeordneten Schichten werden ebenfalls gelöscht.', emp.name));
      if (!confirmDelete) return;

      const doc = await ctx.db.planning_employees.findOne(emp.id).exec();
      if (doc) {
        await doc.remove();
      }

      const associatedShifts = await ctx.db.planning_shifts.find({ selector: { employee_id: emp.id } }).exec();
      for (const s of associatedShifts) {
        await s.remove();
      }

      ctx.closeDrawers();
      selectedEmployeeId = null;
      els.detailInspectorSection.classList.remove('is-open');
      els.aiPlannerSection.classList.remove('hidden');
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
      const doc = await ctx.db.planning_employees.findOne(emp.id).exec();
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
      await ctx.db.planning_employees.insert({
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
        <span class="shiftflow-kicker">${kicker}</span>
        <h2>${title}</h2>
      </div>
      <button class="icon-button" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}">×</button>
    </header>
    <form class="shiftflow-drawer-form">
      <label>
        <span>${t('projectName', 'Projektname')}</span>
        <input type="text" name="name" class="os-input" value="${nameVal}" required placeholder="${t('projectNamePlaceholder', 'z.B. Intersolar Standbau')}" />
      </label>
      <label>
        <span>${t('customer', 'Kunde')}</span>
        <input type="text" name="client" class="os-input" value="${clientVal}" required placeholder="${t('customerPlaceholder', 'z.B. Messe München GmbH')}" />
      </label>
      <label>
        <span>${t('location', 'Einsatzort')}</span>
        <input type="text" name="location" class="os-input" value="${locationVal}" placeholder="${t('locationPlaceholder', 'z.B. Halle A5, Stand 120')}" />
      </label>
      <label>
        <span>${t('hourlyRateExt', 'Stundensatz (externer Umsatz)')}</span>
        <input type="number" step="0.01" name="hourly_rate" class="os-input" value="${rateVal}" required />
      </label>
      <label>
        <span>${t('color', 'Farbe')}</span>
        <div class="shiftflow-color-picker">
          ${colorPickerHtml}
        </div>
      </label>
      <div class="shiftflow-drawer-actions ${isEdit ? 'has-danger' : ''}">
        <button type="button" class="os-btn" data-drawer-cancel>${t('cancel', 'Abbrechen')}</button>
        ${isEdit ? `<button type="button" class="os-btn os-btn-danger" data-drawer-delete>${t('delete', 'Löschen')}</button>` : ''}
        <button type="submit" class="os-btn os-btn-primary">${submitText}</button>
      </div>
    </form>
  `;

  body.querySelector('[data-drawer-close]').addEventListener('click', () => ctx.closeDrawers());
  body.querySelector('[data-drawer-cancel]').addEventListener('click', () => ctx.closeDrawers());

  if (isEdit) {
    body.querySelector('[data-drawer-delete]').addEventListener('click', async () => {
      const shiftCount = await ctx.db.planning_shifts.find({ selector: { project_id: proj.id } }).exec();
      if (shiftCount.length > 0) {
        const confirmDelete = confirm(t('confirmDeleteProjectWithShifts', 'Es sind noch {0} Schichten für das Projekt "{1}" geplant. Willst du das Projekt trotzdem löschen?', shiftCount.length, proj.name));
        if (!confirmDelete) return;
      } else {
        const confirmDelete = confirm(t('confirmDeleteProject', 'Möchtest du das Projekt "{0}" wirklich löschen?', proj.name));
        if (!confirmDelete) return;
      }

      const doc = await ctx.db.planning_projects.findOne(proj.id).exec();
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
      const doc = await ctx.db.planning_projects.findOne(proj.id).exec();
      if (doc) {
        await doc.incrementalPatch(patch);
      }
    } else {
      const id = 'proj_' + Date.now();
      await ctx.db.planning_projects.insert({
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

  const employees = await ctx.db.planning_employees.find().exec();
  const projects = await ctx.db.planning_projects.find().exec();

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
        <span class="shiftflow-kicker">${kicker}</span>
        <h2>${title}</h2>
      </div>
      <button class="icon-button" type="button" data-drawer-close aria-label="${t('close', 'Schließen')}">×</button>
    </header>
    <form class="shiftflow-drawer-form">
      <label>
        <span>${t('employee', 'Mitarbeiter')}</span>
        <select name="employee_id" class="os-select" required>
          ${empOptions}
        </select>
      </label>
      <label>
        <span>${t('colProject', 'Projekt / Einsatzort')}</span>
        <select name="project_id" class="os-select" required>
          ${projOptions}
        </select>
      </label>
      <div class="form-row">
        <label>
          <span>${t('date', 'Datum')}</span>
          <input type="date" name="date" class="os-input" value="${shiftDateStr}" required />
        </label>
        <label>
          <span>${t('department', 'Abteilung')}</span>
          <select name="department" class="os-select" required>
            <option value="Service" ${deptVal === 'Service' ? 'selected' : ''}>${t('deptService', 'Service')}</option>
            <option value="Küche" ${deptVal === 'Küche' ? 'selected' : ''}>${t('deptKitchen', 'Küche')}</option>
            <option value="Bar" ${deptVal === 'Bar' ? 'selected' : ''}>${t('deptBar', 'Bar')}</option>
            <option value="Verwaltung" ${deptVal === 'Verwaltung' ? 'selected' : ''}>${t('deptAdmin', 'Verwaltung')}</option>
          </select>
        </label>
      </div>
      <div class="form-row">
        <label>
          <span>${t('begin', 'Beginn')}</span>
          <input type="time" name="start_time" class="os-input" value="${startVal}" required />
        </label>
        <label>
          <span>${t('end', 'Ende')}</span>
          <input type="time" name="end_time" class="os-input" value="${endVal}" required />
        </label>
      </div>
      <label>
        <span>${t('specialNotes', 'Besondere Notizen')}</span>
        <textarea name="notes" class="os-input" placeholder="${t('notesPlaceholder', 'z.B. Schichtleitung übernehmen, Barista Service...')}" style="min-height: 60px;">${notesVal}</textarea>
      </label>
      <div class="shiftflow-drawer-actions ${isEdit ? 'has-danger' : ''}">
        <button type="button" class="os-btn" data-drawer-cancel>${t('cancel', 'Abbrechen')}</button>
        ${isEdit ? `<button type="button" class="os-btn os-btn-danger" data-drawer-delete>${t('delete', 'Löschen')}</button>` : ''}
        <button type="submit" class="os-btn os-btn-primary">${submitText}</button>
      </div>
    </form>
  `;

  body.querySelector('[data-drawer-close]').addEventListener('click', () => ctx.closeDrawers());
  body.querySelector('[data-drawer-cancel]').addEventListener('click', () => ctx.closeDrawers());

  if (isEdit) {
    body.querySelector('[data-drawer-delete]').addEventListener('click', async () => {
      const confirmDelete = confirm(t('confirmDeleteShift', 'Möchtest du diese Schicht wirklich löschen?'));
      if (!confirmDelete) return;

      const doc = await ctx.db.planning_shifts.findOne(shift.id).exec();
      if (doc) {
        await doc.remove();
      }
      ctx.closeDrawers();
      els.detailInspectorSection.classList.remove('is-open');
      els.aiPlannerSection.classList.remove('hidden');
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

    const proj = await ctx.db.planning_projects.findOne(projId).exec();
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
      const doc = await ctx.db.planning_shifts.findOne(shift.id).exec();
      if (doc) {
        await doc.incrementalPatch(patch);
      }
    } else {
      const id = 'shift_' + Date.now();
      await ctx.db.planning_shifts.insert({
        id,
        kind: 'shift',
        status: 'published',
        created_at_ms: Date.now(),
        ...patch
      });
    }
    ctx.closeDrawers();
    els.detailInspectorSection.classList.remove('is-open');
    els.aiPlannerSection.classList.remove('hidden');
  });

  ctx.openRightDrawer(body);
}

// -------------------------------------------------------------
// Bind Event Listeners
// -------------------------------------------------------------

function bindEventListeners(ctx, els) {
  els.activeEmployeeList.__ctx__db = ctx.db;
  globalThis.CTOX_ACTIVE_DB = ctx.db;

  // Prev / Next Week
  els.prevWeekBtn.addEventListener('click', () => {
    currentWeekStart.setDate(currentWeekStart.getDate() - 7);
    updateWeekRangeDisplay(els);
    renderGridHeader(els);
    triggerScheduleGridRefresh(ctx, els);
  });

  els.nextWeekBtn.addEventListener('click', () => {
    currentWeekStart.setDate(currentWeekStart.getDate() + 7);
    updateWeekRangeDisplay(els);
    renderGridHeader(els);
    triggerScheduleGridRefresh(ctx, els);
  });

  // Tab switching center pane
  els.viewSchedulerTabBtn.addEventListener('click', () => {
    currentView = 'scheduler';
    els.viewSchedulerTabBtn.classList.add('active');
    els.viewTimesheetsTabBtn.classList.remove('active');
    els.viewBillingTabBtn.classList.remove('active');

    els.schedulerView.classList.remove('hidden');
    els.timesheetsView.classList.add('hidden');
    els.billingView.classList.add('hidden');

    els.centerPaneTitle.textContent = t('schedulePlanning', 'Einsatzplanung');
  });

  els.viewTimesheetsTabBtn.addEventListener('click', () => {
    currentView = 'timesheets';
    els.viewSchedulerTabBtn.classList.remove('active');
    els.viewTimesheetsTabBtn.classList.add('active');
    els.viewBillingTabBtn.classList.remove('active');

    els.schedulerView.classList.add('hidden');
    els.timesheetsView.classList.remove('hidden');
    els.billingView.classList.add('hidden');

    els.centerPaneTitle.textContent = t('tabTimesheets', 'Zeiterfassung');
  });

  els.viewBillingTabBtn.addEventListener('click', () => {
    currentView = 'billing';
    els.viewSchedulerTabBtn.classList.remove('active');
    els.viewTimesheetsTabBtn.classList.remove('active');
    els.viewBillingTabBtn.classList.add('active');

    els.schedulerView.classList.add('hidden');
    els.timesheetsView.classList.add('hidden');
    els.billingView.classList.remove('hidden');

    els.centerPaneTitle.textContent = t('billingTitle', 'Leistungsabrechnung & Aggregation');
    triggerBillingAggregationUpdate(ctx, els);
  });

  // Dual Timeline Toggle Button handlers
  els.toggleViewEmployeesBtn.addEventListener('click', () => {
    currentTimelineFocus = 'employees';
    els.toggleViewEmployeesBtn.classList.add('active');
    els.toggleViewProjectsBtn.classList.remove('active');
    renderGridHeader(els);
    triggerScheduleGridRefresh(ctx, els);
  });

  els.toggleViewProjectsBtn.addEventListener('click', () => {
    currentTimelineFocus = 'projects';
    els.toggleViewEmployeesBtn.classList.remove('active');
    els.toggleViewProjectsBtn.classList.add('active');
    renderGridHeader(els);
    triggerScheduleGridRefresh(ctx, els);
  });

  // Projects Drawer Trigger
  els.addProjectBtn.addEventListener('click', () => {
    openProjectDrawer(null, els, ctx);
  });

  // Employee Add Drawer Trigger
  els.addEmployeeBtn.addEventListener('click', () => {
    openEmployeeDrawer(null, els, ctx);
  });

  // Department Filters & Employee Search
  els.departmentFilterSelect.addEventListener('change', (e) => {
    currentDeptFilter = e.target.value;
    triggerSidebarsUpdate(ctx, els);
    triggerScheduleGridRefresh(ctx, els);
  });

  els.employeeSearchInput.addEventListener('input', () => {
    triggerSidebarsUpdate(ctx, els);
  });

  // Timesheets Workbench Actions
  els.approveAllTimesheetsBtn.addEventListener('click', () => approveAllTimesheets(els));

  // Billing Date bounds selectors
  els.billingFilterApplyBtn.addEventListener('click', () => {
    triggerBillingAggregationUpdate(ctx, els);
  });

  // Invoice Export Payload drafting
  els.exportInvoiceDraftBtn.addEventListener('click', () => {
    exportInvoiceDraftPayload();
  });

  // Detail Inspector Drawer Closes
  els.closeInspectorBtn.addEventListener('click', () => {
    els.detailInspectorSection.classList.remove('is-open');
    els.aiPlannerSection.classList.remove('hidden');
  });

  // AI chat triggers
  els.btnAutoGenerateSchedule.addEventListener('click', () => {
    alert(t('aiGenerateAlert', 'Planungs-Assistent: Generiere optimierten Dienstplan für KW {0} basierend auf Mitarbeiter-Sollstunden & Projektbedarf...', getWeekNumber(currentWeekStart)));
    autoGenerateSchedule(ctx, els);
  });

  els.btnCheckConflicts.addEventListener('click', () => {
    runConflictsAnalysis(ctx, els);
  });

  els.btnFindReplacements.addEventListener('click', () => {
    alert(t('aiReplacementAlert', 'Planungs-Assistent: Suche qualifizierten Ersatz für gemeldete Abwesenheiten...'));
  });
}

function triggerScheduleGridRefresh(ctx, els) {
  ctx.db.planning_shifts.find().exec().then(shifts => {
    ctx.db.planning_employees.find().exec().then(employees => {
      ctx.db.planning_projects.find().exec().then(projects => {
        renderSchedulerGrid(employees, projects, shifts, els, ctx);
      });
    });
  });
}

function triggerSidebarsUpdate(ctx, els) {
  ctx.db.planning_employees.find().exec().then(employees => {
    ctx.db.planning_time_records.find().exec().then(records => {
      renderEmployeesList(employees, records, els, ctx);
    });
  });
}

function triggerBillingAggregationUpdate(ctx, els) {
  ctx.db.planning_employees.find().exec().then(employees => {
    ctx.db.planning_projects.find().exec().then(projects => {
      ctx.db.planning_time_records.find().exec().then(records => {
        renderBillingAggregation(employees, projects, records, els);
      });
    });
  });
}

// -------------------------------------------------------------
// Advanced Planning Automations (Mock intelligence)
// -------------------------------------------------------------

async function autoGenerateSchedule(ctx, els) {
  const db = ctx.db;
  if (!db) return;

  const employees = await db.planning_employees.find({ selector: { status: 'active' } }).exec();
  const projects = await db.planning_projects.find({ selector: { status: 'active' } }).exec();

  if (employees.length === 0 || projects.length === 0) {
    alert(t('aiSetupRequirement', 'Es müssen mindestens ein aktiver Mitarbeiter und ein aktives Projekt existieren!'));
    return;
  }

  // Clear existing draft shifts for the current week first
  const monday = new Date(currentWeekStart);
  const dayStart = monday.getTime();
  const dayEnd = dayStart + 7 * 24 * 3600000 - 1;

  const weekShifts = await db.planning_shifts.find({
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
    employees.forEach((emp, empIdx) => {
      // Rotate projects among employees
      const proj = projects[ (empIdx + day) % projects.length ];

      const startTime = getTimestamp(day, '08:00');
      const endTime = getTimestamp(day, '16:00');

      db.planning_shifts.insert({
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
    });
  }

  // Display a nice chat success message
  const msgHtml = `
    <div class="shiftflow-ai-msg bot">
      <div class="shiftflow-ai-avatar">AI</div>
      <div class="shiftflow-ai-text">${t('aiGenerateSuccess', 'Ich habe erfolgreich einen optimierten Dienstplanentwurf (Mo-Fr) für alle aktiven Mitarbeiter generiert! Die Schichten wurden im Modus <strong>Entwurf</strong> angelegt, sodass du sie vor dem Veröffentlichen noch anpassen kannst.')}</div>
    </div>
  `;
  els.aiPlannerChatBody.insertAdjacentHTML('beforeend', msgHtml);
  els.aiPlannerChatBody.scrollTop = els.aiPlannerChatBody.scrollHeight;
}

async function runConflictsAnalysis(ctx, els) {
  const db = ctx.db;
  if (!db) return;

  const shifts = await db.planning_shifts.find().exec();
  const employees = await db.planning_employees.find().exec();

  const monday = new Date(currentWeekStart);
  const weekStartMs = monday.getTime();
  const weekEndMs = weekStartMs + 7 * 24 * 3600000 - 1;

  // Filter shifts this week
  const weekShifts = shifts.filter(s => s.start_time >= weekStartMs && s.start_time <= weekEndMs);

  const conflicts = [];

  // Rule 1: Check maximum working hours (e.g. max 45 hours a week)
  employees.forEach(emp => {
    const empShifts = weekShifts.filter(s => s.employee_id === emp.id);
    let totalHours = 0;
    empShifts.forEach(s => {
      totalHours += (s.end_time - s.start_time) / 3600000;
    });

    if (totalHours > 42) {
      conflicts.push({
        type: 'overtime',
        message: t('conflictMaxHours', '<strong>{0}</strong> überschreitet die wöchentliche Höchstarbeitszeit ({1} Std. geplant, Soll: {2} Std.)', emp.name, totalHours.toFixed(1), emp.weekly_target_hours || 40)
      });
    }
  });

  // Rule 2: Double-booking check
  for (let i = 0; i < weekShifts.length; i++) {
    for (let j = i + 1; j < weekShifts.length; j++) {
      const s1 = weekShifts[i];
      const s2 = weekShifts[j];

      if (s1.employee_id === s2.employee_id) {
        // Check overlap
        if (s1.start_time < s2.end_time && s2.start_time < s1.end_time) {
          const emp = employees.find(e => e.id === s1.employee_id);
          conflicts.push({
            type: 'overlap',
            message: t('conflictDoubleBooking', 'Doppelbuchung für <strong>{0}</strong> am {1} erkannt.', emp ? emp.name : t('employee', 'Mitarbeiter'), new Date(s1.start_time).toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE'))
          });
        }
      }
    }
  }

  // Render Conflicts list
  if (conflicts.length === 0) {
    els.conflictsList.innerHTML = `
      <div class="shiftflow-conflict-empty-state">
        ${t('noConflictsDetected', 'Keine aktiven Konflikte erkannt. Der Dienstplan erfüllt alle Vorgaben.')}
      </div>
    `;

    const botMsg = `
      <div class="shiftflow-ai-msg bot">
        <div class="shiftflow-ai-avatar">AI</div>
        <div class="shiftflow-ai-text">${t('aiCheckSuccess', 'Ich habe die Konfliktanalyse durchgeführt: Keine Regelverletzungen (Ruhezeiten, Höchstarbeitszeiten, Doppelbelegungen) gefunden! Perfekte Dienstplanung.')}</div>
      </div>
    `;
    els.aiPlannerChatBody.insertAdjacentHTML('beforeend', botMsg);
  } else {
    els.conflictsList.innerHTML = conflicts.map(c => {
      const icon = c.type === 'overtime' ? '🕒' : '⚠️';
      return `
        <div class="shiftflow-conflict-item" style="display:flex; gap:8px; padding:8px 12px; background:color-mix(in srgb, #ef4444 8%, transparent); border:1px solid color-mix(in srgb, #ef4444 20%, transparent); border-radius:8px; margin-bottom:6px; font-size:12px;">
          <span>${icon}</span>
          <div>${c.message}</div>
        </div>
      `;
    }).join('');

    const botMsg = `
      <div class="shiftflow-ai-msg bot">
        <div class="shiftflow-ai-avatar">AI</div>
        <div class="shiftflow-ai-text">${t('aiCheckWarning', 'Vorsicht! Ich habe {0} Dienstplankonflikte bzw. Arbeitszeitüberschreitungen gefunden. Bitte prüfe das Konflikte-Panel unten rechts.', conflicts.length)}</div>
      </div>
    `;
    els.aiPlannerChatBody.insertAdjacentHTML('beforeend', botMsg);
  }
  els.aiPlannerChatBody.scrollTop = els.aiPlannerChatBody.scrollHeight;
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

  alert(t('invoiceDraftDownloadSuccess', 'Rechnungsentwurf erfolgreich erstellt und heruntergeladen! 📝\n\nDu kannst diese Datei direkt im CTOX Rechnungs-Modul einlesen.'));
}

// -------------------------------------------------------------
// Column Resizing Logic
// -------------------------------------------------------------

function setupShiftflowColumnResizing(app) {
  if (!app) return;
  const leftResizer = app.querySelector('[data-shiftflow-col-resizer="left"]');
  const rightResizer = app.querySelector('[data-shiftflow-col-resizer="right"]');

  if (!leftResizer || !rightResizer) return;

  const leftWidth = localStorage.getItem('ctox.shiftflow.layout.leftWidth') || localStorage.getItem('shiftflow_left_w') || '300';
  const rightWidth = localStorage.getItem('ctox.shiftflow.layout.rightWidth') || localStorage.getItem('shiftflow_right_w') || '360';

  app.style.setProperty('--shiftflow-left-width', `${leftWidth}px`);
  app.style.setProperty('--shiftflow-right-width', `${rightWidth}px`);

  const cleanups = [];

  const resizerL = new CtoxResizer({
    resizerEl: leftResizer,
    containerEl: app,
    cssVar: '--shiftflow-left-width',
    side: 'left',
    minWidth: 220,
    maxWidth: 480,
    onResize: (width) => {
      localStorage.setItem('ctox.shiftflow.layout.leftWidth', width);
      localStorage.setItem('shiftflow_left_w', width);
    }
  });
  cleanups.push(() => resizerL.destroy());

  const resizerR = new CtoxResizer({
    resizerEl: rightResizer,
    containerEl: app,
    cssVar: '--shiftflow-right-width',
    side: 'right',
    minWidth: 280,
    maxWidth: 520,
    onResize: (width) => {
      localStorage.setItem('ctox.shiftflow.layout.rightWidth', width);
      localStorage.setItem('shiftflow_right_w', width);
    }
  });
  cleanups.push(() => resizerR.destroy());

  return () => {
    cleanups.forEach(fn => fn());
  };
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

function initShiftflowContextMenu(els, ctx) {
  contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu shiftflow-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  contextMenu = menu;

  const handleContextMenu = (event) => {
    if (ctx.module?.id !== 'shiftflow') return;
    const context = shiftflowCommandContextFromElement(els, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderShiftflowContextMenu(els, ctx, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (contextMenu?.contains(event.target)) return;
    hideShiftflowContextMenu();
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideShiftflowContextMenu();
  };

  ctx.host.addEventListener('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    ctx.host.removeEventListener('contextmenu', handleContextMenu);
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideShiftflowContextMenu();
    contextMenu?.remove();
    contextMenu = null;
  };
}

function hideShiftflowContextMenu() {
  if (contextMenu) contextMenu.hidden = true;
}

function canModifyShiftflowApp(ctx) {
  if (typeof ctx.canModifyModule === 'function' && ctx.canModifyModule()) return true;
  const user = ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function shiftflowCommandContextFromElement(els, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;

  const shiftCard = element?.closest('.shift-card');
  const gridCell = element?.closest('.grid-shift-cell');
  const employeeCard = element?.closest('.employee-card');
  const projectCard = element?.closest('.project-card');

  let employee_id = '';
  let employee_name = '';
  let project_id = '';
  let project_name = '';
  let shift_id = '';
  let shift_title = '';
  let date = '';

  if (shiftCard) {
    shift_id = shiftCard.dataset.shiftId || '';
    const titleEl = shiftCard.querySelector('div[style*="font-weight:700"]');
    if (titleEl) shift_title = titleEl.textContent;
  }
  if (gridCell) {
    employee_id = gridCell.dataset.empId || '';
    project_id = gridCell.dataset.projId || '';
    date = gridCell.dataset.date || '';
  }
  if (employeeCard) {
    employee_id = employeeCard.dataset.empId || '';
    const nameEl = employeeCard.querySelector('.emp-name');
    if (nameEl) employee_name = nameEl.textContent;
  }
  if (projectCard) {
    project_id = projectCard.dataset.projId || '';
    const nameEl = projectCard.querySelector('.project-card-name');
    if (nameEl) project_name = nameEl.textContent;
  }

  const searchQuery = els.employeeSearchInput?.value || '';

  return {
    module: 'shiftflow',
    column: currentView || 'scheduler',
    record_type: shift_id ? 'shift' : (employee_id ? 'employee' : (project_id ? 'project' : 'schedule')),
    record_id: shift_id || employee_id || project_id || '',
    label: shift_title || employee_name || project_name || date || t('shiftflow', 'Einsatzplanung'),
    employee_id,
    employee_name,
    project_id,
    project_name,
    shift_id,
    shift_title,
    date,
    current_view: currentView,
    timeline_focus: currentTimelineFocus,
    dept_filter: currentDeptFilter,
    search_query: searchQuery,
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderShiftflowContextMenu(els, ctx, context, x, y) {
  ensureCtoxContextMenuStyles();
  const canModifyApp = canModifyShiftflowApp(ctx);
  contextMenu.innerHTML = `
    <form class="shiftflow-context-chat" data-shiftflow-context-chat-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeHtml(context.label || 'Einsatzplanung')}</span>
        </div>
        <button type="button" data-shiftflow-context-close aria-label="Schließen">×</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
          <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
        </div>
      ` : ''}
      <textarea data-shiftflow-context-message placeholder="Was soll CTOX im Dienstplan / der Einsatzplanung tun?"></textarea>
      <footer>
        <span data-shiftflow-context-status></span>
        <button type="submit">Senden</button>
      </footer>
    </form>
  `;
  contextMenu.hidden = false;
  contextMenu.style.left = '0px';
  contextMenu.style.top = '0px';
  const rect = contextMenu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  contextMenu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  contextMenu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = contextMenu.querySelector('[data-shiftflow-context-chat-form]');
  const textarea = contextMenu.querySelector('[data-shiftflow-context-message]');
  contextMenu.querySelector('[data-shiftflow-context-close]')?.addEventListener('click', hideShiftflowContextMenu);
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
    await dispatchShiftflowContextChat(els, ctx, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchShiftflowContextChat(els, ctx, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = contextMenu?.querySelector('[data-shiftflow-context-status]');
  if (!trimmed) {
    if (status) status.textContent = 'Nachricht fehlt.';
    return;
  }

  const safeMode = mode === 'app' && canModifyShiftflowApp(ctx) ? 'app' : 'data';
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = 'Chat ist noch nicht bereit.';
    return;
  }
  if (status) status.textContent = 'Oeffne Chat...';
  const title = `${safeMode === 'app' ? 'Shiftflow App modifizieren' : 'Dienstplan anpassen'} · ${context.label || 'Einsatzplanung'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die Einsatzplanung-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Dienstplandaten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'shiftflow',
      source_title: 'Einsatzplanung',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'shiftflow' : (context.record_id || 'shiftflow'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        context,
        thread_key: 'business-os/shiftflow',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        shift_id: context.shift_id || '',
        employee_id: context.employee_id || '',
        project_id: context.project_id || '',
      },
    },
  }));
  hideShiftflowContextMenu();
}

function ensureCtoxContextMenuStyles() {
  if (document.getElementById('ctox-unified-context-menu-style')) return;
  const style = document.createElement('style');
  style.id = 'ctox-unified-context-menu-style';
  style.textContent = `
    .ctox-context-menu {
      position: absolute;
      z-index: 2400;
      width: min(560px, calc(100vw - 24px));
      max-width: calc(100% - 16px);
      overflow: hidden;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-panel, 12px);
      background: color-mix(in srgb, var(--bo-surface, var(--surface, #fff)) 75%, transparent);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      box-shadow: 0 18px 50px rgba(0, 0, 0, 0.25);
      padding: 6px;
      font-family: system-ui, -apple-system, sans-serif;
      animation: ctox-menu-fade-in 0.15s ease-out;
    }
    @keyframes ctox-menu-fade-in {
      from { opacity: 0; transform: scale(0.97); }
      to { opacity: 1; transform: scale(1); }
    }
    .ctox-context-menu form {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 10px;
      min-width: 0;
      padding: 12px;
      margin: 0;
    }
    .ctox-context-menu form header,
    .ctox-context-menu form footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      min-width: 0;
    }
    .ctox-context-menu .ctox-context-mode {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 6px;
      min-width: 0;
    }
    .ctox-context-menu .ctox-context-mode label {
      display: flex;
      align-items: center;
      gap: 7px;
      min-width: 0;
      min-height: 30px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      color: var(--bo-muted, var(--muted, #64747c));
      font-size: 11.5px;
      font-weight: 760;
      padding: 0 8px;
      cursor: pointer;
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      margin: 0;
    }
    .ctox-context-menu .ctox-context-mode label:hover {
      border-color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu .ctox-context-mode input {
      margin: 0;
      accent-color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu form header div {
      min-width: 0;
    }
    .ctox-context-menu form strong,
    .ctox-context-menu form span {
      display: block;
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-context-menu form strong {
      color: var(--bo-text, var(--text, #18222d));
      font-size: 12.5px;
      font-weight: 820;
    }
    .ctox-context-menu form span {
      color: var(--bo-muted, var(--muted, #64747c));
      font-size: 11px;
      font-weight: 700;
    }
    .ctox-context-menu form footer > span {
      display: flex;
      align-items: center;
      gap: 6px;
      flex-wrap: wrap;
      white-space: normal;
      font-size: 11px;
      color: var(--bo-muted, var(--muted, #64747c));
    }
    .ctox-context-menu form textarea {
      width: 100%;
      box-sizing: border-box;
      min-height: 92px;
      max-height: 180px;
      min-width: 0;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      color: var(--bo-text, var(--text, #18222d));
      font: 12.5px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif;
      padding: 9px;
      resize: vertical;
    }
    .ctox-context-menu form textarea:focus {
      outline: none;
      border-color: var(--bo-accent, #23665f);
      box-shadow: 0 0 0 2px color-mix(in srgb, var(--bo-accent, #23665f) 25%, transparent);
    }
    .ctox-context-menu form button {
      flex: 0 0 auto;
      min-height: 30px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      color: var(--bo-text, var(--text, #18222d));
      font: inherit;
      font-size: 12px;
      font-weight: 760;
      cursor: pointer;
      padding: 0 10px;
    }
    .ctox-context-menu form button:hover {
      background: color-mix(in srgb, var(--bo-text, #18222d) 8%, var(--bo-surface-muted, #eef3f7));
    }
    .ctox-context-menu form button[type="submit"] {
      border-color: var(--bo-accent, #23665f);
      background: color-mix(in srgb, var(--bo-accent, #23665f) 14%, var(--bo-surface, #fff));
      color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu form button[type="submit"]:hover {
      background: color-mix(in srgb, var(--bo-accent, #23665f) 22%, var(--bo-surface, #fff));
    }
    .ctox-context-menu form button[type="button"][aria-label="Schließen"],
    .ctox-context-menu form [data-creator-context-close],
    .ctox-context-menu form [data-reports-context-close],
    .ctox-context-menu form [data-shiftflow-context-close],
    .ctox-context-menu form [data-app-store-context-close],
    .ctox-context-menu form [data-context-close] {
      width: 30px;
      min-width: 30px;
      padding: 0;
      text-align: center;
      font-size: 18px;
      border: none;
      background: none;
      color: var(--bo-muted, var(--muted, #64747c));
      cursor: pointer;
    }
  `;
  document.head.append(style);
}
