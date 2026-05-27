import assert from 'node:assert/strict';
import { __calendarTestHooks as hooks } from './index.js';
import { __calendarViewAdapterTestHooks as adapterHooks } from './calendar-view-adapter.js';

const tests = [];
function test(name, fn) {
  tests.push({ name, fn });
}

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

console.log(`${passed} calendar tests passed`);
