// outbound.research_source.generation.done — Rückruf (on_complete und on_failed) der Adapter-Erzeugung (Strang SKL-C).
import { handleGenerationDone } from './_lib/research-source-adapter.js';

export default async function handle(command, ctx) {
  return handleGenerationDone(command, ctx);
}
