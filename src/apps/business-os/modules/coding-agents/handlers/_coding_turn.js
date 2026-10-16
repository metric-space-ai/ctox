// Origin: CTOX
// License: Apache-2.0
// §16.44-Form der Coding-Turn-Capability (CAP-3d, PR #844 Commit ebddaa2e8, befund-cap-3.md „CAP-3d“ Punkt 4):
//   agent.coding.turn {app_id, draft_id, base_draft_rev, prompt, model?, turn_id?}
//   agent.coding.cancel {turn_id}
// Die V3-Form (module_id/faux/preset_id), Mischformen und vorgefertigte Hüllen lehnt der Kern mit invalid ab; die
// Handler prüfen dieselben Regeln vorab (Fehler ohne Capability-Aufruf). Solange #848 nicht gebunden ist, antwortet
// der Kern unavailable: der Handler meldet das als sauberen Befehlsfehler (code unavailable → Zustandszone der
// auslösenden App bzw. on_failed-Rückruf), nie als Absturz und nie über den alten V3-Pfad.
import { fail, isObject } from './_base.js';

export const TURN_FIELDS = Object.freeze(['app_id', 'draft_id', 'base_draft_rev', 'prompt', 'model', 'turn_id']);
export const TURN_REQUIRED = Object.freeze(['app_id', 'draft_id', 'base_draft_rev', 'prompt']);
const V3_FIELDS = new Set(['module_id', 'faux', 'preset_id']);
const MAX_PROMPT_BYTES = 64 * 1024; // draft_turn.rs MAX_PROMPT_BYTES
const ID_RE = /^[A-Za-z0-9_.:-]{1,128}$/; // turn_id wie im Kern-Stub (§16.44)

export const UNAVAILABLE_MESSAGE = 'Coding Agent im Kern noch nicht verfügbar (§16.44 nicht gebunden) – Auftrag nicht ausgeführt.';

/** Payload in die §16.44-Form prüfen; liefert die flachen Argumente für agent.coding.turn. */
export function turnArgs(payload) {
  if (!isObject(payload)) fail('invalid', 'payload_invalid');
  for (const key of Object.keys(payload)) {
    if (V3_FIELDS.has(key)) fail('invalid', `payload_invalid:${key} (V3-Form; §16.44 verlangt app_id, draft_id, base_draft_rev, prompt)`, { path: key });
    if (!TURN_FIELDS.includes(key)) fail('invalid', `payload_invalid:${key}`, { path: key });
  }
  for (const key of TURN_REQUIRED) {
    if (typeof payload[key] !== 'string' || payload[key].trim() === '') fail('invalid', `payload_invalid:${key}`, { path: key });
  }
  if (new TextEncoder().encode(payload.prompt).length > MAX_PROMPT_BYTES) fail('invalid', 'payload_invalid:prompt', { path: 'prompt' });
  for (const key of ['model', 'turn_id']) {
    const v = payload[key];
    if (v === undefined || v === null) continue;
    if (typeof v !== 'string' || v.trim() === '') fail('invalid', `payload_invalid:${key}`, { path: key });
  }
  if (payload.turn_id != null && !ID_RE.test(payload.turn_id)) fail('invalid', 'payload_invalid:turn_id', { path: 'turn_id' });
  const args = {};
  for (const key of TURN_FIELDS) if (payload[key] !== undefined && payload[key] !== null) args[key] = payload[key];
  return args;
}

export function cancelArgs(payload) {
  if (!isObject(payload)) fail('invalid', 'payload_invalid');
  for (const key of Object.keys(payload)) if (key !== 'turn_id') fail('invalid', `payload_invalid:${key}`, { path: key });
  if (typeof payload.turn_id !== 'string' || !ID_RE.test(payload.turn_id)) fail('invalid', 'payload_invalid:turn_id', { path: 'turn_id' });
  return { turn_id: payload.turn_id };
}

/** Capability-Aufruf absichern; unavailable (fehlende Capability oder ungebundener Kern) → sauberer Befehlsfehler.
 *  Der Aufruf selbst steht im Handler als callCap(ctx, '<pfad>', {…}), damit check-caps --calls ihn prüft. */
export async function guarded(call) {
  try {
    return await call();
  } catch (error) {
    const code = error?.code ?? (typeof error === 'string' ? undefined : error?.status);
    if (code === 'unavailable') fail('unavailable', UNAVAILABLE_MESSAGE, { retryable: true, cause_message: String(error?.message ?? error ?? '') });
    if (error instanceof Error) throw error;
    // Nicht-Error-Würfe des Kerns ({code, message}) in die Fehlerform §16.10 bringen.
    fail(typeof code === 'string' ? code : 'unavailable', String(error?.message ?? error ?? 'coding capability failed'));
  }
}
