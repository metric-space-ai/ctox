// outbound.research_source.test — GEMISCHT: Fachlogik hier, native Wirkung über ctx.cap.*
// ref: store_outbound_commands.rs:1177 -> outbound_handle_research_source_adapter :2776 (next_status "test_requested")
// Port und Capability-Liste: _lib/research-source-adapter.js, V4-PORT.md §12/§14/§15.
import { handleResearchSourceAdapter } from './_lib/research-source-adapter.js';

export default async function handle(command, ctx) {
  return handleResearchSourceAdapter(command, ctx, 'test_requested');
}
