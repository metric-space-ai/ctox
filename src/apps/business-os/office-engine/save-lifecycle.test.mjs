import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createOfficeSaveTracker, __ctoxForkTestHooks } from './src/runtime/ctox-fork-core.mjs';

test('save acknowledges only the serialized revision, preserving later typing', () => {
  const state = createOfficeSaveTracker();
  state.edit();
  const saved = state.snapshot();
  state.edit();
  assert.equal(state.acknowledge(saved), false);
  assert.equal(state.dirty, true);
  assert.equal(state.acknowledge(state.snapshot()), true);
  assert.equal(state.dirty, false);
});

test('failed save remains dirty until a successful acknowledgement', () => {
  const state = createOfficeSaveTracker();
  state.edit();
  const saved = state.snapshot();
  state.fail();
  assert.equal(state.dirty, true);
  state.acknowledge(saved);
  assert.equal(state.dirty, false);
});

test('late acknowledgement cannot clear edits in another opened document', () => {
  const state = createOfficeSaveTracker();
  state.edit();
  const previous = state.snapshot();
  state.reset();
  state.edit();
  assert.equal(state.acknowledge(previous), false);
  assert.equal(state.dirty, true);
});

// Run the actual production save functions and XLSY decoder. Only the iframe,
// SDK serializer/event delivery and native persistence are controlled doubles.
// This is not a full SDK or live WebRTC/native integration test.
const source = readFileSync(new URL('./src/runtime/ctox-fork-core.mjs', import.meta.url), 'utf8');
function section(start, end) {
  const from = source.indexOf(start);
  assert.notEqual(from, -1, `Missing source boundary: ${start}`);
  const to = source.indexOf(end, from + start.length);
  assert.notEqual(to, -1, `Missing source boundary: ${end}`);
  return source.slice(from, to);
}
const lifecycle = section('  function createPendingSave(reason)', '  function inspection()');
const adapter = section('function installCtoxSdkAdapter(', 'function installCtoxForkUi(');
const decoder = section('function decodeSpreadsheetNativeFile(', 'async function waitForFullSdk(');
const manualSave = 'function ' + section("    save({ reason = 'manual' } = {}) {", '    async export(')
  .trim().replace(/,$/, '');

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function saveHarness(kind) {
  const state = createOfficeSaveTracker();
  const events = [], commits = [], deliveries = [];
  let cleanCalls = 0, fallbackCalls = 0, serializerCalls = 0;
  // One real XLSY directory entry, body-relative offset 6; decoder must shift
  // it past the eleven-byte v10 header before sending the commit.
  const sheetBody = Uint8Array.of(1, 1, 6, 0, 0, 0, 0);
  const goodPayload = () => kind === 'document'
    ? Buffer.from('DOCY;v10;4;test').toString('base64')
    : `XLSY;v2;${sheetBody.length};${Buffer.from(sheetBody).toString('base64')}`;
  let serialize = goodPayload;
  function Api() {}
  Api.prototype.asc_Save = () => { fallbackCalls += 1; };
  const api = new Api();
  const serializerName = kind === 'document' ? 'asc_nativeGetFile2' : 'asc_nativeGetFile';
  const nativeSerialize = () => { serializerCalls += 1; return serialize(); };
  api[serializerName] = nativeSerialize;
  api.SetDocumentModified = value => { if (!value) cleanCalls += 1; };
  api.SetUnchangedDocument = () => { if (kind === 'document') cleanCalls += 1; };
  const upstream = { Asc: { asc_docs_api: Api, spreadsheet_api: Api, editor: api }, atob };
  const bridge = { commit(request) {
    const completion = deferred();
    commits.push({ request, ...completion });
    return completion.promise;
  } };
  const runtime = new Function('saveTracker', 'bridge', 'emit', 'kind', 'upstream',
    'hasEditorBinarySignature', 'waitForFullSdk', `
      let destroyed = false, documentReady = true, access = { write: true };
      let pendingSave = null, recordId = 'record', versionId = 'v1', editorBytes = null;
      const isDocument = kind === 'document';
      const productName = isDocument ? 'CTOX Documents' : 'CTOX Spreadsheets';
      const editorProtocol = isDocument ? 'euro-office-word-binary-v10' : 'euro-office-cell-binary-v10';
      const editorProtocolVersion = 10;
      const frame = { contentWindow: upstream };
      const permissionError = message => new Error(message);
      const normalizeBytes = value => {
        if (!(value instanceof Uint8Array)) throw new TypeError('Expected binary');
        return value;
      };
      ${lifecycle}
      ${decoder}
      ${adapter}
      ${manualSave}
      const fail = typeof failPendingSave === 'function' ? failPendingSave : undefined;
      installCtoxSdkAdapter(upstream, kind, beginSdkSave, fail);
      return {
        save, acceptSavedBinary, fail,
        pending: () => pendingSave,
        writable: value => { access.write = value; },
        ready: value => { documentReady = value; },
      };
    `)(state, bridge, (event, data) => events.push({ event, data }), kind, upstream,
      __ctoxForkTestHooks.hasEditorBinarySignature, () => Promise.resolve());
  api.sendEvent = (event, bytes) => {
    assert.equal(event, 'asc_onSaveDocument');
    deliveries.push(Promise.resolve().then(() => runtime.acceptSavedBinary(bytes)));
  };
  return {
    ...runtime, state, events, commits, api, serializerName,
    serialize: next => { serialize = next; },
    restore: () => { serialize = goodPayload; api[serializerName] = nativeSerialize; },
    flush: () => Promise.resolve(), drain: () => Promise.all(deliveries),
    cleanCalls: () => cleanCalls, fallbackCalls: () => fallbackCalls,
    serializerCalls: () => serializerCalls,
    errors: () => events.filter(({ event }) => event === 'error'),
  };
}

