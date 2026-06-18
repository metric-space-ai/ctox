import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  CANDIDATE_STAGES,
  DEFAULT_CANDIDATE_STAGE,
  canTransitionCandidate,
  candidateStageLabel,
  groupByCandidateStage,
  isCandidateStage,
  normalizeCandidateStage,
  normalizeJobOrderHeader,
  summarizeJobOrder,
  withCandidateStage,
} from './pipeline.js';

test('candidate stages are ordered and unique', () => {
  const keys = CANDIDATE_STAGES.map((s) => s.key);
  assert.equal(new Set(keys).size, keys.length);
  assert.ok(isCandidateStage('screening'));
  assert.ok(!isCandidateStage('not-a-stage'));
  assert.equal(candidateStageLabel('eingestellt'), 'Eingestellt');
});

test('normalizeCandidateStage prefers explicit structured stage', () => {
  assert.equal(
    normalizeCandidateStage({ data: { pipeline: { stage: 'kundenvorstellung' } } }),
    'kundenvorstellung',
  );
});

test('normalizeCandidateStage falls back to legacy status / hashtag canonical', () => {
  assert.equal(normalizeCandidateStage({ status: 'interview' }), 'telefoninterview');
  assert.equal(normalizeCandidateStage({}, 'hired'), 'eingestellt');
  assert.equal(normalizeCandidateStage({ status: 'screening' }), 'screening');
});

test('normalizeCandidateStage defaults when nothing matches', () => {
  assert.equal(normalizeCandidateStage({}), DEFAULT_CANDIDATE_STAGE);
  assert.equal(normalizeCandidateStage({ status: 'garbage' }), DEFAULT_CANDIDATE_STAGE);
});

test('canTransitionCandidate rejects no-op and unknown target', () => {
  assert.ok(canTransitionCandidate('neu', 'screening'));
  assert.ok(canTransitionCandidate('eingestellt', 'neu'));
  assert.ok(!canTransitionCandidate('neu', 'neu'));
  assert.ok(!canTransitionCandidate('neu', 'bogus'));
});

test('withCandidateStage is pure and stamps the change time', () => {
  const data = { pipeline: { stage: 'neu', note: 'x' }, other: 1 };
  const next = withCandidateStage(data, 'screening', 1234);
  assert.equal(next.pipeline.stage, 'screening');
  assert.equal(next.pipeline.stage_changed_at_ms, 1234);
  assert.equal(next.other, 1);
  assert.equal(data.pipeline.stage, 'neu', 'input not mutated');
});

test('groupByCandidateStage buckets all stages in order', () => {
  const groups = groupByCandidateStage([
    { data: { pipeline: { stage: 'screening' } } },
    { status: 'hired' },
    {},
  ]);
  assert.deepEqual(
    groups.map((g) => g.key),
    CANDIDATE_STAGES.map((s) => s.key),
  );
  const byKey = Object.fromEntries(groups.map((g) => [g.key, g.items.length]));
  assert.equal(byKey.screening, 1);
  assert.equal(byKey.eingestellt, 1);
  assert.equal(byKey.neu, 1);
});

test('normalizeJobOrderHeader coerces and drops unknown/empty fields', () => {
  const header = normalizeJobOrderHeader({
    department: '  Pflege ',
    headcount: '3',
    location: '',
    contract_type: 'zeitarbeit',
    bogus: 'ignored',
  });
  assert.deepEqual(header, {
    department: 'Pflege',
    headcount: 3,
    contract_type: 'zeitarbeit',
  });
});

test('summarizeJobOrder renders a compact line', () => {
  assert.equal(
    summarizeJobOrder({ account_label: 'ACME', location: 'Berlin', headcount: 2, contract_type: 'zeitarbeit' }),
    'ACME · Berlin · 2× · zeitarbeit',
  );
  assert.equal(summarizeJobOrder(null), '');
});
