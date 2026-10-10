// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-route-computation-v2.json. Do not edit.
export const SUPERVISOR_ROUTE_COMPUTATION_SCHEMA = "ctox.workjet.supervisor.route-display.v2";
export const SUPERVISOR_ROUTE_COMPUTATION_VERSION = 2;
export const SUPERVISOR_ROUTE_COMPUTATION_TYPES = deepFreeze({
  "RouteCapabilitiesSchema": {
    "enum": [
      "ctox.workjet.supervisor.route-capabilities.v2"
    ]
  },
  "RouteReadCommand": {
    "enum": [
      "ctox.workjet.project.supervisor.route.read.v2"
    ]
  },
  "RouteDisplaySchema": {
    "enum": [
      "ctox.workjet.supervisor.route-display.v2"
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
  "SelectedSupervisorRoute": {
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
      "route_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 160
      }
    }
  },
  "ActualSupervisorComputation": {
    "fields": {
      "receipt_id": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "execution_key": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "selected_route": {
        "type": "SelectedSupervisorRoute"
      },
      "harness": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 32
      },
      "computer_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "account_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "model": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "model_operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "native_message_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "upstream_request_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "sdk_session_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "sdk_turn_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "sdk_assistant_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "sdk_result_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "model_finished_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "published_at_ms": {
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
      "actual": {
        "type": "ActualSupervisorComputation",
        "optional": true
      }
    }
  }
});
export const SUPERVISOR_ROUTE_COMPUTATION_COMMANDS = deepFreeze({
  "ctox.workjet.project.supervisor.route.capabilities.v2": {
    "scope": "Current native Owner project and bound Supervisor; DataRead"
  },
  "ctox.workjet.project.supervisor.route.read.v2": {
    "scope": "Current native Owner project and bound Supervisor; read-only verified published computation",
    "actual": "Published original native computation joined to its immutable SDK and HTTP model witness. IDs are explicitly labelled; selected_route is configured selection, not an upstream assertion."
  }
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateSupervisorRouteComputationValue(typeName, value) {
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
    const shape = SUPERVISOR_ROUTE_COMPUTATION_TYPES[type];
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
