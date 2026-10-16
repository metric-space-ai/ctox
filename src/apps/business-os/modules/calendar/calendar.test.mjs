import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): esbuild aus dem Klon, sonst ESBUILD_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie PLAYWRIGHT_MODULE in den v4-api-Tests). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { build } = loadDev('esbuild', 'ESBUILD_MODULE');
import { calendarEventContext } from './calendar-view-adapter.js';

globalThis.window = globalThis.window || {};

async function importBrowserBundle(relativePath) {
  const bundledModule = await build({
    entryPoints: [fileURLToPath(new URL(relativePath, import.meta.url))],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    write: false,
    // App-API 4.0: ctox:shared/x.js löst der Service Worker auf (§16.15); hier wie im SW auf den Shell-Pfad.
    plugins: [{
      name: 'ctox-shared',
      setup(b) {
        b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
      },
    }],
  });

  const [{ text: bundledSource }] = bundledModule.outputFiles;
  return import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);
}

const { __calendarTestHooks: hooks } = await importBrowserBundle('./index.js');
const { __calendarViewAdapterTestHooks: adapterHooks } = await importBrowserBundle('./calendar-view-adapter.js');

test('recurring EventCalendar ids resolve back to the full source event id', () => {
  assert.equal(
    adapterHooks.getOriginalEventId('evt_standup_9f2c_occ_3'),
    'evt_standup_9f2c',
  );
  assert.equal(
    adapterHooks.getOriginalEventId('evt_lunch_with_michael'),
    'evt_lunch_with_michael',
  );
});

test('event validation blocks empty and inverted event submissions', () => {
  const calendars = [{ id: 'cal_work' }];
  assert.deepEqual(
    hooks.validateEventFormValues({
      title: '',
      calendar_id: '',
      start_time: '2026-05-27T10:00',
      end_time: '2026-05-27T09:00',
    }, calendars).errors,
    {
      title: 'Titel ist erforderlich.',
      calendar_id: 'Wähle einen gültigen Kalender.',
      end_time: 'Endzeit muss nach der Startzeit liegen.',
    },
  );

  assert.equal(
    hooks.validateEventFormValues({
      title: 'Weekly Sync',
      calendar_id: 'cal_work',
      start_time: '2026-05-27T09:00',
      end_time: '2026-05-27T10:00',
    }, calendars).valid,
    true,
  );
});

test('booking page validation normalizes slugs and rejects impossible durations', () => {
  assert.equal(hooks.normalizeSlug('  Erstgespräch 30 Min! '), 'erstgesprach-30-min');
  assert.deepEqual(
    hooks.validateBookingPageFormValues({
      title: '',
      slug: '!!!',
      duration_minutes: '2',
    }).errors,
    {
      title: 'Titel ist erforderlich.',
      slug: 'Slug ist erforderlich und darf nur Buchstaben, Zahlen und Bindestriche enthalten.',
      duration_minutes: 'Dauer muss zwischen 5 und 480 Minuten liegen.',
    },
  );
});

test('calendar title validation prevents empty source creation', () => {
  assert.equal(hooks.validateCalendarFormValues({ title: 'Arbeit' }).valid, true);
  assert.deepEqual(
    hooks.validateCalendarFormValues({ title: '  ' }).errors,
    { title: 'Kalendertitel ist erforderlich.' },
  );
});

test('rendered EventCalendar element resolves to matching Business OS event', () => {
  const element = {
    querySelector(selector) {
      if (selector === '.ec-event-title') return { textContent: 'Tägliches Standup' };
      if (selector === '.ec-event-time') return { getAttribute: () => '2026-05-27T09:30:00' };
      return null;
    },
  };
  const events = [
    { id: 'evt_standup', title: 'Tägliches Standup', start_time: new Date('2026-05-27T09:30:00').getTime() },
  ];

  assert.equal(hooks.findEventForRenderedCalendarElement(element, events)?.id, 'evt_standup');
});

test('non-recurring events map with calendar colors and hidden calendars are excluded', () => {
  const events = [
    { id: 'evt_a', calendar_id: 'cal_a', title: 'Visible', start_time: 1_780_000_000_000, end_time: 1_780_003_600_000 },
    { id: 'evt_b', calendar_id: 'cal_b', title: 'Hidden', start_time: 1_780_000_000_000, end_time: 1_780_003_600_000 },
  ];
  const calendars = [
    { id: 'cal_a', color: '#123456', visibility: true },
    { id: 'cal_b', color: '#654321', visibility: false },
  ];
  const mapped = adapterHooks.prepareEventsForCalendar(
    events,
    calendars,
    new Date('2026-01-01T00:00:00Z'),
    new Date('2026-12-31T00:00:00Z'),
  );

  assert.equal(mapped.length, 1);
  assert.equal(mapped[0].id, 'evt_a');
  assert.equal(mapped[0].color, '#123456');
});

