#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta3_ffn_superblock is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPack, MetalPackEntry, PackLayout, QWEN35_08B};
#[cfg(target_os = "macos")]
use half::{bf16 as half_bf16, f16 as half_f16};

#[cfg(target_os = "macos")]
struct OwnedDeltaFfnLayer {
    input_norm: Vec<u16>,
    qkv: Vec<u16>,
    z: Vec<u16>,
    b: Vec<u16>,
    a: Vec<u16>,
    conv_weight: Vec<u16>,
    conv_bias: Vec<u16>,
    a_log: Vec<f32>,
    dt_bias: Vec<f32>,
    delta_norm: Vec<f32>,
    delta_out: Vec<u16>,
    ffn_norm: Vec<u16>,
    ffn_gate: Vec<u16>,
    ffn_up: Vec<u16>,
    ffn_down: Vec<u16>,
    qkv_row_tile: usize,
    qkv_col_tile: usize,
    delta_out_col_tile: usize,
    ffn_down_col_tile: usize,
}

#[cfg(target_os = "macos")]
struct OwnedMpsFfnLayer {
    gate_up: Vec<u8>,
    down: Vec<u8>,
}

#[cfg(target_os = "macos")]
struct OwnedMpsDeltaProjectLayer {
    qkvz: Vec<u8>,
}

#[cfg(target_os = "macos")]
struct OwnedMpsDeltaOutLayer {
    out: Vec<u8>,
}

#[cfg(target_os = "macos")]
impl OwnedMpsFfnLayer {
    fn as_weights(
        &self,
    ) -> ctox_qwen35_08b_metal_probe::metal::bench::PrefillMpsFfnLayerWeights<'_> {
        ctox_qwen35_08b_metal_probe::metal::bench::PrefillMpsFfnLayerWeights {
            gate_up: &self.gate_up,
            down: &self.down,
        }
    }
}

#[cfg(target_os = "macos")]
impl OwnedMpsDeltaProjectLayer {
    fn as_weights(
        &self,
    ) -> ctox_qwen35_08b_metal_probe::metal::bench::PrefillMpsDeltaProjectLayerWeights<'_> {
        ctox_qwen35_08b_metal_probe::metal::bench::PrefillMpsDeltaProjectLayerWeights {
            qkvz: &self.qkvz,
        }
    }
}

#[cfg(target_os = "macos")]
impl OwnedMpsDeltaOutLayer {
    fn as_weights(
        &self,
    ) -> ctox_qwen35_08b_metal_probe::metal::bench::PrefillMpsDeltaOutLayerWeights<'_> {
        ctox_qwen35_08b_metal_probe::metal::bench::PrefillMpsDeltaOutLayerWeights { out: &self.out }
    }
}