for (const kind of ['document', 'spreadsheet']) {
  test(`${kind}: delayed toolbar commit coalesces and preserves later edits`, async () => {
    const h = saveHarness(kind);
    h.state.edit();
    assert.equal(h.api.asc_Save(), true);
    const pending = h.pending();
    assert.equal(h.save(), pending.promise);
    assert.equal(h.api.asc_Save(), false);
    assert.equal(h.serializerCalls(), 1);
    assert.equal(h.cleanCalls(), 0);
    await h.flush();
    assert.equal(h.commits.length, 1);
    assert.equal(h.commits[0].request.reason, 'toolbar');
    if (kind === 'spreadsheet') {
      const bytes = h.commits[0].request.bytes;
      assert.equal(new TextDecoder().decode(bytes.slice(0, 11)), 'XLSY;v10;0;');
      assert.equal(new DataView(bytes.buffer).getUint32(13, true), 17);
    }
    h.state.edit();
    h.commits[0].resolve({ version_id: 'v2' });
    assert.equal((await pending.promise).dirty, true);
    await h.drain();
    assert.equal(h.cleanCalls(), 0);
    assert.equal(h.pending(), null);
    const retry = h.save();
    await h.flush();
    assert.equal(h.commits[1].request.baseVersionId, 'v2');
    h.commits[1].resolve({ version_id: 'v3' });
    assert.equal((await retry).dirty, false);
    await h.drain();
    assert.equal(h.cleanCalls(), 1);
    assert.equal(h.errors().length, 0);
  });

  test(`${kind}: rejected toolbar commit remains dirty and can retry`, async () => {
    const h = saveHarness(kind);
    h.state.edit();
    h.api.asc_Save();
    const original = new Error('durable commit rejected');
    const rejected = assert.rejects(h.pending().promise, error => error === original);
    await h.flush();
    assert.equal(h.cleanCalls(), 0);
    h.commits[0].reject(original);
    await rejected;
    await h.drain();
    assert.equal(h.pending(), null);
    assert.equal(h.state.dirty, true);
    assert.equal(h.cleanCalls(), 0);
    assert.equal(h.errors().length, 1);
    const retry = h.save();
    await h.flush();
    h.commits[1].resolve({ version_id: 'v2' });
    await retry;
    await h.drain();
    assert.equal(h.state.dirty, false);
    assert.equal(h.cleanCalls(), 1);
  });

  for (const failure of ['throw', 'missing', 'wrong-type', 'empty', 'bad-signature', 'bad-base64']) {
    for (const entry of ['toolbar', 'manual']) {
      test(`${kind}: ${entry} ${failure} fails closed and permits retry`, async () => {
        const h = saveHarness(kind);
        const original = Object.assign(new Error('serializer exploded'), { code: 'serializer_test' });
        if (failure === 'throw') h.serialize(() => { throw original; });
        if (failure === 'missing') delete h.api[h.serializerName];
        if (failure === 'wrong-type') h.serialize(() => ({}));
        if (failure === 'empty') h.serialize(() => '');
        if (failure === 'bad-signature') h.serialize(() => kind === 'document'
          ? Buffer.from('NOPE').toString('base64') : 'NOPE');
        if (failure === 'bad-base64') h.serialize(() => kind === 'document' ? '%%%' : 'XLSY;v2;4;%%%');
        h.state.edit();
        if (entry === 'toolbar') assert.equal(h.api.asc_Save(), false);
        else await assert.rejects(h.save(), error => failure === 'throw' ? error === original : Boolean(error));
        assert.equal(h.pending(), null);
        assert.equal(h.state.dirty, true);
        assert.equal(h.errors().length, 1);
        assert.equal(h.cleanCalls(), 0);
        assert.equal(h.commits.length, 0);
        assert.equal(h.fallbackCalls(), 0);
        if (failure === 'throw') assert.equal(h.errors()[0].data.code, original.code);
        if (failure === 'missing') assert.equal(h.errors()[0].data.code, 'native_serializer_unavailable');
        h.restore();
        const retry = h.save();
        await h.flush();
        h.commits[0].resolve({ version_id: 'v2' });
        await retry;
        await h.drain();
        assert.equal(h.state.dirty, false);
        assert.equal(h.cleanCalls(), 1);
        assert.equal(h.errors().length, 1);
      });
    }
  }

  test(`${kind}: stale commit cannot acknowledge or clear a replacement save`, async () => {
    const h = saveHarness(kind);
    h.state.edit();
    h.api.asc_Save();
    const old = h.pending();
    const original = new Error('SDK save error');
    const rejected = assert.rejects(old.promise, error => error === original);
    await h.flush();
    h.fail(old, original);
    await rejected;
    const retry = h.save();
    const current = h.pending();
    await h.flush();
    h.fail(old, new Error('duplicate failure'));
    h.commits[0].resolve({ version_id: 'stale' });
    await h.flush();
    assert.equal(h.pending(), current);
    assert.equal(h.cleanCalls(), 0);
    assert.equal(h.events.filter(({ event }) => event === 'saved').length, 0);
    assert.equal(h.errors().length, 1);
    h.commits[1].resolve({ version_id: 'v2' });
    await retry;
    await h.drain();
    assert.equal(h.cleanCalls(), 1);
  });

  test(`${kind}: autosave, readiness and read-only gates prevent serialization`, () => {
    const h = saveHarness(kind);
    assert.equal(h.api.asc_Save(true), false);
    h.writable(false);
    assert.equal(h.api.asc_Save(), false);
    assert.throws(() => h.save(), /read-only/);
    h.writable(true);
    h.ready(false);
    assert.equal(h.api.asc_Save(), false);
    assert.throws(() => h.save(), /not ready/);
    assert.equal(h.serializerCalls(), 0);
    assert.equal(h.pending(), null);
    assert.equal(h.events.length, 0);
    assert.equal(h.fallbackCalls(), 0);
  });
}
