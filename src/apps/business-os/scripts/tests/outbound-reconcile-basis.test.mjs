import assert from 'node:assert/strict';
import { test } from 'node:test';
import { abgleichBasisVeraltet, hatBelegteFelder } from '../../customer-modules/outbound-lead-generation/reconcile-basis.mjs';

// thesen 09.10.2026: memory said "running" with an old failed command, the
// document already held the native result ("needs_review", newer command).
const stale = { id: 'lead_a', _rev: '3-a', research_status: 'running', command_id: 'cmd-old' };
const current = { id: 'lead_a', _rev: '5-b', research_status: 'needs_review', command_id: 'cmd-new',
  field_status: { firma_name: { status: 'verified', value: 'A GmbH' } } };

test('a stale memory copy does not decide a write to the current document', () => {
  assert.equal(abgleichBasisVeraltet(stale, current), true);
  assert.equal(abgleichBasisVeraltet({ ...stale, _rev: undefined }, { ...current, _rev: undefined }), true,
    'without revisions the status and command still reveal the stale copy');
  assert.equal(abgleichBasisVeraltet({ ...stale, _rev: '5-b', command_id: 'cmd-new' }, current), true);
});

test('an unchanged document lets the reconcile write', () => {
  assert.equal(abgleichBasisVeraltet({ ...current }, current), false);
  assert.equal(abgleichBasisVeraltet({ ...current, _rev: undefined }, current), false);
});

test('verified fields count as a research result', () => {
  assert.equal(hatBelegteFelder(current), true);
  assert.equal(hatBelegteFelder({ field_status: { x: { status: 'no_match' } } }), false);
  assert.equal(hatBelegteFelder({}), false);
  assert.equal(hatBelegteFelder(null), false);
});
