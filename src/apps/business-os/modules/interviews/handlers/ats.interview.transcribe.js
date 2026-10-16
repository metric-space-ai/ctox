// Origin: CTOX
// License: Apache-2.0
// ats.interview.transcribe – ref: store_ats_commands.rs:1151-1237 @ 4794af03d (GEMISCHT)
// Fachlogik (hier): Pflichtfelder, Ziel-Collection fest interview_meetings, Größenprüfung ≤ 256 MiB über das
// files-Dokument, Rückschrieb transcript_* auf das Meeting, Prüfspur, nicht-fataler Fehlerausgang.
// Fähigkeit (Kernform §16.32, Strang CAPSIG): ctx.cap.speech.stt.transcribe({blob_id:{ref}, model?}) → {text, segments}.
// Quelle ist ein files-Datensatz: `source_file_ref` (ref) bzw. `source_file_id` – ältere Desktop-Dateien liegen nach
// dem einmaligen Kern-Import desktop_files→files unter derselben id in files (V4-HANDLERS.md „Kern-Import“).
// Rohe audio_path werden nie angenommen.
import { atsActorValue, fail, firstStringField, payloadOf, recordAtsGovernanceEvent, upsertRecord } from './_ats.js';

export const MAX_AUDIO_BYTES = 256 * 1024 * 1024;

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const meetingId = firstStringField(p, ['meeting_id', 'record_id']);
  if (meetingId === undefined) fail('invalid', 'ats.interview.transcribe requires meeting_id');
  const collection = 'interview_meetings';
  const fileId = firstStringField(p, ['source_file_id']);
  const fileRef = firstStringField(p, ['source_file_ref']);
  if (fileId === undefined && fileRef === undefined) fail('invalid', 'ats.interview.transcribe requires source_file_id');
  // files-Dokument per id (auch Kern-Import der Desktop-Dateien), sonst per ref. Lesefehler zählen als Größe 0.
  let fileDoc = null;
  try {
    fileDoc = fileId !== undefined ? await ctx.get('files', fileId) : null;
    if (!fileDoc && fileRef !== undefined) [fileDoc] = await ctx.read('files', { selector: { ref: fileRef }, limit: 1 });
  } catch {
    fileDoc = null;
  }
  const declaredSize = typeof fileDoc?.size === 'number' && fileDoc.size >= 0 ? Math.trunc(fileDoc.size) : 0;
  const label = fileId ?? fileRef;
  if (declaredSize > MAX_AUDIO_BYTES) {
    fail('invalid', `interview audio ${label} is too large (${declaredSize} bytes, max ${MAX_AUDIO_BYTES})`);
  }
  const ref = fileRef ?? (typeof fileDoc?.ref === 'string' && fileDoc.ref ? fileDoc.ref : undefined);
  const model = firstStringField(p, ['model']) ?? '';
  const now = ctx.now();
  let resp;
  try {
    if (ref === undefined) fail('not_found', `interview audio ${label} is not available in files (Kern-Import desktop_files→files)`);
    resp = await ctx.cap.speech.stt.transcribe({ blob_id: { ref }, model });
  } catch (error) {
    // Rust: Err(err) aus transcribe_audio_file → ok:false, kein Schreiben (nicht-fataler Ausgang, V4-HANDLERS.md).
    return { ok: false, meeting_id: meetingId, transcription_available: false, error: String(error?.message ?? error) };
  }
  const text = String(resp?.text ?? '');
  const transcriptId = `transcript_${now}`;
  const charCount = [...text].length;
  await ctx.transaction(async (tx) => {
    const existing = await tx.get(collection, meetingId);
    const payload = existing ? JSON.parse(JSON.stringify(existing)) : { id: meetingId };
    payload.transcript_id = transcriptId;
    payload.transcript_text = text;
    payload.transcript_model = String(resp?.model ?? model);
    payload.transcribed_at_ms = now;
    await upsertRecord(tx, collection, meetingId, now, payload);
  });
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.interview_transcribed', {
    meeting_id: meetingId, transcript_id: transcriptId, transcript_chars: charCount, source_file_id: fileId ?? fileDoc?.id ?? null,
  }, now);
  return { ok: true, meeting_id: meetingId, transcript_id: transcriptId, transcript_chars: charCount };
}
