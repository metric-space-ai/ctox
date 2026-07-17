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

const labels = {
  de: {
    calendar: 'Kalender',
    today: 'Heute',
    day: 'Tag',
    week: 'Woche',
    month: 'Monat',
    list: 'Liste',
    myCalendars: 'Meine Kalender',
    bookingLinks: 'Buchungslinks',
    externalSources: 'Externe Quellen',
    newEvent: 'Neuer Termin',
    editEvent: 'Termin bearbeiten',
    dataReady: 'Daten geladen',
    noDatabase: 'Keine lokale Datenbank verbunden',
    save: 'Speichern',
    delete: 'Löschen',
    cancel: 'Abbrechen'
  },
  en: {
    calendar: 'Calendar',
    today: 'Today',
    day: 'Day',
    week: 'Week',
    month: 'Month',
    list: 'List',
    myCalendars: 'My Calendars',
    bookingLinks: 'Booking Links',
    externalSources: 'External Sources',
    newEvent: 'New Event',
    editEvent: 'Edit Event',
    dataReady: 'Data loaded',
    noDatabase: 'No local database connected',
    save: 'Save',
    delete: 'Delete',
    cancel: 'Cancel'
  }
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

  // Active calendar view state
  activeView: 'timeGridWeek', // dayGridMonth, timeGridWeek, timeGridDay, listWeek
  selectedCalendarIds: new Set(),

  // Active editing item in Drawer
  editingType: null, // 'event' | 'bookingPage' | 'calendar'
  editingItem: null,
  selectedBookingPageId: null,

  // Subscriptions & Cleanups
  rxSubscriptions: [],
  activeFormSubscription: null,
  calendarViewInstance: null,
  renderTimer: null,
  domHandlers: null
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
  els.calendarList = host.querySelector('#calendarListContainer');
  els.bookingPagesList = host.querySelector('#bookingPagesListContainer');
  els.calendarTitle = host.querySelector('#calendarViewTitle');
  els.calendarRangeTitle = host.querySelector('#calendarRangeTitle');
  els.calendarDataStatus = host.querySelector('#calendarDataStatus');
  els.eventCalendarMount = host.querySelector('#eventCalendarView');

  els.bookingContext = host.querySelector('#bookingContext');
  els.bookingHoldsList = host.querySelector('#bookingHoldsList');
  els.bookingsList = host.querySelector('#bookingsList');

  // Buttons
  els.btnNewEvent = host.querySelector('#btnNewEvent');
  els.btnPrev = host.querySelector('#prevPeriodBtn');
  els.btnNext = host.querySelector('#nextPeriodBtn');
  els.btnToday = host.querySelector('#todayPeriodBtn');
  els.btnAddNewCalendar = host.querySelector('#addNewCalendarBtn');
  els.btnAddBookingPage = host.querySelector('#addBookingPageBtn');

  // Drawer / Inspector
  els.drawer = host.querySelector('#calendarInspectorDrawer');
  els.drawerKicker = host.querySelector('#drawerKicker');
  els.drawerTitle = host.querySelector('#drawerTitle');
  els.drawerContent = host.querySelector('#drawerContent');
  els.closeDrawerBtn = host.querySelector('#closeDrawerBtn');
}

