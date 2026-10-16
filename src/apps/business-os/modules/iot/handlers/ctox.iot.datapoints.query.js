// Origin: CTOX
// License: Apache-2.0
// ctox.iot.datapoints.query – ref: src/core/iot/commands.rs:902-1015 datapoints_query. Realm-Prüfung und Fenster-Zeile
// in der App; Formen all|interval|lttb ebenfalls in der App (_timeseries.js, Lesen über knowledge.query `data select`).
import { fail } from './_base.js';
import { C, loadAssetInRealm, outcome, parsePayload, sessionRealm, writeRow } from './_iotcore.js';
import { fetchRange, intervalQuery, lttbQuery } from './_timeseries.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const req = parsePayload(command, {
    asset_id: 'string', attribute_name: 'string', from_ms: 'i64', to_ms: 'i64', shape: 'string', interval_ms: 'i64?', threshold: 'i64?',
  });
  const asset = await loadAssetInRealm(ctx, req.asset_id, realm);
  const shape = req.shape.toLowerCase();
  let res;
  if (shape === 'interval') {
    if (req.interval_ms === null) fail('invalid', 'interval shape requires interval_ms');
    res = await intervalQuery(ctx, req.asset_id, req.attribute_name, req.from_ms, req.to_ms, req.interval_ms);
  } else if (shape === 'lttb') {
    if (req.threshold === null || req.threshold < 0) fail('invalid', 'lttb shape requires threshold');
    res = await lttbQuery(ctx, req.asset_id, req.attribute_name, req.from_ms, req.to_ms, req.threshold);
  } else if (shape === 'all') {
    const all = await fetchRange(ctx, req.asset_id, req.attribute_name, req.from_ms, req.to_ms);
    res = { points: all.rows.map((r) => ({ timestamp: r.timestamp_ms, value: r.value })), truncated: all.truncated };
  } else fail('invalid', `unknown datapoints shape: ${shape} (expected all|interval|lttb)`);
  const { points, truncated } = res;
  const windowKey = `${req.asset_id}:${req.attribute_name}:${req.from_ms}:${req.to_ms}:${shape}`;
  const projection = await writeRow(ctx, C.datapoints, windowKey, {
    realm: asset.realm, asset_id: req.asset_id, attribute_name: req.attribute_name, from_ms: req.from_ms, to_ms: req.to_ms, shape,
    point_count: points.length, truncated, data: { points }, index_text: `${req.asset_id}:${req.attribute_name}`.toLowerCase(), sort_key: windowKey,
  });
  return outcome({ window: windowKey, shape, point_count: points.length, truncated, points }, [projection]);
}
