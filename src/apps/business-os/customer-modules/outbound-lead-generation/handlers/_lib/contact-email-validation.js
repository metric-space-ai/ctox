// Kontakt-E-Mail-Prüfung als App-Logik (Strang OLG3; vorher Core-Fähigkeit mail.validate_contacts).
//
// ref: contact_email_validation.rs (Rust-Stand 4794af03d)
//      :38-41 Konstanten, :43-82 VerdictKind/EmailVerdict, :264 verdict_from_records,
//      :296 apply_email_verdicts, :431 promote_pattern_email, :493 native_email_field_status,
//      :519 legacy_validation_placeholder, :548 merged_email_receipts, :612 reconcile_receipted_email_status,
//      :1141 run_validation, :1160 run_validation_once, :1200 validate_lead_contacts,
//      :1312-1313 SWEEP_INTERVAL/RETRY_AFTER_MS, :1318 lead_due_for_sweep
//      person_research_gap_closure.rs :1552 complete_after_native_email_validation_with_native_reviews,
//      :1567 complete_after_native_email_validation
//      store.rs:12742 upsert_native_email_validation_record (CAS gegen expected_master, Zeugen Issuance::Preserve)
//
// Prüflauf: das registrierte Scrape-Ziel `experte-de` der App-Registry (scrape-registry.js), ausgeführt über
// das OS-Primitiv process.run (scrape-runner.js executeScrape). Das Urteil steht in den Records des Laufs
// (Artefakt result_json), nicht im geteilten Record-Speicher (Begründung :20-26).
//
// Der Hintergrund-Thread (spawn_contact_email_validation :1290) ist der App-Befehl
// `outbound.contacts.email_validate` (ctx.command); der stündliche Sweep (:1340) der Trigger-Handler
// `outbound.contacts.email_validation_sweep` (every PT1H).
import { asU64, fail, has, isObject } from './olg.js';
import { LEAD_COLLECTION, cmpStr } from './update-digest.js';
import { jsonEqual } from './research-source-adapter.js';
import { RUNS, readFileText } from './scrape-registry.js';
import { executeScrape } from './scrape-runner.js';
import { loadWitnesses, persistWritebackWitnesses, readCurrentWitnesses } from './field-status-witnesses.js';
import {
  alignLeadEmailValidation, contactEmail, emailReceiptEntries, emailsNeedingValidation, nativeReviewViewFromWitnesses,
  normalizeEmail, openPersonsForField, parseVerdictNote, researchFieldIsAnswered,
} from './research-field-status.js';

// ref: :38-41
export const VALIDATION_TARGET_KEY = 'experte-de';
export const VALIDATION_SOURCE_ID = 'experte.de';
export const MAX_ADDRESSES_PER_PASS = 4;
export const INCONCLUSIVE_LABEL = 'unknown_catch_all';
// ref: :1312-1313
export const SWEEP_INTERVAL_MS = 3600 * 1000;
export const RETRY_AFTER_MS = 6 * 3600 * 1000;
export const VALIDATE_COMMAND = 'outbound.contacts.email_validate';
const VALIDATION_URL = 'https://www.experte.de/email-pruefen';
const NATIVE_VIA = ['native_email_validation', 'native_email_validation_receipt'];

const get = (v, k) => (has(v, k) ? v[k] : undefined);
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
const strOf = (v) => (typeof v === 'string' ? v : null);
const trimmedKey = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);
/** serde `a.get(k) == b.get(k)` (Option<&Value>). */
const sameOpt = (a, b, k) => (get(a, k) === undefined ? get(b, k) === undefined : get(b, k) !== undefined && jsonEqual(a[k], b[k]));
const ensureObject = (holder, key) => {
  if (!isObject(get(holder, key))) holder[key] = {};
  return holder[key];
};

/** ref: :59 VerdictKind::label — kinds are already the labels ('valid' | 'invalid' | 'unknown_catch_all'). */
const isValid = (verdict) => verdict.kind === 'valid';

