// Origin: CTOX
// License: AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  WORKER_EXECUTION_POLICY_SCHEMA, WORKER_EXECUTION_POLICY_VERSION,
  WORKER_EXECUTION_POLICY_TYPES, validateWorkerExecutionPolicyValue,
} from '../../shared/workjet-worker-execution-policy-contract.generated.mjs';

const fixture = JSON.parse(readFileSync(new URL(
  '../../../../core/rxdb/tests/fixtures/workjet-worker-execution-policy-v1.json',
  import.meta.url,
), 'utf8'));
assert.equal(WORKER_EXECUTION_POLICY_SCHEMA, fixture.schema);
assert.equal(WORKER_EXECUTION_POLICY_VERSION, fixture.contract_version);
assert.deepEqual(WORKER_EXECUTION_POLICY_TYPES, fixture.types);
for (const {type, value} of fixture.valid_cases) {
  assert.equal(validateWorkerExecutionPolicyValue(type, value).ok, true, JSON.stringify(value));
}
for (const {type, value} of fixture.invalid_cases) {
  assert.equal(validateWorkerExecutionPolicyValue(type, value).ok, false, JSON.stringify(value));
}
assert.throws(() => {
  WORKER_EXECUTION_POLICY_TYPES.WorkerExecutionPolicyReference.fields.revision.minimum = 0;
}, TypeError);
console.log(`Worker policy reference: ${fixture.valid_cases.length} valid, ${fixture.invalid_cases.length} rejected; native/browser corpus shared`);
