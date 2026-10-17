// Origin: CTOX
// License: Apache-2.0
// ctox.iot.asset.delete – ref: src/core/iot/commands.rs:2409-2415 (Pflichtfeld), :728-779 asset_delete, store.rs:400-409
import { C, loadAssetInRealm, outcome, removeRow, requireField, sessionRealm } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const assetId = requireField(command, 'asset_id', 'string');
  await loadAssetInRealm(ctx, assetId, realm); // fremder Realm/fehlend = „asset not found“, kein Löschen
  const projections = [await removeRow(ctx, C.assets, assetId)];
  // Attributzeilen des Assets mitlöschen (store.rs:401-405).
  for (const row of await ctx.read(C.attributes, { selector: { asset_id: assetId } })) {
    projections.push(await removeRow(ctx, C.attributes, row.id));
  }
  return outcome({ deleted: assetId }, projections);
}
