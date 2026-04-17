// Origin: CTOX
// License: Apache-2.0
//
// Vision preprocessor — core of the CTOX vision path.
//
// The tools carried by `view_image` (and any future screenshot / OCR tool)
// can emit `input_image` content blocks. The primary LLM may or may not
// accept those natively — OpenAI's gpt-4o, Anthropic's Claude 3/4, local
// Qwen 3.5-VL and Gemma-4 families can; GPT-OSS, Kimi, Nemotron-Cascade,
// GLM-4.7, MiniMax text-only cannot.
//
// To keep the guarantee "tools can always evaluate images", this module
// intercepts every `/v1/responses` POST payload before adapter dispatch
// and — when the primary model can't natively consume images — describes
// each image using the configured Vision aux model (Qwen3-VL-2B-Instruct
// by default) and replaces the image block with a plain text block:
//
//   {"type":"input_text","text":"[Image description (via aux-vision): ...]"}
//
// When the primary model CAN see images, the preprocessor is a no-op and
// the adapter is responsible for forwarding the image block unchanged.
//
// The aux endpoint is called synchronously via `ureq` (matching the rest
// of the gateway's HTTP style). On aux-failure the image block is replaced
// with a clear error text so the primary model is told "vision aux not
// available", rather than silently receiving a missing image (which would
// encourage hallucination).

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine as _;
use regex::Regex;
use serde_json::{json, Value};

use crate::inference::engine;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;

/// Marker used by the TUI (and any other CTOX-side client) to refer to a
/// local image file inside a user-message text block. The gateway-side
/// vision preprocessor expands these markers into real `input_image`
/// content blocks before running the capability-based image routing.
///
///   [[ctox:image:/absolute/path/to/file.png]]
///
/// Only absolute paths are accepted — paths are expanded against the
/// CTOX workspace root so relative inputs would be ambiguous across
/// tools and the service loop.
const CTOX_IMAGE_MARKER_PREFIX: &str = "[[ctox:image:";
const CTOX_IMAGE_MARKER_SUFFIX: &str = "]]";

/// Build the canonical `[[ctox:image:/absolute/path]]` marker for a given
/// file path. TUI / CLI / any other CTOX-side client should use this
/// helper instead of string-concatenating the marker themselves, so the
/// format stays consistent with the gateway-side expander.
pub fn encode_image_marker(path: &Path) -> String {
    format!(
        "{}{}{}",
        CTOX_IMAGE_MARKER_PREFIX,
        path.display(),
        CTOX_IMAGE_MARKER_SUFFIX
    )
}

/// Max bytes we'll inline as base64 per image. 20 MB — generous for
/// screenshots / photos while preventing runaway payloads.
const MAX_INLINE_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Environment variable that disables vision preprocessing entirely.
/// When set to a truthy value, image blocks are passed through unchanged
/// and no aux call is made.
pub const VISION_AUX_DISABLE_ENV: &str = "CTOX_DISABLE_VISION_BACKEND";

/// Default describer prompt sent alongside each image to the aux model.
const DEFAULT_DESCRIBE_PROMPT: &str =
    "Describe this image in detail. Include visible text (transcribed verbatim), \
    objects and their spatial arrangement, colors, any charts or diagrams with \
    their data, and the overall context. Be thorough — this description replaces \
    the image for a downstream model that cannot see it.";

/// HTTP timeout for the aux describer call. Vision describe for a 2B model
/// on a single GPU typically returns in 2–8 seconds; 60s keeps headroom
/// for cold-starts and larger images.
const AUX_HTTP_TIMEOUT_SECS: u64 = 60;

