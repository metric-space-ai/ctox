use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelArtifacts {
    pub root: PathBuf,
    pub config_json: PathBuf,
    pub tokenizer_json: PathBuf,
    pub modules_json: Option<PathBuf>,
    pub pooling_config_json: Option<PathBuf>,
    pub safetensors: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDiscovery {
    pub searched: Vec<PathBuf>,
    pub found: Option<ModelArtifacts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qwen3ModelSpec {
    pub model_type: String,
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub max_position_embeddings: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub head_dim: usize,
    pub vocab_size: usize,
    pub torch_dtype: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolingSpec {
    pub word_embedding_dimension: usize,
    pub pooling_mode_lasttoken: bool,
    pub normalize_module_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactInspection {
    pub model: Qwen3ModelSpec,
    pub pooling: Option<PoolingSpec>,
    pub safetensors_count: usize,
    pub safetensors_total_bytes: u64,
    pub safetensors_tensor_count: usize,
    pub required_tensors_present: bool,
    pub missing_required_tensors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorInfo {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub data_offsets: (u64, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetensorsHeader {
    pub data_start: u64,
    pub tensors: Vec<TensorInfo>,
}

pub fn discover_model_artifacts(ctox_root: &Path) -> ArtifactDiscovery {
    let mut searched = candidate_model_dirs(ctox_root);
    dedupe_paths(&mut searched);
    let found = searched.iter().find_map(|dir| artifacts_at(dir));
    ArtifactDiscovery { searched, found }
}

pub fn artifacts_at(dir: &Path) -> Option<ModelArtifacts> {
    let config_json = dir.join("config.json");
    let tokenizer_json = dir.join("tokenizer.json");
    let modules_json = dir
        .join("modules.json")
        .is_file()
        .then(|| dir.join("modules.json"));
    let pooling_config_json = dir
        .join("1_Pooling/config.json")
        .is_file()
        .then(|| dir.join("1_Pooling/config.json"));
    let mut safetensors = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name == "model.safetensors"
                || (name.starts_with("model-") && name.ends_with(".safetensors"))
            {
                safetensors.push(path);
            }
        }
    }
    safetensors.sort();

    (config_json.is_file() && tokenizer_json.is_file() && !safetensors.is_empty()).then(|| {
        ModelArtifacts {
            root: dir.to_path_buf(),
            config_json,
            tokenizer_json,
            modules_json,
            pooling_config_json,
            safetensors,
        }
    })
}

pub fn inspect_artifacts(artifacts: &ModelArtifacts) -> Result<ArtifactInspection, String> {
    let config = std::fs::read_to_string(&artifacts.config_json)
        .map_err(|err| format!("failed to read {}: {err}", artifacts.config_json.display()))?;
    let model = parse_qwen3_config(&config)?;
    let pooling = match artifacts.pooling_config_json.as_ref() {
        Some(path) => {
            let raw = std::fs::read_to_string(path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            Some(parse_pooling_config(
                &raw,
                artifacts
                    .modules_json
                    .as_ref()
                    .and_then(|path| std::fs::read_to_string(path).ok())
                    .as_deref(),
            )?)
        }
        None => None,
    };
    if model.model_type != "qwen3" {
        return Err(format!(
            "expected model_type qwen3, got {}",
            model.model_type
        ));
    }
    if model.hidden_size == 0
        || model.max_position_embeddings == 0
        || model.num_hidden_layers == 0
        || model.num_attention_heads == 0
        || model.num_key_value_heads == 0
    {
        return Err("Qwen3 config contains zero-sized dimensions".to_string());
    }
    if let Some(pooling) = pooling.as_ref() {
        if pooling.word_embedding_dimension != model.hidden_size {
            return Err(format!(
                "pooling dimension {} does not match hidden_size {}",
                pooling.word_embedding_dimension, model.hidden_size
            ));
        }
        if !pooling.pooling_mode_lasttoken {
            return Err("Qwen3-Embedding pooling config is not last-token pooling".to_string());
        }
    }

    let mut safetensors_total_bytes = 0_u64;
    let mut tensors = Vec::new();
    for path in &artifacts.safetensors {
        safetensors_total_bytes = safetensors_total_bytes.saturating_add(
            std::fs::metadata(path)
                .map_err(|err| format!("failed to stat {}: {err}", path.display()))?
                .len(),
        );
        tensors.extend(read_safetensors_header(path)?.tensors);
    }
    let missing_required_tensors = required_qwen3_embedding_tensors(&model)
        .into_iter()
        .filter(|required| !tensors.iter().any(|tensor| tensor.name == *required))
        .collect::<Vec<_>>();
    let required_tensors_present = missing_required_tensors.is_empty();

    Ok(ArtifactInspection {
        model,
        pooling,
        safetensors_count: artifacts.safetensors.len(),
        safetensors_total_bytes,
        safetensors_tensor_count: tensors.len(),
        required_tensors_present,
        missing_required_tensors,
    })
}

pub fn read_safetensors_header(path: &Path) -> Result<SafetensorsHeader, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let mut header_len_bytes = [0_u8; 8];
    use std::io::Read;
    file.read_exact(&mut header_len_bytes)
        .map_err(|err| format!("failed to read safetensors header length: {err}"))?;
    let header_len = u64::from_le_bytes(header_len_bytes);
    if header_len > 64 * 1024 * 1024 {
        return Err(format!(
            "safetensors header is too large: {header_len} bytes"
        ));
    }
    let mut header = vec![0_u8; header_len as usize];
    file.read_exact(&mut header)
        .map_err(|err| format!("failed to read safetensors header: {err}"))?;
    let raw = std::str::from_utf8(&header)
        .map_err(|err| format!("safetensors header is not UTF-8: {err}"))?;
    let mut parsed = parse_safetensors_header(raw)?;
    parsed.data_start = 8 + header_len;
    Ok(parsed)
}

pub fn read_safetensors_tensor(
    path: &Path,
    tensor_name: &str,
) -> Result<(TensorInfo, Vec<u8>), String> {
    let header = read_safetensors_header(path)?;
    let tensor = header
        .tensors
        .into_iter()
        .find(|tensor| tensor.name == tensor_name)
        .ok_or_else(|| format!("tensor `{tensor_name}` not found in {}", path.display()))?;
    if tensor.data_offsets.1 < tensor.data_offsets.0 {
        return Err(format!("tensor `{tensor_name}` has invalid data offsets"));
    }
    let len = tensor.data_offsets.1 - tensor.data_offsets.0;
    if len > usize::MAX as u64 {
        return Err(format!(
            "tensor `{tensor_name}` is too large to read on this host"
        ));
    }
    let mut file = std::fs::File::open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(header.data_start + tensor.data_offsets.0))
        .map_err(|err| format!("failed to seek tensor `{tensor_name}`: {err}"))?;
    let mut data = vec![0_u8; len as usize];
    file.read_exact(&mut data)
        .map_err(|err| format!("failed to read tensor `{tensor_name}` payload: {err}"))?;
    Ok((tensor, data))
}

fn required_qwen3_embedding_tensors(model: &Qwen3ModelSpec) -> Vec<String> {
    let last_layer = model.num_hidden_layers.saturating_sub(1);
    [
        "embed_tokens.weight".to_string(),
        "layers.0.input_layernorm.weight".to_string(),
        "layers.0.self_attn.q_proj.weight".to_string(),
        "layers.0.self_attn.k_proj.weight".to_string(),
        "layers.0.self_attn.v_proj.weight".to_string(),
        "layers.0.self_attn.o_proj.weight".to_string(),
        "layers.0.mlp.gate_proj.weight".to_string(),
        "layers.0.mlp.up_proj.weight".to_string(),
        "layers.0.mlp.down_proj.weight".to_string(),
        format!("layers.{last_layer}.input_layernorm.weight"),
        format!("layers.{last_layer}.self_attn.q_proj.weight"),
        format!("layers.{last_layer}.mlp.down_proj.weight"),
        "norm.weight".to_string(),
    ]
    .into_iter()
    .collect()
}

fn parse_safetensors_header(raw: &str) -> Result<SafetensorsHeader, String> {
    let bytes = raw.as_bytes();
    let mut index = skip_ws(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return Err("safetensors header is not a JSON object".to_string());
    }
    index += 1;
    let mut tensors = Vec::new();
    loop {
        index = skip_ws(bytes, index);
        match bytes.get(index) {
            Some(b'}') => break,
            Some(b',') => {
                index += 1;
                continue;
            }
            Some(b'"') => {}
            _ => return Err("invalid safetensors header entry".to_string()),
        }
        let (name, next) = parse_json_string_at(bytes, index)?;
        index = skip_ws(bytes, next);
        if bytes.get(index) != Some(&b':') {
            return Err(format!("missing colon after tensor key `{name}`"));
        }
        index = skip_ws(bytes, index + 1);
        if name == "__metadata__" {
            index = skip_json_value(bytes, index)?;
            continue;
        }
        if bytes.get(index) != Some(&b'{') {
            return Err(format!("tensor `{name}` entry is not an object"));
        }
        let end = matching_brace(bytes, index)?;
        let object = &raw[index..=end];
        tensors.push(TensorInfo {
            name,
            dtype: json_string_field(object, "dtype")?,
            shape: json_usize_array_field(object, "shape")?,
            data_offsets: json_u64_pair_field(object, "data_offsets")?,
        });
        index = end + 1;
    }
    Ok(SafetensorsHeader {
        data_start: 0,
        tensors,
    })
}

fn parse_qwen3_config(raw: &str) -> Result<Qwen3ModelSpec, String> {
    Ok(Qwen3ModelSpec {
        model_type: json_string_field(raw, "model_type")?,
        hidden_size: json_usize_field(raw, "hidden_size")?,
        intermediate_size: json_usize_field(raw, "intermediate_size")?,
        max_position_embeddings: json_usize_field(raw, "max_position_embeddings")?,
        num_hidden_layers: json_usize_field(raw, "num_hidden_layers")?,
        num_attention_heads: json_usize_field(raw, "num_attention_heads")?,
        num_key_value_heads: json_usize_field(raw, "num_key_value_heads")?,
        head_dim: json_usize_field(raw, "head_dim")?,
        vocab_size: json_usize_field(raw, "vocab_size")?,
        torch_dtype: json_string_field(raw, "torch_dtype").ok(),
    })
}

fn parse_pooling_config(raw: &str, modules_json: Option<&str>) -> Result<PoolingSpec, String> {
    Ok(PoolingSpec {
        word_embedding_dimension: json_usize_field(raw, "word_embedding_dimension")?,
        pooling_mode_lasttoken: json_bool_field(raw, "pooling_mode_lasttoken")?,
        normalize_module_present: modules_json
            .map(|raw| raw.contains("sentence_transformers.models.Normalize"))
            .unwrap_or(false),
    })
}

fn json_usize_field(raw: &str, key: &str) -> Result<usize, String> {
    let value = json_raw_value(raw, key)?;
    let digits = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("JSON field `{key}` is not an unsigned integer"));
    }
    digits
        .parse::<usize>()
        .map_err(|err| format!("JSON field `{key}` is out of range: {err}"))
}

fn json_bool_field(raw: &str, key: &str) -> Result<bool, String> {
    let value = json_raw_value(raw, key)?;
    if value.starts_with("true") {
        Ok(true)
    } else if value.starts_with("false") {
        Ok(false)
    } else {
        Err(format!("JSON field `{key}` is not a boolean"))
    }
}

fn json_string_field(raw: &str, key: &str) -> Result<String, String> {
    let value = json_raw_value(raw, key)?;
    let Some(rest) = value.strip_prefix('"') else {
        return Err(format!("JSON field `{key}` is not a string"));
    };
    let mut out = String::new();
    let mut escaped = false;
    for ch in rest.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Ok(out);
        } else {
            out.push(ch);
        }
    }
    Err(format!("JSON field `{key}` string is unterminated"))
}

