import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';

import { __documentsTestHooks as hooks } from './index.js';

function switchState() {
  return {
    // App-API 4.0: das Fenster-Root kommt aus mount(root, api), nicht aus ctx.host.
    root: { querySelector: () => null },
    // App-API 4.0: Zustandsmeldungen gehen in die Shell-Zone (api.status), nicht in die App.
    api: { status: { report() {}, clear() {} } },
    t: (_key, fallback) => fallback,
    documents: [
      { id: 'merge_1', current_version_id: 'merge_1_recipient_1' },
      { id: 'document_2', current_version_id: 'document_2_v1' },
    ],
    selectedId: 'merge_1',
    selectedVersion: { id: 'merge_1_recipient_1', blob_id: 'blob_1' },
    requestedVersionId: 'merge_1_recipient_1',
    requestedVersionDocumentId: 'merge_1',
    mailMergeNavigation: {
      groupId: 'merge_group_1',
      title: 'MURRELEKTRONIK 2022',
      activeIndex: 0,
      entries: [
        { documentId: 'merge_1', versionId: 'merge_1_recipient_1', label: 'Peter Mehrle', index: 0 },
        { documentId: 'merge_1', versionId: 'merge_1_recipient_2', label: 'Anna Beispiel', index: 1 },
      ],
    },
    mailMergeRecipientLoading: null,
    editorHandle: null,
    dirty: false,
    switchSerial: 0,
    renderSerial: 0,
    officeEngine: 'ctox_documents',
    disposed: false,
  };
}

function switchLifecycle(versionById, counters) {
  return {
    flushActiveEditorDraft: async () => { counters.flush += 1; },
    destroyActiveEditor: async (state) => {
      counters.destroy += 1;
      state.editorHandle = null;
    },
    loadSelectedVersion: async (state) => {
      state.selectedVersion = versionById[state.requestedVersionId];
      const activeIndex = state.mailMergeNavigation?.entries?.findIndex(
        (entry) => entry.versionId === state.selectedVersion?.id,
      );
      if (activeIndex >= 0) {
        state.mailMergeNavigation = { ...state.mailMergeNavigation, activeIndex };
      }
      return state.selectedVersion;
    },
    replaceActiveEditorVersion: async (state, record, version) => {
      counters.open += 1;
      await state.editorHandle.openVersion({ recordId: record.id, versionId: version.id });
      state.editorHandle.renderKey = `${record.id}:${version.id}:ctox_documents`;
    },
    renderSelection: () => {},
    renderCenter: () => {},
    renderError: () => {},
  };
}

test('mail merge recipient versions reuse the active Office iframe without destroying it', async () => {
  const counters = { flush: 0, destroy: 0, open: 0 };
  const opened = [];
  const state = switchState();
  state.editorHandle = {
    kind: 'ctox-documents',
    openVersion: async (request) => opened.push(request),
    focus: () => {},
  };

  await hooks.switchSelectedDocument(
    state,
    'merge_1',
    { versionId: 'merge_1_recipient_2' },
    switchLifecycle({
      merge_1_recipient_2: { id: 'merge_1_recipient_2', blob_id: 'blob_2' },
    }, counters),
  );

  assert.equal(counters.destroy, 0);
  assert.equal(counters.flush, 0, 'an unchanged recipient must not wait for draft persistence');
  assert.equal(counters.open, 1);
  assert.deepEqual(opened, [{ recordId: 'merge_1', versionId: 'merge_1_recipient_2' }]);
  assert.equal(state.selectedVersion.id, 'merge_1_recipient_2');
});

test('switching to another document still destroys the active editor', async () => {
  const counters = { flush: 0, destroy: 0, open: 0 };
  const state = switchState();
  state.editorHandle = {
    kind: 'ctox-documents',
    openVersion: async () => {},
  };

  await hooks.switchSelectedDocument(
    state,
    'document_2',
    { versionId: 'document_2_v1' },
    switchLifecycle({
      document_2_v1: { id: 'document_2_v1', blob_id: 'blob_document_2' },
    }, counters),
  );

  assert.equal(counters.destroy, 1);
  assert.equal(counters.flush, 1);
  assert.equal(counters.open, 0);
  assert.equal(state.selectedId, 'document_2');
});

