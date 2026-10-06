// Origin: CTOX
// License: AGPL-3.0-only

use std::borrow::Cow;

use crate::internal::{modelconfig, registry};

/// Borrowed capabilities shared by the canonical thinking pipeline and its
/// provider appliers. Dynamic IDs and arbitrary configured levels retain their
/// request-owner lifetime; the immutable embedded registry is unchanged.
#[derive(Clone, Debug)]
pub struct ModelInfoView<'a> {
    pub id: &'a str,
    pub provider_type: &'a str,
    pub user_defined: bool,
    pub is_compat: bool,
    pub input_token_limit: usize,
    pub output_token_limit: usize,
    pub context_length: usize,
    pub max_completion_tokens: usize,
    pub thinking: Option<ThinkingSupportView<'a>>,
    pub native_capabilities: Option<&'a registry::NativeCapabilities>,
    pub support_configuration_update: bool,
    static_info: Option<&'a registry::ModelInfo>,
}

#[derive(Clone, Debug)]
pub struct ThinkingSupportView<'a> {
    // Retain both static u64 and configured signed i64 bounds exactly. Native
    // budget validation clamps only at the same isize boundary as before.
    pub min: Option<i128>,
    pub max: Option<i128>,
    pub zero_allowed: bool,
    pub dynamic_allowed: bool,
    pub levels: Cow<'a, [&'a str]>,
}

impl<'a> ModelInfoView<'a> {
    /// Legacy appliers can still receive the exact static descriptor. They must
    /// implement the view entry point to consume dynamically owned metadata.
    pub fn static_info(&self) -> Option<&'a registry::ModelInfo> {
        self.static_info
    }

    /// Shorten this view to the caller's borrow without copying level storage.
    /// Cow's owned level type makes the inner lifetime invariant, so selected
    /// capabilities need this explicit reborrow before joining local fallbacks.
    pub fn reborrow(&self) -> ModelInfoView<'_> {
        ModelInfoView {
            id: self.id,
            provider_type: self.provider_type,
            user_defined: self.user_defined,
            is_compat: self.is_compat,
            input_token_limit: self.input_token_limit,
            output_token_limit: self.output_token_limit,
            context_length: self.context_length,
            max_completion_tokens: self.max_completion_tokens,
            thinking: self.thinking.as_ref().map(|support| ThinkingSupportView {
                min: support.min,
                max: support.max,
                zero_allowed: support.zero_allowed,
                dynamic_allowed: support.dynamic_allowed,
                levels: Cow::Borrowed(support.levels.as_ref()),
            }),
            native_capabilities: self.native_capabilities,
            support_configuration_update: self.support_configuration_update,
            static_info: self.static_info,
        }
    }
}

impl<'a> From<&'a registry::ModelInfo> for ModelInfoView<'a> {
    fn from(info: &'a registry::ModelInfo) -> Self {
        Self {
            id: info.id,
            provider_type: info.provider_type,
            user_defined: info.user_defined,
            is_compat: false,
            input_token_limit: 0,
            output_token_limit: 0,
            context_length: 0,
            max_completion_tokens: info.max_completion_tokens,
            thinking: info.thinking.as_ref().map(|support| ThinkingSupportView {
                min: support.min.map(i128::from),
                max: support.max.map(i128::from),
                zero_allowed: support.zero_allowed,
                dynamic_allowed: support.dynamic_allowed,
                levels: Cow::Borrowed(support.levels),
            }),
            native_capabilities: None,
            support_configuration_update: false,
            static_info: Some(info),
        }
    }
}

impl<'a> From<&'a modelconfig::ModelInfo> for ModelInfoView<'a> {
    fn from(info: &'a modelconfig::ModelInfo) -> Self {
        Self {
            id: &info.id,
            provider_type: &info.provider_type,
            user_defined: info.user_defined,
            is_compat: info.is_compat,
            input_token_limit: info.input_token_limit,
            output_token_limit: info.output_token_limit,
            context_length: info.context_length,
            max_completion_tokens: info.max_completion_tokens,
            thinking: info.thinking.as_ref().map(|support| ThinkingSupportView {
                min: Some(i128::from(support.min)),
                max: Some(i128::from(support.max)),
                zero_allowed: support.zero_allowed,
                dynamic_allowed: support.dynamic_allowed,
                levels: Cow::Owned(support.levels.iter().map(String::as_str).collect()),
            }),
            native_capabilities: info.native_capabilities.as_ref(),
            support_configuration_update: info.support_configuration_update,
            static_info: None,
        }
    }
}

pub(super) fn is_user_defined_model_view(info: Option<&ModelInfoView<'_>>) -> bool {
    info.is_none_or(|model| model.user_defined)
}
