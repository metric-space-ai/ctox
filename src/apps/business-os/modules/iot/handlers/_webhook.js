// Origin: CTOX
// License: Apache-2.0
// Eingehende Webhooks der iot-App über das Kernprimitiv net.ingest (Kernform §16.38, Strang IOTIN).
// Fachliche Referenz: src/core/iot/webhook.rs @ 0a9046e0a (Bindung Token → genau ein Signal, Prüfung des
// Tokens im Header `X-Webhook-Token`, Abbildung des Payloads auf einen Attributwert) und die HTTP-Abbildung
// src/core/service/service.rs:4563-4620 handle_iot_webhook_request (Statuscodes, Antwortkörper).
// Der Core ist nur Transport: er nimmt die Anfrage auf <ingest_url>/<webhook_id> an und stellt sie als Befehl
// `ctox.iot.webhook.ingest` zu; Token-Prüfung, Idempotenz und Datenpunkt sind App-Logik (diese Datei).
import { fail, isObject } from './_base.js';

// Ein Pfad je App (idempotent), Webhook-ID = sub_path. Rust: `/ctox/iot/webhook/{id}` (webhook.rs:190).
export const INGEST_PATH = 'webhook';
export const INGEST_COMMAND = 'ctox.iot.webhook.ingest';
// §16.38-Ergänzung: Altpfad der Rust-Version (webhook.rs:190 `/ctox/iot/webhook/{id}`) als Alias, damit Kundengeräte
// mit der alten URL weiter ankommen; die Webhook-ID bleibt der Teilpfad hinter dem Alias.
export const INGEST_ALIASES = Object.freeze(['/ctox/iot/webhook/']);
// ref: webhook.rs:192 `"header": "X-Webhook-Token"`, service.rs:4582 eq_ignore_ascii_case("x-webhook-token").
// §16.38 Nr. 2: Header kommen ohne Cookie/Authorization; der Token-Header muss ausdrücklich angefordert werden.
export const TOKEN_HEADER = 'X-Webhook-Token';
export const PASS_HEADERS = Object.freeze([TOKEN_HEADER.toLowerCase()]);
// Bindung liegt in iot_agents (kind 'webhook'), Namensraum getrennt von Protokoll-Agenten.
export const WEBHOOK_KIND = 'webhook';
export const webhookRowId = (id) => `${WEBHOOK_KIND}:${id}`;
// Idempotenzfenster je Webhook: die letzten N event_ids mit ihrem Ergebnis (Zustellwiederholungen von net.ingest).
export const EVENT_WINDOW = 256;
// body_ref (> 64 KiB, §16.38 Nr. 2) wird höchstens bis zu dieser Größe gelesen; ein Messwert braucht weit weniger.
export const MAX_BODY_REF_BYTES = 1024 * 1024;

/** ref: webhook.rs:33-41 parse_signal_ref – `"<asset_id>::<attribute_name>"`, beide Hälften nicht leer. */
export function parseSignalRef(signalRef) {
  const at = typeof signalRef === 'string' ? signalRef.indexOf('::') : -1;
  const asset = at >= 0 ? signalRef.slice(0, at) : '';
  const name = at >= 0 ? signalRef.slice(at + 2) : '';
  if (!asset || !name) fail('invalid', `signal_ref must be '<asset_id>::<attribute_name>', got '${signalRef}'`);
  return [asset, name];
}

/** ref: webhook.rs:44-54 value_at_path – Punkt-Pfad; leerer/fehlender Pfad = ganzer Payload. Fehlt ein Glied → undefined. */
export function valueAtPath(payload, path) {
  if (typeof path !== 'string' || path.trim() === '') return payload;
  let cursor = payload;
  for (const seg of path.split('.').filter((s) => s !== '')) {
    // serde_json Value::get(&str): nur Objekte per Schlüssel; Arrays/Skalare → None (kein Index über &str).
    if (!isObject(cursor) || !Object.prototype.hasOwnProperty.call(cursor, seg)) return undefined;
    cursor = cursor[seg];
  }
  return cursor;
}

const toHex = (bytes) => [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, '0')).join('');

export async function sha256Hex(text) {
  return toHex(await globalThis.crypto.subtle.digest('SHA-256', new TextEncoder().encode(String(text))));
}

/** Neues Bearer-Token: 128 Bit Zufall, 32 Hex-Zeichen. ref: webhook.rs:171 Uuid::new_v4().simple() */
export function mintToken() {
  return toHex(globalThis.crypto.getRandomValues(new Uint8Array(16)));
}

/** ref: webhook.rs:202-213 constant_eq – Längenunterschied bricht ab (verrät nur die Länge), sonst XOR über alle Bytes. */
export function constantEq(a, b) {
  const x = new TextEncoder().encode(String(a));
  const y = new TextEncoder().encode(String(b));
  if (x.length !== y.length) return false;
  let diff = 0;
  for (let i = 0; i < x.length; i += 1) diff |= x[i] ^ y[i];
  return diff === 0;
}

/** Header-Wert ohne Rücksicht auf Groß/Klein (ref: service.rs:4576-4585); Liste mit genau einem Wert zählt als Wert. */
export function headerValue(headers, name) {
  if (!isObject(headers)) return '';
  const wanted = name.toLowerCase();
  for (const [key, value] of Object.entries(headers)) {
    if (key.toLowerCase() !== wanted) continue;
    if (typeof value === 'string') return value;
    if (Array.isArray(value) && value.length === 1 && typeof value[0] === 'string') return value[0];
    return '';
  }
  return '';
}

/** ref: service.rs:4566-4573 – Webhook-ID = letztes Pfadsegment (Query abgeschnitten, Schrägstrich am Ende egal). */
export function webhookIdFrom(subPath) {
  const path = String(subPath ?? '').split('?')[0].replace(/\/+$/, '');
  return path.split('/').pop() ?? '';
}

/** Antwort im Rahmen §16.38 Nr. 3 (Status 2xx/400–410, Header nur content-type, Körper JSON ≤ 64 KiB). */
export const jsonResponse = (status, body) => ({
  status,
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify(body),
});