fn json_usize_array_field(raw: &str, key: &str) -> Result<Vec<usize>, String> {
    let value = json_raw_value(raw, key)?;
    let Some(mut rest) = value.strip_prefix('[') else {
        return Err(format!("JSON field `{key}` is not an array"));
    };
    let mut values = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            return Ok(values);
        }
        let digits = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            return Err(format!("JSON array field `{key}` contains a non-integer"));
        }
        values.push(
            digits
                .parse::<usize>()
                .map_err(|err| format!("JSON array field `{key}` value is out of range: {err}"))?,
        );
        rest = &rest[digits.len()..];
        rest = rest.trim_start();
        if rest.starts_with(',') {
            rest = &rest[1..];
        } else if rest.starts_with(']') {
            return Ok(values);
        } else {
            return Err(format!("JSON array field `{key}` is malformed"));
        }
    }
}

fn json_u64_pair_field(raw: &str, key: &str) -> Result<(u64, u64), String> {
    let values = json_u64_array_field(raw, key)?;
    if values.len() != 2 {
        return Err(format!(
            "JSON array field `{key}` must contain exactly two offsets"
        ));
    }
    Ok((values[0], values[1]))
}

fn json_u64_array_field(raw: &str, key: &str) -> Result<Vec<u64>, String> {
    let value = json_raw_value(raw, key)?;
    let Some(mut rest) = value.strip_prefix('[') else {
        return Err(format!("JSON field `{key}` is not an array"));
    };
    let mut values = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            return Ok(values);
        }
        let digits = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            return Err(format!("JSON array field `{key}` contains a non-integer"));
        }
        values.push(
            digits
                .parse::<u64>()
                .map_err(|err| format!("JSON array field `{key}` value is out of range: {err}"))?,
        );
        rest = &rest[digits.len()..];
        rest = rest.trim_start();
        if rest.starts_with(',') {
            rest = &rest[1..];
        } else if rest.starts_with(']') {
            return Ok(values);
        } else {
            return Err(format!("JSON array field `{key}` is malformed"));
        }
    }
}

