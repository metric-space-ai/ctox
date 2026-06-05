// Origin: CTOX
// License: AGPL-3.0-only
//
// Phase 3 — the per-protocol native IoT agents. Each is a self-contained
// `IotAgent` (adapters.rs) implementation; the closed `gateway::IotAgentKind`
// dispatch (gateway.rs) constructs the right one, and `runtime.rs` supervises
// the spawned loops. No shared transport framework: MQTT is a vendored in-tree
// codec, HTTP reuses the existing ureq client, WebSocket reuses the forked
// tokio-tungstenite — see each module header.
pub(crate) mod http_native;
pub(crate) mod mqtt_native;
pub(crate) mod ws_native;