/** ref: :264 — verdict for `requested` from one run's records; a verdict for any other address is ignored. */
export function verdictFromRecords(records, requested, runId) {
  const wanted = normalizeEmail(requested);
  if (wanted == null) return null;
  for (const record of Array.isArray(records) ? records : []) {
    if (get(record, 'field') !== 'person_email_validation') continue;
    const note = strOf(get(record, 'note')) ?? '';
    const parsed = parseVerdictNote(note);
    if (parsed == null) continue;
    const [email, kind] = parsed;
    if (email !== wanted) continue;
    return { email, kind, note: note.trim(), source_url: strOf(get(record, 'source_url')) ?? VALIDATION_URL, run_id: runId };
  }
  return null;
}

/** ref: :493 */
export function nativeEmailFieldStatus(verdict, personKey) {
  const inconclusive = verdict.kind === INCONCLUSIVE_LABEL;
  const status = {
    status: inconclusive ? 'action_required' : 'verified',
    value: inconclusive ? null : verdict.kind,
    verdict: verdict.kind,
    sources: [{ source_id: VALIDATION_SOURCE_ID, url: verdict.source_url, quote: verdict.note }],
    attempts: [],
    reason: `Vom Daemon ueber ${VALIDATION_SOURCE_ID} geprueft: ${verdict.email}`,
    validated_email: verdict.email,
    run_id: verdict.run_id,
    via: 'native_email_validation',
  };
  if (personKey != null) status.person_key = personKey;
  return status;
}

/** ref: :431 — a confirmed deliverable pattern address becomes the person's verified e-mail. */
function promotePatternEmail(lead, personKey, verdict) {
  if (!isValid(verdict)) return;
  const promote = (status) => {
    const value = strOf(get(status, 'value'));
    const claimed = value == null ? null : normalizeEmail(value);
    if (claimed !== verdict.email || get(status, 'status') === 'verified') return;
    const sources = Array.isArray(get(status, 'sources')) ? clone(status.sources) : [];
    sources.push({ source_id: VALIDATION_SOURCE_ID, url: verdict.source_url, quote: verdict.note });
    status.status = 'verified';
    status.sources = sources;
    status.reason = `Aus dem Adressmuster gebildet; ${VALIDATION_SOURCE_ID} bestaetigt die Zustellbarkeit (${verdict.note})`;
    status.validated_email = verdict.email;
    status.run_id = verdict.run_id;
  };
  if (personKey != null) {
    const status = get(get(get(lead, 'person_field_status'), personKey), 'person_email');
    if (status !== undefined) promote(status);
  }
  for (const contact of Array.isArray(get(lead, 'contacts')) ? lead.contacts : []) {
    const rawOwner = get(contact, 'person_key');
    const owner = typeof rawOwner === 'string' ? rawOwner.trim() : null;
    if (owner !== personKey || contactEmail(contact) !== verdict.email) continue;
    const status = get(get(contact, 'field_status'), 'person_email');
    if (status !== undefined) promote(status);
  }
}

