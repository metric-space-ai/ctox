use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiteParseConfig {
    pub ocr_language: String,
    pub ocr_enabled: bool,
    pub ocr_server_url: Option<String>,
    pub num_workers: usize,
    pub max_pages: usize,
    pub target_pages: Option<String>,
    pub dpi: u16,
    pub output_format: OutputFormat,
    pub precise_bounding_box: bool,
    pub preserve_very_small_text: bool,
    pub preserve_layout_alignment_across_pages: bool,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LiteParseConfigOverrides {
    pub ocr_language: Option<String>,
    pub ocr_enabled: Option<bool>,
    pub ocr_server_url: Option<Option<String>>,
    pub num_workers: Option<usize>,
    pub max_pages: Option<usize>,
    pub target_pages: Option<Option<String>>,
    pub dpi: Option<u16>,
    pub output_format: Option<OutputFormat>,
    pub precise_bounding_box: Option<bool>,
    pub preserve_very_small_text: Option<bool>,
    pub preserve_layout_alignment_across_pages: Option<bool>,
    pub password: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Json,
    Text,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Json
    }
}

impl Default for LiteParseConfig {
    fn default() -> Self {
        Self {
            ocr_language: "en".to_string(),
            ocr_enabled: true,
            ocr_server_url: None,
            num_workers: 4,
            max_pages: 1000,
            target_pages: None,
            dpi: 150,
            output_format: OutputFormat::Json,
            precise_bounding_box: true,
            preserve_very_small_text: false,
            preserve_layout_alignment_across_pages: false,
            password: None,
        }
    }
}

impl LiteParseConfig {
    pub fn merge(overrides: LiteParseConfigOverrides) -> Self {
        let defaults = Self::default();

        Self {
            ocr_language: overrides.ocr_language.unwrap_or(defaults.ocr_language),
            ocr_enabled: overrides.ocr_enabled.unwrap_or(defaults.ocr_enabled),
            ocr_server_url: overrides.ocr_server_url.unwrap_or(defaults.ocr_server_url),
            num_workers: overrides.num_workers.unwrap_or(defaults.num_workers),
            max_pages: overrides.max_pages.unwrap_or(defaults.max_pages),
            target_pages: overrides.target_pages.unwrap_or(defaults.target_pages),
            dpi: overrides.dpi.unwrap_or(defaults.dpi),
            output_format: overrides.output_format.unwrap_or(defaults.output_format),
            precise_bounding_box: overrides
                .precise_bounding_box
                .unwrap_or(defaults.precise_bounding_box),
            preserve_very_small_text: overrides
                .preserve_very_small_text
                .unwrap_or(defaults.preserve_very_small_text),
            preserve_layout_alignment_across_pages: overrides
                .preserve_layout_alignment_across_pages
                .unwrap_or(defaults.preserve_layout_alignment_across_pages),
            password: overrides.password.unwrap_or(defaults.password),
        }
    }
}
