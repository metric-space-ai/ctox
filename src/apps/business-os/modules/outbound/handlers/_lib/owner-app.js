// Weitergabe an die Eigentümer-App outbound-lead-generation (Strang OUT3, 15.10.2026).
//
// Die Fachlogik der Scrape-Quellen (research_source.test / .reprobe) und des Update-Digests (update_digest.send_now)
// liegt in der App outbound-lead-generation (Strang OLGSCR/CAPSIG2: Scrape-Registry, process.run-Läufer, Digest-
// Bericht über mail.send). outbound führt sie nicht ein zweites Mal aus und ruft keine Core-Fähigkeit mehr dafür auf
// (vorher Legacy-Durchgriff legacy.<type>): der Befehl geht unverändert per ctx.command an die Eigentümer-App, Routing nach
// (app_id, type) (§16.25). ctx.command liefert nur die Annahme (§16.21); das Ergebnis steht am Befehl der OLG-App.
import { fail, isObject } from './outbound.js';

export const OWNER_APP = 'outbound-lead-generation';

/** Folge-Id: deterministisch je Ausgangsbefehl, damit eine Wiederholung keinen zweiten OLG-Befehl erzeugt. */
export const ownerCommandId = (command) => `${command.id}:${OWNER_APP}`;

export async function forwardToOwnerApp(command, ctx) {
  if (typeof ctx.command !== 'function') fail('unavailable', 'ctx.command is not available');
  if (typeof command?.id !== 'string' || command.id.trim() === '') fail('invalid', 'command id is required');
  const options = { id: ownerCommandId(command), module: OWNER_APP };
  const recordId = command.record_id ?? (isObject(command.payload) ? command.payload.record_id : undefined);
  if (typeof recordId === 'string' && recordId !== '') options.record_id = recordId;
  if (isObject(command.client_context)) options.client_context = command.client_context;
  const sent = (await ctx.command(command.type, isObject(command.payload) ? command.payload : {}, options)) || {};
  const status = typeof sent.status === 'string' ? sent.status : 'accepted';
  if (status === 'rejected' || status === 'failed') {
    fail('unavailable', `${OWNER_APP} rejected ${command.type}: ${sent.reason ?? 'no reason'}`);
  }
  const out = {
    ok: true,
    delegated_to: OWNER_APP,
    command_type: command.type,
    command_id: typeof sent.id === 'string' && sent.id !== '' ? sent.id : options.id,
    status,
  };
  if (typeof sent.task_id === 'string' && sent.task_id !== '') out.task_id = sent.task_id;
  return out;
}
