//! Deep-equality on JSON values.
//!
//! Upstream is a port of `fast-deep-equal` because that npm package does not
//! support ES modules. In Rust the equivalent on `serde_json::Value` is
//! mostly `==`, with the addition of NaN-equals-NaN semantics that match
//! upstream's last-line `return a !== a && b !== b`.

use serde_json::Value;

// ref: rxdb/src/plugins/utils/utils-object-deep-equal.ts:8-46
/// Copied from the fast-deep-equal package
/// because it does not support es modules and causes optimization bailouts.
pub fn deep_equal(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    // Arrays
    if let (Value::Array(aa), Value::Array(bb)) = (a, b) {
        if aa.len() != bb.len() {
            return false;
        }
        for (x, y) in aa.iter().zip(bb.iter()).rev() {
            if !deep_equal(x, y) {
                return false;
            }
        }
        return true;
    }
    // Objects
    if let (Value::Object(aa), Value::Object(bb)) = (a, b) {
        if aa.len() != bb.len() {
            return false;
        }
        for (k, v) in aa.iter() {
            match bb.get(k) {
                None => return false,
                Some(other) => {
                    if !deep_equal(v, other) {
                        return false;
                    }
                }
            }
        }
        return true;
    }
    // NaN == NaN special case from upstream: `a !== a && b !== b`.
    if let (Value::Number(an), Value::Number(bn)) = (a, b) {
        if let (Some(af), Some(bf)) = (an.as_f64(), bn.as_f64()) {
            return af.is_nan() && bf.is_nan();
        }
    }
    false
}
