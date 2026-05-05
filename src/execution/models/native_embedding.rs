use anyhow::{Context, Result};
use ctox_qwen3_embedding_0_6b::{
    artifacts, tokenizer, EmbedBatchRequest, EmbeddingBackend, Qwen3EmbeddingConfig,
    Qwen3EmbeddingModel, QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Duration;

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;

#[derive(Debug, Clone)]
pub struct NativeEmbeddingLaunch {
    pub transport: LocalTransport,
    pub compute_target: engine::ComputeTarget,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalEmbeddingRequest {
    EmbeddingsCreate { model: String, inputs: Vec<String> },
    RuntimeHealth,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalEmbeddingResponse {
    Embeddings {
        model: String,
        data: Vec<Vec<f32>>,
        prompt_tokens: u32,
        total_tokens: u32,
    },
    RuntimeHealth {
        healthy: bool,
        default_model: Option<String>,
        loaded_models: Vec<String>,
    },
    Error {
        code: String,
        message: String,
    },
}

pub fn ensure_launchable(root: &Path) -> Result<()> {
    let status = doctor_json(root);
    anyhow::ensure!(
        status["native_ctox"]["model_artifacts_present"]
            .as_bool()
            .unwrap_or(false),
        "native Qwen3 embedding model artifacts are missing"
    );
    anyhow::ensure!(
        status["native_ctox"]["model_artifact_required_tensors_present"]
            .as_bool()
            .unwrap_or(false),
        "native Qwen3 embedding required tensors are missing"
    );
    anyhow::ensure!(
        status["native_ctox"]["tokenizer_matches_model_vocab_size"]
            .as_bool()
            .unwrap_or(false),
        "native Qwen3 embedding tokenizer vocab does not match model vocab"
    );
    Ok(())
}

pub fn doctor_json(root: &Path) -> serde_json::Value {
    let model_root = root.join("src/inference/models/qwen3_embedding_0_6b");
    let discovery = artifacts::discover_model_artifacts(root);
    let found = discovery.found.as_ref();
    let inspected = found.and_then(|artifacts| artifacts::inspect_artifacts(artifacts).ok());
    let tokenizer_inspection =
        found.and_then(|artifacts| tokenizer::inspect_tokenizer(&artifacts.tokenizer_json).ok());
    let tokenizer_total = tokenizer_inspection
        .as_ref()
        .map(|inspection| inspection.total_token_slots());
    json!({
        "ok": false,
        "model": QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL,
        "native_ctox": {
            "crate_linked": true,
            "cpu_reference_ops": true,
            "metal_kernel_seed_present": model_root.join("vendor/metal/kernels/ctox_qwen3_embedding_glue.metal").is_file(),
            "cuda_kernel_seed_present": model_root.join("vendor/cuda/kernels/ctox_qwen3_embedding_glue.cu").is_file(),
            "model_artifacts_present": found.is_some(),
            "model_artifact_root": found.map(|artifacts| artifacts.root.display().to_string()),
            "model_artifact_safetensors": found.map(|artifacts| artifacts.safetensors.len()).unwrap_or(0),
            "model_artifact_safetensors_bytes": inspected.as_ref().map(|inspection| inspection.safetensors_total_bytes).unwrap_or(0),
            "model_artifact_tensor_count": inspected.as_ref().map(|inspection| inspection.safetensors_tensor_count).unwrap_or(0),
            "model_artifact_required_tensors_present": inspected.as_ref().map(|inspection| inspection.required_tensors_present).unwrap_or(false),
            "model_artifact_missing_required_tensors": inspected.as_ref().map(|inspection| inspection.missing_required_tensors.clone()).unwrap_or_default(),
            "hidden_size": inspected.as_ref().map(|inspection| inspection.model.hidden_size),
            "max_position_embeddings": inspected.as_ref().map(|inspection| inspection.model.max_position_embeddings),
            "num_hidden_layers": inspected.as_ref().map(|inspection| inspection.model.num_hidden_layers),
            "num_attention_heads": inspected.as_ref().map(|inspection| inspection.model.num_attention_heads),
            "num_key_value_heads": inspected.as_ref().map(|inspection| inspection.model.num_key_value_heads),
            "torch_dtype": inspected.as_ref().and_then(|inspection| inspection.model.torch_dtype.clone()),
            "pooling_mode": inspected.as_ref().and_then(|inspection| inspection.pooling.as_ref()).map(|pooling| if pooling.pooling_mode_lasttoken { "last_token" } else { "other" }),
            "normalize_module_present": inspected.as_ref().and_then(|inspection| inspection.pooling.as_ref()).map(|pooling| pooling.normalize_module_present).unwrap_or(false),
            "tokenizer_model_type": tokenizer_inspection.as_ref().map(|inspection| inspection.model_type.clone()),
            "tokenizer_vocab_size": tokenizer_inspection.as_ref().map(|inspection| inspection.vocab_size),
            "tokenizer_added_tokens": tokenizer_inspection.as_ref().map(|inspection| inspection.added_tokens_count),
            "tokenizer_merges": tokenizer_inspection.as_ref().map(|inspection| inspection.merges_count),
            "tokenizer_total_token_slots": tokenizer_total,
            "tokenizer_matches_model_vocab_size": inspected.as_ref().zip(tokenizer_total).map(|(inspection, slots)| inspection.model.vocab_size == slots).unwrap_or(false),
            "endoftext_token_id": tokenizer_inspection.as_ref().and_then(|inspection| inspection.token_id("<|endoftext|>")),
            "im_start_token_id": tokenizer_inspection.as_ref().and_then(|inspection| inspection.token_id("<|im_start|>")),
            "im_end_token_id": tokenizer_inspection.as_ref().and_then(|inspection| inspection.token_id("<|im_end|>")),
            "transformer_forward_wired": false
        }
    })
}

pub fn embedding_smoke_json(root: &Path, token_id: usize) -> serde_json::Value {
    let Some(artifacts) = artifacts::discover_model_artifacts(root).found else {
        return json!({"ok": false, "error": "model_artifacts_missing"});
    };
    let model = match Qwen3EmbeddingModel::from_artifacts(&artifacts, EmbeddingBackend::Cpu) {
        Ok(model) => model,
        Err(err) => return json!({"ok": false, "error": err.to_string()}),
    };
    let row = match model.token_embedding_rows(&[token_id]) {
        Ok(mut rows) => rows.pop().unwrap_or_default(),
        Err(err) => return json!({"ok": false, "error": err.to_string()}),
    };
    json!({
        "ok": true,
        "model": QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL,
        "artifact_root": artifacts.root.display().to_string(),
        "token_id": token_id,
        "embedding_dim": row.len(),
        "nonzero_values": row.iter().filter(|value| **value != 0.0).count(),
        "l2_norm": row.iter().map(|value| value * value).sum::<f32>().sqrt(),
        "sample": row.iter().take(8).copied().collect::<Vec<_>>()
    })
}

pub fn parse_embedding_smoke_token_id(args: &[String]) -> Result<usize> {
    if let Some(index) = args.iter().position(|arg| arg == "--token-id") {
        return args
            .get(index + 1)
            .context("usage: ctox runtime embedding-smoke [--token-id <id>]")?
            .parse::<usize>()
            .context("`--token-id` must be an unsigned integer");
    }
    Ok(0)
}

pub fn serve_socket(launch: NativeEmbeddingLaunch) -> Result<()> {
    let backend = match launch.compute_target {
        engine::ComputeTarget::Cpu => EmbeddingBackend::Cpu,
        engine::ComputeTarget::Gpu => {
            #[cfg(target_os = "macos")]
            {
                EmbeddingBackend::Metal
            }
            #[cfg(target_os = "linux")]
            {
                EmbeddingBackend::Cuda
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                EmbeddingBackend::Cpu
            }
        }
    };
    let model = match artifacts::discover_model_artifacts(&std::env::current_dir()?).found {
        Some(artifacts) => Qwen3EmbeddingModel::from_artifacts(&artifacts, backend)
            .map_err(|err| anyhow::anyhow!(err.to_string()))?,
        None => Qwen3EmbeddingModel::new(Qwen3EmbeddingConfig::default(), backend),
    };
    let mut listener = launch.transport.bind()?;
    loop {
        let stream = listener.accept()?;
        let model = model.clone();
        std::thread::spawn(move || {
            let _ = handle_connection(stream, model);
        });
    }
}

fn handle_connection(
    mut stream: crate::inference::local_transport::LocalStream,
    model: Qwen3EmbeddingModel,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let response = match serde_json::from_str::<LocalEmbeddingRequest>(line.trim()) {
        Ok(LocalEmbeddingRequest::RuntimeHealth) => LocalEmbeddingResponse::RuntimeHealth {
            healthy: false,
            default_model: Some(model.config().model.clone()),
            loaded_models: vec![model.config().model.clone()],
        },
        Ok(LocalEmbeddingRequest::EmbeddingsCreate {
            model: request_model,
            inputs,
        }) => {
            if request_model != QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL {
                LocalEmbeddingResponse::Error {
                    code: "unsupported_model".to_string(),
                    message: format!(
                        "native embedding service only supports {QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL}"
                    ),
                }
            } else {
                match model.embed_batch(&EmbedBatchRequest { inputs: &inputs }) {
                    Ok(output) => LocalEmbeddingResponse::Embeddings {
                        model: output.model,
                        data: output.embeddings,
                        prompt_tokens: 0,
                        total_tokens: 0,
                    },
                    Err(err) => LocalEmbeddingResponse::Error {
                        code: "embedding_failed".to_string(),
                        message: err.to_string(),
                    },
                }
            }
        }
        Err(err) => LocalEmbeddingResponse::Error {
            code: "invalid_request".to_string(),
            message: err.to_string(),
        },
    };
    let mut payload = serde_json::to_vec(&response)?;
    payload.push(b'\n');
    stream.write_all(&payload)?;
    stream.flush()?;
    Ok(())
}
