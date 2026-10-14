/** Public Workjet booking surface. Talks only to the calendar intake control plane. */

const locale = document.documentElement.lang === 'de' ? 'de-DE' : 'en-US';
const language = locale.startsWith('de') ? 'de' : 'en';

const COPY = Object.freeze({
  de: Object.freeze({
    pageTitle: 'Termin buchen | Workjet',
    poweredBy: 'Bereitgestellt mit',
    chooseDateTime: 'Datum und Uhrzeit wählen',
    previousMonth: 'Vorheriger Monat',
    nextMonth: 'Nächster Monat',
    calendar: 'Kalender',
    selectedDate: 'Ausgewähltes Datum',
    chooseDate: 'Wählen Sie zuerst ein Datum.',
    backToTimes: '← Zurück zur Zeitauswahl',
    enterDetails: 'Kontaktdaten eingeben',
    nameLabel: 'Name *',
    namePlaceholder: 'z. B. Max Mustermann',
    emailLabel: 'E-Mail-Adresse *',
    phoneLabel: 'Telefonnummer (optional)',
    notesLabel: 'Notizen (optional)',
    notesPlaceholder: 'Welche Themen möchten Sie besprechen?',
    confirmBooking: 'Termin verbindlich buchen',
    confirming: 'Termin wird gebucht …',
    bookingConfirmed: 'Termin bestätigt',
    confirmationSent: 'Der Termin wurde eingetragen. Eine Bestätigung mit den Details wurde per E-Mail versendet.',
    appointmentType: 'Termin:',
    when: 'Zeit:',
    where: 'Ort:',
    closeWindow: 'Sie können dieses Fenster jetzt schließen.',
    defaultDescription: 'Wählen Sie einen passenden Termin. Anschließend können Sie Ihre Kontaktdaten eingeben.',
    onlineAppointment: 'Online-Termin',
    phoneAppointment: 'Telefontermin',
    inPersonAppointment: 'Termin vor Ort',
    phoneCallback: 'Telefonischer Rückruf',
    minutes: (value) => `${value} Minuten`,
    durationShort: (value) => `${value} Min.`,
    loadingTimes: 'Freie Zeiten werden geladen …',
    noTimes: 'An diesem Tag sind keine freien Termine verfügbar.',
    invalidLink: 'Dieser Buchungslink ist ungültig.',
    pageUnavailable: 'Die Buchungsseite ist zurzeit nicht verfügbar.',
    timesUnavailable: 'Freie Zeiten konnten nicht geladen werden. Bitte versuchen Sie es erneut.',
    slotUnavailable: 'Dieser Termin ist nicht mehr verfügbar. Bitte wählen Sie einen anderen.',
    bookingFailed: 'Der Termin konnte nicht gebucht werden. Bitte versuchen Sie es erneut.',
    errorTitle: 'Buchung nicht möglich',
    weekdays: ['Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa', 'So'],
  }),
  en: Object.freeze({
    pageTitle: 'Book an appointment | Workjet',
    poweredBy: 'Provided with',
    chooseDateTime: 'Choose a date and time',
    previousMonth: 'Previous month',
    nextMonth: 'Next month',
    calendar: 'Calendar',
    selectedDate: 'Selected date',
    chooseDate: 'Choose a date first.',
    backToTimes: '← Back to available times',
    enterDetails: 'Enter your contact details',
    nameLabel: 'Name *',
    namePlaceholder: 'e.g. Alex Morgan',
    emailLabel: 'Email address *',
    phoneLabel: 'Phone number (optional)',
    notesLabel: 'Notes (optional)',
    notesPlaceholder: 'What would you like to discuss?',
    confirmBooking: 'Confirm appointment',
    confirming: 'Booking appointment …',
    bookingConfirmed: 'Appointment confirmed',
    confirmationSent: 'The appointment has been booked. A confirmation with the details was sent by email.',
    appointmentType: 'Appointment:',
    when: 'Time:',
    where: 'Location:',
    closeWindow: 'You can close this window now.',
    defaultDescription: 'Choose a suitable time. You can enter your contact details in the next step.',
    onlineAppointment: 'Online appointment',
    phoneAppointment: 'Phone appointment',
    inPersonAppointment: 'In-person appointment',
    phoneCallback: 'Phone callback',
    minutes: (value) => `${value} minutes`,
    durationShort: (value) => `${value} min`,
    loadingTimes: 'Loading available times …',
    noTimes: 'There are no available appointments on this day.',
    invalidLink: 'This booking link is invalid.',
    pageUnavailable: 'The booking page is currently unavailable.',
    timesUnavailable: 'Available times could not be loaded. Please try again.',
    slotUnavailable: 'This appointment is no longer available. Please choose another.',
    bookingFailed: 'The appointment could not be booked. Please try again.',
    errorTitle: 'Unable to book appointment',
    weekdays: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'],
  }),
});