/** ref: :296 — attaches each verdict to every contact owning the address; returns the changed contact count. */
export function applyEmailVerdicts(lead, verdicts) {
  if (verdicts.length === 0) return 0;
  let changed = 0;
  const newEvidence = [];
  const matched = [];
  for (const contact of Array.isArray(get(lead, 'contacts')) ? lead.contacts : []) {
    const email = contactEmail(contact);
    if (email == null) continue;
    const verdict = verdicts.find((v) => v.email === email);
    if (verdict === undefined) continue;
    const label = verdict.kind;
    contact.person_email_validation = label;
    const personKey = trimmedKey(get(contact, 'person_key'));
    ensureObject(contact, 'field_status').person_email_validation = nativeEmailFieldStatus(verdict, personKey);
    matched.push([personKey, verdict]);
    changed += 1;
    newEvidence.push({
      field_key: 'person_email_validation',
      value: label,
      source_id: VALIDATION_SOURCE_ID,
      source_url: verdict.source_url,
      quote: verdict.note,
      person_key: personKey,
      validated_email: verdict.email,
      run_id: verdict.run_id,
      via: 'native_email_validation',
      label: VALIDATION_SOURCE_ID,
      evidence_gate: verdict.kind === INCONCLUSIVE_LABEL
        ? { verification_status: 'action_required', evidence_eligible: false }
        : { verification_status: 'verified', evidence_eligible: true },
    });
  }
  if (changed === 0) return 0;
  // Canonical per-person status; contacts[].field_status is its projection.
  for (const [personKey, verdict] of matched) {
    if (personKey == null) continue;
    ensureObject(ensureObject(lead, 'person_field_status'), personKey).person_email_validation = nativeEmailFieldStatus(verdict, personKey);
  }
  for (const [personKey, verdict] of matched) promotePatternEmail(lead, personKey, verdict);
  if (!Array.isArray(get(lead, 'evidence'))) lead.evidence = [];
  for (const entry of newEvidence) {
    const duplicate = lead.evidence.some((existing) => ['field_key', 'validated_email', 'person_key', 'value']
      .every((k) => sameOpt(existing, entry, k)));
    if (!duplicate) lead.evidence.push(clone(entry));
  }
  // The lead-level field is answered as soon as one contact is checked; deliverable beats undeliverable.
  const best = (matched.find(([, v]) => isValid(v)) ?? matched.find(([, v]) => v.kind === 'invalid') ?? matched[0])[1];
  const alreadyValid = get(get(get(lead, 'field_status'), 'person_email_validation'), 'value') === 'valid';
  if (!alreadyValid) ensureObject(lead, 'field_status').person_email_validation = nativeEmailFieldStatus(best, null);
  return changed;
}

/** ref: :519 */
export function legacyValidationPlaceholder(lead) {
  const status = get(get(lead, 'field_status'), 'person_email_validation');
  const reason = strOf(get(status, 'reason'));
  const lowered = reason == null ? '' : reason.replace(/[A-Z]/g, (c) => c.toLowerCase());
  return get(status, 'status') === 'action_required' && reason != null
    && lowered.includes('daemon') && lowered.includes('experte.de')
    && emailReceiptEntries(lead).length > 0;
}

/** ref: :548 — address-keyed receipt map over the current addresses (BTreeMap → sorted keys). */
export function mergedEmailReceipts(lead, verdicts) {
  const current = new Set((Array.isArray(get(lead, 'contacts')) ? lead.contacts : []).map(contactEmail).filter((e) => e != null));
  const receipts = new Map();
  const lastPass = get(get(get(lead, 'payload'), 'email_validation_pass'), 'checked');
  for (const entry of [...emailReceiptEntries(lead), ...(Array.isArray(lastPass) ? lastPass : [])]) {
    const raw = strOf(get(entry, 'email'));
    const email = raw == null ? null : normalizeEmail(raw);
    if (email == null) continue;
    const runId = strOf(get(entry, 'run_id'));
    if (current.has(email) && typeof get(entry, 'valid') === 'boolean' && runId != null && runId !== '') {
      const receipt = { email, valid: entry.valid, run_id: entry.run_id };
      if (typeof get(entry, 'verdict') === 'string') receipt.verdict = entry.verdict;
      receipts.set(email, receipt);
    }
  }
  for (const verdict of verdicts) {
    if (current.has(verdict.email)) {
      receipts.set(verdict.email, { email: verdict.email, valid: isValid(verdict), verdict: verdict.kind, run_id: verdict.run_id });
    }
  }
  return Object.fromEntries([...receipts.keys()].sort(cmpStr).map((k) => [k, receipts.get(k)]));
}

/**
 * ref: :656-666 — run id of a saved native receipt.
 * ABWEICHUNG: zusätzlich zum Rust-Präfix `scrape_run-` (importierte Läufe) gilt `run-` (Läufer der App,
 * scrape-runner.js); Länge ≤ 128 und Zeichensatz [A-Za-z0-9_-] unverändert.
 */
export function receiptRunIdOk(id) {
  return typeof id === 'string' && (id.startsWith('scrape_run-') || id.startsWith('run-')) && id.length <= 128
    && /^[A-Za-z0-9_-]+$/.test(id);
}

