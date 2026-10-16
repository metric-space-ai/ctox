// Origin: CTOX
// License: Apache-2.0
// Trigger-Handler (§16.20): every PT1M. Zeitbasierte Regelbedingungen, deren Ergebnis sich ohne neues Signal ändert:
// Ablauf eines Dauerfensters (duration, §2A.16) und timestamp_older_than (Attribut meldet nicht mehr). Der on-Trigger
// ctox.iot.signal.evaluate wertet nur bei Datenpunkten aus; Rust tat das ebenfalls nur ereignisgetrieben
// (conditions.rs:702 evaluate_event). Die Auswertung ist dieselbe (_iotcore.evaluateRuleset), das Ereignis trägt kein
// Attribut: alle Werte kommen aus dem Live-Zustand des Assets, previous_value sieht null (conditions.rs:608-613).
// command = {type:'trigger', trigger_id}.
import { isObject } from './_base.js';
import { C, evaluateRuleset, ruleIsTimeBased, ruleIsWellFormed, ruleUsesOlderThan } from './_iotcore.js';

const TICK_EVENT = Object.freeze({ attribute_name: null, value: null, old_value: null });

export default async function handle(command, ctx) {
  const report = { rulesets: [], otherwise: [], alarms: [], evaluated: 0 };
  const nowMs = ctx.now();
  const realmAssets = new Map();
  const rulesets = await ctx.read(C.rulesets, { selector: { enabled: true } });
  for (const listed of rulesets) {
    const rule = isObject(listed.data) ? listed.data : {};
    if (!ruleIsWellFormed(rule) || !ruleIsTimeBased(rule.when)) continue;
    let assetIds;
    if (ruleUsesOlderThan(rule.when)) {
      // timestamp_older_than: jedes Asset des Realms kann ohne Signal veralten.
      if (!realmAssets.has(listed.realm)) {
        const rows = await ctx.read(C.assets, { selector: { realm: listed.realm } });
        realmAssets.set(listed.realm, rows.map((r) => r.id));
      }
      assetIds = realmAssets.get(listed.realm);
    } else {
      // Nur Dauerfenster: nur Assets mit offenem Fenster (ohne Ereignis startet kein Fenster, wie in Rust).
      const state = isObject(listed.match_state) ? listed.match_state : {};
      assetIds = Object.keys(state).filter((id) => isObject(state[id]?.windows) && Object.keys(state[id].windows).length > 0);
    }
    for (const assetId of assetIds) {
      const assetRow = await ctx.get(C.assets, assetId);
      if (!assetRow || !isObject(assetRow.data) || assetRow.data.realm !== listed.realm) continue;
      // Zeile je Asset neu lesen: evaluateRuleset schreibt match_state, die nächste Auswertung baut darauf auf.
      const ruleset = await ctx.get(C.rulesets, listed.id);
      if (!ruleset || ruleset.enabled !== true) break;
      await evaluateRuleset(ctx, ruleset, assetRow.data, TICK_EVENT, nowMs, report);
      report.evaluated += 1;
    }
  }
  return report;
}
