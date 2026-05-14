// Origin: CTOX
// License: Apache-2.0
//
// Shared Polars helpers for Level 3 operational verbs on
// record-shape knowledge tables.
//
// All helpers are `pub(super)` so they are only callable from the
// sibling `data` and `ops` submodules.

use anyhow::Context;
use anyhow::Result;
use polars::prelude::*;
use rusqlite::params;
use rusqlite::Connection;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;

// ----- scan helpers -------------------------------------------------------

/// Lazily scan the parquet file backing a knowledge data table.
///
/// Polars 0.52 takes a `PlPath` rather than a `&Path` here.
pub(super) fn scan_table(path: &Path) -> PolarsResult<LazyFrame> {
    let pl = PlPath::new(&path.to_string_lossy());
    LazyFrame::scan_parquet(pl, ScanArgsParquet::default())
}

// ----- atomic write -------------------------------------------------------

/// Write `df` to `target` atomically: `<target>.tmp` -> fsync -> rename.
pub(super) fn commit_parquet(target: &Path, mut df: DataFrame) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parquet parent {}", parent.display()))?;
    }
    let tmp = target.with_extension("parquet.tmp");
    {
        let f = File::create(&tmp)
            .with_context(|| format!("create tmp parquet {}", tmp.display()))?;
        ParquetWriter::new(f)
            .with_compression(ParquetCompression::Zstd(None))
            .with_statistics(StatisticsOptions::full())
            .finish(&mut df)
            .with_context(|| format!("write parquet to {}", tmp.display()))?;
        File::open(&tmp)
            .with_context(|| format!("reopen tmp parquet for fsync {}", tmp.display()))?
            .sync_all()
            .with_context(|| format!("fsync tmp parquet {}", tmp.display()))?;
    }
    fs::rename(&tmp, target)
        .with_context(|| format!("rename {} -> {}", tmp.display(), target.display()))?;
    if let Some(parent) = target.parent() {
        if let Ok(d) = File::open(parent) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

// ----- schema hash --------------------------------------------------------

/// SHA-256 hex over `name:dtype\n` lines in column order.
pub(super) fn schema_hash(schema: &Schema) -> String {
    let mut h = Sha256::new();
    for (n, dt) in schema.iter() {
        h.update(n.as_str().as_bytes());
        h.update(b":");
        h.update(format!("{dt:?}").as_bytes());
        h.update(b"\n");
    }
    format!("{:x}", h.finalize())
}

// ----- where-clause parsing ----------------------------------------------

fn parse_value(v: &str) -> Expr {
    if let Ok(i) = v.parse::<i64>() {
        return lit(i);
    }
    if let Ok(f) = v.parse::<f64>() {
        return lit(f);
    }
    match v {
        "true" | "True" => return lit(true),
        "false" | "False" => return lit(false),
        _ => {}
    }
    lit(v.to_string())
}

/// Parse a single `--where` clause into a polars predicate.
///
/// Supported ops (longest-first to avoid prefix collisions):
/// `<=`, `>=`, `!=`, `~` (substring), `=`, `<`, `>`.
fn parse_where(clause: &str) -> Result<Expr> {
    for op in ["<=", ">=", "!=", "~", "=", "<", ">"] {
        if let Some(idx) = clause.find(op) {
            let (c, rest) = clause.split_at(idx);
            let v = rest[op.len()..].trim();
            let col_name = c.trim();
            if col_name.is_empty() {
                anyhow::bail!("--where missing column name: {clause}");
            }
            let ce = col(col_name);
            return Ok(match op {
                "=" => ce.eq(parse_value(v)),
                "!=" => ce.neq(parse_value(v)),
                "<" => ce.lt(parse_value(v)),
                "<=" => ce.lt_eq(parse_value(v)),
                ">" => ce.gt(parse_value(v)),
                ">=" => ce.gt_eq(parse_value(v)),
                "~" => ce.str().contains(lit(v.to_string()), false),
                _ => unreachable!(),
            });
        }
    }
    anyhow::bail!("bad --where clause (expected col<op>val): {clause}")
}

/// Extract the column name referenced by each `--where` clause, in order.
/// Used by verbs to validate the references against the parquet schema
/// before letting Polars surface a cryptic resolution error.
pub(super) fn where_column_names(clauses: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(clauses.len());
    for clause in clauses {
        for op in ["<=", ">=", "!=", "~", "=", "<", ">"] {
            if let Some(idx) = clause.find(op) {
                let name = clause[..idx].trim();
                if !name.is_empty() {
                    out.push(name.to_string());
                }
                break;
            }
        }
    }
    out
}

/// AND-chain a sequence of `--where` clauses; `None` if empty.
pub(super) fn parse_wheres(clauses: &[String]) -> Result<Option<Expr>> {
    let mut acc: Option<Expr> = None;
    for c in clauses {
        let e = parse_where(c)?;
        acc = Some(match acc {
            Some(p) => p.and(e),
            None => e,
        });
    }
    Ok(acc)
}

/// Parse a single `--set` value: `c1=v1,c2=v2` into `[(c1, v1), (c2, v2)]`.
///
/// Splits on `,` then on the first `=`. Whitespace around tokens is
/// trimmed. Values are emitted as raw strings — coercion to dtype is the
/// caller's responsibility (the `update` verb uses `parse_value` via the
/// `lit_for_set` shim).
pub(super) fn parse_set_clauses(raw: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for chunk in raw.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((k, v)) = chunk.split_once('=') else {
            anyhow::bail!("--set expects `col=val[,col=val]`, got: {chunk}");
        };
        let k = k.trim();
        let v = v.trim();
        if k.is_empty() {
            anyhow::bail!("--set has empty column name in chunk: {chunk}");
        }
        out.push((k.to_string(), v.to_string()));
    }
    if out.is_empty() {
        anyhow::bail!("--set produced no assignments");
    }
    Ok(out)
}

