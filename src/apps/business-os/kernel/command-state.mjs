// K-1c Punkt 10: KEEP-CRITICAL-Invarianten des Befehls-Lebenszyklus (shared/command-lifecycle.js, Vertrag
// src/core/rxdb/tests/fixtures/business-command-lifecycle-v2.json) auf dem V4-Befehlsdokument (§9, CommandState).
// Die Fixture bleibt die Quelle: COMMAND_TRANSITIONS ist ihre Projektion auf die V4-Zustände (tests/command-state.test.mjs
// rechnet die Projektion aus der Fixture nach und schlägt bei jeder Abweichung fehl).
//
// Projektion v2-Ausführungsphase → V4-status:
//   waiting_dependencies, queued, leased, retry_wait, blocked → queued · accepted → accepted
//   running, awaiting_review, validating → running · terminal + completed|failed|cancelled → done|failed|cancelled
// V4-eigen: rejected = der Core hat den Befehls-Write abgelehnt (write_result), nur aus queued.

export const COMMAND_STATES = Object.freeze(['queued', 'accepted', 'running', 'done', 'failed', 'cancelled', 'rejected']);
export const COMMAND_TERMINAL = Object.freeze(['done', 'failed', 'cancelled', 'rejected']);
const TERMINAL = new Set(COMMAND_TERMINAL);
export const COMMAND_TRANSITIONS = Object.freeze({
  queued: Object.freeze(['queued', 'accepted', 'running', 'done', 'failed', 'cancelled', 'rejected']),
  accepted: Object.freeze(['accepted', 'queued', 'running', 'done', 'failed', 'cancelled']),
  running: Object.freeze(['running', 'queued', 'done', 'failed', 'cancelled']),
  done: Object.freeze(['done']),
  failed: Object.freeze(['failed']),
  cancelled: Object.freeze(['cancelled']),
  rejected: Object.freeze(['rejected']),
});
/** v2 immutableIntentFields → Felder des V4-Befehlsdokuments (command_id/idempotency_key = id, command_type = type, module = app_id). */
export const COMMAND_IMMUTABLE_FIELDS = Object.freeze(['id', 'type', 'app_id', 'payload', 'record_id', 'client_context', 'created_at']);
export const V2_TO_V4_PHASE = Object.freeze({
  waiting_dependencies: 'queued', accepted: 'accepted', queued: 'queued', leased: 'queued', running: 'running',
  awaiting_review: 'running', validating: 'running', retry_wait: 'queued', blocked: 'queued',
});
export const V2_TERMINAL_TO_V4 = Object.freeze({ completed: 'done', failed: 'failed', cancelled: 'cancelled' });

function canon(v) {
  if (Array.isArray(v)) return `[${v.map(canon).join(',')}]`;
  if (v && typeof v === 'object') return `{${Object.keys(v).sort().filter((k) => v[k] !== undefined).map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(',')}}`;
  return JSON.stringify(v ?? null);
}
const same = (a, b) => canon(a) === canon(b);
export const isTerminalCommand = (doc) => TERMINAL.has(doc?.status);

/**
 * Prüft den Übergang prev → next eines Befehlsdokuments. null = zulässig, sonst {code, message} mit
 * code 'invalid_transition' (unbekannter Zustand, Rückschritt, Terminal geändert) oder 'idempotency_conflict'
 * (Absicht geändert). Gleiches Dokument erneut = zulässig (idempotente Zustellung).
 */
export function checkCommandTransition(prev, next) {
  if (!next || typeof next !== 'object') return { code: 'invalid_transition', message: 'Befehlsdokument fehlt' };
  if (!COMMAND_TRANSITIONS[next.status]) return { code: 'invalid_transition', message: `unbekannter Befehlszustand ${next.status}` };
  if (!prev) return null;
  for (const f of COMMAND_IMMUTABLE_FIELDS) {
    if (f in prev && !same(prev[f], next[f])) return { code: 'idempotency_conflict', message: `${f} ist unveränderlich` };
  }
  if (!COMMAND_TRANSITIONS[prev.status]) return null; // Altbestand ohne gültigen Zustand: neuer Zustand gilt
  if (!COMMAND_TRANSITIONS[prev.status].includes(next.status)) return { code: 'invalid_transition', message: `Übergang ${prev.status} → ${next.status} nicht erlaubt` };
  if (TERMINAL.has(prev.status) && !same({ result: prev.result, error: prev.error }, { result: next.result, error: next.error })) {
    return { code: 'invalid_transition', message: 'Ergebnis eines beendeten Befehls ist unveränderlich' };
  }
  return null;
}
