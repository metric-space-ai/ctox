use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Duration;

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::model_registry;
use crate::inference::runtime_kernel;
use crate::inference::supervisor;

const LOCAL_EMBEDDING_TIMEOUT_BASE_SECS: u64 = 30;
const LOCAL_EMBEDDING_TIMEOUT_PER_INPUT_SECS: u64 = 15;
const LOCAL_EMBEDDING_TIMEOUT_MAX_SECS: u64 = 300;

pub fn handle_doc_command(root: &Path, args: &[String]) -> Result<()> {
    ctox_doc_stack::handle_doc_command(root, args, &CtoxDocEmbeddingExecutor)
}

struct CtoxDocEmbeddingExecutor;

impl ctox_doc_stack::EmbeddingExecutor for CtoxDocEmbeddingExecutor {
    fn default_model(&self, root: &Path) -> Result<String> {
        supervisor::ensure_auxiliary_backend_launchable(root, engine::AuxiliaryRole::Embedding)
            .context("embedding backend is not launchable for document retrieval")?;
        supervisor::ensure_auxiliary_backend_ready(root, engine::AuxiliaryRole::Embedding, false)
            .context("failed to ensure managed embedding backend for document retrieval")?;
        Ok(
            model_registry::default_auxiliary_model(engine::AuxiliaryRole::Embedding)
                .expect("default embedding model must exist in the model registry")
                .to_string(),
        )
    }

    fn embed_texts(&self, root: &Path, model: &str, inputs: &[String]) -> Result<Vec<Vec<f64>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        supervisor::ensure_auxiliary_backend_launchable(root, engine::AuxiliaryRole::Embedding)
            .context("embedding backend is not launchable for document retrieval")?;
        supervisor::ensure_auxiliary_backend_ready(root, engine::AuxiliaryRole::Embedding, false)
            .context("failed to ensure managed embedding backend for document retrieval")?;
        let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root)
            .context("failed to resolve runtime kernel for document retrieval")?;

        if let Some(binding) =
            resolved_runtime.binding_for_auxiliary_role(engine::AuxiliaryRole::Embedding)
        {
            match &binding.transport {
                LocalTransport::UnixSocket { .. } | LocalTransport::NamedPipe { .. } => {
                    return embed_texts_via_local_socket(&binding.transport, inputs, model)
                        .with_context(|| {
                            format!(
                                "failed to reach embedding transport for local documents at {}",
                                binding.transport.display_label()
                            )
                        });
                }
                LocalTransport::TcpLoopback { .. } => {
                    // fall through to HTTP path using binding.base_url
                }
            }
        }

        let base_url = resolved_runtime
            .auxiliary_base_url(engine::AuxiliaryRole::Embedding)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("embedding runtime is not resolved"))?;
        let timeout_secs = embedding_request_timeout_secs(inputs);
        let response = ureq::post(&format!("{}/v1/embeddings", base_url.trim_end_matches('/')))
            .set("content-type", "application/json")
            .timeout(Duration::from_secs(timeout_secs))
            .send_string(&serde_json::to_string(&json!({
                "model": model,
                "input": inputs,
            }))?)
            .with_context(|| format!("failed to reach embedding service at {}", base_url))?;
        let body = response
            .into_string()
            .context("failed to read embedding response")?;
        let payload: Value =
            serde_json::from_str(&body).context("failed to parse embedding response")?;
        let mut indexed = payload
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        indexed.sort_by_key(|item| item.get("index").and_then(Value::as_u64).unwrap_or(0));
        let vectors = indexed
            .into_iter()
            .map(|item| {
                item.get("embedding")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
                    .filter(|items| !items.is_empty())
                    .context("embedding response missing vectors")
            })
            .collect::<Result<Vec<_>>>()?;
        if vectors.len() != inputs.len() {
            anyhow::bail!(
                "embedding response count mismatch: expected {}, got {}",
                inputs.len(),
                vectors.len()
            );
        }
        Ok(vectors)
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalEmbeddingSocketRequest<'a> {
    EmbeddingsCreate {
        model: &'a str,
        inputs: &'a [String],
        truncate_sequence: bool,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalEmbeddingSocketResponse {
    Embeddings {
        model: String,
        data: Vec<Vec<f32>>,
        #[serde(rename = "prompt_tokens")]
        _prompt_tokens: u32,
        #[serde(rename = "total_tokens")]
        _total_tokens: u32,
    },
    Error {
        code: String,
        message: String,
    },
}

fn embed_texts_via_local_socket(
    transport: &LocalTransport,
    inputs: &[String],
    model: &str,
) -> Result<Vec<Vec<f64>>> {
    let timeout_secs = embedding_request_timeout_secs(inputs);
    let label = transport.display_label();
    let mut stream = transport
        .connect_blocking(Duration::from_secs(timeout_secs))
        .with_context(|| format!("failed to connect via {label}"))?;

    let request = LocalEmbeddingSocketRequest::EmbeddingsCreate {
        model,
        inputs,
        truncate_sequence: false,
    };
    let mut payload =
        serde_json::to_vec(&request).context("failed to encode local embedding socket request")?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .with_context(|| format!("failed to write request via {label}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush request via {label}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .with_context(|| format!("failed to read response via {label}"))?;
    if line.trim().is_empty() {
        anyhow::bail!("embedding socket returned an empty response");
    }
    match serde_json::from_str::<LocalEmbeddingSocketResponse>(line.trim())
        .context("failed to parse embedding socket response")?
    {
        LocalEmbeddingSocketResponse::Embeddings {
            model: response_model,
            data,
            _prompt_tokens: _,
            _total_tokens: _,
        } => {
            let _ = response_model;
            Ok(data
                .into_iter()
                .map(|values| values.into_iter().map(|value| value as f64).collect())
                .collect())
        }
        LocalEmbeddingSocketResponse::Error { code, message } => {
            anyhow::bail!("{code}: {message}");
        }
    }
}

fn embedding_request_timeout_secs(inputs: &[String]) -> u64 {
    let per_input = (inputs.len() as u64).saturating_mul(LOCAL_EMBEDDING_TIMEOUT_PER_INPUT_SECS);
    let total_chars = inputs
        .iter()
        .map(|value| value.chars().count() as u64)
        .sum::<u64>();
    let char_budget = total_chars / 2_000;
    (LOCAL_EMBEDDING_TIMEOUT_BASE_SECS + per_input + char_budget)
        .min(LOCAL_EMBEDDING_TIMEOUT_MAX_SECS)
        .max(LOCAL_EMBEDDING_TIMEOUT_BASE_SECS)
}
