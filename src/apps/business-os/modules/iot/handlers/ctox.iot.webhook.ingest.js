// Origin: CTOX
// License: Apache-2.0
// ctox.iot.webhook.ingest – eingehende Webhook-Anfrage, zugestellt von net.ingest (Kernform §16.38 Nr. 2/3):
// payload {event_id, method, path, sub_path, query, headers, received_at_ms, body? | body_ref?:{ref}}.
// ref: src/core/iot/webhook.rs:218-245 handle_http (Bindung nachschlagen, Token konstant-zeitig prüfen, dann ingest),
// :60-80 ingest (value_at_path, Wert nicht null, attribute_write), service.rs:4563-4620 (Statuscodes/Körper).
// Antwort = result.response {status, headers, body} (§16.38 Nr. 3); fachliche Fehler werden Antworten, nie Throws.
// Idempotent je event_id: eine erneute Zustellung desselben Ereignisses schreibt keinen zweiten Datenpunkt und
// bekommt dieselbe Antwort.
import { isObject } from './_base.js';
import { C } from './_iotcore.js';
import { attributeWrite } from './ctox.iot.attribute.write.js';
import {
  EVENT_WINDOW, MAX_BODY_REF_BYTES, TOKEN_HEADER, WEBHOOK_KIND, constantEq, headerValue, jsonResponse,
  parseSignalRef, sha256Hex, valueAtPath, webhookIdFrom, webhookRowId,
} from './_webhook.js';

// ref: service.rs:4541-4546 (Fallthrough), :4589-4601 (Körper), :4605 (Erfolg), :4606-4618 (Fehler ohne Detail).
const NOT_FOUND = () => jsonResponse(404, { error: 'not found' });
const BAD_REQUEST = () => jsonResponse(400, { ok: false, error: 'bad request' });
const INVALID_JSON = () => jsonResponse(400, { ok: false, error: 'invalid json' });
const UNAUTHORIZED = () => jsonResponse(401, { ok: false });
const REJECTED = () => jsonResponse(400, { ok: false });
const ACCEPTED = () => jsonResponse(202, { ok: true });
const BY_STATUS = { 202: ACCEPTED, 400: REJECTED };

const reply = (response, extra = {}) => ({ response, ...extra });

/** Körper als Text: inline (≤ 64 KiB) oder über den files-Datensatz body_ref (Handler-Vertrag ctx.files, §16.32). */
async function readBody(payload, ctx) {
  if (typeof payload.body === 'string') return { text: payload.body };
  if (payload.body != null) return { error: BAD_REQUEST };
  const ref = payload.body_ref;
  if (ref == null) return { text: '' };
  if (!isObject(ref) || typeof ref.ref !== 'string' || ref.ref === '') return { error: BAD_REQUEST };
  if (typeof ctx.files?.get !== 'function') return { unavailable: 'ctx.files.get is not available' };
  let blob;
  try {
    blob = await ctx.files.get(ref.ref);
  } catch (error) {
    return { unavailable: `body_ref ${ref.ref}: ${error?.code ?? error?.message ?? error}` };
  }
  const size = typeof blob === 'string' ? new TextEncoder().encode(blob).length : (blob?.size ?? blob?.byteLength ?? 0);
  if (size > MAX_BODY_REF_BYTES) return { error: BAD_REQUEST };
  if (typeof blob === 'string') return { text: blob };
  if (typeof blob?.text === 'function') return { text: await blob.text() };
  if (blob instanceof ArrayBuffer || ArrayBuffer.isView(blob)) {
    try {
      return { text: new TextDecoder('utf-8', { fatal: true }).decode(blob) };
    } catch {
      return { error: BAD_REQUEST }; // ref: service.rs:4589 read_to_string scheitert an ungültigem UTF-8
    }
  }
  return { unavailable: `body_ref ${ref.ref}: unreadable blob` };
}

/** Ereignis im Idempotenzfenster beanspruchen (eine Schreibtransaktion). Rückgabe: vorhandener Eintrag oder null. */
async function claimEvent(ctx, rowId, eventId, now) {
  return ctx.transaction(async (tx) => {
    const row = await tx.get(C.agents, rowId);
    const events = Array.isArray(row?.data?.events) ? row.data.events : [];
    const seen = events.find((e) => e?.id === eventId);
    if (seen) return seen;
    const next = [...events, { id: eventId, status: 'processing', at_ms: now }].slice(-EVENT_WINDOW);
    await tx.patch(C.agents, rowId, { data: { ...row.data, events: next }, updated_at_ms: now });
    return null;
  });
}