test('loading the same blob twice performs one file read', async () => {
  // App-API 4.0: Blob-Bytes kommen über das files-Dokument (ref) und api.files.get statt über document_blob_chunks.
  let fileReads = 0;
  const api = {
    get: (collection, id) => ({ get: () => (collection === 'files' && id === 'blob_1' ? { id: 'blob_1', ref: 'ref_blob_1' } : null) }),
    files: {
      async get(ref) {
        assert.equal(ref, 'ref_blob_1');
        fileReads += 1;
        return new Blob([Buffer.from('recipient bytes')]);
      },
    },
  };
  const cache = hooks.createDocumentBlobByteCache();

  const first = await hooks.loadBlobBytes({ api }, 'blob_1', cache);
  const second = await hooks.loadBlobBytes({ api }, 'blob_1', cache);

  assert.equal(fileReads, 1);
  assert.equal(Buffer.from(first).toString(), 'recipient bytes');
  assert.equal(second, first);
  assert.equal(await hooks.loadBlobBytes({ api }, 'blob_missing', cache), null, 'unknown blob yields null, not a read');
  assert.equal(fileReads, 1);
});

test('openVersion resolves without producing content, so the recipient switch rebuilds the editor', async () => {
  const counters = { flush: 0, destroy: 0, open: 0, rebuild: 0 };
  const errors = [];
  const state = switchState();
  state.editorHandle = {
    kind: 'ctox-documents',
    openVersion: async ({ recordId, versionId }) => {
      counters.open += 1;
      return { record_id: recordId, version_id: versionId, document_ready: false };
    },
    inspect: async () => ({
      record_id: 'merge_1',
      version_id: 'merge_1_recipient_2',
      document_ready: false,
    }),
  };
  const lifecycle = switchLifecycle({
    merge_1_recipient_2: { id: 'merge_1_recipient_2', blob_id: 'blob_2' },
  }, counters);
  lifecycle.replaceActiveEditorVersion = (currentState, record, version) => (
    hooks.replaceActiveEditorVersion(currentState, record, version, { timeoutMs: 0, pollIntervalMs: 0 })
  );
  lifecycle.renderCenter = async (currentState) => {
    counters.rebuild += 1;
    currentState.editorHandle = {
      inspect: async () => ({
        record_id: 'merge_1',
        version_id: 'merge_1_recipient_2',
        document_ready: true,
      }),
    };
  };
  lifecycle.renderError = (_currentState, message) => errors.push(message);

  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    await hooks.switchSelectedDocument(state, 'merge_1', { versionId: 'merge_1_recipient_2' }, lifecycle);
  } finally {
    console.warn = originalWarn;
  }

  assert.equal(counters.open, 1, 'the fast path was attempted once');
  assert.equal(counters.destroy, 1, 'the empty fast path was discarded');
  assert.equal(counters.rebuild, 1, 'the full editor path rebuilt the selected recipient');
  assert.deepEqual(errors, []);
  assert.equal(state.selectedVersion.id, 'merge_1_recipient_2');
  assert.equal(state.mailMergeRecipientLoading, null);
});

