//! Miscellaneous utilities.
//!
//! T3 deviations: `nameFunction` and `customFetchWithFixedHeaders` are
//! JS-runtime-specific and have no clean Rust counterpart; they are omitted.

use crate::rx_error::{new_rx_error, RxResult};

// ref: rxdb/src/plugins/utils/utils-other.ts:1-3
pub fn run_x_times(x_times: usize, mut f: impl FnMut(usize)) {
    for idx in 0..x_times {
        f(idx);
    }
}

// ref: rxdb/src/plugins/utils/utils-other.ts:5-13
pub fn ensure_not_falsy<T>(value: Option<T>, message: Option<&str>) -> RxResult<T> {
    match value {
        Some(v) => Ok(v),
        None => Err(new_rx_error(
            "UTL3",
            Some(serde_json::json!({
                "message": format!("ensureNotFalsy() is falsy: {}", message.unwrap_or(""))
            })),
        )),
    }
}

// ref: rxdb/src/plugins/utils/utils-other.ts:15-20
pub fn ensure_integer(value: f64) -> RxResult<i64> {
    if value.fract() != 0.0 || !value.is_finite() {
        return Err(new_rx_error(
            "UTL4",
            Some(serde_json::json!({ "message": "ensureInteger() is falsy" })),
        ));
    }
    Ok(value as i64)
}

// ref: rxdb/src/plugins/utils/utils-other.ts:22-32
/// Default args for `shareReplay()`. RxJS-specific in upstream; preserved
/// here as constants for plugins that pass them through `rxjs_compat`.
pub const RXJS_SHARE_REPLAY_BUFFER_SIZE: usize = 1;
pub const RXJS_SHARE_REPLAY_REF_COUNT: bool = true;

// ref: rxdb/src/plugins/utils/utils-other.ts:39-43
// `nameFunction(name, body)` — upstream dynamically renames a function for
// stack traces. Not portable to Rust; omitted.

// ref: rxdb/src/plugins/utils/utils-other.ts:46-58
// `customFetchWithFixedHeaders(headers)` — wraps the JS `fetch` API.
// CTOX uses its own HTTP layer; omitted from this port.
