// Origin: CTOX
// License: AGPL-3.0-only
//
// IoT time-series store + queries (§2A.9-14). Ported query *semantics* from
// OpenRemote (AGPL-3.0, archive/openremote, HEAD 22a42a7); persistence is
// CTOX-native SQLite (single runtime/ctox.sqlite3 via crate::paths::core_db).
//
// Time model (see iot/mod.rs):
//   * datapoint / value / event time is `i64` epoch-ms UTC (the ported domain
//     dimension, §2A.13).
//   * created_at / updated_at audit columns are RFC-3339 millis-precision UTC
//     TEXT (CTOX house style) — not used here (datapoints are immutable
//     append-only samples keyed purely by epoch-ms), but kept available.
//
// ref: AssetDatapointLTTBQuery.java:27-36
// ref: AssetDatapointResource.java (interval / nearest / all queries)

use crate::iot::model::*;
use crate::iot::{now_ms, Result};
use anyhow::{bail, Context};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// Default hard upper bound on rows returned by a single query (§2A.14).
/// Bounded and LOGGED on truncation, never silent.
pub(crate) const DEFAULT_QUERY_LIMIT: usize = 100_000;

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Open the consolidated runtime store and ensure the datapoints schema.
/// Mirrors business_os/store.rs::open_store, but targets the core db
/// (runtime/ctox.sqlite3) per CTOX's single-store rule.
pub(crate) fn open(root: &Path) -> Result<Connection> {
    let path = crate::paths::core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open IoT core store {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure IoT SQLite busy_timeout")?;
    let ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout={ms};"
    ))
    .context("failed to configure IoT SQLite pragmas")?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Create the datapoints table + lookup index if absent.
///
/// `value` holds the canonical JSON of the recorded `AttributeValue`.
/// `timestamp_ms` is i64 epoch-ms UTC (§2A.13) — the sample's domain time.
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS iot_datapoints (
            asset_id       TEXT    NOT NULL,
            attribute_name TEXT    NOT NULL,
            timestamp_ms   INTEGER NOT NULL,
            value          TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_datapoints_lookup
            ON iot_datapoints (asset_id, attribute_name, timestamp_ms);",
    )
    .context("failed to initialize iot_datapoints schema")?;
    Ok(())
}

/// A single stored sample. `timestamp_ms` is i64 epoch-ms UTC (§2A.13).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Datapoint {
    pub asset_id: String,
    pub attribute_name: String,
    pub timestamp_ms: i64,
    pub value: AttributeValue,
}

/// Record one recorded value (§2A.3 — store.rs calls this for BOTH outdated and
/// current events; every recorded value is persisted, ordering is by the
/// sample's own `ts_ms`, not arrival order).
pub(crate) fn record_datapoint(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    value: &AttributeValue,
    ts_ms: i64,
) -> Result<()> {
    let value_json = serde_json::to_string(value).context("failed to serialize datapoint value")?;
    conn.execute(
        "INSERT INTO iot_datapoints (asset_id, attribute_name, timestamp_ms, value)
         VALUES (?1, ?2, ?3, ?4)",
        params![asset_id, attribute_name, ts_ms, value_json],
    )
    .context("failed to insert datapoint")?;
    Ok(())
}

/// Convenience: record using the current UTC clock (§2A.13) as the sample time.
#[allow(dead_code)]
pub(crate) fn record_now(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    value: &AttributeValue,
) -> Result<()> {
    record_datapoint(conn, asset_id, attribute_name, value, now_ms())
}

// ---------------------------------------------------------------------------
// Raw fetch (shared by all query shapes; applies the hard limit §2A.14)
// ---------------------------------------------------------------------------

