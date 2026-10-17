// ref: store_outbound_commands.rs:1171 → outbound_handle_research_source_adapter(.., "active")
import { handleResearchSourceAdapter } from './_lib/research-adapter.js';

export default async function handle(command, ctx) {
  return handleResearchSourceAdapter(command, ctx, 'active');
}
