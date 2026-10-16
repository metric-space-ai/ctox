import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import {
  MAIL_CONTENT_FREEZE_COMMAND,
  freezeWordContentViaCommand,
  normalizeSourceBlobRef,
  buildDocumentsDeepLink,
  normalizeDocumentArtifact,
  normalizeEditorMode,
  openDocumentsArtifact,
  validateEasyEmailHandle,
  validateEasyEmailRuntime,
} from '../mail-content-editor.mjs';

test('normalizes the two supported editor modes', () => {
  assert.equal(normalizeEditorMode(), 'rich-text');
  assert.equal(normalizeEditorMode('word'), 'rich-text');
  assert.equal(normalizeEditorMode('easy-email'), 'html');
  assert.throws(() => normalizeEditorMode('markdown'), /Unsupported mail editor mode/);
});

test('normalizes Documents references without taking ownership of document bytes', () => {
  const artifact = normalizeDocumentArtifact({
    document_id: 'doc_sales_1',
    version_id: 'doc_sales_1_v3',
    label: 'Sales follow-up',
  });
  assert.deepEqual(artifact, {
    documentId: 'doc_sales_1',
    versionId: 'doc_sales_1_v3',
    title: 'Sales follow-up',
    deepLink: '#documents?record=doc_sales_1&record_id=doc_sales_1&version=doc_sales_1_v3&version_id=doc_sales_1_v3',
  });
  assert.throws(() => normalizeDocumentArtifact({ title: 'Missing id' }), /requires documentId/);
});

test('Documents deep links carry current and canonical record aliases', () => {
  assert.equal(buildDocumentsDeepLink({}), '#documents');
  assert.equal(
    buildDocumentsDeepLink({ documentId: 'doc / 4', versionId: 'v#1' }),
    '#documents?record=doc+%2F+4&record_id=doc+%2F+4&version=v%231&version_id=v%231',
  );
});

// App-API 4.0: Sprung in die Dokumente-App über api.shell.navigate (statt ctx.openDesktopApp).
test('opens a Word artifact through api.shell.navigate', async () => {
  const calls = [];
  const result = await openDocumentsArtifact({
    shell: {
      navigate: (...args) => {
        calls.push(args);
        return true;
      },
    },
  }, { documentId: 'doc_9', versionId: 'doc_9_v2' });

  assert.equal(result, true);
  assert.deepEqual(calls, [[
    'documents',
    {
      record: 'doc_9',
      record_id: 'doc_9',
      documentId: 'doc_9',
      version: 'doc_9_v2',
      version_id: 'doc_9_v2',
      versionId: 'doc_9_v2',
    },
  ]]);
});

test('fails closed when the shell navigation is missing', async () => {
  await assert.rejects(
    openDocumentsArtifact({}, { documentId: 'doc_9' }),
    (error) => error.code === 'documents_launcher_unavailable',
  );
});

test('validates the expected local Easy Email API', () => {
  const factory = () => null;
  assert.equal(validateEasyEmailRuntime({ createEasyEmailEditor: factory }), factory);
  assert.throws(() => validateEasyEmailRuntime({}), /must export createEasyEmailEditor/);

  const handle = {
    getDocument() {},
    getHtml() {},
    setDocument() {},
    getSelectedBlockId() {},
    onSelectionChange() {},
    setMergeTags() {},
    focus() {},
    destroy() {},
  };
  assert.equal(validateEasyEmailHandle(handle), handle);
  assert.throws(() => validateEasyEmailHandle({}), /missing getDocument/);
});

test('the vendored Easy Email port exposes the adapter factory', async () => {
  const runtime = await import('../../vendor/easy-email-editor/index.mjs');
  assert.equal(validateEasyEmailRuntime(runtime), runtime.createEasyEmailEditor);
});

test('keeps the HTML canvas central and optional panels in one right drawer', async () => {
  const source = await readFile(new URL('../mail-content-editor.mjs', import.meta.url), 'utf8');
  const css = await readFile(new URL('../editor.css', import.meta.url), 'utf8');

  assert.match(source, /panelHosts.*blocks.*design.*logic.*source/s);
  assert.match(source, /commandHost\.replaceChildren\(commandbar\)/);
  assert.match(source, /requestPanel:\s*\(name\)\s*=>\s*openHtmlPanel\(name,\s*\{\s*fromRuntime:\s*true\s*\}\)/);
  assert.match(source, /panelOptions\.fromRuntime\s*!==\s*true/);
  assert.match(source, /drawer\.hidden = true/);
  assert.match(source, /htmlPanel\.append\(htmlHost, drawerBackdrop, drawer\)/);
  assert.match(css, /\.mail-content-editor-html-host[\s\S]*position:\s*absolute;[\s\S]*inset:\s*0;/);
  assert.match(css, /\.mail-content-editor-drawer[\s\S]*position:\s*absolute;[\s\S]*inset-inline-end:\s*0;/);
  assert.doesNotMatch(css, /grid-template-columns:\s*[^;]*240px[^;]*1fr[^;]*280px/);
});

