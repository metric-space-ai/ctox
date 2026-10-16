/**
 * Adapter for vkurko/EventCalendar UI engine.
 * Decouples Business-OS schema and EventCalendar internals.
 * Supports Recurrence Rule expansion using rrule.js.
 */

// Neutrale Ersatzfarbe, wenn keine Fensterpalette lesbar ist. Bewusst kein
// Host-Blau: Interaktions- und Akzentfarben gehoeren der Shell (Icon-Palette).
const NEUTRAL_ITEM_COLOR = '#8a8f98';

function hexColorToken(value) {
  const raw = String(value || '').trim().toLowerCase();
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/.exec(raw);
  if (short) return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`;
  return /^#[0-9a-f]{6}$/.test(raw) ? raw : '';
}

// Lesbare Schrift auf der Terminfarbe: dunkel auf hellen, weiß auf satten Farben (WCAG-Leuchtdichte).
// Vorher erbte der Termin die Fließtextfarbe (--ec-event-text-color zeigte auf das nirgends definierte
// --accent-contrast) – dunkle Schrift auf blauem/grünem Grund war kaum lesbar.
export function eventTextColor(color) {
  const hex = hexColorToken(color);
  if (!hex) return undefined;
  const channel = (i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const luminance = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
  // Kontrast gegen Weiß vs. gegen #111827 (L≈0.0136): der höhere gewinnt.
  return (1.05 / (luminance + 0.05)) >= ((luminance + 0.05) / 0.0636) ? '#ffffff' : '#111827';
}

function paletteAccentColor(element) {
  if (!element || typeof globalThis.getComputedStyle !== 'function') return NEUTRAL_ITEM_COLOR;
  try {
    const raw = globalThis.getComputedStyle(element).getPropertyValue('--accent');
    return hexColorToken(raw) || NEUTRAL_ITEM_COLOR;
  } catch {
    return NEUTRAL_ITEM_COLOR;
  }
}

export function createCalendarView({
  root,
  events = [],
  calendars = [],
  view = 'resourceTimeGridWeek', // 'dayGridMonth', 'resourceTimeGridWeek', 'resourceTimeGridDay', 'listWeek'
  onEventClick,
  onEventMove,
  onRangeSelect,
  onEventResize,
  resources = []
}) {
  const EventCalendarApi = window.EventCalendar;
  const createCalendar = typeof EventCalendarApi === 'function'
    ? (target, options) => new EventCalendarApi(target, options)
    : EventCalendarApi?.create;
  const destroyCalendar = EventCalendarApi?.destroy;

  if (typeof createCalendar !== 'function') {
    console.error('EventCalendar is not loaded on window.');
    return null;
  }

  // Pre-process and expand events (including recurrence rules)
  const viewStart = new Date();
  viewStart.setDate(viewStart.getDate() - 60); // expand -60 days
  const viewEnd = new Date();
  viewEnd.setDate(viewEnd.getDate() + 90);    // expand +90 days

  // Direktive 31.08.: kein Host-Blau-Hex im Modulcode. Die Ersatzfarbe fuer
  // Termine ohne eigene und ohne Kalenderfarbe kommt aus der Fensterpalette
  // (--accent am Modul-Host), Fallback neutral.
  const fallbackEventColor = paletteAccentColor(root);

  const formattedEvents = prepareEventsForCalendar(events, calendars, viewStart, viewEnd, fallbackEventColor);

  // Initialize EventCalendar
  const options = {
    view: view,
    buttonText: {
      today: 'Heute',
      month: 'Monat',
      week: 'Woche',
      day: 'Tag',
      list: 'Liste'
    },
    headerToolbar: {
      start: '',
      center: '',
      // Ansichtswechsel nur über die Reiter Monat/Woche/Tag der App (Textdiät Regel 3: keine Doppelbedienung).
      end: ''
    },
    locale: 'de',
    firstDay: 1, // Monday
    events: formattedEvents,
    resources: resources,
    eventDidMount: (info) => {
      if (!info?.el?.setAttribute) return;
      for (const [name, value] of Object.entries(calendarEventContext(info))) {
        info.el.setAttribute(name, value);
      }
      // Monatsraster: ein Termin = eine Zeile, Abschneiden ist dort fachlich gewollt (feste Tageszelle); der volle
      // Titel steht im Tooltip. Woche/Tag/Liste brechen den Titel um (index.css).
      if (info.view?.type === 'dayGridMonth') {
        info.el.setAttribute('data-v4-allow-ellipsis', '');
        if (info.event?.title) info.el.setAttribute('title', String(info.event.title));
      }
    },
    moreLinkContent: (arg) => `+${arg?.num ?? ''} weitere`,
    editable: true,
    selectable: true,
    slotMinTime: '00:00:00',
    slotMaxTime: '24:00:00',
    dayMaxEvents: true,
    eventClick: (info) => {
      if (onEventClick) {
        const original = resolveOriginalEventForCalendarClick(info, events);
        onEventClick({
          event: info.event,
          original: original || {
            id: getCalendarEventId(info) || '',
            title: info.event?.title || '',
            start_time: info.event?.start?.getTime?.() || Date.now(),
            end_time: info.event?.end?.getTime?.() || Date.now() + 60 * 60 * 1000,
            all_day: !!info.event?.allDay
          }
        });
      }
    },
    eventDrop: (info) => {
      if (onEventMove) {
        const originalId = getOriginalEventId(info.event.id);
        onEventMove({
          id: originalId,
          start: info.event.start,
          end: info.event.end,
          allDay: info.event.allDay
        });
      }
    },
    eventResize: (info) => {
      if (onEventResize) {
        const originalId = getOriginalEventId(info.event.id);
        onEventResize({
          id: originalId,
          start: info.event.start,
          end: info.event.end
        });
      }
    },
    select: (info) => {
      if (onRangeSelect) {
        onRangeSelect({
          start: info.start,
          end: info.end,
          allDay: info.allDay
        });
      }
    }
  };

  let ec = createCalendar(root, options);
  const recreate = () => {
    if (typeof ec?.destroy === 'function') {
      ec.destroy();
    } else if (typeof destroyCalendar === 'function') {
      destroyCalendar(ec);
    }
    ec = createCalendar(root, options);
  };

  return {
    destroy: () => {
      if (typeof ec?.destroy === 'function') {
        ec.destroy();
      } else if (typeof destroyCalendar === 'function') {
        destroyCalendar(ec);
      }
    },
    setEvents: (newEvents, newCalendars) => {
      const formatted = prepareEventsForCalendar(
        newEvents,
        newCalendars,
        viewStart,
        viewEnd,
        paletteAccentColor(root),
      );
      options.events = formatted;
      if (typeof ec?.setOption === 'function') {
        ec.setOption('events', formatted);
      } else {
        recreate();
      }
    },
    setView: (newView) => {
      options.view = newView;
      if (typeof ec?.setOption === 'function') {
        ec.setOption('view', newView);
      } else {
        recreate();
      }
    },
    next: () => ec?.next?.(),
    prev: () => ec?.prev?.(),
    today: () => ec?.today?.()
  };
}

/**
 * Maps CTOX Sync Engine events to EventCalendar standard structure and expands recurring series using rrule.js.
 */
function prepareEventsForCalendar(
  events,
  calendars,
  rangeStart,
  rangeEnd,
  fallbackColor = NEUTRAL_ITEM_COLOR,
) {
  const result = [];
  const calendarMap = new Map(calendars.map(c => [c.id, c]));
  const fallback = hexColorToken(fallbackColor) || NEUTRAL_ITEM_COLOR;

  for (const ev of events) {
    const cal = calendarMap.get(ev.calendar_id);
    const color = ev.color || (cal ? cal.color : fallback);
    const visibility = cal ? cal.visibility : true;
    if (visibility === false) continue; // skip hidden calendars

    if (ev.recurrence_rule) {
      // Expand recurring event
      try {
        const RRuleClass = window.RRule || (window.rrule && window.rrule.RRule);
        if (!RRuleClass) {
          console.warn('rrule.js is not loaded. Cannot expand recurrence: ', ev.recurrence_rule);
          // fall back to single instance
          result.push(mapSingleEvent(ev, color));
          continue;
        }

        // Parse RRULE
        // RRule expects DTSTART as a JS Date
        const ruleOptions = RRuleClass.parseString(ev.recurrence_rule);
        ruleOptions.dtstart = new Date(ev.start_time);
        const rule = new RRuleClass(ruleOptions);

        // Find occurrences in view range
        const occurrences = rule.between(rangeStart, rangeEnd, true);
        const duration = ev.end_time - ev.start_time;

        occurrences.forEach((occ, idx) => {
          const occStart = occ.getTime();
          const occEnd = occStart + duration;

          // Check if this occurrence was excluded (exdates)
          if (ev.recurrence_exdates && ev.recurrence_exdates.includes(occStart)) {
            return;
          }

          result.push({
            id: `${ev.id}_occ_${idx}`, // virtual ID for occurrences
            title: ev.title,
            start: occ,
            end: new Date(occEnd),
            allDay: !!ev.all_day,
            color: color,
            textColor: eventTextColor(color),
            description: ev.description || '',
            location: ev.location || '',
            sourceEventId: ev.id,
            extendedProps: {
              sourceEventId: ev.id
            },
            editable: true
          });
        });
      } catch (err) {
        console.error('Error expanding recurrence rule:', err, ev.recurrence_rule);
        result.push(mapSingleEvent(ev, color));
      }
    } else {
      // Non-recurring event
      result.push(mapSingleEvent(ev, color));
    }
  }

  return result;
}

function mapSingleEvent(ev, color) {
  return {
    id: ev.id,
    title: ev.title,
    start: new Date(ev.start_time),
    end: new Date(ev.end_time),
    allDay: !!ev.all_day,
    color: color,
    textColor: eventTextColor(color),
    description: ev.description || '',
    location: ev.location || '',
    sourceEventId: ev.id,
    extendedProps: {
      sourceEventId: ev.id
    },
    editable: true
  };
}

function getOriginalEventId(eventCalendarId) {
  return String(eventCalendarId || '').replace(/_occ_\d+$/, '');
}

export function calendarEventContext(info = {}) {
  const id = getOriginalEventId(getCalendarEventId(info));
  const label = String(info?.event?.title || id || 'Calendar event');
  return {
    'data-context-record-id': id,
    'data-context-record-type': 'calendar_event',
    'data-context-label': label,
  };
}

function getCalendarEventId(info) {
  return info?.event?.id
    || info?.event?._def?.publicId
    || info?.event?.sourceEventId
    || info?.event?.extendedProps?.sourceEventId
    || info?.event?.extendedProps?.originalId
    || info?.event?.extendedProps?.id
    || '';
}

function resolveOriginalEventForCalendarClick(info, events) {
  const eventId = getCalendarEventId(info);
  const originalId = getOriginalEventId(eventId);
  if (originalId) {
    const directMatch = events.find(e => e.id === originalId);
    if (directMatch) return directMatch;
  }

  const title = info?.event?.title || '';
  const startMs = info?.event?.start?.getTime?.();
  const endMs = info?.event?.end?.getTime?.();
  if (!title) return null;

  const exactTimeMatch = events.find(e => (
    e.title === title
    && Number(e.start_time) === startMs
    && Number(e.end_time) === endMs
  ));
  if (exactTimeMatch) return exactTimeMatch;

  const sameTitle = events.filter(e => e.title === title);
  return sameTitle.length === 1 ? sameTitle[0] : null;
}

export const __calendarViewAdapterTestHooks = {
  getOriginalEventId,
  paletteAccentColor,
  prepareEventsForCalendar,
  resolveOriginalEventForCalendarClick,
};
