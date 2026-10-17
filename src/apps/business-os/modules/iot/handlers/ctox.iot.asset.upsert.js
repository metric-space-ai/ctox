// Origin: CTOX
// License: Apache-2.0
// ctox.iot.asset.upsert – Fachlogik in der App. ref: src/core/iot/commands.rs:604-726 asset_upsert
import { fail } from './_base.js';
import {
  C, MAX_ATTRIBUTES_PER_ASSET, assetRow, assetTypeRow, attributeNames, attributeRecordId, attributeRow, ensureRealm,
  generateId, newAssetWithType, outcome, parseAssetTypeInfo, parsePayload, sessionRealm, writeRow,
} from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const req = parsePayload(command, { id: 'string?', realm: 'string', asset_type: 'string', name: 'string', parent_id: 'string?', asset_type_info: 'any' });
  const info = parseAssetTypeInfo(req.asset_type_info);
  // Realm nie aus dem Payload (commands.rs:610-624); fremder Realm beim Update = „asset not found“.
  const id = req.id ?? generateId(ctx);
  const existingRow = await ctx.get(C.assets, id);
  if (req.id) ensureRealm(existingRow, realm, `asset not found: ${id}`);
  const projections = [];
  // Typ-Deskriptor zuerst registrieren (commands.rs:626-630).
  if (info) projections.push(await writeRow(ctx, C.assetTypes, info.asset_type, assetTypeRow(info)));
  // Bestehendes Asset behält seine Attribute (commands.rs:633-652).
  const asset = existingRow?.data
    ? structuredClone(existingRow.data)
    : info
      ? newAssetWithType(id, realm, req.asset_type, req.name, info)
      : { id, parent_id: null, realm, asset_type: req.asset_type, name: req.name, path: [], attributes: {} };
  asset.realm = realm;
  asset.asset_type = req.asset_type;
  asset.name = req.name;
  asset.parent_id = req.parent_id;
  if (!asset.attributes || typeof asset.attributes !== 'object') fail('invalid', `asset ${id} has no attribute map`);
  projections.unshift(await writeRow(ctx, C.assets, id, assetRow(asset), existingRow));
  // §2A.14 Attribut-Deckel (commands.rs:684-708): höchstens MAX_ATTRIBUTES_PER_ASSET Attributzeilen.
  const names = attributeNames(asset);
  if (names.length > MAX_ATTRIBUTES_PER_ASSET) {
    ctx.log?.('warn', `asset_upsert projection truncated to ${MAX_ATTRIBUTES_PER_ASSET} attribute rows`, { asset_id: id, had: names.length });
  }
  for (const name of names.slice(0, MAX_ATTRIBUTES_PER_ASSET)) {
    const row = attributeRow(asset, name);
    if (row) projections.push(await writeRow(ctx, C.attributes, attributeRecordId(id, name), row));
  }
  return outcome({ asset }, projections);
}
