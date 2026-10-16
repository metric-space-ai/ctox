// outbound.research_source.reprobe — Fachlogik der Eigentümer-App outbound-lead-generation (Strang OUT3, ersetzt legacy.outbound.research_source.reprobe).
// ref: store_outbound_commands.rs:46 → outbound_handle_source_reprobe :4090-4121; Port: OLG handlers/outbound.research_source.reprobe.js (execute.rs:377-440 reprobe_registered_account_keep_script über process.run).
// outbound gibt den Befehl unverändert per ctx.command an die Eigentümer-App weiter (_lib/owner-app.js) und ruft
// keine Core-Fähigkeit dafür auf.
import { forwardToOwnerApp } from './_lib/owner-app.js';

export default async function handle(command, ctx) {
  return forwardToOwnerApp(command, ctx);
}
