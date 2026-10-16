// Origin: CTOX
// License: Apache-2.0
// Zeitreihe der iot-App über das Kernprimitiv knowledge.query (§16.30 flach; = `ctox knowledge data …`, die
// Parquet-Tabellen-Engine hinter dataframe.window). Der Kern kennt weder dataframe.table.import noch
// dataframe.query (Kernvertrag core-contract.mjs; dataframe.window nur mit leerem filter/sort) – Schreiben ist
// `data append`, Lesen `data select` mit --where/--order-by/--limit. Abfrageformen (all/interval/lttb) sind
// App-Logik und hier portiert: ref src/core/iot/datapoints.rs @ 4794af03d (fetch_range :120-160, nearest :175-195,
// interval :250-325, lttb_query/lttb :340-495, DEFAULT_QUERY_LIMIT :25).
// Spalten: asset_id, attribute_name, timestamp_ms, value_json (kanonisches JSON des Werts wie die Rust-Spalte
// `value TEXT`; eine Parquet-Spalte hat genau einen Typ, die Attributwerte sind gemischt).
import { fail } from './_base.js';

export const TIMESERIES_DOMAIN = 'iot';
export const TIMESERIES_TABLE = 'iot_timeseries';
// ref: datapoints.rs:25 DEFAULT_QUERY_LIMIT
export const DEFAULT_QUERY_LIMIT = 100_000;

const tableArgs = ['--domain', TIMESERIES_DOMAIN, '--key', TIMESERIES_TABLE];

async function knowledge(ctx, argv) {
  return ctx.cap.knowledge.query({ argv: ['data', ...argv] });
}

const failed = (res) => !res || res.ok === false;

/** Datenpunkt anhängen (ref: datapoints.rs:88 record_datapoint). Tabelle beim ersten Datenpunkt anlegen. */
export async function recordDatapoint(ctx, { asset_id, attribute_name, timestamp_ms, value }) {
  const rows = JSON.stringify([{ asset_id, attribute_name, timestamp_ms, value_json: JSON.stringify(value ?? null) }]);
  const append = () => knowledge(ctx, ['append', ...tableArgs, '--rows', rows]);
  let res = await append().catch((error) => ({ ok: false, error }));
  if (!failed(res)) return res;
  const described = await knowledge(ctx, ['describe', ...tableArgs]).catch(() => null);
  if (!failed(described)) fail('unavailable', `iot timeseries append failed: ${res.error?.message ?? res.error ?? 'unknown'}`);
  const created = await knowledge(ctx, [
    'create', ...tableArgs, '--source-system', 'iot', '--title', 'IoT-Zeitreihe',
    '--description', 'Telemetrie-Historie der iot-App (asset_id, attribute_name, timestamp_ms, value_json)',
  ]);
  if (failed(created)) fail('unavailable', 'iot timeseries table could not be created');
  res = await append();
  if (failed(res)) fail('unavailable', 'iot timeseries append failed');
  return res;
}

// `col=val` parst val im Kern als Zahl/Bool, wenn möglich (parquet_io.rs parse_value) – für Zeichenkettenspalten
// mit zahlartigem Inhalt daher `~` (immer String-Literal, Teilzeichenkette); exakt nachgefiltert in matchesKey.
function eqString(column, value) {
  const numeric = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(value) || /^(true|false|True|False|inf|NaN)$/i.test(value);
  return `${column}${numeric ? '~' : '='}${value}`;
}

function toDatapoint(row) {
  let value = null;
  try { value = JSON.parse(row.value_json); } catch { value = null; }
  return { asset_id: row.asset_id, attribute_name: row.attribute_name, timestamp_ms: Number(row.timestamp_ms), value };
}

async function select(ctx, assetId, attributeName, wheres, orderBy, limit) {
  const res = await knowledge(ctx, [
    'select', ...tableArgs, '--where', eqString('asset_id', assetId), '--where', eqString('attribute_name', attributeName),
    ...wheres.flatMap((w) => ['--where', w]), '--order-by', orderBy, '--limit', String(limit),
  ]).catch((error) => {
    if (/not found/i.test(String(error?.message ?? ''))) return { ok: true, rows: [] };
    throw error;
  });
  if (res?.ok === false) {
    if (/not found/i.test(String(res.error ?? res.message ?? ''))) return [];
    fail('unavailable', `iot timeseries select failed: ${res.error ?? res.message ?? 'unknown'}`);
  }
  return (res?.rows ?? []).filter((r) => r.asset_id === assetId && r.attribute_name === attributeName).map(toDatapoint);
}

/** ref: datapoints.rs:120 fetch_range – [from, to] inklusiv, aufsteigend, harte Grenze mit Kennzeichen. */
export async function fetchRange(ctx, assetId, attributeName, fromMs, toMs, limit = DEFAULT_QUERY_LIMIT) {
  const rows = await select(ctx, assetId, attributeName,
    [`timestamp_ms>=${fromMs}`, `timestamp_ms<=${toMs}`], 'timestamp_ms', limit + 1);
  const truncated = rows.length > limit;
  if (truncated) ctx.log?.('warn', `iot datapoint query truncated to limit ${limit} for asset_id=${assetId} attribute=${attributeName} window=[${fromMs},${toMs}]`);
  return { rows: truncated ? rows.slice(0, limit) : rows, truncated };
}

