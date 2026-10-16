// ref: store_outbound_commands.rs:1174 → outbound_handle_research_source_adapter(.., "adapter_requested")
//      + generate path of the scrape effect (:3101), see _lib/scrape-generation.js — GEMISCHT.
import { handleResearchSourceAdapter } from './_lib/research-adapter.js';
import { generateAdapterScrapeEffect } from './_lib/scrape-generation.js';

export default async function handle(command, ctx) {
  return handleResearchSourceAdapter(command, ctx, 'adapter_requested', generateAdapterScrapeEffect);
}
