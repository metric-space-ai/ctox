// Origin: CTOX
// License: Apache-2.0
// ctox.iot.attribute.write – ref: src/core/iot/commands.rs:797-899 attribute_write, store.rs:467-585 process_attribute_event
// (§2A.1-8: Typauflösung, Koerzierung, Zeitnormierung, Outdated-Regel). Aktueller Zustand in iot_assets/iot_attributes;
// die Historie (Datenpunkt) geht in die Zeitreihe (_timeseries.js, Kernprimitiv knowledge.query `data append`). Webhook-Telemetrie aus net.ingest
// (ctox.iot.webhook.ingest) läuft über attributeWrite unten. Regeln und Widget-Wächter wertet der on-Trigger ctox.iot.signal.evaluate aus.
import { fail } from './_base.js';
import {
  C, assetRow, attributeRecordId, attributeRow, coerceValue, inferBaseType, loadAssetInRealm,
  normalizeEventTimestamp, outcome, parsePayload, sessionRealm, writeRow,
} from './_iotcore.js';
import { recordDatapoint } from './_timeseries.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  if (command?.payload && !('value' in command.payload)) fail('invalid', 'failed to deserialize iot command payload');
  const req = parsePayload(command, { asset_id: 'string', name: 'string', value: 'any', timestamp_ms: 'i64?' });
  return attributeWrite(ctx, req, realm);
}

/** Fachkern von attribute_write; auch der Webhook-Eingang (ctox.iot.webhook.ingest, ref: webhook.rs:68-79) ruft ihn. */
export async function attributeWrite(ctx, req, realm) {
  const asset = structuredClone(await loadAssetInRealm(ctx, req.asset_id, realm));
  const stored = asset.attributes?.[req.name] ?? null;
  let descriptor = null;
  if (!stored?.value_type) {
    const type = await ctx.get(C.assetTypes, asset.asset_type);
    descriptor = type?.data?.attributes?.find((d) => d.name === req.name) ?? null;
  }
  const declared = stored?.value_type ?? descriptor?.value_descriptor?.base_type ?? null;
  const value = declared ? coerceValue(req.value, declared) : req.value;
  const now = ctx.now();
  const ts = normalizeEventTimestamp(req.timestamp_ms ?? 0, now);
  // Datenpunkt wird immer aufgezeichnet, auch wenn veraltet (§2A.3).
  await recordDatapoint(ctx, { asset_id: req.asset_id, attribute_name: req.name, timestamp_ms: ts, value });
  const oldTs = stored?.timestamp ?? 0;
  if (oldTs - ts > 0) return outcome({ outcome: 'OutdatedRecordedOnly', attribute: stored }, []);
  const attr = {
    name: req.name, value_type: declared ?? stored?.value_type ?? inferBaseType(value), value, timestamp: ts,
    meta: stored ? stored.meta ?? {} : { ...(descriptor?.meta ?? {}) },
  };
  asset.attributes = { ...(asset.attributes ?? {}), [req.name]: attr };
  const projections = [
    await writeRow(ctx, C.attributes, attributeRecordId(asset.id, req.name), attributeRow(asset, req.name, { prev_value: stored?.value ?? null })),
    await writeRow(ctx, C.assets, asset.id, assetRow(asset)),
  ];
  return outcome({ outcome: 'Updated', attribute: attr }, projections);
}
