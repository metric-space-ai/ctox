import { loadModuleMessages } from '../../shared/i18n.js';
import { createCalendarView } from './calendar-view-adapter.js';

const RENDER_DEBOUNCE_MS = 50;
const CALENDAR_COLLECTIONS = [
  'calendar_sources',
  'calendar_calendars',
  'calendar_events',
  'calendar_event_instances',
  'calendar_availability_rules',
  'calendar_booking_pages',
  'calendar_booking_holds',
  'calendar_bookings',
];

// EventCalendar view keys mapped from the Monat/Woche/Tag band.
const CALENDAR_VIEW_MAP = {
  month: 'dayGridMonth',
  week: 'timeGridWeek',
  day: 'timeGridDay',
};

const labels = {
  de: {
    calendar: 'Kalender',
    today: 'Heute',
    day: 'Tag',
    week: 'Woche',
    month: 'Monat',
    list: 'Liste',
    myCalendars: 'Meine Kalender',
    calendars: 'Kalender',
    bookingPages: 'Buchungsseiten',
    bookings: 'Buchungen',
    bookingHolds: 'Temporäre Holds',
    confirmedBookings: 'Bestätigte Buchungen',
    bookingContextTitle: 'Kontext',
    newEvent: 'Neuer Termin',
    newRecord: 'Neu',
    editEvent: 'Termin bearbeiten',
    import: 'Importieren',
    export: 'Exportieren',
    searchPlaceholder: 'Suchen...',
    allStatus: 'Alle',
    active: 'Aktiv / Sichtbar',
    inactive: 'Inaktiv / Ausgeblendet',
    dataReady: 'Daten geladen',
    noDatabase: 'Keine lokale Datenbank verbunden',
    importInvalid: 'Ungültige JSON-Datei.',
    importEmpty: 'Keine Kalenderdaten in der Datei.',
    importUnavailable: 'Import ist gerade nicht möglich (keine Datenbank).',
    imported: '{count} Datensätze importiert',
    save: 'Speichern',
    delete: 'Löschen',
    cancel: 'Abbrechen',
  },
  en: {
    calendar: 'Calendar',
    today: 'Today',
    day: 'Day',
    week: 'Week',
    month: 'Month',
    list: 'List',
    myCalendars: 'My Calendars',
    calendars: 'Calendars',
    bookingPages: 'Booking Pages',
    bookings: 'Bookings',
    bookingHolds: 'Temporary Holds',
    confirmedBookings: 'Confirmed Bookings',
    bookingContextTitle: 'Context',
    newEvent: 'New Event',
    newRecord: 'New',
    editEvent: 'Edit Event',
    import: 'Import',
    export: 'Export',
    searchPlaceholder: 'Search...',
    allStatus: 'All',
    active: 'Active / Visible',
    inactive: 'Inactive / Hidden',
    dataReady: 'Data loaded',
    noDatabase: 'No local database connected',
    importInvalid: 'Invalid JSON file.',
    importEmpty: 'No calendar data in the file.',
    importUnavailable: 'Import is unavailable right now (no database).',
    imported: '{count} records imported',
    save: 'Save',
    delete: 'Delete',
    cancel: 'Cancel',
  },
};

const state = {
  ctx: null,
  lang: 'de',
  t: (key, fallback) => fallback ?? key,

  // Data lists
  calendars: [],
  events: [],
  bookingPages: [],
  holds: [],
  bookings: [],

  // Left grammar column state (SHELL-wired via data-pg-*, mirrored here).
  leftBand: 'calendars', // 'calendars' | 'pages'
  search: '',
  listView: 'cards', // 'cards' | 'list'
  statusFilter: 'all', // 'all' | 'active' | 'inactive'

  // Active EventCalendar view (month/week/day band).
  calendarView: 'month',
  activeView: 'dayGridMonth',
  selectedCalendarIds: new Set(),

  // Third pane (bookings/holds context) — auto-reveal on selection.
  selectedBookingPageId: null,
  userCollapsedContext: false,

  // Active editing item in Drawer
  editingType: null, // 'event' | 'bookingPage' | 'calendar'
  editingItem: null,

  // Subscriptions & Cleanups
  rxSubscriptions: [],
  activeFormSubscription: null,
  calendarViewInstance: null,
  renderTimer: null,
  domHandlers: null,
};

const els = {};

function calendarCollection(name) {
  const facade = state.ctx?.db;
  return facade?.collection?.(name) || null;
}

function calendarDb() {
  const entries = CALENDAR_COLLECTIONS.map((name) => [name, calendarCollection(name)]);
  if (entries.some(([, collection]) => !collection)) return null;
  return Object.fromEntries(entries);
}

function canWriteCalendarDefaults() {
  const permissionCheck = state.ctx?.permissions?.canWriteCollection;
  return typeof permissionCheck !== 'function'
    || [
      'calendar_sources',
      'calendar_calendars',
      'calendar_events',
      'calendar_booking_pages',
      'calendar_availability_rules',
    ].every((collectionName) => permissionCheck(collectionName));
}

function isBusinessOsPermissionDenied(error) {
  return error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
    || error?.name === 'BusinessOsPermissionError';
}

function notify({ type = 'info', message }) {
  const notifications = state.ctx?.notifications;
  const title = state.t('calendar', labels[state.lang].calendar);
  if (typeof notifications?.show === 'function') {
    notifications.show({ type, title, message });
  } else if (typeof notifications?.notify === 'function') {
    notifications.notify({ title, body: message });
  }
}

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';

  // Load locales
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  // Ensure EventCalendar & RRule assets are loaded
  await ensureAssetsLoaded();

  // Load markup
  ctx.host.innerHTML = await loadModuleMarkup();

  // Clear default left/right content slots
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();

  applyStaticLabels(ctx.host, state.t);
  bindElements(ctx.host);
  wireEvents();

  // Column resizing is declarative: the shell (app.js setupModuleResizers)
  // wires the `.ctox-column-resizer[data-resizer-var]` handles from index.html.
  // The left column chrome (search / shard-list toggle / filter tray / reset /
  // active-dot / view band) is SHELL-wired from the data-pg-* markup by
  // app.js autoWirePaneGrammar; this module owns NO chrome wiring and only
  // listens for the bubbling `ctox-pane-grammar-change` event to re-render.

  // Initialize EventCalendar View Instance
  initCalendarView();

  // Render the workbench before demand reads and optional first-run seed
  // writes. A calendar window must be usable even while sync is catching up.
  renderAll();
  let disposed = false;
  Promise.resolve()
    .then(async () => {
      await seedDefaultDataIfNeeded();
      if (disposed || state.ctx !== ctx) return;
      await loadDataFromDb();
      if (disposed || state.ctx !== ctx) return;
      wireRealtimeSync();
    })
    .catch((error) => {
      if (disposed || state.ctx !== ctx) return;
      console.warn('[calendar] background initialization failed', error);
      renderAll();
    });

  // Presence (advisory UX): publish which event this user has open in the
  // edit drawer, and surface a hint when someone else edits the same event.
  state.presenceRemote = [];
  state.presenceCleanup = null;
  if (ctx.presence?.subscribe) {
    state.presenceCleanup = ctx.presence.subscribe((entries) => {
      state.presenceRemote = Array.isArray(entries) ? entries : [];
      updateEventFormPresenceHint();
    });
  }

  return () => {
    disposed = true;
    state.presenceCleanup?.();
    state.presenceCleanup = null;
    try { state.ctx?.presence?.clear?.(); } catch {}
    if (state.renderTimer) clearTimeout(state.renderTimer);
    state.rxSubscriptions.forEach(sub => sub.unsubscribe());
    state.rxSubscriptions = [];
    if (state.activeFormSubscription) {
      state.activeFormSubscription.unsubscribe();
      state.activeFormSubscription = null;
    }
    if (state.calendarViewInstance) {
      state.calendarViewInstance.destroy();
      state.calendarViewInstance = null;
    }
    unbindEvents();
  };
}

async function ensureAssetsLoaded() {
  // 1. Module stylesheet
  await loadStylesheetOnce({
    selector: 'link[data-module-styles="calendar"]',
    href: new URL('./index.css', import.meta.url).href,
    dataset: { moduleStyles: 'calendar' }
  });

  // 2. EventCalendar CSS
  await loadStylesheetOnce({
    selector: 'link[data-vendor-style="event-calendar"]',
    href: new URL('../../vendor/event-calendar/event-calendar.min.css', import.meta.url).href,
    dataset: { vendorStyle: 'event-calendar' }
  });

  // 3. EventCalendar JS (classic script — must NOT be type=module, otherwise it does not expose `window.EventCalendar`).
  await loadClassicScriptOnce({
    selector: 'script[data-vendor-script="event-calendar"]',
    href: new URL('../../vendor/event-calendar/event-calendar.min.js', import.meta.url).href,
    globalCheck: () => typeof window.EventCalendar !== 'undefined',
    dataset: { vendorScript: 'event-calendar' },
    label: 'EventCalendar'
  });

  // 4. RRule JS
  await loadClassicScriptOnce({
    selector: 'script[data-vendor-script="rrule"]',
    href: new URL('../../vendor/rrule/rrule.min.js', import.meta.url).href,
    globalCheck: () => typeof window.RRule !== 'undefined' || !!(window.rrule && window.rrule.RRule),
    dataset: { vendorScript: 'rrule' },
    label: 'RRule'
  });
}

