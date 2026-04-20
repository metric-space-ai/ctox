//! Run ctox-engine-cli from a full TOML configuration.

use anyhow::Result;
use tracing::info;

use engine_core::initialize_logging;
use engine_server_core::engine_for_server_builder::{MistralRsForServerBuilder, ModelConfig};
#[cfg(unix)]
use engine_server_core::local_ipc;

use crate::commands::run::interactive_mode;
use crate::commands::serve::{convert_to_model_selected, run_server};
use crate::config::{CliConfig, load_cli_config};
use crate::{
    args::{CacheOptions, RuntimeOptions, ServerTransport},
    commands::run_interactive,
};

/// Execute the CLI using a TOML configuration file.
pub async fn run_from_config(path: std::path::PathBuf) -> Result<()> {
    initialize_logging();

    let config = load_cli_config(&path)?;

    match config {
        CliConfig::Serve(cfg) => run_serve_config(cfg).await,
        CliConfig::Run(cfg) => run_run_config(cfg).await,
    }
}

async fn run_serve_config(cfg: crate::config::ServeConfig) -> Result<()> {
    let crate::config::ServeConfig {
        global,
        runtime,
        server,
        paged_attn,
        models,
        default_model_id,
        speculative,
        dflash,
    } = cfg;

    tracing::info!(
        "run_serve_config: models={} speculative={} dflash={}",
        models.len(),
        speculative.is_some(),
        dflash.is_some(),
    );

    if speculative.is_some() && dflash.is_some() {
        anyhow::bail!(
            "Config sets both `speculative` and `dflash`; pick exactly one decode pipeline."
        );
    }

    // Single-model fast path doesn't yet know about spec / dflash decoding,
    // so bail out of it when the config demands a draft — we go through the
    // multi-model/builder path below which does support it.
    if speculative.is_none() && dflash.is_none() {
        if let Some((model_type, runtime)) = resolve_single_model_runtime(
            models.as_slice(),
            default_model_id.as_deref(),
            runtime.clone(),
            CacheOptions {
                paged_attn: paged_attn.clone(),
            },
        )? {
            return run_server(model_type, server, runtime, global.to_global_options()?).await;
        }
    }

    let global = global.to_global_options()?;

    let (
        paged_attn,
        paged_attn_gpu_mem,
        paged_attn_gpu_mem_usage,
        paged_ctxt_len,
        paged_attn_block_size,
        paged_cache_type,
    ) = paged_attn.into_builder_flags();

    let (model_configs, cpu) = build_model_configs(&models)?;

    let mut builder = MistralRsForServerBuilder::new()
        .with_max_seqs(runtime.max_seqs)
        .with_no_kv_cache(runtime.no_kv_cache)
        .with_token_source(global.token_source)
        .with_interactive_mode(false)
        .with_prefix_cache_n(runtime.prefix_cache_n)
        .set_paged_attn(paged_attn)
        .with_cpu(cpu)
        .with_enable_search(runtime.enable_search)
        .with_seed_optional(global.seed)
        .with_log_optional(global.log.as_ref().map(|p| p.to_string_lossy().to_string()))
        .with_chat_template_optional(
            runtime
                .chat_template
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        )
        .with_jinja_explicit_optional(
            runtime
                .jinja_explicit
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        );

    if paged_attn != Some(false) {
        builder = builder
            .with_paged_attn_gpu_mem_optional(paged_attn_gpu_mem)
            .with_paged_attn_gpu_mem_usage_optional(paged_attn_gpu_mem_usage)
            .with_paged_ctxt_len_optional(paged_ctxt_len)
            .with_paged_attn_block_size_optional(paged_attn_block_size)
            .with_paged_attn_cache_type(paged_cache_type);
    }

    for config in model_configs {
        builder = builder.add_model_config(config);
    }

    if let Some(default_model_id) = default_model_id {
        builder = builder.with_default_model_id(default_model_id);
    }

    if let Some(model) = runtime.search_embedding_model {
        builder = builder.with_search_embedding_model(model.into());
    }

    // Speculative-decoding wiring. The target is the first entry in
    // `models`; the draft model goes through the same ModelEntry →
    // ModelSelected conversion as the target. The builder then wraps the
    // target Loader in a `SpeculativeLoader` at build-time, so the
    // pipeline that comes out of `.build()` is a `SpeculativePipeline`.
    if let Some(spec) = speculative {
        let draft_cpu = spec.draft.device.cpu.unwrap_or(false);
        let draft_model_type =
            spec.draft.to_model_type(draft_cpu, CacheOptions::default());
        let draft_selected = crate::commands::serve::convert_to_model_selected(&draft_model_type)?;
        info!(
            "Speculative decoding enabled: target='{}' draft='{}' gamma={}",
            models
                .first()
                .map(|m| m.model_id.as_str())
                .unwrap_or("<unknown>"),
            spec.draft.model_id,
            spec.gamma,
        );
        builder = builder.with_speculative_draft(draft_selected, spec.gamma);
    }

    // DFlash (block-diffusion) speculative decoding — mutually
    // exclusive with the chain-speculative spec above. Validated at
    // build time; here we just thread the paths through.
    if let Some(dflash_spec) = &dflash {
        info!(
            "DFlash decoding enabled: target='{}' draft_safetensors='{}' draft_config='{}'",
            models
                .first()
                .map(|m| m.model_id.as_str())
                .unwrap_or("<unknown>"),
            dflash_spec.draft_safetensors.display(),
            dflash_spec.draft_config.display(),
        );
        builder = builder.with_dflash_draft(
            dflash_spec.draft_safetensors.clone(),
            dflash_spec.draft_config.clone(),
        );
    }

    let engine = builder.build().await?;
    #[cfg(unix)]
    let engine_for_local_ipc = engine.clone();

    match server.transport {
        ServerTransport::LocalIpc => {
            #[cfg(unix)]
            {
                let Some(transport_endpoint) = server.transport_endpoint.clone() else {
                    anyhow::bail!(
                        "server.transport = \"local_ipc\" requires server.transport_endpoint"
                    );
                };
                info!(
                    "Binding local responses IPC endpoint on {} without HTTP listener.",
                    transport_endpoint.display()
                );
                local_ipc::serve_local_openresponses_socket(
                    engine_for_local_ipc,
                    transport_endpoint,
                )
                .await?;
                return Ok(());
            }

            #[cfg(not(unix))]
            {
                anyhow::bail!("server.transport = \"local_ipc\" is only supported on unix");
            }
        }
    }
}

