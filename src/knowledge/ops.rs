// Origin: CTOX
// License: Apache-2.0
//
// Level 3 — operational verbs on record-shape knowledge tables.
//
// All twelve verbs are dispatched here. Each operates on the Parquet file
// referenced by a catalog row in `knowledge_data_tables`. Read verbs leave
// the catalog untouched; write verbs update row_count / bytes / schema_hash
// / updated_at via the shared commit helper.
//
// Polars-backed; no Python at this layer. Python-driven data-science work
// uses Level 2's `clone` (already shipped) plus the `import` verb here to
// land results back in the catalog.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use polars::prelude::*;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use super::data::{
    compute_parquet_path, find_flag, find_table, now_rfc3339, open_runtime_db, print_json,
    required_flag,
};
use super::parquet_io::{
    commit_parquet, df_to_rows, lit_for_set, parse_set_clauses, parse_wheres,
    refresh_catalog_after_write, rows_to_df, scan_table, where_column_names,
};

pub fn operational_verbs() -> Value {
    json!([
        {"verb": "head",         "args": "--domain X --key Y [--limit N] [--columns c1,c2]"},
        {"verb": "schema",       "args": "--domain X --key Y"},
        {"verb": "stats",        "args": "--domain X --key Y [--columns c1,c2]"},
        {"verb": "count",        "args": "--domain X --key Y [--where col=val]"},
        {"verb": "select",       "args": "--domain X --key Y [--columns c1,c2] [--where col=val] [--limit N] [--offset N] [--order-by col[:desc]]"},
        {"verb": "append",       "args": "--domain X --key Y --rows <json-array>"},
        {"verb": "update",       "args": "--domain X --key Y --where col=val --set \"c1=v1,c2=v2\""},
        {"verb": "delete-rows",  "args": "--domain X --key Y --where col=val"},
        {"verb": "add-column",   "args": "--domain X --key Y --column N --dtype T [--default V]"},
        {"verb": "drop-column",  "args": "--domain X --key Y --column N"},
        {"verb": "import",       "args": "--domain X --key Y --from-file <path> [--mode <replace|append>]"},
        {"verb": "export",       "args": "--domain X --key Y --to-file <path>"},
    ])
}

// ----- shared arg-parsing helpers ----------------------------------------

/// Collect `--columns c1,c2,c3` into a Vec<String>, trimming.
fn parse_columns(args: &[String]) -> Option<Vec<String>> {
    let raw = find_flag(args, "--columns")?;
    let cols: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if cols.is_empty() {
        None
    } else {
        Some(cols)
    }
}

