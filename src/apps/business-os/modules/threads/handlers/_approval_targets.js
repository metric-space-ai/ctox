// Feste Ziele freigegebener Arbeit (Kern §16.40, Strang THR2, 15.10.).
//
// Freigaben sind Fachlogik von threads, kein allgemeines Freigabe-Primitiv. Einziges Ziel einer Freigabe ist die
// Agentenarbeit (business_os.chat.task): threads reiht den Agenten-Task selbst ein (ctx.cap.agent.task.enqueue,
// threads.ctox_approval.approve.js), das ist kein Befehl an eine fremde App. Ergebnisse gehen nur an Rückrufe aus
// APPROVAL_CALLBACKS. Ein anderes Ziel scheitert schon bei threads.ctox_approval.request (invalid), nicht erst bei
// approve (approve prüft für Altbestände trotzdem nach).
//
// Strang LOOP Runde 2 (Kern §16.44): das frühere feste Ziel ctox.coding.turn (an coding-agents, ausgeführt als
// Reviewer) ist entfernt. Ein Coding-Turn braucht den eigenen Entwurf des Auftraggebers und dessen aktuelle Revision
// (sonst draft_not_owned, payload_invalid, stale_draft_rev); Coding-Aufträge kommen nur aus der Code-Ansicht der
// jeweiligen App (v4/source/source-mode.js). Altbestände mit diesem Ziel lehnt approve als invalid ab.
//
// Jeder Rückruf hat genau einen Befehlsaufruf mit Typ- und App-Konstante (dispatchApprovalCallback). Daraus leitet
// v4/tools/manifest/gen-manifest.mjs commands_out ab; die Empfänger-Apps deklarieren commands_in mit from:['threads'].
//
// Strang FIX10: der einzige Rückruf (reports.delegation.complete/.failed) ist entfernt. Seit LOOP Runde 2 stellt
// „Bugs & Features“ keine Freigabeanfragen mehr, die Kette hatte keinen Absender. Die Liste ist leer: jede Anfrage mit
// callback ist invalid, Altbestände mit Rückruf werden abgeschlossen, aber nicht weitergeleitet (Log warn).
import { fail } from './_lib.js';

export const AGENT_TASK_TARGET = 'business_os.chat.task';
/** Entferntes Ziel (LOOP Runde 2): nur für die Ablehnungsbegründung. */
const REMOVED_CODING_TARGET = 'ctox.coding.turn';

/** Zulässige Rückrufe anfragender Apps (Ergebnis der freigegebenen Arbeit); derzeit keiner (FIX10). */
export const APPROVAL_CALLBACKS = Object.freeze([]);

/** Zieltyp einer Freigabe prüfen; alles außer der Agentenarbeit → invalid (vor dem ersten Schreiben). */
export function ensureAllowedApprovalTarget(type) {
  if (type === AGENT_TASK_TARGET) return type;
  if (type === REMOVED_CODING_TARGET) {
    fail('invalid', `target_command_type \`${type}\` is not an approval target of threads: coding turns are started only from the app's code view with the requester's own draft (§16.44)`);
  }
  fail('invalid', `target_command_type \`${type}\` is not an approval target of threads (allowed: ${AGENT_TASK_TARGET}); cross-app commands need a fixed commands_out entry (§16.40)`);
}

/** Eintrag aus APPROVAL_CALLBACKS, der genau zu {app, on_complete, on_failed} passt, oder null. */
export function approvalCallbackEntry(app, onComplete, onFailed) {
  return APPROVAL_CALLBACKS.find((c) => c.app === app && c.on_complete === onComplete && c.on_failed === onFailed) ?? null;
}

/**
 * Ergebnis an den festen Rückruf der anfragenden App senden; nicht gelistet → null (nichts gesendet).
 * Neuer Rückruf = Listeneintrag + je Typ ein `case`, der den Befehl mit Typ- und App-Konstante sendet (module fest,
 * nie aus den Optionen) + Test + commands_in beim Empfänger. Derzeit gibt es keinen (FIX10).
 */
export async function dispatchApprovalCallback(ctx, type, payload, options) {
  return null;
}
