// ref: internal/thinking/provider/openai/apply.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::{
    registry::ModelInfo,
    thinking::ModelInfoView,
    thinking::{
        convert_budget_to_level, has_level,
        model_view::is_user_defined_model_view as is_user_defined_model, ProviderApplier,
        ThinkingConfig, ThinkingError, ThinkingMode, LEVEL_AUTO, LEVEL_NONE,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Applier;

impl Applier {
    pub const fn new() -> Self {
        Self
    }
}

impl ProviderApplier for Applier {
    fn apply(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&ModelInfo>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let view = model_info.map(ModelInfoView::from);
        self.apply_model_info(body, config, view.as_ref())
    }

    fn apply_model_info(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&ModelInfoView<'_>>,
    ) -> Result<Vec<u8>, ThinkingError> {
        apply_effort_at_path(body, config, model_info, "reasoning_effort")
    }
}

/// Shared level-only effort behavior used by OpenAI Chat and the xAI
/// Responses-compatible leaf. The upstream xAI applier embeds Codex's applier;
/// the Codex file remains outside this worker, so sharing the identical logic
/// here avoids a second implementation authority.
pub(in crate::internal::thinking::provider) fn apply_effort_at_path(
    body: &[u8],
    config: &ThinkingConfig,
    model_info: Option<&ModelInfoView<'_>>,
    path: &str,
) -> Result<Vec<u8>, ThinkingError> {
    if is_user_defined_model(model_info) {
        return Ok(apply_compatible(body, config, path));
    }

    let Some(support) = model_info.and_then(|model| model.thinking.as_ref()) else {
        return Ok(body.to_vec());
    };
    if !matches!(config.mode, ThinkingMode::Level | ThinkingMode::None) {
        return Ok(body.to_vec());
    }

    let normalized_body = normalize_body(body);
    if config.mode == ThinkingMode::Level {
        return Ok(set_effort(&normalized_body, path, config.level.as_str()));
    }

    let mut effort = "";
    if config.budget == 0
        && (support.zero_allowed || has_level(support.levels.as_ref(), LEVEL_NONE))
    {
        effort = LEVEL_NONE;
    }
    if effort.is_empty() && !config.level.is_empty() {
        effort = config.level.as_str();
    }
    if effort.is_empty() {
        if let Some(first) = support.levels.first() {
            effort = first;
        }
    }
    if effort.is_empty() {
        return Ok(normalized_body);
    }
    Ok(set_effort(&normalized_body, path, effort))
}

fn apply_compatible(body: &[u8], config: &ThinkingConfig, path: &str) -> Vec<u8> {
    let normalized_body = normalize_body(body);
    let effort = match config.mode {
        ThinkingMode::Level => {
            if config.level.is_empty() {
                return normalized_body;
            }
            config.level.as_str()
        }
        ThinkingMode::None => {
            if config.level.is_empty() {
                LEVEL_NONE
            } else {
                config.level.as_str()
            }
        }
        ThinkingMode::Auto => LEVEL_AUTO,
        ThinkingMode::Budget => {
            let Some(level) = convert_budget_to_level(config.budget) else {
                return normalized_body;
            };
            return set_effort(&normalized_body, path, level.as_str());
        }
        ThinkingMode::Unknown(_) => return normalized_body,
    };
    set_effort(&normalized_body, path, effort)
}

fn normalize_body(body: &[u8]) -> Vec<u8> {
    if !body.is_empty() && std::str::from_utf8(body).ok().is_some_and(gjson::valid) {
        body.to_vec()
    } else {
        b"{}".to_vec()
    }
}

fn set_effort(body: &[u8], path: &str, effort: &str) -> Vec<u8> {
    use crate::internal::translator::common::{set_json_string, set_raw_path};
    let Ok(document) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    let root = gjson::parse(document);
    let mut output = match root.kind() {
        gjson::Kind::Object | gjson::Kind::Array => body.to_vec(),
        _ => b"{}".to_vec(),
    };
    if path == "reasoning.effort" {
        let reasoning = crate::internal::util::get_gjson_bytes_no_copy(&output, "reasoning");
        if reasoning.kind() == gjson::Kind::Array {
            return output;
        }
        if reasoning.kind() != gjson::Kind::Object {
            output = set_raw_path(&output, "reasoning", b"{}");
        }
    }
    // Raw mutation preserves duplicate input members, arbitrary numeric
    // lexemes and unrelated fields while changing only the effort path.
    set_json_string(&output, path, effort)
}
