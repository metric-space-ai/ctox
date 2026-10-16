// Origin: CTOX
// License: Apache-2.0
// support.reporting.rebuild_rollups – ref: support.rs:1225-1228, 1977-2043
import { createRun, rebuildReportingRollups, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  return withStore(ctx, run, async (store) => {
    const rebuilt = await rebuildReportingRollups(store, run);
    return run.result({ rebuilt });
  });
}
