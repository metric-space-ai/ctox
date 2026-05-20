//! How plugins are added to RxDB.
//!
//! Upstream uses prototype manipulation on JS classes (`plugin.prototypes`).
//! T1 re-design: Rust has no prototype chain, so the prototype-mutation path
//! is dropped. Plugins instead extend types via trait extensions in their own
//! crates. The remaining contract — `init`, `hooks`, `overwritable`, dedup by
//! name — is preserved.

use std::collections::HashSet;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use serde_json::json;

use crate::hooks::run_plugin_hooks;
use crate::rx_error::{new_rx_error, RxError};

/// RxDB plugin contract.
///
/// Upstream `plugin.rxdb: true` tag is not required: implementing this trait
/// is the Rust equivalent.
pub trait RxPlugin: Send + Sync {
    /// Unique plugin name. Must not collide with another registered plugin.
    fn name(&self) -> &str;
    /// One-time initialization. Called once when the plugin is registered.
    fn init(&self) {}
    /// Apply hook contributions to the global HOOKS registry.
    fn install_hooks(&self) {}
    /// Apply overwritable overrides to the OVERWRITABLE singleton.
    fn install_overwritable(&self) {}
}

// ref: rxdb/src/plugin.ts:43-44
// ADDED_PLUGINS: Set<RxPlugin> and ADDED_PLUGIN_NAMES: Set<string>
static ADDED_PLUGINS: LazyLock<Mutex<Vec<Arc<dyn RxPlugin>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ADDED_PLUGIN_NAMES: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

// ref: rxdb/src/plugin.ts:46-112
/// Add a plugin to the RxDB library.
/// Plugins are added globally and cannot be removed.
pub fn add_rx_plugin(plugin: Arc<dyn RxPlugin>) -> Result<(), RxError> {
    // ref: rxdb/src/plugin.ts:51
    // runPluginHooks('preAddRxPlugin', { plugin, plugins: ADDED_PLUGINS });
    let mut payload = {
        let plugins = ADDED_PLUGINS.lock();
        json!({
            "plugin": plugin.name(),
            "plugins": plugins.iter().map(|p| p.name().to_string()).collect::<Vec<_>>(),
        })
    };
    run_plugin_hooks("preAddRxPlugin", &mut payload);

    // ref: rxdb/src/plugin.ts:54-68
    {
        let mut plugins = ADDED_PLUGINS.lock();
        let mut names = ADDED_PLUGIN_NAMES.lock();
        // do nothing if added before (Arc identity)
        if plugins.iter().any(|p| Arc::ptr_eq(p, &plugin)) {
            return Ok(());
        }
        // ensure no other plugin with the same name was already added
        if names.contains(plugin.name()) {
            return Err(new_rx_error(
                "PL3",
                Some(json!({
                    "name": plugin.name(),
                })),
            ));
        }
        plugins.push(Arc::clone(&plugin));
        names.insert(plugin.name().to_string());
    }

    // ref: rxdb/src/plugin.ts:70-77
    // Upstream: `if (!plugin.rxdb) throw newRxTypeError('PL1', { plugin });`
    // Replaced by trait bound: implementing `RxPlugin` is the tag. (T1 deviation.)

    // ref: rxdb/src/plugin.ts:80-82
    plugin.init();

    // ref: rxdb/src/plugin.ts:84-91
    // prototype-overwrites — NOT SUPPORTED IN RUST (no prototype chain).
    // Plugins extend types via trait extensions in their own crates. (T1 deviation.)

    // ref: rxdb/src/plugin.ts:93-98
    plugin.install_overwritable();

    // ref: rxdb/src/plugin.ts:100-111
    plugin.install_hooks();

    Ok(())
}

/// Snapshot of currently registered plugins, by name.
pub fn registered_plugin_names() -> Vec<String> {
    ADDED_PLUGINS
        .lock()
        .iter()
        .map(|p| p.name().to_string())
        .collect()
}
