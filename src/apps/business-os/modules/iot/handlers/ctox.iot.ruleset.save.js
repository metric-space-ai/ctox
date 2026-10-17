// Origin: CTOX
// License: Apache-2.0
// ctox.iot.ruleset.save – ref: src/core/iot/commands.rs:1089-1143 ruleset_save, :1207-1240 project_ruleset.
// Die Auswertung läuft als on-Trigger ctox.iot.signal.evaluate (iot_attributes), nicht im Core.
import { C, dataObject, ensureRealm, generateId, outcome, parsePayload, rulesetRow, sessionRealm, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const req = parsePayload(command, { id: 'string?', realm: 'string', name: 'string', enabled: 'bool=true', data: 'any' });
  const data = dataObject(req.data);
  const id = req.id ?? generateId(ctx);
  const existing = await ctx.get(C.rulesets, id);
  if (req.id) ensureRealm(existing, realm, `ruleset not found: ${id}`);
  // Upsert überschreibt realm/name/enabled/data; Auswertungszustand bleibt (last_fired_ms, match_state).
  const row = rulesetRow({ realm, name: req.name, enabled: req.enabled, data, last_fired_ms: existing?.last_fired_ms, match_state: existing?.match_state });
  const projection = await writeRow(ctx, C.rulesets, id, row, existing);
  return outcome({ ruleset: { id, realm, name: req.name, enabled: req.enabled } }, [projection]);
}
