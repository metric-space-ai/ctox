import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  avgTimeToFillDays,
  countByStage,
  fillRate,
  sourceOfHire,
  stageConversions,
} from './funnel.js';

const DAY = 24 * 60 * 60 * 1000;
const stages = ['neu', 'screening', 'kundenvorstellung', 'eingestellt'];

test('countByStage tallies known stages', () => {
  const counts = countByStage([{ stage: 'neu' }, { stage: 'neu' }, { stage: 'eingestellt' }, { stage: 'x' }], stages);
  assert.equal(counts.neu, 2);
  assert.equal(counts.eingestellt, 1);
});

test('stageConversions computes step ratios', () => {
  const records = [{ stage: 'neu' }, { stage: 'neu' }, { stage: 'screening' }, { stage: 'kundenvorstellung' }, { stage: 'eingestellt' }];
  const conv = stageConversions(records, stages);
  assert.equal(conv.length, 3);
  assert.equal(conv[0].rate, 50); // screening(1)/neu(2)
});

test('fillRate and avgTimeToFillDays', () => {
  const vac = [
    { status: 'filled', opened_at_ms: 0, filled_at_ms: 10 * DAY },
    { status: 'open', opened_at_ms: 0 },
  ];
  assert.equal(fillRate(vac), 50);
  assert.equal(avgTimeToFillDays(vac), 10);
});

test('sourceOfHire groups hires by channel', () => {
  const recs = [{ stage: 'eingestellt', source: 'stepstone' }, { stage: 'eingestellt', source: 'stepstone' }, { stage: 'neu', source: 'indeed' }];
  assert.deepEqual(sourceOfHire(recs), { stepstone: 2 });
});
