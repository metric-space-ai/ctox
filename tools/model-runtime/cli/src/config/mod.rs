//! Configuration file loading for ctox-engine-cli
//!
//! Supports a full TOML configuration that mirrors the CLI options while
//! allowing multiple models without aliases.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::args::{
    AdapterOptions, CacheOptions, DeviceOptions, FormatOptions, GlobalOptions, ModelSourceOptions,
    ModelType, PagedAttentionOptions, QuantizationOptions, RuntimeOptions, ServerOptions,
    VisionOptions,
};
use engine_core::{ModelDType, NormalLoaderType, TokenSource};

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
pub enum CliConfig {
    Serve(ServeConfig),
    Run(RunConfig),
}

#[derive(Deserialize, Default)]
pub struct ServeConfig {
    #[serde(default)]
    pub global: GlobalOptionsToml,
    #[serde(default)]
    pub runtime: RuntimeOptions,
    #[serde(default)]
    pub server: ServerOptions,
    #[serde(default)]
    pub paged_attn: PagedAttentionOptions,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    #[serde(default)]
    pub default_model_id: Option<String>,
    /// Optional speculative-decoding spec. When set, the first entry in
    /// `models` is the target; `speculative.draft` is loaded as the draft
    /// pipeline and wrapped via `SpeculativePipeline`. Tokenizers must match.
    #[serde(default)]
    pub speculative: Option<SpeculativeSpec>,
    /// Optional DFlash (block-diffusion) speculative spec. Mutually
    /// exclusive with `speculative` — a model has exactly one decode
    /// pipeline. When set, the first entry in `models` is the target
    /// and the DFlash draft is loaded directly from `dflash.*` paths.
    #[serde(default)]
    pub dflash: Option<DFlashSpec>,
}

#[derive(Deserialize, Default)]
pub struct RunConfig {
    #[serde(default)]
    pub global: GlobalOptionsToml,
    #[serde(default)]
    pub runtime: RuntimeOptions,
    #[serde(default)]
    pub paged_attn: PagedAttentionOptions,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    #[serde(default)]
    pub enable_thinking: bool,
    /// See [`ServeConfig::speculative`].
    #[serde(default)]
    pub speculative: Option<SpeculativeSpec>,
    /// See [`ServeConfig::dflash`].
    #[serde(default)]
    pub dflash: Option<DFlashSpec>,
}

/// Draft-model spec for speculative decoding. Picks up where
/// `engine_core::SpeculativePipeline` leaves off: the core was already
/// written, this surface just threads it through the `from-config` CLI.
#[derive(Deserialize, Clone)]
pub struct SpeculativeSpec {
    /// Number of draft tokens per step (γ in the paper). Typical: 4–8.
    pub gamma: usize,
    /// Draft model entry (same schema as an entry in `models`). Its
    /// tokenizer vocabulary must match the target model exactly.
    pub draft: ModelEntry,
}

/// DFlash block-diffusion speculative-decoding spec. Parallel to
/// `SpeculativeSpec`, with a different shape: the draft here is a
/// hand-loaded safetensors checkpoint rather than another `ModelEntry`
/// that goes through the normal loader. That's because the DFlash
/// draft (`z-lab/Qwen3.5-27B-DFlash`) is not a standard Qwen3 — it's
/// a 5-layer conditioned block-diffusion model that has no embed /
/// lm_head of its own (it shares the target's) and doesn't fit the
/// `NormalLoaderType` enum.
///
/// Example TOML:
/// ```toml
/// [dflash]
/// draft_safetensors = "/models/dflash/model.safetensors"
/// draft_config      = "/models/dflash/config.json"
/// ```
///
/// `engine_core::DFlashPipeline` performs the actual wiring; this
/// struct only feeds it through the CLI.
#[derive(Deserialize, Clone)]
pub struct DFlashSpec {
    /// Path to the DFlash draft safetensors. Must contain the
    /// 58-tensor layout the `DFlashDraftModel` loader expects
    /// (`fc.weight`, `hidden_norm.weight`, `layers.<N>.…`, `norm.weight`).
    pub draft_safetensors: PathBuf,

    /// Path to the DFlash draft `config.json`. Parsed via
    /// `DFlashDraftConfig`. Typically sits next to the safetensors in
    /// the HF snapshot directory.
    pub draft_config: PathBuf,
}

