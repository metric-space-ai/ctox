// Origin: CTOX
// License: AGPL-3.0-only (Gates, ref ats_gates.rs) / Apache-2.0 (Befehlsablauf, ref store_ats_commands.rs)
// Gemeinsame ATS-Fachlogik der v4-Handler (Strang H8, Shell v4) @ 4794af03d:
//   src/core/business_os/ats_gates.rs (Gate-Primitive, 1:1 portiert) und
//   src/core/business_os/store_ats_commands.rs:1-385 (Helfer, Allowlist, Rechnungsentwurf, Audit-Ereignis).
// Datei ist in consent, esign, intake, interviews, nachweise, placements, submissions byte-gleich kopiert
// (App-Artefakte importieren nicht über App-Grenzen).

export function fail(code, message, extra = {}) {
  const error = new Error(message);
  error.code = code;
  Object.assign(error, extra);
  throw error;
}

export const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
export const payloadOf = (command) => (isObject(command?.payload) ? command.payload : {});
const fieldStr = (record, key) => (typeof record?.[key] === 'string' ? record[key] : undefined);
const fieldI64 = (record, key) => (typeof record?.[key] === 'number' && Number.isInteger(record[key]) ? record[key] : undefined);
const fieldBool = (record, key) => (typeof record?.[key] === 'boolean' ? record[key] : undefined);
export const asI64 = (value) => (typeof value === 'number' && Number.isInteger(value) ? value : undefined);
/** serde as_f64: jede JSON-Zahl */
export const asF64 = (value) => (typeof value === 'number' ? value : undefined);

/** ref: store.rs:26959 first_string_field */
export function firstStringField(value, keys) {
  if (!isObject(value)) return undefined;
  for (const key of keys) {
    const item = value[key];
    if (typeof item === 'string' && item.trim() !== '') return item.trim();
  }
  return undefined;
}

/** ref: store.rs:24300 checked_scaled */
export function checkedScaled(amount, scale) {
  if (typeof amount !== 'number' || !Number.isFinite(amount)) return 0;
  return roundHalfAwayFromZero(Math.min(Math.max(amount, -1.0e12), 1.0e12) * scale);
}

/** f64::round (halbe Werte von der Null weg; Math.round rundet -0.5 zur Null) */
export function roundHalfAwayFromZero(value) {
  return Math.sign(value) * Math.round(Math.abs(value));
}

const DAY_MS = 24 * 60 * 60 * 1000;
export const EXPIRY_WARN_DAYS = 30;

// ---------------------------------------------------------------------------- Credential / deployment gate
/** ref: ats_gates.rs:51 credential_status → 'valid'|'expiring'|'expired'|'notyetvalid'|'unverified' */
export function credentialStatus(credential, nowMs) {
  if (fieldBool(credential, 'verified') !== true) return 'unverified';
  const from = fieldI64(credential, 'valid_from_ms');
  if (from !== undefined && from > 0 && nowMs < from) return 'notyetvalid';
  const until = fieldI64(credential, 'valid_until_ms');
  if (until === undefined || until === 0) return 'valid';
  if (until <= nowMs) return 'expired';
  // Rust: (until - now) / DAY_MS – ganzzahlige Division
  if (Math.trunc((until - nowMs) / DAY_MS) <= EXPIRY_WARN_DAYS) return 'expiring';
  return 'valid';
}

const isDeployable = (credential, nowMs) => ['valid', 'expiring'].includes(credentialStatus(credential, nowMs));

