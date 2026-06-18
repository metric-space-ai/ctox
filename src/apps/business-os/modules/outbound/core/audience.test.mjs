import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  buildCandidateAudience,
  isCandidateChannel,
  matchesSavedSearch,
} from './audience.js';

test('isCandidateChannel validates channels', () => {
  assert.ok(isCandidateChannel('whatsapp'));
  assert.ok(!isCandidateChannel('carrier_pigeon'));
});

test('matchesSavedSearch ANDs criteria including tags', () => {
  const candidate = { skills: ['Pflege', 'Stapler'], tags: ['silver_medalist'], years: 5 };
  assert.ok(
    matchesSavedSearch(candidate, {
      criteria: [
        { field: 'skills', op: 'includes', value: 'pflege' },
        { op: 'has_tag', value: 'silver_medalist' },
        { field: 'years', op: 'gte', value: 3 },
      ],
    }),
  );
  assert.ok(!matchesSavedSearch(candidate, { criteria: [{ op: 'has_tag', value: 'ex_zeitarbeitnehmer' }] }));
  assert.ok(!matchesSavedSearch(candidate, { criteria: [] }), 'empty search matches nothing');
});

test('buildCandidateAudience filters by reachability, suppression and search', () => {
  const pool = [
    { id: 'a', email: 'a@x.de', tags: ['silver_medalist'] },
    { id: 'b', tags: ['silver_medalist'] }, // no email
    { id: 'c', email: 'c@x.de', tags: [] },
    { id: 'd', email: 'd@x.de', tags: ['silver_medalist'] },
  ];
  const audience = buildCandidateAudience(pool, {
    channel: 'email',
    suppressedIds: ['d'],
    savedSearch: { criteria: [{ op: 'has_tag', value: 'silver_medalist' }] },
  });
  assert.deepEqual(audience.map((c) => c.id), ['a']);
});

test('buildCandidateAudience needs phone for whatsapp/sms', () => {
  const pool = [{ id: 'a', email: 'a@x.de' }, { id: 'b', phone: '+49...' }];
  assert.deepEqual(buildCandidateAudience(pool, { channel: 'sms' }).map((c) => c.id), ['b']);
});