#[derive(Deserialize, Default, Clone)]
pub struct GlobalOptionsToml {
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub log: Option<PathBuf>,
    #[serde(default)]
    pub token_source: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ModelKind {
    Auto,
    Text,
    Vision,
    Diffusion,
    Speech,
    Embedding,
}

#[derive(Deserialize, Clone)]
pub struct ModelEntry {
    pub kind: ModelKind,
    pub model_id: String,
    #[serde(default)]
    pub tokenizer: Option<PathBuf>,
    #[serde(default)]
    pub arch: Option<NormalLoaderType>,
    #[serde(default)]
    pub dtype: ModelDType,
    #[serde(default)]
    pub format: FormatOptions,
    #[serde(default)]
    pub adapter: AdapterOptions,
    #[serde(default)]
    pub quantization: QuantizationOptions,
    #[serde(default)]
    pub device: DeviceOptionsToml,
    #[serde(default)]
    pub vision: VisionOptions,
    #[serde(default)]
    pub chat_template: Option<PathBuf>,
    #[serde(default)]
    pub jinja_explicit: Option<PathBuf>,
}

#[derive(Deserialize, Default, Clone)]
pub struct DeviceOptionsToml {
    #[serde(default)]
    pub cpu: Option<bool>,
    #[serde(default)]
    pub device_layers: Option<Vec<String>>,
    #[serde(default)]
    pub topology: Option<PathBuf>,
    #[serde(default)]
    pub hf_cache: Option<PathBuf>,
    #[serde(default)]
    pub max_seq_len: Option<usize>,
    #[serde(default)]
    pub max_batch_size: Option<usize>,
}

pub fn load_cli_config(path: &Path) -> Result<CliConfig> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("toml"))
        != Some(true)
    {
        anyhow::bail!("ctox-engine-cli config files must be .toml");
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.to_string_lossy()))?;

    let config: CliConfig =
        toml::from_str(&contents).context("Failed to parse TOML config file")?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &CliConfig) -> Result<()> {
    let (models, default_model_id, runtime) = match config {
        CliConfig::Serve(cfg) => (&cfg.models, cfg.default_model_id.as_ref(), &cfg.runtime),
        CliConfig::Run(cfg) => (&cfg.models, None, &cfg.runtime),
    };

    if models.is_empty() {
        anyhow::bail!("Config must define at least one model in [[models]]");
    }

    if let Some(default_id) = default_model_id {
        let has_model = models.iter().any(|model| model.model_id == *default_id);
        if !has_model {
            anyhow::bail!(
                "default_model_id '{}' does not match any model_id in [[models]]",
                default_id
            );
        }
    }

    let mut cpu_setting: Option<bool> = None;
    for model in models {
        if let Some(cpu) = model.device.cpu {
            match cpu_setting {
                None => cpu_setting = Some(cpu),
                Some(existing) if existing != cpu => {
                    anyhow::bail!(
                        "cpu must be consistent across all models (found both true and false)"
                    );
                }
                _ => {}
            }
        }
    }

    if runtime.search_embedding_model.is_some() && !runtime.enable_search {
        anyhow::bail!("search_embedding_model requires enable_search = true");
    }

    Ok(())
}

impl GlobalOptionsToml {
    pub fn to_global_options(&self) -> Result<GlobalOptions> {
        let token_source = match &self.token_source {
            Some(value) => value
                .parse()
                .map_err(|err| anyhow::anyhow!("Invalid token_source: {err}"))?,
            None => TokenSource::CacheToken,
        };

        Ok(GlobalOptions {
            seed: self.seed,
            log: self.log.clone(),
            token_source,
        })
    }
}

impl DeviceOptionsToml {
    pub fn to_device_options(&self, cpu: bool) -> DeviceOptions {
        let defaults = DeviceOptions::default();
        DeviceOptions {
            cpu,
            device_layers: self.device_layers.clone(),
            topology: self.topology.clone(),
            hf_cache: self.hf_cache.clone(),
            max_seq_len: self.max_seq_len.unwrap_or(defaults.max_seq_len),
            max_batch_size: self.max_batch_size.unwrap_or(defaults.max_batch_size),
        }
    }
}

impl ModelEntry {
    pub fn to_model_type(&self, cpu: bool, cache: CacheOptions) -> ModelType {
        let model = ModelSourceOptions {
            model_id: self.model_id.clone(),
            tokenizer: self.tokenizer.clone(),
            arch: self.arch.clone(),
            dtype: self.dtype,
        };

        let device = self.device.to_device_options(cpu);

        match self.kind {
            ModelKind::Auto => ModelType::Auto {
                model,
                format: self.format.clone(),
                adapter: self.adapter.clone(),
                quantization: self.quantization.clone(),
                device,
                cache,
                vision: self.vision.clone(),
            },
            ModelKind::Text => ModelType::Text {
                model,
                format: self.format.clone(),
                adapter: self.adapter.clone(),
                quantization: self.quantization.clone(),
                device,
                cache,
            },
            ModelKind::Vision => ModelType::Vision {
                model,
                format: self.format.clone(),
                adapter: self.adapter.clone(),
                quantization: self.quantization.clone(),
                device,
                cache,
                vision: self.vision.clone(),
            },
            ModelKind::Diffusion => ModelType::Diffusion { model, device },
            ModelKind::Speech => ModelType::Speech {
                model,
                quantization: self.quantization.clone(),
                device,
            },
            ModelKind::Embedding => ModelType::Embedding {
                model,
                format: self.format.clone(),
                quantization: self.quantization.clone(),
                device,
                cache,
            },
        }
    }
}
