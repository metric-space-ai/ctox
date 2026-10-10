import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { validateWorkerOutcomeValue } from './workjet-worker-outcome-contract.generated.mjs';
const corpus=JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-worker-outcome-v1.json',import.meta.url),'utf8'));
for(const [index,item] of corpus.valid_cases.entries()) test('worker outcome valid fixture '+index,()=>assert.equal(validateWorkerOutcomeValue(item.type,item.value).ok,true));
for(const [index,item] of corpus.invalid_cases.entries()) test('worker outcome rejects invalid fixture '+index,()=>assert.equal(validateWorkerOutcomeValue(item.type,item.value).ok,false));
