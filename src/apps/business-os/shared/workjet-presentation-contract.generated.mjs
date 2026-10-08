// Generated from src/core/rxdb/tests/fixtures/workjet-presentation-v1.json. Do not edit.
export const PRESENTATION_SCHEMA = "ctox.workjet.presentation.v1";
export const PRESENTATION_VERSION = 1;
export const PRESENTATION_TYPES = deepFreeze({
  "PresentationSource": {
    "enum": [
      "agent",
      "owner"
    ]
  },
  "PresentationManifest": {
    "fields": {
      "presentation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "owner_user_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "title": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 180
      },
      "revision": {
        "type": "u64",
        "minimum": 1
      },
      "document_schema": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 64
      },
      "document_file_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "document_generation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "document_sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "document_bytes": {
        "type": "u64",
        "minimum": 2,
        "maximum": 8388608
      },
      "slide_ids": {
        "type": "Vec<String>",
        "min_items": 1,
        "max_items": 160
      },
      "source": {
        "type": "PresentationSource"
      },
      "updated_by": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "updated_at_ms": {
        "type": "i64",
        "minimum": 0
      }
    }
  },
  "ReadPresentationRequest": {
    "fields": {
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "ReadPresentationResponse": {
    "fields": {
      "contract": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 64
      },
      "presentation": {
        "type": "PresentationManifest",
        "optional": true
      }
    }
  },
  "ReadPresentationContentRequest": {
    "fields": {
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "presentation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "revision": {
        "type": "u64",
        "minimum": 1
      },
      "offset": {
        "type": "u64",
        "maximum": 8388608
      },
      "length": {
        "type": "u64",
        "minimum": 1,
        "maximum": 131072
      }
    }
  },
  "PresentationContentRange": {
    "fields": {
      "presentation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "revision": {
        "type": "u64",
        "minimum": 1
      },
      "offset": {
        "type": "u64",
        "maximum": 8388608
      },
      "length": {
        "type": "u64",
        "minimum": 1,
        "maximum": 131072
      },
      "total_bytes": {
        "type": "u64",
        "minimum": 2,
        "maximum": 8388608
      },
      "document_sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "data_base64": {
        "type": "String",
        "min_chars": 4,
        "max_chars": 174764
      }
    }
  },
  "SavePresentationCanvasRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "presentation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64",
        "minimum": 1
      },
      "slide_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 120
      },
      "scene_json": {
        "type": "String",
        "min_chars": 2,
        "max_chars": 4194304
      }
    }
  },
  "ApplyPresentationEditsRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "presentation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64",
        "minimum": 1
      },
      "operations_json": {
        "type": "String",
        "min_chars": 2,
        "max_chars": 1048576
      }
    }
  },
  "PresentationMutationReceipt": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "presentation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "revision": {
        "type": "u64",
        "minimum": 1
      },
      "document_sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "document_bytes": {
        "type": "u64",
        "minimum": 2,
        "maximum": 8388608
      },
      "slide_ids": {
        "type": "Vec<String>",
        "min_items": 1,
        "max_items": 160
      }
    }
  }
});
export const PRESENTATION_COMMANDS = deepFreeze({
  "ctox.workjet.presentation.read": {
    "request_type": "ReadPresentationRequest",
    "authorization": "owner"
  },
  "ctox.workjet.presentation.canvas.save": {
    "request_type": "SavePresentationCanvasRequest",
    "authorization": "owner"
  },
  "ctox.workjet.presentation.edits.apply": {
    "request_type": "ApplyPresentationEditsRequest",
    "authorization": "owner"
  }
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validatePresentationValue(typeName, value) {
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
    const shape = PRESENTATION_TYPES[type];
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