/** ref: :612 — rebuilds only statuses whose saved native receipt agrees; returns the repaired count. */
export function reconcileReceiptedEmailStatus(lead) {
  if (!legacyValidationPlaceholder(lead)) return 0;
  const checked = emailReceiptEntries(lead);
  const contacts = get(lead, 'contacts');
  if (!Array.isArray(contacts)) return 0;
  const seenKeys = new Set();
  const seenEmails = new Set();
  let addressCount = 0;
  const matches = [];
  for (let index = 0; index < contacts.length; index += 1) {
    const contact = contacts[index];
    const email = contactEmail(contact);
    if (email == null) continue;
    addressCount += 1;
    const personKey = trimmedKey(get(contact, 'person_key'));
    if (personKey == null) continue;
    if (seenKeys.has(personKey) || seenEmails.has(email)) return 0;
    seenKeys.add(personKey);
    seenEmails.add(email);
    const scalar = get(contact, 'person_email_validation');
    if (scalar !== 'valid' && scalar !== 'invalid') continue;
    const valid = scalar === 'valid';
    const receipts = checked.filter((entry) => {
      const raw = strOf(get(entry, 'email'));
      return raw != null && normalizeEmail(raw) === email;
    });
    if (receipts.length !== 1 || get(receipts[0], 'valid') !== valid) continue;
    const runId = strOf(get(receipts[0], 'run_id'));
    if (!receiptRunIdOk(runId)) continue;
    const oldCanonical = get(get(get(lead, 'person_field_status'), personKey), 'person_email_validation');
    const oldProjected = get(get(contact, 'field_status'), 'person_email_validation');
    if ([oldCanonical, oldProjected].some((s) => typeof get(s, 'status') === 'string' && s.status !== 'action_required')) continue;
    matches.push([index, personKey, email, valid, runId]);
  }
  if (matches.length === 0) return 0;
  for (const [index, personKey, email, valid, runId] of matches) {
    const status = {
      status: 'verified',
      value: valid ? 'valid' : 'invalid',
      sources: [{ source_id: VALIDATION_SOURCE_ID, url: VALIDATION_URL, run_id: runId }],
      attempts: [],
      reason: `Aus gespeichertem nativen experte.de-Prueflauf ${runId} rekonstruiert; Originalzitat nicht erneut gelesen`,
      validated_email: email,
      run_id: runId,
      via: 'native_email_validation_receipt',
      person_key: personKey,
    };
    ensureObject(lead.contacts[index], 'field_status').person_email_validation = clone(status);
    ensureObject(ensureObject(lead, 'person_field_status'), personKey).person_email_validation = status;
  }
  // Every current address must be bound to its receipt before the lead-level field mirrors one.
  const bound = [];
  for (const contact of lead.contacts) {
    const email = contactEmail(contact);
    const personKey = strOf(get(contact, 'person_key'));
    const scalar = strOf(get(contact, 'person_email_validation'));
    if (email == null || personKey == null || (scalar !== 'valid' && scalar !== 'invalid')) continue;
    const valid = scalar === 'valid';
    const receipts = checked.filter((entry) => get(entry, 'email') === email);
    if (receipts.length !== 1 || get(receipts[0], 'valid') !== valid) continue;
    const runId = strOf(get(receipts[0], 'run_id'));
    if (runId == null) continue;
    const canonical = get(get(get(lead, 'person_field_status'), personKey), 'person_email_validation');
    const projected = get(get(contact, 'field_status'), 'person_email_validation');
    if (canonical === undefined || projected === undefined) continue;
    const bad = [canonical, projected].some((s) => {
      const validated = strOf(get(s, 'validated_email'));
      return get(s, 'status') !== 'verified' || !NATIVE_VIA.includes(get(s, 'via')) || get(s, 'person_key') !== personKey
        || get(s, 'run_id') !== runId || get(s, 'value') !== (valid ? 'valid' : 'invalid')
        || (validated == null ? null : normalizeEmail(validated)) !== email;
    });
    if (!bad) bound.push([personKey, valid]);
  }
  if (bound.length === addressCount) {
    const best = bound.find(([, valid]) => valid) ?? bound[0];
    lead.field_status.person_email_validation = clone(lead.person_field_status[best[0]].person_email_validation);
  }
  return matches.length;
}

