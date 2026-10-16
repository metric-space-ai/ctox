// Mail-Tracking (Öffnungen/Klicks) als App-Logik – Kern §16.35 Nr. 3 (Strang TRACK, 15.10.2026),
// Kernform net.ingest §16.38 (Strang OUT3): register({path, command, default_response, aliases}) → {ingest_url};
// Eingang mit body/body_ref (hier ungenutzt), Header ohne Cookie/Authorization (kein pass_headers);
// Antwort nur 2xx/301–308/400–410/429, Header content-type|location|cache-control, Body ≤ 64 KiB (ingestResponse).
//
// Tracking ist NICHT Teil von mail.send. Weil nur der Core HTTPS bedient, meldet outbound über das generische
// Eingangs-Primitiv net.ingest einen eigenen Pfad an (`net.ingest.webhook.register`); jede Anfrage auf diesem Pfad
// stellt der Kern als App-Befehl `outbound.tracking.event` zu und beantwortet die HTTP-Anfrage mit dem `response`
// aus dem Handler-Ergebnis (Pixel bzw. 302-Weiterleitung). Pixel einfügen und Links umschreiben macht die App beim
// Versand selbst (prepareTrackedHtml), die Token-Zuordnung liegt in outbound_tracking_tokens.
//
// Rust-Referenz (fachlich): store_outbound_commands.rs:6188-6263 outbound_prepare_tracked_html,
// :6265-6337 record_mail_tracking_event; server.rs:340-376 Routen /mail/track/{o,c}/…, :4024-4053 Antworten;
// mailserver store/sqlite.rs:157-227 Tokens/Ereignisse; store.rs:16592-16606 Prüfung tracking_base_url.
import { fail, str, asI64, load, payloadInsert, newSimpleUuid } from './outbound.js';

export const TRACKING = {
  endpoints: 'outbound_tracking_endpoints',
  tokens: 'outbound_tracking_tokens',
  events: 'outbound_tracking_events',
  endpointId: 'mail_tracking',
  ingestPath: 'mail-tracking',
  eventCommand: 'outbound.tracking.event',
  // §16.38 Nr. 4: Altpfade bereits versandter Mails ordnet der Kern über diese Aliase zu (einmalige Core-Altliste).
  aliases: ['/mail/track/o/', '/mail/track/c/'],
};

// ref: server.rs:4024-4037 respond_mail_tracking_pixel – 1x1 transparentes GIF, ohne Cache/Referrer.
// Als Bytes (kein eigenes base64): einmalig per ctx.files.put abgelegt, Antworten verweisen mit body_ref:{ref}
// (§16.32 BlobRef = files.ref, §16.38). Der ref steht am Endpunkt-Dokument (`pixel_ref`).
export const TRACKING_PIXEL_GIF = Object.freeze([
  71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 33, 249, 4, 1, 0, 0, 0, 0, 44, 0, 0, 0, 0,
  1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
]);
export const TRACKING_PIXEL_NAME = 'outbound-tracking-pixel.gif';

/** Pixel einmalig in files ablegen (gleicher Inhalt = gleiche ref, §16.35 Nr. 1) → ref. */
export async function putTrackingPixel(ctx) {
  if (typeof ctx.files?.put !== 'function') fail('unavailable', 'ctx.files.put is not available');
  const stored = await ctx.files.put(new Uint8Array(TRACKING_PIXEL_GIF), { name: TRACKING_PIXEL_NAME, mime: 'image/gif' });
  if (typeof stored?.ref !== 'string' || stored.ref === '') fail('unavailable', 'files.put returned no ref for the tracking pixel');
  return stored.ref;
}

/** ref des Pixels vom Endpunkt; fehlt er (z. B. importierter Endpunkt), einmal ablegen und am Endpunkt merken. */
export async function trackingPixelRef(ctx) {
  const endpoint = await load(ctx, TRACKING.endpoints, TRACKING.endpointId);
  if (typeof endpoint?.pixel_ref === 'string' && endpoint.pixel_ref !== '') return endpoint.pixel_ref;
  const ref = await putTrackingPixel(ctx);
  if (endpoint) await ctx.put(TRACKING.endpoints, { ...endpoint, pixel_ref: ref });
  return ref;
}
// §16.38 Nr. 3: Header-Whitelist content-type, location, cache-control (Rusts Referrer-Policy entfällt).
const NO_STORE = { 'cache-control': 'no-store, max-age=0' };
const RESPONSE_HEADERS = new Set(['content-type', 'location', 'cache-control']);
const RESPONSE_BODY_MAX = 64 * 1024;

