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
    // JavaScript has one Number type, so JSON `39` and `39.0` are equal after
    // a browser round-trip even though serde_json preserves their encodings.
    if let (Value::Number(an), Value::Number(bn)) = (a, b) {
        if let (Some(ai), Some(bi)) = (an.as_i64(), bn.as_i64()) {
            return ai == bi;
        }
        if let (Some(au), Some(bu)) = (an.as_u64(), bn.as_u64()) {
            return au == bu;
        }
        if let (Some(af), Some(bf)) = (an.as_f64(), bn.as_f64()) {
            return af == bf || (af.is_nan() && bf.is_nan());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::deep_equal;
    use serde_json::json;

    #[test]
    fn treats_integer_and_decimal_json_encodings_as_the_same_number() {
        assert!(deep_equal(&json!(39), &json!(39.0)));
        assert!(deep_equal(
            &json!({"revenue": 39, "nested": [129, 2.0]}),
            &json!({"revenue": 39.0, "nested": [129.0, 2]}),
        ));
        assert!(!deep_equal(&json!(39), &json!(39.5)));
    }
}