function requestedNeedsReview(statuses, requested) {
  return Object.entries(isObject(statuses) ? statuses : {})
    .some(([field, status]) => requested.includes(field) && ['no_match', 'action_required'].includes(get(status, 'status')));
}

/** ref: gap_closure.rs:1567 */
export function completeAfterNativeEmailValidation(lead) {
  const payload = get(lead, 'payload');
  if (get(lead, 'research_status') !== 'needs_review' || get(payload, 'native_research_terminal_status') !== 'needs_review'
    || asU64(get(payload, 'native_research_rejections_count')) !== 0) return false;
  const requested = get(payload, 'native_research_requested_fields');
  if (!Array.isArray(requested) || requested.length === 0 || requested.some((f) => typeof f !== 'string')) return false;
  if (requested.includes('person_email_validation') && emailsNeedingValidation(lead, 1).length > 0) return false;
  const unanswered = requested.some((field) => {
    const open = openPersonsForField(lead, field);
    if (open != null) return open.length > 0;
    const status = get(get(lead, 'field_status'), field);
    return !(status !== undefined && researchFieldIsAnswered(status));
  });
  const needsReview = unanswered
    || Object.values(isObject(get(lead, 'person_field_status')) ? lead.person_field_status : {}).some((fields) => requestedNeedsReview(fields, requested))
    || requestedNeedsReview(get(lead, 'field_status'), requested);
  if (needsReview) return false;
  lead.research_status = 'completed';
  lead.payload.native_research_terminal_status = 'completed';
  return true;
}

/** ref: gap_closure.rs:1552 — review view from the witnesses of the stored lead. */
export async function completeAfterNativeEmailValidationWithNativeReviews(ctx, recordId, lead) {
  const witnesses = await readCurrentWitnesses(ctx, LEAD_COLLECTION, recordId);
  const view = nativeReviewViewFromWitnesses(lead, witnesses);
  if (!completeAfterNativeEmailValidation(view)) return false;
  lead.research_status = view.research_status;
  ensureObject(lead, 'payload').native_research_terminal_status = view.payload.native_research_terminal_status;
  return true;
}

/** ref: :1318 */
export function leadDueForSweep(lead, nowMs) {
  const status = strOf(get(lead, 'research_status')) ?? '';
  if (status === 'queued' || status === 'running') return false;
  if (emailsNeedingValidation(lead, 1).length === 0) return false;
  const at = get(get(get(lead, 'payload'), 'email_validation_pass'), 'at_ms');
  return !(Number.isInteger(at)) || nowMs - at >= RETRY_AFTER_MS;
}

/** Records of a validation run: result_json artifact of the run document (Rust: <run_dir>/outputs/result.json). */
async function runRecords(ctx, runId) {
  const run = await ctx.get(RUNS, runId);
  const artifact = (Array.isArray(get(run, 'artifacts')) ? run.artifacts : []).find((a) => get(a, 'artifact_kind') === 'result_json');
  const text = artifact ? await readFileText(ctx, artifact.path) : null;
  if (text == null) fail('unavailable', `validation run ${runId} has no readable result`);
  const result = JSON.parse(text);
  return Array.isArray(get(result, 'records')) ? result.records : [];
}

/**
 * ref: :1141/:1160 run_validation(_once) — one run of the validation target for one address.
 * ABWEICHUNG: kein Wiederholen bei „already has an active run“ (der Läufer der App kennt keine Laufsperre).
 */
export async function runValidation(ctx, email) {
  const outcome = await executeScrape(ctx, {
    target_key: VALIDATION_TARGET_KEY, trigger_kind: 'manual', timeout_seconds: 180, input: { email },
  });
  if (typeof get(outcome, 'run_manifest_path') !== 'string' || outcome.run_manifest_path === '') {
    fail('unavailable', `validation run for ${email} reported no run manifest`);
  }
  return verdictFromRecords(await runRecords(ctx, outcome.run_id), email, outcome.run_id);
}

/**
 * ref: store.rs:12742 upsert_native_email_validation_record — CAS against the lead read before the patch,
 * witnesses with Issuance::Preserve (a status stays witnessed only if unchanged), one transaction.
 */