async fn run_run_config(cfg: crate::config::RunConfig) -> Result<()> {
    let crate::config::RunConfig {
        global,
        runtime,
        paged_attn,
        models,
        enable_thinking,
        speculative,
        dflash,
    } = cfg;

    if speculative.is_some() && dflash.is_some() {
        anyhow::bail!(
            "Config sets both `speculative` and `dflash`; pick exactly one decode pipeline."
        );
    }

    if speculative.is_none() && dflash.is_none() {
        if let Some((model_type, runtime)) = resolve_single_model_runtime(
            models.as_slice(),
            None,
            runtime.clone(),
            CacheOptions {
                paged_attn: paged_attn.clone(),
            },
        )? {
            return run_interactive(
                model_type,
                runtime,
                global.to_global_options()?,
                enable_thinking,
            )
            .await;
        }
    }

    let global = global.to_global_options()?;

    let (
        paged_attn,
        paged_attn_gpu_mem,
        paged_attn_gpu_mem_usage,
        paged_ctxt_len,
        paged_attn_block_size,
        paged_cache_type,
    ) = paged_attn.into_builder_flags();

    let (model_configs, cpu) = build_model_configs(&models)?;

    let mut builder = MistralRsForServerBuilder::new()
        .with_max_seqs(runtime.max_seqs)
        .with_no_kv_cache(runtime.no_kv_cache)
        .with_token_source(global.token_source)
        .with_interactive_mode(true)
        .with_prefix_cache_n(runtime.prefix_cache_n)
        .set_paged_attn(paged_attn)
        .with_cpu(cpu)
        .with_enable_search(runtime.enable_search)
        .with_seed_optional(global.seed)
        .with_log_optional(global.log.as_ref().map(|p| p.to_string_lossy().to_string()))
        .with_chat_template_optional(
            runtime
                .chat_template
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        )
        .with_jinja_explicit_optional(
            runtime
                .jinja_explicit
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        );

    if paged_attn != Some(false) {
        builder = builder
            .with_paged_attn_gpu_mem_optional(paged_attn_gpu_mem)
            .with_paged_attn_gpu_mem_usage_optional(paged_attn_gpu_mem_usage)
            .with_paged_ctxt_len_optional(paged_ctxt_len)
            .with_paged_attn_block_size_optional(paged_attn_block_size)
            .with_paged_attn_cache_type(paged_cache_type);
    }

    for config in model_configs {
        builder = builder.add_model_config(config);
    }

    if let Some(model) = runtime.search_embedding_model {
        builder = builder.with_search_embedding_model(model.into());
    }

    if let Some(spec) = speculative {
        let draft_cpu = spec.draft.device.cpu.unwrap_or(false);
        let draft_model_type =
            spec.draft.to_model_type(draft_cpu, CacheOptions::default());
        let draft_selected = crate::commands::serve::convert_to_model_selected(&draft_model_type)?;
        info!(
            "Speculative decoding enabled (interactive): draft='{}' gamma={}",
            spec.draft.model_id, spec.gamma,
        );
        builder = builder.with_speculative_draft(draft_selected, spec.gamma);
    }

    if let Some(dflash_spec) = dflash {
        info!(
            "DFlash decoding enabled (interactive): draft_safetensors='{}' draft_config='{}'",
            dflash_spec.draft_safetensors.display(),
            dflash_spec.draft_config.display(),
        );
        builder = builder
            .with_dflash_draft(dflash_spec.draft_safetensors, dflash_spec.draft_config);
    }

    let engine = builder.build().await?;

    info!("Model(s) loaded, starting interactive mode...");

    interactive_mode(
        engine.clone(),
        runtime.enable_search,
        if enable_thinking { Some(true) } else { None },
    )
    .await;

    Ok(())
}

