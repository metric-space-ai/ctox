// outbound.tracking.event – eingehendes Tracking-Ereignis (Pixel-Abruf, Link-Klick), zugestellt von net.ingest
// auf den von outbound angemeldeten Pfad (Kern §16.35 Nr. 3, Strang TRACK).
// Payload (Kernform §16.38 Nr. 2): {event_id, method, path, sub_path, query, headers, received_at_ms, body?, body_ref?}.
// Ergebnis.response ist die HTTP-Antwort, die net.ingest ausliefert: 200 GIF (Öffnung), 302 location (Klick),
// 404 bei unbekanntem Token oder falschem Ereignistyp; jede Antwort läuft durch ingestResponse (§16.38 Nr. 3).
// ref: server.rs:340-376 (Routen), store_outbound_commands.rs:6265-6337 (record_mail_tracking_event),
//      server.rs:4024-4053 (Antworten).
import { parseTrackingPath, recordTrackingEvent, pixelResponse, redirectResponse, notFound, ingestResponse, trackingPixelRef } from './_lib/tracking.js';

function header(headers, name) {
  if (!headers || typeof headers !== 'object') return null;
  for (const [key, value] of Object.entries(headers)) {
    if (key.toLowerCase() === name) return typeof value === 'string' ? value : Array.isArray(value) ? String(value[0] ?? '') : null;
  }
  return null;
}

/**
 * Teilpfad, sonst voller Pfad (§16.38 Nr. 2). Altpfade ordnet der Kern über `aliases` zu; der Handler liest nur die
 * letzten zwei Segmente (parseTrackingPath) und erkennt keine Altpfade selbst. body/body_ref (§16.38 Nr. 2) braucht
 * ein GET-Pixel/Klick nicht: sie werden weder gelesen noch gespeichert.
 */
function trackingTarget(payload) {
  const fromSub = typeof payload.sub_path === 'string' ? parseTrackingPath(payload.sub_path) : null;
  return fromSub ?? (typeof payload.path === 'string' ? parseTrackingPath(payload.path) : null);
}

async function decide(command, ctx) {
  const payload = command?.payload ?? {};
  const method = String(payload.method ?? 'GET').toUpperCase();
  // Rust bediente nur GET; alles andere fiel auf „nicht gefunden“.
  if (method !== 'GET') return { ok: false, response: notFound('not found') };
  const parsed = trackingTarget(payload);
  if (!parsed || !parsed.token) return { ok: false, response: notFound('tracking token not found') };
  const eventId = typeof payload.event_id === 'string' && payload.event_id ? payload.event_id : command.id;
  const now = ctx.now();
  const hit = await ctx.transaction((tx) => recordTrackingEvent(tx, {
    token: parsed.token, expected: parsed.expected, eventId, userAgent: header(payload.headers, 'user-agent'), now,
  }));
  if (!hit) return { ok: false, response: notFound('tracking token not found') };
  let response;
  if (hit.event_type === 'opened') response = pixelResponse(await trackingPixelRef(ctx));
  else if (!hit.target_url) response = notFound('tracking target not found');
  else response = redirectResponse(hit.target_url);
  return { ok: true, ...hit, response };
}

export default async function handle(command, ctx) {
  const result = await decide(command, ctx);
  return { ...result, response: ingestResponse(result.response) };
}