/// Collect ALL `--where` flag occurrences. CTOX flag-parsing relies on
/// scanning the arg vector; `find_flag` only returns the first match.
fn collect_wheres(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--where" {
            if let Some(v) = args.get(i + 1) {
                out.push(v.clone());
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn parse_usize_flag(args: &[String], flag: &str) -> Result<Option<usize>> {
    match find_flag(args, flag) {
        Some(raw) => Ok(Some(
            raw.parse::<usize>()
                .with_context(|| format!("{flag} expects a non-negative integer, got `{raw}`"))?,
        )),
        None => Ok(None),
    }
}

/// Resolve the catalog row and parquet path for `--domain/--key`, validating
/// the catalog row exists. Read verbs tolerate a missing parquet file via
/// the caller; write verbs MUST also call `bail_if_archived` before mutating.
fn resolve_table(
    root: &Path,
    args: &[String],
    usage: &'static str,
) -> Result<(String, String, PathBuf, rusqlite::Connection, Map<String, Value>)> {
    let domain = required_flag(args, "--domain", usage)?.to_string();
    let table_key = required_flag(args, "--key", usage)?.to_string();
    let conn = open_runtime_db(root)?;
    let row = find_table(&conn, &domain, &table_key)?.with_context(|| {
        format!("knowledge data table not found: domain={domain} key={table_key}")
    })?;
    let parquet_path = row
        .get("parquet_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| compute_parquet_path(root, &domain, &table_key));
    Ok((domain, table_key, parquet_path, conn, row))
}

/// Refuse mutating ops on archived tables. Writes on archived data
/// silently corrupt the "frozen" assumption — restore first.
fn bail_if_archived(row: &Map<String, Value>, domain: &str, table_key: &str) -> Result<()> {
    let archived = row
        .get("archived_at")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    if archived {
        bail!(
            "knowledge data table is archived: domain={domain} key={table_key}; \
             restore with `ctox knowledge data restore` before writing"
        );
    }
    Ok(())
}

/// Validate that every requested column exists in the schema. Surfaces a
/// clean error listing the missing names + available names, instead of
/// letting Polars emit a cryptic resolution dump.
fn ensure_columns_present(schema: &Schema, cols: &[String], what: &str) -> Result<()> {
    let mut missing: Vec<String> = Vec::new();
    for c in cols {
        if !schema.iter_names().any(|n| n.as_str() == c.as_str()) {
            missing.push(c.clone());
        }
    }
    if !missing.is_empty() {
        let available: Vec<String> = schema.iter_names().map(|n| n.to_string()).collect();
        bail!("{what}: column(s) {missing:?} not in schema; available: {available:?}");
    }
    Ok(())
}

// ----- read verbs ---------------------------------------------------------

pub fn head(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data head --domain X --key Y [--limit N] [--columns c1,c2]";
    let (_d, _k, path, _conn, _row) = resolve_table(root, args, USAGE)?;
    let limit = parse_usize_flag(args, "--limit")?.unwrap_or(20);
    let columns = parse_columns(args);

    if !path.exists() {
        return print_json(&json!({"ok": true, "rows": [], "n_rows": 0}));
    }

    let mut lf = scan_table(&path).context("scan parquet for head")?;
    if let Some(cols) = columns.as_deref() {
        let schema = scan_table(&path)?.collect_schema()?;
        ensure_columns_present(&schema, cols, "head --columns")?;
    }
    if let Some(cols) = columns.as_deref() {
        lf = lf.select(cols.iter().map(|c| col(c.as_str())).collect::<Vec<_>>());
    }
    let df = lf
        .limit(limit as IdxSize)
        .collect()
        .context("collect head DataFrame")?;
    let rows = df_to_rows(&df)?;
    print_json(&json!({
        "ok": true,
        "rows": rows,
        "n_rows": df.height(),
    }))
}

pub fn schema(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str = "ctox knowledge data schema --domain X --key Y";
    let (_d, _k, path, _conn, _row) = resolve_table(root, args, USAGE)?;

    if !path.exists() {
        return print_json(&json!({"ok": true, "columns": {}, "n_columns": 0}));
    }

    let mut lf = scan_table(&path).context("scan parquet for schema")?;
    let schema = lf
        .collect_schema()
        .context("collect parquet schema")?;
    let mut columns = Map::new();
    for (name, dt) in schema.iter() {
        columns.insert(name.to_string(), Value::String(format!("{dt:?}")));
    }
    let n = columns.len();
    print_json(&json!({
        "ok": true,
        "columns": columns,
        "n_columns": n,
    }))
}

pub fn stats(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data stats --domain X --key Y [--columns c1,c2]";
    let (_d, _k, path, _conn, _row) = resolve_table(root, args, USAGE)?;
    let columns = parse_columns(args);

    if !path.exists() {
        return print_json(&json!({"ok": true, "stats": {}, "n_rows": 0}));
    }

    let mut lf = scan_table(&path).context("scan parquet for stats")?;
    if let Some(cols) = columns.as_deref() {
        let schema = scan_table(&path)?.collect_schema()?;
        ensure_columns_present(&schema, cols, "stats --columns")?;
    }
    if let Some(cols) = columns.as_deref() {
        lf = lf.select(cols.iter().map(|c| col(c.as_str())).collect::<Vec<_>>());
    }
    let df = lf.collect().context("collect DataFrame for stats")?;

    let mut out = Map::new();
    for col_ref in df.get_columns() {
        let name = col_ref.name().to_string();
        let null_count = col_ref.null_count();
        let n_unique = col_ref.n_unique().ok();
        let dtype = col_ref.dtype();

        let mut stat = Map::new();
        stat.insert("null_count".into(), Value::from(null_count as u64));
        if let Some(nu) = n_unique {
            stat.insert("n_unique".into(), Value::from(nu as u64));
        } else {
            stat.insert("n_unique".into(), Value::Null);
        }

        // min / max / mean only meaningful for numeric / orderable dtypes.
        let is_numeric = dtype.is_primitive_numeric();
        let is_bool = matches!(dtype, DataType::Boolean);
        let is_string = matches!(dtype, DataType::String);
        if is_numeric || is_bool || is_string {
            stat.insert("min".into(), scalar_to_json(col_ref.min_reduce().ok()));
            stat.insert("max".into(), scalar_to_json(col_ref.max_reduce().ok()));
        } else {
            stat.insert("min".into(), Value::Null);
            stat.insert("max".into(), Value::Null);
        }
        if is_numeric {
            stat.insert("mean".into(), scalar_to_json(col_ref.mean_reduce().ok()));
        } else {
            stat.insert("mean".into(), Value::Null);
        }
        stat.insert("dtype".into(), Value::String(format!("{dtype:?}")));
        out.insert(name, Value::Object(stat));
    }

    print_json(&json!({
        "ok": true,
        "stats": out,
        "n_rows": df.height(),
    }))
}

pub fn count(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data count --domain X --key Y [--where col=val] ...";
    let (_d, _k, path, _conn, _row) = resolve_table(root, args, USAGE)?;
    let wheres = collect_wheres(args);

    if !path.exists() {
        return print_json(&json!({"ok": true, "rows": 0}));
    }

    if !wheres.is_empty() {
        let schema = scan_table(&path)?.collect_schema()?;
        let names = where_column_names(&wheres);
        ensure_columns_present(&schema, &names, "count --where")?;
    }
    let mut lf = scan_table(&path).context("scan parquet for count")?;
    if let Some(filter) = parse_wheres(&wheres)? {
        lf = lf.filter(filter);
    }
    let df = lf.collect().context("collect count DataFrame")?;
    print_json(&json!({"ok": true, "rows": df.height()}))
}

pub fn select(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data select --domain X --key Y [--columns c1,c2] [--where col=val] [--limit N] [--offset N] [--order-by col[:desc]]";
    let (_d, _k, path, _conn, _row) = resolve_table(root, args, USAGE)?;
    let wheres = collect_wheres(args);
    let columns = parse_columns(args);
    let limit = parse_usize_flag(args, "--limit")?;
    let offset = parse_usize_flag(args, "--offset")?;
    let order_by = find_flag(args, "--order-by").map(str::to_string);

    if !path.exists() {
        return print_json(&json!({"ok": true, "rows": [], "n_rows": 0}));
    }

    if !wheres.is_empty() || columns.is_some() {
        let schema = scan_table(&path)?.collect_schema()?;
        if !wheres.is_empty() {
            let names = where_column_names(&wheres);
            ensure_columns_present(&schema, &names, "select --where")?;
        }
        if let Some(cols) = columns.as_deref() {
            ensure_columns_present(&schema, cols, "select --columns")?;
        }
    }
    let mut lf = scan_table(&path).context("scan parquet for select")?;
    if let Some(filter) = parse_wheres(&wheres)? {
        lf = lf.filter(filter);
    }
    if let Some(cols) = columns.as_deref() {
        lf = lf.select(cols.iter().map(|c| col(c.as_str())).collect::<Vec<_>>());
    }
    if let Some(ord) = order_by.as_deref() {
        let (col_name, descending) = match ord.split_once(':') {
            Some((c, "desc")) => (c, true),
            Some((c, _)) => (c, false),
            None => (ord, false),
        };
        lf = lf.sort(
            [PlSmallStr::from_str(col_name)],
            SortMultipleOptions::default().with_order_descending(descending),
        );
    }
    if let Some(off) = offset {
        lf = lf.slice(off as i64, IdxSize::MAX);
    }
    if let Some(lim) = limit {
        lf = lf.limit(lim as IdxSize);
    }
    let df = lf.collect().context("collect select DataFrame")?;
    let rows = df_to_rows(&df)?;
    print_json(&json!({
        "ok": true,
        "rows": rows,
        "n_rows": df.height(),
    }))
}

// ----- row-write verbs ----------------------------------------------------

pub fn append(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data append --domain X --key Y --rows <json-array>";
    let (domain, table_key, path, conn, catalog_row) = resolve_table(root, args, USAGE)?;
    bail_if_archived(&catalog_row, &domain, &table_key)?;
    let raw = required_flag(args, "--rows", USAGE)?;
    let parsed: Value = serde_json::from_str(raw).with_context(|| {
        format!("--rows is not valid JSON; expected a JSON array of objects")
    })?;
    let arr = parsed
        .as_array()
        .with_context(|| "--rows must be a JSON array of objects".to_string())?;
    for r in arr {
        if !r.is_object() {
            bail!("--rows array contains a non-object element");
        }
    }
    let rows_added = arr.len();
    if rows_added == 0 {
        let current_row_count = catalog_row
            .get("row_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let current_bytes = catalog_row.get("bytes").and_then(Value::as_i64).unwrap_or(0);
        let current_hash = catalog_row
            .get("schema_hash")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return print_json(&json!({
            "ok": true,
            "rows_added": 0,
            "row_count": current_row_count,
            "bytes": current_bytes,
            "schema_hash": current_hash,
            "note": "no rows supplied; parquet untouched",
        }));
    }

    let df_new = rows_to_df(arr)?;
    let df = if path.exists() {
        concat(
            [scan_table(&path)?, df_new.lazy()],
            UnionArgs {
                parallel: true,
                rechunk: true,
                to_supertypes: true,
                diagonal: true,
                from_partitioned_ds: false,
                maintain_order: true,
            },
        )?
        .collect()
        .context("concat existing + new rows")?
    } else {
        df_new
    };

    commit_parquet(&path, df.clone())?;
    let now = now_rfc3339();
    let (row_count, bytes, hash) =
        refresh_catalog_after_write(&conn, &domain, &table_key, &df, &path, &now)?;

    print_json(&json!({
        "ok": true,
        "rows_added": rows_added,
        "row_count": row_count,
        "bytes": bytes,
        "schema_hash": hash,
    }))
}

pub fn update(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data update --domain X --key Y --where col=val --set \"c1=v1,c2=v2\"";
    let (domain, table_key, path, conn, catalog_row) = resolve_table(root, args, USAGE)?;
    bail_if_archived(&catalog_row, &domain, &table_key)?;
    let wheres = collect_wheres(args);
    if wheres.is_empty() {
        bail!("update requires at least one --where clause; refusing to update every row");
    }
    let set_raw = required_flag(args, "--set", USAGE)?;
    let pairs = parse_set_clauses(set_raw)?;

    if !path.exists() {
        bail!(
            "no parquet on disk for domain={domain} key={table_key}; nothing to update"
        );
    }

    {
        let schema = scan_table(&path)?.collect_schema()?;
        let where_names = where_column_names(&wheres);
        ensure_columns_present(&schema, &where_names, "update --where")?;
        let set_names: Vec<String> = pairs.iter().map(|(c, _)| c.clone()).collect();
        ensure_columns_present(&schema, &set_names, "update --set")?;
    }
    let cond = parse_wheres(&wheres)?.expect("non-empty --where list yields a predicate");

    // First compute how many rows match the predicate.
    let matched_df = scan_table(&path)?
        .filter(cond.clone())
        .collect()
        .context("count matched rows")?;
    let rows_matched = matched_df.height() as i64;

    let exprs: Vec<Expr> = pairs
        .into_iter()
        .map(|(c, v)| {
            when(cond.clone())
                .then(lit_for_set(&v))
                .otherwise(col(c.as_str()))
                .alias(c.as_str())
        })
        .collect();
    let df = scan_table(&path)?
        .with_columns(exprs)
        .collect()
        .context("apply --set assignments")?;

    commit_parquet(&path, df.clone())?;
    let now = now_rfc3339();
    let (row_count, bytes, hash) =
        refresh_catalog_after_write(&conn, &domain, &table_key, &df, &path, &now)?;

    print_json(&json!({
        "ok": true,
        "rows_matched": rows_matched,
        "row_count": row_count,
        "bytes": bytes,
        "schema_hash": hash,
    }))
}

pub fn delete_rows(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data delete-rows --domain X --key Y --where col=val";
    let (domain, table_key, path, conn, catalog_row) = resolve_table(root, args, USAGE)?;
    bail_if_archived(&catalog_row, &domain, &table_key)?;
    let wheres = collect_wheres(args);
    if wheres.is_empty() {
        bail!("delete-rows requires at least one --where clause; refusing to delete every row");
    }
    if !path.exists() {
        bail!(
            "no parquet on disk for domain={domain} key={table_key}; nothing to delete"
        );
    }
    {
        let schema = scan_table(&path)?.collect_schema()?;
        let names = where_column_names(&wheres);
        ensure_columns_present(&schema, &names, "delete-rows --where")?;
    }
    let cond = parse_wheres(&wheres)?.expect("non-empty --where list yields a predicate");

    let before = scan_table(&path)?
        .collect()
        .context("read parquet to count pre-delete rows")?;
    let before_n = before.height() as i64;
    let df = scan_table(&path)?
        .filter(cond.not())
        .collect()
        .context("apply delete-rows filter")?;
    let after_n = df.height() as i64;
    let rows_deleted = before_n - after_n;

    commit_parquet(&path, df.clone())?;
    let now = now_rfc3339();
    let (row_count, bytes, hash) =
        refresh_catalog_after_write(&conn, &domain, &table_key, &df, &path, &now)?;

    print_json(&json!({
        "ok": true,
        "rows_deleted": rows_deleted,
        "row_count": row_count,
        "bytes": bytes,
        "schema_hash": hash,
    }))
}

// ----- column-write verbs -------------------------------------------------

pub fn add_column(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data add-column --domain X --key Y --column N --dtype T [--default V]";
    let (domain, table_key, path, conn, catalog_row) = resolve_table(root, args, USAGE)?;
    bail_if_archived(&catalog_row, &domain, &table_key)?;
    let column = required_flag(args, "--column", USAGE)?.to_string();
    let dtype_raw = required_flag(args, "--dtype", USAGE)?;
    let default = find_flag(args, "--default");

    let dt = match dtype_raw {
        "i64" => DataType::Int64,
        "f64" => DataType::Float64,
        "bool" => DataType::Boolean,
        "str" | "string" => DataType::String,
        other => bail!("unsupported --dtype `{other}`; expected one of i64, f64, bool, str/string"),
    };

    if !path.exists() {
        bail!(
            "no parquet on disk for domain={domain} key={table_key}; \
             append rows first or use import to create the file"
        );
    }

    // Reject if column already exists.
    let mut lf_schema_check = scan_table(&path)?;
    let existing_schema = lf_schema_check.collect_schema()?;
    if existing_schema.iter_names().any(|n| n.as_str() == column) {
        bail!("column `{column}` already exists on domain={domain} key={table_key}");
    }

    let value_expr: Expr = match default {
        Some(v) => lit_for_set(v).cast(dt.clone()),
        None => lit(NULL).cast(dt.clone()),
    };

    let df = scan_table(&path)?
        .with_column(value_expr.alias(column.as_str()))
        .collect()
        .context("add-column collect")?;

    commit_parquet(&path, df.clone())?;
    let now = now_rfc3339();
    let (_row_count, bytes, hash) =
        refresh_catalog_after_write(&conn, &domain, &table_key, &df, &path, &now)?;

    print_json(&json!({
        "ok": true,
        "column": column,
        "dtype": format!("{dt:?}"),
        "schema_hash": hash,
        "bytes": bytes,
    }))
}

pub fn drop_column(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data drop-column --domain X --key Y --column N";
    let (domain, table_key, path, conn, catalog_row) = resolve_table(root, args, USAGE)?;
    bail_if_archived(&catalog_row, &domain, &table_key)?;
    let column = required_flag(args, "--column", USAGE)?.to_string();

    if !path.exists() {
        bail!(
            "no parquet on disk for domain={domain} key={table_key}; nothing to drop"
        );
    }

    let mut lf_schema_check = scan_table(&path)?;
    let existing_schema = lf_schema_check.collect_schema()?;
    if !existing_schema.iter_names().any(|n| n.as_str() == column) {
        bail!("column `{column}` does not exist on domain={domain} key={table_key}");
    }

    let selector = by_name([PlSmallStr::from_str(column.as_str())], true);
    let df = scan_table(&path)?
        .drop(selector)
        .collect()
        .context("drop-column collect")?;

    commit_parquet(&path, df.clone())?;
    let now = now_rfc3339();
    let (_row_count, bytes, hash) =
        refresh_catalog_after_write(&conn, &domain, &table_key, &df, &path, &now)?;

    print_json(&json!({
        "ok": true,
        "column": column,
        "schema_hash": hash,
        "bytes": bytes,
    }))
}

// ----- bridge verbs -------------------------------------------------------

pub fn import(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str =
        "ctox knowledge data import --domain X --key Y --from-file <path> [--mode <replace|append>]";
    let (domain, table_key, path, conn, catalog_row) = resolve_table(root, args, USAGE)?;
    bail_if_archived(&catalog_row, &domain, &table_key)?;
    let from_file = PathBuf::from(required_flag(args, "--from-file", USAGE)?);
    let mode = find_flag(args, "--mode").unwrap_or("replace");
    if mode != "replace" && mode != "append" {
        bail!("--mode must be `replace` or `append`, got `{mode}`");
    }
    if !from_file.exists() {
        bail!("source file does not exist: {}", from_file.display());
    }

    let ext = from_file
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let df_new = match ext.as_str() {
        "parquet" => {
            let pl = PlPath::new(&from_file.to_string_lossy());
            LazyFrame::scan_parquet(pl, ScanArgsParquet::default())?
                .collect()
                .context("read source parquet")?
        }
        "csv" => {
            let pl = PlPath::new(&from_file.to_string_lossy());
            LazyCsvReader::new(pl)
                .with_has_header(true)
                .finish()
                .context("open source CSV")?
                .collect()
                .context("read source CSV")?
        }
        "jsonl" | "ndjson" => {
            let pl = PlPath::new(&from_file.to_string_lossy());
            LazyJsonLineReader::new(pl)
                .finish()
                .context("open source NDJSON")?
                .collect()
                .context("read source NDJSON")?
        }
        "json" => {
            let f = File::open(&from_file)
                .with_context(|| format!("open source JSON {}", from_file.display()))?;
            JsonReader::new(f)
                .with_json_format(JsonFormat::Json)
                .finish()
                .context("read source JSON")?
        }
        other => bail!(
            "unsupported source extension `{other}` (expected parquet|csv|json|jsonl|ndjson)"
        ),
    };

    let rows_imported = df_new.height() as i64;

    let df = if mode == "append" && path.exists() {
        concat(
            [scan_table(&path)?, df_new.lazy()],
            UnionArgs {
                parallel: true,
                rechunk: true,
                to_supertypes: true,
                diagonal: true,
                from_partitioned_ds: false,
                maintain_order: true,
            },
        )?
        .collect()
        .context("concat existing + imported rows")?
    } else {
        df_new
    };

    commit_parquet(&path, df.clone())?;
    let now = now_rfc3339();
    let (row_count, bytes, hash) =
        refresh_catalog_after_write(&conn, &domain, &table_key, &df, &path, &now)?;

    print_json(&json!({
        "ok": true,
        "rows_imported": rows_imported,
        "mode": mode,
        "row_count": row_count,
        "bytes": bytes,
        "schema_hash": hash,
    }))
}

pub fn export(root: &Path, args: &[String]) -> Result<()> {
    const USAGE: &str = "ctox knowledge data export --domain X --key Y --to-file <path>";
    let (_domain, _key, path, _conn, _row) = resolve_table(root, args, USAGE)?;
    let to_file = PathBuf::from(required_flag(args, "--to-file", USAGE)?);

    if !path.exists() {
        bail!("no parquet on disk; cannot export an empty/never-written table");
    }

    let mut df = scan_table(&path)?
        .collect()
        .context("read parquet for export")?;
    let rows_exported = df.height() as i64;

    if let Some(parent) = to_file.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create export parent {}", parent.display()))?;
        }
    }

    let ext = to_file
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let tmp = to_file.with_extension(format!("{ext}.tmp"));

    {
        let f = File::create(&tmp)
            .with_context(|| format!("create export tmp {}", tmp.display()))?;
        match ext.as_str() {
            "parquet" => {
                ParquetWriter::new(f)
                    .with_compression(ParquetCompression::Zstd(None))
                    .with_statistics(StatisticsOptions::full())
                    .finish(&mut df)
                    .context("write parquet export")?;
            }
            "csv" => {
                CsvWriter::new(f)
                    .include_header(true)
                    .finish(&mut df)
                    .context("write csv export")?;
            }
            "jsonl" | "ndjson" => {
                JsonWriter::new(f)
                    .with_json_format(JsonFormat::JsonLines)
                    .finish(&mut df)
                    .context("write ndjson export")?;
            }
            "json" => {
                JsonWriter::new(f)
                    .with_json_format(JsonFormat::Json)
                    .finish(&mut df)
                    .context("write json export")?;
            }
            other => bail!(
                "unsupported export extension `{other}` (expected parquet|csv|json|jsonl|ndjson)"
            ),
        }
        File::open(&tmp)
            .with_context(|| format!("reopen tmp export for fsync {}", tmp.display()))?
            .sync_all()
            .with_context(|| format!("fsync tmp export {}", tmp.display()))?;
    }
    std::fs::rename(&tmp, &to_file)
        .with_context(|| format!("rename {} -> {}", tmp.display(), to_file.display()))?;

    let bytes = std::fs::metadata(&to_file).map(|m| m.len() as i64).unwrap_or(0);

    print_json(&json!({
        "ok": true,
        "rows_exported": rows_exported,
        "bytes": bytes,
    }))
}

// ----- internal -----------------------------------------------------------

/// Convert a `Scalar` aggregate result (min/max/mean) to a JSON value
/// best suited to the underlying datatype.
fn scalar_to_json(scalar: Option<Scalar>) -> Value {
    let Some(s) = scalar else { return Value::Null };
    any_value_to_json(s.value())
}

fn any_value_to_json(av: &AnyValue<'_>) -> Value {
    match av {
        AnyValue::Null => Value::Null,
        AnyValue::Boolean(b) => Value::Bool(*b),
        AnyValue::String(s) => Value::String((*s).to_string()),
        AnyValue::StringOwned(s) => Value::String(s.as_str().to_string()),
        AnyValue::Int8(v) => Value::from(*v as i64),
        AnyValue::Int16(v) => Value::from(*v as i64),
        AnyValue::Int32(v) => Value::from(*v as i64),
        AnyValue::Int64(v) => Value::from(*v),
        AnyValue::Int128(v) => Value::from(*v as f64),
        AnyValue::UInt8(v) => Value::from(*v as u64),
        AnyValue::UInt16(v) => Value::from(*v as u64),
        AnyValue::UInt32(v) => Value::from(*v as u64),
        AnyValue::UInt64(v) => Value::from(*v),
        AnyValue::UInt128(v) => Value::from(*v as f64),
        AnyValue::Float32(v) => serde_json::Number::from_f64(*v as f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        AnyValue::Float64(v) => serde_json::Number::from_f64(*v)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        // Fallback: use the Debug representation rather than crash.
        other => Value::String(format!("{other:?}")),
    }
}
