// outbound.sellify.lookup — FACHLOGIK über die replizierten Sellify-Collections
// ref: store_outbound_commands.rs:1183 → :1194 outbound_handle_sellify_lookup → :1673 outbound_sellify_lookup
// Also replaces the native WebRTC RPC `ctox.outbound.sellify_lookup.v1`
// (rxdb_peer.rs:708, :3113-3135), which called the same function after a
// DataRead check on outbound_lead_generation_leads — see V4-PORT.md.
// Read-only: creates no agent/queue tasks and writes no lead data.
import { sellifyLookup } from './_lib/sellify-lookup.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? null;
  return sellifyLookup(ctx, payload && typeof payload === 'object' ? payload : {});
}
