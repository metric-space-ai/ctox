// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json. Do not edit.
export const SUPERVISOR_ROUTE_DISPLAY_SCHEMA = "ctox.workjet.supervisor.route-display.v1";
export const SUPERVISOR_ROUTE_DISPLAY_VERSION = 1;
export const SUPERVISOR_ROUTE_DISPLAY_TYPES = deepFreeze({
  "RouteCapabilitiesSchema": {
    "enum": [
      "ctox.workjet.supervisor.route-capabilities.v1"
    ]
  },
  "RouteReadCommand": {
    "enum": [
      "ctox.workjet.project.supervisor.route.read.v1"
    ]
  },
  "SupervisorRouteCapabilities": {
    "fields": {
      "schema": {
        "type": "RouteCapabilitiesSchema"
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "supervisor_thread_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "read_schema": {
        "type": "RouteDisplaySchema"
      },
      "read_command": {
        "type": "RouteReadCommand"
      }
    }
  },
  "RouteDisplaySchema": {
    "enum": [
      "ctox.workjet.supervisor.route-display.v1"
    ]
  },
  "ConfiguredSupervisorRoute": {
    "fields": {
      "luma_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 160
      },
      "configuration_revision": {
        "type": "u64",
        "minimum": 1
      },
      "computer_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "harness": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 32
      },
      "route_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 160
      },
      "model": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "catalog_checked_at_ms": {
        "type": "i64",
        "minimum": 0
      }
    }
  },
  "RequestedRouteSource": {
    "fields": {
      "execution_key": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "request_revision": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "error_code": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 96
      },
      "created_at_ms": {
        "type": "i64",
        "minimum": 0
      }
    }
  },
  "ActualSupervisorProducer": {
    "fields": {
      "run_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "turn_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "receipt_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "luma_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 160
      },
      "configuration_revision": {
        "type": "u64",
        "minimum": 1
      },
      "computer_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "harness": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 32
      },
      "route_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 160
      },
      "model": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "catalog_checked_at_ms": {
        "type": "i64",
        "minimum": 0
      }
    }
  },
  "SupervisorRouteDisplay": {
    "fields": {
      "schema": {
        "type": "RouteDisplaySchema"
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "supervisor_thread_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 36
      },
      "configured": {
        "type": "ConfiguredSupervisorRoute",
        "optional": true
      },
      "source": {
        "type": "RequestedRouteSource",
        "optional": true
      },
      "actual": {
        "type": "ActualSupervisorProducer",
        "optional": true
      }
    }
  }
});
export const SUPERVISOR_ROUTE_DISPLAY_COMMANDS = deepFreeze({
  "ctox.workjet.project.supervisor.route.capabilities.v1": {
    "scope": "Native Owner project and bound Supervisor; DataRead; separate from strict v1 turn capabilities"
  },
  "ctox.workjet.project.supervisor.route.read.v1": {
    "scope": "Native Owner project and bound Supervisor; DataRead",
    "actual": "Currently always null. Requested route source is not a producer receipt."
  }
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateSupervisorRouteDisplayValue(typeName, value) {
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
    const shape = SUPERVISOR_ROUTE_DISPLAY_TYPES[type];
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
    for (const order of shape.ordered_fields ?? []) {
      if (value[order.after] <= value[order.before]) throw new Error(field + '.' + order.after + ': must follow ' + order.before);
    }
  }
  try { validate(typeName, value, typeName); return {ok:true}; }
  catch (error) { return {ok:false, error:error.message}; }
}
