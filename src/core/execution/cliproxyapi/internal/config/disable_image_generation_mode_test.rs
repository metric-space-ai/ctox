// ref: internal/config/disable_image_generation_mode_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::{Deserialize, Serialize};

use super::disable_image_generation_mode::DisableImageGenerationMode;

#[derive(Debug, Deserialize, Serialize)]
struct Wrapper {
    #[serde(rename = "disable-image-generation")]
    value: DisableImageGenerationMode,
}
