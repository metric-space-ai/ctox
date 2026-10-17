//! E17.1: the composed native registry covers every app-handler capability call.
use super::handler_calls::{app_calls, missing_calls, registered_paths};
#[test]
fn app_handler_capability_calls_exist_in_native_registry() {
    let missing = missing_calls(&app_calls(), &registered_paths(super::cap3::registry()));
    assert!(
        missing.is_empty(),
        "unregistered app-handler capabilities:\n{}",
        missing.join("\n")
    );
}