#[cfg(target_os = "macos")]
impl OwnedDeltaFfnLayer {
    fn as_weights(
        &self,
    ) -> ctox_qwen35_08b_metal_probe::metal::bench::PrefillDeltaFfnLayerWeights<'_> {
        ctox_qwen35_08b_metal_probe::metal::bench::PrefillDeltaFfnLayerWeights {
            input_norm: &self.input_norm,
            qkv: &self.qkv,
            z: &self.z,
            b: &self.b,
            a: &self.a,
            conv_weight: &self.conv_weight,
            conv_bias: &self.conv_bias,
            a_log: &self.a_log,
            dt_bias: &self.dt_bias,
            delta_norm: &self.delta_norm,
            delta_out: &self.delta_out,
            ffn_norm: &self.ffn_norm,
            ffn_gate: &self.ffn_gate,
            ffn_up: &self.ffn_up,
            ffn_down: &self.ffn_down,
        }
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_delta_ffn_stack_with_weights, PrefillDeltaFfnBlockBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_prefill_delta3_ffn_superblock <metalpack-dir> [start-layer] [tokens] [iterations] [warmup] [delta-layer-count] [mps-ffn-sidecar-dir] [mps-delta-project-sidecar-dir] [mps-delta-out-sidecar-dir]".to_owned());
    }

    let root = PathBuf::from(&args[1]);
    let start_layer = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid start-layer argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(0);
    let tokens = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid tokens argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(512);
    let iterations = args
        .get(4)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(5);
    let warmup = args
        .get(5)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid warmup argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(3);
    let delta_layer_count = args
        .get(6)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid delta-layer-count argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(3);
    let mps_ffn_sidecar = args.get(7).map(PathBuf::from);
    let mps_delta_project_sidecar = args.get(8).map(PathBuf::from);
    let mps_delta_out_sidecar = args.get(9).map(PathBuf::from);
    if mps_delta_project_sidecar.is_some() && mps_ffn_sidecar.is_none() {
        return Err(
            "mps-delta-project-sidecar-dir currently requires mps-ffn-sidecar-dir".to_owned(),
        );
    }
    if delta_layer_count == 0 {
        return Err("delta-layer-count must be > 0".to_owned());
    }

    let mut layer_ids = Vec::with_capacity(delta_layer_count);
    let mut layer = start_layer;
    while layer_ids.len() < delta_layer_count {
        if layer >= QWEN35_08B.n_layers {
            return Err(format!(
                "not enough DeltaNet layers from start-layer {start_layer}; requested {delta_layer_count}"
            ));
        }
        if layer % 4 != 3 {
            layer_ids.push(layer);
        }
        layer += 1;
    }

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let owned_layers = layer_ids
        .iter()
        .map(|layer| load_layer(&pack, *layer))
        .collect::<Result<Vec<_>, String>>()?;
    validate_same_tiles(&owned_layers)?;
    let borrowed_layers = owned_layers
        .iter()
        .map(|layer| layer.as_weights())
        .collect::<Vec<_>>();

    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }

    let cfg = PrefillDeltaFfnBlockBenchConfig {
        tokens,
        row_tile: owned_layers[0].qkv_row_tile,
        hidden_col_tile: owned_layers[0].qkv_col_tile,
        delta_out_col_tile: owned_layers[0].delta_out_col_tile,
        intermediate_col_tile: owned_layers[0].ffn_down_col_tile,
        warmup,
        iterations,
    };
    let result = if let Some(sidecar) = mps_ffn_sidecar.as_ref() {
        let owned_mps_ffn_layers = load_mps_ffn_sidecar(sidecar, &layer_ids)?;
        let borrowed_mps_ffn_layers = owned_mps_ffn_layers
            .iter()
            .map(|layer| layer.as_weights())
            .collect::<Vec<_>>();
        let owned_mps_delta_project_layers =
            if let Some(delta_sidecar) = mps_delta_project_sidecar.as_ref() {
                Some(load_mps_delta_project_sidecar(delta_sidecar, &layer_ids)?)
            } else {
                None
            };
        let borrowed_mps_delta_project_layers =
            owned_mps_delta_project_layers.as_ref().map(|layers| {
                layers
                    .iter()
                    .map(|layer| layer.as_weights())
                    .collect::<Vec<_>>()
            });
        let owned_mps_delta_out_layers = if let Some(delta_sidecar) = mps_delta_out_sidecar.as_ref()
        {
            Some(load_mps_delta_out_sidecar(delta_sidecar, &layer_ids)?)
        } else {
            None
        };
        let borrowed_mps_delta_out_layers = owned_mps_delta_out_layers.as_ref().map(|layers| {
            layers
                .iter()
                .map(|layer| layer.as_weights())
                .collect::<Vec<_>>()
        });
        ctox_qwen35_08b_metal_probe::metal::bench::run_prefill_delta_ffn_stack_with_mps_ffn_sidecar(
            cfg,
            &x_host,
            &borrowed_layers,
            &borrowed_mps_ffn_layers,
            borrowed_mps_delta_project_layers.as_deref(),
            borrowed_mps_delta_out_layers.as_deref(),
        )?
    } else {
        run_prefill_delta_ffn_stack_with_weights(cfg, &x_host, &borrowed_layers)?
    };

    println!("qwen35-08b metalpack prefill DeltaNet + FFN stack benchmark");
    println!("metalpack: {}", root.display());
    println!(
        "layers: {}",
        layer_ids
            .iter()
            .map(|layer| layer.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    println!(
        "shape: tokens={} layers={} hidden={} delta_width={} intermediate={} qkv_rows={}",
        result.tokens,
        result.layers,
        result.hidden,
        result.delta_width,
        result.intermediate,
        result.qkv_rows
    );
    println!(
        "tile: project_tokens={} qkvz_tokens={} out_tokens={} ffn_tokens={} down_tokens={} rows={} packed_bytes={}",
        result.project_token_tile,
        result.qkvz_token_tile,
        result.out_token_tile,
        result.ffn_token_tile,
        result.down_token_tile,
        result.row_tile,
        result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!(
        "ffn_backend: {}",
        if mps_ffn_sidecar.is_some() {
            "mps_sidecar"
        } else {
            "msl"
        }
    );
    if let Some(sidecar) = mps_ffn_sidecar {
        println!("mps_ffn_sidecar: {}", sidecar.display());
    }
    println!(
        "delta_project_backend: {}",
        if mps_delta_project_sidecar.is_some() {
            "mps_sidecar"
        } else {
            "msl"
        }
    );
    if let Some(sidecar) = mps_delta_project_sidecar {
        println!("mps_delta_project_sidecar: {}", sidecar.display());
    }
    println!(
        "delta_out_backend: {}",
        if mps_delta_out_sidecar.is_some() {
            "mps_sidecar"
        } else {
            "msl"
        }
    );
    if let Some(sidecar) = mps_delta_out_sidecar {
        println!("mps_delta_out_sidecar: {}", sidecar.display());
    }
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_delta_ffn_stack_estimate: {:.2}",
        result.effective_gb_s
    );
    println!(
        "profile_stop: {}",
        std::env::var("CTOX_QWEN35_DELTA_STACK_PROFILE_STOP").unwrap_or_else(|_| "full".to_owned())
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
}

#[cfg(target_os = "macos")]
fn load_mps_ffn_sidecar(
    sidecar: &std::path::Path,
    layer_ids: &[usize],
) -> Result<Vec<OwnedMpsFfnLayer>, String> {
    use serde_json::Value;
    use std::fs;

    let manifest_path = sidecar.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|err| format!("{}: {err}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|err| err.to_string())?;
    if manifest.get("format").and_then(Value::as_str) != Some("ctox.qwen35_08b.mps_ffn_sidecar") {
        return Err(format!(
            "invalid MPS FFN sidecar: {}",
            manifest_path.display()
        ));
    }
    let weights_file = manifest
        .get("weights_file")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing MPS FFN weights_file".to_owned())?;
    let entries = manifest
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing MPS FFN entries".to_owned())?;
    let weights_path = sidecar.join(weights_file);
    let weights =
        fs::read(&weights_path).map_err(|err| format!("{}: {err}", weights_path.display()))?;

    layer_ids
        .iter()
        .map(|layer| {
            let entry = entries
                .iter()
                .find(|entry| entry.get("layer").and_then(Value::as_u64) == Some(*layer as u64))
                .ok_or_else(|| format!("missing layer {layer} in MPS FFN sidecar"))?;
            let gate_up = entry
                .get("gate_up")
                .ok_or_else(|| format!("missing gate_up for layer {layer}"))?;
            let down = entry
                .get("down")
                .ok_or_else(|| format!("missing down for layer {layer}"))?;
            let gate_up_offset = json_usize(gate_up, "offset")?;
            let gate_up_bytes = json_usize(gate_up, "bytes")?;
            let down_offset = json_usize(down, "offset")?;
            let down_bytes = json_usize(down, "bytes")?;
            if gate_up_offset + gate_up_bytes > weights.len()
                || down_offset + down_bytes > weights.len()
            {
                return Err(format!(
                    "MPS FFN sidecar layer {layer} exceeds weights.bin length"
                ));
            }
            Ok(OwnedMpsFfnLayer {
                gate_up: weights[gate_up_offset..gate_up_offset + gate_up_bytes].to_vec(),
                down: weights[down_offset..down_offset + down_bytes].to_vec(),
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn load_mps_delta_project_sidecar(
    sidecar: &std::path::Path,
    layer_ids: &[usize],
) -> Result<Vec<OwnedMpsDeltaProjectLayer>, String> {
    use serde_json::Value;
    use std::fs;

    let manifest_path = sidecar.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|err| format!("{}: {err}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|err| err.to_string())?;
    if manifest.get("format").and_then(Value::as_str)
        != Some("ctox.qwen35_08b.mps_delta_project_sidecar")
    {
        return Err(format!(
            "invalid MPS Delta project sidecar: {}",
            manifest_path.display()
        ));
    }
    let weights_file = manifest
        .get("weights_file")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing MPS Delta project weights_file".to_owned())?;
    let entries = manifest
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing MPS Delta project entries".to_owned())?;
    let weights_path = sidecar.join(weights_file);
    let weights =
        fs::read(&weights_path).map_err(|err| format!("{}: {err}", weights_path.display()))?;

    layer_ids
        .iter()
        .map(|layer| {
            let entry = entries
                .iter()
                .find(|entry| entry.get("layer").and_then(Value::as_u64) == Some(*layer as u64))
                .ok_or_else(|| format!("missing layer {layer} in MPS Delta project sidecar"))?;
            let qkvz = entry
                .get("qkvz")
                .ok_or_else(|| format!("missing qkvz for layer {layer}"))?;
            let qkvz_offset = json_usize(qkvz, "offset")?;
            let qkvz_bytes = json_usize(qkvz, "bytes")?;
            if qkvz_offset + qkvz_bytes > weights.len() {
                return Err(format!(
                    "MPS Delta project sidecar layer {layer} exceeds weights.bin length"
                ));
            }
            Ok(OwnedMpsDeltaProjectLayer {
                qkvz: weights[qkvz_offset..qkvz_offset + qkvz_bytes].to_vec(),
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn load_mps_delta_out_sidecar(
    sidecar: &std::path::Path,
    layer_ids: &[usize],
) -> Result<Vec<OwnedMpsDeltaOutLayer>, String> {
    use serde_json::Value;
    use std::fs;

    let manifest_path = sidecar.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|err| format!("{}: {err}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|err| err.to_string())?;
    if manifest.get("format").and_then(Value::as_str)
        != Some("ctox.qwen35_08b.mps_delta_out_sidecar")
    {
        return Err(format!(
            "invalid MPS DeltaOut sidecar: {}",
            manifest_path.display()
        ));
    }
    let weights_file = manifest
        .get("weights_file")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing MPS DeltaOut weights_file".to_owned())?;
    let entries = manifest
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing MPS DeltaOut entries".to_owned())?;
    let weights_path = sidecar.join(weights_file);
    let weights =
        fs::read(&weights_path).map_err(|err| format!("{}: {err}", weights_path.display()))?;

    layer_ids
        .iter()
        .map(|layer| {
            let entry = entries
                .iter()
                .find(|entry| entry.get("layer").and_then(Value::as_u64) == Some(*layer as u64))
                .ok_or_else(|| format!("missing layer {layer} in MPS DeltaOut sidecar"))?;
            let out = entry
                .get("out")
                .ok_or_else(|| format!("missing out for layer {layer}"))?;
            let out_offset = json_usize(out, "offset")?;
            let out_bytes = json_usize(out, "bytes")?;
            if out_offset + out_bytes > weights.len() {
                return Err(format!(
                    "MPS DeltaOut sidecar layer {layer} exceeds weights.bin length"
                ));
            }
            Ok(OwnedMpsDeltaOutLayer {
                out: weights[out_offset..out_offset + out_bytes].to_vec(),
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn json_usize(value: &serde_json::Value, key: &str) -> Result<usize, String> {
    let raw = value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("missing {key}"))?;
    usize::try_from(raw).map_err(|_| format!("{key} exceeds usize"))
}

#[cfg(target_os = "macos")]
fn load_layer(pack: &MetalPack, layer: usize) -> Result<OwnedDeltaFfnLayer, String> {
    let prefix = format!("model.language_model.layers.{layer}.");
    let input_norm = find_tensor(pack, &prefix, "input_layernorm.weight")?;
    let qkv = find_tensor(pack, &prefix, "linear_attn.in_proj_qkv.weight")?;
    let z = find_tensor(pack, &prefix, "linear_attn.in_proj_z.weight")?;
    let b = find_tensor(pack, &prefix, "linear_attn.in_proj_b.weight")?;
    let a = find_tensor(pack, &prefix, "linear_attn.in_proj_a.weight")?;
    let conv_weight = find_tensor(pack, &prefix, "linear_attn.conv1d.weight")?;
    let conv_bias = find_optional_tensor(pack, &prefix, "linear_attn.conv1d.bias");
    let a_log = find_tensor(pack, &prefix, "linear_attn.A_log")?;
    let dt_bias = find_tensor(pack, &prefix, "linear_attn.dt_bias")?;
    let delta_norm = find_tensor(pack, &prefix, "linear_attn.norm.weight")?;
    let delta_out = find_tensor(pack, &prefix, "linear_attn.out_proj.weight")?;
    let ffn_norm = find_tensor(pack, &prefix, "post_attention_layernorm.weight")?;
    let ffn_gate = find_tensor(pack, &prefix, "mlp.gate_proj.weight")?;
    let ffn_up = find_tensor(pack, &prefix, "mlp.up_proj.weight")?;
    let ffn_down = find_tensor(pack, &prefix, "mlp.down_proj.weight")?;

    validate_vector(input_norm, QWEN35_08B.hidden_size)?;
    validate_projection(
        "qkv",
        qkv,
        &[QWEN35_08B.deltanet_qkv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "z",
        z,
        &[QWEN35_08B.deltanet_width(), QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "b",
        b,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "a",
        a,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "delta_out",
        delta_out,
        &[QWEN35_08B.hidden_size, QWEN35_08B.deltanet_width()],
    )?;
    validate_conv_weight(conv_weight)?;
    if let Some(entry) = conv_bias {
        validate_conv_bias(entry)?;
    }
    validate_vector(a_log, QWEN35_08B.deltanet_v_heads)?;
    validate_vector(dt_bias, QWEN35_08B.deltanet_v_heads)?;
    validate_vector(delta_norm, QWEN35_08B.deltanet_head_dim)?;
    validate_vector(ffn_norm, QWEN35_08B.hidden_size)?;
    validate_projection(
        "ffn_gate",
        ffn_gate,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "ffn_up",
        ffn_up,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "ffn_down",
        ffn_down,
        &[QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate],
    )?;

    Ok(OwnedDeltaFfnLayer {
        input_norm: read_u16_entry(pack, input_norm)?,
        qkv: read_u16_entry(pack, qkv)?,
        z: read_u16_entry(pack, z)?,
        b: read_u16_entry(pack, b)?,
        a: read_u16_entry(pack, a)?,
        conv_weight: read_u16_entry(pack, conv_weight)?,
        conv_bias: match conv_bias {
            Some(entry) => read_u16_entry(pack, entry)?,
            None => vec![half_f16::from_f32(0.0).to_bits(); QWEN35_08B.deltanet_qkv_width()],
        },
        a_log: read_float_entry(pack, a_log)?,
        dt_bias: read_float_entry(pack, dt_bias)?,
        delta_norm: read_float_entry(pack, delta_norm)?,
        delta_out: read_u16_entry(pack, delta_out)?,
        ffn_norm: read_u16_entry(pack, ffn_norm)?,
        ffn_gate: read_u16_entry(pack, ffn_gate)?,
        ffn_up: read_u16_entry(pack, ffn_up)?,
        ffn_down: read_u16_entry(pack, ffn_down)?,
        qkv_row_tile: qkv.row_tile,
        qkv_col_tile: qkv.col_tile,
        delta_out_col_tile: delta_out.col_tile,
        ffn_down_col_tile: ffn_down.col_tile,
    })
}

#[cfg(target_os = "macos")]
fn validate_same_tiles(layers: &[OwnedDeltaFfnLayer]) -> Result<(), String> {
    if layers.is_empty() {
        return Err("layer list must not be empty".to_string());
    }
    for layer in &layers[1..] {
        if layer.qkv_row_tile != layers[0].qkv_row_tile
            || layer.qkv_col_tile != layers[0].qkv_col_tile
            || layer.delta_out_col_tile != layers[0].delta_out_col_tile
            || layer.ffn_down_col_tile != layers[0].ffn_down_col_tile
        {
            return Err("all layers must use the same packed tile geometry".to_string());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_tensor<'a>(
    pack: &'a MetalPack,
    prefix: &str,
    name: &str,
) -> Result<&'a MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
        .ok_or_else(|| format!("missing tensor containing `{prefix}` and `{name}`"))
}

#[cfg(target_os = "macos")]
fn find_optional_tensor<'a>(
    pack: &'a MetalPack,
    prefix: &str,
    name: &str,
) -> Option<&'a MetalPackEntry> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
}

#[cfg(target_os = "macos")]
fn validate_projection(label: &str, entry: &MetalPackEntry, shape: &[usize]) -> Result<(), String> {
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != shape {
        return Err(format!(
            "{label}: expected {:?}, got {:?}",
            shape, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_vector(entry: &MetalPackEntry, len: usize) -> Result<(), String> {
    if entry.source_shape != [len] {
        return Err(format!(
            "{}: expected vector len {len}, got {:?}",
            entry.tensor, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_conv_weight(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.deltanet_qkv_width(), 1, 4] {
        return Err(format!(
            "conv weight: expected [{}, 1, 4], got {:?}",
            QWEN35_08B.deltanet_qkv_width(),
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_conv_bias(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.deltanet_qkv_width()] {
        return Err(format!(
            "conv bias: expected [{}], got {:?}",
            QWEN35_08B.deltanet_qkv_width(),
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_u16_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<u16>, String> {
    let bytes = pack
        .read_entry_bytes(entry)
        .map_err(|err| err.to_string())?;
    if bytes.len() % 2 != 0 {
        return Err(format!(
            "{} byte length is not divisible by two",
            entry.tensor
        ));
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}

#[cfg(target_os = "macos")]
fn read_float_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<f32>, String> {
    let bytes = pack
        .read_entry_bytes(entry)
        .map_err(|err| err.to_string())?;
    match entry.dtype.as_str() {
        "F32" => {
            if bytes.len() % 4 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by four",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect())
        }
        "F16" => {
            if bytes.len() % 2 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by two",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(2)
                .map(|chunk| half_f16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32())
                .collect())
        }
        "BF16" => {
            if bytes.len() % 2 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by two",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(2)
                .map(|chunk| {
                    half_bf16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32()
                })
                .collect())
        }
        other => Err(format!("{} unsupported dtype {other}", entry.tensor)),
    }
}
