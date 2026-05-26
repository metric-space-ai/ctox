/**
 * Adapter for vkurko/EventCalendar UI engine.
 * Decouples Business-OS schema and EventCalendar internals.
 * Supports Recurrence Rule expansion using rrule.js.
 */
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

  const formattedEvents = prepareEventsForCalendar(events, calendars, viewStart, viewEnd);

  // Initialize EventCalendar
  const options = {
    view: view,
    headerToolbar: {
      start: 'prev,next today',
      center: 'title',
      end: 'dayGridMonth,timeGridWeek,timeGridDay,listWeek'
    },
    buttonText: {
      today: 'Heute',
      month: 'Monat',
      week: 'Woche',
      day: 'Tag',
      list: 'Liste'
    },
    locale: 'de',
    firstDay: 1, // Monday
    events: formattedEvents,
    resources: resources,
    editable: true,
    selectable: true,
    slotMinTime: '00:00:00',
    slotMaxTime: '24:00:00',
    dayMaxEvents: true,
    eventClick: (info) => {
      if (onEventClick) {
        // Find original event
        const originalId = info.event.id.split('_')[0]; // handle virtual recurring IDs
        const original = events.find(e => e.id === originalId);
        onEventClick({
          event: info.event,
          original: original || {
            id: info.event.id,
            title: info.event.title,
            start_time: info.event.start.getTime(),
            end_time: info.event.end.getTime(),
            all_day: info.event.allDay
          }
        });
      }
    },
    eventDrop: (info) => {
      if (onEventMove) {
        const originalId = info.event.id.split('_')[0];
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
        const originalId = info.event.id.split('_')[0];
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
      const formatted = prepareEventsForCalendar(newEvents, newCalendars, viewStart, viewEnd);
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
 * Maps CTOX DB events to EventCalendar standard structure and expands recurring series using rrule.js.
 */
function prepareEventsForCalendar(events, calendars, rangeStart, rangeEnd) {
  const result = [];
  const calendarMap = new Map(calendars.map(c => [c.id, c]));

  for (const ev of events) {
    const cal = calendarMap.get(ev.calendar_id);
    const color = ev.color || (cal ? cal.color : '#3b82f6');
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
            description: ev.description || '',
            location: ev.location || '',
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
    description: ev.description || '',
    location: ev.location || '',
    editable: true
  };
}
