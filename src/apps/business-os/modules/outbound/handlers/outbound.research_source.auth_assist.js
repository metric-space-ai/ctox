// ref: store_outbound_commands.rs:1180 → outbound_handle_research_source_adapter(.., "auth_requested")
import { handleResearchSourceAdapter } from './_lib/research-adapter.js';

export default async function handle(command, ctx) {
  return handleResearchSourceAdapter(command, ctx, 'auth_requested');
}
