// mail.content.freeze (Strang MAIL3): Word-Inhalt nur über OS-Primitive einfrieren, keine Abhängigkeit zu documents.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createCtx, run } from './_ctx.mjs';

const NOW = 1_760_000_000_000;
const hex = (c) => c.repeat(64);
const ref = (c, size, transfer = `tr_${c}`) => ({ sha256: hex(c), size, transfer_id: transfer });
const SOURCE = ref('a', 2048);
const CANONICAL = ref('b', 2100);
const EDITOR = ref('c', 512);
const HTML = ref('d', 40);
const TEXT = ref('e', 11);
const LOGO = ref('f', 300);

function caps(extra = {}) {
  return {
    'office.edit.prepare': async () => ({
      session: { kind: 'document', source: CANONICAL, editor: EDITOR },
      model: {}, manifest: {}, protocol: 'euro-office-document', protocol_version: 3, sha256: EDITOR.sha256,
    }),
    'office.render': async () => ({
      blob_id: HTML, sha256: HTML.sha256, text_blob: TEXT, compiler_id: 'email-v1',
      assets: [{ content_id: 'logo@ctox', mime_type: 'image/png', blob_id: LOGO }],
    }),
    'blob.materialize': async ({ blob_id: blob }) => ({ blob_id: blob, sha256: blob.sha256 }),
    ...extra,
  };
}

test('mail.content.freeze: prepare → render(email_html) → materialize, Ergebnis als files-Referenzen', async () => {
  const ctx = createCtx({ now: NOW, caps: caps() });
  const out = await run(ctx, 'mail.content.freeze', {
    source: SOURCE, document_id: 'doc_1', version_id: 'doc_1_v2', expected_source_sha256: SOURCE.sha256,
  });

  // Aufrufform exakt nach Kernvertrag (flache Argumente, BlobRefs, deny_unknown_fields).
  assert.deepEqual(ctx.$.capCalls.map((call) => call.cap), [
    'office.edit.prepare', 'office.render', 'blob.materialize', 'blob.materialize', 'blob.materialize',
  ]);
  assert.deepEqual(ctx.$.capsTo('office.edit.prepare')[0].args[0], { kind: 'document', blob_id: SOURCE });
  assert.deepEqual(ctx.$.capsTo('office.render')[0].args[0], {
    session: { kind: 'document', source: CANONICAL, editor: EDITOR }, to: 'email_html',
  });
  assert.deepEqual(ctx.$.capsTo('blob.materialize').map((call) => call.args[0]), [
    { blob_id: HTML }, { blob_id: TEXT }, { blob_id: LOGO },
  ]);
  assert.equal(ctx.$.capsTo('office.edit.apply').length, 0, 'Einfrieren ändert den Inhalt nicht');

  assert.equal(out.ok, true);
  assert.equal(out.operation, 'mail.content.freeze');
  const { artifact } = out;
  assert.equal(artifact.state, 'frozen');
  assert.equal(artifact.compiler_id, 'email-v1');
  assert.equal(artifact.document_id, 'doc_1');
  assert.equal(artifact.document_version_id, 'doc_1_v2');
  assert.deepEqual(artifact.source, CANONICAL);
  assert.equal(artifact.source_sha256, CANONICAL.sha256);
  assert.deepEqual(artifact.html, {
    storage: 'files', ref: HTML.sha256, sha256: HTML.sha256, size: 40, media_type: 'text/html; charset=utf-8', blob: HTML,
  });
  assert.deepEqual(artifact.text, {
    storage: 'files', ref: TEXT.sha256, sha256: TEXT.sha256, size: 11, media_type: 'text/plain; charset=utf-8', blob: TEXT,
  });
  assert.deepEqual(artifact.assets, [{
    content_id: 'logo@ctox', storage: 'files', ref: LOGO.sha256, sha256: LOGO.sha256, size: 300, media_type: 'image/png', blob: LOGO,
  }]);
  assert.equal(artifact.frozen_at_ms, NOW);

  // Keine Schreibvorgänge, keine Folgebefehle (kein documents-Befehl).
  assert.equal(ctx.$.writes.length, 0);
  assert.equal(ctx.$.commands.length, 0);
});

