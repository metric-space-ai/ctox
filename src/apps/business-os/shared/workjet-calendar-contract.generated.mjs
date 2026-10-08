// Generated from src/core/rxdb/tests/fixtures/workjet-calendar-v1.json. Do not edit.
export const CALENDAR_SCHEMA = "ctox.workjet.calendar.v1";
export const CALENDAR_VERSION = 1;
export const CALENDAR_TYPES = deepFreeze({
  "CalendarKind": {
    "enum": [
      "local",
      "project_meeting",
      "synced"
    ]
  },
  "CalendarEvent": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "calendar_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "kind": {
        "type": "CalendarKind"
      },
      "title": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "start_ms": {
        "type": "i64"
      },
      "end_ms": {
        "type": "i64"
      },
      "all_day": {
        "type": "bool"
      },
      "timezone": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "location": {
        "type": "String",
        "max_chars": 512,
        "optional": true
      },
      "notes": {
        "type": "String",
        "max_chars": 4096,
        "optional": true
      },
      "revision": {
        "type": "u64"
      }
    }
  }
});
export const CALENDAR_COMMANDS = deepFreeze({});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateCalendarValue(typeName, value) {
  function validate(type, value, field) {
    if (type.startsWith('Vec<')) {
      if (!Array.isArray(value)) throw new Error(field + ': array required');
      value.forEach(v => validate(type.slice(4,-1), v, field)); return;
    }
    if (type === 'String') { if (typeof value !== 'string') throw new Error(field + ': string required'); return; }
    if (type === 'bool') { if (typeof value !== 'boolean') throw new Error(field + ': boolean required'); return; }

    if (['u64','i64','f64'].includes(type)) {
      if (typeof value !== 'number' || !Number.isFinite(value)
          || (type !== 'f64' && (!Number.isSafeInteger(value) || (type === 'u64' && value < 0)))) throw new Error(field + ': invalid number');
      return;
    }
    const shape = CALENDAR_TYPES[type];
    if (!shape) throw new Error(field + ': unknown type');
    if (shape.enum) { if (!shape.enum.includes(value)) throw new Error(field + ': invalid enum'); return; }
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(field + ': object required');
    for (const key of Object.keys(value)) if (!Object.hasOwn(shape.fields, key)) throw new Error(field + ': unexpected ' + key);
    for (const [key, definition] of Object.entries(shape.fields)) {
      const v = value[key], at = field + '.' + key;
      if (v === undefined || v === null) { if (definition.optional) continue; throw new Error(at + ': required'); }
      validate(definition.type, v, at);
      for (const [constraint,bound] of Object.entries(definition)) {
        const metric = constraint.endsWith('_chars') ? [...v].length : constraint.endsWith('_items') ? v.length : v;
        if ((['minimum','min_chars','min_items'].includes(constraint) && metric < bound)
            || (['maximum','max_chars','max_items'].includes(constraint) && metric > bound)) throw new Error(at + ': ' + constraint);
      }
    }
  }
  try { validate(typeName, value, typeName); return {ok:true}; }
  catch (error) { return {ok:false, error:error.message}; }
}
