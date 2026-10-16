// Origin: CTOX
// License: Apache-2.0
// Fall-Zustandsautomat der Tickets-App (Strang TICKAPP, Shell v4). Fachlogik liegt hier im Handler, nicht im
// Kern: Fälle, Freigaben, Prüfungen, Rückfragen und Rückschriebe werden über die eigenen ctox_ticket_*-Collections
// geführt; Protokoll über das OS-Primitiv audit, Rückschrieb an Fremdsysteme über agent.task.
// Fachliche Referenz: src/core/mission/tickets/cases.rs, case_state.rs, ticket_local_native.rs und
// src/core/core_state/{mod.rs,guard.rs} @ Klon ctox-shell-v4 (Basis main@4794af03d).
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { callCap, fail, isObject, payloadOf } from './_base.js';

export const COLLECTIONS = Object.freeze({
  items: 'ctox_ticket_items',
  events: 'ctox_ticket_events',
  cases: 'ctox_ticket_cases',
  approvals: 'ctox_ticket_approvals',
  verifications: 'ctox_ticket_verifications',
  writebacks: 'ctox_ticket_writebacks',
  clarifications: 'ctox_ticket_clarification_requests',
});

// ---------------------------------------------------------------------------------------------- Zustände

// ref: mission/tickets/case_state.rs:96-117 (TicketCaseState::parse) und :119-131 (core_state)
const CASE_STATE_TO_CORE = Object.freeze({
  created: 'Created', open: 'Created', queued: 'Created',
  classified: 'Classified',
  planned: 'Planned', ready: 'Planned', executable: 'Planned',
  executing: 'Executing', in_progress: 'Executing', running: 'Executing',
  approval_pending: 'AwaitingReview', awaiting_review: 'AwaitingReview', review: 'AwaitingReview', reviewing: 'AwaitingReview',
  rework_required: 'ReworkRequired', rework: 'ReworkRequired',
  awaiting_verification: 'AwaitingVerification', verification: 'AwaitingVerification',
  verified: 'Verified', writeback_pending: 'Verified',
  closed: 'Closed', done: 'Closed', completed: 'Closed',
  blocked: 'Blocked', blocked_needs_clarification: 'Blocked',
});

// ref: mission/tickets/case_state.rs:96-117 – kanonische Schreibform (as_str) zu jedem Alias
const CASE_STATE_CANONICAL = Object.freeze({
  open: 'open', created: 'created', queued: 'queued', classified: 'classified', planned: 'planned', ready: 'ready',
  executable: 'executable', executing: 'executing', in_progress: 'executing', running: 'executing',
  approval_pending: 'approval_pending', awaiting_review: 'awaiting_review', review: 'awaiting_review',
  reviewing: 'awaiting_review', rework_required: 'rework_required', rework: 'rework_required',
  awaiting_verification: 'awaiting_verification', verification: 'awaiting_verification', verified: 'verified',
  writeback_pending: 'writeback_pending', closed: 'closed', done: 'closed', completed: 'closed', blocked: 'blocked',
  blocked_needs_clarification: 'blocked_needs_clarification',
});

/** ref: case_state.rs:96-117 – unbekannte Werte scheitern geschlossen. */
export function parseCaseState(raw) {
  const normalized = String(raw ?? '').trim().toLowerCase();
  const canonical = CASE_STATE_CANONICAL[normalized];
  if (!canonical) fail('invalid', `ticket case state is not recognized: ${normalized}`);
  return canonical;
}
const coreStateOf = (raw) => CASE_STATE_TO_CORE[parseCaseState(raw)];

// ref: core_state/mod.rs:1172-1198 (allowed_transition_catalog, CoreEntityType::Ticket)
const TICKET_TRANSITIONS = new Set([
  'Created>Classified', 'Created>AwaitingReview', 'Created>Blocked', 'Created>Planned', 'Created>Superseded',
  'Classified>Planned', 'Classified>Superseded',
  'Planned>Blocked', 'Planned>Executing', 'Planned>Superseded',
  'Executing>Verified', 'Executing>AwaitingReview',
  'AwaitingReview>ReworkRequired', 'AwaitingReview>AwaitingVerification', 'AwaitingReview>Planned',
  'AwaitingReview>Blocked', 'AwaitingReview>Superseded',
  'ReworkRequired>Executing', 'ReworkRequired>Superseded',
  'AwaitingVerification>Verified', 'Verified>Closed',
  'Executing>Blocked', 'Executing>Superseded',
  'Blocked>Planned', 'Blocked>Superseded',
]);