test('mail.content.freeze: ungültige Quelle → invalid, keine Capability gerufen', async () => {
  const cases = [
    [{}, /payload\.source must be a BlobRef/],
    [{ source: { ...SOURCE, sha256: 'ABC' } }, /payload\.source\.sha256/],
    [{ source: { ...SOURCE, size: -1 } }, /payload\.source\.size/],
    [{ source: { ...SOURCE, size: 64 * 1024 * 1024 + 1 } }, /payload\.source\.size/],
    [{ source: { ...SOURCE, transfer_id: ' ' } }, /payload\.source\.transfer_id/],
    // Nur eine documents-ID reicht nicht: mail löst keine fremden Datensätze auf.
    [{ document_id: 'doc_1', version_id: 'doc_1_v2' }, /payload\.source must be a BlobRef/],
  ];
  for (const [payload, message] of cases) {
    const ctx = createCtx({ now: NOW, caps: caps() });
    await assert.rejects(run(ctx, 'mail.content.freeze', payload), (error) => error.code === 'invalid' && message.test(error.message));
    assert.equal(ctx.$.capCalls.length, 0, String(message));
  }
});

test('mail.content.freeze: erwarteter Quell-Hash weicht ab → conflict vor jeder Capability', async () => {
  const ctx = createCtx({ now: NOW, caps: caps() });
  await assert.rejects(
    run(ctx, 'mail.content.freeze', { source: SOURCE, expected_source_sha256: hex('9') }),
    { code: 'conflict' },
  );
  assert.equal(ctx.$.capCalls.length, 0);
});

test('mail.content.freeze: Render ohne Text-Blob oder Session → unavailable', async () => {
  const noText = createCtx({ now: NOW, caps: caps({ 'office.render': async () => ({ blob_id: HTML, compiler_id: 'x' }) }) });
  await assert.rejects(run(noText, 'mail.content.freeze', { source: SOURCE }), (error) => error.code === 'unavailable'
    && /render\.text_blob/.test(error.message));
  assert.equal(noText.$.capsTo('blob.materialize').length, 0);

  const noSession = createCtx({ now: NOW, caps: caps({ 'office.edit.prepare': async () => ({ sha256: EDITOR.sha256 }) }) });
  await assert.rejects(run(noSession, 'mail.content.freeze', { source: SOURCE }), { code: 'unavailable' });
  assert.equal(noSession.$.capsTo('office.render').length, 0);
});

test('mail.content.freeze: materialisierter Blob weicht von der Referenz ab → conflict', async () => {
  const ctx = createCtx({
    now: NOW,
    caps: caps({ 'blob.materialize': async ({ blob_id: blob }) => ({ blob_id: blob, sha256: blob.sha256 === TEXT.sha256 ? hex('0') : blob.sha256 }) }),
  });
  await assert.rejects(run(ctx, 'mail.content.freeze', { source: SOURCE }), (error) => error.code === 'conflict'
    && /^text:/.test(error.message));
});

test('mail.content.freeze: fehlende Capability → unavailable (Kern nicht gebunden)', async () => {
  const ctx = createCtx({ now: NOW });
  ctx.cap = {};
  await assert.rejects(run(ctx, 'mail.content.freeze', { source: SOURCE }), (error) => error.code === 'unavailable'
    && /ctx\.cap\.office\.edit\.prepare/.test(error.message));
});

test('mail.content.freeze: Quelle hängt an keiner anderen App (kein documents-Import, keine Fremd-Collection)', async () => {
  const source = await readFile(new URL('../mail.content.freeze.js', import.meta.url), 'utf8');
  const code = source.split('\n').filter((line) => !line.trim().startsWith('//')).join('\n');
  assert.doesNotMatch(code, /documents\/|document_versions|document_blob_chunks|freeze_email_content/);
  assert.doesNotMatch(code, /ctx\.(get|read|put|patch|remove|command)\(/);
  const imports = [...code.matchAll(/from\s+'([^']+)'/g)].map((match) => match[1]);
  assert.deepEqual(imports, ['./_base.js']);
});