/** Ergebnis festschreiben (status = HTTP-Status) oder Anspruch lösen (status null → erneute Zustellung verarbeitet neu). */
async function settleEvent(ctx, rowId, eventId, status) {
  await ctx.transaction(async (tx) => {
    const row = await tx.get(C.agents, rowId);
    if (!row) return;
    const events = Array.isArray(row.data?.events) ? row.data.events : [];
    const next = status == null
      ? events.filter((e) => e?.id !== eventId)
      : events.map((e) => (e?.id === eventId ? { ...e, status } : e));
    await tx.patch(C.agents, rowId, { data: { ...row.data, events: next }, updated_at_ms: ctx.now() });
  });
}

export default async function handle(command, ctx) {
  const payload = isObject(command?.payload) ? command.payload : {};
  // Nur POST (service.rs:4538); alles andere fällt wie in Rust auf 404.
  if (String(payload.method ?? '').toUpperCase() !== 'POST') return reply(NOT_FOUND());
  const eventId = typeof payload.event_id === 'string' && payload.event_id !== '' ? payload.event_id : null;
  if (!eventId) return reply(BAD_REQUEST());

  // ref: webhook.rs:225-238 – Bindung nachschlagen; unbekannt oder Token falsch → 401 ohne Unterscheidung
  // (service.rs:4606-4615). Prüfung vor dem Lesen des Körpers: unautorisierte Anfragen lösen kein files-Lesen aus.
  const webhookId = webhookIdFrom(payload.sub_path);
  const rowId = webhookRowId(webhookId);
  const row = webhookId ? await ctx.get(C.agents, rowId) : null;
  const binding = row?.kind === WEBHOOK_KIND && row.enabled !== false && isObject(row.data) ? row.data : null;
  const provided = headerValue(payload.headers, TOKEN_HEADER);
  const expected = typeof binding?.token_sha256 === 'string' ? binding.token_sha256 : '';
  // ref: webhook.rs:239-242 constant_eq(provided, expected) – hier über die sha256-Digests (gleiche Länge immer).
  if (!binding || !expected || !constantEq(await sha256Hex(provided), expected)) return reply(UNAUTHORIZED());

  // Idempotenz je event_id: bereits gesehen → gleiche Antwort, kein zweiter Datenpunkt.
  const now = ctx.now();
  const seen = await claimEvent(ctx, rowId, eventId, now);
  if (seen) return reply((BY_STATUS[seen.status] ?? ACCEPTED)(), { duplicate: true, event_id: eventId });

  const finish = async (status, response, extra = {}) => {
    await settleEvent(ctx, rowId, eventId, status);
    return reply(response, { event_id: eventId, ...extra });
  };

  // ref: service.rs:4588-4601 – Körper lesen (400 bad request) und als JSON parsen (400 invalid json).
  const body = await readBody(payload, ctx);
  if (body.unavailable) {
    ctx.log?.('warn', 'iot webhook body unavailable', { webhook_id: webhookId, event_id: eventId, reason: body.unavailable });
    return finish(null, BAD_REQUEST());
  }
  if (body.error) return finish(400, body.error());
  let json;
  try {
    json = JSON.parse(body.text);
  } catch {
    return finish(400, INVALID_JSON());
  }

  // ref: webhook.rs:68-73 ingest – Wert am Pfad, nicht null.
  let assetId;
  let name;
  try {
    [assetId, name] = parseSignalRef(binding.signal_ref);
  } catch {
    return finish(400, REJECTED());
  }
  const value = valueAtPath(json, binding.value_path);
  if (value === undefined || value === null) return finish(400, REJECTED());

  // ref: webhook.rs:74-79 + service.rs:4604 (ts 0 = Empfangszeit); hier die Empfangszeit des Kerns, damit eine
  // erneute Zustellung denselben Zeitstempel trägt.
  const ts = Number.isInteger(payload.received_at_ms) && payload.received_at_ms > 0 ? payload.received_at_ms : 0;
  try {
    const out = await attributeWrite(ctx, { asset_id: assetId, name, value, timestamp_ms: ts }, row.realm);
    return finish(202, ACCEPTED(), { outcome: out.outcome, projections: out.projections });
  } catch (error) {
    // Fachfehler (Asset fehlt, Koerzierung) → 400 wie Rust; Infrastruktur (unavailable) gibt den Anspruch frei.
    const transient = error?.code === 'unavailable';
    ctx.log?.('warn', 'iot webhook ingest failed', { webhook_id: webhookId, event_id: eventId, code: error?.code ?? null });
    return finish(transient ? null : 400, REJECTED());
  }
}