// ref: core_state/guard.rs:1193-1245 (agent_recovery_message) – Wortlaut der Rückmeldung an Agenten
const RECOVERY_ACTIONS = Object.freeze({
  invalid_transition: 'Bleib im erlaubten Arbeitsablauf: springe nicht direkt zum Zielzustand. Fuehre die fehlenden Zwischenschritte aus, dokumentiere jeden Schritt dauerhaft und versuche danach erneut fortzufahren.',
  owner_visible_completion_requires_review: 'Schliesse owner- oder founder-sichtbare Arbeit noch nicht ab. Fuehre zuerst ein echtes Review durch, arbeite kritische Review-Punkte inhaltlich nach und speichere die Review-Freigabe dauerhaft.',
  closure_requires_verification: 'Schliesse die Aufgabe noch nicht. Verifiziere das Ergebnis zuerst mit belastbarer Evidenz, speichere diese Evidenz und schliesse erst danach.',
});
const DEFAULT_RECOVERY = 'Die geplante Aktion passt noch nicht zum gesicherten Harness-Zustand. Pruefe den naechsten erlaubten Arbeitsschritt, halte Evidenz fest und versuche erst danach erneut fortzufahren.';

export function recoveryMessage(codes) {
  const actions = [];
  for (const code of codes) {
    const action = RECOVERY_ACTIONS[code] ?? DEFAULT_RECOVERY;
    if (!actions.includes(action)) actions.push(action);
  }
  return `Diese Aktion wurde noch nicht ausgefuehrt, weil der abgesicherte Arbeitsablauf unvollstaendig ist. ${actions.join(' ')}\nWenn du eine kompakte Diagnose brauchst, nutze \`ctox process-mining guidance --limit 50\` und arbeite die dort genannten naechsten Schritte ab.`;
}

/**
 * ref: cases.rs:1532-1563 enforce_ticket_case_state_transition → guard.rs:137-161 (gleicher Kernzustand = noop)
 * → mod.rs:372-403 validate_transition. Für Ticket-Fälle greift außerhalb des Abschlusses nur invalid_transition.
 */
export function enforceStateTransition(caseDoc, toState) {
  const from = coreStateOf(caseDoc.state);
  const to = coreStateOf(toState);
  if (from === to) return;
  if (!TICKET_TRANSITIONS.has(`${from}>${to}`)) {
    fail('conflict', recoveryMessage(['invalid_transition']), { violations: ['invalid_transition'] });
  }
}

/**
 * ref: cases.rs:1457-1495 enforce_ticket_case_close_transition (owner_visible_completion, completion_review_required,
 * verdict pass) → mod.rs validate_review_gate / validate_required_completion_review / validate_ticket_closure.
 * Belege: verificationId = letzte bestandene Prüfung, reviewKey = letzte Freigabe- oder Prüfungsentscheidung.
 */
export function enforceCloseTransition(caseDoc, { verificationId, reviewKey }) {
  const from = coreStateOf(caseDoc.state);
  if (from === 'Closed') return;
  const violations = [];
  if (!TICKET_TRANSITIONS.has(`${from}>Closed`)) violations.push('invalid_transition');
  if (!reviewKey) violations.push('owner_visible_completion_requires_review', 'completion_review_required_requires_audit');
  if (!verificationId) violations.push('completion_review_required_requires_validation', 'closure_requires_verification');
  if (violations.length) fail('conflict', recoveryMessage(violations), { violations });
}

// ref: cases.rs:1832-1863
export function ensureCaseIsExecutable(caseDoc) {
  if (caseDoc.state !== 'executable' && caseDoc.state !== 'executing') {
    fail('conflict', `case ${caseDoc.case_id} is not executable; current state is ${caseDoc.state}`);
  }
}
export function ensureCaseCanRequestClarification(caseDoc) {
  if (['closed', 'done', 'completed', 'verified', 'writeback_pending'].includes(caseDoc.state)) {
    fail('conflict', `case ${caseDoc.case_id} cannot request clarification from terminal/writeback state ${caseDoc.state}`);
  }
}
export function ensureCaseReadyForWriteback(caseDoc) {
  if (caseDoc.state !== 'writeback_pending' && caseDoc.state !== 'verifying') {
    fail('conflict', `case ${caseDoc.case_id} is not ready for writeback; current state is ${caseDoc.state}`);
  }
}