/** ref: ats_gates.rs:110 evaluate_deployment_readiness → {ready, blockers:[[type, reason]]} */
export function evaluateDeploymentReadiness(credentials, requiredTypes, nowMs) {
  const typesToCheck = [...requiredTypes];
  for (const credential of credentials) {
    if (fieldBool(credential, 'deployment_blocking') === true) {
      const ty = fieldStr(credential, 'credential_type');
      if (ty !== undefined && !typesToCheck.includes(ty)) typesToCheck.push(ty);
    }
  }
  const blockers = [];
  for (const ty of typesToCheck) {
    const ofType = credentials.filter((c) => fieldStr(c, 'credential_type') === ty);
    if (ofType.length === 0) {
      blockers.push([ty, 'missing']);
      continue;
    }
    if (!ofType.some((c) => isDeployable(c, nowMs))) blockers.push([ty, credentialStatus(ofType[0], nowMs)]);
  }
  return { ready: blockers.length === 0, blockers };
}

export const blockerJson = (blockers) => blockers.map(([ty, reason]) => ({ credential_type: ty, reason }));

// ---------------------------------------------------------------------------- Consent / retention gate
/** ref: ats_gates.rs:156 consent_valid */
export function consentValid(consent, nowMs, requireEvidence) {
  const basis = fieldStr(consent, 'legal_basis') ?? '';
  const withdrawable = basis === '' || basis === 'consent' || basis === 'special_category_consent';
  if (!withdrawable) {
    if (!['contract', 'legal_obligation', 'legitimate_interest'].includes(basis)) return false;
    if (requireEvidence) {
      const evidence = fieldStr(consent, 'basis_evidence');
      return evidence !== undefined && evidence.trim() !== '';
    }
    return true;
  }
  if ((fieldI64(consent, 'granted_at_ms') ?? 0) <= 0) return false;
  const withdrawn = fieldI64(consent, 'withdrawn_at_ms');
  if (withdrawn !== undefined && withdrawn > 0 && withdrawn <= nowMs) return false;
  const expires = fieldI64(consent, 'expires_at_ms');
  if (expires !== undefined && expires > 0 && expires <= nowMs) return false;
  return true;
}

/** ref: ats_gates.rs:198 has_valid_consent */
export const hasValidConsent = (consents, purpose, nowMs, requireEvidence) =>
  consents.some((c) => fieldStr(c, 'purpose') === purpose && consentValid(c, nowMs, requireEvidence));

/** ref: ats_gates.rs:211 evaluate_consent_gate (purpose: string|undefined) */
export function evaluateConsentGate(purpose, consents, nowMs, requireEvidence) {
  if (purpose === undefined || purpose === null || purpose === '') return true;
  return hasValidConsent(consents, purpose, nowMs, requireEvidence);
}

/** ref: ats_gates.rs:226 retention_due */
export function retentionDue(referenceMs, retentionDays, nowMs) {
  if (referenceMs <= 0 || retentionDays <= 0) return false;
  return nowMs >= referenceMs + retentionDays * DAY_MS;
}

// ---------------------------------------------------------------------------- Submission guard + signature status
/** ref: ats_gates.rs:241 find_double_submission → id | '' | undefined */
export function findDoubleSubmission(existing, candidateId, clientAccountId, withinDays, nowMs) {
  const window = Math.max(withinDays, 0) * DAY_MS;
  for (const submission of existing) {
    if (fieldStr(submission, 'status') === 'withdrawn') continue;
    if (fieldStr(submission, 'candidate_id') !== candidateId) continue;
    if (fieldStr(submission, 'client_account_id') !== clientAccountId) continue;
    const sentAt = fieldI64(submission, 'sent_at_ms');
    const within = sentAt === undefined ? true : nowMs - sentAt <= window;
    if (within) return fieldStr(submission, 'id') ?? '';
  }
  return undefined;
}

/** ref: ats_gates.rs:273 signature_request_status */
export function signatureRequestStatus(signers, expiresAtMs, sentAtMs, nowMs) {
  if (signers.some((s) => fieldStr(s, 'state') === 'declined')) return 'declined';
  if (signers.length > 0 && signers.every((s) => fieldStr(s, 'state') === 'signed')) return 'completed';
  if (expiresAtMs !== undefined && expiresAtMs > 0 && nowMs >= expiresAtMs) return 'expired';
  if (signers.some((s) => fieldStr(s, 'state') === 'signed')) return 'partially_signed';
  if ((sentAtMs ?? 0) > 0) return 'sent';
  return 'created';
}

