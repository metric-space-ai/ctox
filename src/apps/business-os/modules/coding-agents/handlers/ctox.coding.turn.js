// Origin: CTOX
// License: Apache-2.0
// ctox.coding.turn – §16.44-Form (CAP-3d): {app_id, draft_id, base_draft_rev, prompt, model?, turn_id?} →
// ctx.cap.agent.coding.turn. Ziel freigegebener Delegationen aus threads (commands_in). Der Turn arbeitet nur auf dem
// Entwurf des auslösenden Nutzers (Kern prüft Eigentum, apps.source.edit, Revision). Die V3-Form (module_id/faux/
// preset_id) gibt invalid. Kern ungebunden → unavailable als Befehlsfehler (Zone bzw. on_failed), kein Absturz.
import { callCap, payloadOf } from './_base.js';
import { guarded, turnArgs } from './_coding_turn.js';

export default async function handle(command, ctx) {
  const a = turnArgs(payloadOf(command));
  return guarded(() => callCap(ctx, 'agent.coding.turn', {
    app_id: a.app_id, draft_id: a.draft_id, base_draft_rev: a.base_draft_rev, prompt: a.prompt, model: a.model, turn_id: a.turn_id,
  }));
}