// ref: mission/tickets/mod.rs:5695-5718
export function canonicalApprovalStatus(raw) {
  const value = String(raw).trim();
  if (value !== 'approved' && value !== 'rejected') fail('invalid', `unsupported approval status: ${value}`);
  return value;
}
export function canonicalVerificationStatus(raw) {
  const value = String(raw).trim();
  if (value !== 'passed' && value !== 'failed') fail('invalid', `unsupported verification status: ${value}`);
  return value;
}

// ref: cases.rs:1865-1904
export function canonicalClarificationTargetType(raw) {
  const value = String(raw).trim().toLowerCase();
  if (['requester', 'customer', 'ticket_requester'].includes(value)) return 'requester';
  if (['owner', 'founder', 'admin'].includes(value)) return 'owner';
  if (['internal', 'team', 'operator'].includes(value)) return 'internal';
  return fail('invalid', `unsupported clarification target_type '${value}' (expected requester|owner|internal)`);
}
export function canonicalClarificationTargetChannel(raw) {
  const value = String(raw).trim().toLowerCase();
  if (['ticket', 'email', 'jami', 'tui', 'teams', 'whatsapp'].includes(value)) return value;
  return fail('invalid', `unsupported clarification target_channel '${value}' (expected ticket|email|jami|tui|teams|whatsapp)`);
}
export function canonicalClarificationResumeState(raw) {
  const value = String(raw).trim().toLowerCase();
  if (['planned', 'ready', 'executable'].includes(value)) return 'executable';
  return fail('invalid', `unsupported clarification resume_state '${value}' (expected executable)`);
}
export function normalizeClarificationInputs(values) {
  const seen = new Set();
  const out = [];
  for (const raw of values) {
    const value = String(raw).trim();
    if (value === '' || seen.has(value.toLowerCase())) continue;
    seen.add(value.toLowerCase());
    out.push(value);
  }
  return out;
}

// ref: ticket_gateway.rs:63-95 – beide Systeme können Kommentar/Übergang, öffentlich und intern
export const WRITEBACK_SYSTEMS = Object.freeze(['local', 'zammad']);

// ---------------------------------------------------------------------------------------------- Helfer

/** nur gesetzte Felder übernehmen (Rust: Option::None = Feld fehlt) */
export function compact(object) {
  return Object.fromEntries(Object.entries(object).filter(([, value]) => value !== undefined));
}

/** ref: cases.rs parse_domain_csv – Komma-Liste, getrimmt, leere verworfen */
export function parseCsv(value) {
  return String(value).split(',').map((item) => item.trim()).filter((item) => item !== '');
}

/** ref: tickets/mod.rs:5475-5479 stable_digest – sha256, erste 12 Hex-Zeichen. */
export async function stableDigest(input) {
  const bytes = new TextEncoder().encode(String(input));
  const hash = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(hash)].map((b) => b.toString(16).padStart(2, '0')).join('').slice(0, 12);
}

/** Zeitstempel einer Ausführung: Core-Uhr (ctx.now) als ISO und ms. */
export function clock(ctx) {
  const raw = ctx.now();
  const ms = typeof raw === 'number' ? raw : Date.parse(raw);
  return { ms, iso: new Date(ms).toISOString() };
}

/** ID-Saat: Befehls-ID statt Uhrzeit, damit eine Wiederholung desselben Befehls dieselben Datensätze trifft. */
export const seedOf = (command) => String(command?.id ?? '');

export async function loadCase(db, caseId) {
  const doc = await db.get(COLLECTIONS.cases, caseId);
  if (!doc || doc.is_deleted) fail('not_found', 'ticket case not found');
  return doc;
}

export async function loadTicket(db, ticketKey, message) {
  const doc = await db.get(COLLECTIONS.items, ticketKey);
  if (!doc || doc.is_deleted) fail('not_found', message);
  return doc;
}

export async function loadClarification(db, clarificationId) {
  const doc = await db.get(COLLECTIONS.clarifications, clarificationId);
  if (!doc || doc.is_deleted) fail('not_found', 'ticket clarification not found');
  return doc;
}

/** ref: cases.rs:1565-1602 – Belege für den Abschluss aus den eigenen Collections. */
export async function closureEvidence(db, caseId) {
  const latest = (rows) => rows.sort((a, b) => String(b.created_at).localeCompare(String(a.created_at)))[0];
  const verifications = await db.read(COLLECTIONS.verifications, { selector: { case_id: caseId } });
  const passed = latest(verifications.filter((row) => row.status === 'passed' && !row.is_deleted));
  const approvals = await db.read(COLLECTIONS.approvals, { selector: { case_id: caseId } });
  const review = latest([...approvals, ...verifications].filter((row) => !row.is_deleted)
    .map((row) => ({ key: row.approval_id ?? row.verification_id, created_at: row.created_at })));
  return { verificationId: passed?.verification_id ?? null, reviewKey: review?.key ?? null };
}

