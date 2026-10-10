import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { validateSupervisorLumaValue } from './workjet-supervisor-luma-contract.generated.mjs';
import { collections, migrationStrategies } from '../modules/ctox/schema.js';

const corpus = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-supervisor-luma-v1.json', import.meta.url)));

test('native and browser share the project Supervisor Luma reference corpus', () => {
  for (const sample of corpus.valid_cases) {
    assert.equal(validateSupervisorLumaValue(sample.type, sample.value).ok, true);
  }
  for (const sample of corpus.invalid_cases) {
    assert.equal(validateSupervisorLumaValue(sample.type, sample.value).ok, false);
  }
  assert.equal(validateSupervisorLumaValue('ProjectSupervisorLuma', { supervisor_luma_id: 'x'.repeat(161) }).ok, false);
});

test('project migration preserves every existing project without inventing a Supervisor selection', () => {
  const legacy = { id: 'project-legacy', name: 'Legacy', status: 'active', owner_user_id: 'owner-1',
    created_at_ms: 1, updated_at_ms: 2, info: { goal: 'Keep the existing work' },
    jour_fixe: { weekday: 1, time: '13:00', timezone: 'Europe/Berlin' } };
  for (const version of [1, 2, 3]) {
    const migrated = migrationStrategies.workjet_projects[version](legacy);
    assert.deepEqual(migrated, legacy);
    assert.equal(Object.hasOwn(migrated, 'supervisor_luma_id'), false);
  }
  const schema = collections.workjet_projects;
  assert.equal(schema.version, 3);
  assert.equal(schema.required.includes('supervisor_luma_id'), false);
  assert.deepEqual(schema.properties.supervisor_luma_id, { type: 'string', minLength: 1, maxLength: 160 });
});
