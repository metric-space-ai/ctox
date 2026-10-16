// outbound.update_digest.send_now — GEMISCHT: Bericht/Regeln hier, Versand und Laufzeitzustand über ctx.cap.*
// ref: store_outbound_commands.rs:1184 -> outbound_update_digest.rs:1552 send_now
//      :1339 load_config, :1350 resolve_sender, :1368 publish_status, :1421 build_current_report,
//      :1442 send_report, :1543 explicit_since_ms
//
// Capabilities (native):
//   Versandzustand (DigestState: last_sent_slot/at, attempt_slot, attempts, last_attempt_at_ms, last_error) liegt im
//   eigenen Statusdatensatz DIGEST_STATUS_ID der App-Collection POLICY_COLLECTION (Strang CAP3A; vorher
//   ctx.cap.runtime_state.read/write auf runtime/outbound-update-digest-state.json – laut CAP-3 #831 App-Logik, kein
//   Core-Primitiv). Ein Schreibvorgang (publishStatus) trägt Zustand und Statusanzeige zugleich.
//   Scrape-Läufe: App-Collections (_lib/scrape-registry.js registryRuns, Strang OLGSCR; vorher Core-SQLite)
//   ctx.cap.config.get({key:'CTO_EMAIL_ADDRESS'}) → {value|null}   (CATALOG §14 config; Instanz-Absender, wenn die
//                                                                  App-Einstellung sender_email fehlt; vorher mail.default_sender)
//   ctx.cap.mail.send({account_id, to, subject, text, html, idempotency_key}) → {message_id, status}
//                                                            (Kernform caps/mail.rs:129; account_id = Absenderadresse,
//                                                            der Kern prüft sie gegen Konten/Instanz-Adresse – resolve_sender;
//                                                            Aufruf in _lib/policy-report.js mit Freigabe-Buchführung, OLG3)
// The scheduled `tick` (outbound_update_digest.rs:1469) is NOT a command type; as an `every: PT1M` trigger (§16.20)
// it must read the same status record so manual and scheduled sends keep one "since" marker.
import { fail, has, isObject } from './_lib/olg.js';
import {
  ADAPTER_COLLECTION, DIGEST_CONFIG_ID, DIGEST_STATUS_ID, FIRST_REPORT_WINDOW_MS, LEAD_COLLECTION, POLICY_COLLECTION,
  SOURCE_COLLECTION, buildReport, foldRegistryRuns, isPlausibleEmail, markAsCorrection, normalizeState,
  parseConfig, parseRecipients,
} from './_lib/update-digest.js';
import { registryRuns as readRegistryRuns } from './_lib/scrape-registry.js';
import { DIGEST_POLICY_SUMMARY, recordAndSendPolicyReportEmail } from './_lib/policy-report.js';

/** ref: :1543 */
export function explicitSinceMs(payload, nowMs) {
  const since = has(payload, 'since_ms') && Number.isInteger(payload.since_ms) ? payload.since_ms : null;
  if (since == null) return null;
  return since > 0 && since < nowMs && nowMs - since <= 31 * 24 * 60 * 60 * 1000 ? since : null;
}

/** ref: :1323 — unreadable/invalid state → default. Quelle: eigener Statusdatensatz (App-Collection). */
async function loadState(ctx) {
  try {
    const doc = await ctx.get(POLICY_COLLECTION, DIGEST_STATUS_ID);
    return normalizeState(doc && doc._deleted !== true && doc.is_deleted !== true ? doc : null);
  } catch {
    return normalizeState(null);
  }
}

/** load_rxdb_collection_records: live records, drops is_deleted / _deleted. */
async function liveRecords(ctx, collection) {
  const rows = await ctx.read(collection, {});
  return rows.filter((row) => !(row?.is_deleted === true || row?._deleted === true));
}

/** ref: :1362 — an unreadable registry yields an empty map. */
async function registryRuns(ctx) {
  try {
    return foldRegistryRuns(await readRegistryRuns(ctx));
  } catch {
    return new Map();
  }
}

/** ref: :1350 */
async function resolveSender(ctx, config) {
  if (isPlausibleEmail(config.sender_email)) return config.sender_email;
  try {
    const answer = await ctx.cap.config.get({ key: 'CTO_EMAIL_ADDRESS' });
    const value = typeof answer?.value === 'string' ? answer.value.trim().replace(/[A-Z]/g, (c) => c.toLowerCase()) : '';
    return isPlausibleEmail(value) ? value : null;
  } catch {
    return null; // Rust `.ok()?`
  }
}

