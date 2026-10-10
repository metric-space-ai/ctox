import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {validateSupervisorRouteComputationValue as validate} from './workjet-supervisor-route-computation-contract.generated.mjs';
const spec=JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-supervisor-route-computation-v2.json',import.meta.url)));
test('v2 native and browser computation contract share the strict fixture corpus',()=>{
 for(const sample of spec.valid_cases) assert.equal(validate(sample.type,sample.value).ok,true,JSON.stringify(sample));
 for(const sample of spec.invalid_cases) assert.equal(validate(sample.type,sample.value).ok,false,JSON.stringify(sample));
});