/// Expr literal for a `--set` value with i64 -> f64 -> bool -> string priority.
pub(super) fn lit_for_set(v: &str) -> Expr {
    parse_value(v)
}

// ----- JSON rows in / out -------------------------------------------------

fn cerr<E: ToString>(e: E) -> anyhow::Error {
    anyhow::anyhow!(e.to_string())
}

/// Build a DataFrame from a slice of JSON object rows. Missing keys
/// become NULL via inferred schema (NDJSON read).
pub(super) fn rows_to_df(rows: &[Value]) -> Result<DataFrame> {
    if rows.is_empty() {
        return Ok(DataFrame::empty());
    }
    let mut buf = Vec::<u8>::with_capacity(rows.len() * 64);
    for r in rows {
        serde_json::to_writer(&mut buf, r).map_err(cerr)?;
        buf.push(b'\n');
    }
    let infer = std::num::NonZeroUsize::new(rows.len().min(1024).max(1));
    let df = JsonReader::new(Cursor::new(buf))
        .with_json_format(JsonFormat::JsonLines)
        .infer_schema_len(infer)
        .finish()
        .context("failed to parse rows JSON into DataFrame")?;
    Ok(df)
}

/// Convert a DataFrame to NDJSON-shaped `Vec<serde_json::Value>` of objects.
pub(super) fn df_to_rows(df: &DataFrame) -> Result<Vec<Value>> {
    if df.height() == 0 {
        return Ok(Vec::new());
    }
    let mut buf = Vec::<u8>::new();
    JsonWriter::new(&mut buf)
        .with_json_format(JsonFormat::JsonLines)
        .finish(&mut df.clone())
        .context("write DataFrame as NDJSON")?;
    let mut out = Vec::with_capacity(df.height());
    for line in buf.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        out.push(serde_json::from_slice(line).map_err(cerr)?);
    }
    Ok(out)
}

// ----- catalog refresh ----------------------------------------------------

/// After a successful parquet write, update the catalog row with the
/// fresh row count, bytes-on-disk, schema hash, and updated-at stamp.
pub(super) fn refresh_catalog_after_write(
    conn: &Connection,
    domain: &str,
    table_key: &str,
    df: &DataFrame,
    parquet_path: &Path,
    now: &str,
) -> Result<(i64, i64, String)> {
    let bytes = fs::metadata(parquet_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let row_count = df.height() as i64;
    let hash = schema_hash(df.schema().as_ref());

    let updated = conn.execute(
        "UPDATE knowledge_data_tables
            SET row_count = ?1,
                bytes = ?2,
                schema_hash = ?3,
                updated_at = ?4
          WHERE domain = ?5 AND table_key = ?6",
        params![row_count, bytes, hash, now, domain, table_key],
    )?;
    if updated == 0 {
        anyhow::bail!(
            "catalog row missing during refresh: domain={domain} key={table_key} \
             (parquet was written but the catalog does not know about this table)"
        );
    }
    Ok((row_count, bytes, hash))
}
