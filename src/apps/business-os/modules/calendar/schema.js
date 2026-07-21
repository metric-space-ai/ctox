const calendarSourceSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    kind: { type: 'string' }, // "local" | "google" | "outlook" | "ics" | "booking"
    title: { type: 'string' },
    color: { type: 'string' },
    sync_status: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'kind', 'title', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const calendarCalendarSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    source_id: { type: 'string' },
    title: { type: 'string' },
    color: { type: 'string' },
    visibility: { type: 'boolean' },
    owner_user_id: { type: 'string' },
    timezone: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'source_id', 'title', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const calendarEventSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    calendar_id: { type: 'string' },
    title: { type: 'string' },
    description: { type: 'string' },
    location: { type: 'string' },
    start_time: { type: 'number' }, // ms timestamp
    end_time: { type: 'number' },   // ms timestamp
    timezone: { type: 'string' },
    all_day: { type: 'boolean' },
    recurrence_rule: { type: 'string' },
    recurrence_exdates: { type: 'array', items: { type: 'number' } },
    status: { type: 'string' },
    attendees: { type: 'array', items: { type: 'object', additionalProperties: true } },
    meeting_url: { type: 'string' },
    booking_id: { type: 'string' },
    source_ref: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'calendar_id', 'title', 'start_time', 'end_time', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const calendarEventInstanceSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    event_id: { type: 'string' },
    start_time: { type: 'number' },
    end_time: { type: 'number' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'event_id', 'start_time', 'end_time', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const calendarBookingPageSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    slug: { type: 'string' },
    title: { type: 'string' },
    description: { type: 'string' },
    duration_minutes: { type: 'number' },
    buffer_before_minutes: { type: 'number' },
    buffer_after_minutes: { type: 'number' },
    min_notice_minutes: { type: 'number' },
    max_days_ahead: { type: 'number' },
    calendar_ids: { type: 'array', items: { type: 'string' } },
    host_user_ids: { type: 'array', items: { type: 'string' } },
    location_mode: { type: 'string' }, // "link" | "phone" | "physical"
    public_token_hash: { type: 'string' },
    status: { type: 'string' }, // "active" | "inactive"
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'slug', 'title', 'duration_minutes', 'status', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const calendarAvailabilityRuleSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    booking_page_id: { type: 'string' },
    weekday: { type: 'number' }, // 0=Sunday, 1=Monday, etc.
    start_minute: { type: 'number' }, // minute of day e.g. 540 for 09:00
    end_minute: { type: 'number' },   // minute of day e.g. 1020 for 17:00
    timezone: { type: 'string' },
    valid_from: { type: 'number' },
    valid_until: { type: 'number' },
    capacity: { type: 'number' },
    status: { type: 'string' } // "active" | "inactive"
  },
  required: ['id', 'booking_page_id', 'weekday', 'start_minute', 'end_minute', 'status'],
  additionalProperties: true
};

const calendarBookingHoldSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    booking_page_id: { type: 'string' },
    slot_start_ms: { type: 'number' },
    slot_end_ms: { type: 'number' },
    expires_at_ms: { type: 'number' },
    hold_token_hash: { type: 'string' },
    status: { type: 'string' } // "active" | "released" | "booked"
  },
  required: ['id', 'booking_page_id', 'slot_start_ms', 'slot_end_ms', 'expires_at_ms', 'status'],
  additionalProperties: true
};

const calendarBookingSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    booking_page_id: { type: 'string' },
    event_id: { type: 'string' },
    attendee_name: { type: 'string' },
    attendee_email: { type: 'string' },
    attendee_phone: { type: 'string' },
    answers: { type: 'object', additionalProperties: true },
    slot_start_ms: { type: 'number' },
    slot_end_ms: { type: 'number' },
    timezone: { type: 'string' },
    status: { type: 'string' }, // "confirmed" | "cancelled"
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'booking_page_id', 'attendee_name', 'attendee_email', 'slot_start_ms', 'slot_end_ms', 'status', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

// `calendar_events` opts into the field-merge conflict strategy
// (docs/ctox-rxdb.md §8.2): concurrent edits to different fields (time vs.
// title vs. description) both survive. Hash-neutral sibling wrapper.
// business_commands is shell-registered — a module schema must not redefine it
// (module.json still declares it for ACCESS; the shell owns the schema).
export const collections = {
  calendar_sources: calendarSourceSchema,
  calendar_calendars: calendarCalendarSchema,
  calendar_events: { schema: calendarEventSchema, conflictStrategy: 'field-merge' },
  calendar_event_instances: calendarEventInstanceSchema,
  calendar_availability_rules: calendarAvailabilityRuleSchema,
  calendar_booking_pages: calendarBookingPageSchema,
  calendar_booking_holds: calendarBookingHoldSchema,
  calendar_bookings: calendarBookingSchema
};

export const migrationStrategies = {};
