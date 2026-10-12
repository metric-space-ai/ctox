// Generated from src/core/rxdb/tests/fixtures/workjet-worker-execution-policy-v1.json. Do not edit.
export const WORKER_EXECUTION_POLICY_SCHEMA = "ctox.workjet.worker_execution_policy.v1";
export const WORKER_EXECUTION_POLICY_VERSION = 1;
export const WORKER_EXECUTION_POLICY_TYPES = deepFreeze({
  "WorkerExecutionPolicyMode": {
    "enum": [
      "autonomous-worktree"
    ]
  },
  "WorkerExecutionPolicyReference": {
    "derive_eq": true,
    "fields": {
      "mode": {
        "type": "WorkerExecutionPolicyMode"
      },
      "projectId": {
        "type": "String",
        "rust_name": "project_id",
        "min_chars": 1,
        "max_chars": 256
      },
      "revision": {
        "type": "u64",
        "minimum": 1,
        "maximum": 9007199254740991
      }
    }
  }
});
export const WORKER_EXECUTION_POLICY_COMMANDS = deepFreeze({
  "business_os.remote_worker_admission": "Optional binding.executionPolicy is a strict project policy reference, never a team/path/provider grant. Issue/claim/revalidate/renew require current Owner project mode/revision and a genuine current native supervisor/team join. Missing preserves legacy admission; null is rejected; revoke remains usable after reset."
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateWorkerExecutionPolicyValue(typeName, value) {
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
    const shape = WORKER_EXECUTION_POLICY_TYPES[type];
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
