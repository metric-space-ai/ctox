// outbound.sellify.lookup — FACHLOGIK über die replizierten Sellify-Collections (Strang OUT3, ersetzt
// legacy.outbound.sellify.lookup). ref: store_outbound_commands.rs:1183 → :1194 outbound_handle_sellify_lookup
// → :1673 outbound_sellify_lookup (Port in _lib/sellify-lookup.js).
// Die Collections sellify_companies / sellify_people / sellify_campaigns gehören outbound-lead-generation; outbound liest
// sie als foreign_collections (ops read). Read-only: keine Agenten-/Queue-Aufgaben, keine Lead-Schreibzugriffe.
import { sellifyLookup } from './_lib/sellify-lookup.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? null;
  return sellifyLookup(ctx, payload && typeof payload === 'object' ? payload : {});
}