function loadStylesheetOnce({ selector, href, dataset }) {
  if (document.querySelector(selector)) return Promise.resolve();
  return new Promise((resolve) => {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = href;
    Object.entries(dataset || {}).forEach(([k, v]) => { link.dataset[k] = v; });
    const done = () => resolve();
    link.addEventListener('load', done, { once: true });
    link.addEventListener('error', done, { once: true });
    document.head.appendChild(link);
  });
}

function loadClassicScriptOnce({ selector, href, globalCheck, dataset, label }) {
  // Already loaded (either globally available or tag already injected and resolved)
  if (typeof globalCheck === 'function' && globalCheck()) return Promise.resolve();
  const existing = document.querySelector(selector);
  if (existing && existing.dataset.loaded === '1') return Promise.resolve();

  return new Promise((resolve) => {
    const script = existing || document.createElement('script');
    // Explicitly classic — type=module would NOT expose the IIFE's `var EventCalendar` on window.
    script.type = 'text/javascript';
    script.async = false;
    if (!existing) {
      script.src = href;
      Object.entries(dataset || {}).forEach(([k, v]) => { script.dataset[k] = v; });
    }
    const finalize = () => {
      script.dataset.loaded = '1';
      if (typeof globalCheck === 'function' && !globalCheck()) {
        // Poll briefly in case the global is assigned asynchronously after onload.
        let attempts = 0;
        const poll = () => {
          if (globalCheck()) return resolve();
          if (attempts++ > 40) {
            console.error(`[calendar] ${label} script loaded from ${href} but global is not defined`);
            return resolve();
          }
          setTimeout(poll, 25);
        };
        poll();
        return;
      }
      resolve();
    };
    script.addEventListener('load', finalize, { once: true });
    script.addEventListener('error', () => {
      console.error(`[calendar] Failed to load ${label} from ${href}`);
      resolve();
    }, { once: true });
    if (!existing) document.head.appendChild(script);
  });
}

