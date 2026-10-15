import assert from 'node:assert/strict';
import { test } from 'node:test';
import { doppelteAdapterIds } from '../../customer-modules/outbound-lead-generation/adapter-dedupe.mjs';

test('legacy adapters next to a current adapter of the same source are removed', () => {
  const ids = doppelteAdapterIds([
    { id: 'adapter_leadgen_northdata-de', source_id: 'northdata.de' },
    { id: 'adapter_old_northdata-de', source_id: 'northdata.de' },
    { id: 'adapter_old_xing-com', source_id: 'xing.com' },
    { id: 'adapter_leadgen_google-de', source_id: 'google.de', _deleted: true },
    { id: 'adapter_old_google-de', source_id: 'google.de' },
  ]);
  assert.deepEqual(ids, ['adapter_old_northdata-de']);
});
