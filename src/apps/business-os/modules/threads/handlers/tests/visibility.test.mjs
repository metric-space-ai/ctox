// Tests: _visibility.js (threads-Zweig der Replikationssichtbarkeit, threads.rs:575-620, :4787-4812).
import test from 'node:test';
import assert from 'node:assert/strict';
import visible, { threadRecordVisibleToUser } from '../_visibility.js';

const threads = { t1: { id: 't1', owner_user_id: 'alice', participant_ids: ['bob'], watcher_user_ids: ['wally'], assigned_user_id: 'ann' } };
const read = async (c, id) => (c === 'user_threads' ? threads[id] ?? null : null);

test('user_threads: Besitzer, Ersteller, Zuständige, Teilnehmer, Beobachter', () => {
  for (const u of ['alice', 'bob', 'wally', 'ann']) assert.equal(threadRecordVisibleToUser(threads.t1, u), true, u);
  assert.equal(threadRecordVisibleToUser(threads.t1, 'eve'), false);
  assert.equal(threadRecordVisibleToUser(threads.t1, ''), false);
});

test('Kinder-Collections über thread_id; persönliche nur Besitzer', async () => {
  assert.equal(await visible('user_thread_links', { thread_id: 't1' }, 'bob', read), true);
  assert.equal(await visible('user_thread_links', { thread_id: 't1' }, 'eve', read), false);
  assert.equal(await visible('user_thread_links', { thread_id: '' }, 'bob', read), false);
  assert.equal(await visible('user_thread_messages', { thread_id: 'nope', target_user_ids: ['eve'] }, 'eve', read), true);
  assert.equal(await visible('user_thread_messages', { thread_id: 'nope', author_user_id: 'eve' }, 'eve', read), true);
  assert.equal(await visible('user_thread_messages', { thread_id: 'nope' }, 'eve', read), false);
  assert.equal(await visible('ctox_task_approval_requests', { thread_id: 'x', reviewer_user_id: 'eve' }, 'eve', read), true);
  assert.equal(await visible('ctox_task_approval_requests', { thread_id: 't1' }, 'wally', read), true);
  assert.equal(await visible('user_notifications', { user_id: 'bob', thread_id: 't1' }, 'alice', read), false);
  assert.equal(await visible('user_thread_states', { user_id: 'bob' }, 'bob', read), true);
  assert.equal(await visible('notes', {}, 'bob', read), false);
  const throwing = async () => { throw new Error('io'); };
  assert.equal(await visible('user_thread_links', { thread_id: 't1' }, 'bob', throwing), false);
});