// ---------------------------------------------------------------------------- Leistungsnachweis billing
export const HOUR_TYPES = ['regular', 'nacht', 'sonntag', 'feiertag', 'mehrarbeit'];
const round2 = (value) => roundHalfAwayFromZero(value * 100) / 100;
const entryHours = (entry) => (typeof entry?.hours === 'number' ? entry.hours : 0);
const entryType = (entry) => (typeof entry?.type === 'string' && HOUR_TYPES.includes(entry.type) ? entry.type : 'regular');

/** ref: ats_gates.rs:345 compute_nachweis_billing */
export function computeNachweisBilling(entries, chargeRate, surchargePct) {
  const lines = [];
  let netTotal = 0;
  let totalHours = 0;
  for (const ty of HOUR_TYPES) {
    const hours = entries.filter((e) => entryType(e) === ty).map(entryHours).reduce((a, b) => a + b, 0);
    if (hours === 0) continue;
    totalHours += hours;
    const pct = ty === 'regular' ? 0 : (typeof surchargePct?.[ty] === 'number' ? surchargePct[ty] : 0);
    const rate = round2(chargeRate * (1 + pct / 100));
    const amount = round2(hours * rate);
    netTotal += amount;
    lines.push({ category: ty, hours, rate, amount });
  }
  return { total_hours: totalHours, net_total: round2(netTotal), lines };
}

/** ref: ats_gates.rs:386 evaluate_billing_gate */
export function evaluateBillingGate(nachweis) {
  const blockers = [];
  const signed = fieldBool(nachweis, 'entleiher_signed') === true && (fieldI64(nachweis, 'signed_at_ms') ?? 0) > 0;
  if (!signed) blockers.push('entleiher_signature_missing');
  const entries = Array.isArray(nachweis?.entries) ? nachweis.entries : undefined;
  if (entries === undefined || entries.length === 0) {
    blockers.push('no_time_entries');
  } else {
    // JSON kennt kein NaN/∞; "nicht endlich" kann nur aus Nicht-Zahlen (→ 0) nicht entstehen.
    const hours = entries.map(entryHours);
    if (hours.some((h) => !Number.isFinite(h) || h < 0)) blockers.push('invalid_hours');
    else if (hours.reduce((a, b) => a + b, 0) <= 0) blockers.push('no_billable_hours');
  }
  return blockers;
}

// ---------------------------------------------------------------------------- Befehlsablauf-Helfer (store_ats_commands.rs)
export const ATS_REASON_CONSENT_MISSING = 'consent_to_present_missing';
export const ATS_REASON_DOUBLE_SUBMISSION = 'double_submission';

/** ref: store_ats_commands.rs:264 ATS_WRITABLE_COLLECTIONS */
export const ATS_WRITABLE_COLLECTIONS = [
  'applications', 'business_credentials', 'business_consents', 'submissions', 'offers', 'placements',
  'interview_scorecards', 'interview_meetings', 'signature_requests', 'planning_time_records',
];

/** ref: :277 ensure_ats_collection_allowed */
export function ensureAtsCollectionAllowed(collection) {
  if (!ATS_WRITABLE_COLLECTIONS.includes(collection)) {
    fail('forbidden', `collection ${JSON.stringify(collection)} is not an ATS-writable collection`);
  }
}

/** ref: :188 ats_actor_value – v4: command.actor {id, role} ersetzt die Sitzung */
export function atsActorValue(command) {
  const actor = isObject(command?.actor) ? command.actor : undefined;
  if (actor && typeof actor.id === 'string' && actor.id !== '') {
    const value = { id: actor.id, role: actor.role ?? null, trusted: true };
    if (typeof actor.display_name === 'string') value.display_name = actor.display_name;
    return value;
  }
  return { id: 'rxdb-command', trusted: false };
}