/** ref: :1368 — projection failure is only logged. */
async function publishStatus(ctx, state, extra) {
  const now = ctx.now();
  const doc = {
    id: DIGEST_STATUS_ID,
    title: 'Update-Verteiler Status',
    status: 'digest_status',
    version_number: 1,
    skill_name: '',
    skill_version: '',
    min_independent_sources: 0,
    rules: [],
    last_sent_slot: state.last_sent_slot,
    last_sent_at_ms: state.last_sent_at_ms,
    attempt_slot: state.attempt_slot,
    last_attempt_at_ms: state.last_attempt_at_ms,
    attempts: state.attempts,
    last_error: state.last_error,
    created_at_ms: now,
    updated_at_ms: now,
    ...JSON.parse(JSON.stringify(extra)),
  };
  try {
    await ctx.put(POLICY_COLLECTION, doc);
  } catch (error) {
    ctx.log('warn', 'outbound-digest status projection failed', { error: error.message });
  }
}

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  // The drawer may send its unsaved form so "Vorschau" works before saving.
  let config = has(payload, 'update_digest') ? parseConfig(payload) : null;
  if (config == null) {
    const doc = await ctx.get(POLICY_COLLECTION, DIGEST_CONFIG_ID);
    config = doc && doc._deleted !== true ? parseConfig(doc) : null;
    if (config == null) {
      fail('not_found', 'Update-Verteiler ist noch nicht gespeichert. Bitte Empfänger und Zeitplan speichern.');
    }
  }
  const dryRun = payload.dry_run === true;
  const explicit = parseRecipients(payload.recipients);
  const toDistribution = explicit.length === 0;
  const recipients = toDistribution ? config.recipients.slice() : explicit;
  if (!(dryRun || recipients.length > 0)) fail('invalid', 'Keine gültige Empfängeradresse hinterlegt.');
  const nowMs = ctx.now();
  const state = await loadState(ctx);
  const sinceMs = explicitSinceMs(payload, nowMs)
    ?? (state.last_sent_at_ms > 0 ? state.last_sent_at_ms : nowMs - FIRST_REPORT_WINDOW_MS);
  const leads = await liveRecords(ctx, LEAD_COLLECTION);
  const sources = await liveRecords(ctx, SOURCE_COLLECTION);
  const adapters = await liveRecords(ctx, ADAPTER_COLLECTION);
  const registry = await registryRuns(ctx);
  const report = buildReport(leads, sources, adapters, registry, sinceMs, nowMs, config.timezone);
  // An update to the whole distribution reaching back before the last update replaces it.
  if (toDistribution && state.last_sent_at_ms > 0 && sinceMs < state.last_sent_at_ms) {
    markAsCorrection(report, state.last_sent_at_ms, config.timezone);
  }
  if (dryRun) {
    return {
      ok: true,
      dry_run: true,
      subject: report.subject,
      body: report.body,
      html: report.html,
      recipients,
      sender: await resolveSender(ctx, config),
      stats: report.stats,
    };
  }
  const slotKey = `outbound-update-test:${nowMs}`;
  let delivery = null;
  let sendError = null;
  try {
    const sender = await resolveSender(ctx, config);
    if (sender == null) fail('unavailable', 'kein Absender-Postfach konfiguriert und keine Instanz-Adresse vorhanden');
    // ref: :1442 send_report → record_and_send_policy_report_email. Seit OLG3 wieder mit Policy-Report-Buchführung,
    // jetzt als App-Logik (_lib/policy-report.js, Collection outbound_lead_generation_policy_report_reviews);
    // Versand über die Kernform mail.send (caps/mail.rs:129).
    delivery = await recordAndSendPolicyReportEmail(ctx, {
      sender_email: sender,
      to: recipients,
      subject: report.subject,
      body: report.body,
      body_html: report.html,
      report_key: slotKey,
      policy_summary: DIGEST_POLICY_SUMMARY,
    });
  } catch (error) {
    sendError = error;
  }
  state.last_attempt_at_ms = nowMs;
  if (sendError == null) {
    state.last_error = '';
    // A manual update to the whole distribution is an update: the next scheduled one reports from here.
    if (toDistribution) state.last_sent_at_ms = nowMs;
  } else {
    state.last_error = Array.from(String(sendError.message)).slice(0, 400).join('');
  }
  // Rust `let _ = save_state(...)`: der Zustand wird mit dem Statusdatensatz geschrieben (Fehler nur geloggt).
  await publishStatus(ctx, state, {
    last_test_at_ms: nowMs,
    last_test_ok: sendError == null,
    last_test_recipients: recipients,
    last_test_subject: report.subject,
    last_kind: 'test',
  });
  if (sendError) throw sendError;
  return {
    ok: true,
    subject: report.subject,
    recipients,
    stats: report.stats,
    delivery_status: has(delivery, 'status') ? delivery.status : null,
  };
}
