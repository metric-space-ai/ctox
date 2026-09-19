import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { ensureDesktopLayoutWithAuthority } from '../layout-authority.js';

// Exercise the actual mount wrapper together with the native-authority helper.
const source = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
const wrapper = source.slice(source.indexOf('  async function ensureLayout('), source.indexOf('  function defaultLayout('));
const classifier = source.slice(source.indexOf('  function isDatabaseClosingError('), source.indexOf('  function showManagedAuthorizationError('));
const defaults = { wallpaper_url: '', taskbar_pins: ['ctox'] };

function fixture(stage, failure) {
  let currentFailure = failure;
  let inserts = 0;
  const saved = { id: 'layout', taskbar_pins: ['documents'] };
  const mount = runInNewContext(`${classifier}\n${wrapper}\nensureLayout`, {
    ensureDesktopLayoutWithAuthority,
    LAYOUT_DOC_ID: 'layout',
    defaultLayout: () => defaults,
    ctx: { readNativeCollectionDocument: async () => null },
    insertMissingSeed: async () => {
      inserts += 1;
      if (stage === 'insert' && currentFailure) throw currentFailure;
    },
    console: { info() {} },
  });
  const db = { findOne() {
    if (stage === 'findOne' && currentFailure) throw currentFailure;
    return { exec: async () => {
      if (stage === 'exec' && currentFailure) throw currentFailure;
      return { toJSON: () => saved };
    } };
  } };
  return { mount: () => mount(db, {}), recover: () => { currentFailure = null; }, saved, inserts: () => inserts };
}

for (const stage of ['insert', 'findOne', 'exec']) {
  test(`desktop mount tolerates closing during ${stage} and adopts saved layout after recovery`, async () => {
    // Plain objects model cross-realm IndexedDB errors, not only DOMException.
    const f = fixture(stage, { message: 'IDBDatabase connection is closing' });
    assert.deepEqual(await f.mount(), defaults);
    assert.equal(f.inserts(), 1, 'fallback does not retry writes');
    f.recover();
    assert.deepEqual(await f.mount(), f.saved);
  });
  test(`desktop mount preserves non-closing ${stage} errors`, async () => {
    const failure = new Error('permission denied');
    const f = fixture(stage, failure);
    await assert.rejects(f.mount, (error) => error === failure);
    assert.equal(f.inserts(), 1);
  });
}
