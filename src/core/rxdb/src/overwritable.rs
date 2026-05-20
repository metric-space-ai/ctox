//! Functions that can or should be overwritten by plugins.
//!
//! IMPORTANT (verbatim from upstream): Do not import any big stuff from RxDB here!
//! An 'overwritable' can be used inside WebWorkers for RxStorage only,
//! and we do not want to have the full RxDB lib bundled in them.
//!
//! T1 re-design: upstream is a mutable singleton object that plugins mutate via
//! `Object.assign`. The Rust counterpart is an `ArcSwap<Overwritable>` of an
//! owned struct of `Arc<dyn Fn>` callbacks, swapped atomically when plugins install
//! overrides.

use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use serde_json::Value;

// ref: rxdb/src/overwritable.ts:10-41
pub struct Overwritable {
    /// if this method is overwritten with one
    /// that returns true, we do additional checks
    /// which help the developer but have bad performance
    pub is_dev_mode: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Deep freezes an object when in dev-mode.
    /// Deep-Freezing has the same performance as deep-cloning, so we only do that in dev-mode.
    pub deep_freeze_when_dev_mode: Arc<dyn Fn(Value) -> Value + Send + Sync>,
    /// overwritten to map error-codes to text-messages
    pub tunnel_error_message: Arc<dyn Fn(&str) -> String + Send + Sync>,
}

impl std::fmt::Debug for Overwritable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overwritable")
            .field("is_dev_mode", &"<fn>")
            .field("deep_freeze_when_dev_mode", &"<fn>")
            .field("tunnel_error_message", &"<fn>")
            .finish()
    }
}

impl Default for Overwritable {
    fn default() -> Self {
        Self {
            is_dev_mode: Arc::new(|| false),
            deep_freeze_when_dev_mode: Arc::new(|x| x),
            tunnel_error_message: Arc::new(default_tunnel_error_message),
        }
    }
}

// ref: rxdb/src/overwritable.ts:33-40
fn default_tunnel_error_message(message: &str) -> String {
    format!(
        "\n        RxDB Error-Code: {message}.\n        \
         Hint: Error messages are not included in RxDB core to reduce build size.\n        \
         To show the full error messages and to ensure that you do not make any mistakes when using RxDB,\n        \
         use the dev-mode plugin when you are in development mode: https://rxdb.info/dev-mode.html?console=error\n        "
    )
}

pub static OVERWRITABLE: LazyLock<ArcSwap<Overwritable>> =
    LazyLock::new(|| ArcSwap::from_pointee(Overwritable::default()));

/// Replace the current overwritable. Equivalent to upstream `Object.assign(overwritable, plugin.overwritable)`
/// but atomic: builds a new `Overwritable` from a builder closure that receives the current value.
pub fn replace_overwritable<F>(builder: F)
where
    F: FnOnce(&Overwritable) -> Overwritable,
{
    let current = OVERWRITABLE.load();
    let new = builder(&current);
    OVERWRITABLE.store(Arc::new(new));
}