function applyStaticLabels(root, t) {
  root.querySelectorAll('[data-t]').forEach(el => el.textContent = t(el.dataset.t));
  root.querySelectorAll('[data-t-title]').forEach(el => el.title = t(el.dataset.tTitle));
  root.querySelectorAll('[data-t-aria]').forEach(el => el.setAttribute('aria-label', t(el.dataset.tAria)));
  root.querySelectorAll('[data-t-placeholder]').forEach(el => el.placeholder = t(el.dataset.tPlaceholder));
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(host) {
  els.root = host.querySelector('[data-calendar-root]');

  // Left grammar column (the band, filter tray and search are SHELL-wired).
  els.leftList = host.querySelector('[data-calendar-left-list]');
  els.leftEmpty = host.querySelector('[data-calendar-left-empty]');

  // Center
  els.calendarDataStatus = host.querySelector('#calendarDataStatus');
  els.eventCalendarMount = host.querySelector('#eventCalendarView');
  els.viewBand = host.querySelector('[data-calendar-view-band]');

  // Third pane (bookings/holds context)
  els.contextEmpty = host.querySelector('[data-calendar-context-empty]');
  els.contextDetail = host.querySelector('[data-calendar-context-detail]');
  els.contextTitle = host.querySelector('[data-calendar-context-title]');
  els.bookingContext = host.querySelector('#bookingContext');
  els.bookingHoldsList = host.querySelector('#bookingHoldsList');
  els.bookingsList = host.querySelector('#bookingsList');

  // Buttons
  els.btnNewEvent = host.querySelector('#btnNewEvent');
  els.btnLeftNew = host.querySelector('#btnLeftNew');
  els.btnPrev = host.querySelector('#prevPeriodBtn');
  els.btnNext = host.querySelector('#nextPeriodBtn');
  els.btnToday = host.querySelector('#todayPeriodBtn');
  els.btnImport = host.querySelector('[data-action="import"]');
  els.btnExport = host.querySelector('[data-action="export"]');
  els.toggleContext = host.querySelector('[data-calendar-toggle-context]');

  // Drawer / Inspector
  els.drawer = host.querySelector('#calendarInspectorDrawer');
  els.drawerKicker = host.querySelector('#drawerKicker');
  els.drawerTitle = host.querySelector('#drawerTitle');
  els.drawerContent = host.querySelector('#drawerContent');
  els.closeDrawerBtn = host.querySelector('#closeDrawerBtn');
}

function leftPane() {
  return els.root?.querySelector('.calendar-left') || null;
}

function wireEvents() {
  state.domHandlers = {
    newEvent: () => openEventForm(),
    leftNew: () => (state.leftBand === 'pages' ? openBookingPageForm() : openCalendarForm()),
    prev: () => state.calendarViewInstance?.prev(),
    next: () => state.calendarViewInstance?.next(),
    today: () => state.calendarViewInstance?.today(),
    importClick: () => importCalendarData(),
    exportClick: () => exportCalendarData(),
    toggleContext: () => {
      state.userCollapsedContext = !state.userCollapsedContext;
      applyContextReveal();
    },
    grammarChange: (event) => onLeftGrammarChange(event),
    viewBandClick: (event) => {
      const tab = event.target?.closest?.('[data-calendar-view]');
      if (!tab || !els.viewBand?.contains(tab)) return;
      setCalendarView(tab.dataset.calendarView);
    },
    leftListClick: (event) => onLeftListClick(event),
    leftListChange: (event) => onLeftListChange(event),
    renderedEventClick: (event) => {
      const eventEl = event.target?.closest?.('.ec-event');
      if (!eventEl || !els.eventCalendarMount?.contains(eventEl)) return;
      const dbEvent = findEventForRenderedCalendarElement(eventEl);
      if (!dbEvent) return;
      event.preventDefault();
      event.stopPropagation();
      openEventForm(dbEvent.id);
    },
    closeDrawer,
    keydown: (event) => {
      if (event.key === 'Escape' && els.drawer?.classList.contains('is-open')) {
        event.preventDefault();
        closeDrawer();
      }
    }
  };

  els.btnNewEvent?.addEventListener('click', state.domHandlers.newEvent);
  els.btnLeftNew?.addEventListener('click', state.domHandlers.leftNew);
  els.btnPrev?.addEventListener('click', state.domHandlers.prev);
  els.btnNext?.addEventListener('click', state.domHandlers.next);
  els.btnToday?.addEventListener('click', state.domHandlers.today);
  els.btnImport?.addEventListener('click', state.domHandlers.importClick);
  els.btnExport?.addEventListener('click', state.domHandlers.exportClick);
  els.toggleContext?.addEventListener('click', state.domHandlers.toggleContext);
  els.viewBand?.addEventListener('click', state.domHandlers.viewBandClick);
  // Selection / row actions are delegated so the handlers survive every
  // data-driven list rebuild (selection itself stays an in-place class flip).
  els.leftList?.addEventListener('click', state.domHandlers.leftListClick);
  els.leftList?.addEventListener('change', state.domHandlers.leftListChange);
  els.eventCalendarMount?.addEventListener('click', state.domHandlers.renderedEventClick, true);
  els.closeDrawerBtn?.addEventListener('click', state.domHandlers.closeDrawer);
  // The canonical column grammar reports search/toggle/tray/band changes via a
  // bubbling CustomEvent from the shell-wired pane.
  els.root?.addEventListener('ctox-pane-grammar-change', state.domHandlers.grammarChange);
  document.addEventListener('keydown', state.domHandlers.keydown);
}

function unbindEvents() {
  const handlers = state.domHandlers;
  if (!handlers) return;
  els.btnNewEvent?.removeEventListener('click', handlers.newEvent);
  els.btnLeftNew?.removeEventListener('click', handlers.leftNew);
  els.btnPrev?.removeEventListener('click', handlers.prev);
  els.btnNext?.removeEventListener('click', handlers.next);
  els.btnToday?.removeEventListener('click', handlers.today);
  els.btnImport?.removeEventListener('click', handlers.importClick);
  els.btnExport?.removeEventListener('click', handlers.exportClick);
  els.toggleContext?.removeEventListener('click', handlers.toggleContext);
  els.viewBand?.removeEventListener('click', handlers.viewBandClick);
  els.leftList?.removeEventListener('click', handlers.leftListClick);
  els.leftList?.removeEventListener('change', handlers.leftListChange);
  els.eventCalendarMount?.removeEventListener('click', handlers.renderedEventClick, true);
  els.closeDrawerBtn?.removeEventListener('click', handlers.closeDrawer);
  els.root?.removeEventListener('ctox-pane-grammar-change', handlers.grammarChange);
  document.removeEventListener('keydown', handlers.keydown);
  state.domHandlers = null;
}

// The left column band/search/filter/view are SHELL-wired. Reading them back
// from the grammar-change event keeps the module free of chrome wiring; a
// band/search/filter change is an intentional list reset (rebuild expected),
// while selecting a ROW stays an in-place class flip (markActiveBookingPage).
function onLeftGrammarChange(event) {
  const pane = leftPane();
  const detail = event?.detail || pane?.__ctoxPaneGrammar?.state?.() || {};
  const filters = detail.filters || {};
  state.search = String(detail.search || '').trim().toLowerCase();
  state.listView = detail.view === 'list' ? 'list' : 'cards';
  state.leftBand = detail.band === 'pages' ? 'pages' : 'calendars';
  state.statusFilter = filters.status || 'all';
  renderLeftList();
}

function setCalendarView(view) {
  const next = CALENDAR_VIEW_MAP[view] ? view : 'month';
  state.calendarView = next;
  state.activeView = CALENDAR_VIEW_MAP[next];
  state.calendarViewInstance?.setView(state.activeView);
  for (const tab of els.viewBand?.querySelectorAll('[data-calendar-view]') || []) {
    const active = tab.dataset.calendarView === next;
    tab.classList.toggle('is-active', active);
    tab.setAttribute('aria-selected', active ? 'true' : 'false');
  }
}

// ----------------------------------------------------
// SEED & DATABASE LOAD METHODS
// ----------------------------------------------------

async function seedDefaultDataIfNeeded() {
  const db = calendarDb();
  if (!db) return;

  const calendarsCount = await db.calendar_calendars.count().exec();
  if (calendarsCount > 0) return; // already seeded
  if (!canWriteCalendarDefaults()) return;

  // 1. Seed Local Source
  const sourceId = 'source_local_' + generateUUID();
  await db.calendar_sources.insert({
    id: sourceId,
    kind: 'local',
    title: 'Lokale Kalender',
    color: '#3b82f6',
    sync_status: 'synced',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  // 2. Seed default calendars
  const calPersonalId = 'cal_personal_' + generateUUID();
  await db.calendar_calendars.insert({
    id: calPersonalId,
    source_id: sourceId,
    title: 'Persönlich',
    color: '#3b82f6',
    visibility: true,
    owner_user_id: 'default_user',
    timezone: 'Europe/Berlin',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  const calWorkId = 'cal_work_' + generateUUID();
  await db.calendar_calendars.insert({
    id: calWorkId,
    source_id: sourceId,
    title: 'Arbeit',
    color: '#a855f7',
    visibility: true,
    owner_user_id: 'default_user',
    timezone: 'Europe/Berlin',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  // 3. Seed some initial events for the current week
  const today = new Date();
  const startOfTodayMs = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();

  // Event 1: Daily Standup (work, recurring daily at 09:30)
  await db.calendar_events.insert({
    id: 'evt_standup_' + generateUUID(),
    calendar_id: calWorkId,
    title: 'Tägliches Standup',
    description: 'Sync mit dem CTOX Team',
    location: 'Meetingraum A / Jami Link',
    start_time: startOfTodayMs + 9.5 * 60 * 60 * 1000, // 09:30
    end_time: startOfTodayMs + 10 * 60 * 60 * 1000,    // 10:00
    timezone: 'Europe/Berlin',
    all_day: false,
    recurrence_rule: 'FREQ=DAILY;INTERVAL=1;COUNT=30',
    status: 'confirmed',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  // Event 2: Lunch with Michael (personal, tomorrow at 12:30)
  await db.calendar_events.insert({
    id: 'evt_lunch_' + generateUUID(),
    calendar_id: calPersonalId,
    title: 'Mittagessen mit Michael',
    description: 'Neues Projekt besprechen',
    location: 'Trattoria Bella',
    start_time: startOfTodayMs + 24 * 60 * 60 * 1000 + 12.5 * 60 * 60 * 1000, // 12:30 tomorrow
    end_time: startOfTodayMs + 24 * 60 * 60 * 1000 + 13.5 * 60 * 60 * 1000,  // 13:30 tomorrow
    timezone: 'Europe/Berlin',
    all_day: false,
    status: 'confirmed',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  // 4. Seed a premium default Booking Page
  const bookingPageId = 'bp_beratung_' + generateUUID();
  await db.calendar_booking_pages.insert({
    id: bookingPageId,
    slug: 'beratungsgespraech',
    title: '30 Min. Erstgespräch',
    description: 'Kennenlernen und Anforderungsklärung für Ihr CTOX Custom Module.',
    duration_minutes: 30,
    buffer_before_minutes: 5,
    buffer_after_minutes: 10,
    min_notice_minutes: 120, // 2 hours
    max_days_ahead: 30,
    calendar_ids: [calWorkId],
    host_user_ids: ['default_user'],
    location_mode: 'link',
    status: 'active',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now()
  });

  // 5. Seed Availability Rules for the Booking Page (Mon-Fri, 9:00 - 17:00)
  for (let weekday = 1; weekday <= 5; weekday++) {
    await db.calendar_availability_rules.insert({
      id: `rule_bp_${bookingPageId}_day_${weekday}`,
      booking_page_id: bookingPageId,
      weekday: weekday,
      start_minute: 540,  // 09:00
      end_minute: 1020,  // 17:00
      timezone: 'Europe/Berlin',
      status: 'active'
    });
  }
}

async function loadDataFromDb() {
  const db = calendarDb();
  if (!db) return;

  try {
    const [cals, evts, bps, hlds, bks] = await Promise.all([
      db.calendar_calendars.find().exec(),
      db.calendar_events.find().exec(),
      db.calendar_booking_pages.find().exec(),
      db.calendar_booking_holds.find().exec(),
      db.calendar_bookings.find().exec()
    ]);

    state.calendars = cals.map(d => d.toJSON());
    state.events = evts.map(d => d.toJSON());
    state.bookingPages = bps.map(d => d.toJSON());
    state.holds = hlds.map(d => d.toJSON());
    state.bookings = bks.map(d => d.toJSON());

    if (
      state.selectedBookingPageId &&
      !state.bookingPages.some(page => page.id === state.selectedBookingPageId)
    ) {
      state.selectedBookingPageId = null;
    }

    // Set default selected calendars if none selected yet
    if (state.selectedCalendarIds.size === 0) {
      state.calendars.forEach(c => {
        if (c.visibility !== false) {
          state.selectedCalendarIds.add(c.id);
        }
      });
    }

    scheduleRender();
  } catch (error) {
    if (isBusinessOsPermissionDenied(error)) throw error;
    console.error('Failed to load calendar data', error);
  }
}

function wireRealtimeSync() {
  const db = calendarDb();
  if (!db) return;

  const tables = [
    db.calendar_calendars,
    db.calendar_events,
    db.calendar_booking_pages,
    db.calendar_booking_holds,
    db.calendar_bookings
  ].filter(Boolean);

  tables.forEach(col => {
    const sub = col.$.subscribe(() => {
      loadDataFromDb().catch(e => console.warn(e));
    });
    state.rxSubscriptions.push(sub);
  });
}

function scheduleRender() {
  if (state.renderTimer) return;
  state.renderTimer = setTimeout(() => {
    state.renderTimer = null;
    renderAll();
  }, RENDER_DEBOUNCE_MS);
}

function renderAll() {
  renderLeftList();
  renderMainBandCounts();
  renderContextPane();
  applyContextReveal();
  renderDataStatus();

  // Refresh Calendar adapter events
  const filteredEvents = state.events.filter(e => state.selectedCalendarIds.has(e.calendar_id));
  state.calendarViewInstance?.setEvents(filteredEvents, state.calendars);
}

function renderDataStatus() {
  if (!els.calendarDataStatus) return;
  const badge = els.calendarDataStatus;
  // The badge is reserved for real status changes (no DB / errors). Decorative
  // count summaries are kept out of the chrome so the calendar speaks for itself.
  if (!calendarDb()) {
    badge.hidden = false;
    badge.textContent = state.t('noDatabase', labels[state.lang].noDatabase);
    badge.classList.add('is-danger');
    badge.classList.remove('is-info');
    return;
  }
  badge.hidden = true;
  badge.textContent = '';
  badge.classList.remove('is-danger', 'is-info');
}

// ----------------------------------------------------
// LEFT GRAMMAR COLUMN RENDERING
// ----------------------------------------------------

function writeLeftCounts(counts) {
  const pg = leftPane()?.__ctoxPaneGrammar;
  if (pg?.setCounts) { pg.setCounts(counts); return; }
  for (const [key, value] of Object.entries(counts || {})) {
    const node = els.root?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function writeLeftFooter(text) {
  const pg = leftPane()?.__ctoxPaneGrammar;
  if (pg?.setFooter) { pg.setFooter(text); return; }
  const node = els.root?.querySelector('[data-calendar-left-footer]');
  if (node) node.textContent = text || '';
}

function calendarMatchesFilters(cal) {
  if (state.search && !String(cal.title || '').toLowerCase().includes(state.search)) return false;
  const visible = cal.visibility !== false;
  if (state.statusFilter === 'active' && !visible) return false;
  if (state.statusFilter === 'inactive' && visible) return false;
  return true;
}

function bookingPageMatchesFilters(bp) {
  if (state.search) {
    const haystack = `${bp.title || ''} ${bp.slug || ''}`.toLowerCase();
    if (!haystack.includes(state.search)) return false;
  }
  const active = bp.status === 'active';
  if (state.statusFilter === 'active' && !active) return false;
  if (state.statusFilter === 'inactive' && active) return false;
  return true;
}

function renderLeftList() {
  if (!els.leftList) return;

  const calendars = state.calendars.filter(calendarMatchesFilters);
  const pages = state.bookingPages.filter(bookingPageMatchesFilters);

  // Band counts reflect search + status (but not the band selection itself);
  // zeros are rendered, never hidden.
  writeLeftCounts({ calendars: calendars.length, pages: pages.length });

  const isPages = state.leftBand === 'pages';
  const rows = isPages ? pages : calendars;
  const scopeLabel = isPages ? state.t('bookingPages', 'Buchungsseiten') : state.t('calendars', 'Kalender');
  writeLeftFooter(`${rows.length} ${state.t('entries', 'Einträge')} · ${scopeLabel}`);

  els.leftList.classList.toggle('is-list-view', state.listView === 'list');

  if (!rows.length) {
    els.leftList.innerHTML = '';
    if (els.leftEmpty) els.leftEmpty.hidden = false;
    return;
  }
  if (els.leftEmpty) els.leftEmpty.hidden = true;

  els.leftList.innerHTML = isPages
    ? rows.map(bookingPageRowHtml).join('')
    : rows.map(calendarRowHtml).join('');

  if (isPages) markActiveBookingPage();
}

function calendarRowHtml(cal) {
  const checked = state.selectedCalendarIds.has(cal.id);
  const checkboxId = `calendar-toggle-${safeDomId(cal.id)}`;
  return `
    <div class="ctox-list-item calendar-item" data-calendar-id="${escapeHtml(cal.id)}" data-context-record-id="${escapeHtml(cal.id)}" data-context-record-type="calendar_calendar" data-context-label="${escapeHtml(cal.title || cal.id)}">
      <div class="calendar-item-left">
        <input id="${checkboxId}" type="checkbox" class="calendar-item-checkbox" data-action="toggle-cal" data-id="${escapeHtml(cal.id)}" aria-label="${escapeHtml(cal.title || 'Kalender')} anzeigen" ${checked ? 'checked' : ''} />
        <span class="calendar-item-color-indicator" style="background-color: ${safeColor(cal.color)}"></span>
        <span class="calendar-item-title" id="${checkboxId}-label">${escapeHtml(cal.title)}</span>
      </div>
      <div class="calendar-item-actions">
        <button type="button" class="ctox-icon-button ctox-icon-button--sm" data-action="edit-cal" data-id="${escapeHtml(cal.id)}" title="Bearbeiten" aria-label="${escapeHtml(cal.title || 'Kalender')} bearbeiten">${actionIcon('edit')}</button>
      </div>
    </div>
  `;
}

function bookingPageRowHtml(bp) {
  const safeSlug = normalizeSlug(bp.slug) || String(bp.slug || '').replace(/[^a-zA-Z0-9-_]/g, '');
  const publicUrl = `${window.location.origin}/book/${encodeURIComponent(safeSlug)}`;
  const isActive = bp.status === 'active';
  const isSelected = bp.id === state.selectedBookingPageId;
  return `
    <div class="ctox-list-item booking-page-item ${isSelected ? 'is-selected' : ''}" data-action="select-bp" data-id="${escapeHtml(bp.id)}" data-context-record-id="${escapeHtml(bp.id)}" data-context-record-type="calendar_booking_page" data-context-label="${escapeHtml(bp.title || bp.id)}" role="button" tabindex="0" aria-pressed="${isSelected ? 'true' : 'false'}">
      <div class="booking-page-item-left">
        <div class="booking-page-item-title">
          <span>${escapeHtml(bp.title)}</span>
          <small>${Number(bp.duration_minutes) || 0} Min · /book/${escapeHtml(safeSlug)} · ${isActive ? 'Aktiv' : 'Inaktiv'}</small>
        </div>
      </div>
      <div class="booking-page-item-actions">
        <a class="ctox-icon-button ctox-icon-button--sm" href="${publicUrl}" target="_blank" rel="noreferrer" title="Öffnen" aria-label="${escapeHtml(bp.title || 'Buchungsseite')} öffnen">${actionIcon('open')}</a>
        <button type="button" class="ctox-icon-button ctox-icon-button--sm" data-action="edit-bp" data-id="${escapeHtml(bp.id)}" title="Bearbeiten" aria-label="${escapeHtml(bp.title || 'Buchungsseite')} bearbeiten">${actionIcon('edit')}</button>
      </div>
    </div>
  `;
}

// Selecting a booking page is an in-place class flip across existing rows —
// never a list rebuild (a rebuild would clamp the well's scrollTop to 0 and
// yank the operator to the top mid-click).
function markActiveBookingPage() {
  for (const node of els.leftList?.querySelectorAll('.booking-page-item') || []) {
    const active = node.dataset.id === state.selectedBookingPageId;
    node.classList.toggle('is-selected', active);
    node.setAttribute('aria-pressed', active ? 'true' : 'false');
  }
}

function selectBookingPage(id) {
  if (state.selectedBookingPageId === id) {
    markActiveBookingPage();
    return;
  }
  state.selectedBookingPageId = id || null;
  state.userCollapsedContext = false;
  markActiveBookingPage();
  applyContextReveal();
  renderContextPane();
}

function onLeftListClick(event) {
  const target = event.target instanceof Element ? event.target : null;
  if (!target) return;

  const editCal = target.closest('[data-action="edit-cal"]');
  if (editCal) {
    event.stopPropagation();
    openCalendarForm(editCal.dataset.id);
    return;
  }
  const editBp = target.closest('[data-action="edit-bp"]');
  if (editBp) {
    event.stopPropagation();
    openBookingPageForm(editBp.dataset.id);
    return;
  }
  if (target.closest('a')) return; // let the open-link anchor work

  const row = target.closest('.booking-page-item[data-action="select-bp"]');
  if (row) {
    selectBookingPage(row.dataset.id || null);
  }
}

function onLeftListChange(event) {
  const toggle = event.target instanceof Element ? event.target.closest('[data-action="toggle-cal"]') : null;
  if (!toggle) return;
  const id = toggle.dataset.id;
  if (toggle.checked) state.selectedCalendarIds.add(id);
  else state.selectedCalendarIds.delete(id);
  // In-place: the checkbox already reflects the new state — refresh the grid
  // and the main-band counts without rebuilding the left list.
  const filteredEvents = state.events.filter(e => state.selectedCalendarIds.has(e.calendar_id));
  state.calendarViewInstance?.setEvents(filteredEvents, state.calendars);
  renderMainBandCounts();
}

// ----------------------------------------------------
// THIRD PANE (bookings / holds context, auto-reveal)
// ----------------------------------------------------

function applyContextReveal() {
  const hasSelection = Boolean(state.selectedBookingPageId);
  const visible = calendarContextVisible(hasSelection, state.userCollapsedContext);
  els.root?.classList.toggle('is-context-hidden', !visible);
  if (els.toggleContext) {
    // Only show the reveal control when there is something to reveal.
    els.toggleContext.hidden = !hasSelection;
    els.toggleContext.setAttribute('aria-pressed', String(visible));
    const label = visible ? 'Buchungen ausblenden' : 'Buchungen einblenden';
    els.toggleContext.setAttribute('aria-label', label);
    els.toggleContext.setAttribute('title', label);
  }
}

function renderContextPane() {
  const selectedPage = state.bookingPages.find(page => page.id === state.selectedBookingPageId) || null;

  if (!selectedPage) {
    if (els.contextDetail) els.contextDetail.hidden = true;
    if (els.contextEmpty) els.contextEmpty.hidden = false;
    return;
  }
  if (els.contextEmpty) els.contextEmpty.hidden = true;
  if (els.contextDetail) els.contextDetail.hidden = false;
  if (els.contextTitle) els.contextTitle.textContent = selectedPage.title || state.t('bookingContextTitle', 'Kontext');

  const safeSlug = normalizeSlug(selectedPage.slug) || String(selectedPage.slug || '').replace(/[^a-zA-Z0-9-_]/g, '');
  if (els.bookingContext) {
    els.bookingContext.innerHTML = `
      <div class="ctox-callout calendar-context-card">
        <span class="ctox-field-label">Ausgewählte Buchungsseite</span>
        <strong>${escapeHtml(selectedPage.title)}</strong>
        <span>${Number(selectedPage.duration_minutes) || 0} Min · /book/${escapeHtml(safeSlug)}</span>
      </div>
    `;
  }

  // 1. Holds List
  if (els.bookingHoldsList) {
    const activeHolds = state.holds.filter(h => (
      h.status === 'active' && h.expires_at_ms > Date.now() && h.booking_page_id === selectedPage.id
    ));
    if (activeHolds.length === 0) {
      els.bookingHoldsList.innerHTML = `<div class="ctox-empty">Keine aktiven Holds für diese Buchungsseite.</div>`;
    } else {
      els.bookingHoldsList.innerHTML = activeHolds.map(hold => {
        const startStr = new Date(hold.slot_start_ms).toLocaleString();
        const expiresStr = new Date(hold.expires_at_ms).toLocaleTimeString();
        return `
          <div class="ctox-list-item calendar-audit-item">
            <div class="calendar-audit-head">
              <strong>${escapeHtml(selectedPage.title || 'Buchung hold')}</strong>
              <span class="ctox-badge is-warning">Hold</span>
            </div>
            <small>Zeit: ${startStr}</small>
            <small class="is-expiry">Läuft ab um ${expiresStr}</small>
          </div>
        `;
      }).join('');
    }
  }

  // 2. Bookings List
  if (els.bookingsList) {
    const sortedBookings = state.bookings
      .filter(booking => booking.booking_page_id === selectedPage.id)
      .sort((a, b) => b.slot_start_ms - a.slot_start_ms);
    if (sortedBookings.length === 0) {
      els.bookingsList.innerHTML = `<div class="ctox-empty">Keine bestätigten Buchungen für diese Buchungsseite.</div>`;
    } else {
      els.bookingsList.innerHTML = sortedBookings.map(bk => {
        const startStr = new Date(bk.slot_start_ms).toLocaleString();
        const statusBadge = bk.status === 'confirmed' ? 'is-success' : 'is-danger';
        return `
          <div class="ctox-list-item calendar-audit-item" data-action="view-booking" data-id="${bk.id}">
            <div class="calendar-audit-head">
              <strong>${escapeHtml(bk.attendee_name)}</strong>
              <span class="ctox-badge ${statusBadge}">${bk.status === 'confirmed' ? 'Bestätigt' : 'Storniert'}</span>
            </div>
            <small>Event: ${escapeHtml(selectedPage.title || 'Beratung')}</small>
            <small>Zeit: ${startStr}</small>
            <small>E-Mail: ${escapeHtml(bk.attendee_email)}</small>
          </div>
        `;
      }).join('');

      els.bookingsList.querySelectorAll('[data-action="view-booking"]').forEach(el => {
        el.addEventListener('click', () => {
          openBookingDetail(el.dataset.id);
        });
      });
    }
  }
}

// ----------------------------------------------------
// MAIN VIEW BAND COUNTS (Monat / Woche / Tag)
// ----------------------------------------------------

function renderMainBandCounts() {
  const counts = computeViewBandCounts(state.events, state.selectedCalendarIds, new Date());
  const write = (view, value) => {
    const node = els.root?.querySelector(`[data-count-view-${view}]`);
    if (node) node.textContent = ` (${value})`;
  };
  write('month', counts.month);
  write('week', counts.week);
  write('day', counts.day);
}

// Counts events (from visible calendars) whose start falls in the month/week/
// day around the reference date. Zeros are rendered, never hidden. Pure so the
// band-count behaviour is unit-testable without the vendor calendar engine.
function computeViewBandCounts(events, selectedCalendarIds, refDate) {
  const ref = refDate instanceof Date ? refDate : new Date(refDate || Date.now());
  const ranges = { month: monthRange(ref), week: weekRange(ref), day: dayRange(ref) };
  const counts = { month: 0, week: 0, day: 0 };
  const selected = selectedCalendarIds instanceof Set ? selectedCalendarIds : null;
  for (const event of Array.isArray(events) ? events : []) {
    if (selected && selected.size > 0 && !selected.has(event.calendar_id)) continue;
    const start = Number(event.start_time);
    if (!Number.isFinite(start)) continue;
    for (const key of ['month', 'week', 'day']) {
      const [from, to] = ranges[key];
      if (start >= from && start < to) counts[key] += 1;
    }
  }
  return counts;
}

function dayRange(ref) {
  const start = new Date(ref.getFullYear(), ref.getMonth(), ref.getDate()).getTime();
  return [start, start + 24 * 60 * 60 * 1000];
}

function weekRange(ref) {
  const day = new Date(ref.getFullYear(), ref.getMonth(), ref.getDate());
  const weekday = (day.getDay() + 6) % 7; // Monday = 0
  const start = day.getTime() - weekday * 24 * 60 * 60 * 1000;
  return [start, start + 7 * 24 * 60 * 60 * 1000];
}

function monthRange(ref) {
  const start = new Date(ref.getFullYear(), ref.getMonth(), 1).getTime();
  const end = new Date(ref.getFullYear(), ref.getMonth() + 1, 1).getTime();
  return [start, end];
}

// ----------------------------------------------------
// IMPORT / EXPORT (JSON via Blob / file-input, honest and small)
// ----------------------------------------------------

function exportCalendarData() {
  const payload = buildCalendarExport({
    calendars: state.calendars,
    bookingPages: state.bookingPages,
    events: state.events,
  }, Date.now());
  downloadJson(payload, 'calendar.json');
}

function buildCalendarExport(sources, nowMs) {
  const src = sources && typeof sources === 'object' ? sources : {};
  return {
    kind: 'ctox-calendar-export',
    exported_at_ms: Number(nowMs) || 0,
    calendars: (Array.isArray(src.calendars) ? src.calendars : []).map((cal) => ({ ...cal })),
    booking_pages: (Array.isArray(src.bookingPages) ? src.bookingPages : []).map((bp) => ({ ...bp })),
    events: (Array.isArray(src.events) ? src.events : []).map((ev) => ({ ...ev })),
  };
}

// Accepts an exported snapshot ({ calendars, booking_pages, events }); keeps
// only records that carry an id so a re-import is a clean round-trip.
function parseCalendarImport(raw) {
  const src = raw && typeof raw === 'object' && !Array.isArray(raw) ? raw : {};
  const keepById = (list) => (Array.isArray(list) ? list : [])
    .filter((item) => item && typeof item === 'object' && item.id)
    .map((item) => ({ ...item }));
  return {
    calendars: keepById(src.calendars),
    bookingPages: keepById(src.booking_pages),
    events: keepById(src.events),
  };
}

function downloadJson(payload, filename) {
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.rel = 'noopener';
    (els.root || document.body)?.appendChild?.(a);
    a.click();
    a.remove?.();
  } catch (error) {
    console.error('[calendar] export failed:', error);
  } finally {
    if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

function importCalendarData() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    let parsed;
    try {
      parsed = JSON.parse(await file.text());
    } catch {
      notify({ type: 'error', message: state.t('importInvalid', 'Ungültige JSON-Datei.') });
      return;
    }
    const { calendars, bookingPages, events } = parseCalendarImport(parsed);
    if (!calendars.length && !bookingPages.length && !events.length) {
      notify({ type: 'warning', message: state.t('importEmpty', 'Keine Kalenderdaten in der Datei.') });
      return;
    }
    const db = calendarDb();
    if (!db) {
      notify({ type: 'error', message: state.t('importUnavailable', 'Import ist gerade nicht möglich (keine Datenbank).') });
      return;
    }
    let count = 0;
    try {
      for (const cal of calendars) { await db.calendar_calendars.upsert({ ...cal, updated_at_ms: Date.now() }); count++; }
      for (const bp of bookingPages) { await db.calendar_booking_pages.upsert({ ...bp, updated_at_ms: Date.now() }); count++; }
      for (const ev of events) { await db.calendar_events.upsert({ ...ev, updated_at_ms: Date.now() }); count++; }
    } catch (error) {
      console.error('[calendar] import failed:', error);
      notify({ type: 'error', message: state.t('importUnavailable', 'Import ist gerade nicht möglich (keine Datenbank).') });
      return;
    }
    await loadDataFromDb();
    notify({ type: 'info', message: state.t('imported', '{count} Datensätze importiert').replace('{count}', String(count)) });
  });
  input.click();
}

// ----------------------------------------------------
// EVENT CALENDAR UI SETUP
// ----------------------------------------------------

function initCalendarView() {
  if (!els.eventCalendarMount) return;

  state.calendarViewInstance = createCalendarView({
    root: els.eventCalendarMount,
    events: state.events,
    calendars: state.calendars,
    view: CALENDAR_VIEW_MAP[state.calendarView] || 'dayGridMonth',
    onEventClick: ({ event, original }) => {
      openEventForm(original.id);
    },
    onEventMove: async ({ id, start, end, allDay }) => {
      const db = calendarDb();
      if (!db) return;
      const doc = await db.calendar_events.findOne(id).exec();
      if (doc) {
        await doc.patch({
          start_time: start.getTime(),
          end_time: end.getTime(),
          all_day: !!allDay,
          updated_at_ms: Date.now()
        });
      }
    },
    onEventResize: async ({ id, start, end }) => {
      const db = calendarDb();
      if (!db) return;
      const doc = await db.calendar_events.findOne(id).exec();
      if (doc) {
        await doc.patch({
          start_time: start.getTime(),
          end_time: end.getTime(),
          updated_at_ms: Date.now()
        });
      }
    },
    onRangeSelect: ({ start, end, allDay }) => {
      openEventForm(null, {
        start_time: start.getTime(),
        end_time: end.getTime(),
        all_day: !!allDay
      });
    }
  });
  const viewLabels = {
    'ec-dayGridMonth': state.t('viewMonth', 'Monat'),
    'ec-timeGridWeek': state.t('viewWeek', 'Woche'),
    'ec-timeGridDay': state.t('viewDay', 'Tag'),
    'ec-listWeek': state.t('viewList', 'Liste'),
  };
  for (const [className, label] of Object.entries(viewLabels)) {
    const button = els.eventCalendarMount.querySelector(`.${className}`);
    if (button) button.setAttribute('aria-label', label);
  }
}

// ----------------------------------------------------
// DRAWER FORMS IMPLEMENTATIONS
// ----------------------------------------------------

function openDrawer(kicker, title, htmlContent) {
  if (!els.drawer) return;
  if (state.activeFormSubscription) {
    state.activeFormSubscription.unsubscribe();
    state.activeFormSubscription = null;
  }
  els.drawerKicker.textContent = kicker;
  els.drawerTitle.textContent = title;
  els.drawerContent.innerHTML = htmlContent;
  els.drawer.classList.add('is-open');
  els.drawer.setAttribute('aria-hidden', 'false');
  requestAnimationFrame(() => {
    const firstField = els.drawer.querySelector('input:not([type="hidden"]), select, textarea, button');
    (firstField || els.drawer).focus?.({ preventScroll: true });
  });
}

function closeDrawer() {
  els.drawer?.classList.remove('is-open');
  els.drawer?.setAttribute('aria-hidden', 'true');
  state.editingType = null;
  state.editingItem = null;
  try { state.ctx?.presence?.set([]); } catch {}
  if (state.activeFormSubscription) {
    state.activeFormSubscription.unsubscribe();
    state.activeFormSubscription = null;
  }
}

// Presence hint inside the event drawer: visible while someone ELSE has the
// same persisted event open in their edit drawer. New (unsaved) events have
// no record id and publish nothing.
function updateEventFormPresenceHint() {
  const drawerOpenForEvent = state.editingType === 'event' && state.editingItem?.id;
  let hint = els.drawer?.querySelector('[data-calendar-presence-hint]') || null;
  if (!drawerOpenForEvent) {
    hint?.remove();
    return;
  }
  const ownActorId = state.ctx?.actor?.id || '';
  const peers = (state.presenceRemote || []).filter((entry) => entry
    && entry.collection === 'calendar_events'
    && entry.recordId === state.editingItem.id
    && entry.actorId
    && entry.actorId !== ownActorId);
  if (!peers.length) {
    hint?.remove();
    return;
  }
  if (!hint) {
    hint = document.createElement('div');
    hint.className = 'ctox-badge is-warning calendar-presence-hint';
    hint.setAttribute('data-calendar-presence-hint', '');
    els.drawerTitle?.insertAdjacentElement('afterend', hint);
  }
  const names = [...new Set(peers.map((entry) => entry.actorName || entry.actorId))].join(', ');
  hint.textContent = `✎ ${names} ${state.t('presenceEditing', 'bearbeitet gerade')}`;
}

// 1. EVENT FORM

function openEventForm(eventId = null, defaults = null) {
  state.editingType = 'event';
  const dbEvent = eventId ? state.events.find(e => e.id === eventId) : null;
  state.editingItem = dbEvent;
  // Publish presence for a persisted event being edited (new events have no
  // record id yet and publish nothing).
  try {
    state.ctx?.presence?.set(dbEvent
      ? [{ collection: 'calendar_events', recordId: dbEvent.id, mode: 'editing' }]
      : []);
  } catch {}
  const defaultCalendarId = dbEvent?.calendar_id
    || [...state.selectedCalendarIds].find(id => state.calendars.some(c => c.id === id))
    || state.calendars[0]?.id
    || '';

  const calsOptions = state.calendars.map(c => `
    <option value="${escapeHtml(c.id)}" ${defaultCalendarId === c.id ? 'selected' : ''}>${escapeHtml(c.title)}</option>
  `).join('');

  const startVal = new Date(dbEvent?.start_time || defaults?.start_time || Date.now());
  const endVal = new Date(dbEvent?.end_time || defaults?.end_time || (Date.now() + 60 * 60 * 1000));

  // Format as YYYY-MM-DDTHH:MM
  const formatDateTimeLocal = (date) => {
    const pad = (n) => String(n).padStart(2, '0');
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
  };

  const html = `
    <form id="drawerEventForm">
      <div class="calendar-drawer-form-inner">
        <div class="calendar-form-group">
          <label class="ctox-field-label">Titel</label>
          <input type="text" class="ctox-input" name="title" value="${escapeHtml(dbEvent?.title || '')}" required placeholder="z. B. Weekly Sync" aria-describedby="event-title-error" />
          <div class="calendar-field-error" id="event-title-error" data-error-for="title"></div>
        </div>

        <div class="calendar-form-row">
          <div class="calendar-form-group">
            <label class="ctox-field-label">Kalender</label>
            <select class="ctox-select" name="calendar_id" id="drawerEventCalendarSelect" required aria-describedby="event-calendar-error">
              ${calsOptions || '<option value="" disabled selected>Keine Kalender verfügbar</option>'}
            </select>
            <div class="calendar-field-error" id="event-calendar-error" data-error-for="calendar_id"></div>
          </div>
          <div class="calendar-form-group">
            <label class="ctox-field-label">Ort / Meeting URL</label>
            <input type="text" class="ctox-input" name="location" value="${escapeHtml(dbEvent?.location || '')}" placeholder="Physisch oder Online Link" />
          </div>
        </div>

        <div class="calendar-form-row">
          <div class="calendar-form-group">
            <label class="ctox-field-label">Startzeit</label>
            <input type="datetime-local" class="ctox-input" name="start_time" value="${formatDateTimeLocal(startVal)}" required aria-describedby="event-start-error" />
            <div class="calendar-field-error" id="event-start-error" data-error-for="start_time"></div>
          </div>
          <div class="calendar-form-group">
            <label class="ctox-field-label">Endzeit</label>
            <input type="datetime-local" class="ctox-input" name="end_time" value="${formatDateTimeLocal(endVal)}" required aria-describedby="event-end-error" />
            <div class="calendar-field-error" id="event-end-error" data-error-for="end_time"></div>
          </div>
        </div>

        <div class="calendar-form-group">
          <label class="ctox-field-label">Wiederholung</label>
          <select class="ctox-select" name="recurrence_rule">
            <option value="" ${!dbEvent?.recurrence_rule ? 'selected' : ''}>Keine</option>
            <option value="FREQ=DAILY;INTERVAL=1" ${dbEvent?.recurrence_rule?.includes('DAILY') ? 'selected' : ''}>Täglich</option>
            <option value="FREQ=WEEKLY;INTERVAL=1" ${dbEvent?.recurrence_rule?.includes('WEEKLY') ? 'selected' : ''}>Wöchentlich</option>
            <option value="FREQ=MONTHLY;INTERVAL=1" ${dbEvent?.recurrence_rule?.includes('MONTHLY') ? 'selected' : ''}>Monatlich</option>
          </select>
        </div>

        <div class="calendar-form-group">
          <label class="ctox-field-label">Beschreibung</label>
          <textarea class="ctox-textarea" name="description" rows="3" placeholder="Notizen...">${escapeHtml(dbEvent?.description || '')}</textarea>
        </div>
      </div>

      <div class="calendar-drawer-actions">
        <div>
          ${dbEvent ? '<button type="button" class="ctox-button is-danger" id="btnDeleteEvent">Termin löschen</button>' : ''}
        </div>
        <div class="calendar-drawer-actions-right">
          <button type="button" class="ctox-button" id="btnCancelDrawer">Abbrechen</button>
          <button type="submit" class="ctox-button is-primary" data-submit-action>Speichern</button>
        </div>
      </div>
    </form>
  `;

  openDrawer('Termin', dbEvent ? 'Termin bearbeiten' : 'Neuer Termin', html);
  updateEventFormPresenceHint();

  // Form Events
  const form = els.drawer.querySelector('#drawerEventForm');
  const validate = () => validateEventFormValues(formToObject(form), state.calendars);
  const updateValidity = installFormValidation(form, validate);
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const validation = updateValidity({ focusFirstInvalid: true });
    if (!validation.valid) return;
    const data = new FormData(form);
    const startMs = new Date(data.get('start_time')).getTime();
    const endMs = new Date(data.get('end_time')).getTime();

    const db = calendarDb();
    if (!db) return;

    const fields = {
      calendar_id: data.get('calendar_id'),
      title: String(data.get('title') || '').trim(),
      location: String(data.get('location') || '').trim(),
      start_time: startMs,
      end_time: endMs,
      recurrence_rule: data.get('recurrence_rule') || null,
      description: String(data.get('description') || '').trim(),
      updated_at_ms: Date.now()
    };

    if (dbEvent) {
      const doc = await db.calendar_events.findOne(dbEvent.id).exec();
      if (doc) {
        await doc.patch(fields);
      }
    } else {
      await db.calendar_events.insert({
        id: 'evt_' + generateUUID(),
        ...fields,
        created_at_ms: Date.now()
      });
    }

    closeDrawer();
  });

  els.drawer.querySelector('#btnDeleteEvent')?.addEventListener('click', async () => {
    if (!confirm('Diesen Termin wirklich löschen?')) return;
    const db = calendarDb();
    if (!db || !dbEvent) return;
    const doc = await db.calendar_events.findOne(dbEvent.id).exec();
    if (doc) {
      await doc.remove();
    }
    closeDrawer();
  });

  els.drawer.querySelector('#btnCancelDrawer')?.addEventListener('click', closeDrawer);

  // Keep the calendar dropdown in sync with the live RxDB calendar list so the
  // select is populated even if calendars arrive after the form was opened
  // (e.g. during the initial sync window with 30 calendars).
  wireDrawerCalendarSelectLive(dbEvent?.calendar_id);
}

function wireDrawerCalendarSelectLive(preferredCalendarId) {
  if (state.activeFormSubscription) {
    state.activeFormSubscription.unsubscribe();
    state.activeFormSubscription = null;
  }
  const db = calendarDb();
  if (!db || !db.calendar_calendars) return;

  const renderOptions = (cals) => {
    const select = els.drawer?.querySelector('#drawerEventCalendarSelect');
    if (!select) return;
    const previous = select.value || preferredCalendarId || '';
    if (!cals || cals.length === 0) {
      select.innerHTML = '<option value="" disabled selected>Keine Kalender verfügbar</option>';
      return;
    }
    select.innerHTML = cals.map(c => {
      const selected = (c.id === previous) ? 'selected' : '';
      return `<option value="${c.id}" ${selected}>${escapeHtml(c.title || c.id)}</option>`;
    }).join('');
    // Restore selection if possible; otherwise leave first option active.
    if (previous && cals.some(c => c.id === previous)) {
      select.value = previous;
    }
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };

  // Initial fill from cache, then live subscribe.
  db.calendar_calendars.find().exec()
    .then(docs => renderOptions(docs.map(d => d.toJSON())))
    .catch(err => console.warn('[calendar] initial calendar fetch failed', err));

  state.activeFormSubscription = db.calendar_calendars.find().$.subscribe(docs => {
    renderOptions(docs.map(d => d.toJSON()));
  });
}

// 2. BOOKING PAGE FORM

function openBookingPageForm(bpId = null) {
  state.editingType = 'bookingPage';
  const dbBp = bpId ? state.bookingPages.find(p => p.id === bpId) : null;
  state.editingItem = dbBp;

  const html = `
    <form id="drawerBookingPageForm">
      <div class="calendar-drawer-form-inner">
        <div class="calendar-form-group">
          <label class="ctox-field-label">Titel des Buchungs-Links</label>
          <input type="text" class="ctox-input" name="title" value="${escapeHtml(dbBp?.title || '')}" required placeholder="z. B. 30 Min. Erstgespräch" aria-describedby="booking-title-error" />
          <div class="calendar-field-error" id="booking-title-error" data-error-for="title"></div>
        </div>

        <div class="calendar-form-row">
          <div class="calendar-form-group">
            <label class="ctox-field-label">Link-Kürzel (Slug)</label>
            <input type="text" class="ctox-input" name="slug" value="${escapeHtml(dbBp?.slug || '')}" required placeholder="z. B. erstgespraech" aria-describedby="booking-slug-error" />
            <div class="calendar-field-error" id="booking-slug-error" data-error-for="slug"></div>
          </div>
          <div class="calendar-form-group">
            <label class="ctox-field-label">Dauer (Minuten)</label>
            <input type="number" class="ctox-input" name="duration_minutes" min="5" max="480" value="${dbBp?.duration_minutes || 30}" required aria-describedby="booking-duration-error" />
            <div class="calendar-field-error" id="booking-duration-error" data-error-for="duration_minutes"></div>
          </div>
        </div>

        <div class="calendar-form-row">
          <div class="calendar-form-group">
            <label class="ctox-field-label">Puffer Davor (Minuten)</label>
            <input type="number" class="ctox-input" name="buffer_before_minutes" value="${dbBp?.buffer_before_minutes || 5}" />
          </div>
          <div class="calendar-form-group">
            <label class="ctox-field-label">Puffer Danach (Minuten)</label>
            <input type="number" class="ctox-input" name="buffer_after_minutes" value="${dbBp?.buffer_after_minutes || 10}" />
          </div>
        </div>

        <div class="calendar-form-row">
          <div class="calendar-form-group">
            <label class="ctox-field-label">Mindestvorlauf (Minuten)</label>
            <input type="number" class="ctox-input" name="min_notice_minutes" value="${dbBp?.min_notice_minutes || 120}" />
          </div>
          <div class="calendar-form-group">
            <label class="ctox-field-label">Max. Tage im Voraus</label>
            <input type="number" class="ctox-input" name="max_days_ahead" value="${dbBp?.max_days_ahead || 30}" />
          </div>
        </div>

        <div class="calendar-form-row">
          <div class="calendar-form-group">
            <label class="ctox-field-label">Standort-Typ</label>
            <select class="ctox-select" name="location_mode">
              <option value="link" ${dbBp?.location_mode === 'link' ? 'selected' : ''}>Online-Meeting Link</option>
              <option value="phone" ${dbBp?.location_mode === 'phone' ? 'selected' : ''}>Telefonnummer</option>
              <option value="physical" ${dbBp?.location_mode === 'physical' ? 'selected' : ''}>Physischer Ort</option>
            </select>
          </div>
          <div class="calendar-form-group">
            <label class="ctox-field-label">Status</label>
            <select class="ctox-select" name="status">
              <option value="active" ${dbBp?.status === 'active' ? 'selected' : ''}>Aktiv</option>
              <option value="inactive" ${dbBp?.status === 'inactive' ? 'selected' : ''}>Inaktiv</option>
            </select>
          </div>
        </div>

        <div class="calendar-form-group">
          <label class="ctox-field-label">Beschreibung</label>
          <textarea class="ctox-textarea" name="description" rows="3" placeholder="Beschreibung für den Kunden...">${escapeHtml(dbBp?.description || '')}</textarea>
        </div>
      </div>

      <div class="calendar-drawer-actions">
        <div>
          ${dbBp ? '<button type="button" class="ctox-button is-danger" id="btnDeleteBp">Löschen</button>' : ''}
        </div>
        <div class="calendar-drawer-actions-right">
          <button type="button" class="ctox-button" id="btnCancelDrawer">Abbrechen</button>
          <button type="submit" class="ctox-button is-primary" data-submit-action>Speichern</button>
        </div>
      </div>
    </form>
  `;

  openDrawer('Buchungsseite', dbBp ? 'Buchungsseite bearbeiten' : 'Neue Buchungsseite', html);

  const form = els.drawer.querySelector('#drawerBookingPageForm');
  const validate = () => validateBookingPageFormValues(formToObject(form));
  const updateValidity = installFormValidation(form, validate);
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const validation = updateValidity({ focusFirstInvalid: true });
    if (!validation.valid) return;
    const data = new FormData(form);
    const db = calendarDb();
    if (!db) return;

    // Slug clean up
    const cleanSlug = normalizeSlug(data.get('slug'));

    const fields = {
      title: String(data.get('title') || '').trim(),
      slug: cleanSlug,
      duration_minutes: parseInt(data.get('duration_minutes'), 10),
      buffer_before_minutes: parseInt(data.get('buffer_before_minutes') || 0, 10),
      buffer_after_minutes: parseInt(data.get('buffer_after_minutes') || 0, 10),
      min_notice_minutes: parseInt(data.get('min_notice_minutes') || 0, 10),
      max_days_ahead: parseInt(data.get('max_days_ahead') || 30, 10),
      location_mode: data.get('location_mode'),
      status: data.get('status'),
      description: String(data.get('description') || '').trim(),
      updated_at_ms: Date.now()
    };

    if (dbBp) {
      const doc = await db.calendar_booking_pages.findOne(dbBp.id).exec();
      if (doc) {
        await doc.patch(fields);
      }
    } else {
      const newBpId = 'bp_' + generateUUID();
      await db.calendar_booking_pages.insert({
        id: newBpId,
        calendar_ids: [state.calendars[0]?.id || 'default'],
        host_user_ids: ['default_user'],
        ...fields,
        created_at_ms: Date.now()
      });

      // Also automatically seed availability rules for new booking pages
      for (let weekday = 1; weekday <= 5; weekday++) {
        await db.calendar_availability_rules.insert({
          id: `rule_bp_${newBpId}_day_${weekday}`,
          booking_page_id: newBpId,
          weekday: weekday,
          start_minute: 540,
          end_minute: 1020,
          timezone: 'Europe/Berlin',
          status: 'active'
        });
      }
    }

    closeDrawer();
  });

  els.drawer.querySelector('#btnDeleteBp')?.addEventListener('click', async () => {
    if (!confirm('Diese Buchungsseite wirklich löschen?')) return;
    const db = calendarDb();
    if (!db || !dbBp) return;
    const doc = await db.calendar_booking_pages.findOne(dbBp.id).exec();
    if (doc) {
      await doc.remove();
    }
    closeDrawer();
  });

  els.drawer.querySelector('#btnCancelDrawer')?.addEventListener('click', closeDrawer);
}

// 3. CALENDAR FORM

function openCalendarForm(calId = null) {
  state.editingType = 'calendar';
  const dbCal = calId ? state.calendars.find(c => c.id === calId) : null;
  state.editingItem = dbCal;

  const html = `
    <form id="drawerCalendarForm">
      <div class="calendar-drawer-form-inner">
        <div class="calendar-form-group">
          <label class="ctox-field-label">Kalendertitel</label>
          <input type="text" class="ctox-input" name="title" value="${escapeHtml(dbCal?.title || '')}" required placeholder="z. B. Privat" aria-describedby="calendar-title-error" />
          <div class="calendar-field-error" id="calendar-title-error" data-error-for="title"></div>
        </div>

        <div class="calendar-form-group">
          <label class="ctox-field-label">Farbe</label>
          <input type="color" class="ctox-input" name="color" value="${dbCal?.color || '#3b82f6'}" />
        </div>
      </div>

      <div class="calendar-drawer-actions">
        <div>
          ${dbCal ? '<button type="button" class="ctox-button is-danger" id="btnDeleteCal">Kalender löschen</button>' : ''}
        </div>
        <div class="calendar-drawer-actions-right">
          <button type="button" class="ctox-button" id="btnCancelDrawer">Abbrechen</button>
          <button type="submit" class="ctox-button is-primary" data-submit-action>Speichern</button>
        </div>
      </div>
    </form>
  `;

  openDrawer('Kalender', dbCal ? 'Kalender bearbeiten' : 'Neuer Kalender', html);

  const form = els.drawer.querySelector('#drawerCalendarForm');
  const validate = () => validateCalendarFormValues(formToObject(form));
  const updateValidity = installFormValidation(form, validate);
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const validation = updateValidity({ focusFirstInvalid: true });
    if (!validation.valid) return;
    const data = new FormData(form);
    const db = calendarDb();
    if (!db) return;

    const fields = {
      title: String(data.get('title') || '').trim(),
      color: data.get('color'),
      updated_at_ms: Date.now()
    };

    if (dbCal) {
      const doc = await db.calendar_calendars.findOne(dbCal.id).exec();
      if (doc) {
        await doc.patch(fields);
      }
    } else {
      const sources = await db.calendar_sources.find().exec();
      const localSourceId = sources[0]?.id || 'default_source';

      await db.calendar_calendars.insert({
        id: 'cal_' + generateUUID(),
        source_id: localSourceId,
        visibility: true,
        owner_user_id: 'default_user',
        timezone: 'Europe/Berlin',
        ...fields,
        created_at_ms: Date.now()
      });
    }

    closeDrawer();
  });

  els.drawer.querySelector('#btnDeleteCal')?.addEventListener('click', async () => {
    if (!confirm('Diesen Kalender wirklich löschen? Alle zugehörigen Termine gehen verloren.')) return;
    const db = calendarDb();
    if (!db || !dbCal) return;

    // Delete calendar events first
    const events = await db.calendar_events.find({ selector: { calendar_id: dbCal.id } }).exec();
    for (const evt of events) {
      await evt.remove();
    }

    // Delete calendar itself
    const doc = await db.calendar_calendars.findOne(dbCal.id).exec();
    if (doc) {
      await doc.remove();
    }
    closeDrawer();
  });

  els.drawer.querySelector('#btnCancelDrawer')?.addEventListener('click', closeDrawer);
}

// 4. BOOKING DETAIL MODAL

function openBookingDetail(bkId) {
  const bk = state.bookings.find(b => b.id === bkId);
  if (!bk) return;

  const bp = state.bookingPages.find(p => p.id === bk.booking_page_id);
  const startStr = new Date(bk.slot_start_ms).toLocaleString();
  const endStr = new Date(bk.slot_end_ms).toLocaleTimeString();

  const html = `
    <div class="calendar-drawer-form-inner">
      <dl class="ctox-fields ctox-fields--stacked">
        <dt>Kunde</dt>
        <dd>${escapeHtml(bk.attendee_name)}</dd>
        <dt>E-Mail</dt>
        <dd>${escapeHtml(bk.attendee_email)}</dd>
        ${bk.attendee_phone ? `
        <dt>Telefonnummer</dt>
        <dd>${escapeHtml(bk.attendee_phone)}</dd>` : ''}
        <dt>Terminart</dt>
        <dd>${escapeHtml(bp?.title || 'Beratung')}</dd>
        <dt>Zeitfenster</dt>
        <dd>${startStr} - ${endStr}</dd>
        <dt>Status</dt>
        <dd><span class="ctox-badge ${bk.status === 'confirmed' ? 'is-success' : 'is-danger'}">${bk.status === 'confirmed' ? 'Bestätigt' : 'Storniert'}</span></dd>
      </dl>
    </div>

    <div class="calendar-drawer-actions">
      <div>
        ${bk.status === 'confirmed' ? '<button type="button" class="ctox-button is-danger" id="btnCancelBooking">Termin Stornieren</button>' : ''}
      </div>
      <div class="calendar-drawer-actions-right">
        <button type="button" class="ctox-button is-primary" id="btnCancelDrawer">Schließen</button>
      </div>
    </div>
  `;

  openDrawer('Buchung', 'Buchungsdetails', html);

  els.drawer.querySelector('#btnCancelBooking')?.addEventListener('click', async () => {
    if (!confirm('Diesen Termin wirklich stornieren?')) return;
    const db = calendarDb();
    if (!db) return;

    // Update booking status
    const doc = await db.calendar_bookings.findOne(bk.id).exec();
    if (doc) {
      await doc.patch({
        status: 'cancelled',
        updated_at_ms: Date.now()
      });
    }

    // Also delete associated calendar event if any exists
    if (bk.event_id) {
      const evtDoc = await db.calendar_events.findOne(bk.event_id).exec();
      if (evtDoc) {
        await evtDoc.remove();
      }
    }

    closeDrawer();
  });

  els.drawer.querySelector('#btnCancelDrawer')?.addEventListener('click', closeDrawer);
}

// ----------------------------------------------------
// UTILITIES
// ----------------------------------------------------

function actionIcon(name) {
  return state.ctx?.getActionIcon?.(name) || '';
}

function formToObject(form) {
  return Object.fromEntries(new FormData(form).entries());
}

function validateEventFormValues(values, calendars = []) {
  const errors = {};
  const title = String(values.title || '').trim();
  const calendarId = String(values.calendar_id || '').trim();
  const startMs = new Date(values.start_time).getTime();
  const endMs = new Date(values.end_time).getTime();

  if (!title) errors.title = 'Titel ist erforderlich.';
  if (!calendarId || !calendars.some(calendar => calendar.id === calendarId)) {
    errors.calendar_id = 'Wähle einen gültigen Kalender.';
  }
  if (!Number.isFinite(startMs)) errors.start_time = 'Startzeit ist erforderlich.';
  if (!Number.isFinite(endMs)) {
    errors.end_time = 'Endzeit ist erforderlich.';
  } else if (Number.isFinite(startMs) && endMs <= startMs) {
    errors.end_time = 'Endzeit muss nach der Startzeit liegen.';
  }

  return { valid: Object.keys(errors).length === 0, errors };
}

function validateBookingPageFormValues(values) {
  const errors = {};
  const title = String(values.title || '').trim();
  const slug = normalizeSlug(values.slug);
  const duration = Number.parseInt(values.duration_minutes, 10);

  if (!title) errors.title = 'Titel ist erforderlich.';
  if (!slug) errors.slug = 'Slug ist erforderlich und darf nur Buchstaben, Zahlen und Bindestriche enthalten.';
  if (!Number.isFinite(duration) || duration < 5 || duration > 480) {
    errors.duration_minutes = 'Dauer muss zwischen 5 und 480 Minuten liegen.';
  }

  return { valid: Object.keys(errors).length === 0, errors };
}

function validateCalendarFormValues(values) {
  const title = String(values.title || '').trim();
  const errors = title ? {} : { title: 'Kalendertitel ist erforderlich.' };
  return { valid: Object.keys(errors).length === 0, errors };
}

function installFormValidation(form, validate) {
  const submit = form.querySelector('[data-submit-action], [type="submit"]');
  const update = ({ focusFirstInvalid = false } = {}) => {
    const result = validate();
    const errorEntries = Object.entries(result.errors);

    form.querySelectorAll('[data-error-for]').forEach(errorNode => {
      const field = errorNode.dataset.errorFor;
      errorNode.textContent = result.errors[field] || '';
      errorNode.hidden = !result.errors[field];
    });

    form.querySelectorAll('input, select, textarea').forEach(field => {
      const hasError = Boolean(result.errors[field.name]);
      field.setAttribute('aria-invalid', hasError ? 'true' : 'false');
    });

    if (submit) {
      submit.disabled = !result.valid;
      submit.setAttribute('aria-disabled', result.valid ? 'false' : 'true');
    }

    if (focusFirstInvalid && errorEntries.length > 0) {
      [...form.querySelectorAll('input, select, textarea')]
        .find(field => field.name === errorEntries[0][0])
        ?.focus();
    }

    return result;
  };

  form.addEventListener('input', () => update());
  form.addEventListener('change', () => update());
  update();
  return update;
}

function normalizeSlug(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9-_]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

function safeDomId(value) {
  return String(value || 'item').replace(/[^a-zA-Z0-9_-]/g, '-');
}

function safeColor(value) {
  const color = String(value || '').trim();
  return /^#[0-9a-fA-F]{3}([0-9a-fA-F]{3})?$/.test(color) ? color : '#3b82f6';
}

function findEventForRenderedCalendarElement(eventEl, events = state.events) {
  const title = eventEl?.querySelector?.('.ec-event-title')?.textContent?.trim();
  if (!title) return null;
  const matches = events.filter(event => event.title === title);
  if (matches.length === 1) return matches[0];

  const datetime = eventEl.querySelector?.('.ec-event-time')?.getAttribute('datetime');
  const startMs = datetime ? new Date(datetime).getTime() : NaN;
  if (Number.isFinite(startMs)) {
    const exactStart = matches.find(event => Number(event.start_time) === startMs);
    if (exactStart) return exactStart;
  }

  return null;
}

function generateUUID() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  })[char]);
}

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom):
// the bookings/holds context pane is shown only when a booking page is
// selected and the user has not collapsed it.
function calendarContextVisible(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

export const __calendarTestHooks = {
  findEventForRenderedCalendarElement,
  normalizeSlug,
  validateBookingPageFormValues,
  validateCalendarFormValues,
  validateEventFormValues,
  calendarContextVisible,
  computeViewBandCounts,
  buildCalendarExport,
  parseCalendarImport,
};
