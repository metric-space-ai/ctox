// Origin: CTOX
// License: Apache-2.0
// ctox.coding.cancel – §16.44 (CAP-3d): {turn_id} → ctx.cap.agent.coding.cancel. Kern ungebunden → unavailable als
// Befehlsfehler (Zone), kein Absturz.
import { callCap, payloadOf } from './_base.js';
import { cancelArgs, guarded } from './_coding_turn.js';

export default async function handle(command, ctx) {
  const a = cancelArgs(payloadOf(command));
  return guarded(() => callCap(ctx, 'agent.coding.cancel', { turn_id: a.turn_id }));
}
