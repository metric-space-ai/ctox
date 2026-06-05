// Origin: CTOX
// License: AGPL-3.0-only
//
// Phase 3 — IoT protocol-agent gateway: the closed-set registry + dispatch that
// turns an `iot_agents.kind` string into a concrete boxed `IotAgent`. This is
// the IoT analogue of `communication::gateway`: a closed enum
// (`gateway.rs:25` CommunicationAdapterKind) whose `spec()` advertises the
// runtime_env keys (`gateway.rs:109`) and a constructor that builds the right
// native adapter (`gateway.rs:448` external_adapter_for_channel). Adding a
// member here is a deliberate core edit, never a data-driven plugin lookup.
//
// HARD RULES honored here:
//   * native Rust only; the dispatch hands back the in-tree agents (vendored
//     MQTT codec / ureq HTTP / forked tokio-tungstenite WS) — no framework dep.
//   * the only configuration this layer reads is the opaque `data` JSON from the
//     `iot_agents` row plus secrets resolved INSIDE each agent via
//     `runtime_env::env_or_config` — never `std::env`. The keys an agent may
//     consult are declared in its `IotAgentSpec::runtime_env_keys` so the
//     surface is auditable, mirroring communication's spec table.
//   * agents talk to DEVICES; there is no HTTP data bridge to the browser here.

use crate::iot::adapters::{AgentContext, IotAgent, IotAgentKind};
use crate::iot::agents::{http_native::HttpAgent, mqtt_native::MqttAgent, ws_native::WsAgent};
use crate::iot::Result;

// The closed protocol-agent kind itself lives in `adapters.rs` (it is the type
// every agent + the value-processing base layer compile against). The gateway
// builds its registry/dispatch around that same kind, exactly like
// `communication::gateway` does around `CommunicationAdapterKind`.

/// Native-only backend marker, mirroring `CommunicationAdapterBackend::NativeRust`
/// (communication/gateway.rs:20). Every curated IoT agent is bare-metal Rust;
/// the enum exists so a future non-native bridge would be an explicit core edit.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IotAgentBackend {
    NativeRust,
}

/// Auditable spec for one protocol-agent kind: its backend and the closed set of
/// runtime_env / secret-store keys the agent may consult through
/// `runtime_env::env_or_config`. ref: communication/gateway.rs:35 (CommunicationAdapterSpec).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IotAgentSpec {
    pub kind: IotAgentKind,
    pub backend: IotAgentBackend,
    pub runtime_env_keys: &'static [&'static str],
}

// Per-kind runtime_env / secret-store keys. These are the keys the matching
// agent resolves via `runtime_env::env_or_config(root, KEY)` (the agent reads a
// `*Key` field out of its `data` config and looks it up here). Listing them is
// the audit surface, exactly like communication's *_RUNTIME_ENV_KEYS tables.
const MQTT_RUNTIME_ENV_KEYS: &[&str] = &["CTO_IOT_MQTT_USERNAME", "CTO_IOT_MQTT_PASSWORD"];
const HTTP_RUNTIME_ENV_KEYS: &[&str] = &["CTO_IOT_HTTP_AUTH_HEADER"];
const WS_RUNTIME_ENV_KEYS: &[&str] = &["CTO_IOT_WS_AUTH_HEADER"];

impl IotAgentKind {
    /// ref: communication/gateway.rs:111 (CommunicationAdapterKind::spec).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spec(self) -> IotAgentSpec {
        match self {
            Self::Mqtt => IotAgentSpec {
                kind: self,
                backend: IotAgentBackend::NativeRust,
                runtime_env_keys: MQTT_RUNTIME_ENV_KEYS,
            },
            Self::Http => IotAgentSpec {
                kind: self,
                backend: IotAgentBackend::NativeRust,
                runtime_env_keys: HTTP_RUNTIME_ENV_KEYS,
            },
            Self::WebSocket => IotAgentSpec {
                kind: self,
                backend: IotAgentBackend::NativeRust,
                runtime_env_keys: WS_RUNTIME_ENV_KEYS,
            },
        }
    }
}

