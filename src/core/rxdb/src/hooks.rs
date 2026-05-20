//! Hook-functions that can be extended by plugins.
//!
//! T1 re-design: upstream uses a literal JS object as a mutable global with
//! arrays of functions per key. The Rust counterpart is a `LazyLock<RwLock<HashMap>>`
//! keyed by the verbatim upstream hook-key strings. Sync and async hooks are
//! both supported (upstream has both `runPluginHooks` and `runAsyncPluginHooks`).

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};

use futures::Future;
use parking_lot::RwLock;
use serde_json::Value;

pub type SyncHookFn = Arc<dyn Fn(&mut Value) + Send + Sync>;
pub type AsyncHookFn = Arc<
    dyn for<'a> Fn(&'a mut Value) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> + Send + Sync,
>;

#[derive(Clone)]
pub enum Hook {
    Sync(SyncHookFn),
    Async(AsyncHookFn),
}

// ref: rxdb/src/hooks.ts:5-103
// The HOOKS object. Keys are preserved verbatim from upstream.
const HOOK_KEYS: &[&str] = &[
    "preAddRxPlugin",
    "preCreateRxDatabase",
    "createRxDatabase",
    "preCreateRxCollection",
    "createRxCollection",
    "createRxState",
    "postCloseRxCollection",
    "postRemoveRxCollection",
    "preCreateRxSchema",
    "createRxSchema",
    "prePrepareRxQuery",
    "preCreateRxQuery",
    "prePrepareQuery",
    "createRxDocument",
    "postCreateRxDocument",
    "preCreateRxStorageInstance",
    "preStorageWrite",
    "preMigrateDocument",
    "postMigrateDocument",
    "preCloseRxDatabase",
    "postRemoveRxDatabase",
    "postCleanup",
    "preReplicationMasterWrite",
    "preReplicationMasterWriteDocumentsHandle",
];

pub static HOOKS: LazyLock<RwLock<HashMap<&'static str, Vec<Hook>>>> = LazyLock::new(|| {
    let mut m: HashMap<&'static str, Vec<Hook>> = HashMap::new();
    for k in HOOK_KEYS {
        m.insert(*k, Vec::new());
    }
    RwLock::new(m)
});

// ref: rxdb/src/hooks.ts:105-109
pub fn run_plugin_hooks(hook_key: &str, obj: &mut Value) {
    let hooks = HOOKS.read();
    if let Some(list) = hooks.get(hook_key) {
        if !list.is_empty() {
            for h in list.iter() {
                if let Hook::Sync(f) = h {
                    f(obj);
                }
            }
        }
    }
}

// ref: rxdb/src/hooks.ts:112-121
// We do intentionally not run the hooks in parallel
// because that makes stuff unpredictable and we use runAsyncPluginHooks()
// only in places that are not that relevant for performance.
pub async fn run_async_plugin_hooks(hook_key: &str, obj: &mut Value) {
    let hook_list: Vec<Hook> = {
        let hooks = HOOKS.read();
        hooks.get(hook_key).cloned().unwrap_or_default()
    };
    for h in hook_list {
        match h {
            Hook::Async(f) => f(obj).await,
            Hook::Sync(f) => f(obj),
        }
    }
}

// ref: rxdb/src/hooks.ts:123-128
// used in tests to remove hooks
pub fn clear_hook(hook_type: &str, target: &Hook) {
    let mut hooks = HOOKS.write();
    if let Some(list) = hooks.get_mut(hook_type) {
        list.retain(|h| !hook_ptr_eq(h, target));
    }
}

fn hook_ptr_eq(a: &Hook, b: &Hook) -> bool {
    match (a, b) {
        (Hook::Sync(x), Hook::Sync(y)) => Arc::ptr_eq(x, y),
        (Hook::Async(x), Hook::Async(y)) => Arc::ptr_eq(x, y),
        _ => false,
    }
}

/// Register a hook for `hook_key`. Pushed to the back (equivalent to `after`).
pub fn push_hook(hook_key: &'static str, hook: Hook) {
    let mut hooks = HOOKS.write();
    if let Some(list) = hooks.get_mut(hook_key) {
        list.push(hook);
    }
}

/// Register a hook for `hook_key` at the front (equivalent to `before`).
pub fn unshift_hook(hook_key: &'static str, hook: Hook) {
    let mut hooks = HOOKS.write();
    if let Some(list) = hooks.get_mut(hook_key) {
        list.insert(0, hook);
    }
}
