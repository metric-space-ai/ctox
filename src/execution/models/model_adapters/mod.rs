use crate::inference::engine;
use anyhow::Context;
use serde_json::Value;

pub(crate) mod anthropic;
pub(crate) mod deepseek;
pub(crate) mod gemma4;
pub(crate) mod glm47;
pub(crate) mod gpt_oss;
pub(crate) mod kimi;
pub(crate) mod minimax;
pub(crate) mod mistral;
pub(crate) mod nemotron_cascade2;
pub(crate) mod qwen35;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsesTransportKind {
    ChatCompletions,
    CompletionTemplate,
    AnthropicMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsesModelAdapter {
    GptOss,
    Qwen35,
    Gemma4,
    NemotronCascade2,
    Glm47,
    MiniMax,
    Mistral,
    Kimi,
    DeepSeek,
    Anthropic,
}

pub fn adapter_reasoning_cap_env_key() -> &'static str {
    "CTOX_LOCAL_ADAPTER_REASONING_CAP"
}

pub fn adapter_max_output_tokens_cap_env_key() -> &'static str {
    "CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCodexExecPolicy {
    adapter: ResponsesModelAdapter,
    model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesAdapterResponsePlan {
    adapter: ResponsesModelAdapter,
    transport_kind: ResponsesTransportKind,
    stream: bool,
    exact_text_override: Option<String>,
    remote: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResponsesAdapterRoute {
    forwarded_body: Vec<u8>,
    response_plan: ResponsesAdapterResponsePlan,
}

impl ResponsesModelAdapter {
    pub fn from_model(model: &str) -> Option<Self> {
        match engine::chat_model_family_for_model(model)? {
            engine::ChatModelFamily::GptOss => Some(Self::GptOss),
            engine::ChatModelFamily::Qwen35 => Some(Self::Qwen35),
            engine::ChatModelFamily::Gemma4 => Some(Self::Gemma4),
            engine::ChatModelFamily::NemotronCascade2 => Some(Self::NemotronCascade2),
            engine::ChatModelFamily::Glm47Flash => Some(Self::Glm47),
            engine::ChatModelFamily::MiniMax => Some(Self::MiniMax),
            engine::ChatModelFamily::Mistral => Some(Self::Mistral),
            engine::ChatModelFamily::Kimi => Some(Self::Kimi),
            engine::ChatModelFamily::DeepSeek => Some(Self::DeepSeek),
            engine::ChatModelFamily::Anthropic => Some(Self::Anthropic),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::GptOss => gpt_oss::adapter_id(),
            Self::Qwen35 => qwen35::adapter_id(),
            Self::Gemma4 => gemma4::adapter_id(),
            Self::NemotronCascade2 => nemotron_cascade2::adapter_id(),
            Self::Glm47 => glm47::adapter_id(),
            Self::MiniMax => minimax::adapter_id(),
            Self::Mistral => mistral::adapter_id(),
            Self::Kimi => kimi::adapter_id(),
            Self::DeepSeek => deepseek::adapter_id(),
            Self::Anthropic => anthropic::adapter_id(),
        }
    }

    pub fn transport_kind(self) -> ResponsesTransportKind {
        match self {
            Self::GptOss => gpt_oss::transport_kind(),
            Self::Qwen35 => qwen35::transport_kind(),
            Self::Gemma4 => gemma4::transport_kind(),
            Self::NemotronCascade2 => nemotron_cascade2::transport_kind(),
            Self::Glm47 => glm47::transport_kind(),
            Self::MiniMax => minimax::transport_kind(),
            Self::Mistral => mistral::transport_kind(),
            Self::Kimi => kimi::transport_kind(),
            Self::DeepSeek => deepseek::transport_kind(),
            Self::Anthropic => anthropic::transport_kind(),
        }
    }

    pub fn upstream_path(self) -> &'static str {
        match self {
            Self::GptOss => gpt_oss::upstream_path(),
            Self::Qwen35 => qwen35::upstream_path(),
            Self::Gemma4 => gemma4::upstream_path(),
            Self::NemotronCascade2 => nemotron_cascade2::upstream_path(),
            Self::Glm47 => glm47::upstream_path(),
            Self::MiniMax => minimax::upstream_path(),
            Self::Mistral => mistral::upstream_path(),
            Self::Kimi => kimi::upstream_path(),
            Self::DeepSeek => deepseek::upstream_path(),
            Self::Anthropic => anthropic::upstream_path(),
        }
    }

    pub fn local_ctox_exec_policy(self, model: &str) -> LocalCodexExecPolicy {
        LocalCodexExecPolicy {
            adapter: self,
            model: model.trim().to_string(),
        }
    }

    pub fn build_route(
        self,
        raw: &[u8],
        remote: bool,
    ) -> anyhow::Result<ResolvedResponsesAdapterRoute> {
        let request_payload: Value = serde_json::from_slice(raw)
            .context("failed to parse canonical responses request for adapter routing")?;
        let transport_kind = if remote && !matches!(self, Self::Anthropic) {
            // Remote API providers (OpenRouter etc.) always use standard
            // /v1/chat/completions — even GPT-OSS, which locally uses the
            // Harmony completion-template transport.
            ResponsesTransportKind::ChatCompletions
        } else {
            self.transport_kind()
        };
        let response_plan = ResponsesAdapterResponsePlan {
            adapter: self,
            transport_kind,
            stream: engine::responses_request_streams(raw)?,
            exact_text_override: engine::extract_exact_text_override_from_materialized_request(
                &request_payload,
            ),
            remote,
        };
        Ok(ResolvedResponsesAdapterRoute {
            forwarded_body: self.rewrite_request(raw, remote)?,
            response_plan,
        })
    }

    fn rewrite_request(self, raw: &[u8], remote: bool) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::GptOss if remote => gpt_oss::rewrite_chat_request(raw),
            Self::GptOss => gpt_oss::rewrite_request(raw),
            Self::Qwen35 => qwen35::rewrite_request(raw),
            Self::Gemma4 => gemma4::rewrite_request(raw),
            Self::NemotronCascade2 => nemotron_cascade2::rewrite_request(raw),
            Self::Glm47 => glm47::rewrite_request(raw),
            Self::MiniMax => minimax::rewrite_request(raw),
            Self::Mistral => mistral::rewrite_request(raw),
            Self::Kimi => kimi::rewrite_request(raw),
            Self::DeepSeek => deepseek::rewrite_request(raw),
            Self::Anthropic => anthropic::rewrite_request(raw),
        }
    }

    fn rewrite_success_response(
        self,
        raw: &[u8],
        fallback_model: Option<&str>,
        exact_text_override: Option<&str>,
        remote: bool,
    ) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::GptOss if remote => {
                gpt_oss::rewrite_chat_success_response(raw, fallback_model, exact_text_override)
            }
            Self::GptOss => {
                gpt_oss::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
            Self::Qwen35 => {
                qwen35::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
            Self::Gemma4 => {
                gemma4::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
            Self::NemotronCascade2 => nemotron_cascade2::rewrite_success_response(
                raw,
                fallback_model,
                exact_text_override,
            ),
            Self::Glm47 => {
                glm47::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
            Self::MiniMax => {
                minimax::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
            Self::Mistral => {
                mistral::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
            Self::Kimi => kimi::rewrite_success_response(raw, fallback_model, exact_text_override),
            Self::DeepSeek => {
                deepseek::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
            Self::Anthropic => {
                anthropic::rewrite_success_response(raw, fallback_model, exact_text_override)
            }
        }
    }

    fn build_followup_request(
        self,
        initial_request_raw: &[u8],
        first_response_raw: &[u8],
    ) -> anyhow::Result<Option<Vec<u8>>> {
        match self {
            Self::GptOss => {
                gpt_oss::build_followup_request(initial_request_raw, first_response_raw)
            }
            Self::Qwen35
            | Self::Gemma4
            | Self::NemotronCascade2
            | Self::Glm47
            | Self::MiniMax
            | Self::Mistral
            | Self::Kimi
            | Self::DeepSeek
            | Self::Anthropic => Ok(None),
        }
    }
}

pub fn runtime_adapter_tuning_for_local_plan(
    model: &str,
    preset: crate::inference::runtime_plan::ChatPreset,
    max_output_tokens: u32,
) -> crate::inference::runtime_state::AdapterRuntimeTuning {
    match ResponsesModelAdapter::from_model(model) {
        Some(ResponsesModelAdapter::GptOss) => gpt_oss::runtime_tuning(preset, max_output_tokens),
        Some(ResponsesModelAdapter::Qwen35) => qwen35::runtime_tuning(preset, max_output_tokens),
        Some(ResponsesModelAdapter::Gemma4) => gemma4::runtime_tuning(preset, max_output_tokens),
        Some(ResponsesModelAdapter::NemotronCascade2) => {
            nemotron_cascade2::runtime_tuning(preset, max_output_tokens)
        }
        Some(ResponsesModelAdapter::Glm47) => glm47::runtime_tuning(preset, max_output_tokens),
        Some(ResponsesModelAdapter::MiniMax) => minimax::runtime_tuning(preset, max_output_tokens),
        Some(ResponsesModelAdapter::Mistral) => mistral::runtime_tuning(preset, max_output_tokens),
        Some(ResponsesModelAdapter::Kimi) => kimi::runtime_tuning(preset, max_output_tokens),
        Some(ResponsesModelAdapter::DeepSeek) => {
            deepseek::runtime_tuning(preset, max_output_tokens)
        }
        Some(ResponsesModelAdapter::Anthropic) => {
            anthropic::runtime_tuning(preset, max_output_tokens)
        }
        None => crate::inference::runtime_state::AdapterRuntimeTuning::default(),
    }
}

impl ResponsesAdapterResponsePlan {
    pub fn id(&self) -> &'static str {
        self.adapter.id()
    }

    pub fn transport_kind(&self) -> ResponsesTransportKind {
        self.transport_kind
    }

    pub fn stream(&self) -> bool {
        self.stream
    }

    pub fn exact_text_override(&self) -> Option<&str> {
        self.exact_text_override.as_deref()
    }

    pub fn rewrite_success_response(
        &self,
        raw: &[u8],
        fallback_model: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        self.adapter.rewrite_success_response(
            raw,
            fallback_model,
            self.exact_text_override(),
            self.remote,
        )
    }

    pub fn build_followup_request(
        &self,
        initial_request_raw: &[u8],
        first_response_raw: &[u8],
    ) -> anyhow::Result<Option<Vec<u8>>> {
        self.adapter
            .build_followup_request(initial_request_raw, first_response_raw)
    }
}

impl ResolvedResponsesAdapterRoute {
    pub fn resolve(model: Option<&str>, raw: &[u8], remote: bool) -> anyhow::Result<Option<Self>> {
        let Some(model) = model else {
            return Ok(None);
        };
        let Some(adapter) = ResponsesModelAdapter::from_model(model) else {
            return Ok(None);
        };
        Ok(Some(adapter.build_route(raw, remote)?))
    }

    pub fn id(&self) -> &'static str {
        self.response_plan.id()
    }

    pub fn upstream_path(&self) -> &'static str {
        self.response_plan.adapter.upstream_path()
    }

    pub fn forwarded_body(&self) -> &[u8] {
        &self.forwarded_body
    }

    pub fn response_plan(&self) -> ResponsesAdapterResponsePlan {
        self.response_plan.clone()
    }
}

impl LocalCodexExecPolicy {
    pub fn resolve(model: &str) -> Option<Self> {
        let adapter = ResponsesModelAdapter::from_model(model)?;
        Some(adapter.local_ctox_exec_policy(model))
    }

    pub fn compact_instructions(&self) -> &'static str {
        match self.adapter {
            ResponsesModelAdapter::GptOss => gpt_oss::compact_instructions(),
            ResponsesModelAdapter::Qwen35 => qwen35::compact_instructions(),
            ResponsesModelAdapter::Gemma4 => gemma4::compact_instructions(),
            ResponsesModelAdapter::NemotronCascade2 => nemotron_cascade2::compact_instructions(),
            ResponsesModelAdapter::Glm47 => glm47::compact_instructions(),
            ResponsesModelAdapter::MiniMax => minimax::compact_instructions(),
            ResponsesModelAdapter::Mistral => mistral::compact_instructions(),
            ResponsesModelAdapter::Kimi => kimi::compact_instructions(),
            ResponsesModelAdapter::DeepSeek => deepseek::compact_instructions(),
            ResponsesModelAdapter::Anthropic => anthropic::compact_instructions(),
        }
    }

    pub fn reasoning_effort_override(&self) -> Option<&'static str> {
        match self.adapter {
            ResponsesModelAdapter::GptOss => gpt_oss::reasoning_effort_override(),
            ResponsesModelAdapter::Qwen35 => qwen35::reasoning_effort_override(),
            ResponsesModelAdapter::Gemma4 => gemma4::reasoning_effort_override(),
            ResponsesModelAdapter::NemotronCascade2 => {
                nemotron_cascade2::reasoning_effort_override()
            }
            ResponsesModelAdapter::Glm47 => glm47::reasoning_effort_override(),
            ResponsesModelAdapter::MiniMax => minimax::reasoning_effort_override(),
            ResponsesModelAdapter::Mistral => mistral::reasoning_effort_override(),
            ResponsesModelAdapter::Kimi => kimi::reasoning_effort_override(),
            ResponsesModelAdapter::DeepSeek => deepseek::reasoning_effort_override(),
            ResponsesModelAdapter::Anthropic => anthropic::reasoning_effort_override(),
        }
    }

    pub fn unified_exec_enabled(&self) -> bool {
        match self.adapter {
            ResponsesModelAdapter::GptOss => gpt_oss::unified_exec_enabled(),
            ResponsesModelAdapter::Qwen35 => qwen35::unified_exec_enabled(),
            ResponsesModelAdapter::Gemma4 => gemma4::unified_exec_enabled(),
            ResponsesModelAdapter::NemotronCascade2 => nemotron_cascade2::unified_exec_enabled(),
            ResponsesModelAdapter::Glm47 => glm47::unified_exec_enabled(),
            ResponsesModelAdapter::MiniMax => minimax::unified_exec_enabled(),
            ResponsesModelAdapter::Mistral => mistral::unified_exec_enabled(),
            ResponsesModelAdapter::Kimi => kimi::unified_exec_enabled(),
            ResponsesModelAdapter::DeepSeek => deepseek::unified_exec_enabled(),
            ResponsesModelAdapter::Anthropic => anthropic::unified_exec_enabled(),
        }
    }

    pub fn uses_ctox_web_stack(&self) -> bool {
        match self.adapter {
            ResponsesModelAdapter::GptOss => gpt_oss::uses_ctox_web_stack(),
            ResponsesModelAdapter::Qwen35 => qwen35::uses_ctox_web_stack(),
            ResponsesModelAdapter::Gemma4 => gemma4::uses_ctox_web_stack(),
            ResponsesModelAdapter::NemotronCascade2 => nemotron_cascade2::uses_ctox_web_stack(),
            ResponsesModelAdapter::Glm47 => glm47::uses_ctox_web_stack(),
            ResponsesModelAdapter::MiniMax => minimax::uses_ctox_web_stack(),
            ResponsesModelAdapter::Mistral => mistral::uses_ctox_web_stack(),
            ResponsesModelAdapter::Kimi => kimi::uses_ctox_web_stack(),
            ResponsesModelAdapter::DeepSeek => deepseek::uses_ctox_web_stack(),
            ResponsesModelAdapter::Anthropic => anthropic::uses_ctox_web_stack(),
        }
    }

    pub fn compact_limit(&self, realized_context: usize) -> usize {
        match self.adapter {
            ResponsesModelAdapter::GptOss => {
                gpt_oss::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::Qwen35 => {
                qwen35::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::Gemma4 => {
                gemma4::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::NemotronCascade2 => {
                nemotron_cascade2::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::Glm47 => {
                glm47::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::MiniMax => {
                minimax::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::Mistral => {
                mistral::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::Kimi => {
                kimi::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::DeepSeek => {
                deepseek::compact_limit(self.model.as_str(), realized_context)
            }
            ResponsesModelAdapter::Anthropic => {
                anthropic::compact_limit(self.model.as_str(), realized_context)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalCodexExecPolicy;
    use super::ResolvedResponsesAdapterRoute;
    use super::ResponsesModelAdapter;
    use super::ResponsesTransportKind;

    #[test]
    fn gpt_oss_route_uses_completion_template_transport() {
        let adapter = ResponsesModelAdapter::from_model("openai/gpt-oss-120b")
            .expect("gpt-oss adapter should resolve");
        assert_eq!(adapter.id(), "gpt_oss");
        assert_eq!(
            adapter.transport_kind(),
            ResponsesTransportKind::CompletionTemplate
        );
        assert_eq!(adapter.upstream_path(), "/v1/completions");
    }

    #[test]
    fn gemma_route_uses_chat_completions_transport() {
        let request = serde_json::json!({
            "model": "google/gemma-4-26B-A4B-it",
            "input": "hello",
            "stream": true
        });
        let route = ResolvedResponsesAdapterRoute::resolve(
            Some("google/gemma-4-26B-A4B-it"),
            &serde_json::to_vec(&request).expect("request should encode"),
            false,
        )
        .expect("route resolution should succeed")
        .expect("gemma route should resolve");
        let plan = route.response_plan();
        assert_eq!(route.id(), "gemma4");
        assert_eq!(route.upstream_path(), "/v1/chat/completions");
        assert_eq!(
            plan.transport_kind(),
            ResponsesTransportKind::ChatCompletions
        );
        assert!(plan.stream());
    }

    #[test]
    fn anthropic_route_uses_messages_transport() {
        let request = serde_json::json!({
            "model": "claude-opus-4-6",
            "input": "hello",
            "stream": false
        });
        let route = ResolvedResponsesAdapterRoute::resolve(
            Some("claude-opus-4-6"),
            &serde_json::to_vec(&request).expect("request should encode"),
            true,
        )
        .expect("route resolution should succeed")
        .expect("anthropic route should resolve");
        let plan = route.response_plan();
        assert_eq!(route.id(), "anthropic");
        assert_eq!(route.upstream_path(), "/v1/messages");
        assert_eq!(
            plan.transport_kind(),
            ResponsesTransportKind::AnthropicMessages
        );
    }

    #[test]
    fn deepseek_route_uses_chat_completions_transport() {
        let request = serde_json::json!({
            "model": "deepseek/deepseek-v4-flash",
            "input": "hello",
            "stream": false
        });
        let route = ResolvedResponsesAdapterRoute::resolve(
            Some("deepseek/deepseek-v4-flash"),
            &serde_json::to_vec(&request).expect("request should encode"),
            true,
        )
        .expect("route resolution should succeed")
        .expect("deepseek route should resolve");
        let plan = route.response_plan();
        assert_eq!(route.id(), "deepseek");
        assert_eq!(route.upstream_path(), "/v1/chat/completions");
        assert_eq!(
            plan.transport_kind(),
            ResponsesTransportKind::ChatCompletions
        );
    }

    #[test]
    fn local_exec_policy_resolves_generic_compact_instructions() {
        let policy = LocalCodexExecPolicy::resolve("openai/gpt-oss-120b")
            .expect("gpt-oss local exec policy should resolve");
        assert!(policy
            .compact_instructions()
            .contains("local responses-backed runtime"));
        assert_eq!(policy.reasoning_effort_override(), Some("low"));
        assert!(policy.unified_exec_enabled());
    }

    #[test]
    fn local_exec_policy_keeps_model_specific_compact_limits_in_adapter_layer() {
        let qwen = LocalCodexExecPolicy::resolve("Qwen/Qwen3.5-35B-A3B")
            .expect("qwen local exec policy should resolve");
        let qwen36 = LocalCodexExecPolicy::resolve("Qwen/Qwen3.6-35B-A3B")
            .expect("qwen3.6 local exec policy should resolve");
        let glm = LocalCodexExecPolicy::resolve("zai-org/GLM-4.7-Flash")
            .expect("glm local exec policy should resolve");
        assert_eq!(qwen.compact_limit(131_072), 1_536);
        assert_eq!(qwen36.compact_limit(32_768), 1_536);
        assert_eq!(glm.compact_limit(131_072), 1_280);
    }
}