const copy = COPY[language];

const state = {
  slug: '',
  bookingPage: null,
  currentDate: new Date(),
  selectedDate: null,
  slots: [],
  selectedSlot: null,
  activeHold: null,
};

const els = {
  card: document.getElementById('bookingMainCard'),
  status: document.getElementById('bookingStatus'),
  eventTitle: document.getElementById('eventTitle'),
  eventDuration: document.getElementById('eventDuration'),
  eventLocation: document.getElementById('eventLocation'),
  eventDescription: document.getElementById('eventDescription'),
  stepSlots: document.getElementById('stepSlots'),
  stepForm: document.getElementById('stepForm'),
  stepSuccess: document.getElementById('stepSuccess'),
  monthYearTitle: document.getElementById('monthYearTitle'),
  btnPrevMonth: document.getElementById('btnPrevMonth'),
  btnNextMonth: document.getElementById('btnNextMonth'),
  datepickerGrid: document.getElementById('datepickerGrid'),
  timeslotPane: document.getElementById('timeslotPane'),
  selectedDateTitle: document.getElementById('selectedDateTitle'),
  timeslotList: document.getElementById('timeslotList'),
  btnBackToSlots: document.getElementById('btnBackToSlots'),
  bookingAttendeeForm: document.getElementById('bookingAttendeeForm'),
  confirmButton: document.querySelector('.confirm-btn'),
  attendeeName: document.getElementById('attendeeName'),
  attendeeEmail: document.getElementById('attendeeEmail'),
  attendeePhone: document.getElementById('attendeePhone'),
  attendeeNotes: document.getElementById('attendeeNotes'),
  summaryTitle: document.getElementById('summaryTitle'),
  summaryTime: document.getElementById('summaryTime'),
  summaryLocationRow: document.getElementById('summaryLocationRow'),
  summaryLocation: document.getElementById('summaryLocation'),
};

document.addEventListener('DOMContentLoaded', () => {
  applyTranslations();
  const paths = window.location.pathname.split('/').filter(Boolean);
  state.slug = paths.at(-1) || '';

  if (!state.slug || state.slug === 'book') {
    renderErrorState(copy.invalidLink);
    return;
  }

  wireEvents();
  void loadBookingPageDetails();
});

function applyTranslations() {
  document.title = copy.pageTitle;
  for (const element of document.querySelectorAll('[data-i18n]')) {
    const value = copy[element.dataset.i18n];
    if (typeof value === 'string') element.textContent = value;
  }
  for (const element of document.querySelectorAll('[data-i18n-aria-label]')) {
    const value = copy[element.dataset.i18nAriaLabel];
    if (typeof value === 'string') element.setAttribute('aria-label', value);
  }
  for (const element of document.querySelectorAll('[data-i18n-placeholder]')) {
    const value = copy[element.dataset.i18nPlaceholder];
    if (typeof value === 'string') element.setAttribute('placeholder', value);
  }
  document.querySelectorAll('[data-weekday]').forEach((element) => {
    element.textContent = copy.weekdays[Number(element.dataset.weekday)] || '';
  });
}

function wireEvents() {
  els.btnPrevMonth.addEventListener('click', () => {
    state.currentDate = new Date(state.currentDate.getFullYear(), state.currentDate.getMonth() - 1, 1);
    renderDatePicker();
  });

  els.btnNextMonth.addEventListener('click', () => {
    state.currentDate = new Date(state.currentDate.getFullYear(), state.currentDate.getMonth() + 1, 1);
    renderDatePicker();
  });

  els.btnBackToSlots.addEventListener('click', () => {
    transitionToStep('slots');
    void releaseHold();
  });

  els.bookingAttendeeForm.addEventListener('submit', handleFormSubmit);
}

async function loadBookingPageDetails() {
  try {
    const response = await fetch(`/api/public/calendar/${encodeURIComponent(state.slug)}/slots?info_only=true`, {
      headers: { Accept: 'application/json' },
    });
    if (!response.ok) throw new Error('page-unavailable');
    const data = await response.json();
    if (!data?.booking_page) throw new Error('page-unavailable');
    state.bookingPage = data.booking_page;

    els.eventTitle.textContent = String(state.bookingPage.title || '');
    els.eventDuration.textContent = copy.minutes(Number(state.bookingPage.duration_minutes) || 0);
    els.eventLocation.textContent = locationLabel(state.bookingPage.location_mode);
    els.eventDescription.textContent = String(state.bookingPage.description || copy.defaultDescription);
    renderDatePicker();
  } catch {
    renderErrorState(copy.pageUnavailable);
  }
}