fn build_model_configs(models: &[crate::config::ModelEntry]) -> Result<(Vec<ModelConfig>, bool)> {
    let mut cpu_setting: Option<bool> = None;
    let mut configs = Vec::new();

    for entry in models {
        if let Some(cpu) = entry.device.cpu {
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

    let cpu = cpu_setting.unwrap_or(false);

    for entry in models {
        let model_type = entry.to_model_type(cpu, CacheOptions::default());
        let model_selected = convert_to_model_selected(&model_type)?;

        let mut config = ModelConfig::new(entry.model_id.clone(), model_selected);

        if let Some(chat_template) = entry.chat_template.as_ref() {
            config = config.with_chat_template(chat_template.to_string_lossy().to_string());
        }

        if let Some(jinja_explicit) = entry.jinja_explicit.as_ref() {
            config = config.with_jinja_explicit(jinja_explicit.to_string_lossy().to_string());
        }

        if let Some(device_layers) = entry.device.device_layers.clone() {
            config = config.with_num_device_layers(device_layers);
        }

        if let Some(isq) = entry.quantization.in_situ_quant.clone() {
            config = config.with_in_situ_quant(isq);
        }

        configs.push(config);
    }

    Ok((configs, cpu))
}

fn resolve_single_model_runtime(
    models: &[crate::config::ModelEntry],
    default_model_id: Option<&str>,
    mut runtime: RuntimeOptions,
    cache: CacheOptions,
) -> Result<Option<(crate::args::ModelType, RuntimeOptions)>> {
    if models.len() != 1 {
        return Ok(None);
    }

    let entry = &models[0];
    if let Some(default_model_id) = default_model_id {
        if default_model_id != entry.model_id {
            return Ok(None);
        }
    }

    let cpu = entry.device.cpu.unwrap_or(false);
    let model_type = entry.to_model_type(cpu, cache);
    if runtime.chat_template.is_none() {
        runtime.chat_template = entry.chat_template.clone();
    }
    if runtime.jinja_explicit.is_none() {
        runtime.jinja_explicit = entry.jinja_explicit.clone();
    }
    Ok(Some((model_type, runtime)))
}
