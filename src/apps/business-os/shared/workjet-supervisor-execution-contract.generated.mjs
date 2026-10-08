// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-execution-v1.json. Do not edit.
export const SUPERVISOR_EXECUTION_SCHEMA = "ctox.workjet.supervisor_execution.v1";
export const SUPERVISOR_EXECUTION_VERSION = 1;
export const SUPERVISOR_EXECUTION_TYPES = deepFreeze({
  "EventCursor": {
    "fields": {
      "after_sequence": {
        "type": "u64",
        "minimum": 1
      },
      "after_event_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "ExecutionPageRequest": {
    "fields": {
      "attempt_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128,
        "optional": true
      },
      "cursor": {
        "type": "EventCursor",
        "optional": true
      },
      "limit": {
        "type": "u64",
        "minimum": 1,
        "maximum": 50,
        "optional": true
      }
    }
  },
  "AttemptRef": {
    "fields": {
      "attempt_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "run_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128,
        "optional": true
      },
      "attempt_index": {
        "type": "u64",
        "optional": true
      },
      "status": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 64,
        "optional": true
      },
      "started_at_ms": {
        "type": "i64",
        "minimum": 0,
        "optional": true
      },
      "finished_at_ms": {
        "type": "i64",
        "minimum": 0,
        "optional": true
      }
    }
  },
  "ExecutionEvent": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "sequence": {
        "type": "u64",
        "minimum": 1
      },
      "kind": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 64
      },
      "title": {
        "type": "String",
        "max_chars": 256
      },
      "created_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "tool_name": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128,
        "optional": true
      },
      "call_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128,
        "optional": true
      },
      "success": {
        "type": "bool",
        "optional": true
      }
    }
  },
  "ExecutionPage": {
    "fields": {
      "command_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "task_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "attempt": {
        "type": "AttemptRef",
        "optional": true
      },
      "events": {
        "type": "Vec<ExecutionEvent>",
        "max_items": 50
      },
      "next_cursor": {
        "type": "EventCursor",
        "optional": true
      },
      "has_more": {
        "type": "bool"
      }
    }
  }
});
export const SUPERVISOR_EXECUTION_COMMANDS = deepFreeze({
  "ctox.workjet.project.supervisor.turn.watch": {
    "opt_in_field": "execution_page",
    "request_type": "ExecutionPageRequest",
    "response_field": "execution_page",
    "response_type": "ExecutionPage",
    "response_contract_field": "execution_contract",
    "authorization": "current_native_project_owner"
  }
});

function deepFreeze(value) { for (const child of Object.values(value)) if (child && typeof child === 'object') deepFreeze(child); return Object.freeze(value); }
export function validateSupervisorExecutionValue(type, value) {
  if (type === 'String') { if (typeof value !== 'string') throw new Error('expected string'); return value; }
  if (type === 'bool') { if (typeof value !== 'boolean') throw new Error('expected boolean'); return value; }
  if (type === 'u64' || type === 'i64') { if (!Number.isSafeInteger(value) || (type === 'u64' && value < 0)) throw new Error('expected safe integer'); return value; }
  if (type.startsWith('Vec<')) { if (!Array.isArray(value)) throw new Error('expected array'); for (const item of value) validateSupervisorExecutionValue(type.slice(4,-1),item); return value; }
  const definition = SUPERVISOR_EXECUTION_TYPES[type];
  if (!definition || !value || typeof value !== 'object' || Array.isArray(value)) throw new Error('invalid observer object');
  for (const key of Object.keys(value)) if (!Object.hasOwn(definition.fields,key)) throw new Error('unknown field '+key);
  for (const [key, rule] of Object.entries(definition.fields)) {
    const field=value[key];
    if (field === undefined || field === null) { if (rule.optional) continue; throw new Error('missing '+key); }
    validateSupervisorExecutionValue(rule.type,field);
    if (rule.type === 'String' && rule.min_chars && /^[\p{White_Space}]*$/u.test(field)) throw new Error('blank '+key);
    const expressions={min_chars:()=>Array.from(field).length,max_chars:()=>Array.from(field).length,max_items:()=>field.length,minimum:()=>field,maximum:()=>field};
    for (const [bound,expression] of Object.entries(expressions)) if (rule[bound] !== undefined && (bound.startsWith('max') ? expression()>rule[bound] : expression()<rule[bound])) throw new Error('bound '+key);
  }
  return value;
}
