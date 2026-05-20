//! A "global" object some plugins use as a shared scratch space.

use std::collections::HashMap;
use std::sync::LazyLock;

use parking_lot::RwLock;
use serde_json::Value;

// ref: rxdb/src/plugins/utils/utils-global.ts:5
/// Mutable global key-value bag for plugins. Upstream is `RXDB_UTILS_GLOBAL: any = {}`.
/// In Rust we expose it as a `LazyLock<RwLock<HashMap<String, Value>>>`.
pub static RXDB_UTILS_GLOBAL: LazyLock<RwLock<HashMap<String, Value>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