let slotsRetryTimer = 0;
let slotsRetryMs = 2000;
let slotsRequestId = 0;

async function loadSlotsForDate(date, attempt = 0) {
  if (slotsRetryTimer) {
    window.clearTimeout(slotsRetryTimer);
    slotsRetryTimer = 0;
  }
  const requestId = attempt === 0 ? (slotsRequestId += 1) : slotsRequestId;
  if (attempt === 0) {
    slotsRetryMs = 2000;
    replaceWithMessage(els.timeslotList, copy.loadingTimes, 'spinner');
  }
  const startOfDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const endOfDay = new Date(date.getFullYear(), date.getMonth(), date.getDate(), 23, 59, 59, 999);

  try {
    const response = await fetch(`/api/public/calendar/${encodeURIComponent(state.slug)}/slots?start=${startOfDay.getTime()}&end=${endOfDay.getTime()}`, {
      headers: { Accept: 'application/json' },
    });
    if (requestId !== slotsRequestId) return;
    if (!response.ok) throw new Error('times-unavailable');
    const data = await response.json();
    if (requestId !== slotsRequestId) return;
    state.slots = Array.isArray(data?.slots) ? data.slots : [];
    renderSlots();
  } catch {
    if (requestId !== slotsRequestId) return;
    const wait = slotsRetryMs;
    slotsRetryMs = Math.min(slotsRetryMs * 2, 60_000);
    if (attempt >= 1) replaceWithMessage(els.timeslotList, copy.timesUnavailable);
    slotsRetryTimer = window.setTimeout(() => {
      slotsRetryTimer = 0;
      if (requestId !== slotsRequestId) return;
      void loadSlotsForDate(date, attempt + 1);
    }, wait);
  }
}

async function reserveHold(slot) {
  clearStatus();
  try {
    const response = await fetch(`/api/public/calendar/${encodeURIComponent(state.slug)}/hold`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify({ slot_start_ms: slot.start_ms, slot_end_ms: slot.end_ms }),
    });
    if (!response.ok) throw new Error('slot-unavailable');
    const hold = await response.json();
    if (!hold?.id || !hold?.token) throw new Error('slot-unavailable');
    state.activeHold = { id: hold.id, token: hold.token };
    state.selectedSlot = slot;
    transitionToStep('form');
  } catch {
    showStatus(copy.slotUnavailable);
  }
}

async function releaseHold() {
  if (!state.activeHold) return;
  const hold = state.activeHold;
  state.activeHold = null;
  state.selectedSlot = null;
  try {
    await fetch(`/api/public/calendar/${encodeURIComponent(state.slug)}/hold`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify({ hold_id: hold.id, hold_token: hold.token }),
    });
  } catch {
    // Expiring holds are cleaned up server-side. No secret or transport detail is exposed.
  }
}

async function handleFormSubmit(event) {
  event.preventDefault();
  if (!state.activeHold || !state.bookingPage) return;
  clearStatus();
  setSubmitting(true);

  const attendee = {
    name: els.attendeeName.value.trim(),
    email: els.attendeeEmail.value.trim(),
    phone: els.attendeePhone.value.trim(),
    notes: els.attendeeNotes.value.trim(),
  };

  try {
    const response = await fetch(`/api/public/calendar/${encodeURIComponent(state.slug)}/book`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify({
        hold_id: state.activeHold.id,
        hold_token: state.activeHold.token,
        attendee_name: attendee.name,
        attendee_email: attendee.email,
        attendee_phone: attendee.phone,
        answers: { notes: attendee.notes },
      }),
    });
    if (!response.ok) throw new Error('booking-failed');
    const booking = await response.json();
    if (!Number.isFinite(Number(booking?.slot_start_ms))) throw new Error('booking-failed');

    els.summaryTitle.textContent = String(state.bookingPage.title || '');
    els.summaryTime.textContent = `${new Date(Number(booking.slot_start_ms)).toLocaleString(locale)} (${copy.durationShort(Number(state.bookingPage.duration_minutes) || 0)})`;

    const locationMode = state.bookingPage.location_mode;
    if (locationMode === 'link') {
      els.summaryLocation.textContent = copy.onlineAppointment;
      els.summaryLocationRow.hidden = false;
    } else if (locationMode === 'phone') {
      els.summaryLocation.textContent = attendee.phone || copy.phoneCallback;
      els.summaryLocationRow.hidden = false;
    } else {
      els.summaryLocationRow.hidden = true;
    }

    state.activeHold = null;
    transitionToStep('success');
  } catch {
    showStatus(copy.bookingFailed);
  } finally {
    setSubmitting(false);
  }
}