/// Fetch raw samples in `[from_ms, to_ms]` (inclusive both ends), ordered by
/// `timestamp_ms` ascending. Applies the hard result limit (§2A.14): on
/// truncation it logs a warning and returns the bounded prefix — never silent.
fn fetch_range(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    from_ms: i64,
    to_ms: i64,
    limit: usize,
) -> Result<Vec<Datapoint>> {
    // Over-fetch by one row so we can detect (and log) truncation deterministically.
    let fetch_cap = limit as i64 + 1;
    let mut stmt = conn.prepare(
        "SELECT asset_id, attribute_name, timestamp_ms, value
         FROM iot_datapoints
         WHERE asset_id = ?1 AND attribute_name = ?2
           AND timestamp_ms >= ?3 AND timestamp_ms <= ?4
         ORDER BY timestamp_ms ASC
         LIMIT ?5",
    )?;
    let rows = stmt.query_map(
        params![asset_id, attribute_name, from_ms, to_ms, fetch_cap],
        map_datapoint,
    )?;
    let mut out: Vec<Datapoint> = Vec::new();
    for row in rows {
        out.push(row?);
    }
    if out.len() > limit {
        out.truncate(limit);
        // §2A.14 — truncation is bounded AND logged, never silent. The rest of
        // this crate (business_os/store.rs) logs operational warnings via
        // eprintln!; match that idiom here.
        eprintln!(
            "CTOX IoT datapoint query truncated to limit {limit} for asset_id={asset_id} \
             attribute={attribute_name} window=[{from_ms},{to_ms}]"
        );
    }
    Ok(out)
}

fn map_datapoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<Datapoint> {
    let value_json: String = row.get(3)?;
    let value: AttributeValue = serde_json::from_str(&value_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Datapoint {
        asset_id: row.get(0)?,
        attribute_name: row.get(1)?,
        timestamp_ms: row.get(2)?,
        value,
    })
}

// ---------------------------------------------------------------------------
// §2A.12 — nearest at-or-before
// ---------------------------------------------------------------------------

/// Closest datapoint at-or-before `target_ms` (`timestamp_ms <= target`, ordered
/// descending, limit 1). §2A.12. Returns `None` if no sample precedes `target`.
pub(crate) fn nearest(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    target_ms: i64,
) -> Result<Option<Datapoint>> {
    let dp = conn
        .query_row(
            "SELECT asset_id, attribute_name, timestamp_ms, value
             FROM iot_datapoints
             WHERE asset_id = ?1 AND attribute_name = ?2
               AND timestamp_ms <= ?3
             ORDER BY timestamp_ms DESC
             LIMIT 1",
            params![asset_id, attribute_name, target_ms],
            map_datapoint,
        )
        .optional()
        .context("failed to query nearest datapoint")?;
    Ok(dp)
}

// ---------------------------------------------------------------------------
// All-query
// ---------------------------------------------------------------------------

/// All samples in `[from_ms, to_ms]`, ascending, bounded by `DEFAULT_QUERY_LIMIT`
/// (§2A.14).
pub(crate) fn all(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<Datapoint>> {
    fetch_range(
        conn,
        asset_id,
        attribute_name,
        from_ms,
        to_ms,
        DEFAULT_QUERY_LIMIT,
    )
}

// ---------------------------------------------------------------------------
// §2A.11 — interval query: bucket-aligned, gap-filled, LOCF
// ---------------------------------------------------------------------------

/// One emitted bucket of an interval query. `value` is `None` only when no
/// observation has ever been carried forward into this bucket (§2A.11 gap).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct IntervalPoint {
    /// Bucket start (inclusive), aligned to a multiple of `interval_ms` measured
    /// from epoch 0 (UTC) — NOT to a data timestamp (§2A.11, §2A.13).
    pub bucket_ms: i64,
    pub value: Option<AttributeValue>,
}

