import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  applicationDedupeKey,
  findDuplicateApplication,
  normalizeApplication,
} from './application.js';

test('normalizeApplication produces one shape from any channel', () => {
  const app = normalizeApplication({
    channel: 'job_board',
    firstName: 'Ada',
    lastName: 'Lovelace',
    email: 'ADA@x.de',
    documents: [{ kind: 'cv', file_id: 'f1' }, { kind: 'cover', file_id: '' }],
  });
  assert.equal(app.channel, 'job_board');
  assert.equal(app.candidate.name, 'Ada Lovelace');
  assert.equal(app.candidate.email, 'ada@x.de');
  assert.equal(app.documents.length, 1);
  assert.equal(app.status, 'new');
});

test('normalizeApplication defaults unknown channel to email', () => {
  assert.equal(normalizeApplication({ channel: 'pigeon' }).channel, 'email');
});

test('dedupe keys prefer email then name', () => {
  assert.equal(applicationDedupeKey({ candidate: { email: 'A@x.de' } }), 'email:a@x.de');
  assert.equal(applicationDedupeKey({ candidate: { name: 'Ada' } }), 'name:ada');
  assert.equal(applicationDedupeKey({}), '');
});

test('findDuplicateApplication matches on dedupe key', () => {
  const existing = [normalizeApplication({ email: 'a@x.de', name: 'Ada' })];
  assert.ok(findDuplicateApplication(existing, normalizeApplication({ email: 'A@X.de' })));
  assert.equal(findDuplicateApplication(existing, normalizeApplication({ email: 'b@x.de' })), null);
});
