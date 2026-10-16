// Origin: CTOX
// License: Apache-2.0
// ctox.iot.webhook.register – ref: src/core/iot/commands.rs:2498-2504, webhook.rs:150-196 register.
// Transport über net.ingest in der Kernform §16.38 Nr. 1 samt Ergänzung: `net.ingest.webhook.register({path, command,
// pass_headers, default_response, aliases}) → {ingest_url}` (idempotent je App+Pfad). `command` ist der Befehlstyp, über
// den der Kern jede eingehende Anfrage zustellt (ohne Handler im Manifest antwortet der Kern rejected{invalid});
// `aliases` hält die Rust-URL `/ctox/iot/webhook/<id>` für bestehende Kundengeräte erreichbar. Ein Pfad `webhook` für alle Webhooks der App; die
// Webhook-ID ist der Teilpfad (`<ingest_url>/<id>`, wie Rust `/ctox/iot/webhook/{id}`). Token-Ausgabe und Bindung
// Token → genau ein Signal sind App-Logik: das Token wird nur einmal im Ergebnis ausgegeben und nur als sha256 in
// iot_agents (kind 'webhook') abgelegt (Rust: Klartext im Secret-Store). Neu-Registrierung derselben ID ersetzt
// Signal/Pfad und erneuert das Token (Rust: ON CONFLICT DO UPDATE + write_secret_record überschreibt).
// Realm nie aus dem Payload.
import { fail, isObject } from './_base.js';
import { C, agentRow, ensureRealm, generateId, sessionRealm, writeRow } from './_iotcore.js';
import {
  INGEST_ALIASES, INGEST_COMMAND, INGEST_PATH, PASS_HEADERS, TOKEN_HEADER, WEBHOOK_KIND, mintToken, parseSignalRef, sha256Hex, webhookRowId,
} from './_webhook.js';

const DESERIALIZE = 'failed to deserialize iot command payload';
const isAbsoluteHttp = (value) => typeof value === 'string' && /^https?:\/\/[^/]/i.test(value);

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  if (!isObject(command?.payload)) fail('invalid', DESERIALIZE);
  // ref: webhook.rs:151-159 WebhookRegisterReq {id?, realm, signal_ref, value_path?} – realm wird ignoriert (Sitzung).
  const { id: rawId, signal_ref: signalRef, value_path: valuePath } = command.payload;
  if (typeof signalRef !== 'string') fail('invalid', DESERIALIZE, { path: 'signal_ref' });
  if (rawId != null && typeof rawId !== 'string') fail('invalid', DESERIALIZE, { path: 'id' });
  if (valuePath != null && typeof valuePath !== 'string') fail('invalid', DESERIALIZE, { path: 'value_path' });
  parseSignalRef(signalRef); // ref: webhook.rs:164 Bindungsform prüfen
  const id = rawId ?? generateId(ctx);
  // Teilpfad der Ingest-URL: nur URL-sichere Zeichen (kein Kodieren/Dekodieren zwischen Kern und App nötig);
  // Länge passend zu iot_agents.id (maxLength 180) samt Präfix.
  if (!/^[A-Za-z0-9._~-]{1,160}$/.test(id)) fail('invalid', `webhook id must match [A-Za-z0-9._~-]{1,160}: '${id}'`);

  const rowId = webhookRowId(id);
  const existing = await ctx.get(C.agents, rowId);
  ensureRealm(existing, realm, `webhook not found: ${id}`);
  if (existing && existing.kind !== WEBHOOK_KIND) fail('invalid', `iot agent ${rowId} is not a webhook`);

  // §16.38 Nr. 1/2: eigener Pfad, Token-Header ausdrücklich durchreichen; ohne Handlerantwort binnen 3 s 202 leer
  // (wie der Erfolgsfall in service.rs:4605).
  const registered = await ctx.cap.net.ingest.webhook.register({
    path: INGEST_PATH,
    command: INGEST_COMMAND,
    pass_headers: [...PASS_HEADERS], // X-Webhook-Token kommt nur über pass_headers
    default_response: { status: 202 },
    aliases: [...INGEST_ALIASES],
  });
  const baseUrl = registered?.ingest_url;
  if (!isAbsoluteHttp(baseUrl)) fail('unavailable', 'net.ingest returned no absolute ingest_url for iot webhooks');
  const url = `${baseUrl.replace(/\/+$/, '')}/${id}`;

  const token = mintToken();
  const data = {
    webhook_id: id,
    signal_ref: signalRef,
    token_sha256: await sha256Hex(token),
    ingest_url: url,
    path: INGEST_PATH,
    events: [], // Idempotenzfenster (ctox.iot.webhook.ingest); neues Token = neue Bindung
  };
  if (valuePath != null) data.value_path = valuePath;
  const projection = await writeRow(ctx, C.agents, rowId, agentRow({
    realm, name: `Webhook ${signalRef}`, kind: WEBHOOK_KIND, enabled: true, data,
  }), existing);

  // ref: webhook.rs:189-195 – Ergebnisform; ingest_path ist jetzt die absolute URL aus net.ingest.
  return {
    id,
    token,
    ingest_path: url,
    ingest_url: url,
    header: TOKEN_HEADER,
    signal_ref: signalRef,
    projections: [projection],
  };
}
