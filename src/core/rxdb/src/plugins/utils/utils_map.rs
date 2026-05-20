//! `Map`/`HashMap` helpers.

use std::collections::HashMap;
use std::hash::Hash;

use crate::rx_error::{RxError, RxResult};

// ref: rxdb/src/plugins/utils/utils-map.ts:1-7
pub fn get_from_map_or_throw<'a, K: Eq + Hash, V>(
    map: &'a HashMap<K, V>,
    key: &K,
) -> RxResult<&'a V>
where
    K: std::fmt::Debug,
{
    map.get(key)
        .ok_or_else(|| {
            // Upstream uses `throw new Error('missing value from map ' + key)`.
            // We map to a generic non-coded RxError variant via newRxError with a synthetic code.
            crate::rx_error::new_rx_error(
                "UTL1",
                Some(serde_json::json!({ "message": format!("missing value from map {:?}", key) })),
            )
        })
        .map_err(|e: RxError| e)
}

// ref: rxdb/src/plugins/utils/utils-map.ts:9-23
pub fn get_from_map_or_create<K: Eq + Hash + Clone, V: Clone>(
    map: &mut HashMap<K, V>,
    index: &K,
    creator: impl FnOnce() -> V,
    if_was_there: Option<&dyn Fn(&V)>,
) -> V {
    if let Some(value) = map.get(index) {
        if let Some(cb) = if_was_there {
            cb(value);
        }
        value.clone()
    } else {
        let value = creator();
        map.insert(index.clone(), value.clone());
        value
    }
}