/// Interval (gap-filled + last-observation-carry-forward) query, §2A.11.
///
/// Buckets are aligned to bucket *boundaries* (multiples of `interval_ms` from
/// epoch 0 UTC), not to data timestamps (§2A.11) and not to a local zone
/// (§2A.13). Every bucket boundary in `[from_ms, to_ms]` is emitted exactly once:
///   * if one or more samples fall inside the bucket, the LAST one (by
///     timestamp) is the bucket value (down-sampling within a bucket);
///   * otherwise the bucket is gap-filled with the last observed value carried
///     forward from an earlier bucket (LOCF), seeded by the nearest sample
///     at-or-before the first bucket boundary;
///   * a leading run of buckets with no prior observation at all is emitted with
///     `value = None` (a true gap).
///
/// `interval_ms` must be > 0. The emitted bucket count is bounded by
/// `DEFAULT_QUERY_LIMIT` (§2A.14); the returned `bool` is `true` when the bucket
/// window was clamped to that limit so the caller can surface `truncated=true`
/// to the consumer (matching the all/lttb query shapes) — never silent.
///
/// ref: AssetDatapointIntervalQuery.java:47,56-58,96-100 — upstream delegates
/// to TimescaleDB's `time_bucket_gapfill(interval, timestamp) GROUP BY x` (the
/// boundary-aligned bucketing + gap-fill, line 47) and
/// `public.locf(public.last(value, timestamp))` (last-in-bucket value carried
/// forward across empty buckets, lines 56/96-100). There is no Java loop to port
/// byte-for-byte (the algorithm is a TimescaleDB aggregate), so the equivalent
/// bucket-walk + LOCF is implemented here with upstream variable intent (`x` =
/// bucket boundary, `last`/`locf` = last-observation-carry-forward).
pub(crate) fn interval(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    from_ms: i64,
    to_ms: i64,
    interval_ms: i64,
) -> Result<(Vec<IntervalPoint>, bool)> {
    if interval_ms <= 0 {
        bail!("interval_ms must be > 0 (got {interval_ms})");
    }
    if to_ms < from_ms {
        return Ok((Vec::new(), false));
    }

    // Align the first/last bucket to the boundary lattice (multiples of
    // interval_ms from epoch 0 UTC). floor_div handles negative epoch-ms too.
    let first_bucket = floor_to_bucket(from_ms, interval_ms);
    let last_bucket = floor_to_bucket(to_ms, interval_ms);

    // Bound the number of emitted buckets up front (§2A.14).
    let bucket_count = ((last_bucket - first_bucket) / interval_ms) + 1;
    let limited = bucket_count.max(0) as usize > DEFAULT_QUERY_LIMIT;
    let last_bucket = if limited {
        eprintln!(
            "CTOX IoT interval query truncated to {DEFAULT_QUERY_LIMIT} buckets for \
             asset_id={asset_id} attribute={attribute_name} window=[{from_ms},{to_ms}] \
             interval_ms={interval_ms}"
        );
        first_bucket + interval_ms * (DEFAULT_QUERY_LIMIT as i64 - 1)
    } else {
        last_bucket
    };

    // Seed LOCF with the nearest sample strictly before the first bucket so a
    // leading partial bucket already carries the prior observation forward.
    let mut carried: Option<AttributeValue> =
        nearest(conn, asset_id, attribute_name, first_bucket - 1)?.map(|dp| dp.value);

    // Fetch every sample inside the aligned window once, ascending, then walk it
    // alongside the bucket lattice (single pass, no per-bucket SQL).
    let window_end = last_bucket + interval_ms - 1;
    let samples = fetch_range(
        conn,
        asset_id,
        attribute_name,
        first_bucket,
        window_end,
        DEFAULT_QUERY_LIMIT,
    )?;

    let mut out: Vec<IntervalPoint> = Vec::new();
    let mut si = 0usize;
    let mut bucket = first_bucket;
    while bucket <= last_bucket {
        let bucket_end = bucket + interval_ms; // exclusive
                                               // The LAST sample within [bucket, bucket_end) is the bucket value.
        let mut bucket_value: Option<AttributeValue> = None;
        while si < samples.len() && samples[si].timestamp_ms < bucket_end {
            if samples[si].timestamp_ms >= bucket {
                bucket_value = Some(samples[si].value.clone());
            }
            si += 1;
        }
        match bucket_value {
            Some(v) => {
                carried = Some(v.clone());
                out.push(IntervalPoint {
                    bucket_ms: bucket,
                    value: Some(v),
                });
            }
            None => {
                // Gap: carry the last observation forward (LOCF), or None if we
                // have never observed a value at or before this bucket.
                out.push(IntervalPoint {
                    bucket_ms: bucket,
                    value: carried.clone(),
                });
            }
        }
        bucket += interval_ms;
    }
    // §2A.14 — return the truncation flag alongside the bounded buckets so the
    // command layer can set `truncated=true` (the eprintln above is the log; the
    // flag is the caller-visible signal, matching all()/lttb_query()).
    Ok((out, limited))
}

