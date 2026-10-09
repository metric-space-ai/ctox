import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const moduleDir = new URL('../../customer-modules/outbound-lead-generation/', import.meta.url);

test('the leads collection registers demand-only, as its manifest declares', async () => {
  const { collections } = await import(new URL('schema.js', moduleDir));
  const manifest = JSON.parse(readFileSync(new URL('collections.schema.json', moduleDir), 'utf8'));
  const leads = collections.outbound_lead_generation_leads;
  assert.equal(manifest.collections.outbound_lead_generation_leads.syncProfile, 'demand-only');
  assert.equal(leads.syncProfile, 'demand-only', 'the shell registers schema.js; the profile must travel with it');
  assert.equal(leads.schema.syncProfile, undefined, 'the profile is wrapper metadata, not part of the schema');
  assert.deepEqual(leads.schema, manifest.collections.outbound_lead_generation_leads.schema,
    'demand-only must not change the installed lead schema');
});
