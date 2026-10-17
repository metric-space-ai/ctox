import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';
import { sha256Hex } from '../_sha256.js';

const NOW = 1_700_000_000_000;

test('signature.request legt Anfrage an und protokolliert', async () => {
  const ctx = createCtx({ now: NOW });
  const out = await run(ctx, 'ats.signature.request', { document_id: ' doc-1 ', signers: [{ id: 's1', state: 'pending' }], subject_kind: 'nachweis' });
  assert.deepEqual(out, { ok: true, request_id: `sig_${NOW}`, status: 'sent' });
  const doc = ctx.$.doc('signature_requests', `sig_${NOW}`);
  assert.equal(doc.document_id, 'doc-1');
  assert.equal(doc.sent_at_ms, NOW);
  assert.deepEqual(doc.signers, [{ id: 's1', state: 'pending' }]);
  assert.equal(ctx.$.capsTo('audit.event')[0].args[0].event_type, 'business_os.ats.signature_requested');
  await assert.rejects(run(ctx, 'ats.signature.request', {}), { code: 'invalid', message: 'ats.signature.request requires document_id' });
});

const seed = () => ({ signature_requests: [{ id: 'req-1', document_id: 'doc-1', sent_at_ms: NOW - 1, updated_at_ms: 1,
  signers: [{ id: 's1', state: 'pending' }, { id: 's2', state: 'pending' }] }] });

test('signature.sign (H11): anonymer Handelnder abgelehnt, Stempel, Teil- und Vollabschluss mit Artefakt-ID', async () => {
  const ctx = createCtx({ seed: seed(), now: NOW });
  await assert.rejects(run(ctx, 'ats.signature.sign', { request_id: 'req-1', signer_id: 's1' }, { actor: { id: 'rxdb-command' } }),
    { code: 'forbidden', message: 'ats.signature.sign requires an authenticated actor' });
  await assert.rejects(run(ctx, 'ats.signature.sign', { request_id: 'req-1', signer_id: 's1' }, { actor: {} }), { code: 'forbidden' });
  const part = await run(ctx, 'ats.signature.sign', { request_id: 'req-1', signer_id: 's1' }, { actor: { id: 'chef1', role: 'chef' } });
  assert.deepEqual(part, { ok: true, request_id: 'req-1', status: 'partially_signed', signed_artifact_id: null });
  const signer = ctx.$.doc('signature_requests', 'req-1').signers[0];
  assert.deepEqual(signer, { id: 's1', state: 'signed', signed_by_actor_id: 'chef1', signed_at_ms: NOW });
  const full = await run(ctx, 'ats.signature.sign', { request_id: 'req-1', signer_id: 's2' }, { actor: { id: 'chef1', role: 'chef' } });
  const signers = ctx.$.doc('signature_requests', 'req-1').signers;
  const expected = `sig_${sha256Hex(`req-1|chef1|${NOW}|${JSON.stringify(signers)}`)}`;
  assert.equal(full.status, 'completed');
  assert.equal(full.signed_artifact_id, expected);
  assert.equal(ctx.$.doc('signature_requests', 'req-1').signed_artifact_id, expected);
  const ev = ctx.$.capsTo('audit.event').at(-1).args[0].payload.summary;
  assert.deepEqual(ev, { request_id: 'req-1', signer_id: 's2', status: 'completed', signed_by_actor_id: 'chef1', signed_artifact_id: expected });
});

test('signature.sign: unbekannte Anfrage / unbekannter Unterzeichner, nichts geschrieben', async () => {
  const ctx = createCtx({ seed: seed() });
  await assert.rejects(run(ctx, 'ats.signature.sign', { request_id: 'nope', signer_id: 's1' }), { code: 'not_found', message: 'signature request not found' });
  await assert.rejects(run(ctx, 'ats.signature.sign', { request_id: 'req-1', signer_id: 'sX' }), { code: 'invalid', message: 'signer_id does not match any signer on this request' });
  assert.equal(ctx.$.writes.length, 0);
  assert.equal(ctx.$.capsTo('audit.event').length, 0);
});
