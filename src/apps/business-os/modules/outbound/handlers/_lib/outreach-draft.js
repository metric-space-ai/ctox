// Anschreiben-Entwurf als Agentenauftrag (Strang SKL-O, Kern §16.30).
//
// Vorher (F-automatisierung-skills §1 #17): `outbound.pipeline.outreach_draft` war ein reiner Queue-Typ ohne Handler;
// die Anweisung stand im UI (t('outreachDraftInstruction')), den Skill wählte der Core per Typnamen
// (ref: store.rs:27743 suggested_skill_for_command → business-os-outbound-message-drafting) und der Agent las das
// Skillbook per MCP und schrieb per CLI `outbound.pipeline.write_outreach_draft` zurück.
// Jetzt: Handler outbound.pipeline.outreach_draft füllt prompts/outreach-draft.md (Skillbook aus der eigenen
// Collection eingebettet), App-Skill outbound/message-drafting, Rückrufe .done/.failed; .done bildet die Antwort auf
// den vorhandenen Befehl outbound.pipeline.write_outreach_draft ab. Zustand am Kontakt: outreach_task_id,
// outreach_generating, outreach_status, outreach_error.
import { isObject } from './outbound.js';

export const DRAFT_SKILL = 'outbound/message-drafting';
export const DRAFT_DONE = 'outbound.pipeline.outreach_draft.done';
export const DRAFT_FAILED = 'outbound.pipeline.outreach_draft.failed';
export const MESSAGE_KEYS = ['message_mail_subject', 'message_mail_body', 'message_followup_1', 'message_followup_2'];

/** Wartet der Kontakt noch auf genau diese Aufgabe? */
export function contactAwaitsTask(contact, taskId) {
  return isObject(contact) && contact.outreach_task_id === taskId && contact.outreach_generating === true;
}

/** Antwort → {messages} oder {error}. Form {messages:{…}} oder die vier Felder direkt; alle vier nicht leer. */
export function parseDraftReply(reply) {
  if (!isObject(reply)) return { error: 'outreach draft reply contained no JSON object' };
  if (typeof reply.skipped === 'string' && reply.skipped.trim()) return { error: `skipped: ${reply.skipped.trim()}` };
  const raw = isObject(reply.messages) ? reply.messages : reply;
  const messages = {};
  for (const key of MESSAGE_KEYS) {
    if (typeof raw[key] !== 'string' || raw[key].trim() === '') return { error: `outreach draft reply is missing ${key}` };
    messages[key] = raw[key];
  }
  return { messages };
}
