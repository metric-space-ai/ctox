#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_decode_layered_pattern is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{
    LayerKind, MetalPack, MetalPackEntry, PackLayout, TensorClass, QWEN35_08B,
};
#[cfg(target_os = "macos")]
use half::{bf16, f16};

#[cfg(target_os = "macos")]
struct OwnedDeltaLayer {
    input_norm: Vec<u16>,
    qkv: Vec<u16>,
    z: Vec<u16>,
    b: Vec<u16>,
    a: Vec<u16>,
    a_log: Vec<f32>,
    dt_bias: Vec<f32>,
    gated_norm: Vec<f32>,
    conv_weight: Vec<u16>,
    conv_bias: Vec<u16>,
    out: Vec<u16>,
}

#[cfg(target_os = "macos")]
struct OwnedAttentionLayer {
    input_norm: Vec<u16>,
    q_norm: Vec<u16>,
    k_norm: Vec<u16>,
    q: Vec<u16>,
    k: Vec<u16>,
    v: Vec<u16>,
    o: Vec<u16>,
}

#[cfg(target_os = "macos")]
struct OwnedFfnLayer {
    post_norm: Vec<u16>,
    gate: Vec<u16>,
    up: Vec<u16>,
    down: Vec<u16>,
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_decode_layered_pattern_tiled_sequence_with_weights,
        run_decode_layered_pattern_tiled_with_weights, AttentionLayerTiled,
        DecodeSkeletonBenchConfig, DeltaLayerTiled, FfnLayerTiled,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_decode_layered_pattern <metalpack-dir> [delta-prefix] [attention-prefix] [ffn-prefix] [input-token] [iterations] [decode-position] [max-context] [steps] [top-k]".to_owned());
    }

    let root = PathBuf::from(&args[1]);
    let delta_prefix = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .unwrap_or("model.layers.0");
    let attention_prefix = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .unwrap_or("model.layers.3.self_attn");
    let ffn_prefix = args
        .get(4)
        .and_then(|arg| arg.to_str())
        .unwrap_or("model.layers.0.mlp");
    let input_token = args
        .get(5)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<u32>()
                .map_err(|err| format!("invalid input token argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(107);
    let iterations = args
        .get(6)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(1);
    let decode_position = args
        .get(7)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<u32>()
                .map_err(|err| format!("invalid decode position argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(0);
    let max_context = args
        .get(8)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid max context argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(1);
    let steps = args
        .get(9)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid steps argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(1);
    let debug_top_k = args
        .get(10)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid top-k argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(0);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let embedding = pack
        .find_first_class(TensorClass::TokenEmbedding)
        .ok_or_else(|| "metalpack has no token_embedding entry".to_owned())?;
    let lm_head = pack
        .find_first_class(TensorClass::LmHead)
        .unwrap_or(embedding);

    validate_entry(
        "embedding",
        embedding,
        &[embedding.source_shape[0], QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "lm_head",
        lm_head,
        &[embedding.source_shape[0], QWEN35_08B.hidden_size],
    )?;
    if lm_head.row_tile != embedding.row_tile || lm_head.col_tile != embedding.col_tile {
        return Err(format!("tile mismatch for {}", lm_head.tensor));
    }

    let embedding_weights = read_u16_entry(&pack, embedding)?;
    let lm_weights = if embedding.tensor == lm_head.tensor {
        embedding_weights.clone()
    } else {
        read_u16_entry(&pack, lm_head)?
    };
    let norm = match pack.find_first_class(TensorClass::FinalNorm) {
        Some(entry) => {
            validate_norm_vector("final norm", entry)?;
            read_rms_norm_entry(&pack, entry)?
        }
        None => {
            eprintln!("missing final norm; using synthetic fallback norm");
            default_hidden_norm()
        }
    };

    let binding = match load_full_layer_binding(&pack, embedding.row_tile, embedding.col_tile) {
        Ok(binding) => binding,
        Err(auto_error) => {
            eprintln!("auto layer binding unavailable: {auto_error}");
            eprintln!("falling back to template prefixes");
            load_template_binding(
                &pack,
                delta_prefix,
                attention_prefix,
                ffn_prefix,
                embedding.row_tile,
                embedding.col_tile,
            )?
        }
    };

    let delta_layers = binding
        .delta
        .iter()
        .map(|layer| DeltaLayerTiled {
            input_norm: &layer.input_norm,
            qkv: &layer.qkv,
            z: &layer.z,
            b: &layer.b,
            a: &layer.a,
            a_log: &layer.a_log,
            dt_bias: &layer.dt_bias,
            gated_norm: &layer.gated_norm,
            conv_weight: &layer.conv_weight,
            conv_bias: &layer.conv_bias,
            out: &layer.out,
        })
        .collect::<Vec<_>>();
    let attention_layers = binding
        .attention
        .iter()
        .map(|layer| AttentionLayerTiled {
            input_norm: &layer.input_norm,
            q_norm: &layer.q_norm,
            k_norm: &layer.k_norm,
            q: &layer.q,
            k: &layer.k,
            v: &layer.v,
            o: &layer.o,
        })
        .collect::<Vec<_>>();
    let ffn_layers = binding
        .ffn
        .iter()
        .map(|layer| FfnLayerTiled {
            post_norm: &layer.post_norm,
            gate: &layer.gate,
            up: &layer.up,
            down: &layer.down,
        })
        .collect::<Vec<_>>();

    let cfg = DecodeSkeletonBenchConfig {
        vocab_rows: embedding.source_shape[0],
        input_token,
        decode_position,
        max_context,
        warmup: 1,
        iterations,
        debug_top_k,
        ..DecodeSkeletonBenchConfig::default()
    };
    println!("qwen35-08b metalpack decode layered 24-layer pattern benchmark");
    println!("metalpack: {}", root.display());
    println!("binding_mode: {}", binding.mode);
    println!("embedding: {}", embedding.tensor);
    if binding.mode == "template-prefix-fallback" {
        println!("delta_template_prefix: {}", delta_prefix);
        println!("attention_template_prefix: {}", attention_prefix);
        println!("ffn_template_prefix: {}", ffn_prefix);
    }
    println!("lm_head: {}", lm_head.tensor);
    println!("input_token: {}", input_token);
    println!("prefill_steps: {}", decode_position);
    println!("max_context: {}", max_context);
    println!("decode_steps: {}", steps);
    println!("debug_top_k: {}", debug_top_k);
    println!(
        "shape: [{} x {}]",
        embedding.source_shape[0], QWEN35_08B.hidden_size
    );
    println!(
        "layers: delta={} attention={} ffn={}",
        delta_layers.len(),
        attention_layers.len(),
        ffn_layers.len()
    );
    println!(
        "tile: rows={} cols={}",
        embedding.row_tile, embedding.col_tile
    );
    if steps == 1 && decode_position == 0 {
        let result = run_decode_layered_pattern_tiled_with_weights(
            cfg,
            &embedding_weights,
            &norm,
            &delta_layers,
            &attention_layers,
            &ffn_layers,
            &lm_weights,
            embedding.row_tile,
            embedding.col_tile,
        )?;
        println!("iterations: {}", result.iterations);
        println!("median_s: {:.9}", result.median_s);
        println!("p95_s: {:.9}", result.p95_s);
        println!(
            "effective_gb_s_layered_pattern_lm_head_pairs: {:.2}",
            result.effective_gb_s
        );
        println!("next_token: {}", result.next_token);
        println!("score: {:.6}", result.score);
        if !result.top_logits.is_empty() {
            println!("cpu_lm_head_top_logits: {:?}", result.top_logits);
        }
    } else {
        let seq_result = run_decode_layered_pattern_tiled_sequence_with_weights(
            cfg,
            steps,
            &embedding_weights,
            &norm,
            &delta_layers,
            &attention_layers,
            &ffn_layers,
            &lm_weights,
            embedding.row_tile,
            embedding.col_tile,
        )?;
        println!("iterations: {}", seq_result.iterations);
        println!("median_s: {:.9}", seq_result.median_s);
        println!("p95_s: {:.9}", seq_result.p95_s);
        println!(
            "effective_gb_s_layered_pattern_sequence_lm_head_pairs: {:.2}",
            seq_result.effective_gb_s
        );
        println!("tokens: {:?}", seq_result.tokens);
        println!("last_score: {:.6}", seq_result.last_score);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
struct LayerBinding {
    mode: &'static str,
    delta: Vec<OwnedDeltaLayer>,
    attention: Vec<OwnedAttentionLayer>,
    ffn: Vec<OwnedFfnLayer>,
}

#[cfg(target_os = "macos")]
fn load_full_layer_binding(
    pack: &MetalPack,
    row_tile: usize,
    col_tile: usize,
) -> Result<LayerBinding, String> {
    let mut delta = Vec::with_capacity(QWEN35_08B.n_deltanet_layers());
    let mut attention = Vec::with_capacity(QWEN35_08B.n_full_attention_layers());
    let mut ffn = Vec::with_capacity(QWEN35_08B.n_layers);

    for layer_id in 0..QWEN35_08B.n_layers {
        let input_norm = find_layer_norm(pack, layer_id, "input_layernorm.weight")?;
        let post_norm = find_layer_norm(pack, layer_id, "post_attention_layernorm.weight")?;
        validate_norm_vector(&format!("layer {layer_id} input norm"), input_norm)?;
        validate_norm_vector(&format!("layer {layer_id} post norm"), post_norm)?;
        let input_norm_weights = read_rms_norm_entry(pack, input_norm)?;
        let post_norm_weights = read_rms_norm_entry(pack, post_norm)?;

        let gate = find_layer_class(pack, layer_id, TensorClass::MlpGate)?;
        let up = find_layer_class(pack, layer_id, TensorClass::MlpUp)?;
        let down = find_layer_class(pack, layer_id, TensorClass::MlpDown)?;
        validate_entry(
            &format!("layer {layer_id} ffn_gate"),
            gate,
            &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
        )?;
        validate_entry(
            &format!("layer {layer_id} ffn_up"),
            up,
            &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
        )?;
        validate_entry(
            &format!("layer {layer_id} ffn_down"),
            down,
            &[QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate],
        )?;
        validate_tile(gate, row_tile, col_tile)?;
        validate_tile(up, row_tile, col_tile)?;
        validate_tile(down, row_tile, col_tile)?;
        ffn.push(OwnedFfnLayer {
            post_norm: post_norm_weights,
            gate: read_u16_entry(pack, gate)?,
            up: read_u16_entry(pack, up)?,
            down: read_u16_entry(pack, down)?,
        });

        match QWEN35_08B.layer_kind(layer_id) {
            LayerKind::GatedDeltaNet => {
                let qkv = find_layer_class(pack, layer_id, TensorClass::DeltaQkv)?;
                let z = find_layer_class(pack, layer_id, TensorClass::DeltaZ)?;
                let b = find_layer_class(pack, layer_id, TensorClass::DeltaB)?;
                let a = find_layer_class(pack, layer_id, TensorClass::DeltaA)?;
                let out = find_layer_class(pack, layer_id, TensorClass::DeltaOut)?;
                let a_log = find_layer_state_param(pack, layer_id, "A_log")?;
                let dt_bias = find_layer_state_param(pack, layer_id, "dt_bias")?;
                let gated_norm = find_layer_state_param(pack, layer_id, "norm.weight")?;
                let conv_weight = find_layer_state_param(pack, layer_id, "conv1d.weight")?;
                let conv_bias = find_optional_layer_state_param(pack, layer_id, "conv1d.bias");
                validate_delta_entries(layer_id, qkv, z, b, a, out, row_tile, col_tile)?;
                validate_state_float(
                    &format!("layer {layer_id} A_log"),
                    a_log,
                    &[QWEN35_08B.deltanet_v_heads],
                )?;
                validate_state_float(
                    &format!("layer {layer_id} dt_bias"),
                    dt_bias,
                    &[QWEN35_08B.deltanet_v_heads],
                )?;
                validate_state_float(
                    &format!("layer {layer_id} gated norm"),
                    gated_norm,
                    &[QWEN35_08B.deltanet_head_dim],
                )?;
                validate_state_half_one_of(
                    &format!("layer {layer_id} conv1d.weight"),
                    conv_weight,
                    &[
                        vec![QWEN35_08B.deltanet_qkv_width(), 4],
                        vec![QWEN35_08B.deltanet_qkv_width(), 1, 4],
                    ],
                )?;
                if let Some(conv_bias) = conv_bias {
                    validate_state_half(
                        &format!("layer {layer_id} conv1d.bias"),
                        conv_bias,
                        &[QWEN35_08B.deltanet_qkv_width()],
                    )?;
                }
                delta.push(OwnedDeltaLayer {
                    input_norm: input_norm_weights,
                    qkv: read_u16_entry(pack, qkv)?,
                    z: read_u16_entry(pack, z)?,
                    b: read_u16_entry(pack, b)?,
                    a: read_u16_entry(pack, a)?,
                    a_log: read_float_state_entry(pack, a_log)?,
                    dt_bias: read_float_state_entry(pack, dt_bias)?,
                    gated_norm: read_float_state_entry(pack, gated_norm)?,
                    conv_weight: read_half_state_entry(pack, conv_weight)?,
                    conv_bias: read_optional_half_state_entry(pack, conv_bias)?,
                    out: read_u16_entry(pack, out)?,
                });
            }
            LayerKind::FullAttention => {
                let q_norm = find_layer_norm(pack, layer_id, "q_norm.weight")?;
                let k_norm = find_layer_norm(pack, layer_id, "k_norm.weight")?;
                let q = find_layer_class(pack, layer_id, TensorClass::AttentionQ)?;
                let k = find_layer_class(pack, layer_id, TensorClass::AttentionK)?;
                let v = find_layer_class(pack, layer_id, TensorClass::AttentionV)?;
                let o = find_layer_class(pack, layer_id, TensorClass::AttentionO)?;
                validate_head_norm_vector(&format!("layer {layer_id} q_norm"), q_norm)?;
                validate_head_norm_vector(&format!("layer {layer_id} k_norm"), k_norm)?;
                validate_attention_entries(layer_id, q, k, v, o, row_tile, col_tile)?;
                attention.push(OwnedAttentionLayer {
                    input_norm: input_norm_weights,
                    q_norm: read_rms_norm_entry(pack, q_norm)?,
                    k_norm: read_rms_norm_entry(pack, k_norm)?,
                    q: read_u16_entry(pack, q)?,
                    k: read_u16_entry(pack, k)?,
                    v: read_u16_entry(pack, v)?,
                    o: read_u16_entry(pack, o)?,
                });
            }
        }
    }

    Ok(LayerBinding {
        mode: "auto-layer-id",
        delta,
        attention,
        ffn,
    })
}

#[cfg(target_os = "macos")]
fn load_template_binding(
    pack: &MetalPack,
    delta_prefix: &str,
    attention_prefix: &str,
    ffn_prefix: &str,
    row_tile: usize,
    col_tile: usize,
) -> Result<LayerBinding, String> {
    let delta_qkv = find_tensor(pack, delta_prefix, "in_proj_qkv")?;
    let delta_z = find_tensor(pack, delta_prefix, "in_proj_z")?;
    let delta_b = find_tensor(pack, delta_prefix, "in_proj_b")?;
    let delta_a = find_tensor(pack, delta_prefix, "in_proj_a")?;
    let delta_out = find_tensor(pack, delta_prefix, "out_proj")?;
    let delta_a_log = find_tensor(pack, delta_prefix, "A_log")?;
    let delta_dt_bias = find_tensor(pack, delta_prefix, "dt_bias")?;
    let delta_gated_norm = find_tensor(pack, delta_prefix, "norm.weight")?;
    let delta_conv_weight = find_tensor(pack, delta_prefix, "conv1d.weight")?;
    let delta_conv_bias = find_optional_tensor(pack, delta_prefix, "conv1d.bias");
    validate_delta_entries(
        0, delta_qkv, delta_z, delta_b, delta_a, delta_out, row_tile, col_tile,
    )?;
    validate_state_float(
        "template A_log",
        delta_a_log,
        &[QWEN35_08B.deltanet_v_heads],
    )?;
    validate_state_float(
        "template dt_bias",
        delta_dt_bias,
        &[QWEN35_08B.deltanet_v_heads],
    )?;
    validate_state_float(
        "template gated norm",
        delta_gated_norm,
        &[QWEN35_08B.deltanet_head_dim],
    )?;
    validate_state_half_one_of(
        "template conv1d.weight",
        delta_conv_weight,
        &[
            vec![QWEN35_08B.deltanet_qkv_width(), 4],
            vec![QWEN35_08B.deltanet_qkv_width(), 1, 4],
        ],
    )?;
    if let Some(delta_conv_bias) = delta_conv_bias {
        validate_state_half(
            "template conv1d.bias",
            delta_conv_bias,
            &[QWEN35_08B.deltanet_qkv_width()],
        )?;
    }
    let delta_qkv_weights = read_u16_entry(pack, delta_qkv)?;
    let delta_z_weights = read_u16_entry(pack, delta_z)?;
    let delta_b_weights = read_u16_entry(pack, delta_b)?;
    let delta_a_weights = read_u16_entry(pack, delta_a)?;
    let delta_out_weights = read_u16_entry(pack, delta_out)?;
    let delta_a_log_weights = read_float_state_entry(pack, delta_a_log)?;
    let delta_dt_bias_weights = read_float_state_entry(pack, delta_dt_bias)?;
    let delta_gated_norm_weights = read_float_state_entry(pack, delta_gated_norm)?;
    let delta_conv_weight_weights = read_half_state_entry(pack, delta_conv_weight)?;
    let delta_conv_bias_weights = read_optional_half_state_entry(pack, delta_conv_bias)?;

    let attn_q = find_tensor(pack, attention_prefix, "q_proj")?;
    let attn_k = find_tensor(pack, attention_prefix, "k_proj")?;
    let attn_v = find_tensor(pack, attention_prefix, "v_proj")?;
    let attn_o = find_tensor(pack, attention_prefix, "o_proj")?;
    let attn_q_norm = find_optional_tensor(pack, attention_prefix, "q_norm.weight");
    let attn_k_norm = find_optional_tensor(pack, attention_prefix, "k_norm.weight");
    if let Some(attn_q_norm) = attn_q_norm {
        validate_head_norm_vector("template q_norm", attn_q_norm)?;
    }
    if let Some(attn_k_norm) = attn_k_norm {
        validate_head_norm_vector("template k_norm", attn_k_norm)?;
    }
    validate_attention_entries(3, attn_q, attn_k, attn_v, attn_o, row_tile, col_tile)?;
    let attn_q_norm_weights = read_optional_rms_head_norm_entry(pack, attn_q_norm)?;
    let attn_k_norm_weights = read_optional_rms_head_norm_entry(pack, attn_k_norm)?;
    let attn_q_weights = read_u16_entry(pack, attn_q)?;
    let attn_k_weights = read_u16_entry(pack, attn_k)?;
    let attn_v_weights = read_u16_entry(pack, attn_v)?;
    let attn_o_weights = read_u16_entry(pack, attn_o)?;

    let ffn_gate = find_tensor(pack, ffn_prefix, "gate_proj")?;
    let ffn_up = find_tensor(pack, ffn_prefix, "up_proj")?;
    let ffn_down = find_tensor(pack, ffn_prefix, "down_proj")?;
    validate_entry(
        "template ffn_gate",
        ffn_gate,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "template ffn_up",
        ffn_up,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "template ffn_down",
        ffn_down,
        &[QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate],
    )?;
    validate_tile(ffn_gate, row_tile, col_tile)?;
    validate_tile(ffn_up, row_tile, col_tile)?;
    validate_tile(ffn_down, row_tile, col_tile)?;
    let ffn_gate_weights = read_u16_entry(pack, ffn_gate)?;
    let ffn_up_weights = read_u16_entry(pack, ffn_up)?;
    let ffn_down_weights = read_u16_entry(pack, ffn_down)?;

    let fallback_norm = default_hidden_norm();
    let delta = (0..QWEN35_08B.n_deltanet_layers())
        .map(|_| OwnedDeltaLayer {
            input_norm: fallback_norm.clone(),
            qkv: delta_qkv_weights.clone(),
            z: delta_z_weights.clone(),
            b: delta_b_weights.clone(),
            a: delta_a_weights.clone(),
            a_log: delta_a_log_weights.clone(),
            dt_bias: delta_dt_bias_weights.clone(),
            gated_norm: delta_gated_norm_weights.clone(),
            conv_weight: delta_conv_weight_weights.clone(),
            conv_bias: delta_conv_bias_weights.clone(),
            out: delta_out_weights.clone(),
        })
        .collect();
    let attention = (0..QWEN35_08B.n_full_attention_layers())
        .map(|_| OwnedAttentionLayer {
            input_norm: fallback_norm.clone(),
            q_norm: attn_q_norm_weights.clone(),
            k_norm: attn_k_norm_weights.clone(),
            q: attn_q_weights.clone(),
            k: attn_k_weights.clone(),
            v: attn_v_weights.clone(),
            o: attn_o_weights.clone(),
        })
        .collect();
    let ffn = (0..QWEN35_08B.n_layers)
        .map(|_| OwnedFfnLayer {
            post_norm: fallback_norm.clone(),
            gate: ffn_gate_weights.clone(),
            up: ffn_up_weights.clone(),
            down: ffn_down_weights.clone(),
        })
        .collect();

    Ok(LayerBinding {
        mode: "template-prefix-fallback",
        delta,
        attention,
        ffn,
    })
}

#[cfg(target_os = "macos")]
fn find_layer_class(
    pack: &MetalPack,
    layer_id: usize,
    class: TensorClass,
) -> Result<&MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| entry.layer == Some(layer_id) && entry.class == class)
        .ok_or_else(|| format!("missing layer {layer_id} tensor class {}", class.as_str()))
}

#[cfg(target_os = "macos")]
fn find_layer_state_param<'a>(
    pack: &'a MetalPack,
    layer_id: usize,
    marker: &str,
) -> Result<&'a MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| {
            entry.layer == Some(layer_id)
                && entry.class == TensorClass::DeltaStateParam
                && entry.tensor.contains(marker)
        })
        .ok_or_else(|| format!("missing layer {layer_id} DeltaNet state param `{marker}`"))
}