function wireEvents() {
  state.domHandlers = {
    newEvent: () => openEventForm(),
    prev: () => state.calendarViewInstance?.prev(),
    next: () => state.calendarViewInstance?.next(),
    today: () => state.calendarViewInstance?.today(),
    newCalendar: () => openCalendarForm(),
    newBookingPage: () => openBookingPageForm(),
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
  els.btnPrev?.addEventListener('click', state.domHandlers.prev);
  els.btnNext?.addEventListener('click', state.domHandlers.next);
  els.btnToday?.addEventListener('click', state.domHandlers.today);
  els.btnAddNewCalendar?.addEventListener('click', state.domHandlers.newCalendar);
  els.btnAddBookingPage?.addEventListener('click', state.domHandlers.newBookingPage);
  els.eventCalendarMount?.addEventListener('click', state.domHandlers.renderedEventClick, true);
  els.closeDrawerBtn?.addEventListener('click', state.domHandlers.closeDrawer);
  document.addEventListener('keydown', state.domHandlers.keydown);
}

function unbindEvents() {
  const handlers = state.domHandlers;
  if (!handlers) return;
  els.btnNewEvent?.removeEventListener('click', handlers.newEvent);
  els.btnPrev?.removeEventListener('click', handlers.prev);
  els.btnNext?.removeEventListener('click', handlers.next);
  els.btnToday?.removeEventListener('click', handlers.today);
  els.btnAddNewCalendar?.removeEventListener('click', handlers.newCalendar);
  els.btnAddBookingPage?.removeEventListener('click', handlers.newBookingPage);
  els.eventCalendarMount?.removeEventListener('click', handlers.renderedEventClick, true);
  els.closeDrawerBtn?.removeEventListener('click', handlers.closeDrawer);
  document.removeEventListener('keydown', handlers.keydown);
  state.domHandlers = null;
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
  renderCalendarsSidebar();
  renderBookingPagesSidebar();
  renderAuditingLists();
  renderDataStatus();

  // Refresh Calendar adapter events
  const filteredEvents = state.events.filter(e => state.selectedCalendarIds.has(e.calendar_id));
  state.calendarViewInstance?.setEvents(filteredEvents, state.calendars);
}

function renderDataStatus() {
  if (!els.calendarDataStatus) return;
  const setStatus = (text, variant) => {
    els.calendarDataStatus.textContent = text;
    els.calendarDataStatus.classList.toggle('is-danger', variant === 'error');
    els.calendarDataStatus.classList.toggle('is-info', variant === 'ready');
  };
  if (!calendarDb()) {
    setStatus(state.t('noDatabase', labels[state.lang].noDatabase), 'error');
    return;
  }
  const selectedEvents = state.events.filter(e => state.selectedCalendarIds.has(e.calendar_id));
  setStatus(
    `${selectedEvents.length} Termine · ${state.calendars.length} Kalender · ${state.bookingPages.length} Buchungsseiten`,
    selectedEvents.length > 0 ? 'ready' : 'empty'
  );
}

// ----------------------------------------------------
// UI RENDERING METHODS
// ----------------------------------------------------

function renderCalendarsSidebar() {
  if (!els.calendarList) return;

  if (state.calendars.length === 0) {
    els.calendarList.innerHTML = `<div class="ctox-empty">Keine Kalender.</div>`;
    return;
  }

  let html = '';
  state.calendars.forEach(cal => {
    const checked = state.selectedCalendarIds.has(cal.id);
    const checkboxId = `calendar-toggle-${safeDomId(cal.id)}`;
    html += `
      <div class="ctox-list-item calendar-item" data-id="${escapeHtml(cal.id)}" data-context-record-id="${escapeHtml(cal.id)}" data-context-record-type="calendar" data-context-label="${escapeHtml(cal.title || cal.id)}">
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
  });

  els.calendarList.innerHTML = html;

  // Bind listeners
  els.calendarList.querySelectorAll('[data-action="toggle-cal"]').forEach(el => {
    el.addEventListener('change', (e) => {
      const id = el.dataset.id;
      if (e.target.checked) {
        state.selectedCalendarIds.add(id);
      } else {
        state.selectedCalendarIds.delete(id);
      }
      scheduleRender();
    });
  });

  els.calendarList.querySelectorAll('[data-action="edit-cal"]').forEach(el => {
    el.addEventListener('click', (e) => {
      e.stopPropagation();
      openCalendarForm(el.dataset.id);
    });
  });
}

function renderBookingPagesSidebar() {
  if (!els.bookingPagesList) return;

  if (state.bookingPages.length === 0) {
    els.bookingPagesList.innerHTML = `<div class="ctox-empty">Keine Buchungsseiten.</div>`;
    return;
  }

  let html = '';
  state.bookingPages.forEach(bp => {
    const safeSlug = normalizeSlug(bp.slug) || String(bp.slug || '').replace(/[^a-zA-Z0-9-_]/g, '');
    const publicUrl = `${window.location.origin}/book/${encodeURIComponent(safeSlug)}`;
    const isActive = bp.status === 'active';
    const isSelected = bp.id === state.selectedBookingPageId;
    html += `
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
  });

  els.bookingPagesList.innerHTML = html;

  els.bookingPagesList.querySelectorAll('[data-action="select-bp"]').forEach(el => {
    const select = () => {
      state.selectedBookingPageId = el.dataset.id || null;
      scheduleRender();
    };
    el.addEventListener('click', (event) => {
      if (event.target.closest('a, button')) return;
      select();
    });
    el.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      select();
    });
  });

  els.bookingPagesList.querySelectorAll('[data-action="edit-bp"]').forEach(el => {
    el.addEventListener('click', (event) => {
      event.stopPropagation();
      openBookingPageForm(el.dataset.id);
    });
  });
}

