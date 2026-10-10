import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {validateSupervisorRouteDisplayValue as validate} from './workjet-supervisor-route-display-contract.generated.mjs';

const spec=JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json',import.meta.url)));
test('native and browser route-display contract share the strict fixture corpus',()=>{
  for (const sample of spec.valid_cases) assert.equal(validate(sample.type,sample.value).ok,true,JSON.stringify(sample));
  for (const sample of spec.invalid_cases) assert.equal(validate(sample.type,sample.value).ok,false,JSON.stringify(sample));
});
test('configured route is not an actual producer witness',()=>{
  const display=spec.valid_cases.find(sample => sample.type === 'SupervisorRouteDisplay' && sample.value.configured).value;
  assert.equal(display.actual,null);
  assert.equal(validate('SupervisorRouteDisplay',{...display,actual:{model:display.configured.model}}).ok,false);
  assert.equal(validate('SupervisorRouteDisplay',{...display,configured:{...display.configured,nativeAccountReference:{accountId:'private'}}}).ok,false);
});