fn json_raw_value<'a>(raw: &'a str, key: &str) -> Result<&'a str, String> {
    let needle = format!("\"{key}\"");
    let start = raw
        .find(&needle)
        .ok_or_else(|| format!("missing JSON field `{key}`"))?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key
        .find(':')
        .ok_or_else(|| format!("missing JSON colon after `{key}`"))?;
    Ok(after_key[colon + 1..].trim_start())
}

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    index
}

fn parse_json_string_at(bytes: &[u8], start: usize) -> Result<(String, usize), String> {
    if bytes.get(start) != Some(&b'"') {
        return Err("expected JSON string".to_string());
    }
    let mut out = String::new();
    let mut index = start + 1;
    let mut escaped = false;
    while let Some(&byte) = bytes.get(index) {
        if escaped {
            out.push(byte as char);
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Ok((out, index + 1));
        } else {
            out.push(byte as char);
        }
        index += 1;
    }
    Err("unterminated JSON string".to_string())
}

fn matching_brace(bytes: &[u8], start: usize) -> Result<usize, String> {
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(start + offset);
                }
            }
            _ => {}
        }
    }
    Err("unterminated JSON object".to_string())
}

fn skip_json_value(bytes: &[u8], start: usize) -> Result<usize, String> {
    match bytes.get(start) {
        Some(b'{') => matching_brace(bytes, start).map(|index| index + 1),
        Some(b'"') => parse_json_string_at(bytes, start).map(|(_, index)| index),
        Some(b'[') => skip_json_array(bytes, start),
        Some(_) => {
            let mut index = start;
            while !matches!(bytes.get(index), None | Some(b',' | b'}')) {
                index += 1;
            }
            Ok(index)
        }
        None => Err("missing JSON value".to_string()),
    }
}

