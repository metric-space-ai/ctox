// outbound.tracking.register – Mail-Tracking-Pfad bei net.ingest anmelden (Kern §16.35 Nr. 3, Strang TRACK).
// Einmalig/idempotent: ist der Endpunkt registriert, kein Kernaufruf; `payload.force: true` meldet neu an.
// Auslöser: Manifest-Trigger `every: 'PT1H'` (Vorschlag V4-HANDLERS.md „Mail-Tracking“) und – unabhängig davon –
// beim ersten Versand (outbound.message.send_approved → trackingBaseForSend).
// ref: store.rs:16592-16606 (Prüfung der Basis-URL wie tracking_base_url).
import { ensureTrackingEndpoint } from './_lib/tracking.js';

export default async function handle(command, ctx) {
  const force = command?.payload?.force === true;
  const { endpoint, registered } = await ensureTrackingEndpoint(ctx, { force });
  return { ok: true, endpoint, registered, idempotent: !registered };
}
