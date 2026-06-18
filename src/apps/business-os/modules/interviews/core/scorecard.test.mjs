import assert from 'node:assert/strict';
import { test } from 'node:test';

import { isScorecardComplete, normalizeScorecard, scoreScorecard } from './scorecard.js';

const def = {
  id: 'sc1',
  role_template: 'pflege',
  criteria: [
    { key: 'fachkompetenz', label: 'Fachkompetenz', weight: 2, scaleMax: 5 },
    { key: 'kommunikation', label: 'Kommunikation', scaleMax: 5 },
  ],
};

test('normalizeScorecard fills defaults and drops invalid criteria', () => {
  const card = normalizeScorecard({ criteria: [{ key: 'a' }, { label: 'no key' }] });
  assert.equal(card.criteria.length, 1);
  assert.equal(card.criteria[0].weight, 1);
  assert.equal(card.criteria[0].scaleMax, 5);
});

test('isScorecardComplete requires every criterion rated', () => {
  assert.ok(!isScorecardComplete(def, { fachkompetenz: 4 }));
  assert.ok(isScorecardComplete(def, { fachkompetenz: 4, kommunikation: 3 }));
});

test('scoreScorecard computes weighted 0..100 with breakdown', () => {
  const result = scoreScorecard(def, { fachkompetenz: 5, kommunikation: 0 });
  // fach normalized 1.0 weight 2, komm 0.0 weight 1 -> (2/3) -> 67
  assert.equal(result.overall, 67);
  assert.equal(result.complete, true);
  assert.equal(result.breakdown.length, 2);
});

test('scoreScorecard tolerates partial ratings', () => {
  const result = scoreScorecard(def, { fachkompetenz: 4 });
  assert.equal(result.complete, false);
  assert.ok(result.overall > 0);
});