/// Floor `ts` to the nearest lower multiple of `interval` (works for negative
/// epoch-ms too — euclidean floor, NOT truncation-toward-zero, so we never
/// introduce a local-zone / sign bug at the epoch boundary, §2A.13).
fn floor_to_bucket(ts: i64, interval: i64) -> i64 {
    ts.div_euclid(interval) * interval
}

// ---------------------------------------------------------------------------
// §2A.9-10 — Largest-Triangle-Three-Buckets (LTTB) downsampler
// ---------------------------------------------------------------------------

/// A downsampled `(x, y)` point. `x` is epoch-ms, `y` the numeric value.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LttbPoint {
    pub x: i64,
    pub y: f64,
}

/// Downsample `[from_ms, to_ms]` for `(asset_id, attribute_name)` to at most
/// `threshold` points using LTTB (§2A.9-10).
///
/// Numeric/boolean values only: boolean coerces true→1.0 / false→0.0 via
/// `AttributeValue::as_numeric`; any non-numeric value is rejected explicitly
/// (§2A.9). Raw fetch is bounded by `DEFAULT_QUERY_LIMIT` (§2A.14).
pub(crate) fn lttb_query(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    from_ms: i64,
    to_ms: i64,
    threshold: usize,
) -> Result<Vec<LttbPoint>> {
    let raw = fetch_range(
        conn,
        asset_id,
        attribute_name,
        from_ms,
        to_ms,
        DEFAULT_QUERY_LIMIT,
    )?;
    let mut data: Vec<LttbPoint> = Vec::with_capacity(raw.len());
    for dp in &raw {
        match dp.value.as_numeric() {
            Some(y) => data.push(LttbPoint {
                x: dp.timestamp_ms,
                y,
            }),
            None => bail!(
                "LTTB requires numeric/boolean values; non-numeric sample at ts={} for \
                 asset_id={asset_id} attribute={attribute_name}",
                dp.timestamp_ms
            ),
        }
    }
    Ok(lttb(&data, threshold))
}

/// Classic Largest-Triangle-Three-Buckets downsampling.
///
/// ref: AssetDatapointLTTBQuery.java:27-36 — upstream delegates the actual
/// down-sampling to TimescaleDB's `lttb()` aggregate (there is NO Java LTTB to
/// port byte-for-byte), so the canonical algorithm is implemented here. It
/// follows Sveinn Steinarsson's reference (the same algorithm TimescaleDB's
/// `lttb()` implements): each of `threshold - 2` interior output points is the
/// data point (within its bucket) that forms the largest-area triangle with the
/// previously selected point and the *average* point of the next bucket.
///
/// §2A.9 edge cases: empty → empty; single → that one point; two → both. For
/// `threshold >= len` (or `threshold < 3`) the input is returned unchanged when
/// it already fits; `threshold == 0` likewise returns the input untouched.
/// §2A.10: the FIRST and LAST points are ALWAYS retained.
pub(crate) fn lttb(data: &[LttbPoint], threshold: usize) -> Vec<LttbPoint> {
    let n = data.len();
    // §2A.9: trivially-small inputs pass through (empty / single / two), and any
    // input already at or below the threshold needs no down-sampling. A
    // threshold < 3 cannot express "first + interior + last", so pass through.
    if n <= 2 || threshold >= n || threshold < 3 {
        return data.to_vec();
    }

    let mut sampled: Vec<LttbPoint> = Vec::with_capacity(threshold);

    // §2A.10: the first point is always retained.
    sampled.push(data[0]);

    // Bucket size over the interior points (exclude the first and last).
    // ref: lttb reference — `every = (data.length - 2) / (threshold - 2)`.
    let every = (n - 2) as f64 / (threshold - 2) as f64;

    // `a` is the index of the previously selected point (starts at the first).
    let mut a: usize = 0;

    for i in 0..(threshold - 2) {
        // Average point of the NEXT bucket (the "c" vertex of the triangle).
        // ref: lttb reference — average x/y over [avg_range_start, avg_range_end).
        let avg_range_start = (((i + 1) as f64) * every).floor() as usize + 1;
        let mut avg_range_end = (((i + 2) as f64) * every).floor() as usize + 1;
        if avg_range_end > n {
            avg_range_end = n;
        }
        let avg_range_len = (avg_range_end - avg_range_start) as f64;

        let mut avg_x = 0.0f64;
        let mut avg_y = 0.0f64;
        for j in avg_range_start..avg_range_end {
            avg_x += data[j].x as f64;
            avg_y += data[j].y;
        }
        if avg_range_len > 0.0 {
            avg_x /= avg_range_len;
            avg_y /= avg_range_len;
        }

        // Range of candidate points for THIS bucket (the "b" vertex).
        // ref: lttb reference — [range_offs, range_to).
        let range_offs = (((i) as f64) * every).floor() as usize + 1;
        let range_to = (((i + 1) as f64) * every).floor() as usize + 1;

        // Point "a" (previously selected) is the fixed first triangle vertex.
        let point_a_x = data[a].x as f64;
        let point_a_y = data[a].y;

        let mut max_area = -1.0f64;
        let mut next_a = range_offs;
        for j in range_offs..range_to.min(n) {
            // Triangle area (×2, abs) of (a, candidate, next-bucket-average).
            // ref: lttb reference — Math.abs(triangle area) used as the score.
            let area = ((point_a_x - avg_x) * (data[j].y - point_a_y)
                - (point_a_x - data[j].x as f64) * (avg_y - point_a_y))
                .abs()
                * 0.5;
            if area > max_area {
                max_area = area;
                next_a = j;
            }
        }

        sampled.push(data[next_a]); // pick the point that yields the largest triangle
        a = next_a; // this point becomes "a" for the next iteration
    }

    // §2A.10: the last point is always retained.
    sampled.push(data[n - 1]);

    sampled
}

