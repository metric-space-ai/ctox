// outbound.research_source.test — Fachlogik der Eigentümer-App outbound-lead-generation (Strang OUT3, ersetzt legacy.outbound.research_source.test).
// ref: store_outbound_commands.rs:1177 → :2776 + Testzweig :3330-3758; Port: OLG handlers/outbound.research_source.test.js (_lib/research-source-adapter.js, Läufer _lib/scrape-runner.js über process.run).
// outbound gibt den Befehl unverändert per ctx.command an die Eigentümer-App weiter (_lib/owner-app.js) und ruft
// keine Core-Fähigkeit dafür auf.
import { forwardToOwnerApp } from './_lib/owner-app.js';

export default async function handle(command, ctx) {
  return forwardToOwnerApp(command, ctx);
}