test('calendar click fallback resolves source event when EventCalendar omits public id', () => {
  const events = [
    {
      id: 'evt_standup',
      calendar_id: 'cal_work',
      title: 'Tägliches Standup',
      start_time: 1_780_000_000_000,
      end_time: 1_780_001_800_000,
      recurrence_rule: 'FREQ=DAILY;INTERVAL=1',
    },
  ];

  assert.equal(
    adapterHooks.resolveOriginalEventForCalendarClick({
      event: {
        title: 'Tägliches Standup',
        start: new Date(1_780_086_400_000),
        end: new Date(1_780_088_200_000),
        extendedProps: {},
      },
    }, events)?.id,
    'evt_standup',
  );
});

test('left column follows the canonical shell-wired column grammar', async () => {
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');

  // Filterbar: search + shard/list toggle + collapsed filter tray with reset.
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-view="cards"/);
// Betreiber-Direktive 31.08.2026: EIN Umschalt-Knopf statt Knopfpaar.
  // data-pg-view traegt die aktuelle Ansicht; beide Ziel-Icons stecken im Knopf.
  assert.equal((html.match(/data-pg-view=/g) || []).length, 1);
  assert.match(html, /data-pg-tray-toggle/);
  assert.match(html, /data-pg-tray\b/);
  assert.match(html, /data-pg-reset/);
  assert.match(html, /data-pg-name="status"/);

  // Recessed well + one-line footer.
  assert.match(html, /class="[^"]*\bctox-well\b/);
  assert.match(html, /data-pg-footer/);

  // Header actions are collected import/export icons (honest JSON I/O).
  assert.match(html, /data-action="import"/);
  assert.match(html, /data-action="export"/);

  // Counted left band: >= 2 real views (calendars + booking pages) with counts.
  const bandTabs = html.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bandTabs.length >= 2, `left band needs >= 2 views, saw ${bandTabs.length}`);
  assert.match(html, /data-pg-band="calendars"/);
  assert.match(html, /data-pg-band="pages"/);
  assert.match(html, /data-pg-count="calendars"/);
  assert.match(html, /data-pg-count="pages"/);

  // Main view band = Monat/Woche/Tag switch (3 counted tabs).
  assert.match(html, /data-calendar-view="month"/);
  assert.match(html, /data-calendar-view="week"/);
  assert.match(html, /data-calendar-view="day"/);
  assert.match(html, /data-count-view-month/);

  // Third pane is layout.right with data-right-content and default-collapsed.
  assert.match(html, /class="ctox-workspace calendar-app is-context-hidden/);
  assert.match(html, /data-right-content/);

  // The old app-owned right-pane toggle is gone; the shell wires the grammar.
  assert.doesNotMatch(html, /data-toggle-right\b/);
});

test('main view band counts month/week/day around the reference date, zeros included', () => {
  const may = (day, hour = 12) => new Date(2026, 4, day, hour).getTime(); // May 2026, local
  const events = [
    { id: 'a', calendar_id: 'cal', start_time: may(4) },   // Mon of ref week
    { id: 'b', calendar_id: 'cal', start_time: may(6) },   // same week + month
    { id: 'c', calendar_id: 'cal', start_time: may(20) },  // same month, other week
    { id: 'd', calendar_id: 'hidden', start_time: may(6) }, // filtered out (not selected)
    { id: 'e', calendar_id: 'cal', start_time: new Date(2026, 3, 30, 12).getTime() }, // April → out of month
  ];
  const selected = new Set(['cal']);
  const ref = new Date(2026, 4, 6, 9); // Wed 2026-05-06

  const counts = hooks.computeViewBandCounts(events, selected, ref);
  assert.equal(counts.day, 1);   // only 'b' on May 6
  assert.equal(counts.week, 2);  // 'a' (May 4) + 'b' (May 6), Monday-based week
  assert.equal(counts.month, 3); // a, b, c in May; 'd' excluded, 'e' in April

  // No selection set → count all calendars; zeros still render structurally.
  const all = hooks.computeViewBandCounts(events, new Set(), ref);
  assert.equal(all.day, 2); // 'b' and 'd'
  assert.equal(hooks.computeViewBandCounts([], selected, ref).month, 0);
});

