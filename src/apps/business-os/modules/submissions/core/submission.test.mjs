import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  evaluateSubmissionGuard,
  findDoubleSubmission,
  normalizeFeedback,
} from './submission.js';

const DAY = 24 * 60 * 60 * 1000;
const NOW = Date.UTC(2026, 5, 1);

const existing = [
  { id: 's1', candidate_id: 'c1', client_account_id: 'a1', sent_at_ms: NOW - 10 * DAY, status: 'sent' },
  { id: 's2', candidate_id: 'c1', client_account_id: 'a2', sent_at_ms: NOW - 10 * DAY, status: 'withdrawn' },
];

test('findDoubleSubmission catches same candidate→same client in window', () => {
  assert.ok(findDoubleSubmission(existing, { candidate_id: 'c1', client_account_id: 'a1' }, { nowMs: NOW }));
  assert.equal(findDoubleSubmission(existing, { candidate_id: 'c1', client_account_id: 'a3' }, { nowMs: NOW }), null);
  // withdrawn does not conflict
  assert.equal(findDoubleSubmission(existing, { candidate_id: 'c1', client_account_id: 'a2' }, { nowMs: NOW }), null);
  // outside window
  assert.equal(
    findDoubleSubmission(existing, { candidate_id: 'c1', client_account_id: 'a1' }, { withinDays: 5, nowMs: NOW }),
    null,
  );
});

test('evaluateSubmissionGuard blocks on conflict or missing consent', () => {
  const blocked = evaluateSubmissionGuard(
    { candidate_id: 'c1', client_account_id: 'a1' },
    { existingSubmissions: existing, hasConsent: true, nowMs: NOW },
  );
  assert.equal(blocked.allowed, false);
  assert.ok(blocked.blockers.some((b) => b.reason === 'double_submission'));

  const noConsent = evaluateSubmissionGuard(
    { candidate_id: 'c9', client_account_id: 'a9' },
    { existingSubmissions: existing, hasConsent: false, nowMs: NOW },
  );
  assert.equal(noConsent.allowed, false);
  assert.ok(noConsent.blockers.some((b) => b.reason === 'consent_to_present_missing'));

  const ok = evaluateSubmissionGuard(
    { candidate_id: 'c9', client_account_id: 'a9' },
    { existingSubmissions: existing, hasConsent: true, nowMs: NOW },
  );
  assert.equal(ok.allowed, true);
});

test('normalizeFeedback coerces outcome vocabulary', () => {
  assert.equal(normalizeFeedback({ outcome: 'interview', submission_id: 's1' }).outcome, 'interview');
  assert.equal(normalizeFeedback({ outcome: 'maybe' }).outcome, 'no_response');
});
