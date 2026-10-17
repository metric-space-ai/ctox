// Origin: CTOX
// License: AGPL-3.0-only
// workjet.worker.dispatch – E54.3b: register_source, revoke_source, poll, complete, report_outcome, dispatch, observe.
// Port von src/core/business_os/mcp_workjet_worker_dispatch.rs; Logik in ./_lib/dispatch.js. capabilities_required [].
import { handleDispatch } from './_lib/dispatch.js';

export default async function handle(command, ctx) {
  return handleDispatch(command, ctx);
}
