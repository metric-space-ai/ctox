//! Port of `mingo/src/operators/_predicates.ts` together with the JSON-shape
//! adapted versions of helpers from `mingo/src/util.ts` that the predicates
//! depend on (`resolve`, `is_equal`, `ensure_array`, `flatten`, `compare`,
//! `type_of`).
//!
//! Upstream factors `_predicates.ts` apart from `util.ts`; we keep the
//! predicate functions here and place the JSON-adapted util helpers above them
//! in the same file so each predicate stays a one-line wrapper that is easy
//! to line up with its TS counterpart.

use std::cmp::Ordering;

use serde_json::Value;

use crate::util::mango::core::{Options, QueryPredicate};

// ---------------------------------------------------------------------------
// JSON-adapted util.ts helpers
// ---------------------------------------------------------------------------

// ref: mingo/src/util.ts:46-59
/// MongoDB sort comparison order for JSON-shaped values. Date / regexp / etc.
/// are unreachable through `serde_json::Value` so we collapse upstream's
/// 12-way table to the 6 cases that JSON values can land in.
fn sort_order(v: &Value) -> u8 {
    match v {
        Value::Null => 2,
        Value::Number(_) => 3,
        Value::String(_) => 4,
        Value::Object(_) => 6,
        Value::Array(_) => 7,
        Value::Bool(_) => 9,
    }
}

// ref: mingo/src/util.ts:202-208
/// Returns the lowercase type name for the value.
pub(crate) fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// ref: mingo/src/util.ts:209-214
pub(crate) fn is_string(v: &Value) -> bool {
    matches!(v, Value::String(_))
}

// ref: mingo/src/util.ts:212-213
pub(crate) fn is_number(v: &Value) -> bool {
    matches!(v, Value::Number(_))
}

// ref: mingo/src/util.ts:209
pub(crate) fn is_boolean(v: &Value) -> bool {
    matches!(v, Value::Bool(_))
}

// ref: mingo/src/util.ts:216
pub(crate) fn is_array(v: &Value) -> bool {
    matches!(v, Value::Array(_))
}

// ref: mingo/src/util.ts:217-221
pub(crate) fn is_object(v: &Value) -> bool {
    matches!(v, Value::Object(_))
}

// ref: mingo/src/util.ts:227
/// Upstream considers `null` and `undefined` nil. In JSON we have no
/// `undefined`, so `null` is the only nil. Missing keys surface as `None` at
/// the call site, not as `Value::Null`, but mingo's predicates were written
/// for `undefined` and JS-style absent fields — treat both the same.
pub(crate) fn is_nil(v: &Value) -> bool {
    matches!(v, Value::Null)
}

// ref: mingo/src/util.ts:228-229
#[allow(dead_code)]
pub(crate) fn truthy(v: &Value, strict: bool) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|x| x != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty() || strict,
        Value::Array(_) | Value::Object(_) => true,
    }
}

// ref: mingo/src/util.ts:230-234
#[allow(dead_code)]
pub(crate) fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

// ref: mingo/src/util.ts:236
pub(crate) fn ensure_array(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().collect(),
        _ => vec![v],
    }
}

// ref: mingo/src/util.ts:68-79
/// Compare function which adheres to MongoDB comparison order for the JSON
/// subset.
pub(crate) fn compare(a: &Value, b: &Value) -> Ordering {
    let u = sort_order(a);
    let v = sort_order(b);
    if u != v {
        return u.cmp(&v);
    }
    if is_equal(a, b) {
        return Ordering::Equal;
    }
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64().unwrap_or(0.0);
            let yf = y.as_f64().unwrap_or(0.0);
            xf.partial_cmp(&yf).unwrap_or(Ordering::Equal)
        }
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Null, Value::Null) => Ordering::Equal,
        // Arrays and objects fall through: upstream returns 0 when comparing
        // types where ordering does not make sense.
        _ => Ordering::Equal,
    }
}

// ref: mingo/src/util.ts:390-415
/// Determine whether two values are the same or strictly equivalent. For JSON
/// values this reduces to structural equality (`PartialEq`), with the
/// additional handling for number equivalence that crosses int/float
/// representation boundaries.
pub(crate) fn is_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(xf), Some(yf)) => xf == yf,
            _ => x == y,
        },
        _ => a == b,
    }
}

