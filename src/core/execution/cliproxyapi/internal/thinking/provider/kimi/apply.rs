// ref: internal/thinking/provider/kimi/apply.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use crate::internal::{
    registry::ModelInfo,
    thinking::ModelInfoView,
    thinking::{
        convert_budget_to_level,
        json::{remove_path, serialize_if_changed, set_path},
        model_view::is_user_defined_model_view as is_user_defined_model,
        ProviderApplier, ThinkingConfig, ThinkingError, ThinkingMode, LEVEL_AUTO, LEVEL_NONE,
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
        if is_user_defined_model(model_info) {
            return Ok(apply_compatible(body, config));
        }
        if model_info
            .and_then(|model| model.thinking.as_ref())
            .is_none()
        {
            return Ok(body.to_vec());
        }
        Ok(apply_config(normalize_body(body), config))
    }
}

fn apply_compatible(body: &[u8], config: &ThinkingConfig) -> Vec<u8> {
    apply_config(normalize_body(body), config)
}

fn apply_config(body: Vec<u8>, config: &ThinkingConfig) -> Vec<u8> {
    let effort = match config.mode {
        ThinkingMode::Level if config.level.is_empty() => return body,
        ThinkingMode::Level => config.level.as_str().to_owned(),
        ThinkingMode::None if config.level.is_empty() || config.level.as_str() == LEVEL_NONE => {
            return apply_disabled(&body);
        }
        ThinkingMode::None => config.level.as_str().to_owned(),
        ThinkingMode::Budget => {
            let Some(level) = convert_budget_to_level(config.budget) else {
                return body;
            };
            level.as_str().to_owned()
        }
        ThinkingMode::Auto => LEVEL_AUTO.to_owned(),
        ThinkingMode::Unknown(_) => return body,
    };

    apply_enabled(&body, &effort)
}

fn normalize_body(body: &[u8]) -> Vec<u8> {
    if !body.is_empty() && serde_json::from_slice::<Value>(body).is_ok() {
        body.to_vec()
    } else {
        b"{}".to_vec()
    }
}

fn apply_enabled(body: &[u8], effort: &str) -> Vec<u8> {
    let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let original = document.clone();
    remove_path(&mut document, "reasoning_effort");
    set_path(
        &mut document,
        "thinking.type",
        Value::String("enabled".to_owned()),
    );
    set_path(
        &mut document,
        "thinking.effort",
        Value::String(effort.to_owned()),
    );
    serialize_if_changed(body, &original, &document)
}

fn apply_disabled(body: &[u8]) -> Vec<u8> {
    let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let original = document.clone();
    remove_path(&mut document, "thinking");
    remove_path(&mut document, "reasoning_effort");
    set_path(
        &mut document,
        "thinking.type",
        Value::String("disabled".to_owned()),
    );
    serialize_if_changed(body, &original, &document)
}
