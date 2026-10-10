// Generated from src/core/rxdb/tests/fixtures/workjet-project-execution-policy-v1.json. Do not edit.
export const PROJECT_EXECUTION_POLICY_SCHEMA = "ctox.workjet.project_execution_policy.v1";
export const PROJECT_EXECUTION_POLICY_VERSION = 1;
export const PROJECT_EXECUTION_POLICY_TYPES = deepFreeze({
  "ProjectExecutionPolicyMode": {
    "enum": [
      "default",
      "autonomous_worktree"
    ]
  },
  "ProjectExecutionPolicySchema": {
    "enum": [
      "ctox.workjet.project_execution_policy.v1"
    ]
  },
  "ProjectExecutionPolicyUpdate": {
    "fields": {
      "schema": {
        "type": "ProjectExecutionPolicySchema"
      },
      "mode": {
        "type": "ProjectExecutionPolicyMode"
      },
      "expected_revision": {
        "type": "u64",
        "maximum": 9007199254740991
      }
    }
  },
  "ProjectExecutionPolicy": {
    "fields": {
      "schema": {
        "type": "ProjectExecutionPolicySchema"
      },
      "mode": {
        "type": "ProjectExecutionPolicyMode"
      },
      "revision": {
        "type": "u64",
        "maximum": 9007199254740991
      }
    }
  }
});
export const PROJECT_EXECUTION_POLICY_COMMANDS = deepFreeze({
  "ctox.workjet.project.upsert": "execution_policy: Owner intent only; omitted preserves. A typed default-mode CAS patch revokes opt-in. Null is rejected because it carries no expected_revision. Never grants tools/paths/host access."
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateProjectExecutionPolicyValue(typeName, value) {
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
    const shape = PROJECT_EXECUTION_POLICY_TYPES[type];
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
