// Origin: CTOX
// License: Apache-2.0
// ctox.mailserver.save_runtime – ref: store.rs:16519-16634 @ 4794af03d
// Fachlogik (hier): Felder mit Rückfall auf die aktuelle Konfiguration zusammensetzen und prüfen
// (Ports 1–65535 und verschieden, Drossel/Verbindungen > 0).
// tracking_base_url entfällt (Kern §16.38 Nr. 5): Mail-Tracking ist App-Logik von outbound über net.ingest
// (Strang TRACK/OUT3); ein mitgesendeter Wert wird ignoriert und nicht an den Kern weitergegeben.
// Fähigkeit (Kern): aktuelle Laufzeitwerte lesen, `stalwart_config`-Prüfung, speichern und anwenden
// → ctx.cap.mail.server.config.get / ctx.cap.mail.server.runtime.save. Rechteprüfung SecretsManage bleibt im Kern.
import { asBool, asU64, callCap, fail, payloadOf } from './_base.js';

export function buildRuntimeSettings(payload, current) {
  const stringValue = (key, fallback) => {
    const value = payload[key];
    if (typeof value === 'string' && value.trim() !== '') return value.trim();
    return fallback;
  };
  const numberValue = (key, fallback) => asU64(payload[key]) ?? fallback;
  const settings = {
    enabled: asBool(payload.enabled) ?? current.enabled,
    hostname: stringValue('hostname', current.hostname),
    bind_host: stringValue('bind_host', current.bind_host),
    smtp_port: numberValue('smtp_port', current.smtp_port),
    imap_port: numberValue('imap_port', current.imap_port),
    outbound_throttle_per_min: numberValue('outbound_throttle_per_min', current.outbound_throttle_per_min),
    max_connections: numberValue('max_connections', current.max_connections),
  };
  // u16::try_from
  if (settings.smtp_port > 65535) fail('invalid', 'smtp_port must be between 1 and 65535');
  if (settings.imap_port > 65535) fail('invalid', 'imap_port must be between 1 and 65535');
  if (settings.smtp_port <= 0 || settings.imap_port <= 0) fail('invalid', 'ports must be greater than zero');
  if (settings.smtp_port === settings.imap_port) fail('invalid', 'SMTP and IMAP ports must differ');
  if (!(settings.outbound_throttle_per_min > 0)) fail('invalid', 'outbound_throttle_per_min must be greater than zero');
  if (!(settings.max_connections > 0)) fail('invalid', 'max_connections must be greater than zero');
  return settings;
}

export default async function handle(command, ctx) {
  const config = await callCap(ctx, 'mail.server.config.get', {});
  const current = config?.runtime_config;
  if (!current || typeof current !== 'object') fail('unavailable', 'mailserver runtime settings are not available');
  const settings = buildRuntimeSettings(payloadOf(command), current);
  // Kern: stalwart_config-Prüfung, save_runtime_settings, apply_runtime_settings → { runtime_status }.
  const applied = await callCap(ctx, 'mail.server.runtime.save', { settings });
  return { saved: true, runtime_config: settings, runtime_status: applied?.runtime_status ?? null };
}
