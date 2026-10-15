//! `<toolkit>.<action>` accepts the short and the full action name.
use ctox_toolhub::*;

#[test]
fn dotted_names_accept_short_and_full_action() {
    let hub = ToolHub::new(
        MemorySecretBackend::default(),
        std::sync::Arc::new(ctox_toolhub::http::UreqTransport::default())
    );
    for name in ["LINEAR_GET_VIEWER", "linear.get_viewer", "linear.GET_VIEWER", "linear.LINEAR_GET_VIEWER"] {
        let (_, spec) = hub.tool(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(spec.slug, "LINEAR_GET_VIEWER", "{name}");
    }
    assert!(hub.tool("linear.NOPE").is_err());
}
