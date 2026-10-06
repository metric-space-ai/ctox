import assert from 'node:assert/strict';
import test from 'node:test';
import { openRecoveryJournal } from '../src/recovery-journal.mjs';
import { openCtoxIndexedDbStorage } from '../src/storage-indexeddb.mjs';

async function withOpenHarness(run) {
  const original = {
    indexedDB: globalThis.indexedDB,
    setTimeout: globalThis.setTimeout,
    clearTimeout: globalThis.clearTimeout,
  };
  const requests = [];
  const timers = new Map();
  let timerId = 0;
  globalThis.indexedDB = {
    open(name, version) {
      const request = { name, version, result: null, error: null };
      requests.push(request);
      return request;
    },
  };
  globalThis.setTimeout = (callback, delay) => {
    const id = ++timerId;
    timers.set(id, { callback, delay });
    return id;
  };
  globalThis.clearTimeout = (id) => timers.delete(id);
  const database = (name, closeError = null) => ({
    name,
    version: 4,
    closes: 0,
    close() {
      this.closes += 1;
      if (closeError) throw closeError;
    },
  });
  const succeed = (request, db) => {
    request.result = db;
    request.onsuccess();
  };
  try {
    await run({ requests, timers, database, succeed });
  } finally {
    Object.assign(globalThis, original);
  }
}

async function flushOpenContinuation() {
  await Promise.resolve();
  await Promise.resolve();
}

test('a successful journal open retires its deadline and remains caller-owned', async () => {
  await withOpenHarness(async ({ requests, timers, database, succeed }) => {
    const opened = openRecoveryJournal({ databaseName: 'journal-success' });
    const db = database(requests[0].name);
    succeed(requests[0], db);
    const journal = await opened;
    assert.equal(journal.db, db);
    assert.equal(timers.size, 0);
    assert.equal(db.closes, 0);
    db.onversionchange();
    assert.equal(db.closes, 1);
  });
});

test('a journal handle that succeeds after a blocked rejection is closed', async () => {
  await withOpenHarness(async ({ requests, timers, database, succeed }) => {
    const rejected = assert.rejects(openRecoveryJournal({ databaseName: 'journal-blocked' }), {
      code: 'indexeddb_journal_unavailable',
    });
    requests[0].onblocked();
    await rejected;
    assert.equal(timers.size, 0);
    const late = database(requests[0].name);
    succeed(requests[0], late);
    assert.equal(late.closes, 1);
  });
});

test('a stalled journal open has a bounded deadline and closes late success', async () => {
  await withOpenHarness(async ({ requests, timers, database, succeed }) => {
    const rejected = assert.rejects(openRecoveryJournal({ databaseName: 'journal-stalled' }), (error) => {
      assert.equal(error.code, 'indexeddb_journal_unavailable');
      assert.match(error.message, /timed out/);
      return true;
    });
    assert.equal(timers.size, 1, 'journal open must have its own deadline');
    const deadline = [...timers.values()][0];
    assert.equal(deadline.delay, 4000);
    deadline.callback();
    await rejected;
    assert.equal(timers.size, 0);
    const late = database(requests[0].name);
    succeed(requests[0], late);
    assert.equal(late.closes, 1);
  });
});

test('failed journal startup releases the already opened primary handle', async () => {
  await withOpenHarness(async ({ requests, timers, database, succeed }) => {
    const rejected = assert.rejects(openCtoxIndexedDbStorage({ databaseName: 'primary-journal-blocked' }), {
      code: 'indexeddb_journal_unavailable',
    });
    const primary = database(requests[0].name);
    succeed(requests[0], primary);
    await flushOpenContinuation();
    assert.equal(requests.length, 2);
    requests[1].onblocked();
    await rejected;
    assert.equal(primary.closes, 1);
    assert.equal(timers.size, 0);
    const late = database(requests[1].name);
    succeed(requests[1], late);
    assert.equal(late.closes, 1);
  });
});

test('primary cleanup cannot replace the original journal-open error', async () => {
  await withOpenHarness(async ({ requests, timers, database, succeed }) => {
    const originalError = new Error('journal request failed');
    const rejected = assert.rejects(openCtoxIndexedDbStorage({ databaseName: 'primary-journal-error' }), (error) => {
      assert.equal(error, originalError);
      return true;
    });
    const primary = database(requests[0].name, new Error('primary close failed'));
    succeed(requests[0], primary);
    await flushOpenContinuation();
    requests[1].error = originalError;
    requests[1].onerror();
    await rejected;
    assert.equal(primary.closes, 1);
    assert.equal(timers.size, 0);
  });
});
