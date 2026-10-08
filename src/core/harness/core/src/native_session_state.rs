//! Private snapshot of the actual quiescent Core/provider state.
//! Strict checkpoint decoding restores context and grants no target execution.
use ctox_protocol::ThreadId;
use serde::Serialize;
use std::io::{self, Write};

pub struct NativeSessionState {
    session_id: ThreadId,
    model: String,
    provider_id: String,
    bytes: Vec<u8>,
    core_effect_capture: Option<crate::NativeCoreEffectCapture>,
}

impl NativeSessionState {
    pub(crate) fn from_core(
        session_id: ThreadId,
        model: String,
        provider_id: String,
        payload: &impl Serialize,
    ) -> io::Result<Self> {
        struct Bounded(Vec<u8>);
        impl Write for Bounded {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if bytes.len() > (64 * 1024 * 1024usize).saturating_sub(self.0.len()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "native state exceeds capture budget",
                    ));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut output = Bounded(Vec::new());
        serde_json::to_writer(&mut output, payload).map_err(io::Error::other)?;
        Ok(Self {
            session_id,
            model,
            provider_id,
            bytes: output.0,
            core_effect_capture: None,
        })
    }
    pub fn session_id(&self) -> ThreadId {
        self.session_id
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }
    /// Protected source input only: never emit these bytes in receipts/logs.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(crate) fn with_effect_capture(
        mut self,
        capture: crate::NativeCoreEffectCapture,
    ) -> io::Result<Self> {
        if capture.session_id() != self.session_id {
            return Err(io::Error::other("foreign native Core effect capture"));
        }
        self.core_effect_capture = Some(capture);
        Ok(self)
    }
    /// Only local checked Core shutdown; checkpoint JSON cannot return this.
    pub fn core_effect_capture(&self) -> Option<&crate::NativeCoreEffectCapture> {
        self.core_effect_capture.as_ref()
    }
}

// Decoding restores context, never target permissions or effect certification.
use ctox_protocol::{
    config_types::{CollaborationMode, ReasoningSummary},
    dynamic_tools::DynamicToolSpec,
    models::ResponseItem,
    protocol::{InitialHistory, TokenUsageInfo, TurnContextItem},
};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeImportPayload {
    format: String,
    version: u32,
    pub(crate) session_id: ThreadId,
    harness: String,
    harness_version: String,
    model_id: String,
    model_route_id: String,
    pub(crate) history: Vec<ResponseItem>,
    pub(crate) reference_context: Option<TurnContextItem>,
    pub(crate) token_usage: Option<TokenUsageInfo>,
    pub(crate) previous_turn: Option<NativePreviousTurn>,
    pub(crate) server_reasoning_included: bool,
    pub(crate) base_instructions: String,
    pub(crate) developer_instructions: Option<String>,
    pub(crate) user_instructions: Option<String>,
    pub(crate) compact_prompt: Option<String>,
    pub(crate) collaboration_mode: CollaborationMode,
    pub(crate) reasoning_summary: Option<ReasoningSummary>,
    pub(crate) dynamic_tools: Vec<DynamicToolSpec>,
    pub(crate) mcp_dependency_prompted: BTreeSet<String>,
    pub(crate) active_connector_selection: BTreeSet<String>,
    pub(crate) provider: NativeModelContinuation,
    target_authority: String,
    external_effects: String,
    #[serde(default)]
    core_effects: Option<crate::NativeCoreEffectReport>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativePreviousTurn {
    pub(crate) model: String,
    pub(crate) realtime_active: Option<bool>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeModelContinuation {
    pub(crate) conversation_id: ThreadId,
    pub(crate) wire_api: crate::WireApi,
    pub(crate) websockets_enabled: bool,
    pub(crate) http_fallback: bool,
    pub(crate) last_request: Option<ctox_api::ResponsesApiRequest>,
    pub(crate) last_response: Option<NativeLastResponse>,
    transport_connection: String,
    turn_routing_token: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeLastResponse {
    pub(crate) response_id: String,
    pub(crate) items_added: Vec<ResponseItem>,
}

fn invalid_state() -> io::Error {
    // Never include protected JSON, response IDs or credentials in diagnostics.
    io::Error::new(
        io::ErrorKind::InvalidData,
        "native checkpoint state is invalid",
    )
}

impl NativeSessionState {
    /// Decode only a protected, hash-verified native checkpoint artifact. The
    /// expected identity must come from the native enrolled checkpoint, never
    /// renderer fields. This grants no policy, ownership or effect authority.
    pub fn from_checkpoint(
        bytes: &[u8],
        expected_session: ThreadId,
        expected_model: &str,
        expected_provider: &str,
    ) -> io::Result<Self> {
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(invalid_state());
        }
        let payload: NativeImportPayload =
            serde_json::from_slice(bytes).map_err(|_| invalid_state())?;
        if payload.format != "ctox-native-session-state"
            || payload.version != 1
            || payload.harness != crate::native_harness_name()
            || payload.harness_version != crate::native_harness_version()
            || payload.session_id != expected_session
            || payload.model_id != expected_model
            || payload.model_route_id != expected_provider
            || expected_provider != crate::OPENAI_PROVIDER_ID
            || payload.collaboration_mode.model() != expected_model
            || payload.target_authority != "reauthorization-required"
            || payload.external_effects != "unknown"
        {
            return Err(invalid_state());
        }
        payload.provider.validate(expected_session)?;
        if let Some(report) = &payload.core_effects {
            report.validate_metadata(expected_session)?;
        }
        Ok(Self {
            session_id: expected_session,
            model: expected_model.to_owned(),
            provider_id: expected_provider.to_owned(),
            bytes: bytes.to_vec(),
            core_effect_capture: None,
        })
    }

    pub(crate) fn import_payload(self) -> io::Result<NativeImportPayload> {
        serde_json::from_slice(&self.bytes).map_err(|_| invalid_state())
    }

    pub(crate) fn validate_target(
        &self,
        config: &crate::config::Config,
        history: &InitialHistory,
    ) -> io::Result<()> {
        if config.ephemeral
            || config.model.as_deref() != Some(self.model.as_str())
            || config.model_provider_id != self.provider_id
            || config.model_provider.wire_api != crate::WireApi::Responses
            || !matches!(history, InitialHistory::Resumed(r) if r.conversation_id == self.session_id)
        {
            return Err(invalid_state());
        }
        Ok(())
    }
}

impl NativeModelContinuation {
    pub(crate) fn validate(&self, expected_session: ThreadId) -> io::Result<()> {
        if self.conversation_id != expected_session
            || self.wire_api != crate::WireApi::Responses
            || self.transport_connection != "reconnect-required"
            || self.turn_routing_token != "not-transferable"
            || self.last_request.is_some() != self.last_response.is_some()
            || self
                .last_response
                .as_ref()
                .is_some_and(|r| r.response_id.is_empty() || r.response_id.len() > 4096)
        {
            return Err(invalid_state());
        }
        Ok(())
    }
}