// ref: mingo/src/util.ts:342-355
/// Flatten the array to the given depth. Matches upstream `flatten(xs, depth)`
/// including its negative-depth convention (`< 0` means "flatten fully").
pub(crate) fn flatten(xs: &[Value], depth: i32) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    fn rec(ys: &[Value], n: i32, out: &mut Vec<Value>) {
        for item in ys {
            if let Value::Array(a) = item {
                if n > 0 || n < 0 {
                    rec(a, std::cmp::max(-1, n - 1), out);
                    continue;
                }
            }
            out.push(item.clone());
        }
    }
    rec(xs, depth, &mut out);
    out
}

// ref: mingo/src/util.ts:847-855
const OPERATOR_NAME_PATTERN: &str = r"^\$[a-zA-Z0-9_]+$";

// ref: mingo/src/util.ts:853-855
pub(crate) fn is_operator(name: &str) -> bool {
    // Compiled once per call; the regex is trivial so the cost is negligible
    // and we avoid pulling `once_cell` into the dep graph.
    regex::Regex::new(OPERATOR_NAME_PATTERN)
        .ok()
        .map(|r| r.is_match(name))
        .unwrap_or(false)
}

// ref: mingo/src/util.ts:862-884
/// Simplify expression for easy evaluation with query operators map.
pub(crate) fn normalize(expr: &Value) -> Value {
    // Upstream treats RegExp instances specially; JSON has no native regex, so
    // the only "scalar" wrap-up case is a non-operator-shaped object turning
    // into `{ $eq: <obj> }`.
    let is_scalar = !matches!(expr, Value::Object(_) | Value::Array(_));
    if is_scalar {
        return serde_json::json!({ "$eq": expr });
    }

    if let Value::Object(map) = expr {
        let any_operator = map.keys().any(|k| is_operator(k));
        if !any_operator {
            return serde_json::json!({ "$eq": expr });
        }
        // ensure valid regex: upstream rebuilds `$regex` with `$options`.
        // Our $regex predicate consumes the string + options directly, so we
        // just inline `$options` value into a normalized `{ $regex: { pattern,
        // options } }` shape that the operator factory understands.
        if map.contains_key("$regex") {
            let pattern = map.get("$regex").cloned().unwrap_or(Value::Null);
            let options = map.get("$options").cloned().unwrap_or(Value::Null);
            let mut new_expr = map.clone();
            new_expr.insert(
                "$regex".to_string(),
                serde_json::json!({ "pattern": pattern, "options": options }),
            );
            new_expr.remove("$options");
            return Value::Object(new_expr);
        }
    }
    expr.clone()
}

// ref: mingo/src/util.ts:581-583
fn get_value<'a>(obj: &'a Value, key: &str) -> Option<&'a Value> {
    match obj {
        Value::Object(m) => m.get(key),
        Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    }
}

// ref: mingo/src/util.ts:591-595
fn unwrap(arr: Vec<Value>, mut depth: usize) -> Vec<Value> {
    let mut cur = arr;
    while depth > 0 && cur.len() == 1 {
        let only = cur.into_iter().next().unwrap();
        match only {
            Value::Array(inner) => {
                cur = inner;
            }
            other => {
                cur = vec![other];
                break;
            }
        }
        depth -= 1;
    }
    cur
}

// ref: mingo/src/util.ts:615-651
/// Resolve the value of the field (dot separated) on the given object.
pub(crate) fn resolve(obj: &Value, selector: &str, unwrap_array: bool) -> Option<Value> {
    let path: Vec<&str> = selector.split('.').collect();
    let mut depth: usize = 0;

    fn resolve2(o: &Value, path: &[&str], depth: &mut usize) -> Option<Value> {
        let mut value: Value = o.clone();
        let mut i = 0;
        while i < path.len() {
            let field = path[i];
            let is_text = field.parse::<usize>().is_err();
            if is_text && matches!(value, Value::Array(_)) {
                if i == 0 && *depth > 0 {
                    break;
                }
                *depth += 1;
                let subpath = &path[i..];
                if let Value::Array(arr) = value {
                    let mut acc: Vec<Value> = Vec::new();
                    for item in &arr {
                        if let Some(v) = resolve2(item, subpath, depth) {
                            acc.push(v);
                        }
                    }
                    value = Value::Array(acc);
                }
                return Some(value);
            } else {
                match get_value(&value, field) {
                    Some(v) => value = v.clone(),
                    None => return None,
                }
            }
            i += 1;
        }
        Some(value)
    }

    // upstream `isScalar(obj)` returns the value itself; for JSON, the only
    // scalars are non-object/non-array values, in which case the selector
    // cannot resolve to anything except the value itself.
    let is_scalar = !matches!(obj, Value::Object(_) | Value::Array(_));
    let res = if is_scalar {
        Some(obj.clone())
    } else {
        resolve2(obj, &path, &mut depth)
    };

    match res {
        Some(Value::Array(arr)) if unwrap_array => {
            let unwrapped = unwrap(arr, depth);
            Some(Value::Array(unwrapped))
        }
        other => other,
    }
}