/// Preprocess the `input` array of a `/v1/responses` POST payload in place.
///
/// Walks every `message.content[]` and every `function_call_output.output[]`
/// looking for `input_image` / `image_url` blocks. For each found image,
/// if `model_supports_vision_primary` is false, calls the aux endpoint,
/// inserts a text block describing the image, and drops the image block.
/// If the primary model supports vision, the image block is left untouched.
///
/// Returns `Ok(true)` if the payload was mutated, `Ok(false)` if untouched.
pub fn preprocess_responses_payload(
    root: &Path,
    primary_model: Option<&str>,
    payload: &mut Value,
) -> Result<bool> {
    // Kill-switch
    if is_vision_preprocessor_disabled(root) {
        return Ok(false);
    }

    let Some(input) = payload.get_mut("input").and_then(Value::as_array_mut) else {
        return Ok(false);
    };

    // First pass: expand any `[[ctox:image:/path]]` markers inside user
    // text blocks into real `input_image` blocks. This runs unconditionally
    // (before the capability check) so the subsequent image-describe vs
    // passthrough decision treats TUI-attached images exactly like
    // user-API-supplied ones.
    let mut mutated = false;
    for item in input.iter_mut() {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        let item_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("message");
        if item_type != "message" {
            continue;
        }
        if let Some(Value::Array(content)) = object.get_mut("content") {
            if expand_ctox_image_markers(content)? {
                mutated = true;
            }
        }
    }

    // Capability check — if the primary model can already see images,
    // no-op. Resolution is delegated to the central registry lookup
    // (`engine::model_supports_vision`) so this path stays in sync with
    // the SUPPORTED_VISION_MODELS / VISION_API_MODELS / ChatFamilyCatalog
    // data sources without duplicating them.
    let primary_supports_vision = primary_model
        .map(engine::model_supports_vision)
        .unwrap_or(false);

    if primary_supports_vision {
        // Marker-expansion may have already mutated the payload; report it.
        return Ok(mutated);
    }

    let aux_endpoint = match resolve_vision_aux_endpoint(root) {
        Some(endpoint) => endpoint,
        None => {
            // No aux configured and no primary vision — replace images with
            // structured error so the primary model is told explicitly.
            return replace_images_with_error(
                input,
                "vision aux model not configured (set CTOX_VISION_BASE_URL or \
                enable the Qwen3-VL-2B-Instruct aux in settings)",
            );
        }
    };

    for item in input.iter_mut() {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        let item_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("message");

        match item_type {
            "message" => {
                if let Some(Value::Array(content)) = object.get_mut("content") {
                    if describe_images_in_content(content, &aux_endpoint)? {
                        mutated = true;
                    }
                }
            }
            "function_call_output" => {
                // Tool outputs can be either a plain string or an array.
                // `view_image` uses the array form with InputImage entries.
                if let Some(Value::Array(output)) = object.get_mut("output") {
                    if describe_images_in_content(output, &aux_endpoint)? {
                        mutated = true;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(mutated)
}

/// Scan every text block in `content` for `[[ctox:image:/path]]` markers
/// and replace each marker with a real `input_image` block (base64 data
/// URI). A single text block can contain multiple markers interleaved with
/// narrative text — in that case the text is split around each marker and
/// the resulting text fragments are kept as their own text blocks.
///
/// Returns true if at least one marker was expanded.
pub fn expand_ctox_image_markers(content: &mut Vec<Value>) -> Result<bool> {
    let re = Regex::new(r"\[\[ctox:image:([^\]]+)\]\]")
        .context("failed to compile ctox image-marker regex")?;

    let mut new_content: Vec<Value> = Vec::with_capacity(content.len());
    let mut mutated = false;

    for entry in content.drain(..) {
        let is_text = entry
            .get("type")
            .and_then(Value::as_str)
            .map(|t| t == "input_text" || t == "text")
            .unwrap_or(false);
        if !is_text {
            new_content.push(entry);
            continue;
        }
        let text_value = entry.get("text").and_then(Value::as_str).unwrap_or("");
        if !text_value.contains(CTOX_IMAGE_MARKER_PREFIX) {
            new_content.push(entry);
            continue;
        }

        // Split around markers, emitting text/image blocks alternately.
        let mut cursor = 0usize;
        let mut emitted_any_image = false;
        for m in re.find_iter(text_value) {
            let before = &text_value[cursor..m.start()];
            if !before.trim().is_empty() {
                new_content.push(json!({"type":"input_text","text":before}));
            }
            let path_segment = &text_value[m.start() + CTOX_IMAGE_MARKER_PREFIX.len()
                ..m.end() - CTOX_IMAGE_MARKER_SUFFIX.len()];
            match load_image_as_data_uri(path_segment) {
                Ok(data_uri) => {
                    new_content.push(json!({
                        "type": "input_image",
                        "image_url": data_uri,
                    }));
                    emitted_any_image = true;
                }
                Err(err) => {
                    new_content.push(json!({
                        "type": "input_text",
                        "text": format!(
                            "[Image attachment failed ({}): {err}]",
                            path_segment.trim()
                        ),
                    }));
                }
            }
            cursor = m.end();
        }
        let tail = &text_value[cursor..];
        if !tail.trim().is_empty() {
            new_content.push(json!({"type":"input_text","text":tail}));
        }
        if emitted_any_image {
            mutated = true;
        }
    }

    *content = new_content;
    Ok(mutated)
}

fn load_image_as_data_uri(path_literal: &str) -> Result<String> {
    let path = PathBuf::from(path_literal.trim());
    if !path.is_absolute() {
        anyhow::bail!("ctox image marker path must be absolute");
    }
    let metadata = std::fs::metadata(&path)
        .with_context(|| format!("cannot stat image {}", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!("ctox image marker target is not a file: {}", path.display());
    }
    if metadata.len() > MAX_INLINE_IMAGE_BYTES {
        anyhow::bail!(
            "image {} exceeds max inline size ({} MB)",
            path.display(),
            MAX_INLINE_IMAGE_BYTES / (1024 * 1024)
        );
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read image {}", path.display()))?;
    let mime = guess_image_mime_type(&path);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

fn guess_image_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("tif") | Some("tiff") => "image/tiff",
        Some("heic") => "image/heic",
        Some("heif") => "image/heif",
        _ => "application/octet-stream",
    }
}

fn is_vision_preprocessor_disabled(root: &Path) -> bool {
    runtime_env::env_or_config(root, VISION_AUX_DISABLE_ENV)
        .map(|value| {
            let trimmed = value.trim();
            matches!(
                trimmed,
                "1" | "true" | "TRUE" | "True" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

/// Resolve the vision aux endpoint from either an explicit env override or
/// the managed InferenceRuntimeKernel binding for the Vision role.
fn resolve_vision_aux_endpoint(root: &Path) -> Option<VisionAuxEndpoint> {
    if let Some(base_url) = runtime_env::env_or_config(root, "CTOX_VISION_BASE_URL") {
        let base = base_url.trim().trim_end_matches('/');
        if !base.is_empty() {
            let model = runtime_env::env_or_config(root, "CTOX_VISION_MODEL").unwrap_or_else(|| {
                // Fall back to the registry's default-for-role Vision aux
                // selection so the model name never leaks into this module
                // as a hardcoded literal.
                engine::auxiliary_model_selection(engine::AuxiliaryRole::Vision, None)
                    .request_model
                    .to_string()
            });
            return Some(VisionAuxEndpoint {
                chat_completions_url: format!("{base}/v1/chat/completions"),
                model,
            });
        }
    }

    // Fall back to the resolved runtime kernel binding (Phase C plumbing).
    let kernel = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok()?;
    let binding = kernel.binding_for_auxiliary_role(engine::AuxiliaryRole::Vision)?;
    let base_url = binding.base_url.trim().trim_end_matches('/').to_string();
    if base_url.is_empty() {
        return None;
    }
    Some(VisionAuxEndpoint {
        chat_completions_url: format!("{base_url}/v1/chat/completions"),
        model: binding.request_model.clone(),
    })
}

struct VisionAuxEndpoint {
    chat_completions_url: String,
    model: String,
}

/// Walk a content array, describe each image block via the aux, and
/// replace the image block with a text block. Returns true if any
/// replacements were made.
fn describe_images_in_content(content: &mut Vec<Value>, aux: &VisionAuxEndpoint) -> Result<bool> {
    let mut mutated = false;
    let mut i = 0;
    while i < content.len() {
        let is_image = content[i]
            .get("type")
            .and_then(Value::as_str)
            .map(|t| t == "input_image" || t == "image_url")
            .unwrap_or(false);
        if !is_image {
            i += 1;
            continue;
        }
        let image_ref = extract_image_reference(&content[i]);
        let description = match image_ref {
            Some(img_ref) => describe_image_via_aux(aux, &img_ref).unwrap_or_else(|err| {
                format!(
                    "[Vision aux describe failed: {err}. Image omitted from context.]"
                )
            }),
            None => "[Image block had no parseable URL or data payload — omitted.]".to_string(),
        };
        content[i] = json!({
            "type": "input_text",
            "text": format!("[Image description (via aux-vision): {description}]"),
        });
        mutated = true;
        i += 1;
    }
    Ok(mutated)
}

/// Extract the canonical image reference (URL or data-URI) from either the
/// OpenResponses-native shape or the OpenAI chat-compat shape.
fn extract_image_reference(block: &Value) -> Option<String> {
    let block = block.as_object()?;
    // OpenResponses-native: {type:"input_image", image_url:"...", image_data:"<base64>"}
    if let Some(url) = block.get("image_url").and_then(Value::as_str) {
        if !url.trim().is_empty() {
            return Some(url.to_string());
        }
    }
    if let Some(data) = block.get("image_data").and_then(Value::as_str) {
        if !data.trim().is_empty() {
            let mime = block
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("image/png");
            return Some(format!("data:{mime};base64,{data}"));
        }
    }
    // OpenAI chat-compat: {type:"image_url", image_url:{url:"..."}}
    if let Some(inner) = block.get("image_url").and_then(Value::as_object) {
        if let Some(url) = inner.get("url").and_then(Value::as_str) {
            if !url.trim().is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Call the aux describer with a single image + describe prompt.
fn describe_image_via_aux(aux: &VisionAuxEndpoint, image_ref: &str) -> Result<String> {
    let body = json!({
        "model": aux.model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image_url", "image_url": {"url": image_ref}},
                {"type": "text", "text": DEFAULT_DESCRIBE_PROMPT},
            ],
        }],
        "max_tokens": 768,
        "temperature": 0.1,
    });

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(AUX_HTTP_TIMEOUT_SECS))
        .timeout_write(Duration::from_secs(10))
        .build();

    let serialized = serde_json::to_string(&body)
        .context("failed to serialize vision aux describe body")?;
    let response = agent
        .post(&aux.chat_completions_url)
        .set("content-type", "application/json")
        .send_string(&serialized)
        .context("vision aux HTTP call failed")?;

    let body_text = response
        .into_string()
        .context("vision aux response body could not be read")?;
    let parsed: Value =
        serde_json::from_str(&body_text).context("vision aux response was not valid JSON")?;

    let choices = parsed
        .get("choices")
        .and_then(Value::as_array)
        .context("vision aux response has no `choices` array")?;
    let first = choices
        .first()
        .context("vision aux response `choices` array is empty")?;
    let content = first
        .get("message")
        .and_then(|msg| msg.get("content"))
        .and_then(Value::as_str)
        .context("vision aux response has no `message.content` string")?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        anyhow::bail!("vision aux returned empty content");
    }
    Ok(trimmed)
}

/// When no aux is available, replace every image block with a clear error
/// text so the primary model is explicitly told what happened (rather than
/// silently losing images and potentially hallucinating).
fn replace_images_with_error(input: &mut Vec<Value>, reason: &str) -> Result<bool> {
    let mut mutated = false;
    for item in input.iter_mut() {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        let item_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("message");
        let array_field = match item_type {
            "message" => "content",
            "function_call_output" => "output",
            _ => continue,
        };
        if let Some(Value::Array(content)) = object.get_mut(array_field) {
            let mut i = 0;
            while i < content.len() {
                let is_image = content[i]
                    .get("type")
                    .and_then(Value::as_str)
                    .map(|t| t == "input_image" || t == "image_url")
                    .unwrap_or(false);
                if is_image {
                    content[i] = json!({
                        "type": "input_text",
                        "text": format!(
                            "[Vision aux unavailable: {reason}. Image omitted from context.]"
                        ),
                    });
                    mutated = true;
                }
                i += 1;
            }
        }
    }
    Ok(mutated)
}

// Resolve primary model from runtime state when not supplied by caller.
#[allow(dead_code)]
pub fn resolve_primary_model(root: &Path) -> Option<String> {
    runtime_state::load_or_resolve_runtime_state(root)
        .ok()
        .and_then(|state| {
            state
                .active_or_selected_model()
                .map(ToOwned::to_owned)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_image_reference_handles_native_url() {
        let block = json!({
            "type": "input_image",
            "image_url": "https://example.com/a.png",
        });
        assert_eq!(
            extract_image_reference(&block),
            Some("https://example.com/a.png".to_string())
        );
    }

    #[test]
    fn extract_image_reference_handles_base64_data() {
        let block = json!({
            "type": "input_image",
            "image_data": "AAAA",
            "mime_type": "image/jpeg",
        });
        assert_eq!(
            extract_image_reference(&block),
            Some("data:image/jpeg;base64,AAAA".to_string())
        );
    }

    #[test]
    fn extract_image_reference_handles_openai_compat_shape() {
        let block = json!({
            "type": "image_url",
            "image_url": {"url": "https://example.com/b.webp"},
        });
        assert_eq!(
            extract_image_reference(&block),
            Some("https://example.com/b.webp".to_string())
        );
    }

    #[test]
    fn replace_images_with_error_replaces_message_content_images() {
        let mut input = vec![json!({
            "type": "message",
            "role": "user",
            "content": [
                {"type": "input_text", "text": "describe this"},
                {"type": "input_image", "image_url": "https://example.com/x.png"},
            ],
        })];
        let mutated = replace_images_with_error(&mut input, "testing").unwrap();
        assert!(mutated);
        let content = input[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "input_text");
        assert_eq!(content[1]["type"], "input_text");
        assert!(content[1]["text"]
            .as_str()
            .unwrap()
            .contains("Vision aux unavailable"));
    }

    #[test]
    fn replace_images_with_error_replaces_tool_output_images() {
        let mut input = vec![json!({
            "type": "function_call_output",
            "call_id": "call_1",
            "output": [
                {"type": "input_image", "image_url": "https://example.com/y.png"},
            ],
        })];
        let mutated = replace_images_with_error(&mut input, "no aux").unwrap();
        assert!(mutated);
        let output = input[0]["output"].as_array().unwrap();
        assert_eq!(output[0]["type"], "input_text");
    }

    #[test]
    fn known_api_vision_models_are_flagged() {
        assert!(engine::model_supports_vision("anthropic/claude-sonnet-4.6"));
        assert!(engine::model_supports_vision("gpt-5.4"));
        assert!(!engine::model_supports_vision("openai/gpt-oss-20b"));
        assert!(!engine::model_supports_vision("moonshotai/kimi-k2.5"));
    }

    #[test]
    fn encode_image_marker_roundtrips() {
        let path = PathBuf::from("/tmp/foo.png");
        let marker = encode_image_marker(&path);
        assert_eq!(marker, "[[ctox:image:/tmp/foo.png]]");
    }

    #[test]
    fn expand_markers_reports_missing_file_as_text_block() {
        let mut content = vec![json!({
            "type": "input_text",
            "text": "Before [[ctox:image:/nonexistent/abc.png]] after.",
        })];
        let mutated = expand_ctox_image_markers(&mut content).unwrap();
        // File is missing => no image block emitted; mutated=false.
        assert!(!mutated);
        // Original text is split into before + error + after.
        assert!(content.len() >= 2);
        let has_error = content.iter().any(|b| {
            b.get("text")
                .and_then(Value::as_str)
                .map(|t| t.contains("Image attachment failed"))
                .unwrap_or(false)
        });
        assert!(has_error);
    }

    #[test]
    fn expand_markers_rejects_relative_paths() {
        let mut content = vec![json!({
            "type": "input_text",
            "text": "See [[ctox:image:./relative.png]].",
        })];
        let _ = expand_ctox_image_markers(&mut content).unwrap();
        // Relative path => load_image_as_data_uri errs => placed as text.
        let has_error = content.iter().any(|b| {
            b.get("text")
                .and_then(Value::as_str)
                .map(|t| t.contains("must be absolute"))
                .unwrap_or(false)
        });
        assert!(has_error);
    }

    #[test]
    fn expand_markers_inlines_png_file() {
        let tmp = std::env::temp_dir().join("ctox_vision_marker_test.png");
        // Minimal 1x1 transparent PNG
        let png_bytes: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(&tmp, png_bytes).unwrap();
        let marker = encode_image_marker(&tmp);
        let mut content = vec![json!({
            "type": "input_text",
            "text": format!("describe: {marker}"),
        })];
        let mutated = expand_ctox_image_markers(&mut content).unwrap();
        assert!(mutated);
        // Expect: text("describe: ") + input_image
        assert!(content.iter().any(|b| b["type"] == "input_image"));
        let _ = std::fs::remove_file(tmp);
    }
}