/** ref: :911-919 / :1012-1019 – authentifizierter Handelnder, nie der anonyme Befehlsweg */
export function authenticatedActorId(command, message) {
  const id = typeof command?.actor?.id === 'string' ? command.actor.id : '';
  if (id === '' || id === 'rxdb-command') fail('forbidden', message);
  return id;
}

/**
 * ref: :206 record_ats_governance_event → business_events (Kern-Prüfspur, keine App-Collection).
 * Capability-Wunsch ctx.cap.audit.event (gleiche Signatur wie outbound, Strang H3). Externe Wirkung → nie in tx.
 */
export async function recordAtsGovernanceEvent(ctx, command, actor, eventType, summary, observedAtMs) {
  if (typeof ctx?.cap?.audit?.event !== 'function') fail('unavailable', 'capability ctx.cap.audit.event is not available');
  await ctx.cap.audit.event({
    collection: 'business_commands',
    record_id: typeof command?.record_id === 'string' ? command.record_id : '',
    event_type: eventType,
    payload: {
      event_type: eventType,
      command_type: command.type,
      module: command.app_id,
      actor,
      summary,
      observed_at_ms: observedAtMs,
    },
    observed_at_ms: observedAtMs,
  });
}

/**
 * Laufzeitkonfiguration (Rust: crate::inference::runtime_env::env_or_config, SQLite-Laufzeitstore).
 * Capability-Wunsch ctx.cap.config.get({key}) → {value: string|null} (neu, nicht in E §3).
 */
export async function runtimeConfig(ctx, key) {
  if (typeof ctx?.cap?.config?.get !== 'function') fail('unavailable', 'capability ctx.cap.config.get is not available');
  const out = await ctx.cap.config.get({ key });
  return typeof out?.value === 'string' ? out.value : undefined;
}
export const configFlag = async (ctx, key) => (await runtimeConfig(ctx, key)) === '1';

/** ref: :221 ats_invoice_line */
export function atsInvoiceLine(position, description, amountEur, taxRate) {
  return { position, description, quantity: 1000, unit_price_cents: checkedScaled(amountEur, 100), tax_rate: taxRate, account_code: '8400' };
}

/** ref: :235 ats_canonical_invoice – kanonischer accounting_invoices-Entwurf */
export function atsCanonicalInvoice(id, invoiceType, partyId, lines, now, sourceField, sourceId) {
  let netTotal = 0;
  let taxTotal = 0;
  for (const line of lines) {
    const quantity = asI64(line.quantity) ?? 0;
    const unitPrice = asI64(line.unit_price_cents) ?? 0;
    const net = roundHalfAwayFromZero((unitPrice * quantity) / 1000);
    const tax = roundHalfAwayFromZero(net * (asF64(line.tax_rate) ?? 0));
    netTotal += net;
    taxTotal += tax;
  }
  const total = netTotal + taxTotal;
  return {
    id,
    invoice_type: invoiceType,
    party_id: partyId,
    currency: 'EUR',
    state: 'draft',
    invoice_date_ms: now,
    lines,
    subtotal_cents: netTotal,
    tax_cents: taxTotal,
    total_cents: total,
    paid_cents: 0,
    open_cents: total,
    search_text: partyId.toLowerCase(),
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
    [sourceField]: sourceId,
  };
}

/** ref: store_projections.rs:1397 upsert_business_record – id/updated_at_ms setzen; _rev und _deleted vergibt der Kern (E47, reserviert). */
export async function upsertRecord(store, collection, id, now, payload) {
  const doc = { ...payload, id, updated_at_ms: now };
  await store.put(collection, doc);
  return doc;
}

/** ref: :17 load_business_records_by_field (nicht gelöscht, JSON-Feld = Wert) */
export const loadByField = (store, collection, field, value) => store.read(collection, { selector: { [field]: value } });
