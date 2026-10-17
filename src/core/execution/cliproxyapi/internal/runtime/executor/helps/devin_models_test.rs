// ref: internal/runtime/executor/helps/devin_models_test.go:10-602
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Test source blob: d73914a640d73ff9f053d989b33b6fb561a525e7
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_models::{
    has_devin_effort_suffix, normalize_devin_thinking_level, resolve_devin_chat_model_uid,
};
use crate::internal::registry::{
    embedded_models_catalog, DevinModelsStore, RegistryModelInfo, RegistryThinkingSupport,
    StaticModelsCatalog,
};