function renderDatePicker() {
  const year = state.currentDate.getFullYear();
  const month = state.currentDate.getMonth();
  els.monthYearTitle.textContent = new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric' }).format(new Date(year, month, 1));

  let firstDayIndex = new Date(year, month, 1).getDay();
  firstDayIndex = firstDayIndex === 0 ? 6 : firstDayIndex - 1;
  const totalDays = new Date(year, month + 1, 0).getDate();
  els.datepickerGrid.replaceChildren();

  for (let index = 0; index < firstDayIndex; index += 1) {
    const emptyCell = document.createElement('span');
    emptyCell.setAttribute('aria-hidden', 'true');
    els.datepickerGrid.append(emptyCell);
  }

  const today = new Date();
  const comparisonDate = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const dateLabelFormatter = new Intl.DateTimeFormat(locale, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });

  for (let day = 1; day <= totalDays; day += 1) {
    const cellDate = new Date(year, month, day);
    const cellButton = document.createElement('button');
    cellButton.type = 'button';
    cellButton.className = 'date-cell';
    cellButton.textContent = String(day);
    cellButton.setAttribute('aria-label', dateLabelFormatter.format(cellDate));

    if (sameDate(cellDate, today)) cellButton.classList.add('today-dot');
    if (state.selectedDate && sameDate(cellDate, state.selectedDate)) {
      cellButton.classList.add('active');
      cellButton.setAttribute('aria-pressed', 'true');
    } else {
      cellButton.setAttribute('aria-pressed', 'false');
    }

    if (cellDate < comparisonDate) {
      cellButton.disabled = true;
    } else {
      cellButton.addEventListener('click', () => selectDate(cellDate, cellButton));
    }
    els.datepickerGrid.append(cellButton);
  }
}

function selectDate(cellDate, cellButton) {
  clearStatus();
  for (const active of els.datepickerGrid.querySelectorAll('.date-cell.active')) {
    active.classList.remove('active');
    active.setAttribute('aria-pressed', 'false');
  }
  cellButton.classList.add('active');
  cellButton.setAttribute('aria-pressed', 'true');
  state.selectedDate = cellDate;
  els.selectedDateTitle.textContent = new Intl.DateTimeFormat(locale, { weekday: 'long', day: '2-digit', month: 'long' }).format(cellDate);
  void loadSlotsForDate(cellDate);
}

function renderSlots() {
  els.timeslotList.replaceChildren();
  if (state.slots.length === 0) {
    replaceWithMessage(els.timeslotList, copy.noTimes);
    return;
  }

  const timeFormatter = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' });
  for (const slot of state.slots) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'timeslot-btn';
    button.textContent = timeFormatter.format(new Date(slot.start_ms));
    button.addEventListener('click', () => void reserveHold(slot));
    els.timeslotList.append(button);
  }
}

function transitionToStep(step) {
  clearStatus();
  els.stepSlots.classList.toggle('hidden', step !== 'slots');
  els.stepForm.classList.toggle('hidden', step !== 'form');
  els.stepSuccess.classList.toggle('hidden', step !== 'success');

  if (step === 'form') els.attendeeName.focus();
  if (step === 'success') els.stepSuccess.focus();
  if (step === 'slots') els.selectedDateTitle.focus?.();
}

function renderErrorState(message) {
  const wrapper = document.createElement('section');
  wrapper.className = 'error-state';
  wrapper.setAttribute('role', 'alert');

  const title = document.createElement('h1');
  title.textContent = copy.errorTitle;
  const description = document.createElement('p');
  description.textContent = String(message);
  const footer = document.createElement('div');
  footer.className = 'error-state-footer';
  footer.textContent = `Workjet`;

  wrapper.append(title, description, footer);
  els.card.replaceChildren(wrapper);
}

function replaceWithMessage(container, message, extraClass = '') {
  const element = document.createElement('div');
  element.className = ['timeslot-empty-state', extraClass].filter(Boolean).join(' ');
  element.textContent = message;
  container.replaceChildren(element);
}

function showStatus(message) {
  els.status.textContent = message;
  els.status.classList.remove('hidden');
  els.status.focus?.();
}

function clearStatus() {
  els.status.textContent = '';
  els.status.classList.add('hidden');
}

function setSubmitting(submitting) {
  els.confirmButton.disabled = submitting;
  els.confirmButton.textContent = submitting ? copy.confirming : copy.confirmBooking;
}

function locationLabel(mode) {
  if (mode === 'link') return copy.onlineAppointment;
  if (mode === 'phone') return copy.phoneAppointment;
  return copy.inPersonAppointment;
}

function sameDate(left, right) {
  return left.getFullYear() === right.getFullYear()
    && left.getMonth() === right.getMonth()
    && left.getDate() === right.getDate();
}
