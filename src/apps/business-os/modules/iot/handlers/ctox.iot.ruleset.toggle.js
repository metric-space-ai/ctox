// Origin: CTOX
// License: Apache-2.0
// ctox.iot.ruleset.toggle – ref: src/core/iot/commands.rs:2420-2430 (Pflichtfelder), :1145-1178 ruleset_toggle
import { fail } from './_base.js';
import { C, outcome, requireField, rulesetRow, sessionRealm, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const id = requireField(command, 'ruleset_id', 'string');
  const enabled = requireField(command, 'enabled', 'bool');
  const existing = await ctx.get(C.rulesets, id);
  if (!existing || existing.realm !== realm) fail('not_found', `ruleset not found: ${id}`); // changed == 0
  const projection = await writeRow(ctx, C.rulesets, id, rulesetRow({ ...existing, enabled }), existing);
  return outcome({ ruleset: { id, enabled } }, [projection]);
}
