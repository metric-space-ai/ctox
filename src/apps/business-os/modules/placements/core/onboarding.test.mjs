import assert from 'node:assert/strict';
import { test } from 'node:test';

import { checklistProgress, evaluateOnboardingGate, normalizeChecklist } from './onboarding.js';

test('normalizeChecklist coerces to booleans for every item', () => {
  const c = normalizeChecklist({ documents_complete: 1, psa_issued: true });
  assert.equal(c.documents_complete, true);
  assert.equal(c.sicherheitsunterweisung, false);
});

test('checklistProgress counts done vs total', () => {
  const p = checklistProgress({ documents_complete: true, sicherheitsunterweisung: true });
  assert.equal(p.total, 4);
  assert.equal(p.done, 2);
  assert.equal(p.complete, false);
});

test('onboarding gate requires all required items', () => {
  assert.equal(evaluateOnboardingGate({ documents_complete: true }).allowed, false);
  assert.equal(evaluateOnboardingGate({ documents_complete: true, sicherheitsunterweisung: true }).allowed, true);
});