// ref: mingo/src/util.ts:121-126
fn intersection_nonempty(a: &[Value], b: &[Value]) -> bool {
    for x in a {
        for y in b {
            if is_equal(x, y) {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Predicate factory
// ---------------------------------------------------------------------------

// ref: mingo/src/operators/_predicates.ts:51-62
/// Returns a query operator created from the predicate.
///
/// Upstream the inner function consumes `(obj: AnyObject) => boolean`; we
/// return `Arc<dyn Fn(&Value) -> bool>` so the closure can travel into the
/// compiled `Query` without lifetime gymnastics.
pub(crate) fn create_query_operator(
    predicate: fn(lhs: Option<&Value>, rhs: &Value, depth: usize) -> bool,
) -> impl Fn(&str, &Value, &Options) -> QueryPredicate {
    move |selector: &str, value: &Value, _options: &Options| -> QueryPredicate {
        // mingo: `depth = Math.max(1, selector.split('.').length - 1)`
        let depth = std::cmp::max(1, selector.split('.').count().saturating_sub(1));
        let selector = selector.to_string();
        let value = value.clone();
        std::sync::Arc::new(move |obj: &Value| -> bool {
            // mingo passes `{ unwrapArray: true }` to `resolve`.
            let lhs = resolve(obj, &selector, true);
            predicate(lhs.as_ref(), &value, depth)
        })
    }
}

// ---------------------------------------------------------------------------
// Comparison predicates (port of `mingo/src/operators/_predicates.ts`)
// ---------------------------------------------------------------------------

// ref: mingo/src/operators/_predicates.ts:85-101
/// Checks that two values are equal.
pub(crate) fn pred_eq(a: Option<&Value>, b: &Value, depth: usize) -> bool {
    // upstream:
    //   if (isEqual(a, b)) return true;
    //   if (isNil(a) && isNil(b)) return true;
    //   if (isArray(a)) return a.some(...) || flatten(a, depth).some(...);
    //   return false;
    match a {
        Some(av) => {
            if is_equal(av, b) {
                return true;
            }
            // upstream: isNil(a) && isNil(b) — in JSON, nil means `null`.
            if is_nil(av) && is_nil(b) {
                return true;
            }
            if let Value::Array(arr) = av {
                if arr.iter().any(|v| is_equal(v, b)) {
                    return true;
                }
                let flat = flatten(arr, depth as i32);
                if flat.iter().any(|v| is_equal(v, b)) {
                    return true;
                }
            }
            false
        }
        // upstream: `a === undefined`. isNil(undefined) is true, so
        // `isNil(a) && isNil(b)` matches when b is null.
        None => is_nil(b),
    }
}

// ref: mingo/src/operators/_predicates.ts:110-112
pub(crate) fn pred_ne(a: Option<&Value>, b: &Value, depth: usize) -> bool {
    !pred_eq(a, b, depth)
}

// ref: mingo/src/operators/_predicates.ts:121-126
pub(crate) fn pred_in(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    // Upstream: $in(a: Any[], b: Any[]). Resolved `a` may be undefined for a
    // missing field — upstream then matches if b contains a literal `null`.
    let b_arr: &[Value] = match b {
        Value::Array(arr) => arr.as_slice(),
        _ => return false,
    };
    let av = match a {
        Some(v) => v,
        None => {
            return b_arr.iter().any(|v| matches!(v, Value::Null));
        }
    };
    if is_nil(av) {
        return b_arr.iter().any(|v| matches!(v, Value::Null));
    }
    // ensureArray equivalent: array stays as-is, scalar wraps to a one-elem
    // vec. Materialize to an owned Vec to keep the slice lifetime simple.
    let lhs: Vec<Value> = match av {
        Value::Array(arr) => arr.clone(),
        other => vec![other.clone()],
    };
    intersection_nonempty(&lhs, b_arr)
}

// ref: mingo/src/operators/_predicates.ts:135-137
pub(crate) fn pred_nin(a: Option<&Value>, b: &Value, depth: usize) -> bool {
    !pred_in(a, b, depth)
}

// ref: mingo/src/operators/_predicates.ts:146-148
pub(crate) fn pred_lt(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    compare_some(a, b, |o| o == Ordering::Less)
}

// ref: mingo/src/operators/_predicates.ts:157-159
pub(crate) fn pred_lte(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    compare_some(a, b, |o| o != Ordering::Greater)
}

// ref: mingo/src/operators/_predicates.ts:168-170
pub(crate) fn pred_gt(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    compare_some(a, b, |o| o == Ordering::Greater)
}

// ref: mingo/src/operators/_predicates.ts:179-181
pub(crate) fn pred_gte(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    compare_some(a, b, |o| o != Ordering::Less)
}

// ref: mingo/src/operators/_predicates.ts:371-373
/// Upstream:
/// ```js
/// function compare(a, b, f) { return ensureArray(a).some(x => typeOf(x) === typeOf(b) && f(x, b)); }
/// ```
fn compare_some(a: Option<&Value>, b: &Value, f: impl Fn(Ordering) -> bool) -> bool {
    let av = match a {
        Some(v) => v,
        None => return false,
    };
    for x in ensure_array(av) {
        if type_of(x) == type_of(b) && f(compare(x, b)) {
            return true;
        }
    }
    false
}

// ref: mingo/src/operators/_predicates.ts:190-198
pub(crate) fn pred_mod(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    let av = match a {
        Some(v) => v,
        None => return false,
    };
    let b_arr = match b {
        Value::Array(arr) => arr,
        _ => return false,
    };
    if b_arr.len() != 2 {
        return false;
    }
    let divisor = match b_arr[0].as_f64() {
        Some(n) => n,
        None => return false,
    };
    let remainder = match b_arr[1].as_f64() {
        Some(n) => n,
        None => return false,
    };
    ensure_array(av).iter().any(|x| {
        x.as_f64()
            .map(|n| (n % divisor) == remainder)
            .unwrap_or(false)
    })
}

// ref: mingo/src/operators/_predicates.ts:207-212
pub(crate) fn pred_regex(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    // We accept either the normalized shape `{ pattern, options }` produced by
    // `normalize`, a literal string (treated as the pattern with no flags), or
    // an already-built regex represented as `{ "$regex": <pattern> }` upstream
    // would compile.
    let (pattern, options) = match b {
        Value::Object(m) => match (m.get("pattern"), m.get("options")) {
            (Some(p), opts) => (
                p.as_str().unwrap_or("").to_string(),
                opts.and_then(|v| v.as_str()).unwrap_or("").to_string(),
            ),
            _ => return false,
        },
        Value::String(s) => (s.clone(), String::new()),
        _ => return false,
    };
    let re = match build_regex(&pattern, &options) {
        Some(r) => r,
        None => return false,
    };
    let av = match a {
        Some(v) => v,
        None => return false,
    };
    let match_fn = |x: &Value| -> bool {
        if let Value::String(s) = x {
            re.is_match(s)
        } else {
            false
        }
    };
    let lhs = ensure_array(av);
    if lhs.iter().any(|v| match_fn(v)) {
        return true;
    }
    // flatten depth=1
    let lhs_owned: Vec<Value> = lhs.iter().map(|v| (*v).clone()).collect();
    let flat = flatten(&lhs_owned, 1);
    flat.iter().any(match_fn)
}

fn build_regex(pattern: &str, options: &str) -> Option<regex::Regex> {
    // Convert MongoDB-style flags (i, m, s, x) to inline regex flags. The
    // `regex` crate supports `(?i)`, `(?m)`, `(?s)` and `(?x)` inline.
    let mut prefix = String::new();
    if !options.is_empty() {
        prefix.push_str("(?");
        for c in options.chars() {
            match c {
                'i' | 'm' | 's' | 'x' => prefix.push(c),
                // ignore unsupported flags rather than failing the whole query
                _ => {}
            }
        }
        prefix.push(')');
    }
    let combined = format!("{prefix}{pattern}");
    regex::Regex::new(&combined).ok()
}

// ref: mingo/src/operators/_predicates.ts:257-263
pub(crate) fn pred_size(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    let av = match a {
        Some(v) => v,
        None => return false,
    };
    let arr = match av {
        Value::Array(arr) => arr,
        _ => return false,
    };
    let target = match b.as_u64() {
        Some(n) => n as usize,
        None => return false,
    };
    arr.len() == target
}

// ref: mingo/src/operators/_predicates.ts:265-267
pub(crate) fn is_non_boolean_operator(name: &str) -> bool {
    is_operator(name) && !matches!(name, "$and" | "$or" | "$nor")
}

// ref: mingo/src/operators/_predicates.ts:275-301
/// $elemMatch — must construct a sub-`Query` so this predicate is implemented
/// inside `array.rs` to avoid a module-level cycle. Re-exported here only for
/// the registration table.
pub(crate) fn pred_elem_match(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    let av = match a {
        Some(v) => v,
        None => return false,
    };
    let arr = match av {
        Value::Array(arr) if !arr.is_empty() => arr,
        _ => return false,
    };
    let b_obj = match b {
        Value::Object(_) => b,
        _ => return false,
    };

    // If every sub-key is a non-boolean operator, wrap with a synthetic
    // `temp` field exactly like upstream so that operators like `$gt`/`$lt`
    // can be applied to array elements directly.
    let keys_are_all_non_boolean = if let Value::Object(map) = b_obj {
        !map.is_empty() && map.keys().all(|k| is_non_boolean_operator(k))
    } else {
        false
    };

    let (criteria, wrap): (Value, bool) = if keys_are_all_non_boolean {
        (serde_json::json!({ "temp": b_obj }), true)
    } else {
        (b_obj.clone(), false)
    };

    // Build the sub-query. We use the public `Query::new` from the parent
    // module to share the operator wiring.
    let q = crate::util::mango::query::Query::new(&criteria);
    for item in arr {
        let candidate = if wrap {
            serde_json::json!({ "temp": item })
        } else {
            item.clone()
        };
        if q.test(&candidate) {
            return true;
        }
    }
    false
}

// ref: mingo/src/operators/_predicates.ts:306-340
/// Mapping of `$type` argument to a JSON-value-shape predicate.
///
/// Upstream's table includes `date`, `regexp`, `function`, and typed arrays
/// that cannot appear in `serde_json::Value`. Those entries always return
/// `false`, which is documented at module level.
pub(crate) fn type_predicate(b: &Value) -> Option<fn(&Value) -> bool> {
    // String forms: "array", "boolean"/"bool", "number"/"int"/"long"/"double"/
    //               "decimal", "null", "object", "string"
    if let Some(s) = b.as_str() {
        return match s {
            "array" => Some(is_array),
            "boolean" | "bool" => Some(is_boolean),
            "number" | "int" | "long" | "double" | "decimal" => Some(is_number),
            "null" => Some(is_nil),
            "object" => Some(is_object),
            "string" => Some(is_string),
            // Unsupported in JSON-shape: date, regexp, regex, undefined,
            // function. Return None so $type yields false rather than the
            // function pointer that would throw at runtime upstream.
            _ => None,
        };
    }
    // Numeric (BSON) forms
    if let Some(n) = b.as_i64() {
        return match n {
            1 | 16 | 18 | 19 => Some(is_number),
            2 => Some(is_string),
            3 => Some(is_object),
            4 => Some(is_array),
            6 | 10 => Some(is_nil),
            8 => Some(is_boolean),
            _ => None,
        };
    }
    None
}

// ref: mingo/src/operators/_predicates.ts:349-352
fn compare_type(a: &Value, b: &Value) -> bool {
    match type_predicate(b) {
        Some(f) => f(a),
        None => false,
    }
}

// ref: mingo/src/operators/_predicates.ts:361-369
pub(crate) fn pred_type(a: Option<&Value>, b: &Value, _depth: usize) -> bool {
    let av = match a {
        Some(v) => v,
        None => return false,
    };
    if let Value::Array(types) = b {
        types.iter().any(|t| compare_type(av, t))
    } else {
        compare_type(av, b)
    }
}