test('only a failed full rebuild shows an error after an empty openVersion result', async () => {
  const counters = { flush: 0, destroy: 0, open: 0, rebuild: 0 };
  const errors = [];
  const state = switchState();
  state.editorHandle = {
    kind: 'ctox-documents',
    openVersion: async ({ recordId, versionId }) => ({
      record_id: recordId,
      version_id: versionId,
      document_ready: false,
    }),
    inspect: async () => ({
      record_id: 'merge_1',
      version_id: 'merge_1_recipient_2',
      document_ready: false,
    }),
  };
  const lifecycle = switchLifecycle({
    merge_1_recipient_2: { id: 'merge_1_recipient_2', blob_id: 'blob_2' },
  }, counters);
  lifecycle.replaceActiveEditorVersion = (currentState, record, version) => (
    hooks.replaceActiveEditorVersion(currentState, record, version, { timeoutMs: 0, pollIntervalMs: 0 })
  );
  lifecycle.renderCenter = async () => {
    counters.rebuild += 1;
    throw new Error('Neuaufbau gescheitert');
  };
  lifecycle.renderError = (_currentState, message) => errors.push(message);

  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    await hooks.switchSelectedDocument(state, 'merge_1', { versionId: 'merge_1_recipient_2' }, lifecycle);
  } finally {
    console.warn = originalWarn;
  }

  assert.equal(counters.destroy, 1);
  assert.equal(counters.rebuild, 1);
  assert.equal(errors.length, 1);
  assert.match(errors[0], /Neuaufbau gescheitert/);
});

test('after a recipient switch the search field contains the active recipient label', async () => {
  const counters = { flush: 0, destroy: 0, open: 0 };
  const state = switchState();
  const search = { value: 'Peter Mehrle' };
  state.editorHandle = {
    kind: 'ctox-documents',
    openVersion: async () => {},
  };
  const lifecycle = switchLifecycle({
    merge_1_recipient_2: { id: 'merge_1_recipient_2', blob_id: 'blob_2' },
  }, counters);
  lifecycle.renderSelection = (currentState) => {
    const navigation = currentState.mailMergeNavigation;
    hooks.syncMailMergeRecipientSearch(search, navigation.entries[navigation.activeIndex]);
  };

  await hooks.switchSelectedDocument(
    state,
    'merge_1',
    { versionId: 'merge_1_recipient_2' },
    lifecycle,
  );

  assert.equal(state.mailMergeNavigation.activeIndex, 1);
  assert.equal(search.value, 'Anna Beispiel');
});

test('switching recipient 1 to 2 and back to 1 restores recipient 1 content', async () => {
  const counters = { flush: 0, destroy: 0, open: 0 };
  const state = switchState();
  const versions = {
    merge_1_recipient_1: { id: 'merge_1_recipient_1', blob_id: 'blob_1' },
    merge_1_recipient_2: { id: 'merge_1_recipient_2', blob_id: 'blob_2' },
  };
  const contentByVersion = {
    merge_1_recipient_1: 'Brief für Peter Mehrle',
    merge_1_recipient_2: 'Brief für Anna Beispiel',
  };
  let visibleContent = contentByVersion.merge_1_recipient_1;
  let openedVersionId = 'merge_1_recipient_1';
  state.editorHandle = {
    kind: 'ctox-documents',
    async openVersion({ recordId, versionId }) {
      counters.open += 1;
      openedVersionId = versionId;
      visibleContent = contentByVersion[versionId];
      return { record_id: recordId, version_id: versionId, document_ready: true };
    },
    async inspect() {
      return { record_id: 'merge_1', version_id: openedVersionId, document_ready: true };
    },
    focus() {},
  };
  const lifecycle = switchLifecycle(versions, counters);
  lifecycle.replaceActiveEditorVersion = (currentState, record, version) => (
    hooks.replaceActiveEditorVersion(currentState, record, version, { timeoutMs: 0, pollIntervalMs: 0 })
  );

  await hooks.switchSelectedDocument(state, 'merge_1', { versionId: 'merge_1_recipient_2' }, lifecycle);
  assert.equal(visibleContent, 'Brief für Anna Beispiel');
  assert.equal(state.selectedVersion.id, 'merge_1_recipient_2');

  await hooks.switchSelectedDocument(state, 'merge_1', { versionId: 'merge_1_recipient_1' }, lifecycle);
  assert.equal(visibleContent, 'Brief für Peter Mehrle');
  assert.equal(state.selectedVersion.id, 'merge_1_recipient_1');
  assert.equal(counters.destroy, 0);
  assert.equal(counters.open, 2);
});
