// ref: internal/thinking/types.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use crate::internal::registry::ModelInfo;

/// The type of thinking configuration carried by [`ThinkingConfig`].
///
/// The `Unknown` case preserves Go's open integer representation and its
/// observable `String()` fallback instead of narrowing the wire-facing type to
/// only the four currently declared constants.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThinkingMode {
    #[default]
    Budget,
    Level,
    None,
    Auto,
    Unknown(i32),
}

impl fmt::Display for ThinkingMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Budget => "budget",
            Self::Level => "level",
            Self::None => "none",
            Self::Auto => "auto",
            Self::Unknown(_) => "unknown",
        })
    }
}

impl ThinkingMode {
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Budget => 0,
            Self::Level => 1,
            Self::None => 2,
            Self::Auto => 3,
            Self::Unknown(value) => value,
        }
    }
}

impl From<i32> for ThinkingMode {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Budget,
            1 => Self::Level,
            2 => Self::None,
            3 => Self::Auto,
            value => Self::Unknown(value),
        }
    }
}

/// A discrete thinking level.
///
/// Go's named string type is open to unknown values, so this Rust port uses a
/// newtype rather than an enum. That matters to validation, which must be able
/// to report an unsupported user-provided value verbatim.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ThinkingLevel(String);

impl ThinkingLevel {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&str> for ThinkingLevel {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ThinkingLevel {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for ThinkingLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub const LEVEL_NONE: &str = "none";
pub const LEVEL_AUTO: &str = "auto";
pub const LEVEL_MINIMAL: &str = "minimal";
pub const LEVEL_LOW: &str = "low";
pub const LEVEL_MEDIUM: &str = "medium";
pub const LEVEL_HIGH: &str = "high";
pub const LEVEL_XHIGH: &str = "xhigh";
pub const LEVEL_MAX: &str = "max";

/// Provider-neutral thinking configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThinkingConfig {
    pub mode: ThinkingMode,
    pub budget: isize,
    pub level: ThinkingLevel,
}

/// Result of extracting a final parenthesized thinking suffix.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SuffixResult {
    pub model_name: String,
    pub has_suffix: bool,
    pub raw_suffix: String,
}

/// Provider-specific application of a provider-neutral thinking config.
pub trait ProviderApplier: Send + Sync {
    fn apply(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&ModelInfo>,
    ) -> Result<Vec<u8>, crate::internal::thinking::ThinkingError>;

    /// Canonical application with either static or account-selected metadata.
    /// Native appliers override this path. A legacy plugin cannot silently
    /// discard a dynamic capability and pretend the model was unregistered.
    fn apply_model_info(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&super::ModelInfoView<'_>>,
    ) -> Result<Vec<u8>, crate::internal::thinking::ThinkingError> {
        match model_info {
            Some(info) if info.static_info().is_none() => Err(super::ThinkingError::with_model(
                super::ErrorCode::ProviderMismatch,
                "provider applier does not implement dynamic model capabilities",
                info.id,
            )),
            info => self.apply(
                body,
                config,
                info.and_then(super::ModelInfoView::static_info),
            ),
        }
    }
}

/// Reports models whose thinking configuration is intentionally passed through
/// without registry validation. A missing model mirrors upstream's compatible
/// provider path and is therefore considered user-defined.
pub fn is_user_defined_model(model_info: Option<&ModelInfo>) -> bool {
    model_info.is_none_or(|model| model.user_defined)
}
