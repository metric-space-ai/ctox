import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';
import { MAX_AUDIO_BYTES } from '../ats.interview.transcribe.js';

const NOW = 1_700_000_000_000;
const FOREIGN = { files: { owner_app: 'ctox-platform', ops: ['read'] } };
const AUDIO_REF = 'd'.repeat(64);
// Kern-Import desktop_files→files: die Desktop-Datei f1 liegt unter derselben id in files.
const seed = (size = 1000) => ({ files: [{ id: 'f1', size, ref: AUDIO_REF }], interview_meetings: [{ id: 'm1', candidate_id: 'c1', updated_at_ms: 1 }] });

test('transcribe: STT über cap.speech.stt, Rückschrieb auf das Meeting, Prüfspur', async () => {
  const ctx = createCtx({ seed: seed(), wishForeign: FOREIGN, now: NOW, caps: { 'speech.stt.transcribe': { text: 'Hallö Welt', model: 'whisper' } } });
  const out = await run(ctx, 'ats.interview.transcribe', { meeting_id: 'm1', source_file_id: 'f1', generation_id: 'g1' }, { id: 'c9' });
  assert.deepEqual(out, { ok: true, meeting_id: 'm1', transcript_id: `transcript_${NOW}`, transcript_chars: 10 });
  const m = ctx.$.doc('interview_meetings', 'm1');
  assert.equal(m.candidate_id, 'c1');
  assert.equal(m.transcript_text, 'Hallö Welt');
  assert.equal(m.transcript_model, 'whisper');
  assert.equal(m.transcribed_at_ms, NOW);
  assert.deepEqual(ctx.$.capsTo('speech.stt.transcribe')[0].args[0], { blob_id: { ref: AUDIO_REF }, model: '' });
  assert.equal(ctx.$.capsTo('audit.event')[0].args[0].event_type, 'business_os.ats.interview_transcribed');
});

test('transcribe: STT-Fehler nicht fatal, nichts geschrieben; zu groß → invalid; Pflichtfelder', async () => {
  const ctx = createCtx({ seed: seed(), wishForeign: FOREIGN, caps: { 'speech.stt.transcribe': () => { throw new Error('model weights missing'); } } });
  const out = await run(ctx, 'ats.interview.transcribe', { record_id: 'm1', source_file_id: 'f1' });
  assert.deepEqual(out, { ok: false, meeting_id: 'm1', transcription_available: false, error: 'model weights missing' });
  assert.equal(ctx.$.writes.length, 0);
  const big = createCtx({ seed: seed(MAX_AUDIO_BYTES + 1), wishForeign: FOREIGN });
  await assert.rejects(run(big, 'ats.interview.transcribe', { meeting_id: 'm1', source_file_id: 'f1' }), { code: 'invalid', message: /too large/ });
  assert.equal(big.$.capCalls.length, 0);
  await assert.rejects(run(ctx, 'ats.interview.transcribe', { meeting_id: 'm1' }), { code: 'invalid', message: 'ats.interview.transcribe requires source_file_id' });
  await assert.rejects(run(ctx, 'ats.interview.transcribe', {}), { code: 'invalid', message: 'ats.interview.transcribe requires meeting_id' });
});

test('transcribe: neue Aufnahme per source_file_ref, Modell durchgereicht; Altdatei ohne Kern-Import → nicht fatal', async () => {
  const ctx = createCtx({ seed: seed(), wishForeign: FOREIGN, now: NOW, caps: { 'speech.stt.transcribe': { text: 'ok' } } });
  const out = await run(ctx, 'ats.interview.transcribe', { meeting_id: 'm1', source_file_ref: AUDIO_REF, model: 'whisper-large' });
  assert.equal(out.ok, true);
  assert.deepEqual(ctx.$.capsTo('speech.stt.transcribe')[0].args[0], { blob_id: { ref: AUDIO_REF }, model: 'whisper-large' });
  assert.equal(ctx.$.doc('interview_meetings', 'm1').transcript_model, 'whisper-large');
  const missing = createCtx({ seed: { interview_meetings: [{ id: 'm1' }] }, wishForeign: FOREIGN, caps: { 'speech.stt.transcribe': { text: 'x' } } });
  const res = await run(missing, 'ats.interview.transcribe', { meeting_id: 'm1', source_file_id: 'old' });
  assert.equal(res.ok, false);
  assert.match(res.error, /Kern-Import/);
  assert.equal(missing.$.capsTo('speech.stt.transcribe').length, 0);
});
