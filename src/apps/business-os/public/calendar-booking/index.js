/**
 * Public Booking Funnel client script.
 * Talks to Rust HTTP intake gateway.
 */

const state = {
  slug: '',
  bookingPage: null,

  // Date Picker State
  currentDate: new Date(),
  selectedDate: null,

  // Slots State
  slots: [],
  selectedSlot: null,
  activeHold: null // { id, token }
};

const els = {
  eventTitle: document.getElementById('eventTitle'),
  eventDuration: document.getElementById('eventDuration'),
  eventLocation: document.getElementById('eventLocation'),
  eventDescription: document.getElementById('eventDescription'),

  // Steps Panels
  stepSlots: document.getElementById('stepSlots'),
  stepForm: document.getElementById('stepForm'),
  stepSuccess: document.getElementById('stepSuccess'),

  // Datepicker Elements
  monthYearTitle: document.getElementById('monthYearTitle'),
  btnPrevMonth: document.getElementById('btnPrevMonth'),
  btnNextMonth: document.getElementById('btnNextMonth'),
  datepickerGrid: document.getElementById('datepickerGrid'),

  // Timeslots Elements
  timeslotPane: document.getElementById('timeslotPane'),
  selectedDateTitle: document.getElementById('selectedDateTitle'),
  timeslotList: document.getElementById('timeslotList'),

  // Form Elements
  btnBackToSlots: document.getElementById('btnBackToSlots'),
  bookingAttendeeForm: document.getElementById('bookingAttendeeForm'),
  attendeeName: document.getElementById('attendeeName'),
  attendeeEmail: document.getElementById('attendeeEmail'),
  attendeePhone: document.getElementById('attendeePhone'),
  attendeeNotes: document.getElementById('attendeeNotes'),

  // Success Summary
  summaryTitle: document.getElementById('summaryTitle'),
  summaryTime: document.getElementById('summaryTime'),
  summaryLocationRow: document.getElementById('summaryLocationRow'),
  summaryLocation: document.getElementById('summaryLocation')
};

// ----------------------------------------------------
// INITIALIZATION
// ----------------------------------------------------

document.addEventListener('DOMContentLoaded', () => {
  // Parse slug from URL: /book/:slug
  const paths = window.location.pathname.split('/').filter(Boolean);
  state.slug = paths.pop() || '';

  if (!state.slug || state.slug === 'book') {
    renderErrorState('Ungültiger Buchungs-Link.');
    return;
  }

  loadBookingPageDetails();
  wireEvents();
});

function wireEvents() {
  els.btnPrevMonth.addEventListener('click', () => {
    state.currentDate.setMonth(state.currentDate.getMonth() - 1);
    renderDatePicker();
  });

  els.btnNextMonth.addEventListener('click', () => {
    state.currentDate.setMonth(state.currentDate.getMonth() + 1);
    renderDatePicker();
  });

  els.btnBackToSlots.addEventListener('click', () => {
    transitionToStep('slots');
    // Release hold if back is clicked
    releaseHold();
  });

  els.bookingAttendeeForm.addEventListener('submit', handleFormSubmit);
}

// ----------------------------------------------------
// API REQUESTS
// ----------------------------------------------------

async function loadBookingPageDetails() {
  try {
    const response = await fetch(`/api/public/calendar/${state.slug}/slots?info_only=true`);
    if (!response.ok) {
      throw new Error('Buchungsseite nicht gefunden.');
    }
    const data = await response.json();
    state.bookingPage = data.booking_page;

    // Set titles in HTML
    els.eventTitle.textContent = state.bookingPage.title;
    els.eventDuration.textContent = `${state.bookingPage.duration_minutes} Minuten`;

    const locMode = state.bookingPage.location_mode;
    els.eventLocation.textContent = locMode === 'link' ? 'Online-Meeting' : (locMode === 'phone' ? 'Telefontermin' : 'Physisches Treffen');
    els.eventDescription.textContent = state.bookingPage.description || 'Laden Sie sich ein und wählen Sie ein passendes Zeitfenster.';

    renderDatePicker();
  } catch (error) {
    console.error(error);
    renderErrorState(error.message);
  }
}

async function loadSlotsForDate(date) {
  els.timeslotList.innerHTML = '<div style="text-align:center; padding: 20px;"><span class="spinner">Lade Slots...</span></div>';

  const startOfDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const endOfDay = new Date(date.getFullYear(), date.getMonth(), date.getDate(), 23, 59, 59, 999);

  try {
    const response = await fetch(`/api/public/calendar/${state.slug}/slots?start=${startOfDay.getTime()}&end=${endOfDay.getTime()}`);
    if (!response.ok) throw new Error('Fehler beim Laden der Slots.');
    const data = await response.json();
    state.slots = data.slots || [];

    renderSlots();
  } catch (error) {
    console.error(error);
    els.timeslotList.innerHTML = `<div class="timeslot-empty-state" style="color:#ef4444;">${error.message}</div>`;
  }
}

async function reserveHold(slot) {
  try {
    const response = await fetch(`/api/public/calendar/${state.slug}/hold`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        slot_start_ms: slot.start_ms,
        slot_end_ms: slot.end_ms
      })
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Dieser Slot ist leider nicht mehr verfügbar.');
    }

    const hold = await response.json();
    state.activeHold = {
      id: hold.id,
      token: hold.token
    };
    state.selectedSlot = slot;

    transitionToStep('form');
  } catch (error) {
    alert(error.message);
  }
}

