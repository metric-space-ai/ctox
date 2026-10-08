// Generated from src/core/rxdb/tests/fixtures/workjet-project-kpis-v1.json. Do not edit.
export const PROJECT_KPIS_SCHEMA = "ctox.workjet.project_kpis.v1";
export const PROJECT_KPIS_VERSION = 1;
export const PROJECT_KPIS_TYPES = deepFreeze({
  "KpiState": {
    "enum": [
      "resolving",
      "ready",
      "stale",
      "missing_source",
      "failed"
    ]
  },
  "SourceKind": {
    "enum": [
      "native_metric",
      "github_metric",
      "connected_metric"
    ]
  },
  "Calculation": {
    "enum": [
      "identity",
      "sum",
      "average",
      "percentage"
    ]
  },
  "PromptInput": {
    "fields": {
      "kpi_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "prompt": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 1024
      }
    }
  },
  "KpiPrompt": {
    "fields": {
      "kpi_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "prompt": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 1024
      },
      "revision": {
        "type": "u64",
        "minimum": 1
      }
    }
  },
  "SourceEvidence": {
    "fields": {
      "source_key": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "kind": {
        "type": "SourceKind"
      },
      "connection_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "metric_key": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "snapshot_revision": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "evidence_ref": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "observed_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "value": {
        "type": "f64"
      }
    }
  },
  "Computation": {
    "fields": {
      "recipe_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "revision": {
        "type": "u64",
        "minimum": 1
      },
      "operation": {
        "type": "Calculation"
      },
      "input_keys": {
        "type": "Vec<String>",
        "min_items": 1,
        "max_items": 8
      },
      "window_start_ms": {
        "type": "i64",
        "minimum": 0
      },
      "window_end_ms": {
        "type": "i64",
        "minimum": 0
      }
    }
  },
  "Freshness": {
    "fields": {
      "calculated_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "refresh_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "fresh_until_ms": {
        "type": "i64",
        "minimum": 0
      }
    }
  },
  "KpiSnapshot": {
    "fields": {
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "kpi_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "prompt_revision": {
        "type": "u64",
        "minimum": 1
      },
      "label": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 14
      },
      "value": {
        "type": "f64"
      },
      "unit": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 16
      },
      "display_value": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 32
      },
      "sources": {
        "type": "Vec<SourceEvidence>",
        "min_items": 1,
        "max_items": 8
      },
      "computation": {
        "type": "Computation"
      },
      "freshness": {
        "type": "Freshness"
      }
    }
  },
  "KpiResult": {
    "fields": {
      "status": {
        "type": "KpiState"
      },
      "snapshot": {
        "type": "KpiSnapshot",
        "optional": true
      },
      "reason_code": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 64,
        "optional": true
      },
      "message": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256,
        "optional": true
      }
    }
  },
  "KpiRecord": {
    "fields": {
      "prompt": {
        "type": "KpiPrompt"
      },
      "result": {
        "type": "KpiResult"
      }
    }
  },
  "ProjectKpis": {
    "fields": {
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "revision": {
        "type": "u64",
        "minimum": 0
      },
      "items": {
        "type": "Vec<KpiRecord>",
        "max_items": 3
      }
    }
  },
  "ReadKpisRequest": {
    "fields": {
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "ConfigureKpisRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64",
        "minimum": 0
      },
      "prompts": {
        "type": "Vec<PromptInput>",
        "max_items": 3
      }
    }
  },
  "ResolveKpiRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "kpi_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "prompt_revision": {
        "type": "u64",
        "minimum": 1
      },
      "expected_revision": {
        "type": "u64",
        "minimum": 0
      }
    }
  },
  "NativeMetricRecipe": {
    "enum": [
      "project_tasks_total",
      "project_tasks_completed",
      "project_tasks_failed",
      "project_tasks_open",
      "project_tasks_success_rate",
      "github_merged_prs"
    ]
  },
  "BindKpiRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "kpi_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "prompt_revision": {
        "type": "u64",
        "minimum": 1
      },
      "expected_revision": {
        "type": "u64",
        "minimum": 0
      },
      "recipe": {
        "type": "NativeMetricRecipe"
      },
      "window_days": {
        "type": "u64",
        "minimum": 1,
        "maximum": 365
      }
    }
  }
});
export const PROJECT_KPIS_COMMANDS = deepFreeze({
  "ctox.workjet.project.kpis.read": {
    "request_type": "ReadKpisRequest",
    "authorization": "owner"
  },
  "ctox.workjet.project.kpis.configure": {
    "request_type": "ConfigureKpisRequest",
    "authorization": "owner"
  },
  "ctox.workjet.project.kpi.resolve": {
    "request_type": "ResolveKpiRequest",
    "authorization": "bound_project_supervisor",
    "execution": "native_registered_metric_resolver",
    "accepts_client_result": false
  }
});
export const PROJECT_KPIS_RULES = deepFreeze({
  "PromptInput": {
    "nonblank": [
      "kpi_id",
      "prompt"
    ]
  },
  "KpiPrompt": {
    "nonblank": [
      "kpi_id",
      "prompt"
    ]
  },
  "Computation": {
    "lte": [
      [
        "window_start_ms",
        "window_end_ms"
      ]
    ],
    "unique": [
      {
        "field": "input_keys"
      }
    ]
  },
  "Freshness": {
    "lt": [
      [
        "calculated_at_ms",
        "refresh_at_ms"
      ]
    ],
    "lte": [
      [
        "refresh_at_ms",
        "fresh_until_ms"
      ]
    ]
  },
  "KpiSnapshot": {
    "unique": [
      {
        "field": "sources",
        "key": "source_key"
      }
    ],
    "every_eq": [
      {
        "field": "sources",
        "key": "project_id",
        "target": "project_id"
      }
    ],
    "every_lte": [
      {
        "field": "sources",
        "key": "observed_at_ms",
        "target": "freshness.calculated_at_ms"
      }
    ],
    "calculate": true
  },
  "KpiResult": {
    "state_fields": {
      "field": "status",
      "states": {
        "resolving": {
          "forbidden": [
            "snapshot",
            "reason_code",
            "message"
          ]
        },
        "ready": {
          "required": [
            "snapshot"
          ],
          "forbidden": [
            "reason_code",
            "message"
          ]
        },
        "stale": {
          "required": [
            "snapshot",
            "reason_code",
            "message"
          ]
        },
        "missing_source": {
          "required": [
            "reason_code",
            "message"
          ],
          "forbidden": [
            "snapshot"
          ]
        },
        "failed": {
          "required": [
            "reason_code",
            "message"
          ],
          "forbidden": [
            "snapshot"
          ]
        }
      }
    }
  },
  "KpiRecord": {
    "eq": [
      [
        "prompt.kpi_id",
        "result.snapshot.kpi_id"
      ],
      [
        "prompt.revision",
        "result.snapshot.prompt_revision"
      ]
    ]
  },
  "ProjectKpis": {
    "unique": [
      {
        "field": "items",
        "key": "prompt.kpi_id"
      }
    ],
    "every_eq": [
      {
        "field": "items",
        "key": "result.snapshot.project_id",
        "target": "project_id"
      }
    ]
  },
  "ReadKpisRequest": {
    "nonblank": [
      "project_id"
    ]
  },
  "ConfigureKpisRequest": {
    "nonblank": [
      "operation_id",
      "project_id"
    ],
    "unique": [
      {
        "field": "prompts",
        "key": "kpi_id"
      }
    ]
  },
  "BindKpiRequest": {
    "nonblank": [
      "operation_id",
      "project_id",
      "kpi_id"
    ]
  }
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateProjectKpiValue(typeName, value) {
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
    const shape = PROJECT_KPIS_TYPES[type];
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
    validateRules(type, value);
  }
  try { validate(typeName, value, typeName); return {ok:true}; }
  catch (error) { return {ok:false, error:error.message}; }
}
function at(value, path) {
  for (const key of path.split('.')) value = value?.[key];
  return value ?? undefined;
}
function validateRules(kind, value) {
  const rules = PROJECT_KPIS_RULES[kind];
  if (!rules) return;
  const fail = rule => { throw new Error(kind + ': ' + rule); };
  for (const field of rules.nonblank || []) if (/^[ \t\r\n]*$/.test(at(value,field))) fail('blank prompt or identity');
  for (const operation of ['eq','lt','lte']) for (const [a,b] of rules[operation] || []) {
    const left=at(value,a), right=at(value,b);
    if (left===undefined || right===undefined) continue;
    if (!(operation==='eq' ? left===right : operation==='lt' ? left<right : left<=right)) fail(operation);
  }
  for (const rule of rules.unique || []) {
    const keys=(at(value,rule.field) || []).map(v => rule.key ? at(v,rule.key) : v);
    if (new Set(keys).size!==keys.length) fail('duplicate identity or input');
  }
  for (const operation of ['every_eq','every_lte']) for (const rule of rules[operation] || []) {
    const right=at(value,rule.target);
    for (const item of at(value,rule.field) || []) {
      const left=at(item,rule.key);
      if (left===undefined || right===undefined) continue;
      if (!(operation==='every_eq' ? left===right : left<=right)) fail(operation);
    }
  }
  if (rules.state_fields) {
    const fields=rules.state_fields.states[at(value,rules.state_fields.field)];
    if (!fields) fail('missing state');
    for (const field of fields.required || []) if (at(value,field)===undefined) fail('required');
    for (const field of fields.forbidden || []) if (at(value,field)!==undefined) fail('forbidden');
  }
  if (rules.calculate) {
    const keys=value.computation.input_keys;
    if (keys.length!==value.sources.length) fail('all evidence must be consumed');
    const inputs=keys.map(key => {
      const source=value.sources.find(s => s.source_key===key);
      if (!source) fail('unknown input');
      return source.value;
    });
    const total=inputs.reduce((a,b) => a+b,0);
    let expected;
    switch (value.computation.operation) {
      case 'identity': if (inputs.length!==1) fail('arity'); expected=inputs[0]; break;
      case 'sum': expected=total; break;
      case 'average': expected=total/inputs.length; break;
      case 'percentage': if (inputs.length!==2 || inputs[1]===0) fail('arity or denominator'); expected=100*inputs[0]/inputs[1]; break;
      default: fail('calculation');
    }
    if (!Number.isFinite(expected) || value.value!==expected) fail('value differs from evidence');
  }
}

