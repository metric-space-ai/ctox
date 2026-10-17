// Origin: CTOX
// License: Apache-2.0
// support.sla.recalculate – ref: support.rs:1218-1224, 2698-2760
import { createRun, firstString, recalculateSlas, recordIdOf, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  // Rust: payload.conversation_id, sonst record_id (auch ""); ohne beides: alle angewendeten SLAs.
  const conversationId = firstString(run.payload, ['conversation_id']) ?? recordIdOf(command);
  return withStore(ctx, run, async (store) => {
    const recalculated = await recalculateSlas(store, run, conversationId);
    return run.result({ conversation_id: conversationId ?? '', recalculated });
  });
}
