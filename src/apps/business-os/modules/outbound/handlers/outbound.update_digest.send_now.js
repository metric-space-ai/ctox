// outbound.update_digest.send_now — Fachlogik der Eigentümer-App outbound-lead-generation (Strang OUT3, ersetzt legacy.outbound.update_digest.send_now).
// ref: store_outbound_commands.rs:1184 → outbound_update_digest.rs (send_now); Port: OLG handlers/outbound.update_digest.send_now.js (_lib/update-digest.js, Versand mail.send).
// outbound gibt den Befehl unverändert per ctx.command an die Eigentümer-App weiter (_lib/owner-app.js) und ruft
// keine Core-Fähigkeit dafür auf.
import { forwardToOwnerApp } from './_lib/owner-app.js';

export default async function handle(command, ctx) {
  return forwardToOwnerApp(command, ctx);
}