/// Construct the native `IotAgent` for `kind`, consuming the resolved context
/// (root / agent_id / realm / opaque `data` config). This is the dispatch table:
/// the only place the closed kind maps to a concrete agent.
/// ref: communication/gateway.rs:448 (external_adapter_for_channel) — a closed
/// match that returns the right boxed adapter, never a plugin registry.
pub(crate) fn build_agent(kind: IotAgentKind, ctx: AgentContext) -> Result<Box<dyn IotAgent>> {
    match kind {
        IotAgentKind::Mqtt => Ok(Box::new(MqttAgent::new(ctx)?)),
        IotAgentKind::Http => Ok(Box::new(HttpAgent::new(ctx)?)),
        IotAgentKind::WebSocket => Ok(Box::new(WsAgent::new(ctx)?)),
    }
}

/// Construct an agent from its persisted `iot_agents.kind` string, parsing the
/// closed kind first. Unknown kinds are a hard error (never silently skipped) so
/// a typo in the configure op surfaces, mirroring communication's strict parse.
/// ref: communication external_adapter_for_channel (kind dispatch).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_agent_from_kind_str(
    kind_str: &str,
    ctx: AgentContext,
) -> Result<Box<dyn IotAgent>> {
    let kind = IotAgentKind::from_str(kind_str)
        .ok_or_else(|| anyhow::anyhow!("unknown iot agent kind: {kind_str:?}"))?;
    build_agent(kind, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::adapters::{ConnectionStatus, IotAgentKind};

    fn ctx_for(root: &std::path::Path, config: serde_json::Value) -> AgentContext<'_> {
        AgentContext {
            root,
            agent_id: "agent-gw-1".into(),
            realm: "master".into(),
            config,
        }
    }

    #[test]
    fn kind_round_trip_and_spec_is_native() {
        for k in [
            IotAgentKind::Mqtt,
            IotAgentKind::Http,
            IotAgentKind::WebSocket,
        ] {
            assert_eq!(IotAgentKind::from_str(k.as_str()), Some(k));
            let spec = k.spec();
            assert_eq!(spec.kind, k);
            assert_eq!(spec.backend, IotAgentBackend::NativeRust);
            assert!(
                !spec.runtime_env_keys.is_empty(),
                "every kind advertises its secret keys for audit"
            );
        }
        assert_eq!(IotAgentKind::from_str("coap"), None);
    }

    #[test]
    fn dispatch_builds_each_kind() {
        let tmp = tempfile::tempdir().unwrap();

        // MQTT: needs a host; construction does not dial.
        let mqtt = build_agent(
            IotAgentKind::Mqtt,
            ctx_for(
                tmp.path(),
                serde_json::json!({ "host": "127.0.0.1", "port": 1883 }),
            ),
        )
        .unwrap();
        assert_eq!(mqtt.kind(), IotAgentKind::Mqtt);
        assert_eq!(mqtt.status(), ConnectionStatus::Disconnected);

        // HTTP: empty config is tolerated (base URI validated at connect()).
        let http = build_agent(
            IotAgentKind::Http,
            ctx_for(tmp.path(), serde_json::json!({})),
        )
        .unwrap();
        assert_eq!(http.kind(), IotAgentKind::Http);

        // WebSocket: requires a ws:// url at construction.
        let ws = build_agent(
            IotAgentKind::WebSocket,
            ctx_for(tmp.path(), serde_json::json!({ "url": "ws://127.0.0.1:1" })),
        )
        .unwrap();
        assert_eq!(ws.kind(), IotAgentKind::WebSocket);
    }

    #[test]
    fn build_from_kind_str_dispatches_and_rejects_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        let agent = build_agent_from_kind_str(
            "mqtt",
            ctx_for(tmp.path(), serde_json::json!({ "host": "127.0.0.1" })),
        )
        .unwrap();
        assert_eq!(agent.kind(), IotAgentKind::Mqtt);

        let err = build_agent_from_kind_str("coap", ctx_for(tmp.path(), serde_json::json!({})));
        assert!(err.is_err(), "unknown kind is a hard error");
    }
}
