import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { ensureDesktopLayoutWithAuthority, isDatabaseClosingError } from '../layout-authority.js?v=20260919-layout-boundary-v2';

const defaults = { wallpaper_url: '', taskbar_pins: ['ctox'] };

function fixture(stage, failure) {
  let currentFailure = failure;
  let inserts = 0;
  const closingErrors = [];
  const saved = { id: 'layout', taskbar_pins: ['documents'] };
  const db = { findOne() {
    if (stage === 'findOne' && currentFailure) throw currentFailure;
    return { exec: async () => {
      if (stage === 'exec' && currentFailure) throw currentFailure;
      return { toJSON: () => saved };
    } };
  } };
  const mount = () => ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout: () => defaults,
    readNativeDocument: async () => null,
    insertMissingSeed: async () => {
      inserts += 1;
      if (stage === 'insert' && currentFailure) throw currentFailure;
    },
    onDatabaseClosing: error => closingErrors.push(error),
  });
  return { mount, recover: () => { currentFailure = null; }, saved, closingErrors, inserts: () => inserts };
}

for (const stage of ['insert', 'findOne', 'exec']) {
  test(`layout resolver tolerates closing during ${stage} and adopts saved layout after recovery`, async () => {
    // Plain objects model cross-realm IndexedDB errors, not only DOMException.
    const failure = { message: 'IDBDatabase connection is closing' };
    const f = fixture(stage, failure);
    assert.deepEqual(await f.mount(), defaults);
    assert.deepEqual(f.closingErrors, [failure]);
    assert.equal(f.inserts(), 1, 'fallback does not retry writes');
    f.recover();
    assert.deepEqual(await f.mount(), f.saved);
    assert.deepEqual(f.closingErrors, [failure], 'recovery does not report another fallback');
  });
  test(`layout resolver preserves non-closing ${stage} errors`, async () => {
    const failure = new Error('permission denied');
    const f = fixture(stage, failure);
    await assert.rejects(f.mount, error => error === failure);
    assert.equal(f.inserts(), 1);
    assert.deepEqual(f.closingErrors, []);
  });
}

test('native rejection remains unknown authority and performs no local work', async () => {
  for (const failure of [new Error('permission denied'), { message: 'database connection is closing' }]) {
    const result = await ensureDesktopLayoutWithAuthority({
      collection: { findOne: () => assert.fail('unknown authority must not read local state') },
      defaultLayout: () => defaults,
      readNativeDocument: async () => { throw failure; },
      insertMissingSeed: () => assert.fail('unknown authority must not seed'),
      onDatabaseClosing: () => assert.fail('native rejection is not a local restart fallback'),
    });
    assert.deepEqual(result, defaults);
  }
});

test('conflict without a winner preserves the original conflict', async () => {
  const conflict = { status: 409, message: 'conflict' };
  await assert.rejects(() => ensureDesktopLayoutWithAuthority({
    collection: { findOne: () => ({ exec: async () => null }) },
    defaultLayout: () => defaults,
    readNativeDocument: async () => null,
    insertMissingSeed: async () => { throw conflict; },
  }), error => error === conflict);
});

test('shared closing classifier accepts cross-realm errors and rejects unrelated failures', () => {
  assert.equal(isDatabaseClosingError({ message: 'IDBDatabase connection is closing' }), true);
  assert.equal(isDatabaseClosingError('database connection is closing'), true);
  assert.equal(isDatabaseClosingError(new Error('permission denied')), false);
  assert.equal(isDatabaseClosingError(null), false);
});

test('desktop imports the new export surface through a fresh module URL', async () => {
  const indexSource = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
  const match = indexSource.match(/import \{([^}]+)\} from '(\.\/layout-authority\.js[^']*)'/);
  assert.ok(match, 'desktop must use the shared module');
  const requested = new URL(match[2], new URL('../index.js', import.meta.url));
  const previouslyCached = new URL('../layout-authority.js', import.meta.url);
  assert.notEqual(requested.href, previouslyCached.href, 'old cached export surface must not satisfy the new import');
  assert.equal(requested.search, '?v=20260919-layout-boundary-v2');
  const actualModule = await import(requested.href);
  for (const imported of match[1].split(',').map(value => value.trim())) {
    assert.equal(typeof actualModule[imported], 'function', `missing production export ${imported}`);
  }
});