export async function setCaseState(db, caseDoc, state, time, extra = {}) {
  await db.patch(COLLECTIONS.cases, caseDoc.id, { state, updated_at: time.iso, updated_at_ms: time.ms, ...extra });
}

export function caseOutcome(caseDoc) {
  return { case: caseDoc, case_id: caseDoc.case_id, ticket_key: caseDoc.ticket_key };
}

// ---------------------------------------------------------------------------------------------- Audit

/**
 * ref: tickets/mod.rs:4326 record_audit – Protokoll über das OS-Primitiv audit (business_events), nach dem Commit
 * (Capabilities dürfen nicht in ctx.transaction laufen, §16.16).
 */
export async function recordAudit(ctx, time, entry) {
  const caseDoc = entry.case ?? null;
  const recordId = entry.case_id ?? caseDoc?.case_id ?? entry.ticket_key;
  return callCap(ctx, 'audit.event', {
    collection: caseDoc || entry.case_id ? COLLECTIONS.cases : COLLECTIONS.items,
    record_id: recordId,
    event_type: `ticket.${entry.action_type}`,
    payload: {
      ticket_key: entry.ticket_key,
      case_id: entry.case_id ?? caseDoc?.case_id ?? null,
      actor_type: entry.actor_type,
      action_type: entry.action_type,
      label: caseDoc?.label ?? null,
      bundle_label: caseDoc?.bundle_label ?? null,
      bundle_version: caseDoc?.bundle_version ?? null,
      details: entry.details,
    },
    observed_at_ms: time.ms,
  });
}

// ---------------------------------------------------------------------------------------------- lokale Tickets

/** ref: tickets/mod.rs:5463-5469 canonical_ticket_key / canonical_event_key */
export const ticketKeyOf = (system, remoteId) => `${String(system).trim()}:${String(remoteId).trim()}`;

/**
 * ref: ticket_local_native.rs:413-443 insert_local_event + Sync-Spiegel (tickets/mod.rs ticket_events): das lokale
 * Ereignis wird direkt als ctox_ticket_events-Dokument geführt (direction outbound nur für eigene Rückschriebe).
 */
export async function insertLocalEvent(db, time, seed, { ticketId, eventType, summary, body, metadata = {} }) {
  const eventId = `LE-${await stableDigest(`${ticketId}${eventType}${seed}`)}`;
  const key = ticketKeyOf('local', eventId);
  await db.put(COLLECTIONS.events, {
    id: key,
    event_key: key,
    ticket_key: ticketKeyOf('local', ticketId),
    source_system: 'local',
    remote_event_id: eventId,
    direction: metadata.origin === 'ctox-writeback' ? 'outbound' : 'inbound',
    event_type: String(eventType).trim(),
    summary: String(summary).trim(),
    body_text: String(body).trim(),
    metadata,
    external_created_at: time.iso,
    observed_at: time.iso,
    updated_at_ms: time.ms,
    is_deleted: false,
  });
  return { event_id: eventId, ticket_id: ticketId, event_type: String(eventType).trim(), summary: String(summary).trim(), body_text: String(body).trim(), created_at: time.iso, metadata };
}

export async function loadLocalTicket(db, ticketId) {
  const doc = await db.get(COLLECTIONS.items, ticketKeyOf('local', ticketId));
  if (!doc || doc.is_deleted) fail('not_found', `local ticket not found: ${ticketId}`);
  return doc;
}

/** ref: ticket_local_native.rs:174-205 transition_local_ticket_with_metadata */
export async function transitionLocalTicket(db, time, seed, ticketId, status, metadata = {}) {
  const ticket = await loadLocalTicket(db, ticketId);
  const next = String(status).trim();
  await db.patch(COLLECTIONS.items, ticket.id, { remote_status: next, updated_at: time.iso, last_synced_at: time.iso, updated_at_ms: time.ms });
  await insertLocalEvent(db, time, `${seed}:status`, {
    ticketId,
    eventType: 'status_changed',
    summary: `status -> ${next}`,
    body: `Ticket moved to status ${next}`,
    metadata: { status: next, ...metadata },
  });
  return localRecord({ ...ticket, remote_status: next, updated_at: time.iso });
}

