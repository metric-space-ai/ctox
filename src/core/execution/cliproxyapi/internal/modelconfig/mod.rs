// Origin: CTOX module boundary for the pinned upstream `internal/modelconfig` package.
// License: AGPL-3.0-only

pub mod model_hash;
pub mod model_info;

pub use model_hash::{
    compute_claude_models_hash, compute_codex_models_hash, compute_gemini_models_hash,
    compute_openai_compat_models_hash, compute_vertex_compat_models_hash, ClaudeModel, CodexModel,
    GeminiModel, ModelAlias, OpenAiCompatibilityModel, VertexCompatModel,
};
pub use model_info::{
    normalize_thinking_support, resolve_model_info, HomeModelOptions, ModelInfo,
    NativeCapabilities, ThinkingSupport,
};

#[cfg(test)]
mod model_info_test;