function renderAuditingLists() {
  const selectedPage = state.bookingPages.find(page => page.id === state.selectedBookingPageId) || null;
  if (els.bookingContext) {
    if (selectedPage) {
      const safeSlug = normalizeSlug(selectedPage.slug) || String(selectedPage.slug || '').replace(/[^a-zA-Z0-9-_]/g, '');
      els.bookingContext.innerHTML = `
        <div class="ctox-callout calendar-context-card">
          <span class="ctox-field-label">Ausgewählte Buchungsseite</span>
          <strong>${escapeHtml(selectedPage.title)}</strong>
          <span>${Number(selectedPage.duration_minutes) || 0} Min · /book/${escapeHtml(safeSlug)}</span>
          <button type="button" class="ctox-button ctox-button--sm" data-action="clear-booking-selection">Alle Buchungen anzeigen</button>
        </div>
      `;
      els.bookingContext.querySelector('[data-action="clear-booking-selection"]')?.addEventListener('click', () => {
        state.selectedBookingPageId = null;
        scheduleRender();
      });
    } else {
      els.bookingContext.innerHTML = `<div class="ctox-callout calendar-context-card">Buchungsseite wählen, um Holds und Buchungen zu filtern.</div>`;
    }
  }

  // 1. Holds List
  if (els.bookingHoldsList) {
    const activeHolds = state.holds.filter(h => {
      const active = h.status === 'active' && h.expires_at_ms > Date.now();
      return active && (!selectedPage || h.booking_page_id === selectedPage.id);
    });
    if (activeHolds.length === 0) {
      els.bookingHoldsList.innerHTML = `<div class="ctox-empty">${selectedPage ? 'Keine aktiven Holds für diese Buchungsseite.' : 'Keine aktiven Holds.'}</div>`;
    } else {
      els.bookingHoldsList.innerHTML = activeHolds.map(hold => {
        const bp = state.bookingPages.find(p => p.id === hold.booking_page_id);
        const startStr = new Date(hold.slot_start_ms).toLocaleString();
        const expiresStr = new Date(hold.expires_at_ms).toLocaleTimeString();
        return `
          <div class="ctox-list-item calendar-audit-item">
            <div class="calendar-audit-head">
              <strong>${escapeHtml(bp?.title || 'Buchung hold')}</strong>
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
      .filter(booking => !selectedPage || booking.booking_page_id === selectedPage.id)
      .sort((a, b) => b.slot_start_ms - a.slot_start_ms);
    if (sortedBookings.length === 0) {
      els.bookingsList.innerHTML = `<div class="ctox-empty">${selectedPage ? 'Keine bestätigten Buchungen für diese Buchungsseite.' : 'Keine bestätigten Buchungen.'}</div>`;
    } else {
      els.bookingsList.innerHTML = sortedBookings.map(bk => {
        const bp = state.bookingPages.find(p => p.id === bk.booking_page_id);
        const startStr = new Date(bk.slot_start_ms).toLocaleString();
        const statusBadge = bk.status === 'confirmed' ? 'is-success' : 'is-danger';
        return `
          <div class="ctox-list-item calendar-audit-item" data-action="view-booking" data-id="${bk.id}">
            <div class="calendar-audit-head">
              <strong>${escapeHtml(bk.attendee_name)}</strong>
              <span class="ctox-badge ${statusBadge}">${bk.status === 'confirmed' ? 'Bestätigt' : 'Storniert'}</span>
            </div>
            <small>Event: ${escapeHtml(bp?.title || 'Beratung')}</small>
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
// EVENT CALENDAR UI SETUP
// ----------------------------------------------------

function initCalendarView() {
  if (!els.eventCalendarMount) return;

  state.calendarViewInstance = createCalendarView({
    root: els.eventCalendarMount,
    events: state.events,
    calendars: state.calendars,
    view: 'timeGridWeek',
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

export const __calendarTestHooks = {
  findEventForRenderedCalendarElement,
  normalizeSlug,
  validateBookingPageFormValues,
  validateCalendarFormValues,
  validateEventFormValues,
};