test('export builds an honest snapshot and import round-trips the authoring records', () => {
  const sources = {
    calendars: [{ id: 'cal_a', title: 'Work', color: '#123456' }],
    bookingPages: [{ id: 'bp_a', slug: 'intro', title: 'Intro', duration_minutes: 30 }],
    events: [{ id: 'evt_a', calendar_id: 'cal_a', title: 'Sync', start_time: 1, end_time: 2 }],
  };
  const payload = hooks.buildCalendarExport(sources, 999);
  assert.equal(payload.kind, 'ctox-calendar-export');
  assert.equal(payload.exported_at_ms, 999);
  assert.equal(payload.calendars.length, 1);
  assert.equal(payload.booking_pages.length, 1);
  assert.equal(payload.events.length, 1);

  const parsed = hooks.parseCalendarImport(payload);
  assert.equal(parsed.calendars[0].id, 'cal_a');
  assert.equal(parsed.bookingPages[0].id, 'bp_a');
  assert.equal(parsed.events[0].id, 'evt_a');

  // Records without an id are dropped; junk yields empty arrays.
  assert.equal(hooks.parseCalendarImport({ calendars: [{ title: 'no id' }] }).calendars.length, 0);
  assert.equal(hooks.parseCalendarImport(null).events.length, 0);
  assert.equal(hooks.parseCalendarImport([]).calendars.length, 0);
});

test('v4: empty left column renders nothing – no syncing shell, no readiness (App-API 4.0)', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  assert.equal(hooks.leftEmptyContent, undefined);
  assert.doesNotMatch(js, /ctox-syncing|syncingData|collectionReadiness|subscribeCollectionReadiness/);
  assert.doesNotMatch(html, /data-calendar-left-empty/);
  assert.match(js, /disposers\.push\(api\.query\(name, \{\}\)\.subscribe\(scheduleCalendarReload\)\)/);
});

test('context pane auto-reveals only with a selection and no user collapse', () => {
  assert.equal(hooks.calendarContextVisible(true, false), true);
  assert.equal(hooks.calendarContextVisible(true, true), false);
  assert.equal(hooks.calendarContextVisible(false, false), false);
  assert.equal(hooks.calendarContextVisible('', false), false);
});

test('event, hold, and booking records expose the agent context trio', async () => {
  assert.deepEqual(calendarEventContext({ event: { id: 'evt_1_occ_3', title: 'Customer call' } }), {
    'data-context-record-id': 'evt_1',
    'data-context-record-type': 'calendar_event',
    'data-context-label': 'Customer call',
  });

  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  assert.match(js, /data-context-record-type="calendar_booking_hold"/);
  assert.match(js, /data-context-record-type="calendar_booking"/);
  assert.match(js, /calendar-context-card" data-context-record-id=/);
});

test('selecting a booking page is an in-place class flip, never a list rebuild', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');

  const markFn = js.match(/function markActiveBookingPage\(\)\s*\{[\s\S]*?\n\}/);
  assert.ok(markFn, 'markActiveBookingPage present');
  assert.match(markFn[0], /classList\.toggle\('is-selected'/);
  assert.match(markFn[0], /aria-pressed/);

  const selectFn = js.match(/function selectBookingPage\(id\)\s*\{[\s\S]*?\n\}/);
  assert.ok(selectFn, 'selectBookingPage present');
  assert.match(selectFn[0], /markActiveBookingPage\(\)/);
  // A rebuild (renderLeftList) would reset the operator's scroll — selection
  // must not trigger it.
  assert.doesNotMatch(selectFn[0], /renderLeftList\(/);
});

test('calendar does not advertise an unavailable public booking route', async () => {
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  assert.doesNotMatch(js, /window\.location\.origin\}\/book\//);
  assert.doesNotMatch(js, /href="\$\{publicUrl\}"/);
  assert.match(js, /Nicht veröffentlicht/);
});

test('v4 inspector drawer is contained by the calendar app and icons have a fallback', async () => {
  const [css, js] = await Promise.all([
    readFile(new URL('./index.css', import.meta.url), 'utf8'),
    readFile(new URL('./index.js', import.meta.url), 'utf8'),
  ]);
  // v4: Shell-Vorfahr entfällt, die Regel liegt unter dem App-Scope (scope-css.mjs).
  assert.match(css, /\[data-app="calendar"\] \.calendar-app \{ position: relative;[\s\S]*overflow: hidden;/);
  assert.match(js, /function actionIcon\(name\) \{[\s\S]*const paths = \{/);
  assert.doesNotMatch(js, /return state\.ctx\?\.getActionIcon\?\.\(name\) \|\| ''/);
});
