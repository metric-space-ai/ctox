//! Sub-skill runner — concrete implementation of `tools::SubSkillRunner`.
//!
//! Three sub-skills: block_writer, revisor, flow_reviewer. Each runs as
//! a one-shot LLM call with the system prompt loaded from
//! `skills/system/research/deep-research/references/sub_skill_*.md` and
//! the user-message JSON-payload built by `Workspace::skill_input(...)`.
//!
//! Output is plain JSON text matching the schema in
//! [`crate::report::schemas`] (parsed and validated by the tool layer).
//!
//! This module exposes two [`InferenceCallable`] implementations:
//!
//! - [`DefaultInferenceCallable`] — provisional in-process implementation
//!   that resolves the active chat model via
//!   [`crate::inference::runtime_env::env_or_config`] and posts a
//!   chat-completions request to the configured OpenAI-compatible
//!   provider over HTTPS using `ureq`. Marked `TODO(integration)`: the
//!   long-term plan is to replace this with a shared CTOX one-shot
//!   helper once the inference team standardises a Responses entry
//!   point. Until then this is the working, dependency-free path.
//! - [`StaticInferenceCallable`] — a test-only fixture that returns a
//!   pre-configured response based on which sub-skill prompt was
//!   supplied. Used by Wave 6 integration tests.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::inference::runtime_env;
use crate::inference::runtime_state;
use crate::report::tools::SubSkillRunner;
use crate::secrets;

/// Default per-sub-skill timeout budget. Read by the manager's tool
/// dispatch when constructing a [`CtoxSubSkillRunner`].
pub const DEFAULT_SKILL_TIMEOUT: Duration = Duration::from_secs(8 * 60);

/// One-shot inference surface. Anything that can take a system prompt
/// plus a JSON user payload and return a raw assistant string fits.
///
/// The trait is deliberately narrow: the host wires whichever
/// implementation makes sense for the deployment (in-process model,
/// remote API, mock).
pub trait InferenceCallable: Send + Sync {
    fn run_one_shot(
        &self,
        system_prompt: &str,
        user_payload: &Value,
        timeout: Duration,
    ) -> Result<String>;
}

/// Sub-skill discriminator. Used to pick the right reference markdown
/// when loading the system prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubSkillKind {
    Writer,
    Revisor,
    FlowReviewer,
}

impl SubSkillKind {
    fn file_name(self) -> &'static str {
        match self {
            SubSkillKind::Writer => "sub_skill_writer.md",
            SubSkillKind::Revisor => "sub_skill_revisor.md",
            SubSkillKind::FlowReviewer => "sub_skill_flow_reviewer.md",
        }
    }
}

/// Concrete `SubSkillRunner` that loads the system prompt from disk
/// and delegates the inference call to a pluggable
/// [`InferenceCallable`].
pub struct CtoxSubSkillRunner {
    #[allow(dead_code)]
    root: PathBuf,
    skill_root: PathBuf,
    timeout: Duration,
    #[allow(dead_code)]
    model_override: Option<String>,
    inference: Box<dyn InferenceCallable>,
}

impl CtoxSubSkillRunner {
    /// Build a runner. `root` is the CTOX root (used by the inference
    /// callable to look up runtime config and secrets); `inference` is
    /// the wired one-shot caller.
    pub fn new(root: &Path, inference: Box<dyn InferenceCallable>) -> Result<Self> {
        let skill_root = root
            .join("skills")
            .join("system")
            .join("research")
            .join("deep-research");
        Ok(Self {
            root: root.to_path_buf(),
            skill_root,
            timeout: DEFAULT_SKILL_TIMEOUT,
            model_override: None,
            inference,
        })
    }

    /// Override the active model for this runner. Currently informational
    /// — the [`DefaultInferenceCallable`] reads `CTOX_CHAT_MODEL` from
    /// runtime config — but kept on the surface so callers can plumb a
    /// boost model in once the inference helper is unified.
    pub fn with_model_override(mut self, model: String) -> Self {
        self.model_override = Some(model);
        self
    }

    /// Override the per-sub-skill timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Load the system prompt file for `sub_skill` from
    /// `<skill_root>/references/`. The full markdown body is the prompt
    /// — including its `## Hard rules` and `## Output schema` sections;
    /// nothing is trimmed or paraphrased.
    fn load_system_prompt(&self, sub_skill: SubSkillKind) -> Result<String> {
        let path = self
            .skill_root
            .join("references")
            .join(sub_skill.file_name());
        std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read sub-skill system prompt {}", path.display()))
    }
}

impl SubSkillRunner for CtoxSubSkillRunner {
    fn run_writer(&self, input: &Value) -> Result<String> {
        let prompt = self.load_system_prompt(SubSkillKind::Writer)?;
        self.inference.run_one_shot(&prompt, input, self.timeout)
    }

    fn run_revisor(&self, input: &Value) -> Result<String> {
        let prompt = self.load_system_prompt(SubSkillKind::Revisor)?;
        self.inference.run_one_shot(&prompt, input, self.timeout)
    }

    fn run_flow_reviewer(&self, input: &Value) -> Result<String> {
        let prompt = self.load_system_prompt(SubSkillKind::FlowReviewer)?;
        self.inference.run_one_shot(&prompt, input, self.timeout)
    }
}

// ---------------------------------------------------------------------
// DefaultInferenceCallable
// ---------------------------------------------------------------------