/** ref: ticket_local_native.rs:256-299 writeback_comment / writeback_transition (lokales System). */
export async function localWriteback(db, time, seed, ticket, request) {
  const ticketId = ticket.remote_ticket_id;
  if (request.operation === 'transition') {
    await transitionLocalTicket(db, time, seed, ticketId, request.state, { origin: 'ctox-writeback' });
    const remoteEventIds = [];
    const note = typeof request.note_body === 'string' ? request.note_body.trim() : '';
    if (note) {
      const event = await insertLocalEvent(db, time, `${seed}:note`, { ticketId, eventType: 'comment', summary: 'comment', body: note, metadata: { origin: 'ctox-writeback' } });
      remoteEventIds.push(event.event_id);
    }
    return { remote_event_ids: remoteEventIds };
  }
  const event = await insertLocalEvent(db, time, `${seed}:comment`, { ticketId, eventType: 'comment', summary: 'comment', body: request.body, metadata: { origin: 'ctox-writeback' } });
  await db.patch(COLLECTIONS.items, ticket.id, { updated_at: time.iso, updated_at_ms: time.ms });
  return { remote_event_ids: [event.event_id] };
}

/** LocalTicketRecord-Form (ticket_local_native.rs:23-34) aus einem ctox_ticket_items-Dokument. */
export function localRecord(item) {
  return {
    ticket_id: item.remote_ticket_id,
    title: item.title,
    body_text: item.body_text,
    status: item.remote_status,
    priority: item.priority ?? null,
    requester: item.requester ?? null,
    created_at: item.created_at,
    updated_at: item.updated_at,
    metadata: item.metadata ?? {},
  };
}

// ---------------------------------------------------------------------------------------------- Fremdsysteme

// Kern §16.30: on_complete/on_failed nennen Befehlstypen der eigenen App; der Core schreibt nach Abschluss genau
// einen business_command {type, payload:{task_id, status, result, error?, workspace?}} (höchstens 1× je task_id).
export const WRITEBACK_COMPLETE_COMMAND = 'ctox.ticket.writeback.complete';
export const WRITEBACK_FAILED_COMMAND = 'ctox.ticket.writeback.failed';

/**
 * Rückschrieb an ein Fremdsystem (heute zammad): statt des Kern-Adapters (ticket_zammad_native.rs) läuft er als
 * gebundene Agentenarbeit über das OS-Primitiv agent.task mit dem App-Skill tickets/zammad-writeback
 * (skills/zammad-writeback/SKILL.md); Ergebnis/Fehler schließen die Befehle ctox.ticket.writeback.complete / .failed
 * ab (Logik und Antwortauswertung in _writeback.js).
 */
export async function enqueueRemoteWriteback(ctx, ticket, writebackId, request) {
  const instruction = await remoteInstruction(ctx, ticket, request);
  const task = await callCap(ctx, 'agent.task.enqueue', {
    title: `Ticket-Rückschrieb ${request.operation} ${ticket.ticket_key}`,
    instruction,
    skill: 'tickets/zammad-writeback',
    thread_key: `ticket/${ticket.source_system}/${ticket.remote_ticket_id}`,
    idempotency_key: `ticket-writeback:${writebackId}`,
    payload: { writeback_id: writebackId, ticket_key: ticket.ticket_key, source_system: ticket.source_system, remote_ticket_id: ticket.remote_ticket_id, ...request },
    on_complete: WRITEBACK_COMPLETE_COMMAND,
    on_failed: WRITEBACK_FAILED_COMMAND,
  });
  return isObject(task) && typeof task.task_id === 'string' ? task.task_id : null;
}

// Prompt-Templates prompts/writeback-*.md (wörtlich aus der früheren Funktion remoteInstruction, nur Platzhalter).
// ref: _tickets.js remoteInstruction @ b16b40a9e – transition mit/ohne Notiz, sonst Kommentar (auch clarification_request)
export async function remoteInstruction(ctx, ticket, request) {
  const head = { source_system: ticket.source_system, remote_ticket_id: ticket.remote_ticket_id };
  let text;
  if (request.operation === 'transition') {
    text = request.note_body
      ? await renderPrompt(ctx, 'writeback-transition-note', { ...head, state: request.state, visibility: request.internal_note ? 'intern' : 'öffentlich', note_body: request.note_body })
      : await renderPrompt(ctx, 'writeback-transition', { ...head, state: request.state });
  } else {
    text = await renderPrompt(ctx, 'writeback-comment', { ...head, visibility: request.internal ? 'einen internen' : 'einen öffentlichen', body: request.body });
  }
  return text.replace(/\n$/, '');
}

export { payloadOf };
