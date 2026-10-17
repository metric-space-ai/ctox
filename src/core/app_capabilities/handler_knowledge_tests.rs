//! The real Knowledge consumers must retain the native CAP-1 route.
use super::handler_calls::{app_calls, missing_calls, registered_paths};
use super::CapabilityRegistry;
#[test]
fn knowledge_app_consumers_keep_their_native_capability() {
    let calls: Vec<_> = app_calls().into_iter().filter(|call| call["path"].as_str().is_some_and(|path| path.starts_with("knowledge."))).collect();
    assert!(!calls.is_empty(), "real Knowledge consumers must remain covered");
    let missing = missing_calls(&calls, &registered_paths(CapabilityRegistry::cap1()));
    assert!(missing.is_empty(), "Knowledge consumer regression:\n{}", missing.join("\n"));
}