/// Provisional one-shot inference helper that targets the configured
/// OpenAI-compatible chat-completions endpoint.
///
/// TODO(integration): replace with shared CTOX inference helper once the
/// team standardises a one-shot Responses entry point. The current
/// implementation is intentionally narrow — it exists to give the
/// manager a working sub-skill path for remote-API deployments without
/// inventing a parallel inference stack. Local in-process runners
/// should plug their own [`InferenceCallable`] in instead of using this
/// HTTP path.
pub struct DefaultInferenceCallable {
    root: PathBuf,
}

impl DefaultInferenceCallable {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn resolve_model(&self) -> Result<String> {
        if let Some(value) = runtime_env::env_or_config(&self.root, "CTOX_CHAT_MODEL")
            .filter(|v| !v.trim().is_empty())
        {
            return Ok(value);
        }
        if let Some(value) = runtime_env::env_or_config(&self.root, "CTOX_ACTIVE_MODEL")
            .filter(|v| !v.trim().is_empty())
        {
            return Ok(value);
        }
        Err(anyhow!(
            "no chat model configured (CTOX_CHAT_MODEL / CTOX_ACTIVE_MODEL unset)"
        ))
    }

    fn resolve_upstream_base_url(&self) -> String {
        if let Some(value) = runtime_env::env_or_config(&self.root, "CTOX_UPSTREAM_BASE_URL")
            .filter(|v| !v.trim().is_empty())
        {
            return value;
        }
        if let Some(provider) = runtime_env::env_or_config(&self.root, "CTOX_API_PROVIDER")
            .filter(|v| !v.trim().is_empty())
        {
            return runtime_state::default_api_upstream_base_url_for_provider(&provider)
                .to_string();
        }
        runtime_state::default_api_upstream_base_url_for_provider("openai").to_string()
    }

    fn resolve_api_key(&self, upstream_base_url: &str) -> Result<String> {
        let env_key = runtime_state::api_key_env_var_for_upstream_base_url(upstream_base_url);
        // Try the secret store first, then the runtime-env map.
        if let Some(value) =
            secrets::get_credential(&self.root, env_key).filter(|v| !v.trim().is_empty())
        {
            return Ok(value);
        }
        if let Some(value) =
            runtime_env::env_or_config(&self.root, env_key).filter(|v| !v.trim().is_empty())
        {
            return Ok(value);
        }
        Err(anyhow!(
            "no API key configured for upstream {upstream_base_url:?} (looked up secret/env key {env_key:?})"
        ))
    }
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

impl InferenceCallable for DefaultInferenceCallable {
    fn run_one_shot(
        &self,
        system_prompt: &str,
        user_payload: &Value,
        timeout: Duration,
    ) -> Result<String> {
        // TODO(integration): replace with shared CTOX inference helper
        // once a one-shot Responses entry point is standardised. Until
        // then we POST to /v1/chat/completions on the configured
        // provider.
        let model = self.resolve_model()?;
        let upstream = self.resolve_upstream_base_url();
        let api_key = self.resolve_api_key(&upstream)?;

        let user_text = serde_json::to_string(user_payload)
            .context("failed to encode sub-skill user payload as JSON")?;

        let body = json!({
            "model": model,
            "temperature": 0.2,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_text },
            ],
        });

        let url = format!("{}/v1/chat/completions", upstream.trim_end_matches('/'));
        let body_text = serde_json::to_string(&body)
            .context("failed to encode chat-completions request body")?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(timeout)
            .timeout_write(Duration::from_secs(30))
            .build();
        let response = agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", api_key))
            .set("Content-Type", "application/json")
            .send_string(&body_text)
            .with_context(|| format!("sub-skill chat-completions POST to {url} failed"))?;

        let response_body = response
            .into_string()
            .context("failed to read chat-completions response body")?;
        let parsed: ChatCompletionResponse = serde_json::from_str(&response_body)
            .context("failed to decode chat-completions response body")?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message)
            .and_then(|message| message.content)
            .ok_or_else(|| {
                anyhow!("chat-completions response carried no choices[0].message.content")
            })?;

        Ok(content)
    }
}

// ---------------------------------------------------------------------
// StaticInferenceCallable (test fixture)
// ---------------------------------------------------------------------

/// Test-only [`InferenceCallable`] that returns one of three canned
/// responses depending on which sub-skill prompt the caller supplied.
/// Discriminates on the title prefix of the system prompt so the same
/// fixture works without re-reading the markdown files.
#[derive(Debug, Clone)]
pub struct StaticInferenceCallable {
    pub writer_response: String,
    pub revisor_response: String,
    pub flow_reviewer_response: String,
}

impl InferenceCallable for StaticInferenceCallable {
    fn run_one_shot(
        &self,
        system_prompt: &str,
        _user_payload: &Value,
        _timeout: Duration,
    ) -> Result<String> {
        if system_prompt.contains("Block Writer") {
            Ok(self.writer_response.clone())
        } else if system_prompt.contains("Revision") {
            Ok(self.revisor_response.clone())
        } else if system_prompt.contains("Flow Review") {
            Ok(self.flow_reviewer_response.clone())
        } else {
            // Default to the writer slot so a malformed prompt does not
            // crash the test fixture; tests can still assert on the
            // exact response by inspecting which slot was returned.
            Ok(self.writer_response.clone())
        }
    }
}
