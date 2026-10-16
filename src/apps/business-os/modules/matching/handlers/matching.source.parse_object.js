// matching.source.parse_object – Port von importer.rs::import_matching_objects
// :847-1018 (Routing store.rs:10177 → importer.rs:26 handle_source_parse).
// Fachlogik (Feldauswertung des CV-Textes, IDs, Datensatzform, Aufräumen alter
// Objekte) hier; PDF-Textextraktion samt Vision-OCR-Rückfall ist Fähigkeit:
// ctx.cap.doc.extract (Kernform §16.32: blob_id {ref}; die Base64-Daten legt der
// Handler vorher über ctx.files.put als files-Datensatz an). write_import_artifact
// entfällt (kein FS).
import { invalid } from './_lib/errors.js';
import { putFileRef } from './_lib/files.js';
import { byteLen, decodeBase64, firstNonempty, hexSha256, nowIso, slug, strPath } from './_lib/text.js';
import { parseCandidateText } from './_lib/candidate.js';
import { envelope } from './_lib/envelope.js';

// importer.rs::decode_file_payload
function decodeFilePayload(file) {
  const raw = firstNonempty([strPath(file, ['base64']), strPath(file, ['data_base64']), strPath(file, ['data'])]);
  let encoded = raw;
  if (raw.trim() === '') {
    const dataUrl = strPath(file, ['data_url']);
    const comma = dataUrl.indexOf(',');
    encoded = comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
  }
  const compact = [...encoded].filter((ch) => !/\s/u.test(ch)).join('');
  if (!compact) throw invalid('file payload is empty');
  const bytes = decodeBase64(compact);
  if (!bytes) throw invalid('failed to decode base64 file payload');
  return { bytes, base64: compact };
}

// importer.rs: PDF über pdf-parse (mit OCR-Rückfall); Text-Dateien direkt.
function extractKind(name, mime) {
  const lower = `${name}`.toLowerCase();
  if (mime.startsWith('text/') || lower.endsWith('.txt') || lower.endsWith('.md')) return 'text';
  if (mime.includes('wordprocessingml') || lower.endsWith('.docx')) return 'document';
  return 'pdf';
}

export default async function handle(command, ctx) {
  const payload = command.payload || {};
  const files = Array.isArray(payload.input?.files) ? payload.input.files
    : Array.isArray(payload.source?.files) ? payload.source.files : [];
  if (files.length === 0) throw invalid('matching object parse requires input.files');
  const recordId = typeof command.record_id === 'string' ? command.record_id : '';
  const baseRecord = recordId.trim() ? recordId : command.id;
  const prefix = `${baseRecord}__`;

  // Dateien vor dem Schreiben auswerten: Rust löscht zuerst und bricht dann ggf.
  // mitten in der Schleife ab (Teilzustand). Hier: alles oder nichts.
  const docs = [];
  const summaries = [];
  for (const file of files) {
    const name = typeof file?.name === 'string' ? file.name : 'document.pdf';
    const mime = typeof file?.type === 'string' ? file.type : 'application/pdf';
    let decoded;
    try {
      decoded = decodeFilePayload(file);
    } catch (error) {
      throw invalid(`failed to decode ${name}: ${error.message}`);
    }
    let rawText = '';
    try {
      // §16.32: Rohbytes erst als files-Datensatz anlegen (ref = sha256), dann das Primitiv mit {ref}.
      const ref = await putFileRef(ctx, decoded.bytes, name, mime);
      const extracted = await ctx.cap.doc.extract({ blob_id: { ref }, kind: extractKind(name, mime), ocr: true });
      rawText = typeof extracted?.text === 'string' ? extracted.text : '';
    } catch (error) {
      // Rust: PDF-Fehler → leerer Text, Import läuft mit raw_text_length 0 weiter.
      ctx.log('warn', `PDF parse failed for ${name}`, { error: String(error?.message ?? error) });
    }
    const candidate = parseCandidateText(name, rawText);
    const now = nowIso(ctx.now());
    const objectId = `${baseRecord}__${slug(candidate.name)}`;
    docs.push({
      id: objectId,
      kind: 'object',
      name: candidate.name,
      firstName: candidate.first_name,
      lastName: candidate.last_name,
      email: candidate.email,
      phone: candidate.phone,
      currentRole: candidate.current_role,
      desiredPosition: candidate.desired_position,
      taxonomy: 'importiertes Objekt',
      region: candidate.region,
      driverLicense: candidate.driver_license,
      highestDegree: candidate.highest_degree,
      languages: candidate.languages,
      skills: candidate.skills,
      executiveInfo: {
        fachlicheQualifikation: candidate.current_role,
        methodenKompetenz: candidate.skills.slice(0, 10).join(', '),
        leadershipFaehigkeit: candidate.leadership,
        gehaltswunschUndOrt: candidate.region,
      },
      active: true,
      activeKey: 1,
      documents: [{
        id: `${objectId}::document`,
        kind: 'document',
        filename: name,
        mimeType: mime,
        size: decoded.bytes.length,
        uploadedAt: now,
        parsed: true,
        meta: { rawText, sha256: hexSha256(decoded.bytes), command_id: command.id, queue_task_id: null },
      }],
      additional: [{
        key: 'system.import',
        label: 'Import',
        type: 'json',
        value: { state: 'done', source: 'drawer', command_id: command.id },
        source: 'ctox-native-importer',
        confidence: candidate.confidence,
        required: false,
      }],
      rawText,
      createdAt: now,
      updatedAt: now,
    });
    summaries.push({ record_id: objectId, source_name: name, raw_text_length: byteLen(rawText) });
  }

  await ctx.transaction(async (tx) => {
    // Rust: DELETE … WHERE record_id LIKE '<base>__%'
    const old = await tx.read('matching_objects', { selector: { id: { $gte: prefix, $lt: `${prefix}￿` } } });
    for (const doc of old) if (doc.id.startsWith(prefix)) await tx.remove('matching_objects', doc.id);
    for (const doc of docs) await tx.put('matching_objects', envelope('objects', doc, ctx.now(), { definition_id: 'matching.objects.v1' }));
  });
  return {
    record_ids: docs.map((d) => d.id),
    records_count: docs.length,
    collection: 'matching_objects',
    definition_id: 'matching.objects.v1',
    records: summaries,
  };
}