// ---------------------------------------------------------------------------
// Tests (§2A.9-14)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn av(n: f64) -> AttributeValue {
        AttributeValue(json!(n))
    }

    fn p(x: i64, y: f64) -> LttbPoint {
        LttbPoint { x, y }
    }

    // -------------------------------------------------------------------
    // §2A.9-10 — LTTB golden tests
    // -------------------------------------------------------------------

    #[test]
    fn lttb_empty_returns_empty() {
        // §2A.9: empty → empty.
        let out = lttb(&[], 100);
        assert!(out.is_empty());
    }

    #[test]
    fn lttb_single_returns_that_point() {
        // §2A.9: single → that point.
        let only = p(5, 1.0);
        let out = lttb(&[only], 100);
        assert_eq!(out, vec![only]);
        // Also at threshold below 3 / equal to 1.
        assert_eq!(lttb(&[only], 1), vec![only]);
    }

    #[test]
    fn lttb_two_returns_both() {
        // §2A.9: two → both.
        let a = p(0, 0.0);
        let b = p(10, 5.0);
        let out = lttb(&[a, b], 100);
        assert_eq!(out, vec![a, b]);
        // Down to the minimum meaningful threshold still keeps both.
        assert_eq!(lttb(&[a, b], 2), vec![a, b]);
    }

    #[test]
    fn lttb_threshold_ge_len_passthrough() {
        let data = vec![p(0, 0.0), p(1, 1.0), p(2, 2.0), p(3, 3.0)];
        // threshold >= len → unchanged.
        assert_eq!(lttb(&data, 4), data);
        assert_eq!(lttb(&data, 10), data);
    }

    #[test]
    fn lttb_downsample_keeps_first_last_and_matches_reference() {
        // n >> buckets. A spike in the middle must be preferred by the
        // triangle-area selection. Hand-compute the reference below.
        //
        // x: 0..=9, y mostly flat at 0 with a spike of 100 at x=4.
        let data: Vec<LttbPoint> = vec![
            p(0, 0.0),
            p(1, 0.0),
            p(2, 0.0),
            p(3, 0.0),
            p(4, 100.0), // spike
            p(5, 0.0),
            p(6, 0.0),
            p(7, 0.0),
            p(8, 0.0),
            p(9, 0.0),
        ];
        let threshold = 4; // first + 2 interior + last
        let out = lttb(&data, threshold);

        assert_eq!(out.len(), threshold, "exactly threshold points");

        // §2A.10: first & last ALWAYS retained.
        assert_eq!(out.first().copied(), Some(data[0]), "first retained");
        assert_eq!(out.last().copied(), Some(data[9]), "last retained");

        // ---- hand-computed reference ----
        // n=10, threshold=4 → every = (10-2)/(4-2) = 4.0.
        // i=0: range_offs = floor(0*4)+1 = 1 ; range_to = floor(1*4)+1 = 5
        //      candidate indices [1,5) = {1,2,3,4}.
        //      avg bucket: avg_range_start = floor(1*4)+1 = 5 ;
        //                  avg_range_end   = floor(2*4)+1 = 9 ; pts {5,6,7,8}
        //                  all y=0 → avg=(x̄=6.5, ȳ=0).
        //      a = data[0] = (0,0). The triangle with the tallest |y| (x=4,y=100)
        //      dominates the area → picks index 4 (the spike).
        // i=1: range_offs = floor(1*4)+1 = 5 ; range_to = floor(2*4)+1 = 9
        //      candidate indices [5,9) = {5,6,7,8}, all y=0.
        //      avg bucket: start = floor(2*4)+1 = 9 ; end = floor(3*4)+1=13→clamp 10
        //                  pts {9} → avg=(9,0).
        //      a = previously selected = (4,100). All candidates collinear-ish at
        //      y=0; the largest-area triangle with apex a=(4,100) and avg=(9,0) is
        //      the FIRST max encountered (index 5) under strict `>` tie-breaking.
        // Reference output: indices [0, 4, 5, 9].
        let reference: Vec<LttbPoint> = vec![data[0], data[4], data[5], data[9]];
        assert_eq!(out, reference, "LTTB selection matches hand reference");

        // The middle spike must be chosen (bucket triangle-area selection).
        assert!(
            out.iter().any(|pt| pt.x == 4 && pt.y == 100.0),
            "spike retained by triangle-area selection"
        );
    }

    #[test]
    fn lttb_boolean_coerced_via_as_numeric() -> Result<()> {
        // §2A.9: boolean true→1 / false→0 through as_numeric.
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open(root)?;
        let asset = "a1";
        let attr = "on";
        record_datapoint(&conn, asset, attr, &AttributeValue(json!(false)), 0)?;
        record_datapoint(&conn, asset, attr, &AttributeValue(json!(true)), 10)?;
        record_datapoint(&conn, asset, attr, &AttributeValue(json!(false)), 20)?;
        let out = lttb_query(&conn, asset, attr, 0, 100, 3)?;
        // threshold>=len passthrough → all three, coerced.
        assert_eq!(out, vec![p(0, 0.0), p(10, 1.0), p(20, 0.0)]);
        Ok(())
    }

    #[test]
    fn lttb_non_numeric_rejected() -> Result<()> {
        // §2A.9: non-numeric rejected explicitly.
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open(root)?;
        record_datapoint(&conn, "a", "label", &AttributeValue(json!("hot")), 0)?;
        record_datapoint(&conn, "a", "label", &AttributeValue(json!(1.0)), 10)?;
        let err = lttb_query(&conn, "a", "label", 0, 100, 5).unwrap_err();
        assert!(err.to_string().contains("numeric/boolean"), "got: {err}");
        Ok(())
    }

    // -------------------------------------------------------------------
    // §2A.11 — interval bucketing + gap-fill + LOCF
    // -------------------------------------------------------------------

    #[test]
    fn interval_aligns_to_bucket_boundaries_not_data_ts() -> Result<()> {
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        // interval = 100ms. Sample at ts=137 falls in bucket [100,200).
        record_datapoint(&conn, asset, attr, &av(7.0), 137)?;
        let (out, truncated) = interval(&conn, asset, attr, 100, 299, 100)?;
        // Buckets must be 100,200 (boundary-aligned), NOT 137.
        assert_eq!(out[0].bucket_ms, 100);
        assert_eq!(out[1].bucket_ms, 200);
        assert_eq!(out[0].value, Some(av(7.0)), "sample lands in its bucket");
        assert!(!truncated, "small window is not truncated");
        Ok(())
    }

    #[test]
    fn interval_gapfill_and_locf() -> Result<()> {
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        // Samples at bucket 0 (=5.0) and bucket 300 (=9.0). Buckets 100, 200 are
        // gaps → must carry 5.0 forward (LOCF). Bucket 400 (no sample) carries 9.0.
        record_datapoint(&conn, asset, attr, &av(5.0), 10)?; // bucket 0
        record_datapoint(&conn, asset, attr, &av(9.0), 320)?; // bucket 300
        let (out, _truncated) = interval(&conn, asset, attr, 0, 499, 100)?;
        let got: Vec<(i64, Option<f64>)> = out
            .iter()
            .map(|ip| (ip.bucket_ms, ip.value.as_ref().and_then(|v| v.as_numeric())))
            .collect();
        assert_eq!(
            got,
            vec![
                (0, Some(5.0)),
                (100, Some(5.0)), // gap → LOCF 5.0
                (200, Some(5.0)), // gap → LOCF 5.0
                (300, Some(9.0)),
                (400, Some(9.0)), // gap → LOCF 9.0
            ]
        );
        Ok(())
    }

    #[test]
    fn interval_leading_gap_is_none_then_seeded_by_prior_sample() -> Result<()> {
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        // No sample before bucket 0 → leading buckets are a true gap (None).
        record_datapoint(&conn, asset, attr, &av(3.0), 250)?; // bucket 200
        let (out, _truncated) = interval(&conn, asset, attr, 0, 399, 100)?;
        let got: Vec<(i64, Option<f64>)> = out
            .iter()
            .map(|ip| (ip.bucket_ms, ip.value.as_ref().and_then(|v| v.as_numeric())))
            .collect();
        assert_eq!(
            got,
            vec![
                (0, None),   // no prior observation → true gap
                (100, None), // still no observation
                (200, Some(3.0)),
                (300, Some(3.0)), // LOCF
            ]
        );

        // And a query window that STARTS after a known sample must seed LOCF from
        // the nearest prior sample (not None).
        let (out2, _trunc2) = interval(&conn, asset, attr, 300, 399, 100)?;
        assert_eq!(out2[0].bucket_ms, 300);
        assert_eq!(out2[0].value, Some(av(3.0)), "seeded by prior sample");
        Ok(())
    }

    #[test]
    fn interval_last_sample_in_bucket_wins() -> Result<()> {
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        // Two samples in bucket [100,200): the later one is the bucket value.
        record_datapoint(&conn, asset, attr, &av(1.0), 110)?;
        record_datapoint(&conn, asset, attr, &av(2.0), 190)?;
        let (out, _truncated) = interval(&conn, asset, attr, 100, 199, 100)?;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value, Some(av(2.0)), "last sample in bucket wins");
        Ok(())
    }

    // -------------------------------------------------------------------
    // §2A.12 — nearest at-or-before (<=)
    // -------------------------------------------------------------------

    #[test]
    fn nearest_at_or_before_semantics() -> Result<()> {
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        record_datapoint(&conn, asset, attr, &av(1.0), 100)?;
        record_datapoint(&conn, asset, attr, &av(2.0), 200)?;
        record_datapoint(&conn, asset, attr, &av(3.0), 300)?;

        // Exactly on a sample → that sample (<= is inclusive).
        assert_eq!(nearest(&conn, asset, attr, 200)?.unwrap().value, av(2.0));
        // Between samples → the earlier one.
        assert_eq!(nearest(&conn, asset, attr, 250)?.unwrap().value, av(2.0));
        // After the last → the last.
        assert_eq!(nearest(&conn, asset, attr, 999)?.unwrap().value, av(3.0));
        // Before the first → None.
        assert!(nearest(&conn, asset, attr, 50)?.is_none());
        Ok(())
    }

    // -------------------------------------------------------------------
    // §2A.13 — UTC/epoch-ms normalization (no local-zone bug)
    // -------------------------------------------------------------------

    #[test]
    fn utc_epoch_ms_no_local_zone_bug() -> Result<()> {
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        // Store raw epoch-ms; the value must round-trip identically regardless of
        // any host TZ. Use a fixed UTC instant well-known to differ from local.
        // 2021-01-01T00:00:00Z = 1_609_459_200_000 ms.
        let ts = 1_609_459_200_000i64;
        record_datapoint(&conn, asset, attr, &av(42.0), ts)?;
        let dp = nearest(&conn, asset, attr, ts)?.unwrap();
        assert_eq!(dp.timestamp_ms, ts, "epoch-ms stored/returned verbatim");

        // Negative epoch-ms (pre-1970) bucket alignment uses euclidean floor, so
        // there is no sign/zone bug at the epoch boundary.
        assert_eq!(floor_to_bucket(-1, 100), -100);
        assert_eq!(floor_to_bucket(-100, 100), -100);
        assert_eq!(floor_to_bucket(-101, 100), -200);
        assert_eq!(floor_to_bucket(0, 100), 0);
        assert_eq!(floor_to_bucket(199, 100), 100);
        Ok(())
    }

    // -------------------------------------------------------------------
    // §2A.14 — query-limit truncation is bounded AND logged
    // -------------------------------------------------------------------

    #[test]
    fn query_limit_truncates_and_is_bounded() -> Result<()> {
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        // Insert more rows than the (tiny) limit we pass to fetch_range, and
        // confirm the result is bounded to that limit (and a warning is emitted —
        // observed via the bounded length; eprintln side-effect is the §2A.14 log).
        for i in 0..10i64 {
            record_datapoint(&conn, asset, attr, &av(i as f64), i)?;
        }
        let bounded = fetch_range(&conn, asset, attr, 0, 100, 3)?;
        assert_eq!(bounded.len(), 3, "result bounded to the hard limit");
        // Bounded prefix is the EARLIEST rows (ascending order preserved).
        assert_eq!(bounded[0].timestamp_ms, 0);
        assert_eq!(bounded[2].timestamp_ms, 2);

        // The public `all()` path applies DEFAULT_QUERY_LIMIT and never panics.
        let everything = all(&conn, asset, attr, 0, 100)?;
        assert_eq!(everything.len(), 10);
        Ok(())
    }

    #[test]
    fn interval_window_truncation_sets_flag() -> Result<()> {
        // §2A.14 — a bucket window wider than DEFAULT_QUERY_LIMIT buckets is
        // clamped AND reports truncated=true (not silent). With interval_ms=1 and
        // a window of DEFAULT_QUERY_LIMIT+10 ms, the bucket count exceeds the cap.
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        record_datapoint(&conn, asset, attr, &av(1.0), 0)?;
        let span = DEFAULT_QUERY_LIMIT as i64 + 10;
        let (out, truncated) = interval(&conn, asset, attr, 0, span, 1)?;
        assert!(truncated, "over-wide bucket window reports truncation");
        assert_eq!(
            out.len(),
            DEFAULT_QUERY_LIMIT,
            "bucket count clamped to the hard limit"
        );

        // A window that fits is NOT flagged.
        let (small, small_trunc) = interval(&conn, asset, attr, 0, 9, 1)?;
        assert!(!small_trunc, "in-bounds window is not truncated");
        assert_eq!(small.len(), 10);
        Ok(())
    }

    #[test]
    fn record_datapoint_stores_outdated_and_current() -> Result<()> {
        // §2A.3 — store.rs records BOTH outdated and current events; ordering is
        // by the sample's own timestamp, not insertion order.
        let temp = tempdir()?;
        let conn = open(temp.path())?;
        let (asset, attr) = ("a", "temp");
        // Insert out of chronological order (an "outdated" event arrives late).
        record_datapoint(&conn, asset, attr, &av(2.0), 200)?;
        record_datapoint(&conn, asset, attr, &av(1.0), 100)?; // outdated arrival
        let rows = all(&conn, asset, attr, 0, 1000)?;
        let ts: Vec<i64> = rows.iter().map(|d| d.timestamp_ms).collect();
        assert_eq!(ts, vec![100, 200], "stored and ordered by sample time");
        Ok(())
    }
}
