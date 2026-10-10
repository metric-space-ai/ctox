// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-source-v1.json. Do not edit.
export const SUPERVISOR_SOURCE_SCHEMA = "ctox.workjet.supervisor.source.v1";
export const SUPERVISOR_SOURCE_VERSION = 1;
export const SUPERVISOR_SOURCE_TYPES = deepFreeze({
  "SourceAction": {
    "enum": [
      "poll",
      "claim",
      "status",
      "cancel",
      "model_invoke",
      "model_read",
      "tool_call",
      "sdk_observe"
    ]
  },
  "SourceOfferState": {
    "enum": [
      "offered",
      "claimed",
      "closed"
    ]
  },
  "SourceOperation": {
    "fields": {
      "version": {
        "type": "u64",
        "minimum": 1,
        "maximum": 1
      },
      "action": {
        "type": "SourceAction"
      },
      "offer_id": {
        "type": "String",
        "min_chars": 36,
        "max_chars": 36,
        "optional": true
      },
      "controller_id": {
        "type": "String",
        "min_chars": 36,
        "max_chars": 36,
        "optional": true
      },
      "operation_id": {
        "type": "String",
        "min_chars": 36,
        "max_chars": 36,
        "optional": true
      },
      "model_operation": {
        "type": "SourceModelOperation",
        "optional": true
      },
      "body_json": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 98304,
        "optional": true
      },
      "sdk_session_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "sequence": {
        "type": "u64",
        "maximum": 65535,
        "optional": true
      },
      "native_tool": {
        "type": "SourceNativeTool",
        "optional": true
      },
      "tool_arguments_json": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 65536,
        "optional": true
      },
      "sdk_observation": {
        "type": "SourceSdkObservation",
        "optional": true
      }
    }
  },
  "SourceRequestedRoute": {
    "fields": {
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
      "model": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      }
    }
  },
  "SourceOffer": {
    "fields": {
      "offer_id": {
        "type": "String",
        "min_chars": 36,
        "max_chars": 36
      },
      "execution_key": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 512
      },
      "route": {
        "type": "SourceRequestedRoute"
      },
      "deadline_ms": {
        "type": "i64",
        "minimum": 1
      },
      "state": {
        "type": "SourceOfferState"
      }
    }
  },
  "SourceModelOperation": {
    "enum": [
      "messages",
      "count_tokens"
    ]
  },
  "SourceNativeTool": {
    "enum": [
      "worker_dispatch"
    ]
  },
  "SourceSdkObservationKind": {
    "enum": [
      "child-spawned",
      "child-closed",
      "sdk-init",
      "turn-submitted",
      "parent-assistant",
      "sdk-result",
      "sdk-stream-joined",
      "sdk-query-close-returned"
    ]
  },
  "SourceSdkObservation": {
    "fields": {
      "version": {
        "type": "u64",
        "minimum": 1,
        "maximum": 1
      },
      "sequence": {
        "type": "u64",
        "maximum": 511
      },
      "kind": {
        "type": "SourceSdkObservationKind"
      },
      "session_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "init_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "turn_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "message_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "message_model": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "assistant_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "result_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "subtype": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "signal": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      },
      "pid": {
        "type": "u64",
        "minimum": 1,
        "maximum": 4294967295,
        "optional": true
      },
      "exit_code": {
        "type": "i64",
        "minimum": -2147483648,
        "maximum": 2147483647,
        "optional": true
      },
      "is_error": {
        "type": "bool",
        "optional": true
      }
    }
  }
});
export const SUPERVISOR_SOURCE_COMMANDS = deepFreeze({
  "ctox.workjet.project.supervisor.execution.v1": {
    "request": "SourceOperation",
    "transport": "guarded native auxiliary WebRTC; no HTTP data path",
    "authority": "actual enrolled Source + original native lease; DTOs are not permits"
  }
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateSupervisorSourceValue(typeName, value) {
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
    const shape = SUPERVISOR_SOURCE_TYPES[type];
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