fn skip_json_array(bytes: &[u8], start: usize) -> Result<usize, String> {
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(start + offset + 1);
                }
            }
            _ => {}
        }
    }
    Err("unterminated JSON array".to_string())
}

fn candidate_model_dirs(ctox_root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for key in [
        "CTOX_QWEN3_EMBEDDING_MODEL_DIR",
        "QWEN3_EMBEDDING_MODEL_DIR",
        "CTOX_EMBEDDING_MODEL_DIR",
    ] {
        if let Some(value) = env::var_os(key) {
            let path = PathBuf::from(value);
            if !path.as_os_str().is_empty() {
                dirs.push(path);
            }
        }
    }

    dirs.extend([
        ctox_root.join("runtime/models/Qwen3-Embedding-0.6B"),
        ctox_root.join("runtime/models/Qwen--Qwen3-Embedding-0.6B"),
        ctox_root.join("models/Qwen3-Embedding-0.6B"),
        ctox_root.join("models/Qwen--Qwen3-Embedding-0.6B"),
    ]);

    if !global_discovery_disabled() {
        if let Some(home) = home_dir() {
            dirs.push(home.join("models/Qwen3-Embedding-0.6B"));
            dirs.push(home.join("models/Qwen--Qwen3-Embedding-0.6B"));
            dirs.extend(huggingface_snapshot_dirs(
                &home
                    .join(".cache/huggingface/hub")
                    .join("models--Qwen--Qwen3-Embedding-0.6B"),
            ));
        }
        if let Some(hf_home) = env::var_os("HF_HOME") {
            dirs.extend(huggingface_snapshot_dirs(
                &PathBuf::from(hf_home).join("hub/models--Qwen--Qwen3-Embedding-0.6B"),
            ));
        }
    }
    dirs
}

