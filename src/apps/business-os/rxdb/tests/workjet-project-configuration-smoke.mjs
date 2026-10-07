import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { collections } from '../../modules/ctox/schema.js';
const cases = JSON.parse(readFileSync(new URL('../../../../core/rxdb/tests/fixtures/workjet-project-configuration-v1.json', import.meta.url)));
const native = JSON.parse(readFileSync(new URL('../../../../core/business_os/business_os_schema_contract.json', import.meta.url)));
assert.deepEqual(native.workjet_projects, collections.workjet_projects);
assert.deepEqual(collections.workjet_projects.properties.info.properties.summary, { type: 'string', maxLength: 4096 });
for (const info of cases.valid) for (const [field, value] of Object.entries(info)) {
  const property = collections.workjet_projects.properties.info.properties[field];
  assert.ok(property, `native payload field ${field} must be declared in the browser schema`);
  assert.equal(typeof value, property.type);
  assert.ok(value.length <= property.maxLength);
}
console.log('Native/browser project configuration schema agrees with the shared valid corpus; negative payloads are guarded by native command tests');