async function releaseHold() {
  if (!state.activeHold) return;

  // Background fire-and-forget release call
  fetch(`/api/public/calendar/${state.slug}/hold`, {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      hold_id: state.activeHold.id,
      hold_token: state.activeHold.token
    })
  }).catch(e => console.warn('Failed to release hold:', e));

  state.activeHold = null;
  state.selectedSlot = null;
}

async function handleFormSubmit(e) {
  e.preventDefault();
  if (!state.activeHold) return;

  const attendee = {
    name: els.attendeeName.value,
    email: els.attendeeEmail.value,
    phone: els.attendeePhone.value,
    notes: els.attendeeNotes.value
  };

  try {
    const response = await fetch(`/api/public/calendar/${state.slug}/book`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        hold_id: state.activeHold.id,
        hold_token: state.activeHold.token,
        attendee_name: attendee.name,
        attendee_email: attendee.email,
        attendee_phone: attendee.phone,
        answers: { notes: attendee.notes }
      })
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Fehler beim Bestätigen der Buchung.');
    }

    const booking = await response.json();

    // Set success details
    els.summaryTitle.textContent = state.bookingPage.title;
    els.summaryTime.textContent = `${new Date(booking.slot_start_ms).toLocaleString()} (${state.bookingPage.duration_minutes} Min)`;

    const locMode = state.bookingPage.location_mode;
    if (locMode === 'link') {
      els.summaryLocation.textContent = 'Jami Online Meeting';
      els.summaryLocationRow.style.display = 'flex';
    } else if (locMode === 'phone') {
      els.summaryLocation.textContent = attendee.phone || 'Telefonrückruf';
      els.summaryLocationRow.style.display = 'flex';
    } else {
      els.summaryLocationRow.style.display = 'none';
    }

    transitionToStep('success');
  } catch (error) {
    alert(error.message);
  }
}

// ----------------------------------------------------
// UI RENDERING & DATE UTILS
// ----------------------------------------------------

function renderDatePicker() {
  const year = state.currentDate.getFullYear();
  const month = state.currentDate.getMonth();

  const monthNames = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
  els.monthYearTitle.textContent = `${monthNames[month]} ${year}`;

  // First day of month (0 = Sunday, 1 = Monday, etc.)
  let firstDayIndex = new Date(year, month, 1).getDay();
  // Adjust so Monday is 0, Sunday is 6
  firstDayIndex = firstDayIndex === 0 ? 6 : firstDayIndex - 1;

  const totalDays = new Date(year, month + 1, 0).getDate();

  els.datepickerGrid.innerHTML = '';

  // Render empty padding cells for previous month
  for (let i = 0; i < firstDayIndex; i++) {
    const emptyCell = document.createElement('div');
    els.datepickerGrid.appendChild(emptyCell);
  }

  const today = new Date();

  // Render day cells
  for (let day = 1; day <= totalDays; day++) {
    const cellDate = new Date(year, month, day);
    const cellBtn = document.createElement('button');
    cellBtn.type = 'button';
    cellBtn.className = 'date-cell';
    cellBtn.textContent = day;

    // Highlight today
    if (cellDate.toDateString() === today.toDateString()) {
      cellBtn.classList.add('today-dot');
    }

    // Highlight selected
    if (state.selectedDate && cellDate.toDateString() === state.selectedDate.toDateString()) {
      cellBtn.classList.add('active');
    }

    // Disable past days
    const comparisonDate = new Date(today.getFullYear(), today.getMonth(), today.getDate());
    if (cellDate < comparisonDate) {
      cellBtn.disabled = true;
    } else {
      cellBtn.addEventListener('click', () => {
        // Toggle selected state in UI
        const active = els.datepickerGrid.querySelector('.date-cell.active');
        if (active) active.classList.remove('active');
        cellBtn.classList.add('active');

        state.selectedDate = cellDate;

        // Show timeslots pane
        els.timeslotPane.style.display = 'flex';
        els.selectedDateTitle.textContent = cellDate.toLocaleDateString('de-DE', { weekday: 'long', day: '2-digit', month: 'long' });

        loadSlotsForDate(cellDate);
      });
    }

    els.datepickerGrid.appendChild(cellBtn);
  }
}

function renderSlots() {
  els.timeslotList.innerHTML = '';

  if (state.slots.length === 0) {
    els.timeslotList.innerHTML = '<div class="timeslot-empty-state">Keine freien Termine für diesen Tag.</div>';
    return;
  }

  state.slots.forEach(slot => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'timeslot-btn';

    const timeStr = new Date(slot.start_ms).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' });
    btn.textContent = timeStr;

    btn.addEventListener('click', () => {
      // Hold slot and continue
      reserveHold(slot);
    });

    els.timeslotList.appendChild(btn);
  });
}

function transitionToStep(step) {
  els.stepSlots.classList.add('hidden');
  els.stepForm.classList.add('hidden');
  els.stepSuccess.classList.add('hidden');

  if (step === 'slots') {
    els.stepSlots.classList.remove('hidden');
  } else if (step === 'form') {
    els.stepForm.classList.remove('hidden');
    els.attendeeName.focus();
  } else if (step === 'success') {
    els.stepSuccess.classList.remove('hidden');
  }
}

function renderErrorState(message) {
  const card = document.getElementById('bookingMainCard');
  card.innerHTML = `
    <div style="padding: 60px; text-align: center; width: 100%;">
      <h1 style="color: #ef4444; font-family:'Outfit', sans-serif; font-size:24px; margin-bottom:16px;">Hoppla! ⚠️</h1>
      <p style="color: var(--text); font-size: 14px; margin-bottom: 24px;">${message}</p>
      <div style="font-size: 12px; color: var(--muted);">Powered by Business OS</div>
    </div>
  `;
}