#[cfg(target_os = "macos")]
fn find_optional_layer_state_param<'a>(
    pack: &'a MetalPack,
    layer_id: usize,
    marker: &str,
) -> Option<&'a MetalPackEntry> {
    pack.entries.iter().find(|entry| {
        entry.layer == Some(layer_id)
            && entry.class == TensorClass::DeltaStateParam
            && entry.tensor.contains(marker)
    })
}

#[cfg(target_os = "macos")]
fn find_layer_norm<'a>(
    pack: &'a MetalPack,
    layer_id: usize,
    marker: &str,
) -> Result<&'a MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| {
            entry.layer == Some(layer_id)
                && entry.tensor.contains(marker)
                && (entry.class == TensorClass::LayerNorm || marker.ends_with("_norm.weight"))
        })
        .ok_or_else(|| format!("missing layer {layer_id} layer norm `{marker}`"))
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
fn validate_delta_entries(
    layer_id: usize,
    qkv: &MetalPackEntry,
    z: &MetalPackEntry,
    b: &MetalPackEntry,
    a: &MetalPackEntry,
    out: &MetalPackEntry,
    row_tile: usize,
    col_tile: usize,
) -> Result<(), String> {
    let delta_width = QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim;
    validate_entry(
        &format!("layer {layer_id} delta_qkv"),
        qkv,
        &[delta_width * 3, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        &format!("layer {layer_id} delta_z"),
        z,
        &[delta_width, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        &format!("layer {layer_id} delta_b"),
        b,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        &format!("layer {layer_id} delta_a"),
        a,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        &format!("layer {layer_id} delta_out"),
        out,
        &[QWEN35_08B.hidden_size, delta_width],
    )?;
    for entry in [qkv, z, b, a, out] {
        validate_tile(entry, row_tile, col_tile)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_attention_entries(
    layer_id: usize,
    q: &MetalPackEntry,
    k: &MetalPackEntry,
    v: &MetalPackEntry,
    o: &MetalPackEntry,
    row_tile: usize,
    col_tile: usize,
) -> Result<(), String> {
    validate_entry_one_of(
        &format!("layer {layer_id} attention_q"),
        q,
        &[
            vec![QWEN35_08B.attention_q_width(), QWEN35_08B.hidden_size],
            vec![
                QWEN35_08B.attention_q_with_head_gate_width(),
                QWEN35_08B.hidden_size,
            ],
        ],
    )?;
    validate_entry(
        &format!("layer {layer_id} attention_k"),
        k,
        &[QWEN35_08B.attention_kv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        &format!("layer {layer_id} attention_v"),
        v,
        &[QWEN35_08B.attention_kv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        &format!("layer {layer_id} attention_o"),
        o,
        &[QWEN35_08B.hidden_size, QWEN35_08B.attention_q_width()],
    )?;
    for entry in [q, k, v, o] {
        validate_tile(entry, row_tile, col_tile)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_tile(entry: &MetalPackEntry, row_tile: usize, col_tile: usize) -> Result<(), String> {
    if entry.row_tile != row_tile || entry.col_tile != col_tile {
        return Err(format!("tile mismatch for {}", entry.tensor));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_entry(label: &str, entry: &MetalPackEntry, shape: &[usize]) -> Result<(), String> {
    if !matches!(entry.dtype.as_str(), "F16" | "BF16") {
        return Err(format!(
            "{label}: expected F16/BF16 tensor, got {}",
            entry.dtype
        ));
    }
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != shape {
        return Err(format!(
            "{label}: expected shape {:?}, got {:?}",
            shape, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_entry_one_of(
    label: &str,
    entry: &MetalPackEntry,
    shapes: &[Vec<usize>],
) -> Result<(), String> {
    if !matches!(entry.dtype.as_str(), "F16" | "BF16") {
        return Err(format!(
            "{label}: expected F16/BF16 tensor, got {}",
            entry.dtype
        ));
    }
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if !shapes.iter().any(|shape| shape == &entry.source_shape) {
        return Err(format!(
            "{label}: expected one of {:?}, got {:?}",
            shapes, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_norm_vector(label: &str, entry: &MetalPackEntry) -> Result<(), String> {
    if !matches!(entry.dtype.as_str(), "F16" | "BF16" | "F32") {
        return Err(format!(
            "{label}: expected F16/BF16/F32 tensor, got {}",
            entry.dtype
        ));
    }
    if entry.layout != PackLayout::Fp16Vector {
        return Err(format!(
            "{label}: expected fp16_vector layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != [QWEN35_08B.hidden_size] {
        return Err(format!(
            "{label}: expected shape {:?}, got {:?}",
            [QWEN35_08B.hidden_size],
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_head_norm_vector(label: &str, entry: &MetalPackEntry) -> Result<(), String> {
    if !matches!(entry.dtype.as_str(), "F16" | "BF16" | "F32") {
        return Err(format!(
            "{label}: expected F16/BF16/F32 tensor, got {}",
            entry.dtype
        ));
    }
    if entry.layout != PackLayout::Fp16Vector {
        return Err(format!(
            "{label}: expected fp16_vector layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != [QWEN35_08B.attention_head_dim] {
        return Err(format!(
            "{label}: expected shape {:?}, got {:?}",
            [QWEN35_08B.attention_head_dim],
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_state_float(
    label: &str,
    entry: &MetalPackEntry,
    shape: &[usize],
) -> Result<(), String> {
    if !matches!(entry.dtype.as_str(), "F32" | "F16" | "BF16") {
        return Err(format!(
            "{label}: expected F32/F16/BF16 tensor, got {}",
            entry.dtype
        ));
    }
    if entry.layout != PackLayout::RawState {
        return Err(format!(
            "{label}: expected raw_state layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != shape {
        return Err(format!(
            "{label}: expected shape {:?}, got {:?}",
            shape, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_state_half(label: &str, entry: &MetalPackEntry, shape: &[usize]) -> Result<(), String> {
    if !matches!(entry.dtype.as_str(), "F16" | "BF16" | "F32") {
        return Err(format!(
            "{label}: expected F16/BF16/F32 tensor, got {}",
            entry.dtype
        ));
    }
    if entry.layout != PackLayout::RawState {
        return Err(format!(
            "{label}: expected raw_state layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != shape {
        return Err(format!(
            "{label}: expected shape {:?}, got {:?}",
            shape, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_state_half_one_of(
    label: &str,
    entry: &MetalPackEntry,
    shapes: &[Vec<usize>],
) -> Result<(), String> {
    if !matches!(entry.dtype.as_str(), "F16" | "BF16" | "F32") {
        return Err(format!(
            "{label}: expected F16/BF16/F32 tensor, got {}",
            entry.dtype
        ));
    }
    if entry.layout != PackLayout::RawState {
        return Err(format!(
            "{label}: expected raw_state layout, got {:?}",
            entry.layout
        ));
    }
    if !shapes.iter().any(|shape| shape == &entry.source_shape) {
        return Err(format!(
            "{label}: expected one of {:?}, got {:?}",
            shapes, entry.source_shape
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
fn read_float_state_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<f32>, String> {
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
                .map(|chunk| f16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32())
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
                .map(|chunk| bf16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32())
                .collect())
        }
        other => Err(format!(
            "{} unsupported float state dtype {other}",
            entry.tensor
        )),
    }
}

#[cfg(target_os = "macos")]
fn read_half_state_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<u16>, String> {
    let values = read_float_state_entry(pack, entry)?;
    Ok(values
        .into_iter()
        .map(|value| f16::from_f32(value).to_bits())
        .collect())
}

#[cfg(target_os = "macos")]
fn read_optional_half_state_entry(
    pack: &MetalPack,
    entry: Option<&MetalPackEntry>,
) -> Result<Vec<u16>, String> {
    match entry {
        Some(entry) => read_half_state_entry(pack, entry),
        None => Ok(vec![0u16; QWEN35_08B.deltanet_qkv_width()]),
    }
}

#[cfg(target_os = "macos")]
fn read_rms_norm_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<u16>, String> {
    let values = read_float_state_entry(pack, entry)?;
    Ok(values
        .into_iter()
        .map(|value| f16::from_f32(1.0 + value).to_bits())
        .collect())
}

fn read_optional_rms_head_norm_entry(
    pack: &MetalPack,
    entry: Option<&MetalPackEntry>,
) -> Result<Vec<u16>, String> {
    match entry {
        Some(entry) => read_rms_norm_entry(pack, entry),
        None => Ok(default_head_norm()),
    }
}

#[cfg(target_os = "macos")]
fn default_hidden_norm() -> Vec<u16> {
    (0..QWEN35_08B.hidden_size)
        .map(|i| f16::from_f32(1.0 + ((i % 17) as f32 - 8.0) / 256.0).to_bits())
        .collect()
}

#[cfg(target_os = "macos")]
fn default_head_norm() -> Vec<u16> {
    vec![f16::from_f32(1.0).to_bits(); QWEN35_08B.attention_head_dim]
}
