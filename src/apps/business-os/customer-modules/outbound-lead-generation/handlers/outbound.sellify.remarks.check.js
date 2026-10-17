// outbound.sellify.remarks.check — Sellify-Vermerke vom Agenten beurteilen lassen (Strang SKL-C, Template prompts/sellify-remarks.md).
import { handleRemarksCheck } from './_lib/sellify-remarks.js';

export default async function handle(command, ctx) {
  return handleRemarksCheck(command, ctx);
}