// ref: store_outbound_commands.rs:6206 `(?i)href\s*=\s*["'](https?://[^"']+)["']`
const LINK_PATTERN = /href\s*=\s*["'](https?:\/\/[^"']+)["']/gi;

/** ref: store.rs:16592-16606 – gleiche Prüfungen und Fehlertexte wie für tracking_base_url. */
export function validateTrackingBase(value) {
  let parsed;
  try {
    parsed = new URL(String(value));
  } catch {
    fail('invalid', 'tracking_base_url must be an absolute HTTP(S) URL');
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') fail('invalid', 'tracking_base_url must use HTTP or HTTPS');
  if (!parsed.hostname) fail('invalid', 'tracking_base_url must include a host');
  if (parsed.username !== '' || parsed.password !== '') fail('invalid', 'tracking_base_url must not contain credentials');
  return String(value).replace(/\/+$/, '');
}

const isAbsoluteHttp = (value) => typeof value === 'string' && /^https?:\/\//i.test(value.trim());

/**
 * Pfad bei net.ingest anmelden (einmalig/idempotent). Ist der Endpunkt schon registriert, kein Kernaufruf.
 * Kernform §16.38 Nr. 1 + Ergänzung: register({path, command, default_response?, pass_headers?, aliases?}) → {ingest_url}.
 * default_response = Pixel (200 GIF), damit Öffnungen schnell bleiben, wenn der Handler nicht binnen 3 s antwortet;
 * kein pass_headers (nur user-agent wird gebraucht, Cookie/Authorization nie). Ohne absolute ingest_url bleibt
 * Tracking aus – wie Rust bei leerer tracking_base_url.
 */
export async function ensureTrackingEndpoint(ctx, { force = false } = {}) {
  const existing = await load(ctx, TRACKING.endpoints, TRACKING.endpointId);
  if (!force && existing && existing.status === 'registered' && str(existing, ['base_url'])) {
    return { endpoint: existing, registered: false };
  }
  const pixelRef = typeof existing?.pixel_ref === 'string' && existing.pixel_ref !== '' ? existing.pixel_ref : await putTrackingPixel(ctx);
  const result = await ctx.cap.net.ingest.webhook.register({
    path: TRACKING.ingestPath,
    // §16.38-Ergänzung: Befehlstyp, über den der Kern jede Anfrage zustellt; ohne Handler im Manifest
    // antwortet der Kern rejected{invalid} (dann bleibt Tracking aus, s. trackingBaseForSend).
    command: TRACKING.eventCommand,
    default_response: pixelResponse(pixelRef),
    aliases: [...TRACKING.aliases],
  });
  const candidate = isAbsoluteHttp(result?.ingest_url) ? result.ingest_url : null;
  if (candidate == null) fail('unavailable', 'net.ingest returned no absolute ingest URL for mail tracking');
  const now = ctx.now();
  const baseUrl = validateTrackingBase(candidate.trim());
  const endpoint = {
    id: TRACKING.endpointId,
    path: TRACKING.ingestPath,
    command_type: TRACKING.eventCommand,
    ingest_path: new URL(baseUrl).pathname,
    base_url: baseUrl,
    pixel_ref: pixelRef,
    status: 'registered',
    registered_at_ms: existing?.registered_at_ms ?? now,
    updated_at_ms: now,
  };
  // Das Ingest-Token des Kerns ist ein Zugangsmerkmal und wird nicht in eine synchronisierte Collection geschrieben.
  await ctx.put(TRACKING.endpoints, endpoint);
  return { endpoint, registered: true };
}

/** Basis-URL für den Versand; Fehler bei der Anmeldung blockieren nie den Versand (Rust: leere Basis = aus). */
export async function trackingBaseForSend(ctx) {
  try {
    const { endpoint } = await ensureTrackingEndpoint(ctx);
    return endpoint.base_url;
  } catch (error) {
    ctx.log?.('warn', 'outbound mail tracking endpoint unavailable; sending untracked', { code: error?.code, message: error?.message });
    return '';
  }
}

function freshToken(ctx, command, salt) {
  const fromCtx = typeof ctx.uuid === 'function' ? ctx.uuid() : null;
  if (typeof fromCtx === 'string' && fromCtx) return fromCtx.replaceAll('-', '');
  return newSimpleUuid(ctx, command, salt);
}

/** ASCII-Kleinschreibung mit gleicher Länge (Rust to_ascii_lowercase), damit Indizes gültig bleiben. */
const asciiLowerSameLength = (value) => value.replace(/[A-Z]/g, (ch) => ch.toLowerCase());

/**
 * ref: store_outbound_commands.rs:6188-6263 outbound_prepare_tracked_html.
 * Stempelt den Payload der Nachricht wie Rust, legt die Tokens in outbound_tracking_tokens an und liefert das
 * versandfertige HTML. Opt-out: payload.tracking_enabled === false; ohne Basis-URL ebenfalls aus.
 */
export async function prepareTrackedHtml(ctx, command, message, html, base) {
  const explicitlyDisabled = message?.payload?.tracking_enabled === false;
  if (!base || explicitlyDisabled) {
    payloadInsert(message, 'tracking_enabled', false);
    return html;
  }
  const messageId = str(message, ['id']);
  if (messageId == null) fail('invalid', 'id is required');
  const campaignId = str(message, ['campaign_id']);
  const root = base.replace(/\/+$/, '');
  const now = ctx.now();
  const tokens = [];
  let clickLinks = 0;
  const tracked = html.replace(LINK_PATTERN, (_whole, target) => {
    const token = freshToken(ctx, command, `click:${clickLinks}`);
    tokens.push({ token, event_type: 'clicked', target_url: target });
    clickLinks += 1;
    return `href="${root}/c/${token}"`;
  });
  const openToken = freshToken(ctx, command, 'open');
  tokens.push({ token: openToken, event_type: 'opened', target_url: null });
  await ctx.transaction(async (tx) => {
    for (const t of tokens) {
      await tx.put(TRACKING.tokens, {
        id: t.token,
        token: t.token,
        message_id: messageId,
        campaign_id: campaignId ?? '',
        event_type: t.event_type,
        target_url: t.target_url ?? '',
        created_at_ms: now,
      });
    }
  });
  const pixel = `<img src="${root}/o/${openToken}.gif" width="1" height="1" alt="" style="display:block;width:1px;height:1px;border:0" />`;
  const index = asciiLowerSameLength(tracked).lastIndexOf('</body>');
  const out = index >= 0 ? `${tracked.slice(0, index)}${pixel}${tracked.slice(index)}` : `${tracked}${pixel}`;
  payloadInsert(message, 'tracking_enabled', true);
  payloadInsert(message, 'tracking_status', 'armed');
  payloadInsert(message, 'tracking_open_token', openToken);
  payloadInsert(message, 'tracking_click_links', clickLinks);
  return out;
}

// ---------------------------------------------------------------------------
// Eingehende Ereignisse (net.ingest → outbound.tracking.event)

const textResponse = (status, text) => ({ status, headers: { 'content-type': 'text/plain; charset=utf-8' }, body: text });
export const notFound = (text) => textResponse(404, text);

/** ref: server.rs:4024-4037. Körper als files-Verweis body_ref:{ref} (§16.32/§16.38; Rückfrage Antwortrahmen s. V4-HANDLERS). */
export const pixelResponse = (ref) => ({
  status: 200,
  headers: { 'content-type': 'image/gif', ...NO_STORE },
  body_ref: { ref },
});

/**
 * ref: server.rs:4041-4053 – nur gespeicherte HTTP(S)-Ziele, sonst Fehler (kein offener Redirect).
 * Abweichung §16.38 Nr. 3: Rust antwortete 500; 5xx ist nicht zulässig → 404 mit dem Rust-Fehlertext.
 */
export function redirectResponse(target) {
  let parsed;
  try {
    parsed = new URL(target);
  } catch {
    return textResponse(404, 'stored mail tracking target is invalid');
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    return textResponse(404, 'stored mail tracking target is not HTTP(S)');
  }
  return { status: 302, headers: { location: target, ...NO_STORE } };
}

const isPlainRef = (value) => value !== null && typeof value === 'object' && typeof value.ref === 'string' && value.ref !== '';

const ALLOWED_STATUS = (status) => Number.isInteger(status)
  && ((status >= 200 && status <= 299) || (status >= 301 && status <= 308) || (status >= 400 && status <= 410) || status === 429);

/**
 * §16.38 Nr. 3 – letzte Prüfung jeder Antwort vor der Rückgabe an net.ingest: unzulässiger Status, 3xx ohne absolute
 * http(s)-Location oder Körper > 64 KiB → 404; Header nur aus der Whitelist (Kleinschreibung).
 */
export function ingestResponse(response) {
  const status = response?.status;
  const headers = {};
  for (const [key, value] of Object.entries(response?.headers ?? {})) {
    const name = key.toLowerCase();
    if (RESPONSE_HEADERS.has(name) && typeof value === 'string') headers[name] = value;
  }
  const bodyBytes = typeof response?.body === 'string' ? new TextEncoder().encode(response.body).length
    : 0; // body_ref: Größe kennt files; das Pixel hat 43 Byte
  const location = headers.location;
  const redirect = status >= 301 && status <= 308;
  const bodyRef = response?.body_ref;
  const badRef = bodyRef !== undefined && !(isPlainRef(bodyRef));
  if (!ALLOWED_STATUS(status) || badRef || bodyBytes > RESPONSE_BODY_MAX
    || (redirect && !(typeof location === 'string' && /^https?:\/\/[^/\s]/i.test(location)))) {
    return textResponse(404, 'not found');
  }
  if (!redirect) delete headers.location;
  const out = { status, headers };
  if (typeof response.body === 'string') out.body = response.body;
  else if (bodyRef !== undefined) out.body_ref = { ref: bodyRef.ref };
  return out;
}

/**
 * Pfad → {expected, token}: die letzten zwei Segmente `o/<t>.gif` bzw. `c/<t>` (Query ignoriert). Gilt gleich für
 * den angemeldeten Pfad und für die Altpfade, die der Kern über `aliases` zuordnet (§16.38 Nr. 4) – keine eigene
 * Altpfad-Erkennung im Handler. ref: server.rs:340-356 (Öffnung nur mit Endung .gif, Klick: das Token).
 */
export function parseTrackingPath(subPath) {
  const segments = String(subPath ?? '').split('?')[0].split('/').filter((s) => s !== '');
  if (segments.length < 2) return null;
  const [kind, last] = segments.slice(-2);
  if (kind === 'o' && last.endsWith('.gif')) return { expected: 'opened', token: last.slice(0, -'.gif'.length) };
  if (kind === 'c') return { expected: 'clicked', token: last };
  return null;
}

/**
 * ref: store_outbound_commands.rs:6265-6337 record_mail_tracking_event – innerhalb EINER Transaktion:
 * Token prüfen (fehlend oder falscher Typ → null), Ereignis anhängen, Zähler der Nachricht fortschreiben.
 * Idempotent je eventId: ein erneut zugestelltes Ereignis zählt nicht noch einmal.
 */
export async function recordTrackingEvent(tx, { token, expected, eventId, userAgent, now }) {
  const tracking = await load(tx, TRACKING.tokens, token);
  if (!tracking) return null;
  if (tracking.event_type !== expected) return null;
  const hit = {
    message_id: tracking.message_id,
    campaign_id: tracking.campaign_id || null,
    event_type: tracking.event_type,
    target_url: tracking.target_url || null,
  };
  const previous = await load(tx, TRACKING.events, eventId);
  if (previous) return { ...hit, occurred_at_ms: previous.occurred_at_ms, duplicate: true };
  await tx.put(TRACKING.events, {
    id: eventId,
    token,
    message_id: tracking.message_id,
    campaign_id: tracking.campaign_id ?? '',
    event_type: tracking.event_type,
    occurred_at_ms: now,
    user_agent: userAgent ?? null,
  });
  const message = await load(tx, 'outbound_messages', tracking.message_id);
  if (message) {
    const [countKey, firstKey, lastKey] = tracking.event_type === 'clicked'
      ? ['click_count', 'first_clicked_at_ms', 'last_clicked_at_ms']
      : ['open_count', 'first_opened_at_ms', 'last_opened_at_ms'];
    payloadInsert(message, countKey, (asI64(message.payload?.[countKey]) ?? 0) + 1);
    if (asI64(message.payload?.[firstKey]) == null) payloadInsert(message, firstKey, now);
    payloadInsert(message, lastKey, now);
    payloadInsert(message, 'tracking_status', tracking.event_type);
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
  }
  return { ...hit, occurred_at_ms: now, duplicate: false };
}