// MAIL3: Word-Inhalt über den mail-eigenen Handler mail.content.freeze (OS-Primitive), nicht über documents.
const SOURCE = { sha256: 'a'.repeat(64), size: 2048, transfer_id: 'tr_src' };
const fileRef = (sha, mediaType) => ({ storage: 'files', ref: sha, sha256: sha, size: 4, media_type: mediaType, blob: { sha256: sha, size: 4, transfer_id: `tr_${sha}` } });
function fakeApi({ outcome, artifact } = {}) {
  const calls = [];
  const listeners = new Set();
  const files = { hs: 'Hallo <b>Welt</b>', ts: 'Hallo Welt' };
  const api = {
    calls,
    command(type, payload, opts) {
      calls.push({ type, payload, opts });
      return {
        id: 'cmd-freeze-1',
        local: Promise.resolve(),
        status: {
          subscribe(fn) {
            listeners.add(fn);
            fn({ status: 'queued' });
            queueMicrotask(() => fn(outcome === 'failed' ? { status: 'failed', error: 'Quell-Hash weicht ab' } : { status: 'done', result: { ok: true, artifact } }));
            return () => listeners.delete(fn);
          },
        },
      };
    },
    files: { async get(ref) { calls.push({ files: ref }); return new Blob([files[ref]]); } },
  };
  return { api, listeners };
}

test('normalizes an optional source BlobRef on the Word artifact', () => {
  const artifact = normalizeDocumentArtifact({ document_id: 'doc_1', source: { ...SOURCE, extra: 1 } });
  assert.deepEqual(artifact.source, SOURCE);
  assert.equal(normalizeSourceBlobRef({ sha256: 'x', size: 'big', transfer_id: 't' }), null);
  assert.equal('source' in normalizeDocumentArtifact({ document_id: 'doc_1' }), false);
});

test('freezes Word content through the mail command and loads files via api.files', async () => {
  const artifact = { compiler_id: 'email-v1', source_sha256: 'b'.repeat(64), html: fileRef('hs', 'text/html; charset=utf-8'), text: fileRef('ts', 'text/plain; charset=utf-8'), assets: [] };
  const { api, listeners } = fakeApi({ artifact });
  const result = await freezeWordContentViaCommand(api, { source: SOURCE, documentId: 'doc_1', versionId: 'doc_1_v2' });
  assert.equal(api.calls[0].type, MAIL_CONTENT_FREEZE_COMMAND);
  assert.equal(MAIL_CONTENT_FREEZE_COMMAND, 'mail.content.freeze');
  assert.deepEqual(api.calls[0].payload, { source: SOURCE, document_id: 'doc_1', version_id: 'doc_1_v2' });
  assert.equal(api.calls[0].opts, undefined, 'kein Bezug auf einen fremden Datensatz');
  assert.deepEqual(api.calls.slice(1).map((c) => c.files).sort(), ['hs', 'ts']);
  assert.equal(result.html, 'Hallo <b>Welt</b>');
  assert.equal(result.text, 'Hallo Welt');
  assert.equal(result.artifact.compiler_id, 'email-v1');
  assert.equal(listeners.size, 0, 'status subscription released');
});

test('freeze failure rejects with the command id (app reports it to the zone)', async () => {
  const { api, listeners } = fakeApi({ outcome: 'failed' });
  await assert.rejects(freezeWordContentViaCommand(api, { source: SOURCE }), (error) => error.code === 'command_failed'
    && error.command_id === 'cmd-freeze-1' && /Quell-Hash/.test(error.message));
  assert.deepEqual(api.calls[0].payload, { source: SOURCE });
  assert.equal(listeners.size, 0);
});

test('freeze without a source BlobRef sends no command', async () => {
  const { api } = fakeApi({});
  await assert.rejects(freezeWordContentViaCommand(api, { documentId: 'doc_1' }), (error) => error.code === 'invalid');
  assert.equal(api.calls.length, 0);
});

test('freeze result without transport files is rejected', async () => {
  const { api } = fakeApi({ artifact: { html: fileRef('hs', 'text/html') } });
  await assert.rejects(freezeWordContentViaCommand(api, { source: SOURCE }), (error) => error.code === 'email_artifact_invalid');
});

test('serialize keeps the frozen files references from the mail command result', async () => {
  const source = await readFile(new URL('../mail-content-editor.mjs', import.meta.url), 'utf8');
  assert.match(source, /compiledHtmlRef: compiled\?\.artifact\?\.html\?\.ref \? cloneJson\(compiled\.artifact\.html\)/);
  assert.match(source, /compiledTextRef: compiled\?\.artifact\?\.text\?\.ref \? cloneJson\(compiled\.artifact\.text\)/);
  assert.doesNotMatch(source, /office\.document\.|freeze_email_content/);
});

test('mail editor no longer imports the v3 Office bridge', async () => {
  const source = await readFile(new URL('../mail-content-editor.mjs', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /business-os-bridge|createBusinessOsOfficeBridge|office-engine/);
});