async function persistValidation(ctx, recordId, lead, expectedMaster) {
  await ctx.transaction(async (tx) => {
    const current = await tx.get(LEAD_COLLECTION, recordId);
    if (current == null) fail('not_found', 'native research persistence cannot create or resurrect a missing/deleted lead');
    if (isObject(current) && !has(current, 'id')) current.id = recordId;
    if (!jsonEqual(current, expectedMaster)) fail('conflict', 'native lead changed before persistence; retry from the current record');
    const previous = await loadWitnesses(tx, recordId);
    await tx.put(LEAD_COLLECTION, lead);
    await persistWritebackWitnesses(tx, recordId, lead, previous, null, [], ctx.now());
  });
}

/**
 * ref: :1200 validate_lead_contacts — returns the number of changed contacts.
 * `run` = (ctx, email) → verdict | null (Tests ersetzen den Prüflauf).
 * ABWEICHUNG: kein Warten bis 900 s auf den Recherche-Fence (kein Schlafen im Handler); ist der Lead belegt,
 * scheitert der Durchlauf mit dem Rust-Fehlertext und der nächste Sweep wiederholt ihn (ohne Pass-Vermerk fällig).
 */
export async function validateLeadContacts(ctx, recordId, run = runValidation) {
  const initial = await ctx.get(LEAD_COLLECTION, recordId);
  if (initial == null) return 0;
  const emails = emailsNeedingValidation(initial, MAX_ADDRESSES_PER_PASS);
  if (emails.length === 0 && !legacyValidationPlaceholder(initial)) return 0;
  const verdicts = [];
  const failures = [];
  for (const email of emails) {
    try {
      const verdict = await run(ctx, email);
      if (verdict != null) verdicts.push(verdict);
      else failures.push({ email, error: 'kein Urteil im Lauf' });
    } catch (error) {
      failures.push({ email, error: String(error?.message ?? error) });
    }
  }
  // The slow runs happen outside the per-record guard; the patch never interleaves with a research writeback.
  const fence = await ctx.cap.fence.claim({ key: `outbound-research:${recordId}` });
  if (fence?.token == null) fail('conflict', `lead ${recordId} stayed busy; ${verdicts.length} verdicts not applied`);
  try {
    const lead = await ctx.get(LEAD_COLLECTION, recordId);
    if (lead == null) return 0;
    const expectedMaster = clone(lead);
    const repaired = emails.length === 0 ? reconcileReceiptedEmailStatus(lead) : 0;
    const changed = repaired + applyEmailVerdicts(lead, verdicts);
    const aligned = alignLeadEmailValidation(lead);
    if (emails.length === 0 && changed === 0 && !aligned) return 0;
    if (changed > 0) await completeAfterNativeEmailValidationWithNativeReviews(ctx, recordId, lead);
    const now = ctx.now();
    // Every pass leaves its account on the lead itself.
    const payload = ensureObject(lead, 'payload');
    payload.email_validation_receipts = mergedEmailReceipts(lead, verdicts);
    if (emails.length > 0) {
      payload.email_validation_pass = {
        at_ms: now,
        checked: verdicts.map((v) => ({ email: v.email, valid: isValid(v), verdict: v.kind, run_id: v.run_id })),
        failed: failures,
      };
    }
    lead.research_updated_at_ms = now;
    await persistValidation(ctx, recordId, lead, expectedMaster);
    return changed;
  } finally {
    await ctx.cap.fence.release({ token: fence.token });
  }
}

/**
 * ref: :1290 spawn_contact_email_validation — startet die Prüfung als eigenen App-Befehl (der Kern führt ihn
 * nach dem aufrufenden Befehl aus); ein Startfehler wird nur protokolliert.
 */
export async function spawnContactEmailValidation(ctx, recordId) {
  try {
    await ctx.command(VALIDATE_COMMAND, { record_id: recordId }, { record_id: recordId, module: 'outbound-lead-generation' });
  } catch (error) {
    ctx.log('warn', 'contact email validation could not start', { record_id: recordId, error: String(error?.message || error) });
  }
}
