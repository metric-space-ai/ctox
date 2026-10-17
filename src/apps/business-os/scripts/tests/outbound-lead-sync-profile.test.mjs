import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const moduleDir = new URL('../../customer-modules/outbound-lead-generation/', import.meta.url);

// Owner 15.10.2026: client first - the leads replicate into the browser. v4: schema.js is gone, the
// installed collections.schema.json is the only source; the profile is wrapper metadata next to the schema.
test('the leads collection replicates eagerly; the profile is wrapper metadata and the schema stays at v0', () => {
  const manifest = JSON.parse(readFileSync(new URL('collections.schema.json', moduleDir), 'utf8'));
  const leads = manifest.collections.outbound_lead_generation_leads;
  assert.equal(leads.syncProfile, 'eager');
  assert.equal(leads.schema.syncProfile, undefined, 'the profile is wrapper metadata, not part of the schema');
  assert.equal(leads.schema.version, 0);
  assert.equal(leads.schema.primaryKey, 'id');
  const original = JSON.parse(readFileSync(new URL('./fixtures/outbound-lead-schema-v0.json', import.meta.url), 'utf8'));
  assert.deepEqual(leads.schema, original, 'the sync profile must not migrate the installed lead schema');
  assert.equal(leads.schema.additionalProperties, true, 'field_status remains permitted as an additional property');
});
