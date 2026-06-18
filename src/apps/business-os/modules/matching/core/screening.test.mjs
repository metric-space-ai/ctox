import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  assertNonDiscriminatory,
  buildScreeningAudit,
  evaluateKnockouts,
  isProtectedAttribute,
  isValidRejectionReason,
  rankShortlist,
} from './screening.js';

test('isProtectedAttribute catches AGG attributes', () => {
  assert.ok(isProtectedAttribute('geschlecht'));
  assert.ok(isProtectedAttribute('candidate.age'));
  assert.ok(isProtectedAttribute('Religion'));
  assert.ok(!isProtectedAttribute('staplerschein'));
  assert.ok(!isProtectedAttribute('location'));
});

test('assertNonDiscriminatory throws on a protected-attribute rule', () => {
  assert.throws(() => assertNonDiscriminatory([{ field: 'alter', op: 'gte', value: 18 }]), /AGG/);
  assert.ok(assertNonDiscriminatory([{ field: 'skills', op: 'includes', value: 'pflege' }]));
});

test('evaluateKnockouts enforces must-have rules', () => {
  const object = { skills: ['Pflege', 'Stapler'], location: 'Berlin', years: 5 };
  const rules = [
    { field: 'skills', op: 'includes', value: 'stapler', label: 'Staplerschein' },
    { field: 'years', op: 'gte', value: 3 },
  ];
  assert.equal(evaluateKnockouts(object, rules).passed, true);

  const fail = evaluateKnockouts({ skills: ['Pflege'], years: 1 }, rules);
  assert.equal(fail.passed, false);
  assert.equal(fail.failed.length, 2);
  assert.equal(fail.failed[0].reasonCode, 'knockout_unmet');
});

test('evaluateKnockouts skips protected-attribute rules (defense in depth)', () => {
  const result = evaluateKnockouts({ skills: [] }, [{ field: 'geschlecht', op: 'equals', value: 'm' }]);
  assert.equal(result.passed, true);
  assert.equal(result.failed.length, 0);
});

test('rankShortlist ranks by score, excludes knocked-out, caps at topN', () => {
  const scored = [
    { objectId: 'a', score: 70, evaluated: true },
    { objectId: 'b', score: 90, evaluated: true },
    { objectId: 'c', score: 95, evaluated: true, knockoutFailed: true },
    { objectId: 'd', score: null, evaluated: false },
  ];
  const shortlist = rankShortlist(scored, { topN: 2 });
  assert.equal(shortlist.length, 2);
  assert.equal(shortlist[0].objectId, 'b');
  assert.equal(shortlist[0].rank, 1);
  assert.equal(shortlist[1].objectId, 'a');
  assert.ok(!shortlist.some((s) => s.objectId === 'c'), 'knocked-out excluded');
});

test('rankShortlist puts unevaluated below evaluated with a clear reason', () => {
  const shortlist = rankShortlist(
    [
      { objectId: 'x', score: null, evaluated: false },
      { objectId: 'y', score: 40, evaluated: true },
    ],
    { topN: 5 },
  );
  assert.equal(shortlist[0].objectId, 'y');
  assert.match(shortlist[1].reason, /Noch nicht bewertet/);
});

test('rejection reason vocabulary is controlled', () => {
  assert.ok(isValidRejectionReason('knockout_unmet'));
  assert.ok(!isValidRejectionReason('too_old'));
});

test('buildScreeningAudit records a defensible, immutable decision', () => {
  const audit = buildScreeningAudit({
    requirementId: 'r1',
    objectId: 'o1',
    decision: 'rejected',
    reasonCode: 'experience_gap',
    actor: 'recruiter:42',
    atMs: 1000,
  });
  assert.equal(audit.kind, 'screening_decision');
  assert.equal(audit.reason_code, 'experience_gap');
  assert.equal(audit.reason_label, 'Berufserfahrung passt nicht ausreichend');
  assert.equal(audit.immutable, true);

  const advanced = buildScreeningAudit({ requirementId: 'r', objectId: 'o', decision: 'advanced', atMs: 2 });
  assert.equal(advanced.reason_code, null, 'no rejection reason on advance');

  const bogus = buildScreeningAudit({ requirementId: 'r', objectId: 'o', decision: 'rejected', reasonCode: 'too_old', atMs: 3 });
  assert.equal(bogus.reason_code, 'other_better_fit', 'illegal reason coerced to safe default');
});
