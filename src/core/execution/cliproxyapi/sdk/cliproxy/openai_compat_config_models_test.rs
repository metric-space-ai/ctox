// ref: sdk/cliproxy/openai_compat_config_models_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::service_models::{
    build_openai_compatibility_config_models, ConfiguredModel, OpenAiCompatibilityConfig,
};
use crate::internal::registry::OPENAI_IMAGE_MODEL_TYPE;
