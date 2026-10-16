// outbound.research_source.script_read.done — Rückruf des Kerns nach outbound.scrape.script.read an
// outbound-lead-generation (Strang OLG725, ausgelöst in outbound.research_source.registry_read mit include_script_target).
// Form verbindlich nach §16.48 Nr. 1: ein Befehl an die auslösende App mit dem Typ aus on_complete bzw. on_failed,
// payload {command_id, status:'done'|'failed'|'cancelled', result?, error?}, error nach §16.42 ({code, reason}),
// höchstens einmal je command_id (der Kern garantiert das; dieser Handler schreibt nichts und ist damit wiederholbar).
//   done             → Skriptsicht aus OLG (Schema ctox.outbound_lead_generation.scrape_script.v1) als Ergebnis dieses
//                      Befehls; Text nicht in eine Collection kopiert (keine Dumps).
//   failed/cancelled → typisierter Fehler `unavailable` mit Code und Grund aus error.
//   jeder andere Status (auch das frühere 'completed') → invalid.
import { fail, isObject } from './_lib/outbound.js';
import { SCRIPT_READ_SCHEMA } from './_lib/olg-registry.js';

const FAILED = new Set(['failed', 'cancelled']);

// §16.42: error = {code, reason}; ein String wird toleriert, fehlt error, nennt die Meldung den Status.
const errorText = (payload) => {
  const e = payload.error;
  if (isObject(e)) return [e.code, e.reason ?? e.message].filter((x) => typeof x === 'string' && x).join(': ') || payload.status;
  if (typeof e === 'string' && e) return e;
  return `command ${payload.status}`;
};

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const commandId = typeof payload.command_id === 'string' ? payload.command_id.trim() : '';
  if (commandId === '') fail('invalid', 'outbound.research_source.script_read.done: command_id is required');
  const status = typeof payload.status === 'string' ? payload.status : '';
  if (FAILED.has(status)) fail('unavailable', `outbound-lead-generation could not read the scrape script (${commandId}, ${status}): ${errorText(payload)}`);
  if (status !== 'done') fail('invalid', `outbound.research_source.script_read.done: unknown status ${status || '-'} (expected done|failed|cancelled)`);
  const result = isObject(payload.result) ? payload.result : null;
  if (result?.schema !== SCRIPT_READ_SCHEMA || !isObject(result.script) || typeof result.script.target_key !== 'string') {
    fail('invalid', `outbound.research_source.script_read.done: result of ${commandId} is not ${SCRIPT_READ_SCHEMA}`);
  }
  ctx.log?.('info', 'scrape script read via outbound-lead-generation', { command_id: commandId, target_key: result.script.target_key, available: result.script.available === true });
  return { ok: true, schema: 'ctox.outbound.research_source_script.v1', command_id: commandId, script: result.script };
}
