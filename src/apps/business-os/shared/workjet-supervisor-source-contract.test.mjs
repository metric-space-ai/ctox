import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { SUPERVISOR_SOURCE_SCHEMA, SUPERVISOR_SOURCE_VERSION, validateSupervisorSourceValue } from './workjet-supervisor-source-contract.generated.mjs';

const fixture = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-supervisor-source-v1.json', import.meta.url), 'utf8'));
test('browser and native share Source goal page fixtures and reject caller scope overrides', () => {
  assert.equal(fixture.schema, SUPERVISOR_SOURCE_SCHEMA);
  assert.equal(fixture.contract_version, SUPERVISOR_SOURCE_VERSION);
  for (const item of fixture.valid_cases) {
    assert.equal(validateSupervisorSourceValue(item.type, item.value).ok, true, item.type);
  }
  for (const item of fixture.invalid_cases) {
    assert.equal(validateSupervisorSourceValue(item.type, item.value).ok, false, item.reason);
  }
});

test('goal page EOF is framing; changed, unavailable and capacity states carry no completion promise', () => {
  const base = fixture.valid_cases.find(item => item.type === 'SourceGoalReadPage').value;
  assert.equal(base.document_complete, true);
  assert.equal(base.next_cursor ?? null, null);
  assert.equal(base.goal_complete, undefined);
  for (const state of ['snapshot_changed', 'snapshot_unavailable', 'capacity_unavailable']) {
    const value = { ...base, state, json_fragment: '', byte_length: 0, document_complete: false };
    assert.equal(validateSupervisorSourceValue('SourceGoalReadPage', value).ok, true);
  }
});