/** ref: datapoints.rs:175 nearest – jüngster Datenpunkt mit timestamp_ms <= target. */
export async function nearest(ctx, assetId, attributeName, targetMs) {
  const rows = await select(ctx, assetId, attributeName, [`timestamp_ms<=${targetMs}`], 'timestamp_ms:desc', 1);
  return rows[0] ?? null;
}

const floorToBucket = (ts, interval) => Math.floor(ts / interval) * interval;

/** ref: datapoints.rs:250 interval – Buckets ab Epoche ausgerichtet, letzter Wert je Bucket, LOCF, führende Lücke null. */
export async function intervalQuery(ctx, assetId, attributeName, fromMs, toMs, intervalMs) {
  if (intervalMs <= 0) fail('invalid', `interval_ms must be > 0 (got ${intervalMs})`);
  if (toMs < fromMs) return { points: [], truncated: false };
  const firstBucket = floorToBucket(fromMs, intervalMs);
  let lastBucket = floorToBucket(toMs, intervalMs);
  const bucketCount = (lastBucket - firstBucket) / intervalMs + 1;
  const limited = bucketCount > DEFAULT_QUERY_LIMIT;
  if (limited) lastBucket = firstBucket + intervalMs * (DEFAULT_QUERY_LIMIT - 1);
  let carried = (await nearest(ctx, assetId, attributeName, firstBucket - 1))?.value ?? null;
  const { rows: samples } = await fetchRange(ctx, assetId, attributeName, firstBucket, lastBucket + intervalMs - 1);
  const points = [];
  let si = 0;
  for (let bucket = firstBucket; bucket <= lastBucket; bucket += intervalMs) {
    const bucketEnd = bucket + intervalMs;
    let value;
    let hit = false;
    while (si < samples.length && samples[si].timestamp_ms < bucketEnd) {
      if (samples[si].timestamp_ms >= bucket) { value = samples[si].value; hit = true; }
      si += 1;
    }
    if (hit) carried = value;
    points.push({ timestamp: bucket, value: hit ? value : carried });
  }
  return { points, truncated: limited };
}

const asNumeric = (v) => (typeof v === 'number' ? v : typeof v === 'boolean' ? (v ? 1 : 0) : null);

/** ref: datapoints.rs:440 lttb (Largest-Triangle-Three-Buckets, erster und letzter Punkt bleiben). */
export function lttb(data, threshold) {
  const n = data.length;
  if (n <= 2 || threshold >= n || threshold < 3) return data.slice();
  const sampled = [data[0]];
  const every = (n - 2) / (threshold - 2);
  let a = 0;
  for (let i = 0; i < threshold - 2; i += 1) {
    const avgStart = Math.floor((i + 1) * every) + 1;
    const avgEnd = Math.min(Math.floor((i + 2) * every) + 1, n);
    const avgLen = avgEnd - avgStart;
    let avgX = 0;
    let avgY = 0;
    for (let j = avgStart; j < avgEnd; j += 1) { avgX += data[j].x; avgY += data[j].y; }
    if (avgLen > 0) { avgX /= avgLen; avgY /= avgLen; }
    const rangeOffs = Math.floor(i * every) + 1;
    const rangeTo = Math.floor((i + 1) * every) + 1;
    const ax = data[a].x;
    const ay = data[a].y;
    let maxArea = -1;
    let nextA = rangeOffs;
    for (let j = rangeOffs; j < Math.min(rangeTo, n); j += 1) {
      const area = Math.abs((ax - avgX) * (data[j].y - ay) - (ax - data[j].x) * (avgY - ay)) * 0.5;
      if (area > maxArea) { maxArea = area; nextA = j; }
    }
    sampled.push(data[nextA]);
    a = nextA;
  }
  sampled.push(data[n - 1]);
  return sampled;
}

/** ref: datapoints.rs:340 lttb_query – nur Zahl/Bool, sonst invalid. */
export async function lttbQuery(ctx, assetId, attributeName, fromMs, toMs, threshold) {
  const { rows } = await fetchRange(ctx, assetId, attributeName, fromMs, toMs);
  const data = rows.map((dp) => {
    const y = asNumeric(dp.value);
    if (y === null) {
      fail('invalid', `LTTB requires numeric/boolean values; non-numeric sample at ts=${dp.timestamp_ms} for asset_id=${assetId} attribute=${attributeName}`);
    }
    return { x: dp.timestamp_ms, y };
  });
  const sampled = lttb(data, threshold);
  // ref: commands.rs:957 truncated = pts.len() > threshold
  return { points: sampled.map((p) => ({ timestamp: p.x, value: p.y })), truncated: sampled.length > threshold };
}