fn global_discovery_disabled() -> bool {
    env::var_os("CTOX_QWEN3_EMBEDDING_DISABLE_GLOBAL_DISCOVERY")
        .and_then(|value| value.into_string().ok())
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn huggingface_snapshot_dirs(repo_cache: &Path) -> Vec<PathBuf> {
    let snapshots = repo_cache.join("snapshots");
    let Ok(entries) = std::fs::read_dir(snapshots) else {
        return Vec::new();
    };
    let mut dirs = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    dirs.reverse();
    dirs
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn dedupe_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = Vec::<PathBuf>::new();
    paths.retain(|path| {
        if seen.iter().any(|existing| existing == path) {
            false
        } else {
            seen.push(path.clone());
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn finds_complete_hf_style_artifact_dir() {
        let root = temp_root("artifact-discovery");
        let model = root.join("runtime/models/Qwen3-Embedding-0.6B");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("config.json"), "{}").unwrap();
        std::fs::write(model.join("tokenizer.json"), "{}").unwrap();
        std::fs::write(model.join("model-00001-of-00002.safetensors"), b"").unwrap();
        std::fs::write(model.join("model-00002-of-00002.safetensors"), b"").unwrap();

        let found = discover_model_artifacts(&root).found.unwrap();
        assert_eq!(found.root, model);
        assert_eq!(found.safetensors.len(), 2);
    }

    #[test]
    fn rejects_incomplete_artifact_dir() {
        let root = temp_root("artifact-discovery-incomplete");
        let model = root.join("runtime/models/Qwen3-Embedding-0.6B");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("config.json"), "{}").unwrap();

        assert!(artifacts_at(&model).is_none());
    }

    #[test]
    fn inspects_qwen3_embedding_config_and_pooling() {
        let root = temp_root("artifact-inspection");
        let model = root.join("runtime/models/Qwen3-Embedding-0.6B");
        std::fs::create_dir_all(model.join("1_Pooling")).unwrap();
        std::fs::write(
            model.join("config.json"),
            r#"{
              "model_type": "qwen3",
              "hidden_size": 1024,
              "intermediate_size": 3072,
              "max_position_embeddings": 32768,
              "num_hidden_layers": 28,
              "num_attention_heads": 16,
              "num_key_value_heads": 8,
              "head_dim": 128,
              "vocab_size": 151669,
              "torch_dtype": "bfloat16"
            }"#,
        )
        .unwrap();
        std::fs::write(model.join("tokenizer.json"), "{}").unwrap();
        std::fs::write(model.join("model.safetensors"), minimal_safetensors()).unwrap();
        std::fs::write(
            model.join("1_Pooling/config.json"),
            r#"{
              "word_embedding_dimension": 1024,
              "pooling_mode_lasttoken": true
            }"#,
        )
        .unwrap();
        std::fs::write(
            model.join("modules.json"),
            r#"[{"type":"sentence_transformers.models.Normalize"}]"#,
        )
        .unwrap();

        let artifacts = artifacts_at(&model).unwrap();
        let inspected = inspect_artifacts(&artifacts).unwrap();
        assert_eq!(inspected.model.model_type, "qwen3");
        assert_eq!(inspected.model.hidden_size, 1024);
        assert_eq!(inspected.model.max_position_embeddings, 32768);
        assert_eq!(inspected.pooling.unwrap().word_embedding_dimension, 1024);
        assert!(inspected.safetensors_total_bytes > 8);
        assert!(inspected.required_tensors_present);
        assert_eq!(inspected.safetensors_tensor_count, 13);
    }

    #[test]
    fn parses_safetensors_header_without_loading_weight_payload() {
        let root = temp_root("safetensors-header");
        let path = root.join("model.safetensors");
        std::fs::write(&path, minimal_safetensors()).unwrap();

        let header = read_safetensors_header(&path).unwrap();
        assert!(header
            .tensors
            .iter()
            .any(|tensor| tensor.name == "embed_tokens.weight"
                && tensor.dtype == "BF16"
                && tensor.shape == vec![151669, 1024]));
    }

    #[test]
    fn reads_safetensors_tensor_payload_by_offset() {
        let root = temp_root("safetensors-payload");
        let path = root.join("model.safetensors");
        let header = r#"{"tiny.weight":{"dtype":"U8","shape":[4],"data_offsets":[0,4]}}"#;
        let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(b"abcd");
        std::fs::write(&path, bytes).unwrap();

        let (tensor, payload) = read_safetensors_tensor(&path, "tiny.weight").unwrap();
        assert_eq!(tensor.shape, vec![4]);
        assert_eq!(tensor.data_offsets, (0, 4));
        assert_eq!(payload, b"abcd");
    }

    fn minimal_safetensors() -> Vec<u8> {
        let required = [
            ("embed_tokens.weight", "[151669,1024]"),
            ("layers.0.input_layernorm.weight", "[1024]"),
            ("layers.0.self_attn.q_proj.weight", "[2048,1024]"),
            ("layers.0.self_attn.k_proj.weight", "[1024,1024]"),
            ("layers.0.self_attn.v_proj.weight", "[1024,1024]"),
            ("layers.0.self_attn.o_proj.weight", "[1024,2048]"),
            ("layers.0.mlp.gate_proj.weight", "[3072,1024]"),
            ("layers.0.mlp.up_proj.weight", "[3072,1024]"),
            ("layers.0.mlp.down_proj.weight", "[1024,3072]"),
            ("layers.27.input_layernorm.weight", "[1024]"),
            ("layers.27.self_attn.q_proj.weight", "[2048,1024]"),
            ("layers.27.mlp.down_proj.weight", "[1024,3072]"),
            ("norm.weight", "[1024]"),
        ];
        let mut header = String::from("{");
        for (index, (name, shape)) in required.iter().enumerate() {
            if index > 0 {
                header.push(',');
            }
            header.push_str(&format!(
                "\"{name}\":{{\"dtype\":\"BF16\",\"shape\":{shape},\"data_offsets\":[0,0]}}"
            ));
        }
        header.push('}');
        let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(header.as_bytes());
        bytes
    }

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = env::temp_dir().join(format!("ctox-qwen3-{label}-{unique}"));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
}
