//! Cache and memory-traffic model for the Qwen3.5-0.8B Metal probe.
//!
//! This is a planning model, not a substitute for Apple GPU counters. It makes
//! cache assumptions explicit so benchmark results can be interpreted against a
//! stable byte model and then checked with Metal System Trace captures.

use crate::QWEN35_08B;

fn active_down_token_tile() -> usize {
    if std::env::var_os("CTOX_QWEN35_DOWN_MMA32").is_some() {
        32
    } else {
        64
    }
}

fn active_delta_qkvz_token_tile() -> usize {
    if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128").is_some() {
        128
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64").is_some() {
        64
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32").is_some() {
        32
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA16").is_some() {
        16
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA8").is_some() {
        8
    } else {
        64
    }
}

fn active_delta_out_token_tile() -> usize {
    if std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA64").is_some() {
        64
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA32").is_some() {
        32
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA16").is_some() {
        16
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_TOK8").is_some() {
        8
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_TOK2").is_some() {
        2
    } else {
        64
    }
}

fn active_gate_up_token_tile() -> usize {
    if std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA32").is_some() {
        32
    } else {
        64
    }
}

fn active_decode_splitk_key_block() -> usize {
    match std::env::var("CTOX_QWEN35_DECODE_ATTENTION_SPLITK_BLOCK")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(128)
    {
        128 => 128,
        512 => 512,
        1024 => 1024,
        _ => 256,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CacheModelConfig {
    pub tokens: usize,
    pub decode_position: usize,
    pub modeled_l2_bytes: usize,
    pub sustained_bandwidth_bytes_s: f64,
}

impl Default for CacheModelConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            decode_position: 4096,
            modeled_l2_bytes: 32 * 1024 * 1024,
            sustained_bandwidth_bytes_s: 90.0e9,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheResidency {
    FitsModeledL2,
    StreamsBeyondModeledL2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterPriority {
    Required,
    Useful,
}

#[derive(Clone, Debug)]
pub struct CacheCounterPlan {
    pub priority: CounterPriority,
    pub question: &'static str,
}

#[derive(Clone, Debug)]
pub struct CacheOpAnalysis {
    pub op: &'static str,
    pub kernel_family: &'static str,
    pub calls_per_layer: usize,
    pub layers_per_model: usize,
    pub token_tile: usize,
    pub working_set_bytes: usize,
    pub logical_bytes: usize,
    pub modeled_dram_miss_bytes: usize,
    pub modeled_cache_hit_bytes: usize,
    pub modeled_hit_rate: f64,
    pub residency: CacheResidency,
    pub dominant: &'static str,
    pub optimization: &'static str,
    pub counters: Vec<CacheCounterPlan>,
}

impl CacheOpAnalysis {
    pub fn modeled_time_ms(&self, cfg: CacheModelConfig) -> f64 {
        self.modeled_dram_miss_bytes as f64 / cfg.sustained_bandwidth_bytes_s * 1_000.0
    }
}

pub fn qwen35_cache_analysis(cfg: CacheModelConfig) -> Vec<CacheOpAnalysis> {
    let h = QWEN35_08B.hidden_size;
    let i = QWEN35_08B.ffn_intermediate;
    let vocab = QWEN35_08B.vocab_size;
    let delta_width = QWEN35_08B.deltanet_width();
    let delta_qkv = QWEN35_08B.deltanet_qkv_width();
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let attn_q = QWEN35_08B.attention_q_width();
    let attn_kv = QWEN35_08B.attention_kv_width();
    let attn_head_dim = QWEN35_08B.attention_head_dim;
    let token = cfg.tokens;
    let qkvz_tile = active_delta_qkvz_token_tile();
    let delta_out_tile = active_delta_out_token_tile();
    let gate_up_tile = active_gate_up_token_tile();

    let mut ops = Vec::new();
    ops.push(weight_stream_op(
        cfg,
        "delta.project.qkv",
        match qkvz_tile {
            128 => "prefill_matmul MMA128 for qkv",
            64 => "prefill_matmul MMA64 for qkv",
            32 => "prefill_matmul MMA32 for qkv",
            16 => "prefill_matmul MMA16 for qkv",
            _ => "prefill_matmul MMA8 for qkv",
        },
        QWEN35_08B.n_deltanet_layers(),
        delta_qkv * h * 2,
        token,
        qkvz_tile,
        token * (h * 2 + h * 2 + delta_qkv * 4),
        "FP16 weight streaming",
        "Active QKV/Z tile must be checked against achieved GB/s and register pressure before promotion.",
    ));
    ops.push(weight_stream_op(
        cfg,
        "delta.project.z",
        match qkvz_tile {
            128 => "prefill_matmul MMA128 for z",
            64 => "prefill_matmul MMA64 for z",
            32 => "prefill_matmul MMA32 for z",
            16 => "prefill_matmul MMA16 for z",
            _ => "prefill_matmul MMA8 for z",
        },
        QWEN35_08B.n_deltanet_layers(),
        delta_width * h * 2,
        token,
        qkvz_tile,
        token * (h * 2 + h * 2 + delta_width * 4),
        "FP16 weight streaming",
        "Active QKV/Z tile must be checked against achieved GB/s and register pressure before promotion.",
    ));
    ops.push(weight_stream_op(
        cfg,
        "delta.project.b/a",
        "fused b/a project + beta/decay activate",
        QWEN35_08B.n_deltanet_layers() * 2,
        heads * h * 2,
        token,
        4,
        token * (h * 2 + h * 2 + heads * 4),
        "small projection dispatch overhead",
        "Fused path is correct but only a micro-optimization; not the main bottleneck.",
    ));
    ops.push(sequence_state_op(
        cfg,
        "delta.conv1d",
        "prefill_deltanet_causal_conv1d",
        QWEN35_08B.n_deltanet_layers(),
        delta_qkv * 4 * 2 + delta_qkv * 2,
        token * delta_qkv * (4 + 4 + 2),
        "tiny depthwise conv state",
        "Already not dominant; keep it chained in the block.",
    ));
    ops.push(sequence_state_op(
        cfg,
        "delta.prepare.qkv_beta_decay",
        "prefill_deltanet_prepare",
        QWEN35_08B.n_deltanet_layers(),
        token * (delta_qkv * 4 + delta_width * 2 * 3 + heads * 4 * 4),
        token * (delta_qkv * 4 + delta_width * 2 * 3 + heads * 4 * 4),
        "activation traffic",
        "Not dominant; validate q/k norm cache behavior with trace counters.",
    ));
    let delta_state_bytes = heads * head_dim * head_dim * 4;
    ops.push(recurrent_state_op(
        cfg,
        "delta.recurrent_scan",
        "prefill_deltanet_scan",
        QWEN35_08B.n_deltanet_layers(),
        delta_state_bytes,
        token,
        token * (delta_width * 2 * 3 + delta_width * 4 + heads * 4 * 2),
        "recurrent state reuse",
        "State should fit modeled L2; trace must confirm low DRAM pressure after first token.",
    ));
    ops.push(sequence_state_op(
        cfg,
        "delta.gated_rmsnorm",
        "prefill_deltanet_gated_rmsnorm",
        QWEN35_08B.n_deltanet_layers(),
        token * (delta_width * 4 * 2 + delta_width * 2 + head_dim * 4),
        token * (delta_width * 4 * 2 + delta_width * 2 + head_dim * 4),
        "activation traffic",
        "Fuse with scan output if trace shows activation write/read misses.",
    ));
    ops.push(weight_stream_op(
        cfg,
        "delta.out_proj",
        match delta_out_tile {
            64 => "prefill_deltanet_out MMA64 residual",
            32 => "prefill_deltanet_out MMA32",
            16 => "prefill_deltanet_out MMA16",
            8 => "prefill_deltanet_out tok8",
            2 => "prefill_deltanet_out tok2",
            _ => "prefill_deltanet_out tok4",
        },
        QWEN35_08B.n_deltanet_layers(),
        h * delta_width * 2,
        token,
        delta_out_tile,
        token * (delta_width * 2 + h * 4),
        "FP16 weight streaming",
        "DeltaOut tile is part of the accepted profile; any faster candidate still needs hidden-dump equality and full-profile p95 evidence.",
    ));
    ops.push(weight_stream_op(
        cfg,
        "ffn.gate_up_swiglu",
        if gate_up_tile == 64 {
            "prefill_ffn_gate_up MMA64"
        } else {
            "prefill_ffn_gate_up MMA32"
        },
        QWEN35_08B.n_layers,
        i * h * 2 * 2,
        token,
        gate_up_tile,
        token * (h * 2 + h * 2 + i * 2),
        "FP16 weight streaming",
        if gate_up_tile == 64 {
            "MMA64 is active; validate register pressure before assuming byte-model gains."
        } else {
            "MMA32 is active; next reduce launch/scratch overhead around FFN."
        },
    ));
    ops.push(weight_stream_op(
        cfg,
        "ffn.down",
        if active_down_token_tile() == 64 {
            "prefill_down_matmul MMA64"
        } else {
            "prefill_down_matmul MMA32"
        },
        QWEN35_08B.n_layers,
        h * i * 2,
        token,
        active_down_token_tile(),
        token * (i * 2 + h * 4),
        "FP16 weight streaming",
        if active_down_token_tile() == 64 {
            "MMA64 candidate is active; validate register pressure against the byte-stream reduction."
        } else {
            "MMA32 is active; remaining FFN issue is scratch/dispatch and accumulation reuse."
        },
    ));
    ops.push(weight_stream_op(
        cfg,
        "attention.q_gate",
        "prefill attention q/gate MMA16",
        QWEN35_08B.n_full_attention_layers(),
        QWEN35_08B.attention_q_with_head_gate_width() * h * 2,
        token,
        16,
        token * (h * 2 + h * 2 + QWEN35_08B.attention_q_with_head_gate_width() * 4),
        "FP16 weight streaming",
        "MMA16 projection is active; remaining long-context pressure is KV traffic.",
    ));
    ops.push(weight_stream_op(
        cfg,
        "attention.kv",
        "prefill attention kv MMA16",
        QWEN35_08B.n_full_attention_layers() * 2,
        attn_kv * h * 2,
        token,
        16,
        token * (h * 2 + h * 2 + attn_kv * 4),
        "FP16 weight streaming",
        "MMA16 projection is active; K/V remain small versus attention T^2 traffic.",
    ));
    let prefill_attention_logical_bytes =
        token * (token + 1) / 2 * attn_q / attn_head_dim * attn_head_dim * 2 * 2;
    let prefill_attention_qh4_bytes =
        token * (token + 1) / 2 * QWEN35_08B.attention_kv_heads * attn_head_dim * 2 * 2;
    ops.push(streaming_reuse_op(
        cfg,
        "attention.prefill_kv_stream",
        "qh4 SIMD32 vec8 exact prefill attention",
        QWEN35_08B.n_full_attention_layers(),
        prefill_attention_qh4_bytes,
        prefill_attention_logical_bytes,
        prefill_attention_qh4_bytes,
        "causal T^2 K/V stream",
        "Exact qh4 already reuses each KV head across four Q heads. Remaining exact win requires query-block K/V reuse without register-pressure collapse, or a lower-precision KV-cache format.",
    ));
    let kv_read_bytes = cfg.decode_position * attn_kv * 2 * 2;
    ops.push(streaming_op(
        cfg,
        "attention.kv_cache_read",
        "qh4 online attention decode",
        QWEN35_08B.n_full_attention_layers(),
        kv_read_bytes,
        kv_read_bytes,
        "context-length KV streaming",
        "qh4 removes repeated KV reads across Q heads; next target is long-context Split-K and KV quantization.",
    ));
    ops.push(decode_splitk_scratch_op(cfg));
    ops.push(weight_stream_op(
        cfg,
        "attention.o_proj",
        "prefill attention out MMA16 candidate",
        QWEN35_08B.n_full_attention_layers(),
        h * attn_q * 2,
        token,
        16,
        token * (attn_q * 2 + h * 4),
        "FP16 weight streaming",
        "MMA16 candidate is active in optimized forensics; validate with full attention dumps.",
    ));
    ops.push(weight_stream_op(
        cfg,
        "lm_head",
        "lm_head_argmax_tiled",
        1,
        vocab * h * 2,
        1,
        1,
        h * 2 + vocab * (4 + 4),
        "huge vocab weight stream",
        "Do GPU-local argmax/top-k; later quantize or shortlist.",
    ));
    ops.push(streaming_op(
        cfg,
        "embedding",
        "embedding lookup",
        1,
        h * 2,
        h * 2,
        "tiny token read",
        "Not relevant for bandwidth.",
    ));
    ops.push(streaming_op(
        cfg,
        "final_norm",
        "RMSNorm",
        1,
        h * (2 + 2 + 2),
        h * (2 + 2 + 2),
        "tiny activation traffic",
        "Fuse into LM-head when the LM-head path is stable.",
    ));

    ops
}

fn decode_splitk_scratch_op(cfg: CacheModelConfig) -> CacheOpAnalysis {
    let key_block = active_decode_splitk_key_block();
    let n_key_blocks = cfg.decode_position.max(1).div_ceil(key_block);
    let q_heads = QWEN35_08B.attention_q_heads;
    let head_dim = QWEN35_08B.attention_head_dim;
    let partial_scalars = q_heads * n_key_blocks;
    let partial_scalar_bytes = partial_scalars * std::mem::size_of::<f32>() * 2;
    let partial_acc_bytes = partial_scalars * head_dim * std::mem::size_of::<f32>();
    let scratch_bytes = partial_scalar_bytes + partial_acc_bytes;
    let logical_bytes = scratch_bytes * 2;
    let modeled_miss_bytes = if scratch_bytes <= cfg.modeled_l2_bytes {
        scratch_bytes
    } else {
        logical_bytes
    };
    op_analysis(
        cfg,
        "attention.decode_splitk_scratch",
        "Split-K partial m/l/acc write + combine read",
        1,
        QWEN35_08B.n_full_attention_layers(),
        key_block,
        scratch_bytes,
        logical_bytes,
        modeled_miss_bytes,
        "Split-K scratch traffic",
        "Autotune key_block and reduce partial_acc traffic; Split-K is only useful once context parallelism beats scratch write/read cost.",
    )
}

fn weight_stream_op(
    cfg: CacheModelConfig,
    op: &'static str,
    kernel_family: &'static str,
    layers_per_model: usize,
    weight_bytes: usize,
    tokens: usize,
    token_tile: usize,
    non_weight_logical_bytes: usize,
    dominant: &'static str,
    optimization: &'static str,
) -> CacheOpAnalysis {
    let token_groups = tokens.div_ceil(token_tile.max(1));
    let logical_weight_bytes = weight_bytes * tokens;
    let miss_weight_bytes = weight_bytes * token_groups;
    let logical_bytes = logical_weight_bytes + non_weight_logical_bytes;
    let modeled_dram_miss_bytes = miss_weight_bytes + non_weight_logical_bytes;
    op_analysis(
        cfg,
        op,
        kernel_family,
        1,
        layers_per_model,
        token_tile,
        weight_bytes,
        logical_bytes,
        modeled_dram_miss_bytes,
        dominant,
        optimization,
    )
}

fn recurrent_state_op(
    cfg: CacheModelConfig,
    op: &'static str,
    kernel_family: &'static str,
    layers_per_model: usize,
    state_bytes: usize,
    tokens: usize,
    non_state_logical_bytes: usize,
    dominant: &'static str,
    optimization: &'static str,
) -> CacheOpAnalysis {
    let logical_state_bytes = state_bytes * tokens * 3;
    let modeled_state_miss_bytes = if state_bytes <= cfg.modeled_l2_bytes {
        state_bytes * 2
    } else {
        logical_state_bytes
    };
    op_analysis(
        cfg,
        op,
        kernel_family,
        1,
        layers_per_model,
        1,
        state_bytes,
        logical_state_bytes + non_state_logical_bytes,
        modeled_state_miss_bytes + non_state_logical_bytes,
        dominant,
        optimization,
    )
}

fn sequence_state_op(
    cfg: CacheModelConfig,
    op: &'static str,
    kernel_family: &'static str,
    layers_per_model: usize,
    working_set_bytes: usize,
    logical_bytes: usize,
    dominant: &'static str,
    optimization: &'static str,
) -> CacheOpAnalysis {
    let modeled_dram_miss_bytes = if working_set_bytes <= cfg.modeled_l2_bytes {
        logical_bytes.min(working_set_bytes + logical_bytes / 4)
    } else {
        logical_bytes
    };
    op_analysis(
        cfg,
        op,
        kernel_family,
        1,
        layers_per_model,
        1,
        working_set_bytes,
        logical_bytes,
        modeled_dram_miss_bytes,
        dominant,
        optimization,
    )
}

fn streaming_op(
    cfg: CacheModelConfig,
    op: &'static str,
    kernel_family: &'static str,
    layers_per_model: usize,
    working_set_bytes: usize,
    logical_bytes: usize,
    dominant: &'static str,
    optimization: &'static str,
) -> CacheOpAnalysis {
    op_analysis(
        cfg,
        op,
        kernel_family,
        1,
        layers_per_model,
        1,
        working_set_bytes,
        logical_bytes,
        logical_bytes,
        dominant,
        optimization,
    )
}

#[allow(clippy::too_many_arguments)]
fn streaming_reuse_op(
    cfg: CacheModelConfig,
    op: &'static str,
    kernel_family: &'static str,
    layers_per_model: usize,
    working_set_bytes: usize,
    logical_bytes: usize,
    modeled_dram_miss_bytes: usize,
    dominant: &'static str,
    optimization: &'static str,
) -> CacheOpAnalysis {
    op_analysis(
        cfg,
        op,
        kernel_family,
        1,
        layers_per_model,
        1,
        working_set_bytes,
        logical_bytes,
        modeled_dram_miss_bytes,
        dominant,
        optimization,
    )
}

#[allow(clippy::too_many_arguments)]
fn op_analysis(
    cfg: CacheModelConfig,
    op: &'static str,
    kernel_family: &'static str,
    calls_per_layer: usize,
    layers_per_model: usize,
    token_tile: usize,
    working_set_bytes: usize,
    logical_bytes: usize,
    modeled_dram_miss_bytes: usize,
    dominant: &'static str,
    optimization: &'static str,
) -> CacheOpAnalysis {
    let modeled_cache_hit_bytes = logical_bytes.saturating_sub(modeled_dram_miss_bytes);
    let modeled_hit_rate = if logical_bytes == 0 {
        0.0
    } else {
        modeled_cache_hit_bytes as f64 / logical_bytes as f64
    };
    let residency = if working_set_bytes <= cfg.modeled_l2_bytes {
        CacheResidency::FitsModeledL2
    } else {
        CacheResidency::StreamsBeyondModeledL2
    };
    CacheOpAnalysis {
        op,
        kernel_family,
        calls_per_layer,
        layers_per_model,
        token_tile,
        working_set_bytes,
        logical_bytes,
        modeled_dram_miss_bytes,
        modeled_cache_hit_bytes,
        modeled_hit_rate,
        residency,
        dominant,
        optimization,
        counters: counter_plan_for(dominant),
    }
}

fn counter_plan_for(dominant: &'static str) -> Vec<CacheCounterPlan> {
    let mut counters = vec![
        CacheCounterPlan {
            priority: CounterPriority::Required,
            question: "GPU device memory read/write bytes per dispatch",
        },
        CacheCounterPlan {
            priority: CounterPriority::Required,
            question: "L2/system-cache hit rate or equivalent cache-miss counter",
        },
        CacheCounterPlan {
            priority: CounterPriority::Useful,
            question: "threadgroup occupancy and stall reason breakdown",
        },
    ];
    if dominant.contains("KV") || dominant.contains("state") {
        counters.push(CacheCounterPlan {
            priority: CounterPriority::Required,
            question: "cache residency across sequential token loop or context scan",
        });
    }
    counters
}

pub fn format_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.2} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_stream_hit_rate_reflects_token_tile_reuse() {
        let cfg = CacheModelConfig {
            tokens: 512,
            ..CacheModelConfig::default()
        };
        let op = weight_stream_op(cfg, "test", "kernel", 1, 1024, 512, 4, 0, "weights", "none");
        assert!((op.modeled_hit_rate - 0.75).abs() < 0.001);
    }

    #[test]
    fn qwen_analysis_covers_hot_path_families() {
        let ops = qwen35_cache_analysis(CacheModelConfig::default());
        for name in [
            "delta.project.qkv",
            "delta.recurrent_scan",
            "ffn.gate_up_swiglu",
            "attention.prefill_kv_stream",
            "attention.kv_cache_read",
            "attention.decode_splitk_scratch",
            "lm_head",
        ] {
            assert!(ops.iter().any(|op| op.op == name), "missing {name}");
        }
    }
}
