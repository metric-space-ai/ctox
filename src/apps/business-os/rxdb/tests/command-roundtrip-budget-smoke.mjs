import assert from 'node:assert/strict';
import { assertWarmCommandBudget } from './command-roundtrip-budget.mjs';
import { synthesizeRoundtripSamples } from './command-roundtrip-stage-report.mjs';

// Synthetic inputs test the rejection boundary only; they are not performance evidence.
const fixture = (total) => synthesizeRoundtripSamples(30).map((sample) => ({
  ...sample,
  marks: {
    ...sample.marks,
    browser_terminal_observed: sample.marks.browser_dispatch_started + total,
  },
}));
assert.equal(assertWarmCommandBudget(fixture(299)).summary.total.raw.p50, 299);
assert.throws(() => assertWarmCommandBudget(fixture(300)), /below 300 ms/);
assert.throws(() => assertWarmCommandBudget(fixture(500)), /below 300 ms/);
assert.throws(() => assertWarmCommandBudget(fixture(200).slice(1)), /at least 30/);
const duplicate = fixture(200);
duplicate[1].command_id = duplicate[0].command_id;
assert.throws(() => assertWarmCommandBudget(duplicate), /duplicate/);
for (const invalid of [null, undefined, '42', NaN, Infinity, 0, -1]) {
  const samples = fixture(200);
  samples[0].marks.native_dispatch_entered = invalid;
  assert.throws(() => assertWarmCommandBudget(samples), /invalid measured mark/);
}
const incomplete = fixture(200);
delete incomplete[0].marks.browser_push_confirmed;
assert.throws(() => assertWarmCommandBudget(incomplete), /invalid measured mark/);
console.log('warm command budget rejection smoke OK (synthetic guard fixtures only)');
