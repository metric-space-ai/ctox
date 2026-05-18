//! First Metal microbenchmark: synthetic stream read/write.

use std::time::Instant;

use half::f16;

use crate::metal::ffi::{Buffer, CommandBuffer, Device};
use crate::metal::mps_sidecar::{device_supports_mps, MpsTiledAttentionPlan};
use crate::QWEN35_08B;

fn new_readonly_buffer<T: Copy>(dev: &Device, data: &[T]) -> Result<Buffer, String> {
    if std::env::var_os("CTOX_QWEN35_SHARED_WEIGHTS").is_none() {
        return dev.new_private_buffer_with_data(data);
    }

    let buf = dev.new_buffer(std::mem::size_of_val(data))?;
    unsafe {
        buf.write(0, data);
    }
    Ok(buf)
}

fn new_readonly_byte_buffer(dev: &Device, data: &[u8]) -> Result<Buffer, String> {
    let buf = dev.new_buffer(data.len())?;
    unsafe {
        buf.write(0, data);
    }
    Ok(buf)
}

#[derive(Clone, Copy, Debug)]
pub struct StreamBenchConfig {
    pub bytes: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for StreamBenchConfig {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StreamBenchResult {
    pub bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: u32,
}

pub fn run_stream_bench(config: StreamBenchConfig) -> Result<StreamBenchResult, String> {
    let bytes = round_down_to_vec4(config.bytes);
    if bytes == 0 {
        return Err("stream bench bytes rounded to zero".to_string());
    }
    let n_u32 = bytes / std::mem::size_of::<u32>();
    let n_vec4 = bytes / 16;
    let n_vec4_u32 = u32::try_from(n_vec4).map_err(|_| "benchmark buffer too large")?;

    let dev = Device::default_system()?;
    let src = dev.new_buffer(bytes)?;
    let dst = dev.new_buffer(bytes)?;
    let seed: Vec<u32> = (0..n_u32).map(|i| i as u32 ^ 0x9e37_79b9).collect();
    unsafe {
        src.write(0, &seed);
    }

    for _ in 0..config.warmup {
        dispatch_once(&dev, &src, &dst, n_vec4_u32)?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_once(&dev, &src, &dst, n_vec4_u32)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut check = [0u32; 4];
    unsafe {
        dst.read(0, &mut check);
    }
    let checksum = check.iter().fold(0u32, |acc, v| acc ^ *v);
    let effective_bytes = (bytes * 2) as f64;
    let effective_gb_s = effective_bytes / median_s.max(1e-12) / 1e9;

    Ok(StreamBenchResult {
        bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

fn dispatch_once(
    dev: &Device,
    src: &crate::metal::ffi::Buffer,
    dst: &crate::metal::ffi::Buffer,
    n_vec4: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_stream_rw_u32x4")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, src, 0);
    enc.set_buffer(1, dst, 0);
    enc.set_bytes(2, &n_vec4);
    enc.dispatch_threads(n_vec4 as usize, 256);
    enc.end();
    cmd.commit_and_wait()
}

fn round_down_to_vec4(bytes: usize) -> usize {
    bytes / 16 * 16
}

fn percentile_sorted(samples: &[f64], q: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let idx = ((samples.len() - 1) as f64 * q).round() as usize;
    samples[idx.min(samples.len() - 1)]
}

fn sparse_f32_matrix_checksum(buffer: &crate::metal::ffi::Buffer, rows: usize, cols: usize) -> f32 {
    if rows == 0 || cols == 0 {
        return 0.0;
    }
    let row_samples = sparse_indices(rows);
    let col_samples = sparse_indices(cols);
    let mut checksum = 0.0f32;
    for row in row_samples {
        for col in &col_samples {
            let mut value = [0.0f32; 1];
            let element = row * cols + *col;
            unsafe {
                buffer.read(element * std::mem::size_of::<f32>(), &mut value);
            }
            checksum += value[0];
        }
    }
    checksum
}

fn sparse_indices(len: usize) -> Vec<usize> {
    let mut values = vec![
        0,
        len / 7,
        len / 3,
        len / 2,
        len * 2 / 3,
        len * 6 / 7,
        len - 1,
    ];
    values.sort_unstable();
    values.dedup();
    values
}

fn prefill_rms_matmul_kernel() -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_PREFILL_RMS_TOK2").is_some() {
        (
            "qwen35_08b_prefill_rms_matmul_rowtiles_tok2_fp16_tiled_k1024_f32",
            2,
        )
    } else if std::env::var_os("CTOX_QWEN35_PREFILL_RMS_TOK8").is_some() {
        (
            "qwen35_08b_prefill_rms_matmul_rowtiles_tok8_simd_fp16_tiled_k1024_f32",
            8,
        )
    } else {
        (
            "qwen35_08b_prefill_rms_matmul_rowtiles_tok4_simd_fp16_tiled_k1024_f32",
            4,
        )
    }
}

fn prefill_project_split_norm_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_PROJECT_SPLIT_NORM").is_some()
}

fn prefill_project_mma_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_PROJECT_MMA").is_some()
}

fn prefill_delta_project_qkvz_mma_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_NO_MMA").is_none()
}

fn prefill_delta_ba_fused_activate_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_BA_FUSED_NO").is_none()
}

fn prefill_delta_project_qkvz_mma_kernel() -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED").is_some() {
        (
            "qwen35_08b_prefill_matmul_mma128x8_rg4_ashared_fp16_tiled_k1024_f32",
            128,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128").is_some() {
        (
            "qwen35_08b_prefill_matmul_mma128x8_fp16_tiled_k1024_f32",
            128,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64").is_some() {
        ("qwen35_08b_prefill_matmul_mma64x8_fp16_tiled_k1024_f32", 64)
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32").is_some() {
        ("qwen35_08b_prefill_matmul_mma32x8_fp16_tiled_k1024_f32", 32)
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA16").is_some() {
        ("qwen35_08b_prefill_matmul_mma16x8_fp16_tiled_k1024_f32", 16)
    } else {
        ("qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32", 8)
    }
}

fn prefill_delta_project_qkvz_mma_kernel_for_tokens(tokens: usize) -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED").is_some() {
        (
            "qwen35_08b_prefill_matmul_mma128x8_rg4_ashared_fp16_tiled_k1024_f32",
            128,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128").is_some() {
        (
            "qwen35_08b_prefill_matmul_mma128x8_fp16_tiled_k1024_f32",
            128,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64").is_some() {
        ("qwen35_08b_prefill_matmul_mma64x8_fp16_tiled_k1024_f32", 64)
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32").is_some() {
        ("qwen35_08b_prefill_matmul_mma32x8_fp16_tiled_k1024_f32", 32)
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA8").is_some() {
        ("qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32", 8)
    } else if std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA16").is_some()
        || tokens.is_multiple_of(16)
    {
        ("qwen35_08b_prefill_matmul_mma16x8_fp16_tiled_k1024_f32", 16)
    } else {
        prefill_delta_project_qkvz_mma_kernel()
    }
}

fn prefill_delta_project_qkvz_mma_rg4_ashared_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED").is_some()
}

fn prefill_mps_qkvz_direct_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_MPS_QKVZ_DIRECT").is_some()
}

fn prefill_attention_project_mma_kernel_for_tokens(tokens: usize) -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_ATTENTION_PROJECT_MMA8").is_some() {
        ("qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32", 8)
    } else if tokens.is_multiple_of(16) {
        ("qwen35_08b_prefill_matmul_mma16x8_fp16_tiled_k1024_f32", 16)
    } else {
        ("qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32", 8)
    }
}

fn prefill_project_matmul_kernel() -> (&'static str, usize) {
    if prefill_project_split_norm_enabled() {
        if prefill_project_mma_enabled() {
            ("qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32", 8)
        } else {
            (
                "qwen35_08b_prefill_matmul_rowtiles_tok4_simd_fp16_tiled_k1024_f32",
                4,
            )
        }
    } else {
        prefill_rms_matmul_kernel()
    }
}

fn prefill_project_matmul_threadgroup_threads() -> usize {
    if prefill_project_split_norm_enabled() && prefill_project_mma_enabled() {
        32
    } else {
        256
    }
}

fn prefill_ffn_gate_up_kernel() -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_TOK2").is_some() {
        (
            "qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok2_fp16_tiled_k1024_i3584",
            2,
        )
    } else if std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_TOK8").is_some() {
        (
            "qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok8_simd_fp16_tiled_k1024_i3584",
            8,
        )
    } else {
        (
            "qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok4_simd_fp16_tiled_k1024_i3584",
            4,
        )
    }
}

fn prefill_ffn_gate_up_mma_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA").is_some()
        || std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA16").is_some()
        || std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA32").is_some()
        || std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA64").is_some()
}

fn prefill_ffn_gate_up_mma_kernel() -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA64").is_some() {
        (
            "qwen35_08b_prefill_ffn_gate_up_mma64x8_normed_fp16_tiled_k1024_i3584",
            64,
        )
    } else if std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA32").is_some() {
        (
            "qwen35_08b_prefill_ffn_gate_up_mma32x8_normed_fp16_tiled_k1024_i3584",
            32,
        )
    } else if std::env::var_os("CTOX_QWEN35_FFN_GATE_UP_MMA16").is_some() {
        (
            "qwen35_08b_prefill_ffn_gate_up_mma16x8_normed_fp16_tiled_k1024_i3584",
            16,
        )
    } else {
        (
            "qwen35_08b_prefill_ffn_gate_up_mma8x8_normed_fp16_tiled_k1024_i3584",
            8,
        )
    }
}

fn prefill_delta_ffn_gate_up_token_tile() -> usize {
    if prefill_ffn_gate_up_mma_enabled() {
        let (_, token_tile) = prefill_ffn_gate_up_mma_kernel();
        token_tile
    } else {
        let (_, token_tile) = prefill_ffn_gate_up_kernel();
        token_tile
    }
}

fn prefill_down_matmul_kernel() -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_DOWN_MMA64").is_some() {
        ("qwen35_08b_prefill_down_mma64x8_fp16_tiled_k3584_f32", 64)
    } else if std::env::var_os("CTOX_QWEN35_DOWN_MMA32").is_some() {
        ("qwen35_08b_prefill_down_mma32x8_fp16_tiled_k3584_f32", 32)
    } else if std::env::var_os("CTOX_QWEN35_DOWN_MMA16").is_some() {
        ("qwen35_08b_prefill_down_mma16x8_fp16_tiled_k3584_f32", 16)
    } else if std::env::var_os("CTOX_QWEN35_DOWN_MMA").is_some() {
        ("qwen35_08b_prefill_down_mma8x8_fp16_tiled_k3584_f32", 8)
    } else if std::env::var_os("CTOX_QWEN35_DOWN_TOK2").is_some() {
        (
            "qwen35_08b_prefill_down_matmul_rowtiles_tok2_fp16_tiled_k3584_f32",
            2,
        )
    } else if std::env::var_os("CTOX_QWEN35_DOWN_TOK8").is_some() {
        (
            "qwen35_08b_prefill_down_matmul_rowtiles_tok8_simd_fp16_tiled_k3584_f32",
            8,
        )
    } else {
        (
            "qwen35_08b_prefill_down_matmul_rowtiles_tok4_simd_fp16_tiled_k3584_f32",
            4,
        )
    }
}

fn prefill_down_threadgroup_threads() -> usize {
    if std::env::var_os("CTOX_QWEN35_DOWN_MMA").is_some()
        || std::env::var_os("CTOX_QWEN35_DOWN_MMA16").is_some()
        || std::env::var_os("CTOX_QWEN35_DOWN_MMA32").is_some()
        || std::env::var_os("CTOX_QWEN35_DOWN_MMA64").is_some()
    {
        32
    } else {
        256
    }
}

fn prefill_down_mma_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DOWN_MMA").is_some()
        || std::env::var_os("CTOX_QWEN35_DOWN_MMA16").is_some()
        || std::env::var_os("CTOX_QWEN35_DOWN_MMA32").is_some()
        || std::env::var_os("CTOX_QWEN35_DOWN_MMA64").is_some()
}

fn prefill_down_mma32_residual_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DOWN_MMA32_RESIDUAL").is_some()
}

fn prefill_down_mma64_residual_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DOWN_MMA64_RESIDUAL").is_some()
}

fn prefill_down_mma_residual_kernel(token_tile: usize) -> Option<&'static str> {
    match token_tile {
        32 if prefill_down_mma32_residual_enabled() => {
            Some("qwen35_08b_prefill_down_mma32x8_residual_fp16_tiled_k3584_f32")
        }
        64 if prefill_down_mma64_residual_enabled() => {
            Some("qwen35_08b_prefill_down_mma64x8_residual_fp16_tiled_k3584_f32")
        }
        _ => None,
    }
}

fn prefill_attention_qblk4_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QBLK4").is_some()
}

fn prefill_attention_qblk2_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QBLK2").is_some()
}

fn prefill_attention_qblk2_simdreduce_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QBLK2_SIMDREDUCE").is_some()
}

fn prefill_attention_qblk4_simdreduce_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE").is_some()
}

fn prefill_attention_qblk4_simdreduce_batch_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH").is_some()
}

fn prefill_attention_qblk8_simdreduce_batch_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QBLK8_SIMDREDUCE_BATCH").is_some()
}

fn prefill_attention_qh2_qblk4_simdreduce_batch_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH2_QBLK4_SIMDREDUCE_BATCH").is_some()
}

fn prefill_attention_qh4_qblk2_simdreduce_batch_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_QBLK2_SIMDREDUCE_BATCH").is_some()
}

fn prefill_attention_qh4_simd32_vec8_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8").is_some()
}

fn prefill_attention_qh4_simd32_vec8_interleaved_kv_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INTERLEAVED_KV").is_some()
}

fn prefill_attention_qh4_simd32_vec8_int8_kv_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_KV").is_some()
}

fn prefill_attention_qh4_simd32_vec8_int8_v_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V").is_some()
}

fn prefill_attention_qh4_simd32_vec8_int8_v_pack4_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V_PACK4").is_some()
}

fn prefill_attention_qh4_simd32_vec8_halfacc_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFACC").is_some()
}

fn prefill_attention_qh4_simd32_vec8_halfdot_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT").is_some()
}

fn prefill_attention_qh4_qblk2_simd32_vec8_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_QBLK2_SIMD32_VEC8").is_some()
}

fn prefill_attention_qh4_simd32_vec8_win4096_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WIN4096").is_some()
}

fn prefill_attention_qh4_simd32_vec8_window() -> Result<Option<u32>, String> {
    match std::env::var("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW") {
        Ok(value) => {
            let window = value.parse::<u32>().map_err(|err| {
                format!("invalid CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW `{value}`: {err}")
            })?;
            if window == 0 {
                return Err("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW must be > 0".to_owned());
            }
            Ok(Some(window))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW must be valid UTF-8".to_owned())
        }
    }
}

fn prefill_attention_qh4_simd32_vec8_window_halfdot() -> Result<Option<u32>, String> {
    match std::env::var("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT") {
        Ok(value) => {
            let window = value.parse::<u32>().map_err(|err| {
                format!(
                    "invalid CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT `{value}`: {err}"
                )
            })?;
            if window == 0 {
                return Err(
                    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT must be > 0".to_owned(),
                );
            }
            Ok(Some(window))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(
            "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT must be valid UTF-8".to_owned(),
        ),
    }
}

fn prefill_attention_qh4_qblk1_simdreduce_batch_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_QBLK1_SIMDREDUCE_BATCH").is_some()
}

fn prefill_attention_qblk2x512_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QBLK2X512").is_some()
}

fn prefill_attention_partial_qblk2_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_PARTIAL_QBLK2").is_some()
}

fn prefill_attention_qh4_splitk64_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SPLITK64").is_some()
}

fn prefill_attention_qh4_splitk128_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SPLITK128").is_some()
}

fn prefill_attention_qh4_splitk256_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SPLITK256").is_some()
}

fn prefill_attention_qh4_splitk512_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_QH4_SPLITK512").is_some()
}

fn prefill_attention_mps_tiled_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_MPS_TILED").is_some()
}

fn prefill_attention_mps_tile_env(name: &str, default: usize) -> Result<usize, String> {
    match std::env::var(name) {
        Ok(raw) => raw
            .parse::<usize>()
            .map_err(|err| format!("invalid {name} `{raw}`: {err}"))
            .and_then(|value| {
                if value == 0 {
                    Err(format!("{name} must be > 0"))
                } else {
                    Ok(value)
                }
            }),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

fn prefill_attention_simdreduce_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE").is_none()
}

fn decode_attention_qh4_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DECODE_ATTENTION_NO_QH4").is_none()
}

fn decode_attention_splitk_enabled(position: u32) -> bool {
    if std::env::var_os("CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK").is_some()
        || std::env::var_os("CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK256").is_some()
    {
        return false;
    }
    if std::env::var_os("CTOX_QWEN35_DECODE_ATTENTION_SPLITK").is_some()
        || std::env::var_os("CTOX_QWEN35_DECODE_ATTENTION_SPLITK256").is_some()
    {
        return true;
    }
    let min_context = std::env::var("CTOX_QWEN35_DECODE_ATTENTION_SPLITK_MIN_CONTEXT")
        .or_else(|_| std::env::var("CTOX_QWEN35_DECODE_ATTENTION_SPLITK256_MIN_CONTEXT"))
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    position.saturating_add(1) >= min_context
}

fn decode_attention_splitk_block_size() -> usize {
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

fn decode_attention_splitk_partial_kernel(block_size: usize) -> &'static str {
    match block_size {
        128 => "qwen35_08b_attention_norm_rope_cache_qh4_splitk128_partial_gqa8_kv2_d256",
        512 => "qwen35_08b_attention_norm_rope_cache_qh4_splitk512_partial_gqa8_kv2_d256",
        1024 => "qwen35_08b_attention_norm_rope_cache_qh4_splitk1024_partial_gqa8_kv2_d256",
        _ => "qwen35_08b_attention_norm_rope_cache_qh4_splitk256_partial_gqa8_kv2_d256",
    }
}

fn decode_attention_splitk_blocks(max_context: usize) -> usize {
    max_context
        .div_ceil(decode_attention_splitk_block_size())
        .max(1)
}

fn prefill_deltanet_out_matmul_kernel() -> (&'static str, usize) {
    if std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA64").is_some() {
        (
            "qwen35_08b_prefill_deltanet_out_mma64x8_residual_fp16_tiled_k2048_f32",
            64,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA32").is_some() {
        (
            "qwen35_08b_prefill_deltanet_out_mma32x8_fp16_tiled_k2048_f32",
            32,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA16").is_some() {
        (
            "qwen35_08b_prefill_deltanet_out_mma16x8_fp16_tiled_k2048_f32",
            16,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_TOK2").is_some() {
        (
            "qwen35_08b_prefill_deltanet_out_matmul_rowtiles_tok2_fp16_tiled_k2048_f32",
            2,
        )
    } else if std::env::var_os("CTOX_QWEN35_DELTA_OUT_TOK8").is_some() {
        (
            "qwen35_08b_prefill_deltanet_out_matmul_rowtiles_tok8_simd_fp16_tiled_k2048_f32",
            8,
        )
    } else {
        (
            "qwen35_08b_prefill_deltanet_out_matmul_rowtiles_tok4_simd_fp16_tiled_k2048_f32",
            4,
        )
    }
}

fn prefill_deltanet_out_mma_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA16").is_some()
        || std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA32").is_some()
        || std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA64").is_some()
}

fn prefill_deltanet_out_mma32_residual_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL").is_some()
}

fn prefill_delta_scan_gated_norm_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_GATED_NORM").is_some()
        && !prefill_delta_scan_rowcache_direct_enabled()
        && !prefill_delta_scan_rowcache_block64_enabled()
        && !prefill_delta_scan_rowcache_block32_enabled()
        && !prefill_delta_scan_rowcache_block_auto_enabled()
        && !prefill_delta_scan_lanes4_enabled()
        && !prefill_delta_scan_lanes4_sharedqk_enabled()
        && !prefill_delta_scan_lanes4_ordered_enabled()
        && !prefill_mps_qkvz_direct_enabled()
}

fn prefill_delta_scan_rowcache_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_ROWCACHE").is_some()
}

fn prefill_delta_scan_lanes4_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_LANES4").is_some()
}

fn prefill_delta_scan_lanes4_sharedqk_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK").is_some()
}

fn prefill_delta_scan_lanes4_ordered_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_LANES4_ORDERED").is_some()
}

fn prefill_delta_gated_norm_simd32x4_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4").is_some()
}

fn prefill_delta_gated_norm_kernel(qkvz_direct: bool) -> &'static str {
    match (qkvz_direct, prefill_delta_gated_norm_simd32x4_enabled()) {
        (true, true) => {
            "qwen35_08b_prefill_deltanet_gated_rmsnorm_qkvz_simd32x4_tok_h16d128_f32_to_fp16"
        }
        (true, false) => "qwen35_08b_prefill_deltanet_gated_rmsnorm_qkvz_tok_h16d128_f32_to_fp16",
        (false, true) => {
            "qwen35_08b_prefill_deltanet_gated_rmsnorm_simd32x4_tok_h16d128_f32_to_fp16"
        }
        (false, false) => "qwen35_08b_prefill_deltanet_gated_rmsnorm_tok_h16d128_f32_to_fp16",
    }
}

fn prefill_delta_gated_norm_threads() -> (usize, usize, usize) {
    if prefill_delta_gated_norm_simd32x4_enabled() {
        (32, 1, 1)
    } else {
        (QWEN35_08B.deltanet_head_dim, 1, 1)
    }
}

fn prefill_delta_scan_rowcache_direct_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT").is_some()
}

fn prefill_delta_scan_rowcache_block64_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64").is_some()
}

fn prefill_delta_scan_rowcache_block32_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32").is_some()
}

fn prefill_delta_scan_rowcache_block_auto_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO").is_some()
}

fn prefill_delta_scan_rowcache_block_auto_uses_block64(tokens: usize) -> bool {
    let min_tokens = std::env::var("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64_MIN_TOKENS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4096);
    prefill_delta_scan_rowcache_block_auto_enabled() && tokens >= min_tokens
}

fn prefill_delta_scan_chunk_f32x4_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_CHUNK_F32X4").is_some()
}

fn prefill_delta_scan_chunk_hstate_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_CHUNK_HSTATE").is_some()
}

fn prefill_delta_scan_chunk_tokens() -> usize {
    std::env::var("CTOX_QWEN35_DELTA_SCAN_CHUNK_TOKENS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| matches!(*value, 8 | 16 | 32))
        .unwrap_or(32)
}

fn prefill_delta_conv_split_fused_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED").is_some()
        || std::env::var_os("CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED_TOK4").is_some()
}

fn prefill_delta_conv_split_fused_tok4_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED_TOK4").is_some()
}

fn prefill_delta_conv_split_fused_kernel() -> (&'static str, usize) {
    if prefill_delta_conv_split_fused_tok4_enabled() {
        (
            "qwen35_08b_prefill_deltanet_conv_split_qkv_norm_tok4_f32_to_fp16_h16d128",
            4,
        )
    } else {
        (
            "qwen35_08b_prefill_deltanet_conv_split_qkv_norm_tok_f32_to_fp16_h16d128",
            1,
        )
    }
}

fn prefill_delta_scan_rowcache_gated_norm_enabled() -> bool {
    prefill_delta_scan_gated_norm_enabled() && prefill_delta_scan_rowcache_enabled()
}

fn prefill_delta_scan_gated_norm_kernel() -> &'static str {
    if prefill_delta_scan_rowcache_gated_norm_enabled() {
        "qwen35_08b_prefill_deltanet_scan_rowcache_gated_norm_f32_state_tok_h16d128"
    } else {
        "qwen35_08b_prefill_deltanet_scan_gated_norm_f32_state_tok_h16d128"
    }
}

fn prefill_delta_scan_kernel_for_tokens(tokens: usize) -> &'static str {
    if prefill_delta_scan_lanes4_ordered_enabled() {
        "qwen35_08b_prefill_deltanet_scan_lanes4_ordered_f32_state_tok_h16d128"
    } else if prefill_delta_scan_lanes4_sharedqk_enabled() {
        "qwen35_08b_prefill_deltanet_scan_lanes4_sharedqk_f32_state_tok_h16d128"
    } else if prefill_delta_scan_lanes4_enabled() {
        "qwen35_08b_prefill_deltanet_scan_lanes4_f32_state_tok_h16d128"
    } else if prefill_delta_scan_rowcache_block_auto_uses_block64(tokens) {
        "qwen35_08b_prefill_deltanet_scan_rowcache_block64_f32_state_tok_h16d128"
    } else if prefill_delta_scan_rowcache_block_auto_enabled() {
        "qwen35_08b_prefill_deltanet_scan_rowcache_block32_f32_state_tok_h16d128"
    } else if prefill_delta_scan_rowcache_block32_enabled() {
        "qwen35_08b_prefill_deltanet_scan_rowcache_block32_f32_state_tok_h16d128"
    } else if prefill_delta_scan_rowcache_block64_enabled() {
        "qwen35_08b_prefill_deltanet_scan_rowcache_block64_f32_state_tok_h16d128"
    } else if prefill_delta_scan_rowcache_direct_enabled() {
        "qwen35_08b_prefill_deltanet_scan_rowcache_direct_f32_state_tok_h16d128"
    } else if prefill_delta_scan_rowcache_enabled() {
        "qwen35_08b_prefill_deltanet_scan_rowcache_f32_state_tok_h16d128"
    } else {
        "qwen35_08b_prefill_deltanet_scan_f32_state_tok_h16d128"
    }
}

fn dispatch_prefill_delta_scan_shape_for_tokens(
    tokens: usize,
) -> ((usize, usize, usize), (usize, usize, usize)) {
    if prefill_delta_scan_lanes4_enabled()
        || prefill_delta_scan_lanes4_sharedqk_enabled()
        || prefill_delta_scan_lanes4_ordered_enabled()
    {
        (
            (
                QWEN35_08B.deltanet_head_dim.div_ceil(4),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (32, 4, 1),
        )
    } else if prefill_delta_scan_rowcache_block64_enabled()
        || prefill_delta_scan_rowcache_block_auto_uses_block64(tokens)
    {
        (
            (
                QWEN35_08B.deltanet_head_dim.div_ceil(64),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (64, 1, 1),
        )
    } else if prefill_delta_scan_rowcache_block32_enabled()
        || prefill_delta_scan_rowcache_block_auto_enabled()
    {
        (
            (
                QWEN35_08B.deltanet_head_dim.div_ceil(32),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (32, 1, 1),
        )
    } else {
        ((QWEN35_08B.deltanet_v_heads, 1, 1), (128, 1, 1))
    }
}

fn prefill_delta_scan_state_stream_bytes(tokens: usize, heads: usize, head_dim: usize) -> usize {
    let state_bytes = heads * head_dim * head_dim * std::mem::size_of::<f32>();
    if prefill_delta_scan_rowcache_enabled()
        || prefill_delta_scan_rowcache_block32_enabled()
        || prefill_delta_scan_rowcache_block64_enabled()
        || prefill_delta_scan_rowcache_block_auto_enabled()
        || prefill_delta_scan_rowcache_direct_enabled()
        || prefill_delta_scan_lanes4_enabled()
        || prefill_delta_scan_lanes4_sharedqk_enabled()
        || prefill_delta_scan_lanes4_ordered_enabled()
    {
        state_bytes * 2
    } else {
        tokens * state_bytes * 3
    }
}

fn decode_deltanet_step_kernel() -> &'static str {
    if std::env::var_os("CTOX_QWEN35_DECODE_DELTA_ROWCACHE").is_some() {
        "qwen35_08b_deltanet_step_rowcache_f32_state"
    } else {
        "qwen35_08b_deltanet_step_f32_state"
    }
}

fn decode_lm_head_argmax_kernel() -> &'static str {
    if std::env::var_os("CTOX_QWEN35_DECODE_LM_HEAD_SIMD32").is_some() {
        "qwen35_08b_lm_head_argmax_rowtiles_simd32_f32_tiled_k1024"
    } else {
        "qwen35_08b_lm_head_argmax_rowtiles_f32_tiled_k1024"
    }
}

fn decode_async_commands_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DECODE_ASYNC_COMMANDS").is_some()
}

fn prefill_deltanet_out_threadgroup_threads(token_tile: usize) -> usize {
    if token_tile == 16 || token_tile == 32 || token_tile == 64 {
        32
    } else {
        256
    }
}

fn validate_deltanet_out_kernel_tile(
    token_tile: usize,
    tokens: u32,
    row_tile: u32,
) -> Result<(), String> {
    if token_tile == 16 && (!tokens.is_multiple_of(16) || row_tile != 8) {
        return Err("Delta-Out MMA16 requires tokens % 16 == 0 and row_tile == 8".to_owned());
    }
    if token_tile == 32 && (!tokens.is_multiple_of(32) || row_tile != 8) {
        return Err("Delta-Out MMA32 requires tokens % 32 == 0 and row_tile == 8".to_owned());
    }
    if token_tile == 64 && (!tokens.is_multiple_of(64) || row_tile != 8) {
        return Err("Delta-Out MMA64 requires tokens % 64 == 0 and row_tile == 8".to_owned());
    }
    Ok(())
}

fn validate_mma_token_tile(
    label: &str,
    token_tile: usize,
    tokens: usize,
    row_tile: usize,
) -> Result<(), String> {
    if token_tile > 8 && (!tokens.is_multiple_of(token_tile) || row_tile != 8) {
        return Err(format!(
            "{label} requires tokens % {token_tile} == 0 and row_tile == 8"
        ));
    }
    if token_tile == 8 && row_tile != 8 {
        return Err(format!("{label} requires row_tile == 8"));
    }
    Ok(())
}

fn prefill_deltanet_out_residual_kernel() -> (&'static str, usize) {
    (
        "qwen35_08b_prefill_deltanet_out_matmul_residual_rowtiles_tok4_simd_fp16_tiled_k2048",
        4,
    )
}

fn prefill_down_residual_kernel() -> (&'static str, usize) {
    (
        "qwen35_08b_prefill_down_matmul_residual_rowtiles_tok4_simd_fp16_tiled_k3584",
        4,
    )
}

#[derive(Clone, Copy, Debug)]
pub struct MatvecBenchConfig {
    pub rows: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for MatvecBenchConfig {
    fn default() -> Self {
        Self {
            rows: QWEN35_08B.ffn_intermediate,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MatvecBenchResult {
    pub rows: usize,
    pub cols: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MatvecTiledBenchConfig {
    pub rows: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for MatvecTiledBenchConfig {
    fn default() -> Self {
        Self {
            rows: QWEN35_08B.ffn_intermediate,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MatvecTiledBenchResult {
    pub rows: usize,
    pub cols: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RmsMatvecBenchConfig {
    pub rows: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for RmsMatvecBenchConfig {
    fn default() -> Self {
        Self {
            rows: QWEN35_08B.ffn_intermediate,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RmsMatvecBenchResult {
    pub rows: usize,
    pub cols: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RmsMatvecTiledBenchConfig {
    pub rows: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for RmsMatvecTiledBenchConfig {
    fn default() -> Self {
        Self {
            rows: QWEN35_08B.ffn_intermediate,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RmsMatvecTiledBenchResult {
    pub rows: usize,
    pub cols: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillRmsMatmulBenchConfig {
    pub tokens: usize,
    pub rows: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillRmsMatmulBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            rows: QWEN35_08B.ffn_intermediate,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillRmsMatmulBenchResult {
    pub tokens: usize,
    pub rows: usize,
    pub cols: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillFfnGateUpBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillFfnGateUpBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillFfnGateUpBenchResult {
    pub tokens: usize,
    pub hidden: usize,
    pub intermediate: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDownMatmulBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillDownMatmulBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDownMatmulBenchResult {
    pub tokens: usize,
    pub rows: usize,
    pub cols: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillOut2048MatmulBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillOut2048MatmulBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillOut2048MatmulBenchResult {
    pub tokens: usize,
    pub rows: usize,
    pub cols: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillFfnBlockBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub hidden_col_tile: usize,
    pub intermediate_col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillFfnBlockBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            hidden_col_tile: 256,
            intermediate_col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillFfnBlockBenchResult {
    pub tokens: usize,
    pub hidden: usize,
    pub intermediate: usize,
    pub row_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Debug)]
pub struct PrefillDownMmaCompareResult {
    pub tokens: usize,
    pub hidden: usize,
    pub intermediate: usize,
    pub baseline_median_s: f64,
    pub baseline_p95_s: f64,
    pub mma_median_s: f64,
    pub mma_p95_s: f64,
    pub baseline_checksum: f32,
    pub mma_checksum: f32,
    pub max_abs_error: f32,
    pub mean_abs_error: f32,
    pub max_abs_index: usize,
}

#[derive(Clone, Debug)]
pub struct PrefillGateUpMmaCompareResult {
    pub tokens: usize,
    pub hidden: usize,
    pub intermediate: usize,
    pub mma_token_tile: usize,
    pub baseline_median_s: f64,
    pub baseline_p95_s: f64,
    pub mma_median_s: f64,
    pub mma_p95_s: f64,
    pub baseline_checksum: f32,
    pub mma_checksum: f32,
    pub max_abs_error: f32,
    pub mean_abs_error: f32,
    pub max_abs_index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDeltaProjectBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillDeltaProjectBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDeltaProjectBenchResult {
    pub tokens: usize,
    pub hidden: usize,
    pub qkv_rows: usize,
    pub z_rows: usize,
    pub gate_rows: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillAttentionProjectBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillAttentionProjectBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillAttentionProjectBenchResult {
    pub tokens: usize,
    pub hidden: usize,
    pub q_rows: usize,
    pub kv_rows: usize,
    pub row_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillAttentionCoreBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub hidden_col_tile: usize,
    pub attention_col_tile: usize,
    pub use_project_mma: bool,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillAttentionCoreBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            hidden_col_tile: 256,
            attention_col_tile: 256,
            use_project_mma: true,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillAttentionCoreBenchResult {
    pub tokens: usize,
    pub hidden: usize,
    pub q_rows: usize,
    pub kv_rows: usize,
    pub attention_width: usize,
    pub row_tile: usize,
    pub project_token_tile: usize,
    pub out_token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrefillAttentionCoreProfileStop {
    Full,
    Norm,
    Project,
    Prepare,
    Attention,
}

impl PrefillAttentionCoreProfileStop {
    fn from_env() -> Self {
        match std::env::var("CTOX_QWEN35_ATTENTION_CORE_PROFILE_STOP")
            .ok()
            .as_deref()
        {
            Some("norm") => Self::Norm,
            Some("project") | Some("qkv") => Self::Project,
            Some("prepare") | Some("rope") => Self::Prepare,
            Some("attention") | Some("attn") => Self::Attention,
            _ => Self::Full,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDeltaConvBenchConfig {
    pub tokens: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillDeltaConvBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDeltaConvBenchResult {
    pub tokens: usize,
    pub channels: usize,
    pub kernel_width: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDeltaPrepareBenchConfig {
    pub tokens: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillDeltaPrepareBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDeltaPrepareBenchResult {
    pub tokens: usize,
    pub heads: usize,
    pub head_dim: usize,
    pub qkv_width: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDeltaScanBenchConfig {
    pub tokens: usize,
    pub warmup: usize,
    pub iterations: usize,
    pub validate_tokens: usize,
}

impl Default for PrefillDeltaScanBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            warmup: 3,
            iterations: 20,
            validate_tokens: 8,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDeltaScanBenchResult {
    pub kernel_name: &'static str,
    pub grid: (usize, usize, usize),
    pub threads: (usize, usize, usize),
    pub tokens: usize,
    pub heads: usize,
    pub head_dim: usize,
    pub state_bytes: usize,
    pub bytes_moved_estimate: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub max_abs_error_out: f32,
    pub max_abs_error_state: f32,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDeltaOutBlockBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillDeltaOutBlockBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDeltaOutBlockBenchResult {
    pub tokens: usize,
    pub rows: usize,
    pub cols: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
    pub checksum_sparse: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDeltaBlockBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub hidden_col_tile: usize,
    pub out_col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillDeltaBlockBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            hidden_col_tile: 256,
            out_col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDeltaBlockBenchResult {
    pub tokens: usize,
    pub hidden: usize,
    pub delta_width: usize,
    pub qkv_rows: usize,
    pub row_tile: usize,
    pub token_tile: usize,
    pub out_token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefillDeltaFfnBlockBenchConfig {
    pub tokens: usize,
    pub row_tile: usize,
    pub hidden_col_tile: usize,
    pub delta_out_col_tile: usize,
    pub intermediate_col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PrefillDeltaFfnBlockBenchConfig {
    fn default() -> Self {
        Self {
            tokens: 512,
            row_tile: 8,
            hidden_col_tile: 256,
            delta_out_col_tile: 256,
            intermediate_col_tile: 256,
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefillDeltaFfnBlockBenchResult {
    pub tokens: usize,
    pub hidden: usize,
    pub delta_width: usize,
    pub intermediate: usize,
    pub qkv_rows: usize,
    pub row_tile: usize,
    pub project_token_tile: usize,
    pub qkvz_token_tile: usize,
    pub out_token_tile: usize,
    pub ffn_token_tile: usize,
    pub down_token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

pub struct PrefillDeltaFfnLayerWeights<'a> {
    pub input_norm: &'a [u16],
    pub qkv: &'a [u16],
    pub z: &'a [u16],
    pub b: &'a [u16],
    pub a: &'a [u16],
    pub conv_weight: &'a [u16],
    pub conv_bias: &'a [u16],
    pub a_log: &'a [f32],
    pub dt_bias: &'a [f32],
    pub delta_norm: &'a [f32],
    pub delta_out: &'a [u16],
    pub ffn_norm: &'a [u16],
    pub ffn_gate: &'a [u16],
    pub ffn_up: &'a [u16],
    pub ffn_down: &'a [u16],
}

pub struct PrefillMpsFfnLayerWeights<'a> {
    pub gate_up: &'a [u8],
    pub down: &'a [u8],
}

pub struct PrefillMpsDeltaProjectLayerWeights<'a> {
    pub qkvz: &'a [u8],
}

pub struct PrefillMpsDeltaOutLayerWeights<'a> {
    pub out: &'a [u8],
}

#[derive(Clone, Debug)]
pub struct PrefillDelta3FfnSuperblockBenchResult {
    pub tokens: usize,
    pub layers: usize,
    pub hidden: usize,
    pub delta_width: usize,
    pub intermediate: usize,
    pub qkv_rows: usize,
    pub row_tile: usize,
    pub project_token_tile: usize,
    pub qkvz_token_tile: usize,
    pub out_token_tile: usize,
    pub ffn_token_tile: usize,
    pub down_token_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrefillDeltaFfnProfileStop {
    Full,
    Project,
    ConvSplit,
    ScanNorm,
    DeltaOut,
    FfnGateUp,
}

impl PrefillDeltaFfnProfileStop {
    fn from_env() -> Self {
        match std::env::var("CTOX_QWEN35_DELTA_STACK_PROFILE_STOP")
            .ok()
            .as_deref()
        {
            Some("project") => Self::Project,
            Some("conv_split") | Some("conv") => Self::ConvSplit,
            Some("scan_norm") | Some("scan") => Self::ScanNorm,
            Some("delta_out") | Some("delta") => Self::DeltaOut,
            Some("ffn_gate_up") | Some("gate_up") | Some("ffn") => Self::FfnGateUp,
            _ => Self::Full,
        }
    }
}

struct PrefillDeltaFfnLayerDeviceBuffers {
    input_norm: Buffer,
    qkv: Buffer,
    z: Buffer,
    b: Buffer,
    a: Buffer,
    conv_weight: Buffer,
    conv_bias: Buffer,
    a_log: Buffer,
    dt_bias: Buffer,
    delta_norm: Buffer,
    delta_out: Buffer,
    ffn_norm: Buffer,
    ffn_gate: Buffer,
    ffn_up: Buffer,
    ffn_down: Buffer,
    conv_state: Buffer,
    recurrent_state: Buffer,
}

#[derive(Clone, Copy, Debug)]
pub struct DeltaNetStepBenchConfig {
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for DeltaNetStepBenchConfig {
    fn default() -> Self {
        Self {
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DeltaNetStepBenchResult {
    pub heads: usize,
    pub head_dim: usize,
    pub state_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub max_abs_error_out: f32,
    pub max_abs_error_state: f32,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct DeltaNetDecayActivationBenchConfig {
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for DeltaNetDecayActivationBenchConfig {
    fn default() -> Self {
        Self {
            warmup: 3,
            iterations: 20,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DeltaNetDecayActivationBenchResult {
    pub heads: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub max_abs_error_beta: f32,
    pub max_abs_error_decay: f32,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FfnSwiGluBenchConfig {
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for FfnSwiGluBenchConfig {
    fn default() -> Self {
        Self {
            warmup: 1,
            iterations: 5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfnSwiGluBenchResult {
    pub hidden: usize,
    pub intermediate: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub checksum: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LmHeadArgmaxBenchConfig {
    pub vocab_rows: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for LmHeadArgmaxBenchConfig {
    fn default() -> Self {
        Self {
            vocab_rows: QWEN35_08B.vocab_size,
            warmup: 2,
            iterations: 10,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LmHeadArgmaxBenchResult {
    pub vocab_rows: usize,
    pub cols: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub next_token: u32,
    pub score: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LmHeadArgmaxTiledBenchConfig {
    pub vocab_rows: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for LmHeadArgmaxTiledBenchConfig {
    fn default() -> Self {
        Self {
            vocab_rows: QWEN35_08B.vocab_size,
            row_tile: 8,
            col_tile: 256,
            warmup: 2,
            iterations: 10,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LmHeadArgmaxTiledBenchResult {
    pub vocab_rows: usize,
    pub cols: usize,
    pub row_tile: usize,
    pub col_tile: usize,
    pub packed_weight_bytes: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub next_token: u32,
    pub score: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct DecodeSkeletonBenchConfig {
    pub vocab_rows: usize,
    pub input_token: u32,
    pub decode_position: u32,
    pub max_context: usize,
    pub warmup: usize,
    pub iterations: usize,
    pub debug_top_k: usize,
}

impl Default for DecodeSkeletonBenchConfig {
    fn default() -> Self {
        Self {
            vocab_rows: QWEN35_08B.vocab_size,
            input_token: 107,
            decode_position: 0,
            max_context: 1,
            warmup: 2,
            iterations: 10,
            debug_top_k: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DecodeSkeletonBenchResult {
    pub vocab_rows: usize,
    pub cols: usize,
    pub input_token: u32,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub next_token: u32,
    pub score: f32,
    pub top_logits: Vec<(u32, f32)>,
}

#[derive(Clone, Debug)]
pub struct DecodeSequenceBenchResult {
    pub vocab_rows: usize,
    pub cols: usize,
    pub input_token: u32,
    pub steps: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub effective_gb_s: f64,
    pub tokens: Vec<u32>,
    pub last_score: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct DeltaLayerTiled<'a> {
    pub input_norm: &'a [u16],
    pub qkv: &'a [u16],
    pub z: &'a [u16],
    pub b: &'a [u16],
    pub a: &'a [u16],
    pub a_log: &'a [f32],
    pub dt_bias: &'a [f32],
    pub gated_norm: &'a [f32],
    pub conv_weight: &'a [u16],
    pub conv_bias: &'a [u16],
    pub out: &'a [u16],
}

#[derive(Clone, Copy, Debug)]
pub struct AttentionLayerTiled<'a> {
    pub input_norm: &'a [u16],
    pub q_norm: &'a [u16],
    pub k_norm: &'a [u16],
    pub q: &'a [u16],
    pub k: &'a [u16],
    pub v: &'a [u16],
    pub o: &'a [u16],
}

#[derive(Clone, Copy, Debug)]
pub struct FfnLayerTiled<'a> {
    pub post_norm: &'a [u16],
    pub gate: &'a [u16],
    pub up: &'a [u16],
    pub down: &'a [u16],
}

struct DeltaLayerBuffers {
    input_norm: crate::metal::ffi::Buffer,
    qkv: crate::metal::ffi::Buffer,
    z: crate::metal::ffi::Buffer,
    b: crate::metal::ffi::Buffer,
    a: crate::metal::ffi::Buffer,
    a_log: crate::metal::ffi::Buffer,
    dt_bias: crate::metal::ffi::Buffer,
    gated_norm: crate::metal::ffi::Buffer,
    conv_weight: crate::metal::ffi::Buffer,
    conv_bias: crate::metal::ffi::Buffer,
    out: crate::metal::ffi::Buffer,
}

struct AttentionLayerBuffers {
    input_norm: crate::metal::ffi::Buffer,
    q_norm: crate::metal::ffi::Buffer,
    k_norm: crate::metal::ffi::Buffer,
    q: crate::metal::ffi::Buffer,
    k: crate::metal::ffi::Buffer,
    v: crate::metal::ffi::Buffer,
    o: crate::metal::ffi::Buffer,
}

struct FfnLayerBuffers {
    post_norm: crate::metal::ffi::Buffer,
    gate: crate::metal::ffi::Buffer,
    up: crate::metal::ffi::Buffer,
    down: crate::metal::ffi::Buffer,
}

#[derive(Clone, Copy, Debug)]
pub struct SyntheticMegaBenchConfig {
    pub vocab_rows: usize,
    pub input_token: u32,
    pub layers: usize,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for SyntheticMegaBenchConfig {
    fn default() -> Self {
        Self {
            vocab_rows: 8192,
            input_token: 107,
            layers: QWEN35_08B.n_layers,
            warmup: 1,
            iterations: 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SyntheticMegaBenchResult {
    pub vocab_rows: usize,
    pub cols: usize,
    pub input_token: u32,
    pub layers: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub estimated_gb_s: f64,
    pub next_token: u32,
    pub score: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PatternMegaBenchConfig {
    pub vocab_rows: usize,
    pub input_token: u32,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PatternMegaBenchConfig {
    fn default() -> Self {
        Self {
            vocab_rows: 8192,
            input_token: 107,
            warmup: 1,
            iterations: 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PatternMegaBenchResult {
    pub vocab_rows: usize,
    pub cols: usize,
    pub input_token: u32,
    pub layers: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub estimated_gb_s: f64,
    pub next_token: u32,
    pub score: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PatternFfnMegaBenchConfig {
    pub vocab_rows: usize,
    pub input_token: u32,
    pub warmup: usize,
    pub iterations: usize,
}

impl Default for PatternFfnMegaBenchConfig {
    fn default() -> Self {
        Self {
            vocab_rows: 4096,
            input_token: 107,
            warmup: 0,
            iterations: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PatternFfnMegaBenchResult {
    pub vocab_rows: usize,
    pub cols: usize,
    pub intermediate: usize,
    pub input_token: u32,
    pub layers: usize,
    pub iterations: usize,
    pub median_s: f64,
    pub p95_s: f64,
    pub estimated_gb_s: f64,
    pub next_token: u32,
    pub score: f32,
}

pub fn run_matvec_bench(config: MatvecBenchConfig) -> Result<MatvecBenchResult, String> {
    let rows = config.rows;
    let cols = QWEN35_08B.hidden_size;
    if rows == 0 {
        return Err("matvec rows must be > 0".to_string());
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(rows * std::mem::size_of::<f32>())?;

    let x_host: Vec<u16> = (0..cols)
        .map(|i| f16::from_f32(((i % 97) as f32 - 48.0) / 97.0).to_bits())
        .collect();
    let w_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(13) % 251) as f32 - 125.0) / 251.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        x.write(0, &x_host);
        w.write(0, &w_host);
    }

    for _ in 0..config.warmup {
        dispatch_matvec_once(&dev, &x, &w, &y, rows_u32)?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_matvec_once(&dev, &x, &w, &y, rows_u32)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);

    let bytes_moved = rows * cols * 2 + cols * 2 + rows * std::mem::size_of::<f32>();
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(MatvecBenchResult {
        rows,
        cols,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

fn dispatch_matvec_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    rows: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_matvec_fp16_k1024_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w, 0);
    enc.set_buffer(2, y, 0);
    enc.set_bytes(3, &rows);
    enc.dispatch_threadgroups((rows as usize, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

pub fn run_matvec_tiled_bench(
    config: MatvecTiledBenchConfig,
) -> Result<MatvecTiledBenchResult, String> {
    let rows = config.rows;
    let cols = QWEN35_08B.hidden_size;
    let x_host = synthetic_matvec_x(cols);
    let w_row_major = synthetic_matvec_w(rows, cols);
    let w_tiled =
        pack_row_major_fp16_to_tiles(&w_row_major, rows, cols, config.row_tile, config.col_tile);

    run_matvec_tiled_with_weights(config, &x_host, &w_tiled)
}

pub fn run_matvec_tiled_with_weights(
    config: MatvecTiledBenchConfig,
    x_host: &[u16],
    w_tiled: &[u16],
) -> Result<MatvecTiledBenchResult, String> {
    let rows = config.rows;
    let cols = QWEN35_08B.hidden_size;
    validate_tiled_matvec_shape(rows, cols, config.row_tile, config.col_tile)?;
    if x_host.len() != cols {
        return Err(format!(
            "x_host length must be {cols}, got {}",
            x_host.len()
        ));
    }
    let expected_weights =
        round_up_usize(rows, config.row_tile) * round_up_usize(cols, config.col_tile);
    if w_tiled.len() != expected_weights {
        return Err(format!(
            "w_tiled length must be {expected_weights}, got {}",
            w_tiled.len()
        ));
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(config.col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(w_tiled.len() * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, &x_host);
        w.write(0, &w_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_matvec_tiled_once(
            &dev,
            &x,
            &w,
            &y,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_matvec_tiled_once(
            &dev,
            &x,
            &w,
            &y,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let packed_weight_bytes = w_tiled.len() * std::mem::size_of::<u16>();
    let bytes_moved = packed_weight_bytes + cols * 2 + rows * std::mem::size_of::<f32>();
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(MatvecTiledBenchResult {
        rows,
        cols,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
fn dispatch_matvec_tiled_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w, 0);
    enc.set_buffer(2, y, 0);
    enc.set_bytes(3, &rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &n_col_tiles);
    enc.dispatch_threadgroups((rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

pub fn run_rms_matvec_bench(config: RmsMatvecBenchConfig) -> Result<RmsMatvecBenchResult, String> {
    let rows = config.rows;
    let cols = QWEN35_08B.hidden_size;
    if rows == 0 {
        return Err("rms_matvec rows must be > 0".to_string());
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let norm_weight = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(rows * std::mem::size_of::<f32>())?;

    let x_host: Vec<u16> = (0..cols)
        .map(|i| f16::from_f32(((i % 97) as f32 - 48.0) / 97.0).to_bits())
        .collect();
    let norm_host: Vec<u16> = (0..cols)
        .map(|i| f16::from_f32(1.0 + ((i % 17) as f32 - 8.0) / 256.0).to_bits())
        .collect();
    let w_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(13) % 251) as f32 - 125.0) / 251.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        x.write(0, &x_host);
        norm_weight.write(0, &norm_host);
        w.write(0, &w_host);
    }

    for _ in 0..config.warmup {
        dispatch_rms_matvec_once(&dev, &x, &norm_weight, &w, &y, rows_u32)?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_rms_matvec_once(&dev, &x, &norm_weight, &w, &y, rows_u32)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);

    let bytes_moved =
        rows * cols * 2 + rows * cols * 2 + rows * cols * 2 + rows * std::mem::size_of::<f32>();
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(RmsMatvecBenchResult {
        rows,
        cols,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_rms_matvec_tiled_bench(
    config: RmsMatvecTiledBenchConfig,
) -> Result<RmsMatvecTiledBenchResult, String> {
    let rows = config.rows;
    let cols = QWEN35_08B.hidden_size;
    validate_tiled_matvec_shape(rows, cols, config.row_tile, config.col_tile)?;
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(config.col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let norm_weight = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let w_tiled_len = round_up_usize(rows, config.row_tile) * round_up_usize(cols, config.col_tile);
    let w = dev.new_buffer(w_tiled_len * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(rows * std::mem::size_of::<f32>())?;

    let x_host = synthetic_matvec_x(cols);
    let norm_host: Vec<u16> = (0..cols)
        .map(|i| f16::from_f32(1.0 + ((i % 17) as f32 - 8.0) / 256.0).to_bits())
        .collect();
    let w_row_major = synthetic_matvec_w(rows, cols);
    let w_tiled =
        pack_row_major_fp16_to_tiles(&w_row_major, rows, cols, config.row_tile, config.col_tile);
    unsafe {
        x.write(0, &x_host);
        norm_weight.write(0, &norm_host);
        w.write(0, &w_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_rms_matvec_tiled_once(
            &dev,
            &x,
            &norm_weight,
            &w,
            &y,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_rms_matvec_tiled_once(
            &dev,
            &x,
            &norm_weight,
            &w,
            &y,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let packed_weight_bytes = w_tiled.len() * std::mem::size_of::<u16>();
    let bytes_moved = packed_weight_bytes + cols * 2 + cols * 2 + rows * std::mem::size_of::<f32>();
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(RmsMatvecTiledBenchResult {
        rows,
        cols,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_rms_matmul_bench(
    config: PrefillRmsMatmulBenchConfig,
) -> Result<PrefillRmsMatmulBenchResult, String> {
    let cols = QWEN35_08B.hidden_size;
    let mut x_host = Vec::with_capacity(config.tokens * cols);
    for token in 0..config.tokens {
        for col in 0..cols {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }
    let norm_host: Vec<u16> = (0..cols)
        .map(|i| f16::from_f32(1.0 + ((i % 17) as f32 - 8.0) / 256.0).to_bits())
        .collect();
    let w_row_major = synthetic_matvec_w(config.rows, cols);
    let w_tiled = pack_row_major_fp16_to_tiles(
        &w_row_major,
        config.rows,
        cols,
        config.row_tile,
        config.col_tile,
    );
    run_prefill_rms_matmul_with_weights(config, &x_host, &norm_host, &w_tiled)
}

pub fn run_prefill_rms_matmul_with_weights(
    config: PrefillRmsMatmulBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    w_tiled: &[u16],
) -> Result<PrefillRmsMatmulBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let rows = config.rows;
    let cols = QWEN35_08B.hidden_size;
    validate_tiled_matvec_shape(rows, cols, config.row_tile, config.col_tile)?;
    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(config.col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    if x_host.len() != config.tokens * cols {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * cols,
            x_host.len()
        ));
    }
    if norm_host.len() != cols {
        return Err(format!(
            "norm_host length must be {cols}, got {}",
            norm_host.len()
        ));
    }
    let expected_weights =
        round_up_usize(rows, config.row_tile) * round_up_usize(cols, config.col_tile);
    if w_tiled.len() != expected_weights {
        return Err(format!(
            "w_tiled length must be {expected_weights}, got {}",
            w_tiled.len()
        ));
    }
    let x = dev.new_buffer(config.tokens * cols * std::mem::size_of::<u16>())?;
    let norm_weight = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(w_tiled.len() * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(config.tokens * rows * std::mem::size_of::<f32>())?;

    unsafe {
        x.write(0, x_host);
        norm_weight.write(0, norm_host);
        w.write(0, w_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_rms_matmul_once(
            &dev,
            &x,
            &norm_weight,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_rms_matmul_once(
            &dev,
            &x,
            &norm_weight,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let packed_weight_bytes = w_tiled.len() * std::mem::size_of::<u16>();
    let (_, token_tile) = prefill_rms_matmul_kernel();
    let token_groups = config.tokens.div_ceil(token_tile);
    let bytes_moved = token_groups * packed_weight_bytes
        + config.tokens * (cols * 2 + cols * 2 + rows * std::mem::size_of::<f32>());
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillRmsMatmulBenchResult {
        tokens: config.tokens,
        rows,
        cols,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_ffn_gate_up_bench(
    config: PrefillFfnGateUpBenchConfig,
) -> Result<PrefillFfnGateUpBenchResult, String> {
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    let mut x_host = Vec::with_capacity(config.tokens * hidden);
    for token in 0..config.tokens {
        for col in 0..hidden {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }
    let norm_host: Vec<u16> = (0..hidden)
        .map(|i| f16::from_f32(1.0 + ((i % 17) as f32 - 8.0) / 256.0).to_bits())
        .collect();
    let gate_row_major = synthetic_matvec_w(intermediate, hidden);
    let up_row_major: Vec<u16> = (0..intermediate * hidden)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 257.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let gate_tiled = pack_row_major_fp16_to_tiles(
        &gate_row_major,
        intermediate,
        hidden,
        config.row_tile,
        config.col_tile,
    );
    let up_tiled = pack_row_major_fp16_to_tiles(
        &up_row_major,
        intermediate,
        hidden,
        config.row_tile,
        config.col_tile,
    );
    run_prefill_ffn_gate_up_with_weights(config, &x_host, &norm_host, &gate_tiled, &up_tiled)
}

pub fn run_prefill_ffn_gate_up_with_weights(
    config: PrefillFfnGateUpBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    gate_tiled: &[u16],
    up_tiled: &[u16],
) -> Result<PrefillFfnGateUpBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(intermediate, hidden, config.row_tile, config.col_tile)?;
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if norm_host.len() != hidden {
        return Err(format!(
            "norm_host length must be {hidden}, got {}",
            norm_host.len()
        ));
    }
    let expected_weights =
        round_up_usize(intermediate, config.row_tile) * round_up_usize(hidden, config.col_tile);
    if gate_tiled.len() != expected_weights {
        return Err(format!(
            "gate_tiled length must be {expected_weights}, got {}",
            gate_tiled.len()
        ));
    }
    if up_tiled.len() != expected_weights {
        return Err(format!(
            "up_tiled length must be {expected_weights}, got {}",
            up_tiled.len()
        ));
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let rows_u32 = u32::try_from(intermediate).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(config.col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let gate = dev.new_buffer(gate_tiled.len() * std::mem::size_of::<u16>())?;
    let up = dev.new_buffer(up_tiled.len() * std::mem::size_of::<u16>())?;
    let out = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    unsafe {
        x.write(0, x_host);
        norm.write(0, norm_host);
        gate.write(0, gate_tiled);
        up.write(0, up_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_ffn_gate_up_once(
            &dev,
            &x,
            &norm,
            &gate,
            &up,
            &out,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_ffn_gate_up_once(
            &dev,
            &x,
            &norm,
            &gate,
            &up,
            &out,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0u16; intermediate.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    let checksum = first
        .iter()
        .map(|v| f16::from_bits(*v).to_f32())
        .fold(0.0f32, |acc, v| acc + v);
    let (_, token_tile) = prefill_ffn_gate_up_kernel();
    let token_groups = config.tokens.div_ceil(token_tile);
    let packed_weight_bytes = (gate_tiled.len() + up_tiled.len()) * std::mem::size_of::<u16>();
    let bytes_moved = token_groups * packed_weight_bytes
        + config.tokens * (hidden * 2 + hidden * 2 + intermediate * std::mem::size_of::<u16>());
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillFfnGateUpBenchResult {
        tokens: config.tokens,
        hidden,
        intermediate,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_down_matmul_with_weights(
    config: PrefillDownMatmulBenchConfig,
    x_host: &[u16],
    w_tiled: &[u16],
) -> Result<PrefillDownMatmulBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let rows = QWEN35_08B.hidden_size;
    let cols = QWEN35_08B.ffn_intermediate;
    if std::env::var_os("CTOX_QWEN35_DOWN_MMA").is_some()
        && (!config.tokens.is_multiple_of(8) || rows % 8 != 0 || config.row_tile != 8)
    {
        return Err(
            "DOWN_MMA requires tokens multiple of 8, rows multiple of 8, row_tile=8".to_string(),
        );
    }
    validate_tiled_matvec_shape(rows, cols, config.row_tile, config.col_tile)?;
    if x_host.len() != config.tokens * cols {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * cols,
            x_host.len()
        ));
    }
    let expected_weights =
        round_up_usize(rows, config.row_tile) * round_up_usize(cols, config.col_tile);
    if w_tiled.len() != expected_weights {
        return Err(format!(
            "w_tiled length must be {expected_weights}, got {}",
            w_tiled.len()
        ));
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(config.col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(w_tiled.len() * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(config.tokens * rows * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, x_host);
        w.write(0, w_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_down_matmul_once(
            &dev,
            &x,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_down_matmul_once(
            &dev,
            &x,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let (_, token_tile) = prefill_down_matmul_kernel();
    let token_groups = config.tokens.div_ceil(token_tile);
    let packed_weight_bytes = w_tiled.len() * std::mem::size_of::<u16>();
    let bytes_moved = token_groups * packed_weight_bytes
        + config.tokens * (cols * std::mem::size_of::<u16>() + rows * std::mem::size_of::<f32>());
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDownMatmulBenchResult {
        tokens: config.tokens,
        rows,
        cols,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_prefill_down_mma_compare_with_weights(
    config: PrefillFfnBlockBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    gate_tiled: &[u16],
    up_tiled: &[u16],
    down_tiled: &[u16],
) -> Result<PrefillDownMmaCompareResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    if !config.tokens.is_multiple_of(8) {
        return Err("MMA compare requires token count multiple of 8".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(
        intermediate,
        hidden,
        config.row_tile,
        config.hidden_col_tile,
    )?;
    validate_tiled_matvec_shape(
        hidden,
        intermediate,
        config.row_tile,
        config.intermediate_col_tile,
    )?;
    if (prefill_ffn_gate_up_mma_enabled() || prefill_down_mma_enabled())
        && (!config.tokens.is_multiple_of(8) || config.row_tile != 8)
    {
        return Err("FFN MMA paths require token count multiple of 8 and row_tile=8".to_string());
    }
    if config.row_tile != 8 {
        return Err("MMA compare requires row_tile=8".to_string());
    }
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if norm_host.len() != hidden {
        return Err(format!(
            "norm_host length must be {hidden}, got {}",
            norm_host.len()
        ));
    }
    let gate_weights = round_up_usize(intermediate, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let down_weights = round_up_usize(hidden, config.row_tile)
        * round_up_usize(intermediate, config.intermediate_col_tile);
    if gate_tiled.len() != gate_weights
        || up_tiled.len() != gate_weights
        || down_tiled.len() != down_weights
    {
        return Err("one or more FFN weight buffers have invalid packed length".to_string());
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let hidden_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let intermediate_u32 = u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let intermediate_col_tile_u32 = u32::try_from(config.intermediate_col_tile)
        .map_err(|_| "intermediate_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;
    let intermediate_col_tiles_u32 =
        u32::try_from(intermediate.div_ceil(config.intermediate_col_tile))
            .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let gate = dev.new_buffer(gate_tiled.len() * std::mem::size_of::<u16>())?;
    let up = dev.new_buffer(up_tiled.len() * std::mem::size_of::<u16>())?;
    let down = dev.new_buffer(down_tiled.len() * std::mem::size_of::<u16>())?;
    let act = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    let baseline = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let mma = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, x_host);
        norm.write(0, norm_host);
        gate.write(0, gate_tiled);
        up.write(0, up_tiled);
        down.write(0, down_tiled);
    }

    dispatch_prefill_ffn_gate_up_once(
        &dev,
        &x,
        &norm,
        &gate,
        &up,
        &act,
        tokens_u32,
        intermediate_u32,
        row_tile_u32,
        hidden_col_tile_u32,
        hidden_col_tiles_u32,
    )?;

    let baseline_kernel = "qwen35_08b_prefill_down_matmul_rowtiles_tok4_simd_fp16_tiled_k3584_f32";
    let mma_kernel = "qwen35_08b_prefill_down_mma8x8_fp16_tiled_k3584_f32";
    for _ in 0..config.warmup {
        dispatch_prefill_down_matmul_named_once(
            &dev,
            baseline_kernel,
            4,
            256,
            &act,
            &down,
            &baseline,
            tokens_u32,
            hidden_u32,
            row_tile_u32,
            intermediate_col_tile_u32,
            intermediate_col_tiles_u32,
        )?;
        dispatch_prefill_down_matmul_named_once(
            &dev,
            mma_kernel,
            8,
            32,
            &act,
            &down,
            &mma,
            tokens_u32,
            hidden_u32,
            row_tile_u32,
            intermediate_col_tile_u32,
            intermediate_col_tiles_u32,
        )?;
    }

    let mut baseline_samples = Vec::with_capacity(config.iterations);
    let mut mma_samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_down_matmul_named_once(
            &dev,
            baseline_kernel,
            4,
            256,
            &act,
            &down,
            &baseline,
            tokens_u32,
            hidden_u32,
            row_tile_u32,
            intermediate_col_tile_u32,
            intermediate_col_tiles_u32,
        )?;
        baseline_samples.push(start.elapsed().as_secs_f64());

        let start = Instant::now();
        dispatch_prefill_down_matmul_named_once(
            &dev,
            mma_kernel,
            8,
            32,
            &act,
            &down,
            &mma,
            tokens_u32,
            hidden_u32,
            row_tile_u32,
            intermediate_col_tile_u32,
            intermediate_col_tiles_u32,
        )?;
        mma_samples.push(start.elapsed().as_secs_f64());
    }
    baseline_samples.sort_by(|a, b| a.total_cmp(b));
    mma_samples.sort_by(|a, b| a.total_cmp(b));

    let mut baseline_host = vec![0.0f32; config.tokens * hidden];
    let mut mma_host = vec![0.0f32; config.tokens * hidden];
    unsafe {
        baseline.read(0, &mut baseline_host);
        mma.read(0, &mut mma_host);
    }
    let baseline_checksum = baseline_host
        .iter()
        .take(hidden.min(16))
        .copied()
        .sum::<f32>();
    let mma_checksum = mma_host.iter().take(hidden.min(16)).copied().sum::<f32>();
    let mut max_abs_error = 0.0f32;
    let mut max_abs_index = 0usize;
    let mut abs_sum = 0.0f32;
    for (idx, (a, b)) in mma_host.iter().zip(baseline_host.iter()).enumerate() {
        let err = (a - b).abs();
        abs_sum += err;
        if err > max_abs_error {
            max_abs_error = err;
            max_abs_index = idx;
        }
    }
    let mean_abs_error = abs_sum / baseline_host.len().max(1) as f32;

    Ok(PrefillDownMmaCompareResult {
        tokens: config.tokens,
        hidden,
        intermediate,
        baseline_median_s: percentile_sorted(&baseline_samples, 0.50),
        baseline_p95_s: percentile_sorted(&baseline_samples, 0.95),
        mma_median_s: percentile_sorted(&mma_samples, 0.50),
        mma_p95_s: percentile_sorted(&mma_samples, 0.95),
        baseline_checksum,
        mma_checksum,
        max_abs_error,
        mean_abs_error,
        max_abs_index,
    })
}

pub fn run_prefill_out2048_matmul_with_weights(
    config: PrefillOut2048MatmulBenchConfig,
    x_host: &[u16],
    w_tiled: &[u16],
) -> Result<PrefillOut2048MatmulBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let rows = QWEN35_08B.hidden_size;
    let cols = QWEN35_08B.attention_q_width();
    validate_tiled_matvec_shape(rows, cols, config.row_tile, config.col_tile)?;
    if x_host.len() != config.tokens * cols {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * cols,
            x_host.len()
        ));
    }
    let expected_weights =
        round_up_usize(rows, config.row_tile) * round_up_usize(cols, config.col_tile);
    if w_tiled.len() != expected_weights {
        return Err(format!(
            "w_tiled length must be {expected_weights}, got {}",
            w_tiled.len()
        ));
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(config.col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(w_tiled.len() * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(config.tokens * rows * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, x_host);
        w.write(0, w_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_out2048_matmul_once(
            &dev,
            &x,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_out2048_matmul_once(
            &dev,
            &x,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let (_, token_tile) = prefill_deltanet_out_matmul_kernel();
    let token_groups = config.tokens.div_ceil(token_tile);
    let packed_weight_bytes = w_tiled.len() * std::mem::size_of::<u16>();
    let bytes_moved = token_groups * packed_weight_bytes
        + config.tokens * (cols * std::mem::size_of::<u16>() + rows * std::mem::size_of::<f32>());
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillOut2048MatmulBenchResult {
        tokens: config.tokens,
        rows,
        cols,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_prefill_ffn_block_with_weights(
    config: PrefillFfnBlockBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    gate_tiled: &[u16],
    up_tiled: &[u16],
    down_tiled: &[u16],
) -> Result<PrefillFfnBlockBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(
        intermediate,
        hidden,
        config.row_tile,
        config.hidden_col_tile,
    )?;
    validate_tiled_matvec_shape(
        hidden,
        intermediate,
        config.row_tile,
        config.intermediate_col_tile,
    )?;
    if prefill_ffn_gate_up_mma_enabled() {
        let (_, gate_up_token_tile) = prefill_ffn_gate_up_mma_kernel();
        validate_mma_token_tile(
            "FFN GateUp MMA",
            gate_up_token_tile,
            config.tokens,
            config.row_tile,
        )?;
    }
    if prefill_down_mma_enabled() {
        let (_, down_token_tile) = prefill_down_matmul_kernel();
        validate_mma_token_tile(
            "FFN Down MMA",
            down_token_tile,
            config.tokens,
            config.row_tile,
        )?;
    }
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if norm_host.len() != hidden {
        return Err(format!(
            "norm_host length must be {hidden}, got {}",
            norm_host.len()
        ));
    }
    let gate_weights = round_up_usize(intermediate, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let down_weights = round_up_usize(hidden, config.row_tile)
        * round_up_usize(intermediate, config.intermediate_col_tile);
    if gate_tiled.len() != gate_weights {
        return Err(format!(
            "gate_tiled length must be {gate_weights}, got {}",
            gate_tiled.len()
        ));
    }
    if up_tiled.len() != gate_weights {
        return Err(format!(
            "up_tiled length must be {gate_weights}, got {}",
            up_tiled.len()
        ));
    }
    if down_tiled.len() != down_weights {
        return Err(format!(
            "down_tiled length must be {down_weights}, got {}",
            down_tiled.len()
        ));
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let hidden_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let intermediate_u32 = u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let intermediate_col_tile_u32 = u32::try_from(config.intermediate_col_tile)
        .map_err(|_| "intermediate_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;
    let intermediate_col_tiles_u32 =
        u32::try_from(intermediate.div_ceil(config.intermediate_col_tile))
            .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let gate = dev.new_buffer(gate_tiled.len() * std::mem::size_of::<u16>())?;
    let up = dev.new_buffer(up_tiled.len() * std::mem::size_of::<u16>())?;
    let down = dev.new_buffer(down_tiled.len() * std::mem::size_of::<u16>())?;
    let normed = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let act = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    let out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, x_host);
        norm.write(0, norm_host);
        gate.write(0, gate_tiled);
        up.write(0, up_tiled);
        down.write(0, down_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_ffn_block_once(
            &dev,
            &x,
            &norm,
            &gate,
            &up,
            &down,
            &normed,
            &act,
            &out,
            tokens_u32,
            hidden_u32,
            intermediate_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            intermediate_col_tile_u32,
            hidden_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_ffn_block_once(
            &dev,
            &x,
            &norm,
            &gate,
            &up,
            &down,
            &normed,
            &act,
            &out,
            tokens_u32,
            hidden_u32,
            intermediate_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            intermediate_col_tile_u32,
            hidden_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; hidden.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let token_tile = prefill_delta_ffn_gate_up_token_tile();
    let gate_up_groups = config.tokens.div_ceil(token_tile);
    let (_, down_token_tile) = prefill_down_matmul_kernel();
    let down_groups = config.tokens.div_ceil(down_token_tile);
    let packed_weight_bytes =
        (gate_tiled.len() + up_tiled.len() + down_tiled.len()) * std::mem::size_of::<u16>();
    let bytes_moved = gate_up_groups
        * (gate_tiled.len() + up_tiled.len())
        * std::mem::size_of::<u16>()
        + down_groups * down_tiled.len() * std::mem::size_of::<u16>()
        + config.tokens
            * (hidden * 2 + hidden * 2 + intermediate * 2 + hidden * std::mem::size_of::<f32>());
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillFfnBlockBenchResult {
        tokens: config.tokens,
        hidden,
        intermediate,
        row_tile: config.row_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_prefill_gate_up_mma_compare_with_weights(
    config: PrefillFfnBlockBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    gate_tiled: &[u16],
    up_tiled: &[u16],
) -> Result<PrefillGateUpMmaCompareResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let (mma_kernel, mma_token_tile) = prefill_ffn_gate_up_mma_kernel();
    if !config.tokens.is_multiple_of(mma_token_tile) || config.row_tile != 8 {
        return Err(format!(
            "gate/up MMA compare requires token count multiple of {mma_token_tile} and row_tile=8"
        ));
    }
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(
        intermediate,
        hidden,
        config.row_tile,
        config.hidden_col_tile,
    )?;
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if norm_host.len() != hidden {
        return Err(format!(
            "norm_host length must be {hidden}, got {}",
            norm_host.len()
        ));
    }
    let gate_weights = round_up_usize(intermediate, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    if gate_tiled.len() != gate_weights || up_tiled.len() != gate_weights {
        return Err("gate/up weight buffers have invalid packed length".to_string());
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let intermediate_u32 = u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let gate = dev.new_buffer(gate_tiled.len() * std::mem::size_of::<u16>())?;
    let up = dev.new_buffer(up_tiled.len() * std::mem::size_of::<u16>())?;
    let baseline = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    let normed = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let mma = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    unsafe {
        x.write(0, x_host);
        norm.write(0, norm_host);
        gate.write(0, gate_tiled);
        up.write(0, up_tiled);
    }

    let norm_pso = dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?;
    let mma_pso = dev.pipeline(mma_kernel)?;

    let run_mma = || -> Result<(), String> {
        let cmd = dev.command_buffer()?;
        let enc = cmd.compute()?;
        enc.set_pipeline(&norm_pso);
        enc.set_buffer(0, &x, 0);
        enc.set_buffer(1, &norm, 0);
        enc.set_buffer(2, &normed, 0);
        enc.set_bytes(3, &tokens_u32);
        enc.dispatch_threadgroups((config.tokens, 1, 1), (256, 1, 1));

        enc.set_pipeline(&mma_pso);
        enc.set_buffer(0, &normed, 0);
        enc.set_buffer(1, &gate, 0);
        enc.set_buffer(2, &up, 0);
        enc.set_buffer(3, &mma, 0);
        enc.set_bytes(4, &tokens_u32);
        enc.set_bytes(5, &intermediate_u32);
        enc.set_bytes(6, &row_tile_u32);
        enc.set_bytes(7, &hidden_col_tile_u32);
        enc.set_bytes(8, &hidden_col_tiles_u32);
        enc.dispatch_threadgroups(
            (
                intermediate.div_ceil(config.row_tile),
                config.tokens.div_ceil(mma_token_tile),
                1,
            ),
            (32, 1, 1),
        );
        enc.end();
        cmd.commit_and_wait()
    };

    for _ in 0..config.warmup {
        dispatch_prefill_ffn_gate_up_once(
            &dev,
            &x,
            &norm,
            &gate,
            &up,
            &baseline,
            tokens_u32,
            intermediate_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            hidden_col_tiles_u32,
        )?;
        run_mma()?;
    }

    let mut baseline_samples = Vec::with_capacity(config.iterations);
    let mut mma_samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_ffn_gate_up_once(
            &dev,
            &x,
            &norm,
            &gate,
            &up,
            &baseline,
            tokens_u32,
            intermediate_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            hidden_col_tiles_u32,
        )?;
        baseline_samples.push(start.elapsed().as_secs_f64());

        let start = Instant::now();
        run_mma()?;
        mma_samples.push(start.elapsed().as_secs_f64());
    }
    baseline_samples.sort_by(|a, b| a.total_cmp(b));
    mma_samples.sort_by(|a, b| a.total_cmp(b));

    let mut baseline_host = vec![0u16; config.tokens * intermediate];
    let mut mma_host = vec![0u16; config.tokens * intermediate];
    unsafe {
        baseline.read(0, &mut baseline_host);
        mma.read(0, &mut mma_host);
    }
    let baseline_checksum = baseline_host
        .iter()
        .take(intermediate.min(16))
        .map(|v| f16::from_bits(*v).to_f32())
        .sum::<f32>();
    let mma_checksum = mma_host
        .iter()
        .take(intermediate.min(16))
        .map(|v| f16::from_bits(*v).to_f32())
        .sum::<f32>();
    let mut max_abs_error = 0.0f32;
    let mut max_abs_index = 0usize;
    let mut abs_sum = 0.0f32;
    for (idx, (a, b)) in mma_host.iter().zip(baseline_host.iter()).enumerate() {
        let err = (f16::from_bits(*a).to_f32() - f16::from_bits(*b).to_f32()).abs();
        abs_sum += err;
        if err > max_abs_error {
            max_abs_error = err;
            max_abs_index = idx;
        }
    }
    let mean_abs_error = abs_sum / baseline_host.len().max(1) as f32;

    Ok(PrefillGateUpMmaCompareResult {
        tokens: config.tokens,
        hidden,
        intermediate,
        mma_token_tile,
        baseline_median_s: percentile_sorted(&baseline_samples, 0.50),
        baseline_p95_s: percentile_sorted(&baseline_samples, 0.95),
        mma_median_s: percentile_sorted(&mma_samples, 0.50),
        mma_p95_s: percentile_sorted(&mma_samples, 0.95),
        baseline_checksum,
        mma_checksum,
        max_abs_error,
        mean_abs_error,
        max_abs_index,
    })
}

pub fn run_prefill_attention_project_with_weights(
    config: PrefillAttentionProjectBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    q_tiled: &[u16],
    k_tiled: &[u16],
    v_tiled: &[u16],
) -> Result<PrefillAttentionProjectBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let q_rows = QWEN35_08B.attention_q_with_head_gate_width();
    let kv_rows = QWEN35_08B.attention_kv_width();
    validate_tiled_matvec_shape(q_rows, hidden, config.row_tile, config.col_tile)?;
    validate_tiled_matvec_shape(kv_rows, hidden, config.row_tile, config.col_tile)?;
    if prefill_project_mma_enabled() && (!config.tokens.is_multiple_of(8) || config.row_tile != 8) {
        return Err("PROJECT_MMA requires token count multiple of 8 and row_tile=8".to_string());
    }
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if norm_host.len() != hidden {
        return Err(format!(
            "norm_host length must be {hidden}, got {}",
            norm_host.len()
        ));
    }
    let q_weights = tiled_matvec_len(q_rows, hidden, config.row_tile, config.col_tile);
    let kv_weights = tiled_matvec_len(kv_rows, hidden, config.row_tile, config.col_tile);
    if q_tiled.len() != q_weights || k_tiled.len() != kv_weights || v_tiled.len() != kv_weights {
        return Err("attention q/k/v tiled weight length mismatch".to_string());
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let q_rows_u32 = u32::try_from(q_rows).map_err(|_| "q_rows exceed u32")?;
    let kv_rows_u32 = u32::try_from(kv_rows).map_err(|_| "kv_rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let normed = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let q = new_readonly_buffer(&dev, q_tiled)?;
    let k = new_readonly_buffer(&dev, k_tiled)?;
    let v = new_readonly_buffer(&dev, v_tiled)?;
    let q_out = dev.new_buffer(config.tokens * q_rows * std::mem::size_of::<f32>())?;
    let k_out = dev.new_buffer(config.tokens * kv_rows * std::mem::size_of::<f32>())?;
    let v_out = dev.new_buffer(config.tokens * kv_rows * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, x_host);
        norm.write(0, norm_host);
    }

    let norm_pso = dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?;
    let (project_kernel, token_tile, project_threads) = if prefill_project_mma_enabled() {
        (
            "qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32",
            8usize,
            32usize,
        )
    } else {
        (
            "qwen35_08b_prefill_matmul_rowtiles_tok4_simd_fp16_tiled_k1024_f32",
            4usize,
            256usize,
        )
    };
    let project_pso = dev.pipeline(project_kernel)?;
    let run_once = || -> Result<(), String> {
        let cmd = dev.command_buffer()?;
        let enc = cmd.compute()?;
        enc.set_pipeline(&norm_pso);
        enc.set_buffer(0, &x, 0);
        enc.set_buffer(1, &norm, 0);
        enc.set_buffer(2, &normed, 0);
        enc.set_bytes(3, &tokens_u32);
        enc.dispatch_threadgroups((config.tokens, 1, 1), (256, 1, 1));

        enc.set_pipeline(&project_pso);
        for (weights, output, rows) in [
            (&q, &q_out, q_rows_u32),
            (&k, &k_out, kv_rows_u32),
            (&v, &v_out, kv_rows_u32),
        ] {
            enc.set_buffer(0, &normed, 0);
            enc.set_buffer(1, weights, 0);
            enc.set_buffer(2, output, 0);
            enc.set_bytes(3, &tokens_u32);
            enc.set_bytes(4, &rows);
            enc.set_bytes(5, &row_tile_u32);
            enc.set_bytes(6, &col_tile_u32);
            enc.set_bytes(7, &hidden_col_tiles_u32);
            enc.dispatch_threadgroups(
                (
                    (rows as usize).div_ceil(config.row_tile),
                    config.tokens.div_ceil(token_tile),
                    1,
                ),
                (project_threads, 1, 1),
            );
        }
        enc.end();
        cmd.commit_and_wait()
    };

    for _ in 0..config.warmup {
        run_once()?;
    }
    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        run_once()?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; 16];
    unsafe {
        q_out.read(0, &mut first);
    }
    let checksum = first.iter().sum::<f32>();
    let token_groups = config.tokens.div_ceil(token_tile);
    let packed_weight_bytes = (q_tiled.len() + k_tiled.len() + v_tiled.len()) * 2;
    let bytes_moved = token_groups * packed_weight_bytes
        + config.tokens
            * (hidden * 2 + hidden * 2 + (q_rows + kv_rows + kv_rows) * std::mem::size_of::<f32>());
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillAttentionProjectBenchResult {
        tokens: config.tokens,
        hidden,
        q_rows,
        kv_rows,
        row_tile: config.row_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

struct MpsTiledAttentionScratch {
    plan: MpsTiledAttentionPlan,
    q_tile: usize,
    k_tile: usize,
    q_rows: usize,
    q_blocks: usize,
    k_blocks: usize,
    q_row_stride: usize,
    k_row_stride: usize,
    v_row_stride: usize,
    score_row_stride: usize,
    out_row_stride: usize,
    q: Buffer,
    k: Buffer,
    v: Buffer,
    score: Buffer,
    prob: Buffer,
    pv: Buffer,
    out: Buffer,
    m_state: Buffer,
    l_state: Buffer,
    old_scale: Buffer,
    inv_l: Buffer,
    pv_scale: Buffer,
}

impl MpsTiledAttentionScratch {
    fn new(dev: &Device, tokens: usize, q_tile: usize, k_tile: usize) -> Result<Self, String> {
        let head_dim = QWEN35_08B.attention_head_dim;
        let heads_per_group = QWEN35_08B.attention_q_heads / QWEN35_08B.attention_kv_heads;
        let element_bytes = std::mem::size_of::<u16>();
        let q_row_bytes = aligned_mps_row_bytes(head_dim, element_bytes);
        let k_row_bytes = aligned_mps_row_bytes(tokens, element_bytes);
        let v_row_bytes = aligned_mps_row_bytes(head_dim, element_bytes);
        let score_row_bytes = aligned_mps_row_bytes(k_tile, element_bytes);
        let out_row_bytes = aligned_mps_row_bytes(head_dim, element_bytes);
        let q_rows = q_tile * heads_per_group;
        let q_matrix_rows = tokens * heads_per_group;
        let q_blocks = tokens.div_ceil(q_tile);
        let k_blocks = tokens.div_ceil(k_tile);
        let plan = MpsTiledAttentionPlan::new(
            dev,
            tokens,
            q_tile,
            k_tile,
            head_dim,
            heads_per_group,
            q_row_bytes,
            k_row_bytes,
            v_row_bytes,
            score_row_bytes,
            out_row_bytes,
        )?;
        let q = dev.new_buffer(q_matrix_rows * q_row_bytes)?;
        let k = dev.new_buffer(head_dim * k_row_bytes)?;
        let v = dev.new_buffer(tokens * v_row_bytes)?;
        let score = dev.new_buffer(q_rows * score_row_bytes)?;
        let prob = dev.new_buffer(q_rows * score_row_bytes)?;
        let pv = dev.new_buffer(q_rows * out_row_bytes)?;
        let out = dev.new_buffer(q_rows * out_row_bytes)?;
        let row_state_bytes = q_rows * std::mem::size_of::<f32>();
        let m_state = dev.new_buffer(row_state_bytes)?;
        let l_state = dev.new_buffer(row_state_bytes)?;
        let old_scale = dev.new_buffer(row_state_bytes)?;
        let inv_l = dev.new_buffer(row_state_bytes)?;
        let pv_scale = dev.new_buffer(row_state_bytes)?;
        Ok(Self {
            plan,
            q_tile,
            k_tile,
            q_rows,
            q_blocks,
            k_blocks,
            q_row_stride: q_row_bytes / element_bytes,
            k_row_stride: k_row_bytes / element_bytes,
            v_row_stride: v_row_bytes / element_bytes,
            score_row_stride: score_row_bytes / element_bytes,
            out_row_stride: out_row_bytes / element_bytes,
            q,
            k,
            v,
            score,
            prob,
            pv,
            out,
            m_state,
            l_state,
            old_scale,
            inv_l,
            pv_scale,
        })
    }
}

fn aligned_mps_row_bytes(columns: usize, element_bytes: usize) -> usize {
    (columns * element_bytes).div_ceil(128) * 128
}

#[allow(clippy::too_many_arguments)]
fn encode_mps_tiled_attention_to_qwen_attn(
    dev: &Device,
    cmd: &CommandBuffer,
    scratch: &MpsTiledAttentionScratch,
    tokens: usize,
    q_project_rows: usize,
    q_cache: &Buffer,
    k_cache: &Buffer,
    v_cache: &Buffer,
    q_tokens: &Buffer,
    attn: &Buffer,
) -> Result<(), String> {
    for kv_group in 0..QWEN35_08B.attention_kv_heads {
        encode_mps_tiled_pack_qwen_group(
            dev,
            cmd,
            tokens,
            kv_group,
            scratch.q_row_stride,
            scratch.k_row_stride,
            scratch.v_row_stride,
            q_cache,
            k_cache,
            v_cache,
            &scratch.q,
            &scratch.k,
            &scratch.v,
        )?;
        for qb in 0..scratch.q_blocks {
            encode_mps_tiled_init(
                dev,
                cmd,
                scratch.q_rows,
                QWEN35_08B.attention_head_dim,
                &scratch.m_state,
                &scratch.l_state,
                &scratch.out,
            )?;
            let q_last = ((qb + 1) * scratch.q_tile).min(tokens) - 1;
            let allowed_k_blocks = scratch.k_blocks.min(q_last / scratch.k_tile + 1);
            for kb in 0..allowed_k_blocks {
                scratch
                    .plan
                    .encode_qk(cmd, &scratch.q, &scratch.k, &scratch.score, qb, kb)?;
                encode_mps_tiled_softmax(
                    dev,
                    cmd,
                    scratch.q_rows,
                    scratch.k_tile,
                    scratch.score_row_stride,
                    qb,
                    kb,
                    scratch.q_tile,
                    &scratch.score,
                    &scratch.prob,
                    &scratch.m_state,
                    &scratch.l_state,
                    &scratch.old_scale,
                    &scratch.inv_l,
                    &scratch.pv_scale,
                )?;
                scratch
                    .plan
                    .encode_pv(cmd, &scratch.prob, &scratch.v, &scratch.pv, kb)?;
                encode_mps_tiled_combine(
                    dev,
                    cmd,
                    scratch.q_rows,
                    QWEN35_08B.attention_head_dim,
                    scratch.out_row_stride,
                    &scratch.out,
                    &scratch.pv,
                    &scratch.old_scale,
                    &scratch.inv_l,
                    &scratch.pv_scale,
                )?;
            }
            encode_mps_tiled_store_qwen_attn(
                dev,
                cmd,
                tokens,
                scratch.q_tile,
                q_project_rows,
                scratch.out_row_stride,
                qb,
                kv_group,
                &scratch.out,
                q_tokens,
                attn,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_mps_tiled_pack_qwen_group(
    dev: &Device,
    cmd: &CommandBuffer,
    tokens: usize,
    kv_group: usize,
    q_row_stride: usize,
    k_row_stride: usize,
    v_row_stride: usize,
    q_cache: &Buffer,
    k_cache: &Buffer,
    v_cache: &Buffer,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_pack_qwen_qkv_group")?;
    let enc = cmd.compute()?;
    let tokens_u32 = tokens as u32;
    let kv_group_u32 = kv_group as u32;
    let q_row_stride_u32 = q_row_stride as u32;
    let k_row_stride_u32 = k_row_stride as u32;
    let v_row_stride_u32 = v_row_stride as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, q_cache, 0);
    enc.set_buffer(1, k_cache, 0);
    enc.set_buffer(2, v_cache, 0);
    enc.set_buffer(3, q, 0);
    enc.set_buffer(4, k, 0);
    enc.set_buffer(5, v, 0);
    enc.set_bytes(6, &tokens_u32);
    enc.set_bytes(7, &kv_group_u32);
    enc.set_bytes(8, &q_row_stride_u32);
    enc.set_bytes(9, &k_row_stride_u32);
    enc.set_bytes(10, &v_row_stride_u32);
    enc.dispatch_threads(
        tokens
            * (QWEN35_08B.attention_q_heads / QWEN35_08B.attention_kv_heads)
            * QWEN35_08B.attention_head_dim,
        256,
    );
    enc.end();
    Ok(())
}

fn encode_mps_tiled_init(
    dev: &Device,
    cmd: &CommandBuffer,
    q_rows: usize,
    head_dim: usize,
    m_state: &Buffer,
    l_state: &Buffer,
    out: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_init_rows")?;
    let enc = cmd.compute()?;
    let q_rows_u32 = q_rows as u32;
    let head_dim_u32 = head_dim as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, m_state, 0);
    enc.set_buffer(1, l_state, 0);
    enc.set_buffer(2, out, 0);
    enc.set_bytes(3, &q_rows_u32);
    enc.set_bytes(4, &head_dim_u32);
    enc.dispatch_threads((q_rows * head_dim).max(q_rows), 256);
    enc.end();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_mps_tiled_softmax(
    dev: &Device,
    cmd: &CommandBuffer,
    q_rows: usize,
    k_tile: usize,
    score_row_stride: usize,
    q_block: usize,
    k_block: usize,
    q_tile: usize,
    score: &Buffer,
    prob: &Buffer,
    m_state: &Buffer,
    l_state: &Buffer,
    old_scale: &Buffer,
    inv_l: &Buffer,
    pv_scale: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_softmax_update_simd32")?;
    let enc = cmd.compute()?;
    let q_rows_u32 = q_rows as u32;
    let k_tile_u32 = k_tile as u32;
    let score_row_stride_u32 = score_row_stride as u32;
    let q_block_u32 = q_block as u32;
    let k_block_u32 = k_block as u32;
    let q_tile_u32 = q_tile as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, score, 0);
    enc.set_buffer(1, prob, 0);
    enc.set_buffer(2, m_state, 0);
    enc.set_buffer(3, l_state, 0);
    enc.set_buffer(4, old_scale, 0);
    enc.set_buffer(5, inv_l, 0);
    enc.set_buffer(6, pv_scale, 0);
    enc.set_bytes(7, &q_rows_u32);
    enc.set_bytes(8, &k_tile_u32);
    enc.set_bytes(9, &score_row_stride_u32);
    enc.set_bytes(10, &q_block_u32);
    enc.set_bytes(11, &k_block_u32);
    enc.set_bytes(12, &q_tile_u32);
    enc.dispatch_threads(q_rows * 32, 256);
    enc.end();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_mps_tiled_combine(
    dev: &Device,
    cmd: &CommandBuffer,
    q_rows: usize,
    head_dim: usize,
    out_row_stride: usize,
    out: &Buffer,
    pv: &Buffer,
    old_scale: &Buffer,
    inv_l: &Buffer,
    pv_scale: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_combine")?;
    let enc = cmd.compute()?;
    let q_rows_u32 = q_rows as u32;
    let head_dim_u32 = head_dim as u32;
    let out_row_stride_u32 = out_row_stride as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, out, 0);
    enc.set_buffer(1, pv, 0);
    enc.set_buffer(2, old_scale, 0);
    enc.set_buffer(3, inv_l, 0);
    enc.set_buffer(4, pv_scale, 0);
    enc.set_bytes(5, &q_rows_u32);
    enc.set_bytes(6, &head_dim_u32);
    enc.set_bytes(7, &out_row_stride_u32);
    enc.dispatch_threads(q_rows * head_dim, 256);
    enc.end();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_mps_tiled_store_qwen_attn(
    dev: &Device,
    cmd: &CommandBuffer,
    tokens: usize,
    q_tile: usize,
    q_project_rows: usize,
    out_row_stride: usize,
    q_block: usize,
    kv_group: usize,
    out: &Buffer,
    q_tokens: &Buffer,
    attn: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_store_qwen_attn_with_gate")?;
    let enc = cmd.compute()?;
    let q_project_rows_u32 = q_project_rows as u32;
    let out_row_stride_u32 = out_row_stride as u32;
    let q_block_u32 = q_block as u32;
    let q_tile_u32 = q_tile as u32;
    let tokens_u32 = tokens as u32;
    let kv_group_u32 = kv_group as u32;
    let q_rows = q_tile * (QWEN35_08B.attention_q_heads / QWEN35_08B.attention_kv_heads);
    enc.set_pipeline(&pso);
    enc.set_buffer(0, out, 0);
    enc.set_buffer(1, q_tokens, 0);
    enc.set_buffer(2, attn, 0);
    enc.set_bytes(3, &q_project_rows_u32);
    enc.set_bytes(4, &out_row_stride_u32);
    enc.set_bytes(5, &q_block_u32);
    enc.set_bytes(6, &q_tile_u32);
    enc.set_bytes(7, &tokens_u32);
    enc.set_bytes(8, &kv_group_u32);
    enc.dispatch_threads(q_rows * QWEN35_08B.attention_head_dim, 256);
    enc.end();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_prefill_attention_core_with_weights(
    config: PrefillAttentionCoreBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    q_norm_host: &[u16],
    k_norm_host: &[u16],
    q_tiled: &[u16],
    k_tiled: &[u16],
    v_tiled: &[u16],
    o_tiled: &[u16],
    mps_attention_out_weight: Option<&[u8]>,
) -> Result<PrefillAttentionCoreBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let q_rows = QWEN35_08B.attention_q_with_head_gate_width();
    let kv_rows = QWEN35_08B.attention_kv_width();
    let attention_width = QWEN35_08B.attention_q_width();
    validate_tiled_matvec_shape(q_rows, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(kv_rows, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(
        hidden,
        attention_width,
        config.row_tile,
        config.attention_col_tile,
    )?;
    let (_, attention_project_mma_token_tile) =
        prefill_attention_project_mma_kernel_for_tokens(config.tokens);
    if config.use_project_mma
        && (!config
            .tokens
            .is_multiple_of(attention_project_mma_token_tile)
            || config.row_tile != 8)
    {
        return Err(
            "attention core PROJECT_MMA requires compatible token count and row_tile=8".to_string(),
        );
    }
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if norm_host.len() != hidden {
        return Err(format!(
            "norm_host length must be {hidden}, got {}",
            norm_host.len()
        ));
    }
    if q_norm_host.len() != QWEN35_08B.attention_head_dim
        || k_norm_host.len() != QWEN35_08B.attention_head_dim
    {
        return Err("q_norm/k_norm length must match attention head_dim".to_string());
    }
    let q_weights = tiled_matvec_len(q_rows, hidden, config.row_tile, config.hidden_col_tile);
    let kv_weights = tiled_matvec_len(kv_rows, hidden, config.row_tile, config.hidden_col_tile);
    let o_weights = tiled_matvec_len(
        hidden,
        attention_width,
        config.row_tile,
        config.attention_col_tile,
    );
    if q_tiled.len() != q_weights
        || k_tiled.len() != kv_weights
        || v_tiled.len() != kv_weights
        || o_tiled.len() != o_weights
    {
        return Err("attention core tiled weight length mismatch".to_string());
    }
    if let Some(weight) = mps_attention_out_weight {
        let expected = attention_width * hidden * std::mem::size_of::<u16>();
        if weight.len() != expected {
            return Err(format!(
                "MPS attention out weight length mismatch: expected {expected}, got {}",
                weight.len()
            ));
        }
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let hidden_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let q_rows_u32 = u32::try_from(q_rows).map_err(|_| "q_rows exceed u32")?;
    let kv_rows_u32 = u32::try_from(kv_rows).map_err(|_| "kv_rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let attention_col_tile_u32 =
        u32::try_from(config.attention_col_tile).map_err(|_| "attention_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;
    let attention_col_tiles_u32 =
        u32::try_from(attention_width.div_ceil(config.attention_col_tile))
            .map_err(|_| "attention col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let q_norm = new_readonly_buffer(&dev, q_norm_host)?;
    let k_norm = new_readonly_buffer(&dev, k_norm_host)?;
    let normed = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let q = new_readonly_buffer(&dev, q_tiled)?;
    let k = new_readonly_buffer(&dev, k_tiled)?;
    let v = new_readonly_buffer(&dev, v_tiled)?;
    let o = new_readonly_buffer(&dev, o_tiled)?;
    let mps_attention_out_weight_buffer = if let Some(weight) = mps_attention_out_weight {
        let buffer = dev.new_buffer(weight.len())?;
        unsafe {
            buffer.write(0, weight);
        }
        Some(buffer)
    } else {
        None
    };
    let q_out = dev.new_buffer(config.tokens * q_rows * std::mem::size_of::<f32>())?;
    let k_out = dev.new_buffer(config.tokens * kv_rows * std::mem::size_of::<f32>())?;
    let v_out = dev.new_buffer(config.tokens * kv_rows * std::mem::size_of::<f32>())?;
    let q_cache = dev.new_buffer(config.tokens * attention_width * std::mem::size_of::<u16>())?;
    let qh4_simd32_vec8_interleaved_kv = prefill_attention_qh4_simd32_vec8_interleaved_kv_enabled();
    let qh4_simd32_vec8_int8_kv = prefill_attention_qh4_simd32_vec8_int8_kv_enabled();
    let qh4_simd32_vec8_int8_v = prefill_attention_qh4_simd32_vec8_int8_v_enabled();
    let qh4_simd32_vec8_int8_v_pack4 = prefill_attention_qh4_simd32_vec8_int8_v_pack4_enabled();
    let qh4_simd32_vec8_halfacc = prefill_attention_qh4_simd32_vec8_halfacc_enabled();
    let qh4_simd32_vec8_halfdot = prefill_attention_qh4_simd32_vec8_halfdot_enabled();
    let k_cache_elems = if qh4_simd32_vec8_interleaved_kv {
        config.tokens * kv_rows * 2
    } else {
        config.tokens * kv_rows
    };
    let k_cache_bytes = if qh4_simd32_vec8_int8_kv {
        config.tokens * kv_rows
    } else {
        k_cache_elems * std::mem::size_of::<u16>()
    };
    let v_cache_bytes =
        if qh4_simd32_vec8_int8_kv || qh4_simd32_vec8_int8_v || qh4_simd32_vec8_int8_v_pack4 {
            config.tokens * kv_rows
        } else {
            config.tokens * kv_rows * std::mem::size_of::<u16>()
        };
    let k_cache = dev.new_buffer(k_cache_bytes)?;
    let v_cache = dev.new_buffer(v_cache_bytes)?;
    let kv_scale =
        if qh4_simd32_vec8_int8_kv || qh4_simd32_vec8_int8_v || qh4_simd32_vec8_int8_v_pack4 {
            Some(dev.new_buffer(
                config.tokens * QWEN35_08B.attention_kv_heads * std::mem::size_of::<u16>(),
            )?)
        } else {
            None
        };
    let attn = dev.new_buffer(config.tokens * attention_width * std::mem::size_of::<u16>())?;
    let out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let partial_attention = prefill_attention_partial_qblk2_enabled();
    let qh4_splitk64 = prefill_attention_qh4_splitk64_enabled();
    let qh4_splitk128 = prefill_attention_qh4_splitk128_enabled();
    let qh4_splitk256 = prefill_attention_qh4_splitk256_enabled();
    let qh4_splitk512 = prefill_attention_qh4_splitk512_enabled();
    let qh4_splitk = qh4_splitk64 || qh4_splitk128 || qh4_splitk256 || qh4_splitk512;
    let split_attention = partial_attention || qh4_splitk;
    let mps_tiled_attention = prefill_attention_mps_tiled_enabled();
    let mps_q_tile =
        prefill_attention_mps_tile_env("CTOX_QWEN35_ATTENTION_MPS_Q_TILE", 256)?.min(config.tokens);
    let mps_k_tile = prefill_attention_mps_tile_env("CTOX_QWEN35_ATTENTION_MPS_K_TILE", 1024)?
        .min(config.tokens);
    if mps_tiled_attention && !device_supports_mps(&dev) {
        return Err("CTOX_QWEN35_ATTENTION_MPS_TILED requires MPS support".to_owned());
    }
    if mps_tiled_attention && mps_q_tile > config.tokens {
        return Err("CTOX_QWEN35_ATTENTION_MPS_Q_TILE must be <= tokens".to_owned());
    }
    let partial_key_block = if qh4_splitk512 {
        512usize
    } else if qh4_splitk256 {
        256usize
    } else if qh4_splitk128 {
        128usize
    } else {
        64usize
    };
    let partial_key_blocks = config.tokens.div_ceil(partial_key_block);
    let partial_scalars = config.tokens * QWEN35_08B.attention_q_heads * partial_key_blocks;
    let partial_m = if split_attention {
        Some(dev.new_buffer(partial_scalars * std::mem::size_of::<f32>())?)
    } else {
        None
    };
    let partial_l = if split_attention {
        Some(dev.new_buffer(partial_scalars * std::mem::size_of::<f32>())?)
    } else {
        None
    };
    let partial_acc = if split_attention {
        Some(dev.new_buffer(
            partial_scalars * QWEN35_08B.attention_head_dim * std::mem::size_of::<f32>(),
        )?)
    } else {
        None
    };
    let mps_attention_scratch = if mps_tiled_attention {
        Some(MpsTiledAttentionScratch::new(
            &dev,
            config.tokens,
            mps_q_tile,
            mps_k_tile,
        )?)
    } else {
        None
    };
    unsafe {
        x.write(0, x_host);
        norm.write(0, norm_host);
    }

    let norm_pso = dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?;
    let (project_kernel, project_token_tile, project_threads) = if config.use_project_mma {
        let (kernel, token_tile) = prefill_attention_project_mma_kernel_for_tokens(config.tokens);
        (kernel, token_tile, 32usize)
    } else {
        (
            "qwen35_08b_prefill_matmul_rowtiles_tok4_simd_fp16_tiled_k1024_f32",
            4usize,
            256usize,
        )
    };
    let project_pso = dev.pipeline(project_kernel)?;
    let prepare_pso = if qh4_simd32_vec8_int8_kv {
        dev.pipeline("qwen35_08b_prefill_attention_prepare_qk_rope_v_int8_gqa8_kv2_d256")?
    } else if qh4_simd32_vec8_int8_v || qh4_simd32_vec8_int8_v_pack4 {
        dev.pipeline("qwen35_08b_prefill_attention_prepare_qk_rope_v_int8_v_gqa8_kv2_d256")?
    } else if qh4_simd32_vec8_interleaved_kv {
        dev.pipeline("qwen35_08b_prefill_attention_prepare_qk_rope_v_interleaved_gqa8_kv2_d256")?
    } else {
        dev.pipeline("qwen35_08b_prefill_attention_prepare_qk_rope_v_gqa8_kv2_d256")?
    };
    let qblk2 = prefill_attention_qblk2_enabled();
    let qblk4 = prefill_attention_qblk4_enabled();
    let qblk2x512 = prefill_attention_qblk2x512_enabled();
    let attention_simdreduce = prefill_attention_simdreduce_enabled();
    let qblk2_simdreduce = prefill_attention_qblk2_simdreduce_enabled();
    let qblk4_simdreduce = prefill_attention_qblk4_simdreduce_enabled();
    let qblk4_simdreduce_batch = prefill_attention_qblk4_simdreduce_batch_enabled();
    let qblk8_simdreduce_batch = prefill_attention_qblk8_simdreduce_batch_enabled();
    let qh2_qblk4_simdreduce_batch = prefill_attention_qh2_qblk4_simdreduce_batch_enabled();
    let qh4_qblk2_simdreduce_batch = prefill_attention_qh4_qblk2_simdreduce_batch_enabled();
    let qh4_simd32_vec8 = prefill_attention_qh4_simd32_vec8_enabled();
    let qh4_qblk2_simd32_vec8 = prefill_attention_qh4_qblk2_simd32_vec8_enabled();
    let qh4_simd32_vec8_win4096 = prefill_attention_qh4_simd32_vec8_win4096_enabled();
    let qh4_simd32_vec8_window = prefill_attention_qh4_simd32_vec8_window()?;
    let qh4_simd32_vec8_window_enabled = qh4_simd32_vec8_window.is_some();
    let qh4_simd32_vec8_window_u32 = qh4_simd32_vec8_window.unwrap_or(4096);
    let qh4_simd32_vec8_window_halfdot = prefill_attention_qh4_simd32_vec8_window_halfdot()?;
    let qh4_simd32_vec8_window_halfdot_enabled = qh4_simd32_vec8_window_halfdot.is_some();
    let qh4_simd32_vec8_window_halfdot_u32 = qh4_simd32_vec8_window_halfdot.unwrap_or(4096);
    let qh4_qblk1_simdreduce_batch = prefill_attention_qh4_qblk1_simdreduce_batch_enabled();
    let explicit_attention_variants = [
        partial_attention,
        qh4_splitk64,
        qh4_splitk128,
        qh4_splitk256,
        qh4_splitk512,
        mps_tiled_attention,
        qh2_qblk4_simdreduce_batch,
        qh4_qblk2_simdreduce_batch,
        qh4_simd32_vec8,
        qh4_simd32_vec8_interleaved_kv,
        qh4_simd32_vec8_int8_kv,
        qh4_simd32_vec8_int8_v,
        qh4_simd32_vec8_int8_v_pack4,
        qh4_simd32_vec8_halfacc,
        qh4_simd32_vec8_halfdot,
        qh4_qblk2_simd32_vec8,
        qh4_simd32_vec8_window_halfdot_enabled,
        qh4_simd32_vec8_window_enabled,
        qh4_simd32_vec8_win4096,
        qh4_qblk1_simdreduce_batch,
        qblk8_simdreduce_batch,
        qblk4_simdreduce_batch,
        qblk4_simdreduce,
        qblk2_simdreduce,
        qblk2x512,
        qblk4,
        qblk2,
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count();
    if explicit_attention_variants > 1 {
        return Err(
            "set only one CTOX_QWEN35_ATTENTION_* variant flag for prefill attention".to_owned(),
        );
    }
    let (attention_query_block, attention_threads) = if partial_attention {
        (2usize, 512usize)
    } else if qh4_splitk {
        (1usize, 32usize)
    } else if qh2_qblk4_simdreduce_batch {
        (4usize, QWEN35_08B.attention_head_dim)
    } else if qh4_qblk2_simdreduce_batch {
        (2usize, QWEN35_08B.attention_head_dim)
    } else if qh4_qblk2_simd32_vec8 {
        (2usize, 32usize)
    } else if qh4_simd32_vec8_halfdot {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_halfacc {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_int8_v {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_int8_v_pack4 {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_int8_kv {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_interleaved_kv {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_window_halfdot_enabled {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_window_enabled {
        (1usize, 32usize)
    } else if qh4_simd32_vec8_win4096 {
        (1usize, 32usize)
    } else if qh4_simd32_vec8 {
        (1usize, 32usize)
    } else if qh4_qblk1_simdreduce_batch {
        (1usize, QWEN35_08B.attention_head_dim)
    } else if qblk8_simdreduce_batch {
        (8usize, QWEN35_08B.attention_head_dim)
    } else if qblk4_simdreduce_batch {
        (4usize, QWEN35_08B.attention_head_dim)
    } else if qblk4_simdreduce {
        (4usize, QWEN35_08B.attention_head_dim)
    } else if qblk2_simdreduce {
        (2usize, QWEN35_08B.attention_head_dim)
    } else if qblk2x512 {
        (2usize, 512usize)
    } else if qblk4 {
        (4usize, QWEN35_08B.attention_head_dim)
    } else if qblk2 {
        (2usize, QWEN35_08B.attention_head_dim)
    } else {
        (1usize, QWEN35_08B.attention_head_dim)
    };
    let attention_head_groups = if qh2_qblk4_simdreduce_batch {
        QWEN35_08B.attention_q_heads / 2
    } else if qh4_qblk2_simdreduce_batch
        || qh4_qblk2_simd32_vec8
        || qh4_splitk
        || qh4_simd32_vec8_int8_v
        || qh4_simd32_vec8_int8_v_pack4
        || qh4_simd32_vec8_halfacc
        || qh4_simd32_vec8_halfdot
        || qh4_simd32_vec8_int8_kv
        || qh4_simd32_vec8_interleaved_kv
        || qh4_simd32_vec8_window_halfdot_enabled
        || qh4_simd32_vec8_window_enabled
        || qh4_simd32_vec8_win4096
        || qh4_simd32_vec8
        || qh4_qblk1_simdreduce_batch
    {
        QWEN35_08B.attention_q_heads / 4
    } else {
        QWEN35_08B.attention_q_heads
    };
    let attention_pso = if partial_attention {
        dev.pipeline("qwen35_08b_prefill_attention_partial_qblk2_kblk64_gqa8_kv2_d256")?
    } else if qh4_splitk {
        dev.pipeline("qwen35_08b_prefill_attention_qh4_splitk_gqa8_kv2_d256")?
    } else if qh2_qblk4_simdreduce_batch {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh2_qblk4_simdreduce_batch_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_qblk2_simdreduce_batch {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk2_simdreduce_batch_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_qblk2_simd32_vec8 {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk2_simd32_vec8_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_int8_kv {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_kv_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_int8_v {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_v_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_int8_v_pack4 {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_v_pack4_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_halfacc {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_halfacc_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_halfdot {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_halfdot_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_interleaved_kv {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_interleaved_kv_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_window_halfdot_enabled {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_window_halfdot_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_window_enabled {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_window_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8_win4096 {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_win4096_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_simd32_vec8 {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_gqa8_kv2_d256_to_fp16",
        )?
    } else if qh4_qblk1_simdreduce_batch {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qh4_qblk1_simdreduce_batch_gqa8_kv2_d256_to_fp16",
        )?
    } else if qblk8_simdreduce_batch {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qblk8_simdreduce_batch_gqa8_kv2_d256_to_fp16",
        )?
    } else if qblk4_simdreduce_batch {
        dev.pipeline(
            "qwen35_08b_prefill_attention_causal_qblk4_simdreduce_batch_gqa8_kv2_d256_to_fp16",
        )?
    } else if qblk4_simdreduce {
        dev.pipeline("qwen35_08b_prefill_attention_causal_qblk4_simdreduce_gqa8_kv2_d256_to_fp16")?
    } else if qblk2_simdreduce {
        dev.pipeline("qwen35_08b_prefill_attention_causal_qblk2_simdreduce_gqa8_kv2_d256_to_fp16")?
    } else if qblk2x512 {
        dev.pipeline("qwen35_08b_prefill_attention_causal_qblk2x512_gqa8_kv2_d256_to_fp16")?
    } else if qblk4 {
        dev.pipeline("qwen35_08b_prefill_attention_causal_qblk4_gqa8_kv2_d256_to_fp16")?
    } else if qblk2 {
        dev.pipeline("qwen35_08b_prefill_attention_causal_qblk2_gqa8_kv2_d256_to_fp16")?
    } else if attention_simdreduce {
        dev.pipeline("qwen35_08b_prefill_attention_causal_simdreduce_gqa8_kv2_d256_to_fp16")?
    } else {
        dev.pipeline("qwen35_08b_prefill_attention_causal_gqa8_kv2_d256_to_fp16")?
    };
    let attention_combine_pso = if qh4_splitk {
        Some(dev.pipeline("qwen35_08b_prefill_attention_partial_combine_splitk_gqa8_d256_to_fp16")?)
    } else if partial_attention {
        Some(dev.pipeline("qwen35_08b_prefill_attention_partial_combine_kblk64_gqa8_d256_to_fp16")?)
    } else {
        None
    };
    let (out_kernel, out_token_tile) = prefill_deltanet_out_matmul_kernel();
    validate_deltanet_out_kernel_tile(out_token_tile, tokens_u32, row_tile_u32)?;
    let out_pso = dev.pipeline(out_kernel)?;
    let mps_attention_out_plan = if mps_attention_out_weight_buffer.is_some()
        && crate::metal::mps_sidecar::device_supports_mps(&dev)
    {
        Some(crate::metal::mps_sidecar::MpsDeltaProjectPlan::new(
            &dev,
            config.tokens,
            attention_width,
            hidden,
            attention_width * std::mem::size_of::<u16>(),
            hidden * std::mem::size_of::<u16>(),
            hidden * std::mem::size_of::<f32>(),
        )?)
    } else {
        None
    };
    let profile_stop = PrefillAttentionCoreProfileStop::from_env();

    let run_once = || -> Result<(), String> {
        let cmd = dev.command_buffer()?;
        let enc = cmd.compute()?;
        enc.set_pipeline(&norm_pso);
        enc.set_buffer(0, &x, 0);
        enc.set_buffer(1, &norm, 0);
        enc.set_buffer(2, &normed, 0);
        enc.set_bytes(3, &tokens_u32);
        enc.dispatch_threadgroups((config.tokens, 1, 1), (256, 1, 1));
        if profile_stop == PrefillAttentionCoreProfileStop::Norm {
            enc.end();
            return cmd.commit_and_wait();
        }

        enc.set_pipeline(&project_pso);
        for (weights, output, rows) in [
            (&q, &q_out, q_rows_u32),
            (&k, &k_out, kv_rows_u32),
            (&v, &v_out, kv_rows_u32),
        ] {
            enc.set_buffer(0, &normed, 0);
            enc.set_buffer(1, weights, 0);
            enc.set_buffer(2, output, 0);
            enc.set_bytes(3, &tokens_u32);
            enc.set_bytes(4, &rows);
            enc.set_bytes(5, &row_tile_u32);
            enc.set_bytes(6, &hidden_col_tile_u32);
            enc.set_bytes(7, &hidden_col_tiles_u32);
            enc.dispatch_threadgroups(
                (
                    (rows as usize).div_ceil(config.row_tile),
                    config.tokens.div_ceil(project_token_tile),
                    1,
                ),
                (project_threads, 1, 1),
            );
        }
        if profile_stop == PrefillAttentionCoreProfileStop::Project {
            enc.end();
            return cmd.commit_and_wait();
        }

        enc.set_pipeline(&prepare_pso);
        enc.set_buffer(0, &q_out, 0);
        enc.set_buffer(1, &k_out, 0);
        enc.set_buffer(2, &v_out, 0);
        enc.set_buffer(3, &q_norm, 0);
        enc.set_buffer(4, &k_norm, 0);
        enc.set_buffer(5, &q_cache, 0);
        enc.set_buffer(6, &k_cache, 0);
        enc.set_buffer(7, &v_cache, 0);
        enc.set_bytes(8, &tokens_u32);
        enc.set_bytes(9, &q_rows_u32);
        if let Some(scale) = kv_scale.as_ref() {
            enc.set_buffer(10, scale, 0);
        }
        enc.dispatch_threadgroups(
            (config.tokens, QWEN35_08B.attention_q_heads, 1),
            (QWEN35_08B.attention_head_dim, 1, 1),
        );
        if profile_stop == PrefillAttentionCoreProfileStop::Prepare {
            enc.end();
            return cmd.commit_and_wait();
        }

        if mps_tiled_attention {
            enc.end();
            let scratch = mps_attention_scratch
                .as_ref()
                .ok_or_else(|| "missing MPS tiled attention scratch".to_owned())?;
            encode_mps_tiled_attention_to_qwen_attn(
                &dev,
                &cmd,
                scratch,
                config.tokens,
                q_rows,
                &q_cache,
                &k_cache,
                &v_cache,
                &q_out,
                &attn,
            )?;
            if profile_stop == PrefillAttentionCoreProfileStop::Attention {
                return cmd.commit_and_wait();
            }

            if let (Some(plan), Some(weight)) = (
                mps_attention_out_plan.as_ref(),
                mps_attention_out_weight_buffer.as_ref(),
            ) {
                plan.encode(&cmd, &attn, weight, &out)?;
                return cmd.commit_and_wait();
            }

            let enc = cmd.compute()?;
            enc.set_pipeline(&out_pso);
            enc.set_buffer(0, &attn, 0);
            enc.set_buffer(1, &o, 0);
            enc.set_buffer(2, &out, 0);
            enc.set_bytes(3, &tokens_u32);
            enc.set_bytes(4, &hidden_u32);
            enc.set_bytes(5, &row_tile_u32);
            enc.set_bytes(6, &attention_col_tile_u32);
            enc.set_bytes(7, &attention_col_tiles_u32);
            enc.dispatch_threadgroups(
                (
                    hidden.div_ceil(config.row_tile),
                    config.tokens.div_ceil(out_token_tile),
                    1,
                ),
                (
                    prefill_deltanet_out_threadgroup_threads(out_token_tile),
                    1,
                    1,
                ),
            );
            enc.end();
            return cmd.commit_and_wait();
        }

        enc.set_pipeline(&attention_pso);
        if split_attention {
            let partial_m = partial_m
                .as_ref()
                .ok_or_else(|| "missing partial_m buffer".to_string())?;
            let partial_l = partial_l
                .as_ref()
                .ok_or_else(|| "missing partial_l buffer".to_string())?;
            let partial_acc = partial_acc
                .as_ref()
                .ok_or_else(|| "missing partial_acc buffer".to_string())?;
            let partial_key_blocks_u32 =
                u32::try_from(partial_key_blocks).map_err(|_| "partial key blocks exceed u32")?;
            enc.set_buffer(0, &q_cache, 0);
            enc.set_buffer(1, &k_cache, 0);
            enc.set_buffer(2, &v_cache, 0);
            enc.set_buffer(3, partial_m, 0);
            enc.set_buffer(4, partial_l, 0);
            enc.set_buffer(5, partial_acc, 0);
            enc.set_bytes(6, &tokens_u32);
            enc.set_bytes(7, &partial_key_blocks_u32);
            let partial_key_block_u32 =
                u32::try_from(partial_key_block).map_err(|_| "partial key block exceeds u32")?;
            if qh4_splitk {
                enc.set_bytes(8, &partial_key_block_u32);
            }
            let stage1_groups = if qh4_splitk {
                (
                    config.tokens,
                    QWEN35_08B.attention_q_heads / 4,
                    partial_key_blocks,
                )
            } else {
                (
                    config.tokens.div_ceil(attention_query_block),
                    QWEN35_08B.attention_q_heads,
                    partial_key_blocks,
                )
            };
            enc.dispatch_threadgroups(stage1_groups, (attention_threads, 1, 1));

            let combine_pso = attention_combine_pso
                .as_ref()
                .ok_or_else(|| "missing attention combine pipeline".to_string())?;
            enc.set_pipeline(combine_pso);
            enc.set_buffer(0, partial_m, 0);
            enc.set_buffer(1, partial_l, 0);
            enc.set_buffer(2, partial_acc, 0);
            enc.set_buffer(3, &q_out, 0);
            enc.set_buffer(4, &attn, 0);
            enc.set_bytes(5, &tokens_u32);
            enc.set_bytes(6, &q_rows_u32);
            enc.set_bytes(7, &partial_key_blocks_u32);
            if qh4_splitk {
                enc.set_bytes(8, &partial_key_block_u32);
            }
            enc.dispatch_threadgroups(
                (config.tokens, QWEN35_08B.attention_q_heads, 1),
                (QWEN35_08B.attention_head_dim, 1, 1),
            );
        } else if attention_query_block > 1 || attention_head_groups != QWEN35_08B.attention_q_heads
        {
            enc.set_buffer(0, &q_cache, 0);
            enc.set_buffer(1, &k_cache, 0);
            enc.set_buffer(2, &v_cache, 0);
            enc.set_buffer(3, &q_out, 0);
            enc.set_buffer(4, &attn, 0);
            enc.set_bytes(5, &tokens_u32);
            enc.set_bytes(6, &q_rows_u32);
            if qh4_simd32_vec8_window_halfdot_enabled {
                enc.set_bytes(7, &qh4_simd32_vec8_window_halfdot_u32);
            } else if qh4_simd32_vec8_window_enabled {
                enc.set_bytes(7, &qh4_simd32_vec8_window_u32);
            } else if qh4_simd32_vec8_int8_kv
                || qh4_simd32_vec8_int8_v
                || qh4_simd32_vec8_int8_v_pack4
            {
                let scale = kv_scale
                    .as_ref()
                    .ok_or_else(|| "missing int8 K/V scale buffer".to_string())?;
                enc.set_buffer(7, scale, 0);
            }
            enc.dispatch_threadgroups(
                (
                    config.tokens.div_ceil(attention_query_block),
                    attention_head_groups,
                    1,
                ),
                (attention_threads, 1, 1),
            );
        } else {
            enc.set_buffer(0, &q_cache, 0);
            enc.set_buffer(1, &k_cache, 0);
            enc.set_buffer(2, &v_cache, 0);
            enc.set_buffer(3, &q_out, 0);
            enc.set_buffer(4, &attn, 0);
            enc.set_bytes(5, &tokens_u32);
            enc.set_bytes(6, &q_rows_u32);
            enc.dispatch_threadgroups(
                (config.tokens, QWEN35_08B.attention_q_heads, 1),
                (attention_threads, 1, 1),
            );
        }
        if profile_stop == PrefillAttentionCoreProfileStop::Attention {
            enc.end();
            return cmd.commit_and_wait();
        }

        if let (Some(plan), Some(weight)) = (
            mps_attention_out_plan.as_ref(),
            mps_attention_out_weight_buffer.as_ref(),
        ) {
            enc.end();
            plan.encode(&cmd, &attn, weight, &out)?;
            return cmd.commit_and_wait();
        }

        enc.set_pipeline(&out_pso);
        enc.set_buffer(0, &attn, 0);
        enc.set_buffer(1, &o, 0);
        enc.set_buffer(2, &out, 0);
        enc.set_bytes(3, &tokens_u32);
        enc.set_bytes(4, &hidden_u32);
        enc.set_bytes(5, &row_tile_u32);
        enc.set_bytes(6, &attention_col_tile_u32);
        enc.set_bytes(7, &attention_col_tiles_u32);
        enc.dispatch_threadgroups(
            (
                hidden.div_ceil(config.row_tile),
                config.tokens.div_ceil(out_token_tile),
                1,
            ),
            (
                prefill_deltanet_out_threadgroup_threads(out_token_tile),
                1,
                1,
            ),
        );
        enc.end();
        cmd.commit_and_wait()
    };

    for _ in 0..config.warmup {
        run_once()?;
    }
    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        run_once()?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; hidden.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    if let Ok(path) = std::env::var("CTOX_QWEN35_ATTENTION_RAW_DUMP") {
        let mut raw = vec![0u16; config.tokens * attention_width];
        unsafe {
            attn.read(0, &mut raw);
        }
        let bytes = raw
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        std::fs::write(&path, bytes)
            .map_err(|err| format!("failed to write attention raw dump `{path}`: {err}"))?;
    }
    let checksum = first.iter().sum::<f32>();
    let project_groups = config.tokens.div_ceil(project_token_tile);
    let out_groups = config.tokens.div_ceil(out_token_tile);
    let packed_weight_bytes = (q_tiled.len() + k_tiled.len() + v_tiled.len() + o_tiled.len()) * 2;
    let attention_stream_bytes = if split_attention {
        let q_blocks = config.tokens.div_ceil(attention_query_block);
        let mut active_qblock_key_visits = 0usize;
        for q_block in 0..q_blocks {
            let query_start = q_block * attention_query_block;
            let last_query = (query_start + attention_query_block - 1).min(config.tokens - 1);
            let active_key_blocks = last_query / partial_key_block + 1;
            active_qblock_key_visits += active_key_blocks * partial_key_block;
        }
        let stage1_head_groups = if qh4_splitk {
            QWEN35_08B.attention_q_heads / 4
        } else {
            QWEN35_08B.attention_q_heads
        };
        let stage1_kv =
            active_qblock_key_visits * stage1_head_groups * QWEN35_08B.attention_head_dim * 2 * 2;
        let mut active_query_key_blocks = 0usize;
        for query in 0..config.tokens {
            active_query_key_blocks += query / partial_key_block + 1;
        }
        let active_partials = active_query_key_blocks * QWEN35_08B.attention_q_heads;
        let partial_scalar_bytes = active_partials * std::mem::size_of::<f32>() * 2;
        let partial_acc_bytes =
            active_partials * QWEN35_08B.attention_head_dim * std::mem::size_of::<f32>() * 2;
        let combine_bytes = active_partials
            * (std::mem::size_of::<f32>() * 2
                + QWEN35_08B.attention_head_dim * std::mem::size_of::<f32>());
        stage1_kv + partial_scalar_bytes + partial_acc_bytes + combine_bytes
    } else if attention_query_block > 1 {
        let block_count = config.tokens.div_ceil(attention_query_block);
        let mut bytes = 0usize;
        for block in 0..block_count {
            let query_start = block * attention_query_block;
            let last_query = (query_start + attention_query_block - 1).min(config.tokens - 1);
            bytes += (last_query + 1)
                * QWEN35_08B.attention_q_heads
                * QWEN35_08B.attention_head_dim
                * 2
                * 2;
        }
        if qh2_qblk4_simdreduce_batch {
            bytes /= 2;
        } else if qh4_qblk2_simdreduce_batch || qh4_qblk2_simd32_vec8 {
            bytes /= 4;
        }
        bytes
    } else if qh4_simd32_vec8_window_halfdot_enabled
        || qh4_simd32_vec8_window_enabled
        || qh4_simd32_vec8_win4096
    {
        let window = if qh4_simd32_vec8_window_halfdot_enabled {
            qh4_simd32_vec8_window_halfdot_u32 as usize
        } else if qh4_simd32_vec8_window_enabled {
            qh4_simd32_vec8_window_u32 as usize
        } else {
            4096usize
        };
        let full_window_queries = config.tokens.saturating_sub(window);
        let ramp_queries = config.tokens.min(window);
        let key_visits = ramp_queries * (ramp_queries + 1) / 2 + full_window_queries * window;
        key_visits * (QWEN35_08B.attention_q_heads / 4) * QWEN35_08B.attention_head_dim * 2 * 2
    } else if qh4_simd32_vec8_int8_kv {
        config.tokens * (config.tokens + 1) / 2
            * (QWEN35_08B.attention_q_heads / 4)
            * (QWEN35_08B.attention_head_dim * 2 + std::mem::size_of::<u16>())
    } else if qh4_simd32_vec8_int8_v || qh4_simd32_vec8_int8_v_pack4 {
        config.tokens * (config.tokens + 1) / 2
            * (QWEN35_08B.attention_q_heads / 4)
            * (QWEN35_08B.attention_head_dim * 3 + std::mem::size_of::<u16>())
    } else if qh4_simd32_vec8
        || qh4_simd32_vec8_halfacc
        || qh4_simd32_vec8_halfdot
        || qh4_simd32_vec8_interleaved_kv
        || qh4_qblk1_simdreduce_batch
    {
        config.tokens * (config.tokens + 1) / 2
            * (QWEN35_08B.attention_q_heads / 4)
            * QWEN35_08B.attention_head_dim
            * 2
            * 2
    } else {
        config.tokens * (config.tokens + 1) / 2
            * QWEN35_08B.attention_q_heads
            * QWEN35_08B.attention_head_dim
            * 2
            * 2
    };
    let bytes_moved = project_groups * (q_tiled.len() + k_tiled.len() + v_tiled.len()) * 2
        + if mps_attention_out_weight.is_some() {
            o_tiled.len() * 2
                + config.tokens
                    * (attention_width * std::mem::size_of::<u16>()
                        + hidden * std::mem::size_of::<f32>())
        } else {
            out_groups * o_tiled.len() * 2
        }
        + attention_stream_bytes
        + config.tokens
            * (hidden * 2
                + hidden * 2
                + q_rows * 4
                + kv_rows * 4 * 2
                + attention_width * 2 * 2
                + kv_rows * 2 * 2
                + hidden * 4);
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillAttentionCoreBenchResult {
        tokens: config.tokens,
        hidden,
        q_rows,
        kv_rows,
        attention_width,
        row_tile: config.row_tile,
        project_token_tile,
        out_token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_prefill_deltanet_project_with_weights(
    config: PrefillDeltaProjectBenchConfig,
    x_host: &[u16],
    norm_host: &[u16],
    qkv_tiled: &[u16],
    z_tiled: &[u16],
    b_tiled: &[u16],
    a_tiled: &[u16],
) -> Result<PrefillDeltaProjectBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let qkv_rows = QWEN35_08B.deltanet_qkv_width();
    let z_rows = QWEN35_08B.deltanet_width();
    let gate_rows = QWEN35_08B.deltanet_v_heads;
    validate_tiled_matvec_shape(qkv_rows, hidden, config.row_tile, config.col_tile)?;
    validate_tiled_matvec_shape(z_rows, hidden, config.row_tile, config.col_tile)?;
    validate_tiled_matvec_shape(gate_rows, hidden, config.row_tile, config.col_tile)?;
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if norm_host.len() != hidden {
        return Err(format!(
            "norm_host length must be {hidden}, got {}",
            norm_host.len()
        ));
    }
    let qkv_weights =
        round_up_usize(qkv_rows, config.row_tile) * round_up_usize(hidden, config.col_tile);
    let z_weights =
        round_up_usize(z_rows, config.row_tile) * round_up_usize(hidden, config.col_tile);
    let gate_weights =
        round_up_usize(gate_rows, config.row_tile) * round_up_usize(hidden, config.col_tile);
    if qkv_tiled.len() != qkv_weights {
        return Err(format!(
            "qkv_tiled length must be {qkv_weights}, got {}",
            qkv_tiled.len()
        ));
    }
    if z_tiled.len() != z_weights {
        return Err(format!(
            "z_tiled length must be {z_weights}, got {}",
            z_tiled.len()
        ));
    }
    if b_tiled.len() != gate_weights {
        return Err(format!(
            "b_tiled length must be {gate_weights}, got {}",
            b_tiled.len()
        ));
    }
    if a_tiled.len() != gate_weights {
        return Err(format!(
            "a_tiled length must be {gate_weights}, got {}",
            a_tiled.len()
        ));
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv rows exceed u32")?;
    let z_rows_u32 = u32::try_from(z_rows).map_err(|_| "z rows exceed u32")?;
    let gate_rows_u32 = u32::try_from(gate_rows).map_err(|_| "gate rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let qkv = dev.new_buffer(qkv_tiled.len() * std::mem::size_of::<u16>())?;
    let z = dev.new_buffer(z_tiled.len() * std::mem::size_of::<u16>())?;
    let b = dev.new_buffer(b_tiled.len() * std::mem::size_of::<u16>())?;
    let a = dev.new_buffer(a_tiled.len() * std::mem::size_of::<u16>())?;
    let qkv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let z_out = dev.new_buffer(config.tokens * z_rows * std::mem::size_of::<f32>())?;
    let b_out = dev.new_buffer(config.tokens * gate_rows * std::mem::size_of::<f32>())?;
    let a_out = dev.new_buffer(config.tokens * gate_rows * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, x_host);
        norm.write(0, norm_host);
        qkv.write(0, qkv_tiled);
        z.write(0, z_tiled);
        b.write(0, b_tiled);
        a.write(0, a_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_deltanet_project_once(
            &dev,
            &x,
            &norm,
            &qkv,
            &z,
            &b,
            &a,
            &qkv_out,
            &z_out,
            &b_out,
            &a_out,
            tokens_u32,
            qkv_rows_u32,
            z_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_deltanet_project_once(
            &dev,
            &x,
            &norm,
            &qkv,
            &z,
            &b,
            &a,
            &qkv_out,
            &z_out,
            &b_out,
            &a_out,
            tokens_u32,
            qkv_rows_u32,
            z_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; qkv_rows.min(16)];
    unsafe {
        qkv_out.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let (_, token_tile) = prefill_rms_matmul_kernel();
    let token_groups = config.tokens.div_ceil(token_tile);
    let packed_weight_bytes = (qkv_tiled.len() + z_tiled.len() + b_tiled.len() + a_tiled.len())
        * std::mem::size_of::<u16>();
    let out_rows = qkv_rows + z_rows + gate_rows + gate_rows;
    let bytes_moved = token_groups * packed_weight_bytes
        + 4 * config.tokens * (hidden * 2 + hidden * 2)
        + config.tokens * out_rows * std::mem::size_of::<f32>();
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDeltaProjectBenchResult {
        tokens: config.tokens,
        hidden,
        qkv_rows,
        z_rows,
        gate_rows,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_deltanet_conv_with_weights(
    config: PrefillDeltaConvBenchConfig,
    x_host: &[f32],
    conv_weight: &[u16],
    conv_bias: &[u16],
) -> Result<PrefillDeltaConvBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let channels = QWEN35_08B.deltanet_qkv_width();
    let kernel_width = 4usize;
    if x_host.len() != config.tokens * channels {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * channels,
            x_host.len()
        ));
    }
    if conv_weight.len() != channels * kernel_width {
        return Err(format!(
            "conv_weight length must be {}, got {}",
            channels * kernel_width,
            conv_weight.len()
        ));
    }
    if conv_bias.len() != channels {
        return Err(format!(
            "conv_bias length must be {channels}, got {}",
            conv_bias.len()
        ));
    }
    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * channels * std::mem::size_of::<f32>())?;
    let state = dev.new_buffer(3 * channels * std::mem::size_of::<u16>())?;
    let weight = dev.new_buffer(conv_weight.len() * std::mem::size_of::<u16>())?;
    let bias = dev.new_buffer(conv_bias.len() * std::mem::size_of::<u16>())?;
    let out = dev.new_buffer(config.tokens * channels * std::mem::size_of::<f32>())?;
    let state_host = vec![0u16; 3 * channels];
    unsafe {
        x.write(0, x_host);
        state.write(0, &state_host);
        weight.write(0, conv_weight);
        bias.write(0, conv_bias);
    }

    for _ in 0..config.warmup {
        unsafe {
            state.write(0, &state_host);
        }
        dispatch_prefill_deltanet_conv_once(&dev, &x, &state, &weight, &bias, &out, tokens_u32)?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        unsafe {
            state.write(0, &state_host);
        }
        let start = Instant::now();
        dispatch_prefill_deltanet_conv_once(&dev, &x, &state, &weight, &bias, &out, tokens_u32)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; channels.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let bytes_moved = config.tokens * channels * (std::mem::size_of::<f32>() * 2)
        + channels * kernel_width * std::mem::size_of::<u16>()
        + channels * std::mem::size_of::<u16>()
        + 3 * channels * std::mem::size_of::<u16>() * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDeltaConvBenchResult {
        tokens: config.tokens,
        channels,
        kernel_width,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_deltanet_prepare_with_weights(
    config: PrefillDeltaPrepareBenchConfig,
    qkv_host: &[f32],
    beta_raw_host: &[f32],
    alpha_raw_host: &[f32],
    a_log_host: &[f32],
    dt_bias_host: &[f32],
) -> Result<PrefillDeltaPrepareBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let width = QWEN35_08B.deltanet_width();
    let qkv_width = QWEN35_08B.deltanet_qkv_width();
    if qkv_host.len() != config.tokens * qkv_width {
        return Err(format!(
            "qkv_host length must be {}, got {}",
            config.tokens * qkv_width,
            qkv_host.len()
        ));
    }
    if beta_raw_host.len() != config.tokens * heads {
        return Err(format!(
            "beta_raw_host length must be {}, got {}",
            config.tokens * heads,
            beta_raw_host.len()
        ));
    }
    if alpha_raw_host.len() != config.tokens * heads {
        return Err(format!(
            "alpha_raw_host length must be {}, got {}",
            config.tokens * heads,
            alpha_raw_host.len()
        ));
    }
    if a_log_host.len() != heads || dt_bias_host.len() != heads {
        return Err("a_log/dt_bias length must match DeltaNet heads".to_string());
    }
    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;

    let dev = Device::default_system()?;
    let qkv = dev.new_buffer(config.tokens * qkv_width * std::mem::size_of::<f32>())?;
    let beta_raw = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let alpha_raw = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let a_log = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let dt_bias = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let q = dev.new_buffer(config.tokens * width * std::mem::size_of::<u16>())?;
    let k = dev.new_buffer(config.tokens * width * std::mem::size_of::<u16>())?;
    let v = dev.new_buffer(config.tokens * width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    unsafe {
        qkv.write(0, qkv_host);
        beta_raw.write(0, beta_raw_host);
        alpha_raw.write(0, alpha_raw_host);
        a_log.write(0, a_log_host);
        dt_bias.write(0, dt_bias_host);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_deltanet_prepare_once(
            &dev, &qkv, &beta_raw, &alpha_raw, &a_log, &dt_bias, &q, &k, &v, &beta, &decay,
            tokens_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_deltanet_prepare_once(
            &dev, &qkv, &beta_raw, &alpha_raw, &a_log, &dt_bias, &q, &k, &v, &beta, &decay,
            tokens_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first_q = vec![0u16; head_dim.min(16)];
    let mut first_beta = vec![0.0f32; heads.min(16)];
    unsafe {
        q.read(0, &mut first_q);
        beta.read(0, &mut first_beta);
    }
    let checksum = first_q
        .iter()
        .map(|v| f16::from_bits(*v).to_f32())
        .chain(first_beta.iter().copied())
        .fold(0.0f32, |acc, v| acc + v);
    let bytes_moved = config.tokens * qkv_width * std::mem::size_of::<f32>()
        + config.tokens * width * std::mem::size_of::<u16>() * 3
        + config.tokens * heads * std::mem::size_of::<f32>() * 4
        + heads * std::mem::size_of::<f32>() * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDeltaPrepareBenchResult {
        tokens: config.tokens,
        heads,
        head_dim,
        qkv_width,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_deltanet_scan_with_inputs(
    config: PrefillDeltaScanBenchConfig,
    q_host: &[u16],
    k_host: &[u16],
    v_host: &[u16],
    beta_host: &[f32],
    decay_host: &[f32],
    state_host: &[f32],
) -> Result<PrefillDeltaScanBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let width = QWEN35_08B.deltanet_width();
    let vec_elems = config.tokens * width;
    let state_elems = heads * head_dim * head_dim;
    let state_bytes = state_elems * std::mem::size_of::<f32>();
    if q_host.len() != vec_elems || k_host.len() != vec_elems || v_host.len() != vec_elems {
        return Err(format!(
            "q/k/v length must be {vec_elems}, got q={} k={} v={}",
            q_host.len(),
            k_host.len(),
            v_host.len()
        ));
    }
    if beta_host.len() != config.tokens * heads || decay_host.len() != config.tokens * heads {
        return Err(format!(
            "beta/decay length must be {}, got beta={} decay={}",
            config.tokens * heads,
            beta_host.len(),
            decay_host.len()
        ));
    }
    if state_host.len() != state_elems {
        return Err(format!(
            "state length must be {state_elems}, got {}",
            state_host.len()
        ));
    }
    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let kernel_name = prefill_delta_scan_kernel_for_tokens(config.tokens);
    let (grid, threads) = dispatch_prefill_delta_scan_shape_for_tokens(config.tokens);

    let dev = Device::default_system()?;
    let q = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let k = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let v = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let state = dev.new_buffer(state_bytes)?;
    let out = dev.new_buffer(vec_elems * std::mem::size_of::<f32>())?;
    unsafe {
        q.write(0, q_host);
        k.write(0, k_host);
        v.write(0, v_host);
        beta.write(0, beta_host);
        decay.write(0, decay_host);
    }

    for _ in 0..config.warmup {
        unsafe {
            state.write(0, state_host);
        }
        dispatch_prefill_deltanet_scan_once(
            &dev, &q, &k, &v, &beta, &decay, &state, &out, tokens_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        unsafe {
            state.write(0, state_host);
        }
        let start = Instant::now();
        dispatch_prefill_deltanet_scan_once(
            &dev, &q, &k, &v, &beta, &decay, &state, &out, tokens_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let validate_tokens = config.validate_tokens.min(config.tokens);
    let (max_abs_error_out, max_abs_error_state, checksum) = if validate_tokens > 0 {
        unsafe {
            state.write(0, state_host);
        }
        let validate_tokens_u32 =
            u32::try_from(validate_tokens).map_err(|_| "validate_tokens exceed u32")?;
        dispatch_prefill_deltanet_scan_once(
            &dev,
            &q,
            &k,
            &v,
            &beta,
            &decay,
            &state,
            &out,
            validate_tokens_u32,
        )?;
        let mut out_host = vec![0.0f32; validate_tokens * width];
        let mut state_out_host = vec![0.0f32; state_elems];
        unsafe {
            out.read(0, &mut out_host);
            state.read(0, &mut state_out_host);
        }
        let (out_ref, state_ref) = cpu_deltanet_scan_once(
            heads,
            head_dim,
            validate_tokens,
            q_host,
            k_host,
            v_host,
            beta_host,
            decay_host,
            state_host,
        );
        let checksum = out_host.iter().take(32).fold(0.0f32, |acc, v| acc + *v);
        (
            max_abs_error(&out_host, &out_ref),
            max_abs_error(&state_out_host, &state_ref),
            checksum,
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    let state_stream_bytes =
        prefill_delta_scan_state_stream_bytes(config.tokens, heads, head_dim);
    let per_token_stream_bytes = width * std::mem::size_of::<u16>() * 3
        + width * std::mem::size_of::<f32>()
        + heads * std::mem::size_of::<f32>() * 2;
    let bytes_moved_estimate = state_stream_bytes + per_token_stream_bytes * config.tokens;
    let effective_gb_s = bytes_moved_estimate as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDeltaScanBenchResult {
        kernel_name,
        grid,
        threads,
        tokens: config.tokens,
        heads,
        head_dim,
        state_bytes,
        bytes_moved_estimate,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        max_abs_error_out,
        max_abs_error_state,
        checksum,
    })
}

pub fn run_prefill_deltanet_out_block_with_weights(
    config: PrefillDeltaOutBlockBenchConfig,
    delta_host: &[f32],
    z_host: &[f32],
    norm_host: &[f32],
    out_tiled: &[u16],
) -> Result<PrefillDeltaOutBlockBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let rows = QWEN35_08B.hidden_size;
    let cols = QWEN35_08B.deltanet_width();
    let norm_cols = QWEN35_08B.deltanet_head_dim;
    validate_tiled_matvec_shape(rows, cols, config.row_tile, config.col_tile)?;
    if delta_host.len() != config.tokens * cols || z_host.len() != config.tokens * cols {
        return Err(format!(
            "delta/z length must be {}, got delta={} z={}",
            config.tokens * cols,
            delta_host.len(),
            z_host.len()
        ));
    }
    if norm_host.len() != norm_cols {
        return Err(format!(
            "norm_host length must be {norm_cols}, got {}",
            norm_host.len()
        ));
    }
    let expected_weights =
        round_up_usize(rows, config.row_tile) * round_up_usize(cols, config.col_tile);
    if out_tiled.len() != expected_weights {
        return Err(format!(
            "out_tiled length must be {expected_weights}, got {}",
            out_tiled.len()
        ));
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(config.col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let delta = dev.new_buffer(config.tokens * cols * std::mem::size_of::<f32>())?;
    let z = dev.new_buffer(config.tokens * cols * std::mem::size_of::<f32>())?;
    let norm = dev.new_buffer(norm_cols * std::mem::size_of::<f32>())?;
    let out_w = dev.new_buffer(out_tiled.len() * std::mem::size_of::<u16>())?;
    let gated = dev.new_buffer(config.tokens * cols * std::mem::size_of::<u16>())?;
    let out = dev.new_buffer(config.tokens * rows * std::mem::size_of::<f32>())?;
    unsafe {
        delta.write(0, delta_host);
        z.write(0, z_host);
        norm.write(0, norm_host);
        out_w.write(0, out_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_prefill_deltanet_out_block_once(
            &dev,
            &delta,
            &z,
            &norm,
            &gated,
            &out_w,
            &out,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_prefill_deltanet_out_block_once(
            &dev,
            &delta,
            &z,
            &norm,
            &gated,
            &out_w,
            &out,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let checksum_sparse = sparse_f32_matrix_checksum(&out, config.tokens, rows);
    let (_, token_tile) = prefill_deltanet_out_matmul_kernel();
    let token_groups = config.tokens.div_ceil(token_tile);
    let packed_weight_bytes = out_tiled.len() * std::mem::size_of::<u16>();
    let norm_bytes = config.tokens
        * (cols * std::mem::size_of::<f32>() * 2
            + cols * std::mem::size_of::<u16>()
            + norm_cols * std::mem::size_of::<f32>());
    let matmul_bytes = token_groups * packed_weight_bytes
        + config.tokens * (cols * std::mem::size_of::<u16>() + rows * std::mem::size_of::<f32>());
    let effective_gb_s = (norm_bytes + matmul_bytes) as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDeltaOutBlockBenchResult {
        tokens: config.tokens,
        rows,
        cols,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
        checksum_sparse,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_prefill_deltanet_block_with_weights(
    config: PrefillDeltaBlockBenchConfig,
    x_host: &[u16],
    input_norm_host: &[u16],
    qkv_tiled: &[u16],
    z_tiled: &[u16],
    b_tiled: &[u16],
    a_tiled: &[u16],
    conv_weight_host: &[u16],
    conv_bias_host: &[u16],
    a_log_host: &[f32],
    dt_bias_host: &[f32],
    delta_norm_host: &[f32],
    out_tiled: &[u16],
) -> Result<PrefillDeltaBlockBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let delta_width = QWEN35_08B.deltanet_width();
    let qkv_rows = QWEN35_08B.deltanet_qkv_width();
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    validate_tiled_matvec_shape(qkv_rows, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(heads, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(hidden, delta_width, config.row_tile, config.out_col_tile)?;
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if input_norm_host.len() != hidden {
        return Err(format!(
            "input_norm_host length must be {hidden}, got {}",
            input_norm_host.len()
        ));
    }
    let qkv_weights =
        round_up_usize(qkv_rows, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let z_weights = round_up_usize(delta_width, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let gate_weights =
        round_up_usize(heads, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let out_weights =
        round_up_usize(hidden, config.row_tile) * round_up_usize(delta_width, config.out_col_tile);
    if qkv_tiled.len() != qkv_weights {
        return Err(format!(
            "qkv_tiled length must be {qkv_weights}, got {}",
            qkv_tiled.len()
        ));
    }
    if z_tiled.len() != z_weights {
        return Err(format!(
            "z_tiled length must be {z_weights}, got {}",
            z_tiled.len()
        ));
    }
    if b_tiled.len() != gate_weights || a_tiled.len() != gate_weights {
        return Err(format!(
            "b/a length must be {gate_weights}, got b={} a={}",
            b_tiled.len(),
            a_tiled.len()
        ));
    }
    if conv_weight_host.len() != qkv_rows * 4 || conv_bias_host.len() != qkv_rows {
        return Err(format!(
            "conv weight/bias lengths must be {} and {}, got {} and {}",
            qkv_rows * 4,
            qkv_rows,
            conv_weight_host.len(),
            conv_bias_host.len()
        ));
    }
    if a_log_host.len() != heads || dt_bias_host.len() != heads {
        return Err("a_log/dt_bias length must match DeltaNet heads".to_string());
    }
    if delta_norm_host.len() != head_dim {
        return Err(format!(
            "delta_norm length must be {head_dim}, got {}",
            delta_norm_host.len()
        ));
    }
    if out_tiled.len() != out_weights {
        return Err(format!(
            "out_tiled length must be {out_weights}, got {}",
            out_tiled.len()
        ));
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let hidden_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv_rows exceed u32")?;
    let delta_width_u32 = u32::try_from(delta_width).map_err(|_| "delta_width exceed u32")?;
    let heads_u32 = u32::try_from(heads).map_err(|_| "heads exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let out_col_tile_u32 =
        u32::try_from(config.out_col_tile).map_err(|_| "out_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;
    let out_col_tiles_u32 = u32::try_from(delta_width.div_ceil(config.out_col_tile))
        .map_err(|_| "out col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let input_norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let qkv = dev.new_buffer(qkv_tiled.len() * std::mem::size_of::<u16>())?;
    let z = dev.new_buffer(z_tiled.len() * std::mem::size_of::<u16>())?;
    let b = dev.new_buffer(b_tiled.len() * std::mem::size_of::<u16>())?;
    let a = dev.new_buffer(a_tiled.len() * std::mem::size_of::<u16>())?;
    let conv_weight = dev.new_buffer(conv_weight_host.len() * std::mem::size_of::<u16>())?;
    let conv_bias = dev.new_buffer(conv_bias_host.len() * std::mem::size_of::<u16>())?;
    let a_log = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let dt_bias = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let delta_norm = dev.new_buffer(head_dim * std::mem::size_of::<f32>())?;
    let out_w = dev.new_buffer(out_tiled.len() * std::mem::size_of::<u16>())?;
    let qkv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let conv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let z_out = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let b_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let a_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let conv_state = dev.new_buffer(3 * qkv_rows * std::mem::size_of::<u16>())?;
    let recurrent_state =
        dev.new_buffer(heads * head_dim * head_dim * std::mem::size_of::<f32>())?;
    let delta = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let gated = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let conv_state_zero = vec![0u16; 3 * qkv_rows];
    let recurrent_state_zero = vec![0.0f32; heads * head_dim * head_dim];
    unsafe {
        x.write(0, x_host);
        input_norm.write(0, input_norm_host);
        qkv.write(0, qkv_tiled);
        z.write(0, z_tiled);
        b.write(0, b_tiled);
        a.write(0, a_tiled);
        conv_weight.write(0, conv_weight_host);
        conv_bias.write(0, conv_bias_host);
        a_log.write(0, a_log_host);
        dt_bias.write(0, dt_bias_host);
        delta_norm.write(0, delta_norm_host);
        out_w.write(0, out_tiled);
    }

    for _ in 0..config.warmup {
        unsafe {
            conv_state.write(0, &conv_state_zero);
            recurrent_state.write(0, &recurrent_state_zero);
        }
        dispatch_prefill_deltanet_block_once(
            &dev,
            &x,
            &input_norm,
            &qkv,
            &z,
            &b,
            &a,
            &qkv_out,
            &z_out,
            &b_out,
            &a_out,
            &conv_state,
            &conv_weight,
            &conv_bias,
            &conv_out,
            &a_log,
            &dt_bias,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &decay,
            &recurrent_state,
            &delta,
            &delta_norm,
            &gated,
            &out_w,
            &out,
            tokens_u32,
            hidden_u32,
            qkv_rows_u32,
            delta_width_u32,
            heads_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            out_col_tile_u32,
            hidden_col_tiles_u32,
            out_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        unsafe {
            conv_state.write(0, &conv_state_zero);
            recurrent_state.write(0, &recurrent_state_zero);
        }
        let start = Instant::now();
        dispatch_prefill_deltanet_block_once(
            &dev,
            &x,
            &input_norm,
            &qkv,
            &z,
            &b,
            &a,
            &qkv_out,
            &z_out,
            &b_out,
            &a_out,
            &conv_state,
            &conv_weight,
            &conv_bias,
            &conv_out,
            &a_log,
            &dt_bias,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &decay,
            &recurrent_state,
            &delta,
            &delta_norm,
            &gated,
            &out_w,
            &out,
            tokens_u32,
            hidden_u32,
            qkv_rows_u32,
            delta_width_u32,
            heads_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            out_col_tile_u32,
            hidden_col_tiles_u32,
            out_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; hidden.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let (_, token_tile) = prefill_rms_matmul_kernel();
    let token_groups = config.tokens.div_ceil(token_tile);
    let (_, out_token_tile) = prefill_deltanet_out_matmul_kernel();
    let out_token_groups = config.tokens.div_ceil(out_token_tile);
    let packed_weight_bytes =
        (qkv_tiled.len() + z_tiled.len() + b_tiled.len() + a_tiled.len() + out_tiled.len())
            * std::mem::size_of::<u16>();
    let projection_bytes = token_groups
        * (qkv_tiled.len() + z_tiled.len() + b_tiled.len() + a_tiled.len())
        * std::mem::size_of::<u16>();
    let conv_bytes = config.tokens * qkv_rows * (std::mem::size_of::<f32>() * 2 + 8);
    let scan_bytes = prefill_delta_scan_state_stream_bytes(config.tokens, heads, head_dim);
    let out_bytes = out_token_groups * out_tiled.len() * std::mem::size_of::<u16>();
    let effective_gb_s =
        (projection_bytes + conv_bytes + scan_bytes + out_bytes) as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDeltaBlockBenchResult {
        tokens: config.tokens,
        hidden,
        delta_width,
        qkv_rows,
        row_tile: config.row_tile,
        token_tile,
        out_token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_prefill_delta_ffn_block_with_weights(
    config: PrefillDeltaFfnBlockBenchConfig,
    x_host: &[u16],
    input_norm_host: &[u16],
    qkv_tiled: &[u16],
    z_tiled: &[u16],
    b_tiled: &[u16],
    a_tiled: &[u16],
    conv_weight_host: &[u16],
    conv_bias_host: &[u16],
    a_log_host: &[f32],
    dt_bias_host: &[f32],
    delta_norm_host: &[f32],
    delta_out_tiled: &[u16],
    ffn_norm_host: &[u16],
    ffn_gate_tiled: &[u16],
    ffn_up_tiled: &[u16],
    ffn_down_tiled: &[u16],
) -> Result<PrefillDeltaFfnBlockBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let delta_width = QWEN35_08B.deltanet_width();
    let qkv_rows = QWEN35_08B.deltanet_qkv_width();
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let intermediate = QWEN35_08B.ffn_intermediate;

    validate_tiled_matvec_shape(qkv_rows, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(heads, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(
        hidden,
        delta_width,
        config.row_tile,
        config.delta_out_col_tile,
    )?;
    validate_tiled_matvec_shape(
        intermediate,
        hidden,
        config.row_tile,
        config.hidden_col_tile,
    )?;
    validate_tiled_matvec_shape(
        hidden,
        intermediate,
        config.row_tile,
        config.intermediate_col_tile,
    )?;
    if prefill_ffn_gate_up_mma_enabled() {
        let (_, gate_up_token_tile) = prefill_ffn_gate_up_mma_kernel();
        validate_mma_token_tile(
            "Delta+FFN GateUp MMA",
            gate_up_token_tile,
            config.tokens,
            config.row_tile,
        )?;
    }
    if prefill_down_mma_enabled() {
        let (_, down_token_tile) = prefill_down_matmul_kernel();
        validate_mma_token_tile(
            "Delta+FFN Down MMA",
            down_token_tile,
            config.tokens,
            config.row_tile,
        )?;
    }
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }
    if input_norm_host.len() != hidden || ffn_norm_host.len() != hidden {
        return Err("input_norm/post_norm length must match hidden size".to_string());
    }
    if conv_weight_host.len() != qkv_rows * 4 || conv_bias_host.len() != qkv_rows {
        return Err("conv weight/bias lengths do not match DeltaNet shape".to_string());
    }
    if a_log_host.len() != heads || dt_bias_host.len() != heads {
        return Err("a_log/dt_bias length must match DeltaNet heads".to_string());
    }
    if delta_norm_host.len() != head_dim {
        return Err("delta_norm length must match DeltaNet head dim".to_string());
    }

    let qkv_weights =
        round_up_usize(qkv_rows, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let delta_weights = round_up_usize(delta_width, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let gate_weights =
        round_up_usize(heads, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let delta_out_weights = round_up_usize(hidden, config.row_tile)
        * round_up_usize(delta_width, config.delta_out_col_tile);
    let ffn_gate_weights = round_up_usize(intermediate, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let ffn_down_weights = round_up_usize(hidden, config.row_tile)
        * round_up_usize(intermediate, config.intermediate_col_tile);
    if qkv_tiled.len() != qkv_weights
        || z_tiled.len() != delta_weights
        || b_tiled.len() != gate_weights
        || a_tiled.len() != gate_weights
        || delta_out_tiled.len() != delta_out_weights
        || ffn_gate_tiled.len() != ffn_gate_weights
        || ffn_up_tiled.len() != ffn_gate_weights
        || ffn_down_tiled.len() != ffn_down_weights
    {
        return Err(
            "one or more tiled weight buffers do not match expected packed shape".to_string(),
        );
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let hidden_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv_rows exceed u32")?;
    let delta_width_u32 = u32::try_from(delta_width).map_err(|_| "delta_width exceed u32")?;
    let heads_u32 = u32::try_from(heads).map_err(|_| "heads exceed u32")?;
    let intermediate_u32 = u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let delta_out_col_tile_u32 =
        u32::try_from(config.delta_out_col_tile).map_err(|_| "delta_out_col_tile exceeds u32")?;
    let intermediate_col_tile_u32 = u32::try_from(config.intermediate_col_tile)
        .map_err(|_| "intermediate_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;
    let delta_out_col_tiles_u32 = u32::try_from(delta_width.div_ceil(config.delta_out_col_tile))
        .map_err(|_| "delta out col tiles exceed u32")?;
    let intermediate_col_tiles_u32 =
        u32::try_from(intermediate.div_ceil(config.intermediate_col_tile))
            .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let input_norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let qkv = new_readonly_buffer(&dev, qkv_tiled)?;
    let z = new_readonly_buffer(&dev, z_tiled)?;
    let b = new_readonly_buffer(&dev, b_tiled)?;
    let a = new_readonly_buffer(&dev, a_tiled)?;
    let conv_weight = new_readonly_buffer(&dev, conv_weight_host)?;
    let conv_bias = new_readonly_buffer(&dev, conv_bias_host)?;
    let a_log = new_readonly_buffer(&dev, a_log_host)?;
    let dt_bias = new_readonly_buffer(&dev, dt_bias_host)?;
    let delta_norm = new_readonly_buffer(&dev, delta_norm_host)?;
    let delta_out_w = new_readonly_buffer(&dev, delta_out_tiled)?;
    let ffn_norm = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let ffn_gate = new_readonly_buffer(&dev, ffn_gate_tiled)?;
    let ffn_up = new_readonly_buffer(&dev, ffn_up_tiled)?;
    let ffn_down = new_readonly_buffer(&dev, ffn_down_tiled)?;

    let qkv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let conv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let z_out = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let b_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let a_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let conv_state = dev.new_buffer(3 * qkv_rows * std::mem::size_of::<u16>())?;
    let recurrent_state =
        dev.new_buffer(heads * head_dim * head_dim * std::mem::size_of::<f32>())?;
    let delta = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let gated = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let delta_out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let after_delta = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let ffn_act = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    let ffn_out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let after_ffn = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;

    let conv_state_zero = vec![0u16; 3 * qkv_rows];
    let recurrent_state_zero = vec![0.0f32; heads * head_dim * head_dim];
    unsafe {
        x.write(0, x_host);
        input_norm.write(0, input_norm_host);
        ffn_norm.write(0, ffn_norm_host);
    }

    for _ in 0..config.warmup {
        unsafe {
            conv_state.write(0, &conv_state_zero);
            recurrent_state.write(0, &recurrent_state_zero);
        }
        dispatch_prefill_delta_ffn_block_once(
            &dev,
            &x,
            &input_norm,
            &qkv,
            &z,
            &b,
            &a,
            &qkv_out,
            &z_out,
            &b_out,
            &a_out,
            &conv_state,
            &conv_weight,
            &conv_bias,
            &conv_out,
            &a_log,
            &dt_bias,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &decay,
            &recurrent_state,
            &delta,
            &delta_norm,
            &gated,
            &delta_out_w,
            &delta_out,
            &after_delta,
            &ffn_norm,
            &ffn_gate,
            &ffn_up,
            &ffn_down,
            &ffn_act,
            &ffn_out,
            &after_ffn,
            tokens_u32,
            hidden_u32,
            qkv_rows_u32,
            delta_width_u32,
            heads_u32,
            intermediate_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            delta_out_col_tile_u32,
            intermediate_col_tile_u32,
            hidden_col_tiles_u32,
            delta_out_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        unsafe {
            conv_state.write(0, &conv_state_zero);
            recurrent_state.write(0, &recurrent_state_zero);
        }
        let start = Instant::now();
        dispatch_prefill_delta_ffn_block_once(
            &dev,
            &x,
            &input_norm,
            &qkv,
            &z,
            &b,
            &a,
            &qkv_out,
            &z_out,
            &b_out,
            &a_out,
            &conv_state,
            &conv_weight,
            &conv_bias,
            &conv_out,
            &a_log,
            &dt_bias,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &decay,
            &recurrent_state,
            &delta,
            &delta_norm,
            &gated,
            &delta_out_w,
            &delta_out,
            &after_delta,
            &ffn_norm,
            &ffn_gate,
            &ffn_up,
            &ffn_down,
            &ffn_act,
            &ffn_out,
            &after_ffn,
            tokens_u32,
            hidden_u32,
            qkv_rows_u32,
            delta_width_u32,
            heads_u32,
            intermediate_u32,
            row_tile_u32,
            hidden_col_tile_u32,
            delta_out_col_tile_u32,
            intermediate_col_tile_u32,
            hidden_col_tiles_u32,
            delta_out_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0u16; hidden.min(16)];
    unsafe {
        after_ffn.read(0, &mut first);
    }
    let checksum = first
        .iter()
        .map(|v| f16::from_bits(*v).to_f32())
        .fold(0.0f32, |acc, v| acc + v);

    let (_, project_token_tile) = prefill_project_matmul_kernel();
    let (_, out_token_tile) = prefill_deltanet_out_matmul_kernel();
    let ffn_token_tile = prefill_delta_ffn_gate_up_token_tile();
    let (_, down_token_tile) = prefill_down_matmul_kernel();
    let projection_groups = config.tokens.div_ceil(project_token_tile);
    let (_, delta_project_qkvz_mma_token_tile) =
        prefill_delta_project_qkvz_mma_kernel_for_tokens(config.tokens);
    let delta_project_qkvz_mma = prefill_project_split_norm_enabled()
        && prefill_delta_project_qkvz_mma_enabled()
        && config
            .tokens
            .is_multiple_of(delta_project_qkvz_mma_token_tile)
        && config.row_tile == 8;
    let qkvz_projection_groups = if delta_project_qkvz_mma {
        config.tokens.div_ceil(delta_project_qkvz_mma_token_tile)
    } else {
        projection_groups
    };
    let out_groups = config.tokens.div_ceil(out_token_tile);
    let ffn_groups = config.tokens.div_ceil(ffn_token_tile);
    let down_groups = config.tokens.div_ceil(down_token_tile);
    let packed_weight_bytes = (qkv_tiled.len()
        + z_tiled.len()
        + b_tiled.len()
        + a_tiled.len()
        + delta_out_tiled.len()
        + ffn_gate_tiled.len()
        + ffn_up_tiled.len()
        + ffn_down_tiled.len())
        * std::mem::size_of::<u16>();
    let bytes_moved =
        qkvz_projection_groups * (qkv_tiled.len() + z_tiled.len()) * std::mem::size_of::<u16>()
            + projection_groups * (b_tiled.len() + a_tiled.len()) * std::mem::size_of::<u16>()
            + config.tokens * qkv_rows * (std::mem::size_of::<f32>() * 2 + 8)
            + prefill_delta_scan_state_stream_bytes(config.tokens, heads, head_dim)
            + out_groups * delta_out_tiled.len() * std::mem::size_of::<u16>()
            + ffn_groups * (ffn_gate_tiled.len() + ffn_up_tiled.len()) * std::mem::size_of::<u16>()
            + down_groups * ffn_down_tiled.len() * std::mem::size_of::<u16>()
            + config.tokens * (hidden * 2 * 3 + hidden * 4 * 2 + intermediate * 2);
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDeltaFfnBlockBenchResult {
        tokens: config.tokens,
        hidden,
        delta_width,
        intermediate,
        qkv_rows,
        row_tile: config.row_tile,
        project_token_tile,
        qkvz_token_tile: if delta_project_qkvz_mma {
            delta_project_qkvz_mma_token_tile
        } else {
            project_token_tile
        },
        out_token_tile,
        ffn_token_tile,
        down_token_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_delta3_ffn_superblock_with_weights(
    config: PrefillDeltaFfnBlockBenchConfig,
    x_host: &[u16],
    layers: &[PrefillDeltaFfnLayerWeights<'_>; 3],
) -> Result<PrefillDelta3FfnSuperblockBenchResult, String> {
    run_prefill_delta_ffn_stack_with_weights(config, x_host, layers.as_slice())
}

pub fn run_prefill_delta_ffn_stack_with_weights(
    config: PrefillDeltaFfnBlockBenchConfig,
    x_host: &[u16],
    layers: &[PrefillDeltaFfnLayerWeights<'_>],
) -> Result<PrefillDelta3FfnSuperblockBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    if layers.is_empty() {
        return Err("prefill layer stack must not be empty".to_string());
    }
    let hidden = QWEN35_08B.hidden_size;
    let delta_width = QWEN35_08B.deltanet_width();
    let qkv_rows = QWEN35_08B.deltanet_qkv_width();
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(qkv_rows, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(heads, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(
        hidden,
        delta_width,
        config.row_tile,
        config.delta_out_col_tile,
    )?;
    validate_tiled_matvec_shape(
        intermediate,
        hidden,
        config.row_tile,
        config.hidden_col_tile,
    )?;
    validate_tiled_matvec_shape(
        hidden,
        intermediate,
        config.row_tile,
        config.intermediate_col_tile,
    )?;
    if prefill_ffn_gate_up_mma_enabled() {
        let (_, gate_up_token_tile) = prefill_ffn_gate_up_mma_kernel();
        validate_mma_token_tile(
            "Delta stack GateUp MMA",
            gate_up_token_tile,
            config.tokens,
            config.row_tile,
        )?;
    }
    if prefill_down_mma_enabled() {
        let (_, down_token_tile) = prefill_down_matmul_kernel();
        validate_mma_token_tile(
            "Delta stack Down MMA",
            down_token_tile,
            config.tokens,
            config.row_tile,
        )?;
    }
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }

    let qkv_weights =
        round_up_usize(qkv_rows, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let delta_weights = round_up_usize(delta_width, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let gate_weights =
        round_up_usize(heads, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let delta_out_weights = round_up_usize(hidden, config.row_tile)
        * round_up_usize(delta_width, config.delta_out_col_tile);
    let ffn_gate_weights = round_up_usize(intermediate, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let ffn_down_weights = round_up_usize(hidden, config.row_tile)
        * round_up_usize(intermediate, config.intermediate_col_tile);
    let mut packed_weight_elems = 0usize;
    for layer in layers {
        if layer.input_norm.len() != hidden
            || layer.ffn_norm.len() != hidden
            || layer.qkv.len() != qkv_weights
            || layer.z.len() != delta_weights
            || layer.b.len() != gate_weights
            || layer.a.len() != gate_weights
            || layer.conv_weight.len() != qkv_rows * 4
            || layer.conv_bias.len() != qkv_rows
            || layer.a_log.len() != heads
            || layer.dt_bias.len() != heads
            || layer.delta_norm.len() != head_dim
            || layer.delta_out.len() != delta_out_weights
            || layer.ffn_gate.len() != ffn_gate_weights
            || layer.ffn_up.len() != ffn_gate_weights
            || layer.ffn_down.len() != ffn_down_weights
        {
            return Err("one or more layer buffers do not match expected packed shape".to_string());
        }
        packed_weight_elems += layer.qkv.len()
            + layer.z.len()
            + layer.b.len()
            + layer.a.len()
            + layer.delta_out.len()
            + layer.ffn_gate.len()
            + layer.ffn_up.len()
            + layer.ffn_down.len();
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let hidden_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv_rows exceed u32")?;
    let delta_width_u32 = u32::try_from(delta_width).map_err(|_| "delta_width exceed u32")?;
    let heads_u32 = u32::try_from(heads).map_err(|_| "heads exceed u32")?;
    let intermediate_u32 = u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let delta_out_col_tile_u32 =
        u32::try_from(config.delta_out_col_tile).map_err(|_| "delta_out_col_tile exceeds u32")?;
    let intermediate_col_tile_u32 = u32::try_from(config.intermediate_col_tile)
        .map_err(|_| "intermediate_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;
    let delta_out_col_tiles_u32 = u32::try_from(delta_width.div_ceil(config.delta_out_col_tile))
        .map_err(|_| "delta out col tiles exceed u32")?;
    let intermediate_col_tiles_u32 =
        u32::try_from(intermediate.div_ceil(config.intermediate_col_tile))
            .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    unsafe {
        x.write(0, x_host);
    }
    let layer_buffers = layers
        .iter()
        .map(|layer| {
            Ok(PrefillDeltaFfnLayerDeviceBuffers {
                input_norm: new_readonly_buffer(&dev, layer.input_norm)?,
                qkv: new_readonly_buffer(&dev, layer.qkv)?,
                z: new_readonly_buffer(&dev, layer.z)?,
                b: new_readonly_buffer(&dev, layer.b)?,
                a: new_readonly_buffer(&dev, layer.a)?,
                conv_weight: new_readonly_buffer(&dev, layer.conv_weight)?,
                conv_bias: new_readonly_buffer(&dev, layer.conv_bias)?,
                a_log: new_readonly_buffer(&dev, layer.a_log)?,
                dt_bias: new_readonly_buffer(&dev, layer.dt_bias)?,
                delta_norm: new_readonly_buffer(&dev, layer.delta_norm)?,
                delta_out: new_readonly_buffer(&dev, layer.delta_out)?,
                ffn_norm: new_readonly_buffer(&dev, layer.ffn_norm)?,
                ffn_gate: new_readonly_buffer(&dev, layer.ffn_gate)?,
                ffn_up: new_readonly_buffer(&dev, layer.ffn_up)?,
                ffn_down: new_readonly_buffer(&dev, layer.ffn_down)?,
                conv_state: dev.new_buffer(3 * qkv_rows * std::mem::size_of::<u16>())?,
                recurrent_state: dev
                    .new_buffer(heads * head_dim * head_dim * std::mem::size_of::<f32>())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let hidden_a = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let hidden_b = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let qkv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let conv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let z_out = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let b_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let a_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let delta = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let chunk_scan_tokens = prefill_delta_scan_chunk_tokens();
    let chunk_scan_state_element_bytes = if prefill_delta_scan_chunk_hstate_enabled() {
        std::mem::size_of::<u16>()
    } else {
        std::mem::size_of::<f32>()
    };
    let chunk_scan_state = dev.new_buffer(
        config.tokens.div_ceil(chunk_scan_tokens)
            * heads
            * head_dim
            * head_dim
            * chunk_scan_state_element_bytes,
    )?;
    let gated = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let delta_out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let after_delta = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let ffn_act = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    let ffn_out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let conv_state_zero = vec![0u16; 3 * qkv_rows];
    let recurrent_state_zero = vec![0.0f32; heads * head_dim * head_dim];
    let profile_stop = PrefillDeltaFfnProfileStop::from_env();

    let run_once = |wait: bool| -> Result<bool, String> {
        for layer in &layer_buffers {
            unsafe {
                layer.conv_state.write(0, &conv_state_zero);
                layer.recurrent_state.write(0, &recurrent_state_zero);
            }
        }
        let cmd = dev.command_buffer()?;
        let enc = cmd.compute()?;
        for (idx, layer) in layer_buffers.iter().enumerate() {
            let input = if idx == 0 {
                &x
            } else if idx % 2 == 1 {
                &hidden_a
            } else {
                &hidden_b
            };
            let output = if idx % 2 == 0 { &hidden_a } else { &hidden_b };
            encode_prefill_delta_ffn_layer_ops(
                &dev,
                &enc,
                input,
                output,
                layer,
                &qkv_out,
                &z_out,
                &b_out,
                &a_out,
                &conv_out,
                &q_half,
                &k_half,
                &v_half,
                &beta,
                &decay,
                &delta,
                &chunk_scan_state,
                &gated,
                &delta_out,
                &after_delta,
                &ffn_act,
                &ffn_out,
                tokens_u32,
                hidden_u32,
                qkv_rows_u32,
                delta_width_u32,
                heads_u32,
                intermediate_u32,
                row_tile_u32,
                hidden_col_tile_u32,
                delta_out_col_tile_u32,
                intermediate_col_tile_u32,
                hidden_col_tiles_u32,
                delta_out_col_tiles_u32,
                intermediate_col_tiles_u32,
                profile_stop,
            )?;
        }
        enc.end();
        if wait {
            cmd.commit_and_wait()?;
        } else {
            cmd.commit();
        }
        Ok(true)
    };

    for _ in 0..config.warmup {
        run_once(true)?;
    }
    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        run_once(true)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0u16; hidden.min(16)];
    let final_hidden = if (layers.len() - 1) % 2 == 0 {
        &hidden_a
    } else {
        &hidden_b
    };
    unsafe {
        final_hidden.read(0, &mut first);
    }
    if let Ok(path) = std::env::var("CTOX_QWEN35_DELTA_STACK_FINAL_DUMP") {
        let mut raw = vec![0u16; config.tokens * hidden];
        unsafe {
            final_hidden.read(0, &mut raw);
        }
        let bytes = raw
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        std::fs::write(&path, bytes)
            .map_err(|err| format!("failed to write Delta stack final dump `{path}`: {err}"))?;
    }
    let checksum = first
        .iter()
        .map(|v| f16::from_bits(*v).to_f32())
        .fold(0.0f32, |acc, v| acc + v);

    let (_, project_token_tile) = prefill_project_matmul_kernel();
    let (_, out_token_tile) = prefill_deltanet_out_matmul_kernel();
    let ffn_token_tile = prefill_delta_ffn_gate_up_token_tile();
    let (_, down_token_tile) = prefill_down_matmul_kernel();
    let projection_groups = config.tokens.div_ceil(project_token_tile);
    let (_, delta_project_qkvz_mma_token_tile) =
        prefill_delta_project_qkvz_mma_kernel_for_tokens(config.tokens);
    let delta_project_qkvz_mma = prefill_project_split_norm_enabled()
        && prefill_delta_project_qkvz_mma_enabled()
        && config
            .tokens
            .is_multiple_of(delta_project_qkvz_mma_token_tile)
        && config.row_tile == 8;
    let qkvz_projection_groups = if delta_project_qkvz_mma {
        config.tokens.div_ceil(delta_project_qkvz_mma_token_tile)
    } else {
        projection_groups
    };
    let out_groups = config.tokens.div_ceil(out_token_tile);
    let ffn_groups = config.tokens.div_ceil(ffn_token_tile);
    let down_groups = config.tokens.div_ceil(down_token_tile);
    let delta_scan_gated_norm_saved_bytes = if prefill_delta_scan_gated_norm_enabled() {
        config.tokens * delta_width * std::mem::size_of::<f32>() * 2
    } else {
        0
    };
    let delta_out_residual_mma_saved_bytes =
        if prefill_deltanet_out_mma32_residual_enabled() && out_token_tile == 32 {
            config.tokens * hidden * std::mem::size_of::<f32>() * 2
        } else {
            0
        };
    let down_residual_mma_saved_bytes =
        if prefill_down_mma_residual_kernel(down_token_tile).is_some() {
            config.tokens * hidden * std::mem::size_of::<f32>() * 2
        } else {
            0
        };
    let residual_mma_saved_bytes =
        delta_out_residual_mma_saved_bytes + down_residual_mma_saved_bytes;
    let conv_split_fused_saved_bytes = if prefill_delta_conv_split_fused_enabled() {
        config.tokens * qkv_rows * std::mem::size_of::<f32>() * 2
    } else {
        0
    };
    let scan_state_stream_bytes =
        prefill_delta_scan_state_stream_bytes(config.tokens, heads, head_dim);
    let per_layer_bytes_before_fusion =
        qkvz_projection_groups * (qkv_weights + delta_weights) * std::mem::size_of::<u16>()
            + projection_groups * (gate_weights + gate_weights) * std::mem::size_of::<u16>()
            + config.tokens * qkv_rows * (std::mem::size_of::<f32>() * 2 + 8)
            + scan_state_stream_bytes
            + out_groups * delta_out_weights * std::mem::size_of::<u16>()
            + ffn_groups * (ffn_gate_weights + ffn_gate_weights) * std::mem::size_of::<u16>()
            + down_groups * ffn_down_weights * std::mem::size_of::<u16>()
            + config.tokens * (hidden * 2 * 3 + hidden * 4 * 2 + intermediate * 2);
    let per_layer_bytes = per_layer_bytes_before_fusion
        .saturating_sub(delta_scan_gated_norm_saved_bytes)
        .saturating_sub(residual_mma_saved_bytes)
        .saturating_sub(conv_split_fused_saved_bytes);
    let bytes_moved = per_layer_bytes * layers.len();
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDelta3FfnSuperblockBenchResult {
        tokens: config.tokens,
        layers: layers.len(),
        hidden,
        delta_width,
        intermediate,
        qkv_rows,
        row_tile: config.row_tile,
        project_token_tile,
        qkvz_token_tile: if delta_project_qkvz_mma {
            delta_project_qkvz_mma_token_tile
        } else {
            project_token_tile
        },
        out_token_tile,
        ffn_token_tile,
        down_token_tile,
        packed_weight_bytes: packed_weight_elems * std::mem::size_of::<u16>(),
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

pub fn run_prefill_delta_ffn_stack_with_mps_ffn_sidecar(
    config: PrefillDeltaFfnBlockBenchConfig,
    x_host: &[u16],
    layers: &[PrefillDeltaFfnLayerWeights<'_>],
    mps_ffn_layers: &[PrefillMpsFfnLayerWeights<'_>],
    mps_delta_project_layers: Option<&[PrefillMpsDeltaProjectLayerWeights<'_>]>,
    mps_delta_out_layers: Option<&[PrefillMpsDeltaOutLayerWeights<'_>]>,
) -> Result<PrefillDelta3FfnSuperblockBenchResult, String> {
    if config.tokens == 0 {
        return Err("prefill tokens must be > 0".to_string());
    }
    if layers.is_empty() || layers.len() != mps_ffn_layers.len() {
        return Err("layer stack and MPS FFN sidecar count must match and be > 0".to_string());
    }
    if let Some(delta_layers) = mps_delta_project_layers {
        if layers.len() != delta_layers.len() {
            return Err("layer stack and MPS Delta project sidecar count must match".to_string());
        }
        if !prefill_project_split_norm_enabled() || !prefill_delta_ba_fused_activate_enabled() {
            return Err(
                "MPS Delta project sidecar requires split-norm and fused B/A activation profile"
                    .to_string(),
            );
        }
    }
    if let Some(delta_out_layers) = mps_delta_out_layers {
        if layers.len() != delta_out_layers.len() {
            return Err("layer stack and MPS DeltaOut sidecar count must match".to_string());
        }
    }

    let hidden = QWEN35_08B.hidden_size;
    let delta_width = QWEN35_08B.deltanet_width();
    let qkv_rows = QWEN35_08B.deltanet_qkv_width();
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(qkv_rows, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(heads, hidden, config.row_tile, config.hidden_col_tile)?;
    validate_tiled_matvec_shape(
        hidden,
        delta_width,
        config.row_tile,
        config.delta_out_col_tile,
    )?;
    if x_host.len() != config.tokens * hidden {
        return Err(format!(
            "x_host length must be {}, got {}",
            config.tokens * hidden,
            x_host.len()
        ));
    }

    let qkv_weights =
        round_up_usize(qkv_rows, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let delta_weights = round_up_usize(delta_width, config.row_tile)
        * round_up_usize(hidden, config.hidden_col_tile);
    let gate_weights =
        round_up_usize(heads, config.row_tile) * round_up_usize(hidden, config.hidden_col_tile);
    let delta_out_weights = round_up_usize(hidden, config.row_tile)
        * round_up_usize(delta_width, config.delta_out_col_tile);
    let gate_up_bytes = hidden * intermediate * 2 * std::mem::size_of::<u16>();
    let down_bytes = intermediate * hidden * std::mem::size_of::<u16>();
    let qkvz_rows = qkv_rows + delta_width;
    let qkvz_bytes = hidden * qkvz_rows * std::mem::size_of::<u16>();
    let mut packed_weight_bytes = 0usize;
    for (idx, (layer, sidecar)) in layers.iter().zip(mps_ffn_layers).enumerate() {
        let delta_sidecar_ok = mps_delta_project_layers
            .map(|delta_layers| delta_layers[idx].qkvz.len() == qkvz_bytes)
            .unwrap_or(true);
        if layer.input_norm.len() != hidden
            || layer.ffn_norm.len() != hidden
            || layer.qkv.len() != qkv_weights
            || layer.z.len() != delta_weights
            || layer.b.len() != gate_weights
            || layer.a.len() != gate_weights
            || layer.conv_weight.len() != qkv_rows * 4
            || layer.conv_bias.len() != qkv_rows
            || layer.a_log.len() != heads
            || layer.dt_bias.len() != heads
            || layer.delta_norm.len() != head_dim
            || layer.delta_out.len() != delta_out_weights
            || sidecar.gate_up.len() != gate_up_bytes
            || sidecar.down.len() != down_bytes
            || !delta_sidecar_ok
        {
            return Err(
                "one or more layer or MPS sidecar buffers do not match expected shape".to_string(),
            );
        }
        packed_weight_bytes += (if mps_delta_project_layers.is_some() {
            layer.b.len() + layer.a.len() + layer.delta_out.len()
        } else {
            layer.qkv.len() + layer.z.len() + layer.b.len() + layer.a.len() + layer.delta_out.len()
        }) * std::mem::size_of::<u16>()
            + sidecar.gate_up.len()
            + sidecar.down.len()
            + mps_delta_project_layers
                .map(|delta_layers| delta_layers[idx].qkvz.len())
                .unwrap_or(0);
    }

    let tokens_u32 = u32::try_from(config.tokens).map_err(|_| "tokens exceed u32")?;
    let hidden_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv_rows exceed u32")?;
    let delta_width_u32 = u32::try_from(delta_width).map_err(|_| "delta_width exceed u32")?;
    let heads_u32 = u32::try_from(heads).map_err(|_| "heads exceed u32")?;
    let intermediate_u32 = u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let hidden_col_tile_u32 =
        u32::try_from(config.hidden_col_tile).map_err(|_| "hidden_col_tile exceeds u32")?;
    let delta_out_col_tile_u32 =
        u32::try_from(config.delta_out_col_tile).map_err(|_| "delta_out_col_tile exceeds u32")?;
    let intermediate_col_tile_u32 = u32::try_from(config.intermediate_col_tile)
        .map_err(|_| "intermediate_col_tile exceeds u32")?;
    let hidden_col_tiles_u32 = u32::try_from(hidden.div_ceil(config.hidden_col_tile))
        .map_err(|_| "hidden col tiles exceed u32")?;
    let delta_out_col_tiles_u32 = u32::try_from(delta_width.div_ceil(config.delta_out_col_tile))
        .map_err(|_| "delta out col tiles exceed u32")?;
    let intermediate_col_tiles_u32 =
        u32::try_from(intermediate.div_ceil(config.intermediate_col_tile))
            .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let mps_plan = crate::metal::mps_sidecar::MpsFfnPlan::new(
        &dev,
        config.tokens,
        hidden,
        intermediate,
        hidden * std::mem::size_of::<u16>(),
        intermediate * 2 * std::mem::size_of::<u16>(),
        intermediate * 2 * std::mem::size_of::<u16>(),
        intermediate * std::mem::size_of::<u16>(),
        hidden * std::mem::size_of::<u16>(),
        hidden * std::mem::size_of::<u16>(),
    )?;
    let mps_delta_project_plan = if mps_delta_project_layers.is_some() {
        Some(crate::metal::mps_sidecar::MpsDeltaProjectPlan::new(
            &dev,
            config.tokens,
            hidden,
            qkvz_rows,
            hidden * std::mem::size_of::<u16>(),
            qkvz_rows * std::mem::size_of::<u16>(),
            qkvz_rows * std::mem::size_of::<f32>(),
        )?)
    } else {
        None
    };
    let mps_delta_out_plan = if mps_delta_out_layers.is_some() {
        Some(crate::metal::mps_sidecar::MpsDeltaProjectPlan::new(
            &dev,
            config.tokens,
            delta_width,
            hidden,
            delta_width * std::mem::size_of::<u16>(),
            hidden * std::mem::size_of::<u16>(),
            hidden * std::mem::size_of::<f32>(),
        )?)
    } else {
        None
    };
    let delta_project_norm_pso = if mps_delta_project_layers.is_some() {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let delta_qkvz_split_pso = if mps_delta_project_layers.is_some() {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_split_qkvz_project_f32")?)
    } else {
        None
    };
    let ba_activate_pso = if mps_delta_project_layers.is_some() {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_ba_project_activate_tok4_h16_k1024")?)
    } else {
        None
    };
    let ffn_norm_pso = dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?;
    let swiglu_pso = dev.pipeline("qwen35_08b_mps_swiglu_gateup_fp16_i3584")?;
    let residual_half_pso = dev.pipeline("qwen35_08b_prefill_residual_add_fp16_to_fp16_k1024")?;
    let delta_out_residual_pso =
        dev.pipeline("qwen35_08b_prefill_residual_add_f32_to_fp16_k1024")?;

    let x = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    unsafe {
        x.write(0, x_host);
    }
    let layer_buffers = layers
        .iter()
        .map(|layer| {
            Ok(PrefillDeltaFfnLayerDeviceBuffers {
                input_norm: new_readonly_buffer(&dev, layer.input_norm)?,
                qkv: new_readonly_buffer(&dev, layer.qkv)?,
                z: new_readonly_buffer(&dev, layer.z)?,
                b: new_readonly_buffer(&dev, layer.b)?,
                a: new_readonly_buffer(&dev, layer.a)?,
                conv_weight: new_readonly_buffer(&dev, layer.conv_weight)?,
                conv_bias: new_readonly_buffer(&dev, layer.conv_bias)?,
                a_log: new_readonly_buffer(&dev, layer.a_log)?,
                dt_bias: new_readonly_buffer(&dev, layer.dt_bias)?,
                delta_norm: new_readonly_buffer(&dev, layer.delta_norm)?,
                delta_out: new_readonly_buffer(&dev, layer.delta_out)?,
                ffn_norm: new_readonly_buffer(&dev, layer.ffn_norm)?,
                ffn_gate: dev.new_buffer(1)?,
                ffn_up: dev.new_buffer(1)?,
                ffn_down: dev.new_buffer(1)?,
                conv_state: dev.new_buffer(3 * qkv_rows * std::mem::size_of::<u16>())?,
                recurrent_state: dev
                    .new_buffer(heads * head_dim * head_dim * std::mem::size_of::<f32>())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mps_ffn_buffers = mps_ffn_layers
        .iter()
        .map(|layer| {
            Ok((
                new_readonly_byte_buffer(&dev, layer.gate_up)?,
                new_readonly_byte_buffer(&dev, layer.down)?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mps_delta_project_buffers = mps_delta_project_layers
        .map(|delta_layers| {
            delta_layers
                .iter()
                .map(|layer| new_readonly_byte_buffer(&dev, layer.qkvz))
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;
    let mps_delta_out_buffers = mps_delta_out_layers
        .map(|delta_layers| {
            delta_layers
                .iter()
                .map(|layer| new_readonly_byte_buffer(&dev, layer.out))
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;

    let hidden_a = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let hidden_b = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let qkvz_out = if mps_delta_project_layers.is_some() {
        Some(dev.new_buffer(config.tokens * qkvz_rows * std::mem::size_of::<f32>())?)
    } else {
        None
    };
    let qkv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let conv_out = dev.new_buffer(config.tokens * qkv_rows * std::mem::size_of::<f32>())?;
    let z_out = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let b_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let a_out = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(config.tokens * heads * std::mem::size_of::<f32>())?;
    let delta = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<f32>())?;
    let chunk_scan_tokens = prefill_delta_scan_chunk_tokens();
    let chunk_scan_state_element_bytes = if prefill_delta_scan_chunk_hstate_enabled() {
        std::mem::size_of::<u16>()
    } else {
        std::mem::size_of::<f32>()
    };
    let chunk_scan_state = dev.new_buffer(
        config.tokens.div_ceil(chunk_scan_tokens)
            * heads
            * head_dim
            * head_dim
            * chunk_scan_state_element_bytes,
    )?;
    let gated = dev.new_buffer(config.tokens * delta_width * std::mem::size_of::<u16>())?;
    let delta_out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<f32>())?;
    let after_delta = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let ffn_normed = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let ffn_gate_up =
        dev.new_buffer(config.tokens * intermediate * 2 * std::mem::size_of::<u16>())?;
    let ffn_act = dev.new_buffer(config.tokens * intermediate * std::mem::size_of::<u16>())?;
    let ffn_half_out = dev.new_buffer(config.tokens * hidden * std::mem::size_of::<u16>())?;
    let conv_state_zero = vec![0u16; 3 * qkv_rows];
    let recurrent_state_zero = vec![0.0f32; heads * head_dim * head_dim];
    let profile_stop = PrefillDeltaFfnProfileStop::from_env();

    let run_once = |wait: bool| -> Result<(), String> {
        for layer in &layer_buffers {
            unsafe {
                layer.conv_state.write(0, &conv_state_zero);
                layer.recurrent_state.write(0, &recurrent_state_zero);
            }
        }
        let cmd = dev.command_buffer()?;
        for (idx, layer) in layer_buffers.iter().enumerate() {
            let input = if idx == 0 {
                &x
            } else if idx % 2 == 1 {
                &hidden_a
            } else {
                &hidden_b
            };
            let output = if idx % 2 == 0 { &hidden_a } else { &hidden_b };
            if let (Some(delta_buffers), Some(delta_plan), Some(qkvz_out)) = (
                mps_delta_project_buffers.as_ref(),
                mps_delta_project_plan.as_ref(),
                qkvz_out.as_ref(),
            ) {
                let enc = cmd.compute()?;
                enc.set_pipeline(delta_project_norm_pso.as_ref().expect("delta norm pso"));
                enc.set_buffer(0, input, 0);
                enc.set_buffer(1, &layer.input_norm, 0);
                enc.set_buffer(2, &q_half, 0);
                enc.set_bytes(3, &tokens_u32);
                enc.dispatch_threadgroups((config.tokens, 1, 1), (256, 1, 1));
                enc.end();

                delta_plan.encode(&cmd, &q_half, &delta_buffers[idx], qkvz_out)?;

                let enc = cmd.compute()?;
                enc.set_pipeline(delta_qkvz_split_pso.as_ref().expect("qkvz split pso"));
                enc.set_buffer(0, qkvz_out, 0);
                enc.set_buffer(1, &qkv_out, 0);
                enc.set_buffer(2, &z_out, 0);
                enc.set_bytes(3, &tokens_u32);
                if !prefill_mps_qkvz_direct_enabled() {
                    enc.dispatch_threads(config.tokens * qkvz_rows, 256);
                }

                enc.set_pipeline(ba_activate_pso.as_ref().expect("ba activate pso"));
                enc.set_buffer(0, &q_half, 0);
                enc.set_buffer(1, &layer.b, 0);
                enc.set_buffer(2, &layer.a, 0);
                enc.set_buffer(3, &layer.a_log, 0);
                enc.set_buffer(4, &layer.dt_bias, 0);
                enc.set_buffer(5, &beta, 0);
                enc.set_buffer(6, &decay, 0);
                enc.set_bytes(7, &tokens_u32);
                enc.set_bytes(8, &row_tile_u32);
                enc.set_bytes(9, &hidden_col_tile_u32);
                enc.set_bytes(10, &hidden_col_tiles_u32);
                enc.dispatch_threadgroups(
                    (
                        heads.div_ceil(config.row_tile),
                        config.tokens.div_ceil(4),
                        1,
                    ),
                    (256, 1, 1),
                );
                if profile_stop == PrefillDeltaFfnProfileStop::Project {
                    enc.end();
                    continue;
                }

                let use_mps_delta_out = mps_delta_out_buffers.is_some()
                    && mps_delta_out_plan.is_some()
                    && matches!(
                        profile_stop,
                        PrefillDeltaFfnProfileStop::Full
                            | PrefillDeltaFfnProfileStop::DeltaOut
                            | PrefillDeltaFfnProfileStop::FfnGateUp
                    );
                encode_prefill_delta_projected_to_delta_out(
                    &dev,
                    &enc,
                    input,
                    layer,
                    &qkv_out,
                    &z_out,
                    &conv_out,
                    &q_half,
                    &k_half,
                    &v_half,
                    &beta,
                    &decay,
                    &delta,
                    &chunk_scan_state,
                    &gated,
                    &delta_out,
                    &after_delta,
                    tokens_u32,
                    hidden_u32,
                    row_tile_u32,
                    delta_out_col_tile_u32,
                    delta_out_col_tiles_u32,
                    if use_mps_delta_out {
                        PrefillDeltaFfnProfileStop::ScanNorm
                    } else {
                        profile_stop
                    },
                    if prefill_mps_qkvz_direct_enabled() {
                        Some(qkvz_out)
                    } else {
                        None
                    },
                )?;
                enc.end();
                if use_mps_delta_out {
                    let delta_out_plan = mps_delta_out_plan.as_ref().expect("delta out plan");
                    let delta_out_weights =
                        &mps_delta_out_buffers.as_ref().expect("delta out buffers")[idx];
                    delta_out_plan.encode(&cmd, &gated, delta_out_weights, &delta_out)?;

                    let enc = cmd.compute()?;
                    enc.set_pipeline(&delta_out_residual_pso);
                    enc.set_buffer(0, input, 0);
                    enc.set_buffer(1, &delta_out, 0);
                    enc.set_buffer(2, &after_delta, 0);
                    enc.set_bytes(3, &tokens_u32);
                    enc.dispatch_threads(config.tokens * hidden, 256);
                    enc.end();
                }
                if profile_stop != PrefillDeltaFfnProfileStop::Full
                    && profile_stop != PrefillDeltaFfnProfileStop::FfnGateUp
                {
                    continue;
                }
            } else {
                let delta_profile_stop = match profile_stop {
                    PrefillDeltaFfnProfileStop::Full | PrefillDeltaFfnProfileStop::FfnGateUp => {
                        PrefillDeltaFfnProfileStop::DeltaOut
                    }
                    other => other,
                };
                let enc = cmd.compute()?;
                encode_prefill_delta_ffn_layer_ops(
                    &dev,
                    &enc,
                    input,
                    output,
                    layer,
                    &qkv_out,
                    &z_out,
                    &b_out,
                    &a_out,
                    &conv_out,
                    &q_half,
                    &k_half,
                    &v_half,
                    &beta,
                    &decay,
                    &delta,
                    &chunk_scan_state,
                    &gated,
                    &delta_out,
                    &after_delta,
                    &ffn_act,
                    &ffn_half_out,
                    tokens_u32,
                    hidden_u32,
                    qkv_rows_u32,
                    delta_width_u32,
                    heads_u32,
                    intermediate_u32,
                    row_tile_u32,
                    hidden_col_tile_u32,
                    delta_out_col_tile_u32,
                    intermediate_col_tile_u32,
                    hidden_col_tiles_u32,
                    delta_out_col_tiles_u32,
                    intermediate_col_tiles_u32,
                    delta_profile_stop,
                )?;
                enc.end();
                if profile_stop != PrefillDeltaFfnProfileStop::Full
                    && profile_stop != PrefillDeltaFfnProfileStop::FfnGateUp
                {
                    continue;
                }
            }

            let enc = cmd.compute()?;
            enc.set_pipeline(&ffn_norm_pso);
            enc.set_buffer(0, &after_delta, 0);
            enc.set_buffer(1, &layer.ffn_norm, 0);
            enc.set_buffer(2, &ffn_normed, 0);
            enc.set_bytes(3, &tokens_u32);
            enc.dispatch_threadgroups((config.tokens, 1, 1), (256, 1, 1));
            enc.end();

            let (gate_up_w, down_w) = &mps_ffn_buffers[idx];
            mps_plan.encode_gate_up(&cmd, &ffn_normed, gate_up_w, &ffn_gate_up)?;

            let enc = cmd.compute()?;
            enc.set_pipeline(&swiglu_pso);
            enc.set_buffer(0, &ffn_gate_up, 0);
            enc.set_buffer(1, &ffn_act, 0);
            enc.set_bytes(2, &intermediate_u32);
            enc.set_bytes(3, &(intermediate_u32 * 2));
            enc.set_bytes(4, &intermediate_u32);
            enc.dispatch_threadgroups((intermediate.div_ceil(256), config.tokens, 1), (256, 1, 1));
            enc.end();
            if profile_stop == PrefillDeltaFfnProfileStop::FfnGateUp {
                continue;
            }

            mps_plan.encode_down(&cmd, &ffn_act, down_w, &ffn_half_out)?;

            let enc = cmd.compute()?;
            enc.set_pipeline(&residual_half_pso);
            enc.set_buffer(0, &after_delta, 0);
            enc.set_buffer(1, &ffn_half_out, 0);
            enc.set_buffer(2, output, 0);
            enc.set_bytes(3, &tokens_u32);
            enc.dispatch_threads(config.tokens * hidden, 256);
            enc.end();
        }
        if wait {
            cmd.commit_and_wait()?;
        } else {
            cmd.commit();
        }
        Ok(())
    };

    for _ in 0..config.warmup {
        run_once(true)?;
    }
    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        run_once(true)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0u16; hidden.min(16)];
    let final_hidden = if (layers.len() - 1) % 2 == 0 {
        &hidden_a
    } else {
        &hidden_b
    };
    unsafe {
        final_hidden.read(0, &mut first);
    }
    if let Ok(path) = std::env::var("CTOX_QWEN35_DELTA_STACK_FINAL_DUMP") {
        let mut raw = vec![0u16; config.tokens * hidden];
        unsafe {
            final_hidden.read(0, &mut raw);
        }
        let bytes = raw
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        std::fs::write(&path, bytes)
            .map_err(|err| format!("failed to write Delta stack final dump `{path}`: {err}"))?;
    }
    let checksum = first
        .iter()
        .map(|v| f16::from_bits(*v).to_f32())
        .fold(0.0f32, |acc, v| acc + v);

    let (_, project_token_tile) = prefill_project_matmul_kernel();
    let (_, out_token_tile) = prefill_deltanet_out_matmul_kernel();
    let (_, qkvz_token_tile) = prefill_delta_project_qkvz_mma_kernel_for_tokens(config.tokens);
    let bytes_moved = packed_weight_bytes
        + layers.len()
            * (config.tokens * (qkv_rows * 8 + delta_width * 8 + hidden * 8 + intermediate * 6)
                + prefill_delta_scan_state_stream_bytes(config.tokens, heads, head_dim));
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PrefillDelta3FfnSuperblockBenchResult {
        tokens: config.tokens,
        layers: layers.len(),
        hidden,
        delta_width,
        intermediate,
        qkv_rows,
        row_tile: config.row_tile,
        project_token_tile,
        qkvz_token_tile,
        out_token_tile,
        ffn_token_tile: 0,
        down_token_tile: 0,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

fn dispatch_rms_matvec_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    norm_weight: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    rows: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_rms_matvec_fp16_k1024_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, norm_weight, 0);
    enc.set_buffer(2, w, 0);
    enc.set_buffer(3, y, 0);
    enc.set_bytes(4, &rows);
    enc.dispatch_threadgroups((rows as usize, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_down_matmul_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    tokens: u32,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let (kernel, token_tile) = prefill_down_matmul_kernel();
    dispatch_prefill_down_matmul_named_once(
        dev,
        kernel,
        token_tile,
        prefill_down_threadgroup_threads(),
        x,
        w,
        y,
        tokens,
        rows,
        row_tile,
        col_tile,
        n_col_tiles,
    )
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_down_matmul_named_once(
    dev: &Device,
    kernel: &str,
    token_tile: usize,
    threads_per_threadgroup: usize,
    x: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    tokens: u32,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let pso = dev.pipeline(kernel)?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w, 0);
    enc.set_buffer(2, y, 0);
    enc.set_bytes(3, &tokens);
    enc.set_bytes(4, &rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups(
        (
            rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(token_tile),
            1,
        ),
        (threads_per_threadgroup, 1, 1),
    );
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_out2048_matmul_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    tokens: u32,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let (kernel, token_tile) = prefill_deltanet_out_matmul_kernel();
    validate_deltanet_out_kernel_tile(token_tile, tokens, row_tile)?;
    let pso = dev.pipeline(kernel)?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w, 0);
    enc.set_buffer(2, y, 0);
    enc.set_bytes(3, &tokens);
    enc.set_bytes(4, &rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups(
        (
            rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(token_tile),
            1,
        ),
        (prefill_deltanet_out_threadgroup_threads(token_tile), 1, 1),
    );
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_ffn_block_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    up: &crate::metal::ffi::Buffer,
    down: &crate::metal::ffi::Buffer,
    normed: &crate::metal::ffi::Buffer,
    act: &crate::metal::ffi::Buffer,
    out: &crate::metal::ffi::Buffer,
    tokens: u32,
    hidden_rows: u32,
    intermediate_rows: u32,
    row_tile: u32,
    hidden_col_tile: u32,
    intermediate_col_tile: u32,
    hidden_col_tiles: u32,
    intermediate_col_tiles: u32,
) -> Result<(), String> {
    let gate_up_mma = prefill_ffn_gate_up_mma_enabled();
    let norm_pso = if gate_up_mma {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let (gate_up_kernel, gate_up_token_tile) = if gate_up_mma {
        prefill_ffn_gate_up_mma_kernel()
    } else {
        prefill_ffn_gate_up_kernel()
    };
    let gate_up_pso = dev.pipeline(gate_up_kernel)?;
    let (down_kernel, down_token_tile) = prefill_down_matmul_kernel();
    let down_pso = dev.pipeline(down_kernel)?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    if gate_up_mma {
        enc.set_pipeline(norm_pso.as_ref().expect("ffn norm pso"));
        enc.set_buffer(0, x, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, normed, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threadgroups((tokens as usize, 1, 1), (256, 1, 1));

        enc.set_pipeline(&gate_up_pso);
        enc.set_buffer(0, normed, 0);
        enc.set_buffer(1, gate, 0);
        enc.set_buffer(2, up, 0);
        enc.set_buffer(3, act, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &intermediate_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &hidden_col_tile);
        enc.set_bytes(8, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                intermediate_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(gate_up_token_tile),
                1,
            ),
            (32, 1, 1),
        );
    } else {
        enc.set_pipeline(&gate_up_pso);
        enc.set_buffer(0, x, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, gate, 0);
        enc.set_buffer(3, up, 0);
        enc.set_buffer(4, act, 0);
        enc.set_bytes(5, &tokens);
        enc.set_bytes(6, &intermediate_rows);
        enc.set_bytes(7, &row_tile);
        enc.set_bytes(8, &hidden_col_tile);
        enc.set_bytes(9, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                intermediate_rows.div_ceil(4) as usize,
                (tokens as usize).div_ceil(gate_up_token_tile),
                1,
            ),
            (prefill_project_matmul_threadgroup_threads(), 1, 1),
        );
    }

    enc.set_pipeline(&down_pso);
    enc.set_buffer(0, act, 0);
    enc.set_buffer(1, down, 0);
    enc.set_buffer(2, out, 0);
    enc.set_bytes(3, &tokens);
    enc.set_bytes(4, &hidden_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &intermediate_col_tile);
    enc.set_bytes(7, &intermediate_col_tiles);
    enc.dispatch_threadgroups(
        (
            hidden_rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(down_token_tile),
            1,
        ),
        (prefill_down_threadgroup_threads(), 1, 1),
    );

    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_deltanet_project_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    qkv: &crate::metal::ffi::Buffer,
    z: &crate::metal::ffi::Buffer,
    b: &crate::metal::ffi::Buffer,
    a: &crate::metal::ffi::Buffer,
    qkv_out: &crate::metal::ffi::Buffer,
    z_out: &crate::metal::ffi::Buffer,
    b_out: &crate::metal::ffi::Buffer,
    a_out: &crate::metal::ffi::Buffer,
    tokens: u32,
    qkv_rows: u32,
    z_rows: u32,
    gate_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
) -> Result<(), String> {
    let (kernel, token_tile) = prefill_rms_matmul_kernel();
    let pso = dev.pipeline(kernel)?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);

    for (weights, output, rows) in [
        (qkv, qkv_out, qkv_rows),
        (z, z_out, z_rows),
        (b, b_out, gate_rows),
        (a, a_out, gate_rows),
    ] {
        enc.set_buffer(0, x, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, weights, 0);
        enc.set_buffer(3, output, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &col_tile);
        enc.set_bytes(8, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(token_tile),
                1,
            ),
            (prefill_project_matmul_threadgroup_threads(), 1, 1),
        );
    }

    enc.end();
    cmd.commit_and_wait()
}

fn dispatch_prefill_deltanet_conv_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    state: &crate::metal::ffi::Buffer,
    weight: &crate::metal::ffi::Buffer,
    bias: &crate::metal::ffi::Buffer,
    out: &crate::metal::ffi::Buffer,
    tokens: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_prefill_deltanet_causal_conv1d_silu_c6144_k4")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, state, 0);
    enc.set_buffer(2, weight, 0);
    enc.set_buffer(3, bias, 0);
    enc.set_buffer(4, out, 0);
    enc.set_bytes(5, &tokens);
    enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_deltanet_prepare_once(
    dev: &Device,
    qkv: &crate::metal::ffi::Buffer,
    beta_raw: &crate::metal::ffi::Buffer,
    alpha_raw: &crate::metal::ffi::Buffer,
    a_log: &crate::metal::ffi::Buffer,
    dt_bias: &crate::metal::ffi::Buffer,
    q: &crate::metal::ffi::Buffer,
    k: &crate::metal::ffi::Buffer,
    v: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    decay: &crate::metal::ffi::Buffer,
    tokens: u32,
) -> Result<(), String> {
    let split_pso =
        dev.pipeline("qwen35_08b_prefill_deltanet_split_qkv_norm_tok_f32_to_fp16_h16d128")?;
    let activate_pso = dev.pipeline("qwen35_08b_prefill_deltanet_activate_beta_decay_tok_h16")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&split_pso);
    enc.set_buffer(0, qkv, 0);
    enc.set_buffer(1, q, 0);
    enc.set_buffer(2, k, 0);
    enc.set_buffer(3, v, 0);
    enc.set_bytes(4, &tokens);
    enc.dispatch_threadgroups(
        (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
        (QWEN35_08B.deltanet_head_dim, 1, 1),
    );

    enc.set_pipeline(&activate_pso);
    enc.set_buffer(0, beta_raw, 0);
    enc.set_buffer(1, alpha_raw, 0);
    enc.set_buffer(2, a_log, 0);
    enc.set_buffer(3, dt_bias, 0);
    enc.set_buffer(4, beta, 0);
    enc.set_buffer(5, decay, 0);
    enc.set_bytes(6, &tokens);
    enc.dispatch_threads(tokens as usize * QWEN35_08B.deltanet_v_heads, 256);

    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_ffn_gate_up_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    up: &crate::metal::ffi::Buffer,
    out: &crate::metal::ffi::Buffer,
    tokens: u32,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let (kernel, token_tile) = prefill_ffn_gate_up_kernel();
    let pso = dev.pipeline(kernel)?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, norm, 0);
    enc.set_buffer(2, gate, 0);
    enc.set_buffer(3, up, 0);
    enc.set_buffer(4, out, 0);
    enc.set_bytes(5, &tokens);
    enc.set_bytes(6, &rows);
    enc.set_bytes(7, &row_tile);
    enc.set_bytes(8, &col_tile);
    enc.set_bytes(9, &n_col_tiles);
    enc.dispatch_threadgroups(
        (
            rows.div_ceil(4) as usize,
            (tokens as usize).div_ceil(token_tile),
            1,
        ),
        (256, 1, 1),
    );
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_rms_matmul_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    norm_weight: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    tokens: u32,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let (kernel, token_tile) = prefill_rms_matmul_kernel();
    let pso = dev.pipeline(kernel)?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, norm_weight, 0);
    enc.set_buffer(2, w, 0);
    enc.set_buffer(3, y, 0);
    enc.set_bytes(4, &tokens);
    enc.set_bytes(5, &rows);
    enc.set_bytes(6, &row_tile);
    enc.set_bytes(7, &col_tile);
    enc.set_bytes(8, &n_col_tiles);
    enc.dispatch_threadgroups(
        (
            rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(token_tile),
            1,
        ),
        (256, 1, 1),
    );
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_rms_matvec_tiled_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    norm_weight: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, norm_weight, 0);
    enc.set_buffer(2, w, 0);
    enc.set_buffer(3, y, 0);
    enc.set_bytes(4, &rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups((rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

pub fn run_deltanet_step_bench(
    config: DeltaNetStepBenchConfig,
) -> Result<DeltaNetStepBenchResult, String> {
    let heads = QWEN35_08B.deltanet_v_heads;
    let dim = QWEN35_08B.deltanet_head_dim;
    let vec_elems = heads * dim;
    let state_elems = heads * dim * dim;
    let state_bytes = state_elems * std::mem::size_of::<f32>();

    let dev = Device::default_system()?;
    let q = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let k = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let v = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let gate = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let state = dev.new_buffer(state_bytes)?;
    let out = dev.new_buffer(vec_elems * std::mem::size_of::<f32>())?;

    let q_host: Vec<u16> = (0..vec_elems)
        .map(|i| f16::from_f32(((i % 97) as f32 - 48.0) / 97.0).to_bits())
        .collect();
    let k_host: Vec<u16> = (0..vec_elems)
        .map(|i| f16::from_f32(((i.wrapping_mul(3) % 89) as f32 - 44.0) / 89.0).to_bits())
        .collect();
    let v_host: Vec<u16> = (0..vec_elems)
        .map(|i| f16::from_f32(((i.wrapping_mul(5) % 83) as f32 - 41.0) / 83.0).to_bits())
        .collect();
    let beta_host: Vec<f32> = (0..heads).map(|i| 0.25 + (i as f32) * 0.01).collect();
    let gate_host: Vec<f32> = (0..heads).map(|i| 0.95 - (i as f32) * 0.005).collect();
    let state_host: Vec<f32> = (0..state_elems)
        .map(|i| ((i.wrapping_mul(11) % 127) as f32 - 63.0) / 4096.0)
        .collect();
    unsafe {
        q.write(0, &q_host);
        k.write(0, &k_host);
        v.write(0, &v_host);
        beta.write(0, &beta_host);
        gate.write(0, &gate_host);
        state.write(0, &state_host);
    }

    for _ in 0..config.warmup {
        dispatch_deltanet_step_once(&dev, &q, &k, &v, &beta, &gate, &state, &out)?;
    }
    unsafe {
        state.write(0, &state_host);
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_deltanet_step_once(&dev, &q, &k, &v, &beta, &gate, &state, &out)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut out_host = vec![0.0f32; vec_elems];
    let mut state_out_host = vec![0.0f32; state_elems];
    unsafe {
        out.read(0, &mut out_host);
        state.read(0, &mut state_out_host);
    }
    let (out_ref, state_ref) = cpu_deltanet_step_repeated(
        heads,
        dim,
        config.iterations,
        &q_host,
        &k_host,
        &v_host,
        &beta_host,
        &gate_host,
        &state_host,
    );
    let max_abs_error_out = max_abs_error(&out_host, &out_ref);
    let max_abs_error_state = max_abs_error(&state_out_host, &state_ref);
    let first = &out_host[..vec_elems.min(32)];
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let state_passes = if std::env::var_os("CTOX_QWEN35_DECODE_DELTA_ROWCACHE").is_some() {
        2
    } else {
        3
    };
    let bytes_moved = state_bytes * state_passes + vec_elems * std::mem::size_of::<u16>() * 3;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DeltaNetStepBenchResult {
        heads,
        head_dim: dim,
        state_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        max_abs_error_out,
        max_abs_error_state,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
fn cpu_deltanet_step_repeated(
    heads: usize,
    dim: usize,
    iterations: usize,
    q: &[u16],
    k: &[u16],
    v: &[u16],
    beta: &[f32],
    gate: &[f32],
    initial_state: &[f32],
) -> (Vec<f32>, Vec<f32>) {
    let mut state = initial_state.to_vec();
    let mut out = vec![0.0f32; heads * dim];
    for _ in 0..iterations {
        cpu_deltanet_step_once(heads, dim, q, k, v, beta, gate, &mut state, &mut out);
    }
    (out, state)
}

#[allow(clippy::too_many_arguments)]
fn cpu_deltanet_step_once(
    heads: usize,
    dim: usize,
    q: &[u16],
    k: &[u16],
    v: &[u16],
    beta: &[f32],
    gate: &[f32],
    state: &mut [f32],
    out: &mut [f32],
) {
    let prev_state = state.to_vec();
    for head in 0..heads {
        let vec_base = head * dim;
        let state_base = head * dim * dim;
        for i in 0..dim {
            let mut kv_mem = 0.0f32;
            for j in 0..dim {
                kv_mem += prev_state[state_base + i * dim + j]
                    * gate[head]
                    * f16::from_bits(k[vec_base + j]).to_f32();
            }
            let delta = (f16::from_bits(v[vec_base + i]).to_f32() - kv_mem) * beta[head];
            for j in 0..dim {
                state[state_base + i * dim + j] = prev_state[state_base + i * dim + j] * gate[head]
                    + f16::from_bits(k[vec_base + j]).to_f32() * delta;
            }
        }
        for i in 0..dim {
            let mut acc = 0.0f32;
            for j in 0..dim {
                acc += state[state_base + i * dim + j] * f16::from_bits(q[vec_base + j]).to_f32();
            }
            out[vec_base + i] = acc;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cpu_deltanet_scan_once(
    heads: usize,
    dim: usize,
    tokens: usize,
    q: &[u16],
    k: &[u16],
    v: &[u16],
    beta: &[f32],
    decay: &[f32],
    initial_state: &[f32],
) -> (Vec<f32>, Vec<f32>) {
    let width = heads * dim;
    let mut state = initial_state.to_vec();
    let mut out = vec![0.0f32; tokens * width];
    for token in 0..tokens {
        for head in 0..heads {
            let vec_base = token * width + head * dim;
            let state_base = head * dim * dim;
            let beta_h = beta[token * heads + head];
            let decay_h = decay[token * heads + head];
            for row in 0..dim {
                let row_state_base = state_base + row * dim;
                let mut kv_mem = 0.0f32;
                for col in 0..dim {
                    kv_mem += state[row_state_base + col]
                        * decay_h
                        * f16::from_bits(k[vec_base + col]).to_f32();
                }
                let delta = (f16::from_bits(v[vec_base + row]).to_f32() - kv_mem) * beta_h;
                let mut acc = 0.0f32;
                for col in 0..dim {
                    let next_state = state[row_state_base + col] * decay_h
                        + f16::from_bits(k[vec_base + col]).to_f32() * delta;
                    state[row_state_base + col] = next_state;
                    acc += next_state * f16::from_bits(q[vec_base + col]).to_f32();
                }
                out[vec_base + row] = acc;
            }
        }
    }
    (out, state)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_deltanet_step_once(
    dev: &Device,
    q: &crate::metal::ffi::Buffer,
    k: &crate::metal::ffi::Buffer,
    v: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    state: &crate::metal::ffi::Buffer,
    out: &crate::metal::ffi::Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline(decode_deltanet_step_kernel())?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, q, 0);
    enc.set_buffer(1, k, 0);
    enc.set_buffer(2, v, 0);
    enc.set_buffer(3, beta, 0);
    enc.set_buffer(4, gate, 0);
    enc.set_buffer(5, state, 0);
    enc.set_buffer(6, out, 0);
    enc.dispatch_threadgroups((QWEN35_08B.deltanet_v_heads, 1, 1), (128, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_deltanet_scan_once(
    dev: &Device,
    q: &crate::metal::ffi::Buffer,
    k: &crate::metal::ffi::Buffer,
    v: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    decay: &crate::metal::ffi::Buffer,
    state: &crate::metal::ffi::Buffer,
    out: &crate::metal::ffi::Buffer,
    tokens: u32,
) -> Result<(), String> {
    let pso = dev.pipeline(prefill_delta_scan_kernel_for_tokens(tokens as usize))?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, q, 0);
    enc.set_buffer(1, k, 0);
    enc.set_buffer(2, v, 0);
    enc.set_buffer(3, beta, 0);
    enc.set_buffer(4, decay, 0);
    enc.set_buffer(5, state, 0);
    enc.set_buffer(6, out, 0);
    enc.set_bytes(7, &tokens);
    let (grid, threads) = dispatch_prefill_delta_scan_shape_for_tokens(tokens as usize);
    enc.dispatch_threadgroups(grid, threads);
    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_deltanet_out_block_once(
    dev: &Device,
    delta: &crate::metal::ffi::Buffer,
    z: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    gated: &crate::metal::ffi::Buffer,
    out_w: &crate::metal::ffi::Buffer,
    out: &crate::metal::ffi::Buffer,
    tokens: u32,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<(), String> {
    let norm_pso =
        dev.pipeline(prefill_delta_gated_norm_kernel(false))?;
    let (out_kernel, out_token_tile) = prefill_deltanet_out_matmul_kernel();
    validate_deltanet_out_kernel_tile(out_token_tile, tokens, row_tile)?;
    let out_pso = dev.pipeline(out_kernel)?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&norm_pso);
    enc.set_buffer(0, delta, 0);
    enc.set_buffer(1, z, 0);
    enc.set_buffer(2, norm, 0);
    enc.set_buffer(3, gated, 0);
    enc.set_bytes(4, &tokens);
    enc.dispatch_threadgroups(
        (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
        prefill_delta_gated_norm_threads(),
    );

    enc.set_pipeline(&out_pso);
    enc.set_buffer(0, gated, 0);
    enc.set_buffer(1, out_w, 0);
    enc.set_buffer(2, out, 0);
    enc.set_bytes(3, &tokens);
    enc.set_bytes(4, &rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups(
        (
            rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(out_token_tile),
            1,
        ),
        (
            prefill_deltanet_out_threadgroup_threads(out_token_tile),
            1,
            1,
        ),
    );

    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_deltanet_block_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    input_norm: &crate::metal::ffi::Buffer,
    qkv: &crate::metal::ffi::Buffer,
    z: &crate::metal::ffi::Buffer,
    b: &crate::metal::ffi::Buffer,
    a: &crate::metal::ffi::Buffer,
    qkv_out: &crate::metal::ffi::Buffer,
    z_out: &crate::metal::ffi::Buffer,
    b_out: &crate::metal::ffi::Buffer,
    a_out: &crate::metal::ffi::Buffer,
    conv_state: &crate::metal::ffi::Buffer,
    conv_weight: &crate::metal::ffi::Buffer,
    conv_bias: &crate::metal::ffi::Buffer,
    conv_out: &crate::metal::ffi::Buffer,
    a_log: &crate::metal::ffi::Buffer,
    dt_bias: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    decay: &crate::metal::ffi::Buffer,
    recurrent_state: &crate::metal::ffi::Buffer,
    delta: &crate::metal::ffi::Buffer,
    delta_norm: &crate::metal::ffi::Buffer,
    gated: &crate::metal::ffi::Buffer,
    out_w: &crate::metal::ffi::Buffer,
    out: &crate::metal::ffi::Buffer,
    tokens: u32,
    hidden_rows: u32,
    qkv_rows: u32,
    delta_rows: u32,
    gate_rows: u32,
    row_tile: u32,
    hidden_col_tile: u32,
    out_col_tile: u32,
    hidden_col_tiles: u32,
    out_col_tiles: u32,
) -> Result<(), String> {
    let split_project_norm = prefill_project_split_norm_enabled();
    let project_norm_pso = if split_project_norm {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let (project_kernel, project_token_tile) = prefill_project_matmul_kernel();
    let project_pso = dev.pipeline(project_kernel)?;
    let conv_split_fused = prefill_delta_conv_split_fused_enabled();
    let conv_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_causal_conv1d_silu_c6144_k4")?)
    };
    let split_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_split_qkv_norm_tok_f32_to_fp16_h16d128")?)
    };
    let (conv_split_kernel, conv_split_token_block) = prefill_delta_conv_split_fused_kernel();
    let conv_split_pso = if conv_split_fused {
        Some(dev.pipeline(conv_split_kernel)?)
    } else {
        None
    };
    let conv_state_update_pso = if conv_split_fused {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_conv_state_update_c6144_k4")?)
    } else {
        None
    };
    let activate_pso = dev.pipeline("qwen35_08b_prefill_deltanet_activate_beta_decay_tok_h16")?;
    let scan_gated_norm = prefill_delta_scan_gated_norm_enabled();
    let scan_pso = if scan_gated_norm {
        dev.pipeline(prefill_delta_scan_gated_norm_kernel())?
    } else {
        dev.pipeline(prefill_delta_scan_kernel_for_tokens(tokens as usize))?
    };
    let gated_norm_pso = if scan_gated_norm {
        None
    } else {
        Some(dev.pipeline(prefill_delta_gated_norm_kernel(false))?)
    };
    let (out_kernel, out_token_tile) = prefill_deltanet_out_matmul_kernel();
    let out_pso = dev.pipeline(out_kernel)?;

    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    if split_project_norm {
        enc.set_pipeline(project_norm_pso.as_ref().expect("project norm pso"));
        enc.set_buffer(0, x, 0);
        enc.set_buffer(1, input_norm, 0);
        enc.set_buffer(2, q_half, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threadgroups((tokens as usize, 1, 1), (256, 1, 1));
    }

    enc.set_pipeline(&project_pso);
    for (weights, output, rows) in [
        (qkv, qkv_out, qkv_rows),
        (z, z_out, delta_rows),
        (b, b_out, gate_rows),
        (a, a_out, gate_rows),
    ] {
        if split_project_norm {
            enc.set_buffer(0, q_half, 0);
            enc.set_buffer(1, weights, 0);
            enc.set_buffer(2, output, 0);
            enc.set_bytes(3, &tokens);
            enc.set_bytes(4, &rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &hidden_col_tile);
            enc.set_bytes(7, &hidden_col_tiles);
        } else {
            enc.set_buffer(0, x, 0);
            enc.set_buffer(1, input_norm, 0);
            enc.set_buffer(2, weights, 0);
            enc.set_buffer(3, output, 0);
            enc.set_bytes(4, &tokens);
            enc.set_bytes(5, &rows);
            enc.set_bytes(6, &row_tile);
            enc.set_bytes(7, &hidden_col_tile);
            enc.set_bytes(8, &hidden_col_tiles);
        }
        enc.dispatch_threadgroups(
            (
                rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(project_token_tile),
                1,
            ),
            (prefill_project_matmul_threadgroup_threads(), 1, 1),
        );
    }

    if conv_split_fused {
        enc.set_pipeline(conv_split_pso.as_ref().expect("conv split fused pso"));
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, conv_state, 0);
        enc.set_buffer(2, conv_weight, 0);
        enc.set_buffer(3, conv_bias, 0);
        enc.set_buffer(4, q_half, 0);
        enc.set_buffer(5, k_half, 0);
        enc.set_buffer(6, v_half, 0);
        enc.set_bytes(7, &tokens);
        enc.dispatch_threadgroups(
            (
                (tokens as usize).div_ceil(conv_split_token_block),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );

        enc.set_pipeline(
            conv_state_update_pso
                .as_ref()
                .expect("conv state update pso"),
        );
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, conv_state, 0);
        enc.set_bytes(2, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);
    } else {
        enc.set_pipeline(conv_pso.as_ref().expect("conv pso"));
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, conv_state, 0);
        enc.set_buffer(2, conv_weight, 0);
        enc.set_buffer(3, conv_bias, 0);
        enc.set_buffer(4, conv_out, 0);
        enc.set_bytes(5, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);

        enc.set_pipeline(split_pso.as_ref().expect("split pso"));
        enc.set_buffer(0, conv_out, 0);
        enc.set_buffer(1, q_half, 0);
        enc.set_buffer(2, k_half, 0);
        enc.set_buffer(3, v_half, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (
                (tokens as usize).div_ceil(conv_split_token_block),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );
    }

    enc.set_pipeline(&activate_pso);
    enc.set_buffer(0, b_out, 0);
    enc.set_buffer(1, a_out, 0);
    enc.set_buffer(2, a_log, 0);
    enc.set_buffer(3, dt_bias, 0);
    enc.set_buffer(4, beta, 0);
    enc.set_buffer(5, decay, 0);
    enc.set_bytes(6, &tokens);
    enc.dispatch_threads(tokens as usize * QWEN35_08B.deltanet_v_heads, 256);

    enc.set_pipeline(&scan_pso);
    enc.set_buffer(0, q_half, 0);
    enc.set_buffer(1, k_half, 0);
    enc.set_buffer(2, v_half, 0);
    enc.set_buffer(3, beta, 0);
    enc.set_buffer(4, decay, 0);
    enc.set_buffer(5, recurrent_state, 0);
    if scan_gated_norm {
        enc.set_buffer(6, z_out, 0);
        enc.set_buffer(7, delta_norm, 0);
        enc.set_buffer(8, gated, 0);
        enc.set_bytes(9, &tokens);
    } else {
        enc.set_buffer(6, delta, 0);
        enc.set_bytes(7, &tokens);
    }
    let (grid, threads) = dispatch_prefill_delta_scan_shape_for_tokens(tokens as usize);
    enc.dispatch_threadgroups(grid, threads);

    if !scan_gated_norm {
        enc.set_pipeline(gated_norm_pso.as_ref().expect("gated norm pso"));
        enc.set_buffer(0, delta, 0);
        enc.set_buffer(1, z_out, 0);
        enc.set_buffer(2, delta_norm, 0);
        enc.set_buffer(3, gated, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
            prefill_delta_gated_norm_threads(),
        );
    }

    enc.set_pipeline(&out_pso);
    enc.set_buffer(0, gated, 0);
    enc.set_buffer(1, out_w, 0);
    enc.set_buffer(2, out, 0);
    enc.set_bytes(3, &tokens);
    enc.set_bytes(4, &hidden_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &out_col_tile);
    enc.set_bytes(7, &out_col_tiles);
    enc.dispatch_threadgroups(
        (
            hidden_rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(out_token_tile),
            1,
        ),
        (256, 1, 1),
    );

    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn dispatch_prefill_delta_ffn_block_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    input_norm: &crate::metal::ffi::Buffer,
    qkv: &crate::metal::ffi::Buffer,
    z: &crate::metal::ffi::Buffer,
    b: &crate::metal::ffi::Buffer,
    a: &crate::metal::ffi::Buffer,
    qkv_out: &crate::metal::ffi::Buffer,
    z_out: &crate::metal::ffi::Buffer,
    b_out: &crate::metal::ffi::Buffer,
    a_out: &crate::metal::ffi::Buffer,
    conv_state: &crate::metal::ffi::Buffer,
    conv_weight: &crate::metal::ffi::Buffer,
    conv_bias: &crate::metal::ffi::Buffer,
    conv_out: &crate::metal::ffi::Buffer,
    a_log: &crate::metal::ffi::Buffer,
    dt_bias: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    decay: &crate::metal::ffi::Buffer,
    recurrent_state: &crate::metal::ffi::Buffer,
    delta: &crate::metal::ffi::Buffer,
    delta_norm: &crate::metal::ffi::Buffer,
    gated: &crate::metal::ffi::Buffer,
    delta_out_w: &crate::metal::ffi::Buffer,
    delta_out: &crate::metal::ffi::Buffer,
    after_delta: &crate::metal::ffi::Buffer,
    ffn_norm: &crate::metal::ffi::Buffer,
    ffn_gate: &crate::metal::ffi::Buffer,
    ffn_up: &crate::metal::ffi::Buffer,
    ffn_down: &crate::metal::ffi::Buffer,
    ffn_act: &crate::metal::ffi::Buffer,
    ffn_out: &crate::metal::ffi::Buffer,
    after_ffn: &crate::metal::ffi::Buffer,
    tokens: u32,
    hidden_rows: u32,
    qkv_rows: u32,
    delta_rows: u32,
    gate_rows: u32,
    intermediate_rows: u32,
    row_tile: u32,
    hidden_col_tile: u32,
    delta_out_col_tile: u32,
    intermediate_col_tile: u32,
    hidden_col_tiles: u32,
    delta_out_col_tiles: u32,
    intermediate_col_tiles: u32,
) -> Result<(), String> {
    let split_project_norm = prefill_project_split_norm_enabled();
    let project_norm_pso = if split_project_norm {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let (project_kernel, project_token_tile) = prefill_project_matmul_kernel();
    let project_pso = dev.pipeline(project_kernel)?;
    let conv_split_fused = prefill_delta_conv_split_fused_enabled();
    let conv_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_causal_conv1d_silu_c6144_k4")?)
    };
    let split_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_split_qkv_norm_tok_f32_to_fp16_h16d128")?)
    };
    let (conv_split_kernel, conv_split_token_block) = prefill_delta_conv_split_fused_kernel();
    let conv_split_pso = if conv_split_fused {
        Some(dev.pipeline(conv_split_kernel)?)
    } else {
        None
    };
    let conv_state_update_pso = if conv_split_fused {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_conv_state_update_c6144_k4")?)
    } else {
        None
    };
    let activate_pso = dev.pipeline("qwen35_08b_prefill_deltanet_activate_beta_decay_tok_h16")?;
    let scan_gated_norm = prefill_delta_scan_gated_norm_enabled();
    let scan_pso = if scan_gated_norm {
        dev.pipeline(prefill_delta_scan_gated_norm_kernel())?
    } else {
        dev.pipeline(prefill_delta_scan_kernel_for_tokens(tokens as usize))?
    };
    let gated_norm_pso = if scan_gated_norm {
        None
    } else {
        Some(dev.pipeline(prefill_delta_gated_norm_kernel(false))?)
    };
    let (out_kernel, out_token_tile) = prefill_deltanet_out_residual_kernel();
    let out_pso = dev.pipeline(out_kernel)?;
    let ffn_gate_up_mma = prefill_ffn_gate_up_mma_enabled();
    let ffn_norm_pso = if ffn_gate_up_mma {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let (gate_up_kernel, gate_up_token_tile) = if ffn_gate_up_mma {
        prefill_ffn_gate_up_mma_kernel()
    } else {
        prefill_ffn_gate_up_kernel()
    };
    let gate_up_pso = dev.pipeline(gate_up_kernel)?;
    let down_mma = prefill_down_mma_enabled();
    let (down_kernel, down_token_tile) = if down_mma {
        prefill_down_matmul_kernel()
    } else {
        prefill_down_residual_kernel()
    };
    let down_pso = dev.pipeline(down_kernel)?;
    let residual_pso = if down_mma {
        Some(dev.pipeline("qwen35_08b_prefill_residual_add_f32_to_fp16_k1024")?)
    } else {
        None
    };
    let _ = delta_out;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    if split_project_norm {
        enc.set_pipeline(project_norm_pso.as_ref().expect("project norm pso"));
        enc.set_buffer(0, x, 0);
        enc.set_buffer(1, input_norm, 0);
        enc.set_buffer(2, q_half, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threadgroups((tokens as usize, 1, 1), (256, 1, 1));
    }

    enc.set_pipeline(&project_pso);
    for (weights, output, rows) in [
        (qkv, qkv_out, qkv_rows),
        (z, z_out, delta_rows),
        (b, b_out, gate_rows),
        (a, a_out, gate_rows),
    ] {
        if split_project_norm {
            enc.set_buffer(0, q_half, 0);
            enc.set_buffer(1, weights, 0);
            enc.set_buffer(2, output, 0);
            enc.set_bytes(3, &tokens);
            enc.set_bytes(4, &rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &hidden_col_tile);
            enc.set_bytes(7, &hidden_col_tiles);
        } else {
            enc.set_buffer(0, x, 0);
            enc.set_buffer(1, input_norm, 0);
            enc.set_buffer(2, weights, 0);
            enc.set_buffer(3, output, 0);
            enc.set_bytes(4, &tokens);
            enc.set_bytes(5, &rows);
            enc.set_bytes(6, &row_tile);
            enc.set_bytes(7, &hidden_col_tile);
            enc.set_bytes(8, &hidden_col_tiles);
        }
        enc.dispatch_threadgroups(
            (
                rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(project_token_tile),
                1,
            ),
            (256, 1, 1),
        );
    }

    if conv_split_fused {
        enc.set_pipeline(conv_split_pso.as_ref().expect("conv split fused pso"));
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, conv_state, 0);
        enc.set_buffer(2, conv_weight, 0);
        enc.set_buffer(3, conv_bias, 0);
        enc.set_buffer(4, q_half, 0);
        enc.set_buffer(5, k_half, 0);
        enc.set_buffer(6, v_half, 0);
        enc.set_bytes(7, &tokens);
        enc.dispatch_threadgroups(
            (
                (tokens as usize).div_ceil(conv_split_token_block),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );

        enc.set_pipeline(
            conv_state_update_pso
                .as_ref()
                .expect("conv state update pso"),
        );
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, conv_state, 0);
        enc.set_bytes(2, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);
    } else {
        enc.set_pipeline(conv_pso.as_ref().expect("conv pso"));
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, conv_state, 0);
        enc.set_buffer(2, conv_weight, 0);
        enc.set_buffer(3, conv_bias, 0);
        enc.set_buffer(4, conv_out, 0);
        enc.set_bytes(5, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);

        enc.set_pipeline(split_pso.as_ref().expect("split pso"));
        enc.set_buffer(0, conv_out, 0);
        enc.set_buffer(1, q_half, 0);
        enc.set_buffer(2, k_half, 0);
        enc.set_buffer(3, v_half, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );
    }

    enc.set_pipeline(&activate_pso);
    enc.set_buffer(0, b_out, 0);
    enc.set_buffer(1, a_out, 0);
    enc.set_buffer(2, a_log, 0);
    enc.set_buffer(3, dt_bias, 0);
    enc.set_buffer(4, beta, 0);
    enc.set_buffer(5, decay, 0);
    enc.set_bytes(6, &tokens);
    enc.dispatch_threads(tokens as usize * QWEN35_08B.deltanet_v_heads, 256);

    enc.set_pipeline(&scan_pso);
    enc.set_buffer(0, q_half, 0);
    enc.set_buffer(1, k_half, 0);
    enc.set_buffer(2, v_half, 0);
    enc.set_buffer(3, beta, 0);
    enc.set_buffer(4, decay, 0);
    enc.set_buffer(5, recurrent_state, 0);
    if scan_gated_norm {
        enc.set_buffer(6, z_out, 0);
        enc.set_buffer(7, delta_norm, 0);
        enc.set_buffer(8, gated, 0);
        enc.set_bytes(9, &tokens);
    } else {
        enc.set_buffer(6, delta, 0);
        enc.set_bytes(7, &tokens);
    }
    let (grid, threads) = dispatch_prefill_delta_scan_shape_for_tokens(tokens as usize);
    enc.dispatch_threadgroups(grid, threads);

    if !scan_gated_norm {
        enc.set_pipeline(gated_norm_pso.as_ref().expect("gated norm pso"));
        enc.set_buffer(0, delta, 0);
        enc.set_buffer(1, z_out, 0);
        enc.set_buffer(2, delta_norm, 0);
        enc.set_buffer(3, gated, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
            prefill_delta_gated_norm_threads(),
        );
    }

    enc.set_pipeline(&out_pso);
    enc.set_buffer(0, gated, 0);
    enc.set_buffer(1, delta_out_w, 0);
    enc.set_buffer(2, x, 0);
    enc.set_buffer(3, after_delta, 0);
    enc.set_bytes(4, &tokens);
    enc.set_bytes(5, &hidden_rows);
    enc.set_bytes(6, &row_tile);
    enc.set_bytes(7, &delta_out_col_tile);
    enc.set_bytes(8, &delta_out_col_tiles);
    enc.dispatch_threadgroups(
        (
            hidden_rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(out_token_tile),
            1,
        ),
        (256, 1, 1),
    );

    if ffn_gate_up_mma {
        enc.set_pipeline(ffn_norm_pso.as_ref().expect("ffn norm pso"));
        enc.set_buffer(0, after_delta, 0);
        enc.set_buffer(1, ffn_norm, 0);
        enc.set_buffer(2, q_half, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threadgroups((tokens as usize, 1, 1), (256, 1, 1));

        enc.set_pipeline(&gate_up_pso);
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, ffn_gate, 0);
        enc.set_buffer(2, ffn_up, 0);
        enc.set_buffer(3, ffn_act, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &intermediate_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &hidden_col_tile);
        enc.set_bytes(8, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                intermediate_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(gate_up_token_tile),
                1,
            ),
            (32, 1, 1),
        );
    } else {
        enc.set_pipeline(&gate_up_pso);
        enc.set_buffer(0, after_delta, 0);
        enc.set_buffer(1, ffn_norm, 0);
        enc.set_buffer(2, ffn_gate, 0);
        enc.set_buffer(3, ffn_up, 0);
        enc.set_buffer(4, ffn_act, 0);
        enc.set_bytes(5, &tokens);
        enc.set_bytes(6, &intermediate_rows);
        enc.set_bytes(7, &row_tile);
        enc.set_bytes(8, &hidden_col_tile);
        enc.set_bytes(9, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                intermediate_rows.div_ceil(4) as usize,
                (tokens as usize).div_ceil(gate_up_token_tile),
                1,
            ),
            (256, 1, 1),
        );
    }

    enc.set_pipeline(&down_pso);
    enc.set_buffer(0, ffn_act, 0);
    enc.set_buffer(1, ffn_down, 0);
    if down_mma {
        enc.set_buffer(2, ffn_out, 0);
        enc.set_bytes(3, &tokens);
        enc.set_bytes(4, &hidden_rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &intermediate_col_tile);
        enc.set_bytes(7, &intermediate_col_tiles);
        enc.dispatch_threadgroups(
            (
                hidden_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(down_token_tile),
                1,
            ),
            (prefill_down_threadgroup_threads(), 1, 1),
        );

        enc.set_pipeline(residual_pso.as_ref().expect("residual pso"));
        enc.set_buffer(0, after_delta, 0);
        enc.set_buffer(1, ffn_out, 0);
        enc.set_buffer(2, after_ffn, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threads(tokens as usize * QWEN35_08B.hidden_size, 256);
    } else {
        enc.set_buffer(2, after_delta, 0);
        enc.set_buffer(3, after_ffn, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &hidden_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &intermediate_col_tile);
        enc.set_bytes(8, &intermediate_col_tiles);
        enc.dispatch_threadgroups(
            (
                hidden_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(down_token_tile),
                1,
            ),
            (256, 1, 1),
        );
    }

    enc.end();
    cmd.commit_and_wait()
}

#[allow(clippy::too_many_arguments)]
fn encode_prefill_delta_ffn_layer_ops(
    dev: &Device,
    enc: &crate::metal::ffi::ComputeEncoder,
    input: &crate::metal::ffi::Buffer,
    output: &crate::metal::ffi::Buffer,
    layer: &PrefillDeltaFfnLayerDeviceBuffers,
    qkv_out: &crate::metal::ffi::Buffer,
    z_out: &crate::metal::ffi::Buffer,
    b_out: &crate::metal::ffi::Buffer,
    a_out: &crate::metal::ffi::Buffer,
    conv_out: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    decay: &crate::metal::ffi::Buffer,
    delta: &crate::metal::ffi::Buffer,
    chunk_scan_state: &crate::metal::ffi::Buffer,
    gated: &crate::metal::ffi::Buffer,
    delta_out: &crate::metal::ffi::Buffer,
    after_delta: &crate::metal::ffi::Buffer,
    ffn_act: &crate::metal::ffi::Buffer,
    ffn_out: &crate::metal::ffi::Buffer,
    tokens: u32,
    hidden_rows: u32,
    qkv_rows: u32,
    delta_rows: u32,
    gate_rows: u32,
    intermediate_rows: u32,
    row_tile: u32,
    hidden_col_tile: u32,
    delta_out_col_tile: u32,
    intermediate_col_tile: u32,
    hidden_col_tiles: u32,
    delta_out_col_tiles: u32,
    intermediate_col_tiles: u32,
    profile_stop: PrefillDeltaFfnProfileStop,
) -> Result<(), String> {
    let split_project_norm = prefill_project_split_norm_enabled();
    let project_norm_pso = if split_project_norm {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let (project_kernel, project_token_tile) = prefill_project_matmul_kernel();
    let project_pso = dev.pipeline(project_kernel)?;
    let (delta_project_mma_kernel, delta_project_mma_token_tile) =
        prefill_delta_project_qkvz_mma_kernel_for_tokens(tokens as usize);
    let delta_project_qkvz_mma = split_project_norm
        && prefill_delta_project_qkvz_mma_enabled()
        && tokens.is_multiple_of(delta_project_mma_token_tile as u32)
        && row_tile == 8;
    let delta_project_mma_pso = if delta_project_qkvz_mma {
        Some(dev.pipeline(delta_project_mma_kernel)?)
    } else {
        None
    };
    let conv_split_fused = prefill_delta_conv_split_fused_enabled();
    let conv_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_causal_conv1d_silu_c6144_k4")?)
    };
    let split_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_split_qkv_norm_tok_f32_to_fp16_h16d128")?)
    };
    let (conv_split_kernel, conv_split_token_block) = prefill_delta_conv_split_fused_kernel();
    let conv_split_pso = if conv_split_fused {
        Some(dev.pipeline(conv_split_kernel)?)
    } else {
        None
    };
    let conv_state_update_pso = if conv_split_fused {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_conv_state_update_c6144_k4")?)
    } else {
        None
    };
    let activate_pso = dev.pipeline("qwen35_08b_prefill_deltanet_activate_beta_decay_tok_h16")?;
    let fused_ba_activate =
        split_project_norm && prefill_delta_ba_fused_activate_enabled() && row_tile == 8;
    let fused_ba_activate_pso = if fused_ba_activate {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_ba_project_activate_tok4_h16_k1024")?)
    } else {
        None
    };
    let scan_gated_norm = prefill_delta_scan_gated_norm_enabled();
    let chunk_scan_f32x4 = prefill_delta_scan_chunk_f32x4_enabled() && !scan_gated_norm;
    let chunk_scan_hstate = prefill_delta_scan_chunk_hstate_enabled();
    let scan_pso = if chunk_scan_f32x4 {
        None
    } else if scan_gated_norm {
        Some(dev.pipeline(prefill_delta_scan_gated_norm_kernel())?)
    } else {
        Some(dev.pipeline(prefill_delta_scan_kernel_for_tokens(tokens as usize))?)
    };
    let chunk_scan_phase2_pso = if chunk_scan_f32x4 {
        Some(dev.pipeline(if chunk_scan_hstate {
            "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_hstate_h16d128"
        } else {
            "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_f32state_h16d128"
        })?)
    } else {
        None
    };
    let chunk_scan_phase3_pso = if chunk_scan_f32x4 {
        Some(dev.pipeline(if chunk_scan_hstate {
            "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_hstate_h16d128"
        } else {
            "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_f32state_h16d128"
        })?)
    } else {
        None
    };
    let chunk_scan_tokens =
        u32::try_from(prefill_delta_scan_chunk_tokens()).map_err(|_| "chunk tokens exceed u32")?;
    let gated_norm_pso = if scan_gated_norm {
        None
    } else {
        Some(dev.pipeline(prefill_delta_gated_norm_kernel(false))?)
    };
    let requested_delta_out_mma = prefill_deltanet_out_mma_enabled();
    let (candidate_out_kernel, candidate_out_token_tile) = prefill_deltanet_out_matmul_kernel();
    let delta_out_mma = requested_delta_out_mma
        && tokens.is_multiple_of(candidate_out_token_tile as u32)
        && row_tile == 8;
    let (out_kernel, out_token_tile) = if delta_out_mma {
        (candidate_out_kernel, candidate_out_token_tile)
    } else {
        prefill_deltanet_out_residual_kernel()
    };
    let delta_out_mma_residual_fused = delta_out_mma
        && ((out_token_tile == 32 && prefill_deltanet_out_mma32_residual_enabled())
            || out_token_tile == 64);
    let out_pso =
        if delta_out_mma && out_token_tile == 32 && prefill_deltanet_out_mma32_residual_enabled() {
            dev.pipeline("qwen35_08b_prefill_deltanet_out_mma32x8_residual_fp16_tiled_k2048_f32")?
        } else {
            dev.pipeline(out_kernel)?
        };
    let delta_out_residual_pso = if delta_out_mma && !delta_out_mma_residual_fused {
        Some(dev.pipeline("qwen35_08b_prefill_residual_add_f32_to_fp16_k1024")?)
    } else {
        None
    };
    let ffn_gate_up_mma = prefill_ffn_gate_up_mma_enabled();
    let ffn_norm_pso = if ffn_gate_up_mma {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let (gate_up_kernel, gate_up_token_tile) = if ffn_gate_up_mma {
        prefill_ffn_gate_up_mma_kernel()
    } else {
        prefill_ffn_gate_up_kernel()
    };
    let gate_up_pso = dev.pipeline(gate_up_kernel)?;
    let down_mma = prefill_down_mma_enabled();
    let (down_kernel, down_token_tile) = if down_mma {
        prefill_down_matmul_kernel()
    } else {
        prefill_down_residual_kernel()
    };
    let down_mma_residual_kernel = if down_mma {
        prefill_down_mma_residual_kernel(down_token_tile)
    } else {
        None
    };
    let down_pso = if let Some(kernel) = down_mma_residual_kernel {
        dev.pipeline(kernel)?
    } else {
        dev.pipeline(down_kernel)?
    };
    let residual_pso = if down_mma && down_mma_residual_kernel.is_none() {
        Some(dev.pipeline("qwen35_08b_prefill_residual_add_f32_to_fp16_k1024")?)
    } else {
        None
    };
    if split_project_norm {
        enc.set_pipeline(project_norm_pso.as_ref().expect("project norm pso"));
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, &layer.input_norm, 0);
        enc.set_buffer(2, q_half, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threadgroups((tokens as usize, 1, 1), (256, 1, 1));
    }

    for (idx, (weights, output, rows)) in [
        (&layer.qkv, qkv_out, qkv_rows),
        (&layer.z, z_out, delta_rows),
        (&layer.b, b_out, gate_rows),
        (&layer.a, a_out, gate_rows),
    ]
    .into_iter()
    .enumerate()
    {
        if fused_ba_activate && idx >= 2 {
            continue;
        }
        let use_mma = delta_project_qkvz_mma && idx < 2;
        if use_mma {
            enc.set_pipeline(
                delta_project_mma_pso
                    .as_ref()
                    .expect("delta project MMA pso"),
            );
        } else {
            enc.set_pipeline(&project_pso);
        }
        if split_project_norm {
            enc.set_buffer(0, q_half, 0);
            enc.set_buffer(1, weights, 0);
            enc.set_buffer(2, output, 0);
            enc.set_bytes(3, &tokens);
            enc.set_bytes(4, &rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &hidden_col_tile);
            enc.set_bytes(7, &hidden_col_tiles);
        } else {
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, &layer.input_norm, 0);
            enc.set_buffer(2, weights, 0);
            enc.set_buffer(3, output, 0);
            enc.set_bytes(4, &tokens);
            enc.set_bytes(5, &rows);
            enc.set_bytes(6, &row_tile);
            enc.set_bytes(7, &hidden_col_tile);
            enc.set_bytes(8, &hidden_col_tiles);
        }
        enc.dispatch_threadgroups(
            (
                if use_mma && prefill_delta_project_qkvz_mma_rg4_ashared_enabled() {
                    rows.div_ceil(row_tile * 4) as usize
                } else {
                    rows.div_ceil(row_tile) as usize
                },
                (tokens as usize).div_ceil(if use_mma {
                    delta_project_mma_token_tile
                } else {
                    project_token_tile
                }),
                1,
            ),
            (
                if use_mma && prefill_delta_project_qkvz_mma_rg4_ashared_enabled() {
                    128
                } else if use_mma {
                    32
                } else {
                    256
                },
                1,
                1,
            ),
        );
    }

    if fused_ba_activate {
        enc.set_pipeline(
            fused_ba_activate_pso
                .as_ref()
                .expect("fused b/a activation pso"),
        );
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, &layer.b, 0);
        enc.set_buffer(2, &layer.a, 0);
        enc.set_buffer(3, &layer.a_log, 0);
        enc.set_buffer(4, &layer.dt_bias, 0);
        enc.set_buffer(5, beta, 0);
        enc.set_buffer(6, decay, 0);
        enc.set_bytes(7, &tokens);
        enc.set_bytes(8, &row_tile);
        enc.set_bytes(9, &hidden_col_tile);
        enc.set_bytes(10, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                gate_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(4),
                1,
            ),
            (256, 1, 1),
        );
    }
    if profile_stop == PrefillDeltaFfnProfileStop::Project {
        return Ok(());
    }

    if conv_split_fused {
        enc.set_pipeline(conv_split_pso.as_ref().expect("conv split fused pso"));
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, &layer.conv_state, 0);
        enc.set_buffer(2, &layer.conv_weight, 0);
        enc.set_buffer(3, &layer.conv_bias, 0);
        enc.set_buffer(4, q_half, 0);
        enc.set_buffer(5, k_half, 0);
        enc.set_buffer(6, v_half, 0);
        enc.set_bytes(7, &tokens);
        enc.dispatch_threadgroups(
            (
                (tokens as usize).div_ceil(conv_split_token_block),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );

        enc.set_pipeline(
            conv_state_update_pso
                .as_ref()
                .expect("conv state update pso"),
        );
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, &layer.conv_state, 0);
        enc.set_bytes(2, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);
    } else {
        enc.set_pipeline(conv_pso.as_ref().expect("conv pso"));
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, &layer.conv_state, 0);
        enc.set_buffer(2, &layer.conv_weight, 0);
        enc.set_buffer(3, &layer.conv_bias, 0);
        enc.set_buffer(4, conv_out, 0);
        enc.set_bytes(5, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);

        enc.set_pipeline(split_pso.as_ref().expect("split pso"));
        enc.set_buffer(0, conv_out, 0);
        enc.set_buffer(1, q_half, 0);
        enc.set_buffer(2, k_half, 0);
        enc.set_buffer(3, v_half, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );
    }

    if !fused_ba_activate {
        enc.set_pipeline(&activate_pso);
        enc.set_buffer(0, b_out, 0);
        enc.set_buffer(1, a_out, 0);
        enc.set_buffer(2, &layer.a_log, 0);
        enc.set_buffer(3, &layer.dt_bias, 0);
        enc.set_buffer(4, beta, 0);
        enc.set_buffer(5, decay, 0);
        enc.set_bytes(6, &tokens);
        enc.dispatch_threads(tokens as usize * QWEN35_08B.deltanet_v_heads, 256);
    }
    if profile_stop == PrefillDeltaFfnProfileStop::ConvSplit {
        return Ok(());
    }

    if chunk_scan_f32x4 {
        enc.set_pipeline(
            chunk_scan_phase2_pso
                .as_ref()
                .expect("chunk scan phase2 pso"),
        );
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, k_half, 0);
        enc.set_buffer(2, v_half, 0);
        enc.set_buffer(3, beta, 0);
        enc.set_buffer(4, decay, 0);
        enc.set_buffer(5, qkv_out, 0);
        enc.set_buffer(6, chunk_scan_state, 0);
        enc.set_bytes(7, &tokens);
        enc.set_bytes(8, &chunk_scan_tokens);
        enc.dispatch_threadgroups(
            (
                (tokens as usize).div_ceil(chunk_scan_tokens as usize),
                QWEN35_08B.deltanet_v_heads,
                QWEN35_08B.deltanet_head_dim,
            ),
            (32, 1, 1),
        );

        enc.set_pipeline(
            chunk_scan_phase3_pso
                .as_ref()
                .expect("chunk scan phase3 pso"),
        );
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, k_half, 0);
        enc.set_buffer(2, beta, 0);
        enc.set_buffer(3, decay, 0);
        enc.set_buffer(4, &layer.recurrent_state, 0);
        enc.set_buffer(5, qkv_out, 0);
        enc.set_buffer(6, chunk_scan_state, 0);
        enc.set_buffer(7, delta, 0);
        enc.set_buffer(8, &layer.recurrent_state, 0);
        enc.set_bytes(9, &tokens);
        enc.set_bytes(10, &chunk_scan_tokens);
        enc.dispatch_threadgroups(
            (QWEN35_08B.deltanet_v_heads, QWEN35_08B.deltanet_head_dim, 1),
            (32, 1, 1),
        );
    } else {
        enc.set_pipeline(scan_pso.as_ref().expect("scan pso"));
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, k_half, 0);
        enc.set_buffer(2, v_half, 0);
        enc.set_buffer(3, beta, 0);
        enc.set_buffer(4, decay, 0);
        enc.set_buffer(5, &layer.recurrent_state, 0);
        if scan_gated_norm {
            enc.set_buffer(6, z_out, 0);
            enc.set_buffer(7, &layer.delta_norm, 0);
            enc.set_buffer(8, gated, 0);
            enc.set_bytes(9, &tokens);
        } else {
            enc.set_buffer(6, delta, 0);
            enc.set_bytes(7, &tokens);
        }
        let (grid, threads) = dispatch_prefill_delta_scan_shape_for_tokens(tokens as usize);
        enc.dispatch_threadgroups(grid, threads);
    }

    if !scan_gated_norm {
        enc.set_pipeline(gated_norm_pso.as_ref().expect("gated norm pso"));
        enc.set_buffer(0, delta, 0);
        enc.set_buffer(1, z_out, 0);
        enc.set_buffer(2, &layer.delta_norm, 0);
        enc.set_buffer(3, gated, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
            prefill_delta_gated_norm_threads(),
        );
    }
    if profile_stop == PrefillDeltaFfnProfileStop::ScanNorm {
        return Ok(());
    }

    enc.set_pipeline(&out_pso);
    enc.set_buffer(0, gated, 0);
    enc.set_buffer(1, &layer.delta_out, 0);
    if delta_out_mma {
        if delta_out_mma_residual_fused {
            enc.set_buffer(2, input, 0);
            enc.set_buffer(3, after_delta, 0);
            enc.set_bytes(4, &tokens);
            enc.set_bytes(5, &hidden_rows);
            enc.set_bytes(6, &row_tile);
            enc.set_bytes(7, &delta_out_col_tile);
            enc.set_bytes(8, &delta_out_col_tiles);
        } else {
            enc.set_buffer(2, delta_out, 0);
            enc.set_bytes(3, &tokens);
            enc.set_bytes(4, &hidden_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &delta_out_col_tile);
            enc.set_bytes(7, &delta_out_col_tiles);
        }
    } else {
        enc.set_buffer(2, input, 0);
        enc.set_buffer(3, after_delta, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &hidden_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &delta_out_col_tile);
        enc.set_bytes(8, &delta_out_col_tiles);
    }
    enc.dispatch_threadgroups(
        (
            hidden_rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(out_token_tile),
            1,
        ),
        (if delta_out_mma { 32 } else { 256 }, 1, 1),
    );
    if delta_out_mma && !delta_out_mma_residual_fused {
        enc.set_pipeline(
            delta_out_residual_pso
                .as_ref()
                .expect("delta out residual pso"),
        );
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, delta_out, 0);
        enc.set_buffer(2, after_delta, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threads(tokens as usize * QWEN35_08B.hidden_size, 256);
    }
    if profile_stop == PrefillDeltaFfnProfileStop::DeltaOut {
        return Ok(());
    }

    if ffn_gate_up_mma {
        enc.set_pipeline(ffn_norm_pso.as_ref().expect("ffn norm pso"));
        enc.set_buffer(0, after_delta, 0);
        enc.set_buffer(1, &layer.ffn_norm, 0);
        enc.set_buffer(2, q_half, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threadgroups((tokens as usize, 1, 1), (256, 1, 1));

        enc.set_pipeline(&gate_up_pso);
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, &layer.ffn_gate, 0);
        enc.set_buffer(2, &layer.ffn_up, 0);
        enc.set_buffer(3, ffn_act, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &intermediate_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &hidden_col_tile);
        enc.set_bytes(8, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                intermediate_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(gate_up_token_tile),
                1,
            ),
            (32, 1, 1),
        );
    } else {
        enc.set_pipeline(&gate_up_pso);
        enc.set_buffer(0, after_delta, 0);
        enc.set_buffer(1, &layer.ffn_norm, 0);
        enc.set_buffer(2, &layer.ffn_gate, 0);
        enc.set_buffer(3, &layer.ffn_up, 0);
        enc.set_buffer(4, ffn_act, 0);
        enc.set_bytes(5, &tokens);
        enc.set_bytes(6, &intermediate_rows);
        enc.set_bytes(7, &row_tile);
        enc.set_bytes(8, &hidden_col_tile);
        enc.set_bytes(9, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (
                intermediate_rows.div_ceil(4) as usize,
                (tokens as usize).div_ceil(gate_up_token_tile),
                1,
            ),
            (256, 1, 1),
        );
    }
    if profile_stop == PrefillDeltaFfnProfileStop::FfnGateUp {
        return Ok(());
    }

    enc.set_pipeline(&down_pso);
    enc.set_buffer(0, ffn_act, 0);
    enc.set_buffer(1, &layer.ffn_down, 0);
    if down_mma {
        if down_mma_residual_kernel.is_some() {
            enc.set_buffer(2, after_delta, 0);
            enc.set_buffer(3, output, 0);
            enc.set_bytes(4, &tokens);
            enc.set_bytes(5, &hidden_rows);
            enc.set_bytes(6, &row_tile);
            enc.set_bytes(7, &intermediate_col_tile);
            enc.set_bytes(8, &intermediate_col_tiles);
        } else {
            enc.set_buffer(2, ffn_out, 0);
            enc.set_bytes(3, &tokens);
            enc.set_bytes(4, &hidden_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &intermediate_col_tile);
            enc.set_bytes(7, &intermediate_col_tiles);
        }
        enc.dispatch_threadgroups(
            (
                hidden_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(down_token_tile),
                1,
            ),
            (prefill_down_threadgroup_threads(), 1, 1),
        );

        if down_mma_residual_kernel.is_none() {
            enc.set_pipeline(residual_pso.as_ref().expect("residual pso"));
            enc.set_buffer(0, after_delta, 0);
            enc.set_buffer(1, ffn_out, 0);
            enc.set_buffer(2, output, 0);
            enc.set_bytes(3, &tokens);
            enc.dispatch_threads(tokens as usize * QWEN35_08B.hidden_size, 256);
        }
    } else {
        enc.set_buffer(2, after_delta, 0);
        enc.set_buffer(3, output, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &hidden_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &intermediate_col_tile);
        enc.set_bytes(8, &intermediate_col_tiles);
        enc.dispatch_threadgroups(
            (
                hidden_rows.div_ceil(row_tile) as usize,
                (tokens as usize).div_ceil(down_token_tile),
                1,
            ),
            (256, 1, 1),
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_prefill_delta_projected_to_delta_out(
    dev: &Device,
    enc: &crate::metal::ffi::ComputeEncoder,
    input: &crate::metal::ffi::Buffer,
    layer: &PrefillDeltaFfnLayerDeviceBuffers,
    qkv_out: &crate::metal::ffi::Buffer,
    z_out: &crate::metal::ffi::Buffer,
    conv_out: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    decay: &crate::metal::ffi::Buffer,
    delta: &crate::metal::ffi::Buffer,
    chunk_scan_state: &crate::metal::ffi::Buffer,
    gated: &crate::metal::ffi::Buffer,
    delta_out: &crate::metal::ffi::Buffer,
    after_delta: &crate::metal::ffi::Buffer,
    tokens: u32,
    hidden_rows: u32,
    row_tile: u32,
    delta_out_col_tile: u32,
    delta_out_col_tiles: u32,
    profile_stop: PrefillDeltaFfnProfileStop,
    qkvz_direct: Option<&crate::metal::ffi::Buffer>,
) -> Result<(), String> {
    let conv_split_fused = prefill_delta_conv_split_fused_enabled();
    let conv_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_causal_conv1d_silu_c6144_k4")?)
    };
    let split_pso = if conv_split_fused {
        None
    } else {
        Some(dev.pipeline("qwen35_08b_prefill_deltanet_split_qkv_norm_tok_f32_to_fp16_h16d128")?)
    };
    let (conv_split_kernel, conv_split_token_block) = prefill_delta_conv_split_fused_kernel();
    let conv_split_token_block = if qkvz_direct.is_some() {
        1
    } else {
        conv_split_token_block
    };
    let conv_split_pso = if conv_split_fused {
        Some(dev.pipeline(if qkvz_direct.is_some() {
            "qwen35_08b_prefill_deltanet_conv_split_qkvz_norm_tok_f32_to_fp16_h16d128"
        } else {
            conv_split_kernel
        })?)
    } else {
        None
    };
    let conv_state_update_pso = if conv_split_fused {
        Some(dev.pipeline(if qkvz_direct.is_some() {
            "qwen35_08b_prefill_deltanet_conv_state_update_qkvz_c6144_k4"
        } else {
            "qwen35_08b_prefill_deltanet_conv_state_update_c6144_k4"
        })?)
    } else {
        None
    };
    let scan_gated_norm = prefill_delta_scan_gated_norm_enabled();
    let chunk_scan_f32x4 = prefill_delta_scan_chunk_f32x4_enabled() && !scan_gated_norm;
    let chunk_scan_hstate = prefill_delta_scan_chunk_hstate_enabled();
    let scan_pso = if chunk_scan_f32x4 {
        None
    } else if scan_gated_norm {
        Some(dev.pipeline(prefill_delta_scan_gated_norm_kernel())?)
    } else {
        Some(dev.pipeline(prefill_delta_scan_kernel_for_tokens(tokens as usize))?)
    };
    let chunk_scan_phase2_pso = if chunk_scan_f32x4 {
        Some(dev.pipeline(if chunk_scan_hstate {
            "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_hstate_h16d128"
        } else {
            "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_f32state_h16d128"
        })?)
    } else {
        None
    };
    let chunk_scan_phase3_pso = if chunk_scan_f32x4 {
        Some(dev.pipeline(if chunk_scan_hstate {
            "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_hstate_h16d128"
        } else {
            "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_f32state_h16d128"
        })?)
    } else {
        None
    };
    let chunk_scan_tokens =
        u32::try_from(prefill_delta_scan_chunk_tokens()).map_err(|_| "chunk tokens exceed u32")?;
    let gated_norm_pso = if scan_gated_norm {
        None
    } else {
        Some(dev.pipeline(prefill_delta_gated_norm_kernel(qkvz_direct.is_some()))?)
    };
    let requested_delta_out_mma = prefill_deltanet_out_mma_enabled();
    let (candidate_out_kernel, candidate_out_token_tile) = prefill_deltanet_out_matmul_kernel();
    let delta_out_mma = requested_delta_out_mma
        && tokens.is_multiple_of(candidate_out_token_tile as u32)
        && row_tile == 8;
    let (out_kernel, out_token_tile) = if delta_out_mma {
        (candidate_out_kernel, candidate_out_token_tile)
    } else {
        prefill_deltanet_out_residual_kernel()
    };
    let delta_out_mma_residual_fused = delta_out_mma
        && ((out_token_tile == 32 && prefill_deltanet_out_mma32_residual_enabled())
            || out_token_tile == 64);
    let out_pso =
        if delta_out_mma && out_token_tile == 32 && prefill_deltanet_out_mma32_residual_enabled() {
            dev.pipeline("qwen35_08b_prefill_deltanet_out_mma32x8_residual_fp16_tiled_k2048_f32")?
        } else {
            dev.pipeline(out_kernel)?
        };
    let delta_out_residual_pso = if delta_out_mma && !delta_out_mma_residual_fused {
        Some(dev.pipeline("qwen35_08b_prefill_residual_add_f32_to_fp16_k1024")?)
    } else {
        None
    };

    if conv_split_fused {
        enc.set_pipeline(conv_split_pso.as_ref().expect("conv split fused pso"));
        enc.set_buffer(0, qkvz_direct.unwrap_or(qkv_out), 0);
        enc.set_buffer(1, &layer.conv_state, 0);
        enc.set_buffer(2, &layer.conv_weight, 0);
        enc.set_buffer(3, &layer.conv_bias, 0);
        enc.set_buffer(4, q_half, 0);
        enc.set_buffer(5, k_half, 0);
        enc.set_buffer(6, v_half, 0);
        enc.set_bytes(7, &tokens);
        enc.dispatch_threadgroups(
            (
                (tokens as usize).div_ceil(conv_split_token_block),
                QWEN35_08B.deltanet_v_heads,
                1,
            ),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );

        enc.set_pipeline(
            conv_state_update_pso
                .as_ref()
                .expect("conv state update pso"),
        );
        enc.set_buffer(0, qkvz_direct.unwrap_or(qkv_out), 0);
        enc.set_buffer(1, &layer.conv_state, 0);
        enc.set_bytes(2, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);
    } else {
        enc.set_pipeline(conv_pso.as_ref().expect("conv pso"));
        enc.set_buffer(0, qkv_out, 0);
        enc.set_buffer(1, &layer.conv_state, 0);
        enc.set_buffer(2, &layer.conv_weight, 0);
        enc.set_buffer(3, &layer.conv_bias, 0);
        enc.set_buffer(4, conv_out, 0);
        enc.set_bytes(5, &tokens);
        enc.dispatch_threads(QWEN35_08B.deltanet_qkv_width(), 256);

        enc.set_pipeline(split_pso.as_ref().expect("split pso"));
        enc.set_buffer(0, conv_out, 0);
        enc.set_buffer(1, q_half, 0);
        enc.set_buffer(2, k_half, 0);
        enc.set_buffer(3, v_half, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
            (QWEN35_08B.deltanet_head_dim, 1, 1),
        );
    }
    if profile_stop == PrefillDeltaFfnProfileStop::ConvSplit {
        return Ok(());
    }

    if chunk_scan_f32x4 {
        enc.set_pipeline(
            chunk_scan_phase2_pso
                .as_ref()
                .expect("chunk scan phase2 pso"),
        );
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, k_half, 0);
        enc.set_buffer(2, v_half, 0);
        enc.set_buffer(3, beta, 0);
        enc.set_buffer(4, decay, 0);
        enc.set_buffer(5, qkv_out, 0);
        enc.set_buffer(6, chunk_scan_state, 0);
        enc.set_bytes(7, &tokens);
        enc.set_bytes(8, &chunk_scan_tokens);
        enc.dispatch_threadgroups(
            (
                (tokens as usize).div_ceil(chunk_scan_tokens as usize),
                QWEN35_08B.deltanet_v_heads,
                QWEN35_08B.deltanet_head_dim,
            ),
            (32, 1, 1),
        );

        enc.set_pipeline(
            chunk_scan_phase3_pso
                .as_ref()
                .expect("chunk scan phase3 pso"),
        );
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, k_half, 0);
        enc.set_buffer(2, beta, 0);
        enc.set_buffer(3, decay, 0);
        enc.set_buffer(4, &layer.recurrent_state, 0);
        enc.set_buffer(5, qkv_out, 0);
        enc.set_buffer(6, chunk_scan_state, 0);
        enc.set_buffer(7, delta, 0);
        enc.set_buffer(8, &layer.recurrent_state, 0);
        enc.set_bytes(9, &tokens);
        enc.set_bytes(10, &chunk_scan_tokens);
        enc.dispatch_threadgroups(
            (QWEN35_08B.deltanet_v_heads, QWEN35_08B.deltanet_head_dim, 1),
            (32, 1, 1),
        );
    } else {
        enc.set_pipeline(scan_pso.as_ref().expect("scan pso"));
        enc.set_buffer(0, q_half, 0);
        enc.set_buffer(1, k_half, 0);
        enc.set_buffer(2, v_half, 0);
        enc.set_buffer(3, beta, 0);
        enc.set_buffer(4, decay, 0);
        enc.set_buffer(5, &layer.recurrent_state, 0);
        if scan_gated_norm {
            enc.set_buffer(6, z_out, 0);
            enc.set_buffer(7, &layer.delta_norm, 0);
            enc.set_buffer(8, gated, 0);
            enc.set_bytes(9, &tokens);
        } else {
            enc.set_buffer(6, delta, 0);
            enc.set_bytes(7, &tokens);
        }
        let (grid, threads) = dispatch_prefill_delta_scan_shape_for_tokens(tokens as usize);
        enc.dispatch_threadgroups(grid, threads);
    }

    if !scan_gated_norm {
        enc.set_pipeline(gated_norm_pso.as_ref().expect("gated norm pso"));
        enc.set_buffer(0, delta, 0);
        enc.set_buffer(1, qkvz_direct.unwrap_or(z_out), 0);
        enc.set_buffer(2, &layer.delta_norm, 0);
        enc.set_buffer(3, gated, 0);
        enc.set_bytes(4, &tokens);
        enc.dispatch_threadgroups(
            (tokens as usize, QWEN35_08B.deltanet_v_heads, 1),
            prefill_delta_gated_norm_threads(),
        );
    }
    if profile_stop == PrefillDeltaFfnProfileStop::ScanNorm {
        return Ok(());
    }

    enc.set_pipeline(&out_pso);
    enc.set_buffer(0, gated, 0);
    enc.set_buffer(1, &layer.delta_out, 0);
    if delta_out_mma {
        if delta_out_mma_residual_fused {
            enc.set_buffer(2, input, 0);
            enc.set_buffer(3, after_delta, 0);
            enc.set_bytes(4, &tokens);
            enc.set_bytes(5, &hidden_rows);
            enc.set_bytes(6, &row_tile);
            enc.set_bytes(7, &delta_out_col_tile);
            enc.set_bytes(8, &delta_out_col_tiles);
        } else {
            enc.set_buffer(2, delta_out, 0);
            enc.set_bytes(3, &tokens);
            enc.set_bytes(4, &hidden_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &delta_out_col_tile);
            enc.set_bytes(7, &delta_out_col_tiles);
        }
    } else {
        enc.set_buffer(2, input, 0);
        enc.set_buffer(3, after_delta, 0);
        enc.set_bytes(4, &tokens);
        enc.set_bytes(5, &hidden_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &delta_out_col_tile);
        enc.set_bytes(8, &delta_out_col_tiles);
    }
    enc.dispatch_threadgroups(
        (
            hidden_rows.div_ceil(row_tile) as usize,
            (tokens as usize).div_ceil(out_token_tile),
            1,
        ),
        (if delta_out_mma { 32 } else { 256 }, 1, 1),
    );
    if delta_out_mma && !delta_out_mma_residual_fused {
        enc.set_pipeline(
            delta_out_residual_pso
                .as_ref()
                .expect("delta out residual pso"),
        );
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, delta_out, 0);
        enc.set_buffer(2, after_delta, 0);
        enc.set_bytes(3, &tokens);
        enc.dispatch_threads(tokens as usize * QWEN35_08B.hidden_size, 256);
    }

    Ok(())
}

pub fn run_deltanet_decay_activation_bench(
    config: DeltaNetDecayActivationBenchConfig,
) -> Result<DeltaNetDecayActivationBenchResult, String> {
    let heads = QWEN35_08B.deltanet_v_heads;
    let dev = Device::default_system()?;
    let beta_raw = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let alpha_raw = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let a_log = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let dt_bias = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let beta = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(heads * std::mem::size_of::<f32>())?;

    let beta_raw_host = (0..heads)
        .map(|i| ((i as f32 * 1.37) - 8.0) / 3.0)
        .collect::<Vec<_>>();
    let alpha_raw_host = (0..heads)
        .map(|i| ((i as f32 * 0.91) - 5.0) / 2.5)
        .collect::<Vec<_>>();
    let a_log_host = (0..heads)
        .map(|i| -3.0 + (i as f32) * 0.06)
        .collect::<Vec<_>>();
    let dt_bias_host = (0..heads)
        .map(|i| -0.25 + (i as f32) * 0.03)
        .collect::<Vec<_>>();

    let beta_ref = beta_raw_host
        .iter()
        .map(|value| sigmoid_f32(*value))
        .collect::<Vec<_>>();
    let decay_ref = alpha_raw_host
        .iter()
        .zip(a_log_host.iter())
        .zip(dt_bias_host.iter())
        .map(|((alpha, a), bias)| (-a.exp() * softplus_f32(*alpha + *bias)).exp())
        .collect::<Vec<_>>();

    unsafe {
        beta_raw.write(0, &beta_raw_host);
        alpha_raw.write(0, &alpha_raw_host);
        a_log.write(0, &a_log_host);
        dt_bias.write(0, &dt_bias_host);
    }

    for _ in 0..config.warmup {
        dispatch_deltanet_decay_activation_once(
            &dev, &beta_raw, &alpha_raw, &a_log, &dt_bias, &beta, &decay,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_deltanet_decay_activation_once(
            &dev, &beta_raw, &alpha_raw, &a_log, &dt_bias, &beta, &decay,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut beta_out = vec![0.0f32; heads];
    let mut decay_out = vec![0.0f32; heads];
    unsafe {
        beta.read(0, &mut beta_out);
        decay.read(0, &mut decay_out);
    }
    let max_abs_error_beta = max_abs_error(&beta_out, &beta_ref);
    let max_abs_error_decay = max_abs_error(&decay_out, &decay_ref);
    let checksum = beta_out.iter().chain(decay_out.iter()).copied().sum();
    Ok(DeltaNetDecayActivationBenchResult {
        heads,
        iterations: config.iterations,
        median_s: percentile_sorted(&samples, 0.50),
        p95_s: percentile_sorted(&samples, 0.95),
        max_abs_error_beta,
        max_abs_error_decay,
        checksum,
    })
}

#[allow(clippy::too_many_arguments)]
fn dispatch_deltanet_decay_activation_once(
    dev: &Device,
    beta_raw: &crate::metal::ffi::Buffer,
    alpha_raw: &crate::metal::ffi::Buffer,
    a_log: &crate::metal::ffi::Buffer,
    dt_bias: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    decay: &crate::metal::ffi::Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_deltanet_activate_beta_decay_h16")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, beta_raw, 0);
    enc.set_buffer(1, alpha_raw, 0);
    enc.set_buffer(2, a_log, 0);
    enc.set_buffer(3, dt_bias, 0);
    enc.set_buffer(4, beta, 0);
    enc.set_buffer(5, decay, 0);
    enc.dispatch_threads(QWEN35_08B.deltanet_v_heads, 16);
    enc.end();
    cmd.commit_and_wait()
}

fn sigmoid_f32(value: f32) -> f32 {
    let clamped = value.clamp(-20.0, 20.0);
    1.0 / (1.0 + (-clamped).exp())
}

fn softplus_f32(value: f32) -> f32 {
    let clamped = value.clamp(-20.0, 20.0);
    if clamped > 20.0 {
        clamped
    } else {
        (1.0 + clamped.exp()).ln()
    }
}

fn max_abs_error(actual: &[f32], expected: &[f32]) -> f32 {
    actual
        .iter()
        .zip(expected.iter())
        .map(|(actual, expected)| (actual - expected).abs())
        .fold(0.0, f32::max)
}

pub fn run_ffn_swiglu_bench(config: FfnSwiGluBenchConfig) -> Result<FfnSwiGluBenchResult, String> {
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    let dev = Device::default_system()?;
    let x = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let gate_w = dev.new_buffer(intermediate * hidden * std::mem::size_of::<u16>())?;
    let up_w = dev.new_buffer(intermediate * hidden * std::mem::size_of::<u16>())?;
    let down_w = dev.new_buffer(hidden * intermediate * std::mem::size_of::<u16>())?;
    let y = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;

    let x_host: Vec<u16> = (0..hidden)
        .map(|i| f16::from_f32(((i % 97) as f32 - 48.0) / 97.0).to_bits())
        .collect();
    let gate_host: Vec<u16> = (0..intermediate * hidden)
        .map(|i| {
            let v = ((i.wrapping_mul(13) % 251) as f32 - 125.0) / 512.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let up_host: Vec<u16> = (0..intermediate * hidden)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 512.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let down_host: Vec<u16> = (0..hidden * intermediate)
        .map(|i| {
            let v = ((i.wrapping_mul(19) % 263) as f32 - 131.0) / 1024.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        x.write(0, &x_host);
        gate_w.write(0, &gate_host);
        up_w.write(0, &up_host);
        down_w.write(0, &down_host);
    }

    for _ in 0..config.warmup {
        dispatch_ffn_swiglu_once(&dev, &x, &gate_w, &up_w, &down_w, &y)?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_ffn_swiglu_once(&dev, &x, &gate_w, &up_w, &down_w, &y)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut first = vec![0.0f32; hidden.min(32)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().fold(0.0f32, |acc, v| acc + *v);
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let bytes_moved = (intermediate * hidden * 2 * 2)
        + (hidden * intermediate * 2)
        + hidden * std::mem::size_of::<u16>()
        + hidden * std::mem::size_of::<f32>();
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(FfnSwiGluBenchResult {
        hidden,
        intermediate,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        checksum,
    })
}

fn dispatch_ffn_swiglu_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    gate_w: &crate::metal::ffi::Buffer,
    up_w: &crate::metal::ffi::Buffer,
    down_w: &crate::metal::ffi::Buffer,
    y: &crate::metal::ffi::Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_ffn_swiglu_fp16")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, gate_w, 0);
    enc.set_buffer(2, up_w, 0);
    enc.set_buffer(3, down_w, 0);
    enc.set_buffer(4, y, 0);
    enc.dispatch_threadgroups((1, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

pub fn run_lm_head_argmax_bench(
    config: LmHeadArgmaxBenchConfig,
) -> Result<LmHeadArgmaxBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    if rows == 0 {
        return Err("LM-head rows must be > 0".to_string());
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let x_host: Vec<u16> = (0..cols)
        .map(|i| f16::from_f32(((i % 113) as f32 - 56.0) / 113.0).to_bits())
        .collect();
    let w_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 257.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        x.write(0, &x_host);
        w.write(0, &w_host);
    }

    for _ in 0..config.warmup {
        dispatch_lm_head_argmax_once(&dev, &x, &w, &scores_a, &ids_a, &scores_b, &ids_b, rows_u32)?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_lm_head_argmax_once(
            &dev, &x, &w, &scores_a, &ids_a, &scores_b, &ids_b, rows_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let bytes_moved = rows * cols * 2 + cols * 2 + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(LmHeadArgmaxBenchResult {
        vocab_rows: rows,
        cols,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
    })
}

pub fn run_lm_head_argmax_tiled_bench(
    config: LmHeadArgmaxTiledBenchConfig,
) -> Result<LmHeadArgmaxTiledBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    let x_host: Vec<u16> = (0..cols)
        .map(|i| f16::from_f32(((i % 113) as f32 - 56.0) / 113.0).to_bits())
        .collect();
    let w_row_major: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 257.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let w_tiled =
        pack_row_major_fp16_to_tiles(&w_row_major, rows, cols, config.row_tile, config.col_tile);

    run_lm_head_argmax_tiled_with_weights(config, &x_host, &w_tiled)
}

pub fn run_lm_head_argmax_tiled_with_weights(
    config: LmHeadArgmaxTiledBenchConfig,
    x_host: &[u16],
    w_tiled: &[u16],
) -> Result<LmHeadArgmaxTiledBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    if rows == 0 {
        return Err("LM-head rows must be > 0".to_string());
    }
    if config.row_tile == 0 || config.col_tile == 0 {
        return Err("row_tile and col_tile must be > 0".to_string());
    }
    if config.row_tile != 8 {
        return Err(
            "tiled LM-head row-tile benchmark currently specializes row_tile=8".to_string(),
        );
    }
    if !cols.is_multiple_of(config.col_tile) {
        return Err(format!(
            "hidden size {cols} must be a multiple of col_tile {} for this benchmark",
            config.col_tile
        ));
    }
    if x_host.len() != cols {
        return Err(format!(
            "x_host length must be {cols}, got {}",
            x_host.len()
        ));
    }
    let expected_weights =
        round_up_usize(rows, config.row_tile) * round_up_usize(cols, config.col_tile);
    if w_tiled.len() != expected_weights {
        return Err(format!(
            "w_tiled length must be {expected_weights}, got {}",
            w_tiled.len()
        ));
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(config.row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(config.col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles = cols.div_ceil(config.col_tile);
    let n_col_tiles_u32 = u32::try_from(n_col_tiles).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let x = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(w_tiled.len() * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    unsafe {
        x.write(0, &x_host);
        w.write(0, &w_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_lm_head_argmax_tiled_once(
            &dev,
            &x,
            &w,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_lm_head_argmax_tiled_once(
            &dev,
            &x,
            &w,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let packed_weight_bytes = w_tiled.len() * std::mem::size_of::<u16>();
    let bytes_moved = packed_weight_bytes + cols * 2 + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(LmHeadArgmaxTiledBenchResult {
        vocab_rows: rows,
        cols,
        row_tile: config.row_tile,
        col_tile: config.col_tile,
        packed_weight_bytes,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
    })
}

#[allow(clippy::too_many_arguments)]
fn dispatch_lm_head_argmax_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    rows: u32,
) -> Result<bool, String> {
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_pairs_fp16_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &rows);
    enc.dispatch_threadgroups((rows as usize, 1, 1), (256, 1, 1));

    let mut n = rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_lm_head_argmax_tiled_once(
    dev: &Device,
    x: &crate::metal::ffi::Buffer,
    w: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<bool, String> {
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups((rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

pub fn run_decode_skeleton_bench(
    config: DecodeSkeletonBenchConfig,
) -> Result<DecodeSkeletonBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    if rows == 0 {
        return Err("decode skeleton vocab rows must be > 0".to_string());
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding_and_lm_head = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let hidden = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    let w_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 257.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        token.write(0, &token_host);
        embedding_and_lm_head.write(0, &w_host);
    }

    for _ in 0..config.warmup {
        dispatch_decode_skeleton_once(
            &dev,
            &token,
            &embedding_and_lm_head,
            &hidden,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_skeleton_once(
            &dev,
            &token,
            &embedding_and_lm_head,
            &hidden,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let bytes_moved = rows * cols * 2 + cols * 2 + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

pub fn run_decode_skeleton_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    embedding_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    validate_tiled_matvec_shape(rows, cols, row_tile, col_tile)?;
    let expected_weights = round_up_usize(rows, row_tile) * round_up_usize(cols, col_tile);
    if embedding_tiled.len() != expected_weights {
        return Err(format!(
            "embedding_tiled length must be {expected_weights}, got {}",
            embedding_tiled.len()
        ));
    }
    if lm_head_tiled.len() != expected_weights {
        return Err(format!(
            "lm_head_tiled length must be {expected_weights}, got {}",
            lm_head_tiled.len()
        ));
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        lm_head.write(0, lm_head_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_decode_skeleton_tiled_once(
            &dev,
            &token,
            &embedding,
            &lm_head,
            &hidden,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_skeleton_tiled_once(
            &dev,
            &token,
            &embedding,
            &lm_head,
            &hidden,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let bytes_moved = lm_head_tiled.len() * 2 + cols * 2 + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_one_projection_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    projection_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    validate_tiled_matvec_shape(rows, cols, row_tile, col_tile)?;
    validate_tiled_matvec_shape(cols, cols, row_tile, col_tile)?;
    if norm_weight.len() != cols {
        return Err(format!(
            "norm_weight length must be {cols}, got {}",
            norm_weight.len()
        ));
    }
    let vocab_weights = round_up_usize(rows, row_tile) * round_up_usize(cols, col_tile);
    let projection_weights = round_up_usize(cols, row_tile) * round_up_usize(cols, col_tile);
    if embedding_tiled.len() != vocab_weights {
        return Err(format!(
            "embedding_tiled length must be {vocab_weights}, got {}",
            embedding_tiled.len()
        ));
    }
    if lm_head_tiled.len() != vocab_weights {
        return Err(format!(
            "lm_head_tiled length must be {vocab_weights}, got {}",
            lm_head_tiled.len()
        ));
    }
    if projection_tiled.len() != projection_weights {
        return Err(format!(
            "projection_tiled length must be {projection_weights}, got {}",
            projection_tiled.len()
        ));
    }

    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let hidden_rows_u32 = u32::try_from(cols).map_err(|_| "hidden rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let n_col_tiles_u32 =
        u32::try_from(cols.div_ceil(col_tile)).map_err(|_| "n_col_tiles exceeds u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let projection = dev.new_buffer(projection_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden_a = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(cols * std::mem::size_of::<f32>())?;
    let hidden_b = dev.new_buffer(cols * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        projection.write(0, projection_tiled);
        lm_head.write(0, lm_head_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_decode_one_projection_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &projection,
            &lm_head,
            &hidden_a,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
            hidden_rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_one_projection_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &projection,
            &lm_head,
            &hidden_a,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            rows_u32,
            hidden_rows_u32,
            row_tile_u32,
            col_tile_u32,
            n_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let bytes_moved =
        projection_tiled.len() * 2 + lm_head_tiled.len() * 2 + cols * 2 + cols * 4 + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_ffn_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    gate_tiled: &[u16],
    up_tiled: &[u16],
    down_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    let rows = config.vocab_rows;
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(intermediate, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, intermediate, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }

    let vocab_weights = round_up_usize(rows, row_tile) * round_up_usize(hidden, col_tile);
    let up_gate_weights = round_up_usize(intermediate, row_tile) * round_up_usize(hidden, col_tile);
    let down_weights = round_up_usize(hidden, row_tile) * round_up_usize(intermediate, col_tile);
    if embedding_tiled.len() != vocab_weights {
        return Err(format!(
            "embedding_tiled length must be {vocab_weights}, got {}",
            embedding_tiled.len()
        ));
    }
    if lm_head_tiled.len() != vocab_weights {
        return Err(format!(
            "lm_head_tiled length must be {vocab_weights}, got {}",
            lm_head_tiled.len()
        ));
    }
    if gate_tiled.len() != up_gate_weights {
        return Err(format!(
            "gate_tiled length must be {up_gate_weights}, got {}",
            gate_tiled.len()
        ));
    }
    if up_tiled.len() != up_gate_weights {
        return Err(format!(
            "up_tiled length must be {up_gate_weights}, got {}",
            up_tiled.len()
        ));
    }
    if down_tiled.len() != down_weights {
        return Err(format!(
            "down_tiled length must be {down_weights}, got {}",
            down_tiled.len()
        ));
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let intermediate_rows_u32 =
        u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let intermediate_col_tiles_u32 = u32::try_from(intermediate.div_ceil(col_tile))
        .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let gate = dev.new_buffer(gate_tiled.len() * std::mem::size_of::<u16>())?;
    let up = dev.new_buffer(up_tiled.len() * std::mem::size_of::<u16>())?;
    let down = dev.new_buffer(down_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let gate_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let up_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let act = dev.new_buffer(intermediate * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        gate.write(0, gate_tiled);
        up.write(0, up_tiled);
        down.write(0, down_tiled);
        lm_head.write(0, lm_head_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_decode_ffn_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &gate,
            &up,
            &down,
            &lm_head,
            &hidden_a,
            &gate_f32,
            &up_f32,
            &act,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            intermediate_rows_u32,
            hidden_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_ffn_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &gate,
            &up,
            &down,
            &lm_head,
            &hidden_a,
            &gate_f32,
            &up_f32,
            &act,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            intermediate_rows_u32,
            hidden_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let bytes_moved = (gate_tiled.len() + up_tiled.len() + down_tiled.len() + lm_head_tiled.len())
        * 2
        + intermediate * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>())
        + hidden * (std::mem::size_of::<u16>() * 2 + std::mem::size_of::<f32>())
        + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_repeated_ffn_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    ffn_layers: usize,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    gate_tiled: &[u16],
    up_tiled: &[u16],
    down_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    if ffn_layers == 0 {
        return Err("ffn_layers must be > 0".to_string());
    }
    let rows = config.vocab_rows;
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(intermediate, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, intermediate, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }

    let vocab_weights = round_up_usize(rows, row_tile) * round_up_usize(hidden, col_tile);
    let up_gate_weights = round_up_usize(intermediate, row_tile) * round_up_usize(hidden, col_tile);
    let down_weights = round_up_usize(hidden, row_tile) * round_up_usize(intermediate, col_tile);
    if embedding_tiled.len() != vocab_weights || lm_head_tiled.len() != vocab_weights {
        return Err("embedding/lm_head tiled length does not match vocab shape".to_string());
    }
    if gate_tiled.len() != up_gate_weights
        || up_tiled.len() != up_gate_weights
        || down_tiled.len() != down_weights
    {
        return Err("FFN tiled weight length does not match Qwen FFN shape".to_string());
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let intermediate_rows_u32 =
        u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let ffn_layers_u32 = u32::try_from(ffn_layers).map_err(|_| "ffn_layers exceeds u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let intermediate_col_tiles_u32 = u32::try_from(intermediate.div_ceil(col_tile))
        .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let gate = dev.new_buffer(gate_tiled.len() * std::mem::size_of::<u16>())?;
    let up = dev.new_buffer(up_tiled.len() * std::mem::size_of::<u16>())?;
    let down = dev.new_buffer(down_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let gate_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let up_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let act = dev.new_buffer(intermediate * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        gate.write(0, gate_tiled);
        up.write(0, up_tiled);
        down.write(0, down_tiled);
        lm_head.write(0, lm_head_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_decode_repeated_ffn_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &gate,
            &up,
            &down,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &gate_f32,
            &up_f32,
            &act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            intermediate_rows_u32,
            hidden_rows_u32,
            ffn_layers_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_repeated_ffn_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &gate,
            &up,
            &down,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &gate_f32,
            &up_f32,
            &act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            intermediate_rows_u32,
            hidden_rows_u32,
            ffn_layers_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            intermediate_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let ffn_bytes = (gate_tiled.len() + up_tiled.len() + down_tiled.len()) * 2
        + intermediate * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>())
        + hidden * (std::mem::size_of::<u16>() * 2 + std::mem::size_of::<f32>());
    let bytes_moved = ffn_bytes * ffn_layers + lm_head_tiled.len() * 2 + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_attention_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    q_tiled: &[u16],
    k_tiled: &[u16],
    v_tiled: &[u16],
    o_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    let rows = config.vocab_rows;
    let hidden = QWEN35_08B.hidden_size;
    let attention_width = QWEN35_08B.attention_q_width();
    let attention_kv_width = QWEN35_08B.attention_kv_width();
    let attention_q_rows = expected_attention_q_rows(q_tiled.len(), row_tile, col_tile)?;
    let max_context = config.max_context.max(1);
    if usize::try_from(config.decode_position).map_err(|_| "decode position exceeds usize")?
        >= max_context
    {
        return Err(format!(
            "decode_position {} must be < max_context {max_context}",
            config.decode_position
        ));
    }
    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_q_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_kv_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, attention_width, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }
    let vocab_weights = tiled_matvec_len(rows, hidden, row_tile, col_tile);
    let q_weights = tiled_matvec_len(attention_q_rows, hidden, row_tile, col_tile);
    let kv_weights = tiled_matvec_len(attention_kv_width, hidden, row_tile, col_tile);
    let o_weights = tiled_matvec_len(hidden, attention_width, row_tile, col_tile);
    if embedding_tiled.len() != vocab_weights || lm_head_tiled.len() != vocab_weights {
        return Err("embedding/lm_head tiled length does not match vocab shape".to_string());
    }
    if q_tiled.len() != q_weights {
        return Err(format!(
            "q_tiled length must be {q_weights}, got {}",
            q_tiled.len()
        ));
    }
    if k_tiled.len() != kv_weights || v_tiled.len() != kv_weights {
        return Err(format!(
            "k/v tiled length must be {kv_weights}, got {}/{}",
            k_tiled.len(),
            v_tiled.len()
        ));
    }
    if o_tiled.len() != o_weights {
        return Err(format!(
            "o_tiled length must be {o_weights}, got {}",
            o_tiled.len()
        ));
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let attention_q_rows_u32 =
        u32::try_from(attention_q_rows).map_err(|_| "attention q rows exceed u32")?;
    let attention_kv_rows_u32 =
        u32::try_from(attention_kv_width).map_err(|_| "attention kv rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let attention_col_tiles_u32 = u32::try_from(attention_width.div_ceil(col_tile))
        .map_err(|_| "attention col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let q = dev.new_buffer(q_tiled.len() * std::mem::size_of::<u16>())?;
    let k = dev.new_buffer(k_tiled.len() * std::mem::size_of::<u16>())?;
    let v = dev.new_buffer(v_tiled.len() * std::mem::size_of::<u16>())?;
    let o = dev.new_buffer(o_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let q_f32 = dev.new_buffer(attention_q_rows * std::mem::size_of::<f32>())?;
    let k_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let v_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let k_cache = dev.new_buffer(max_context * attention_kv_width * std::mem::size_of::<u16>())?;
    let v_cache = dev.new_buffer(max_context * attention_kv_width * std::mem::size_of::<u16>())?;
    let attn = dev.new_buffer(attention_width * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        q.write(0, q_tiled);
        k.write(0, k_tiled);
        v.write(0, v_tiled);
        o.write(0, o_tiled);
        lm_head.write(0, lm_head_tiled);
    }

    for _ in 0..config.warmup {
        dispatch_decode_attention_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &q,
            &k,
            &v,
            &o,
            &lm_head,
            &hidden_a,
            &q_f32,
            &k_f32,
            &v_f32,
            &k_cache,
            &v_cache,
            &attn,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            hidden_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            config.decode_position,
            u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_attention_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &q,
            &k,
            &v,
            &o,
            &lm_head,
            &hidden_a,
            &q_f32,
            &k_f32,
            &v_f32,
            &k_cache,
            &v_cache,
            &attn,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            hidden_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            config.decode_position,
            u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let bytes_moved = (q_tiled.len() + k_tiled.len() + v_tiled.len() + o_tiled.len()) * 2
        + lm_head_tiled.len() * 2
        + (attention_q_rows + attention_kv_width * 2 + hidden) * std::mem::size_of::<f32>()
        + (attention_width + max_context * attention_kv_width * 2 + hidden * 2)
            * std::mem::size_of::<u16>()
        + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_attention_tiled_sequence_with_weights(
    config: DecodeSkeletonBenchConfig,
    steps: usize,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    q_tiled: &[u16],
    k_tiled: &[u16],
    v_tiled: &[u16],
    o_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSequenceBenchResult, String> {
    if steps == 0 {
        return Err("sequence steps must be > 0".to_string());
    }
    let rows = config.vocab_rows;
    let hidden = QWEN35_08B.hidden_size;
    let attention_width = QWEN35_08B.attention_q_width();
    let attention_kv_width = QWEN35_08B.attention_kv_width();
    let attention_q_rows = expected_attention_q_rows(q_tiled.len(), row_tile, col_tile)?;
    let max_context = config.max_context.max(steps);
    if steps > max_context {
        return Err(format!(
            "steps {steps} must be <= max_context {max_context}"
        ));
    }
    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_q_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_kv_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, attention_width, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }

    let vocab_weights = tiled_matvec_len(rows, hidden, row_tile, col_tile);
    let q_weights = tiled_matvec_len(attention_q_rows, hidden, row_tile, col_tile);
    let kv_weights = tiled_matvec_len(attention_kv_width, hidden, row_tile, col_tile);
    let o_weights = tiled_matvec_len(hidden, attention_width, row_tile, col_tile);
    if embedding_tiled.len() != vocab_weights || lm_head_tiled.len() != vocab_weights {
        return Err("embedding/lm_head tiled length does not match vocab shape".to_string());
    }
    if q_tiled.len() != q_weights {
        return Err(format!(
            "q_tiled length must be {q_weights}, got {}",
            q_tiled.len()
        ));
    }
    if k_tiled.len() != kv_weights || v_tiled.len() != kv_weights {
        return Err(format!(
            "k/v tiled length must be {kv_weights}, got {}/{}",
            k_tiled.len(),
            v_tiled.len()
        ));
    }
    if o_tiled.len() != o_weights {
        return Err(format!(
            "o_tiled length must be {o_weights}, got {}",
            o_tiled.len()
        ));
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let attention_q_rows_u32 =
        u32::try_from(attention_q_rows).map_err(|_| "attention q rows exceed u32")?;
    let attention_kv_rows_u32 =
        u32::try_from(attention_kv_width).map_err(|_| "attention kv rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let attention_col_tiles_u32 = u32::try_from(attention_width.div_ceil(col_tile))
        .map_err(|_| "attention col tiles exceed u32")?;
    let max_context_u32 = u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let q = dev.new_buffer(q_tiled.len() * std::mem::size_of::<u16>())?;
    let k = dev.new_buffer(k_tiled.len() * std::mem::size_of::<u16>())?;
    let v = dev.new_buffer(v_tiled.len() * std::mem::size_of::<u16>())?;
    let o = dev.new_buffer(o_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let q_f32 = dev.new_buffer(attention_q_rows * std::mem::size_of::<f32>())?;
    let k_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let v_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let k_cache = dev.new_buffer(max_context * attention_kv_width * std::mem::size_of::<u16>())?;
    let v_cache = dev.new_buffer(max_context * attention_kv_width * std::mem::size_of::<u16>())?;
    let attn = dev.new_buffer(attention_width * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    unsafe {
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        q.write(0, q_tiled);
        k.write(0, k_tiled);
        v.write(0, v_tiled);
        o.write(0, o_tiled);
        lm_head.write(0, lm_head_tiled);
    }

    for _ in 0..config.warmup {
        run_attention_sequence_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &q,
            &k,
            &v,
            &o,
            &lm_head,
            &hidden_a,
            &q_f32,
            &k_f32,
            &v_f32,
            &k_cache,
            &v_cache,
            &attn,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            config.input_token,
            steps,
            vocab_rows_u32,
            hidden_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            max_context_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_tokens = Vec::new();
    let mut last_score = 0.0f32;
    for _ in 0..config.iterations {
        let start = Instant::now();
        let (tokens, score) = run_attention_sequence_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &q,
            &k,
            &v,
            &o,
            &lm_head,
            &hidden_a,
            &q_f32,
            &k_f32,
            &v_f32,
            &k_cache,
            &v_cache,
            &attn,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            config.input_token,
            steps,
            vocab_rows_u32,
            hidden_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            max_context_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
        final_tokens = tokens;
        last_score = score;
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let bytes_per_step = (q_tiled.len() + k_tiled.len() + v_tiled.len() + o_tiled.len()) * 2
        + lm_head_tiled.len() * 2
        + (attention_q_rows + attention_kv_width * 2 + hidden) * std::mem::size_of::<f32>()
        + (attention_width + hidden * 2) * std::mem::size_of::<u16>()
        + pair_bytes * 2;
    let cache_bytes = max_context * attention_kv_width * 2 * std::mem::size_of::<u16>();
    let effective_gb_s = (bytes_per_step * steps + cache_bytes) as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSequenceBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        steps,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        tokens: final_tokens,
        last_score,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_deltanet_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    qkv_tiled: &[u16],
    z_tiled: &[u16],
    b_tiled: &[u16],
    a_tiled: &[u16],
    out_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    let rows = config.vocab_rows;
    let hidden = QWEN35_08B.hidden_size;
    let delta_width = QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim;
    let qkv_rows = delta_width * 3;
    let gate_rows = QWEN35_08B.deltanet_v_heads;

    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(qkv_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(gate_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, delta_width, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }

    let vocab_weights = round_up_usize(rows, row_tile) * round_up_usize(hidden, col_tile);
    let qkv_weights = round_up_usize(qkv_rows, row_tile) * round_up_usize(hidden, col_tile);
    let z_weights = round_up_usize(delta_width, row_tile) * round_up_usize(hidden, col_tile);
    let gate_weights = round_up_usize(gate_rows, row_tile) * round_up_usize(hidden, col_tile);
    let out_weights = round_up_usize(hidden, row_tile) * round_up_usize(delta_width, col_tile);
    if embedding_tiled.len() != vocab_weights || lm_head_tiled.len() != vocab_weights {
        return Err("embedding/lm_head tiled length does not match vocab shape".to_string());
    }
    if qkv_tiled.len() != qkv_weights {
        return Err(format!(
            "qkv_tiled length must be {qkv_weights}, got {}",
            qkv_tiled.len()
        ));
    }
    if z_tiled.len() != z_weights {
        return Err(format!(
            "z_tiled length must be {z_weights}, got {}",
            z_tiled.len()
        ));
    }
    if b_tiled.len() != gate_weights || a_tiled.len() != gate_weights {
        return Err("b/a tiled length does not match DeltaNet gate shape".to_string());
    }
    if out_tiled.len() != out_weights {
        return Err(format!(
            "out_tiled length must be {out_weights}, got {}",
            out_tiled.len()
        ));
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let delta_rows_u32 = u32::try_from(delta_width).map_err(|_| "delta width exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv rows exceed u32")?;
    let gate_rows_u32 = u32::try_from(gate_rows).map_err(|_| "gate rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let delta_col_tiles_u32 =
        u32::try_from(delta_width.div_ceil(col_tile)).map_err(|_| "delta col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let qkv = dev.new_buffer(qkv_tiled.len() * std::mem::size_of::<u16>())?;
    let z = dev.new_buffer(z_tiled.len() * std::mem::size_of::<u16>())?;
    let b = dev.new_buffer(b_tiled.len() * std::mem::size_of::<u16>())?;
    let a = dev.new_buffer(a_tiled.len() * std::mem::size_of::<u16>())?;
    let out_proj = dev.new_buffer(out_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let qkv_f32 = dev.new_buffer(qkv_rows * std::mem::size_of::<f32>())?;
    let z_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let beta_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let state_len = gate_rows * QWEN35_08B.deltanet_head_dim * QWEN35_08B.deltanet_head_dim;
    let state = dev.new_buffer(state_len * std::mem::size_of::<f32>())?;
    let delta_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let delta_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    let zero_state = vec![0.0f32; state_len];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        qkv.write(0, qkv_tiled);
        z.write(0, z_tiled);
        b.write(0, b_tiled);
        a.write(0, a_tiled);
        out_proj.write(0, out_tiled);
        lm_head.write(0, lm_head_tiled);
        state.write(0, &zero_state);
    }

    for _ in 0..config.warmup {
        dispatch_decode_deltanet_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &qkv,
            &z,
            &b,
            &a,
            &out_proj,
            &lm_head,
            &hidden_a,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            &delta_f32,
            &delta_half,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            hidden_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            delta_col_tiles_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_deltanet_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &qkv,
            &z,
            &b,
            &a,
            &out_proj,
            &lm_head,
            &hidden_a,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            &delta_f32,
            &delta_half,
            &hidden_f32,
            &hidden_b,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            hidden_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            delta_col_tiles_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let state_bytes = state_len * std::mem::size_of::<f32>();
    let bytes_moved = (qkv_tiled.len()
        + z_tiled.len()
        + b_tiled.len()
        + a_tiled.len()
        + out_tiled.len()
        + lm_head_tiled.len())
        * 2
        + (qkv_rows + delta_width + gate_rows * 4 + hidden) * std::mem::size_of::<f32>()
        + (delta_width * 5 + hidden) * std::mem::size_of::<u16>()
        + state_bytes * 2
        + pair_bytes * 2;
    let effective_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_superblock_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    superblocks: usize,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    delta_qkv_tiled: &[u16],
    delta_z_tiled: &[u16],
    delta_b_tiled: &[u16],
    delta_a_tiled: &[u16],
    delta_out_tiled: &[u16],
    attn_q_tiled: &[u16],
    attn_k_tiled: &[u16],
    attn_v_tiled: &[u16],
    attn_o_tiled: &[u16],
    ffn_gate_tiled: &[u16],
    ffn_up_tiled: &[u16],
    ffn_down_tiled: &[u16],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    if superblocks == 0 || superblocks > 6 {
        return Err("superblocks must be in 1..=6".to_string());
    }
    let rows = config.vocab_rows;
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    let delta_width = QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim;
    let qkv_rows = delta_width * 3;
    let gate_rows = QWEN35_08B.deltanet_v_heads;
    let attention_width = QWEN35_08B.attention_q_width();
    let attention_kv_width = QWEN35_08B.attention_kv_width();
    let attention_q_rows = expected_attention_q_rows(attn_q_tiled.len(), row_tile, col_tile)?;
    let max_context = config.max_context.max(1);
    if usize::try_from(config.decode_position).map_err(|_| "decode position exceeds usize")?
        >= max_context
    {
        return Err(format!(
            "decode_position {} must be < max_context {max_context}",
            config.decode_position
        ));
    }

    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_q_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_kv_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, attention_width, row_tile, col_tile)?;
    validate_tiled_matvec_shape(qkv_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(gate_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, delta_width, row_tile, col_tile)?;
    validate_tiled_matvec_shape(intermediate, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, intermediate, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }

    let vocab_weights = tiled_matvec_len(rows, hidden, row_tile, col_tile);
    let attention_q_weights = tiled_matvec_len(attention_q_rows, hidden, row_tile, col_tile);
    let attention_kv_weights = tiled_matvec_len(attention_kv_width, hidden, row_tile, col_tile);
    let attention_o_weights = tiled_matvec_len(hidden, attention_width, row_tile, col_tile);
    let delta_qkv_weights = tiled_matvec_len(qkv_rows, hidden, row_tile, col_tile);
    let delta_z_weights = tiled_matvec_len(delta_width, hidden, row_tile, col_tile);
    let delta_gate_weights = tiled_matvec_len(gate_rows, hidden, row_tile, col_tile);
    let delta_out_weights = tiled_matvec_len(hidden, delta_width, row_tile, col_tile);
    let ffn_up_weights = tiled_matvec_len(intermediate, hidden, row_tile, col_tile);
    let ffn_down_weights = tiled_matvec_len(hidden, intermediate, row_tile, col_tile);
    if embedding_tiled.len() != vocab_weights || lm_head_tiled.len() != vocab_weights {
        return Err("embedding/lm_head tiled length does not match vocab shape".to_string());
    }
    if delta_qkv_tiled.len() != delta_qkv_weights
        || delta_z_tiled.len() != delta_z_weights
        || delta_b_tiled.len() != delta_gate_weights
        || delta_a_tiled.len() != delta_gate_weights
        || delta_out_tiled.len() != delta_out_weights
    {
        return Err("DeltaNet tiled weight length does not match Qwen DeltaNet slice".to_string());
    }
    if attn_q_tiled.len() != attention_q_weights
        || attn_k_tiled.len() != attention_kv_weights
        || attn_v_tiled.len() != attention_kv_weights
        || attn_o_tiled.len() != attention_o_weights
    {
        return Err("attention tiled weight length does not match Qwen GQA shape".to_string());
    }
    if ffn_gate_tiled.len() != ffn_up_weights
        || ffn_up_tiled.len() != ffn_up_weights
        || ffn_down_tiled.len() != ffn_down_weights
    {
        return Err("FFN tiled weight length does not match Qwen FFN shape".to_string());
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let attention_q_rows_u32 =
        u32::try_from(attention_q_rows).map_err(|_| "attention q rows exceed u32")?;
    let attention_kv_rows_u32 =
        u32::try_from(attention_kv_width).map_err(|_| "attention kv rows exceed u32")?;
    let intermediate_rows_u32 =
        u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let delta_rows_u32 = u32::try_from(delta_width).map_err(|_| "delta width exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv rows exceed u32")?;
    let gate_rows_u32 = u32::try_from(gate_rows).map_err(|_| "gate rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let attention_col_tiles_u32 = u32::try_from(attention_width.div_ceil(col_tile))
        .map_err(|_| "attention col tiles exceed u32")?;
    let delta_col_tiles_u32 =
        u32::try_from(delta_width.div_ceil(col_tile)).map_err(|_| "delta col tiles exceed u32")?;
    let intermediate_col_tiles_u32 = u32::try_from(intermediate.div_ceil(col_tile))
        .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let delta_qkv = dev.new_buffer(delta_qkv_tiled.len() * std::mem::size_of::<u16>())?;
    let delta_z = dev.new_buffer(delta_z_tiled.len() * std::mem::size_of::<u16>())?;
    let delta_b = dev.new_buffer(delta_b_tiled.len() * std::mem::size_of::<u16>())?;
    let delta_a = dev.new_buffer(delta_a_tiled.len() * std::mem::size_of::<u16>())?;
    let delta_out = dev.new_buffer(delta_out_tiled.len() * std::mem::size_of::<u16>())?;
    let attn_q = dev.new_buffer(attn_q_tiled.len() * std::mem::size_of::<u16>())?;
    let attn_k = dev.new_buffer(attn_k_tiled.len() * std::mem::size_of::<u16>())?;
    let attn_v = dev.new_buffer(attn_v_tiled.len() * std::mem::size_of::<u16>())?;
    let attn_o = dev.new_buffer(attn_o_tiled.len() * std::mem::size_of::<u16>())?;
    let ffn_gate = dev.new_buffer(ffn_gate_tiled.len() * std::mem::size_of::<u16>())?;
    let ffn_up = dev.new_buffer(ffn_up_tiled.len() * std::mem::size_of::<u16>())?;
    let ffn_down = dev.new_buffer(ffn_down_tiled.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<u16>())?;
    let qkv_f32 = dev.new_buffer(qkv_rows * std::mem::size_of::<f32>())?;
    let z_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let beta_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let state_len = gate_rows * QWEN35_08B.deltanet_head_dim * QWEN35_08B.deltanet_head_dim;
    let state = dev.new_buffer(superblocks * 3 * state_len * std::mem::size_of::<f32>())?;
    let delta_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let delta_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let attn_q_f32 = dev.new_buffer(attention_q_rows * std::mem::size_of::<f32>())?;
    let attn_k_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let attn_v_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let attn_k_cache =
        dev.new_buffer(max_context * attention_kv_width * std::mem::size_of::<u16>())?;
    let attn_v_cache =
        dev.new_buffer(max_context * attention_kv_width * std::mem::size_of::<u16>())?;
    let attn_half = dev.new_buffer(attention_width * std::mem::size_of::<u16>())?;
    let ffn_gate_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let ffn_up_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let ffn_act = dev.new_buffer(intermediate * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    let zero_state = vec![0.0f32; superblocks * 3 * state_len];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        delta_qkv.write(0, delta_qkv_tiled);
        delta_z.write(0, delta_z_tiled);
        delta_b.write(0, delta_b_tiled);
        delta_a.write(0, delta_a_tiled);
        delta_out.write(0, delta_out_tiled);
        attn_q.write(0, attn_q_tiled);
        attn_k.write(0, attn_k_tiled);
        attn_v.write(0, attn_v_tiled);
        attn_o.write(0, attn_o_tiled);
        ffn_gate.write(0, ffn_gate_tiled);
        ffn_up.write(0, ffn_up_tiled);
        ffn_down.write(0, ffn_down_tiled);
        lm_head.write(0, lm_head_tiled);
        state.write(0, &zero_state);
    }

    for _ in 0..config.warmup {
        dispatch_decode_superblock_tiled_once(
            &dev,
            superblocks,
            &token,
            &embedding,
            &norm,
            &delta_qkv,
            &delta_z,
            &delta_b,
            &delta_a,
            &delta_out,
            &attn_q,
            &attn_k,
            &attn_v,
            &attn_o,
            &ffn_gate,
            &ffn_up,
            &ffn_down,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            state_len * std::mem::size_of::<f32>(),
            &delta_f32,
            &delta_half,
            &attn_q_f32,
            &attn_k_f32,
            &attn_v_f32,
            &attn_k_cache,
            &attn_v_cache,
            &attn_half,
            &ffn_gate_f32,
            &ffn_up_f32,
            &ffn_act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            hidden_rows_u32,
            intermediate_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            delta_col_tiles_u32,
            intermediate_col_tiles_u32,
            config.decode_position,
            u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?,
            true,
            true,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_in_a = true;
    for _ in 0..config.iterations {
        let start = Instant::now();
        final_in_a = dispatch_decode_superblock_tiled_once(
            &dev,
            superblocks,
            &token,
            &embedding,
            &norm,
            &delta_qkv,
            &delta_z,
            &delta_b,
            &delta_a,
            &delta_out,
            &attn_q,
            &attn_k,
            &attn_v,
            &attn_o,
            &ffn_gate,
            &ffn_up,
            &ffn_down,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            state_len * std::mem::size_of::<f32>(),
            &delta_f32,
            &delta_half,
            &attn_q_f32,
            &attn_k_f32,
            &attn_v_f32,
            &attn_k_cache,
            &attn_v_cache,
            &attn_half,
            &ffn_gate_f32,
            &ffn_up_f32,
            &ffn_act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            vocab_rows_u32,
            hidden_rows_u32,
            intermediate_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            delta_col_tiles_u32,
            intermediate_col_tiles_u32,
            config.decode_position,
            u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?,
            true,
            true,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        if final_in_a {
            scores_a.read(0, &mut score);
            ids_a.read(0, &mut next_token);
        } else {
            scores_b.read(0, &mut score);
            ids_b.read(0, &mut next_token);
        }
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let state_bytes = superblocks * 3 * state_len * std::mem::size_of::<f32>();
    let weight_bytes = (superblocks
        * (3 * (delta_qkv_tiled.len()
            + delta_z_tiled.len()
            + delta_b_tiled.len()
            + delta_a_tiled.len()
            + delta_out_tiled.len())
            + attn_q_tiled.len()
            + attn_k_tiled.len()
            + attn_v_tiled.len()
            + attn_o_tiled.len()
            + 4 * (ffn_gate_tiled.len() + ffn_up_tiled.len() + ffn_down_tiled.len()))
        + lm_head_tiled.len())
        * std::mem::size_of::<u16>();
    let scratch_bytes = qkv_rows * std::mem::size_of::<f32>()
        + delta_width * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>() * 5)
        + hidden * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>() * 3)
        + (attention_q_rows + attention_kv_width * 2 + attention_width)
            * std::mem::size_of::<f32>()
        + (attention_width + max_context * attention_kv_width * 2) * std::mem::size_of::<u16>()
        + intermediate * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>())
        + pair_bytes * 2;
    let effective_gb_s =
        (weight_bytes + scratch_bytes + state_bytes * 2) as f64 / median_s.max(1e-12) / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_layered_pattern_tiled_with_weights(
    config: DecodeSkeletonBenchConfig,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    delta_layers: &[DeltaLayerTiled<'_>],
    attention_layers: &[AttentionLayerTiled<'_>],
    ffn_layers: &[FfnLayerTiled<'_>],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSkeletonBenchResult, String> {
    if delta_layers.len() != QWEN35_08B.n_deltanet_layers() {
        return Err(format!(
            "expected {} DeltaNet layers, got {}",
            QWEN35_08B.n_deltanet_layers(),
            delta_layers.len()
        ));
    }
    if attention_layers.len() != QWEN35_08B.n_full_attention_layers() {
        return Err(format!(
            "expected {} attention layers, got {}",
            QWEN35_08B.n_full_attention_layers(),
            attention_layers.len()
        ));
    }
    if ffn_layers.len() != QWEN35_08B.n_layers {
        return Err(format!(
            "expected {} FFN layers, got {}",
            QWEN35_08B.n_layers,
            ffn_layers.len()
        ));
    }

    let rows = config.vocab_rows;
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    let delta_width = QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim;
    let qkv_rows = delta_width * 3;
    let gate_rows = QWEN35_08B.deltanet_v_heads;
    let attention_width = QWEN35_08B.attention_q_width();
    let attention_kv_width = QWEN35_08B.attention_kv_width();
    let attention_q_rows =
        expected_attention_q_rows(attention_layers[0].q.len(), row_tile, col_tile)?;
    let max_context = config.max_context.max(1);
    if usize::try_from(config.decode_position).map_err(|_| "decode position exceeds usize")?
        >= max_context
    {
        return Err(format!(
            "decode_position {} must be < max_context {max_context}",
            config.decode_position
        ));
    }

    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_q_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_kv_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, attention_width, row_tile, col_tile)?;
    validate_tiled_matvec_shape(qkv_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(gate_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, delta_width, row_tile, col_tile)?;
    validate_tiled_matvec_shape(intermediate, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, intermediate, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }

    let vocab_weights = tiled_matvec_len(rows, hidden, row_tile, col_tile);
    let attention_q_weights = tiled_matvec_len(attention_q_rows, hidden, row_tile, col_tile);
    let attention_kv_weights = tiled_matvec_len(attention_kv_width, hidden, row_tile, col_tile);
    let attention_o_weights = tiled_matvec_len(hidden, attention_width, row_tile, col_tile);
    let delta_qkv_weights = tiled_matvec_len(qkv_rows, hidden, row_tile, col_tile);
    let delta_z_weights = tiled_matvec_len(delta_width, hidden, row_tile, col_tile);
    let delta_gate_weights = tiled_matvec_len(gate_rows, hidden, row_tile, col_tile);
    let delta_out_weights = tiled_matvec_len(hidden, delta_width, row_tile, col_tile);
    let ffn_up_weights = tiled_matvec_len(intermediate, hidden, row_tile, col_tile);
    let ffn_down_weights = tiled_matvec_len(hidden, intermediate, row_tile, col_tile);
    if embedding_tiled.len() != vocab_weights || lm_head_tiled.len() != vocab_weights {
        return Err("embedding/lm_head tiled length does not match vocab shape".to_string());
    }
    for (idx, layer) in delta_layers.iter().enumerate() {
        if layer.input_norm.len() != hidden
            || layer.qkv.len() != delta_qkv_weights
            || layer.z.len() != delta_z_weights
            || layer.b.len() != delta_gate_weights
            || layer.a.len() != delta_gate_weights
            || layer.a_log.len() != gate_rows
            || layer.dt_bias.len() != gate_rows
            || layer.gated_norm.len() != QWEN35_08B.deltanet_head_dim
            || layer.conv_weight.len() != qkv_rows * 4
            || layer.conv_bias.len() != qkv_rows
            || layer.out.len() != delta_out_weights
        {
            return Err(format!("DeltaNet layer {idx} tiled weight length mismatch"));
        }
    }
    for (idx, layer) in attention_layers.iter().enumerate() {
        if layer.input_norm.len() != hidden
            || layer.q_norm.len() != QWEN35_08B.attention_head_dim
            || layer.k_norm.len() != QWEN35_08B.attention_head_dim
            || layer.q.len() != attention_q_weights
            || layer.k.len() != attention_kv_weights
            || layer.v.len() != attention_kv_weights
            || layer.o.len() != attention_o_weights
        {
            return Err(format!(
                "attention layer {idx} tiled weight length mismatch"
            ));
        }
    }
    for (idx, layer) in ffn_layers.iter().enumerate() {
        if layer.post_norm.len() != hidden
            || layer.gate.len() != ffn_up_weights
            || layer.up.len() != ffn_up_weights
            || layer.down.len() != ffn_down_weights
        {
            return Err(format!("FFN layer {idx} tiled weight length mismatch"));
        }
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let intermediate_rows_u32 =
        u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let attention_q_rows_u32 =
        u32::try_from(attention_q_rows).map_err(|_| "attention q rows exceed u32")?;
    let attention_kv_rows_u32 =
        u32::try_from(attention_kv_width).map_err(|_| "attention kv rows exceed u32")?;
    let delta_rows_u32 = u32::try_from(delta_width).map_err(|_| "delta width exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv rows exceed u32")?;
    let gate_rows_u32 = u32::try_from(gate_rows).map_err(|_| "gate rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let attention_col_tiles_u32 = u32::try_from(attention_width.div_ceil(col_tile))
        .map_err(|_| "attention col tiles exceed u32")?;
    let delta_col_tiles_u32 =
        u32::try_from(delta_width.div_ceil(col_tile)).map_err(|_| "delta col tiles exceed u32")?;
    let intermediate_col_tiles_u32 = u32::try_from(intermediate.div_ceil(col_tile))
        .map_err(|_| "intermediate col tiles exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(embedding_tiled.len() * std::mem::size_of::<u16>())?;
    let norm = dev.new_buffer(norm_weight.len() * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(lm_head_tiled.len() * std::mem::size_of::<u16>())?;
    let mut delta_buffers = Vec::with_capacity(delta_layers.len());
    for layer in delta_layers {
        let input_norm = dev.new_buffer(layer.input_norm.len() * std::mem::size_of::<u16>())?;
        let qkv = dev.new_buffer(layer.qkv.len() * std::mem::size_of::<u16>())?;
        let z = dev.new_buffer(layer.z.len() * std::mem::size_of::<u16>())?;
        let b = dev.new_buffer(layer.b.len() * std::mem::size_of::<u16>())?;
        let a = dev.new_buffer(layer.a.len() * std::mem::size_of::<u16>())?;
        let a_log = dev.new_buffer(layer.a_log.len() * std::mem::size_of::<f32>())?;
        let dt_bias = dev.new_buffer(layer.dt_bias.len() * std::mem::size_of::<f32>())?;
        let gated_norm = dev.new_buffer(layer.gated_norm.len() * std::mem::size_of::<f32>())?;
        let conv_weight = dev.new_buffer(layer.conv_weight.len() * std::mem::size_of::<u16>())?;
        let conv_bias = dev.new_buffer(layer.conv_bias.len() * std::mem::size_of::<u16>())?;
        let out = dev.new_buffer(layer.out.len() * std::mem::size_of::<u16>())?;
        unsafe {
            input_norm.write(0, layer.input_norm);
            qkv.write(0, layer.qkv);
            z.write(0, layer.z);
            b.write(0, layer.b);
            a.write(0, layer.a);
            a_log.write(0, layer.a_log);
            dt_bias.write(0, layer.dt_bias);
            gated_norm.write(0, layer.gated_norm);
            conv_weight.write(0, layer.conv_weight);
            conv_bias.write(0, layer.conv_bias);
            out.write(0, layer.out);
        }
        delta_buffers.push(DeltaLayerBuffers {
            input_norm,
            qkv,
            z,
            b,
            a,
            a_log,
            dt_bias,
            gated_norm,
            conv_weight,
            conv_bias,
            out,
        });
    }
    let mut attention_buffers = Vec::with_capacity(attention_layers.len());
    for layer in attention_layers {
        let input_norm = dev.new_buffer(layer.input_norm.len() * std::mem::size_of::<u16>())?;
        let q_norm = dev.new_buffer(layer.q_norm.len() * std::mem::size_of::<u16>())?;
        let k_norm = dev.new_buffer(layer.k_norm.len() * std::mem::size_of::<u16>())?;
        let q = dev.new_buffer(layer.q.len() * std::mem::size_of::<u16>())?;
        let k = dev.new_buffer(layer.k.len() * std::mem::size_of::<u16>())?;
        let v = dev.new_buffer(layer.v.len() * std::mem::size_of::<u16>())?;
        let o = dev.new_buffer(layer.o.len() * std::mem::size_of::<u16>())?;
        unsafe {
            input_norm.write(0, layer.input_norm);
            q_norm.write(0, layer.q_norm);
            k_norm.write(0, layer.k_norm);
            q.write(0, layer.q);
            k.write(0, layer.k);
            v.write(0, layer.v);
            o.write(0, layer.o);
        }
        attention_buffers.push(AttentionLayerBuffers {
            input_norm,
            q_norm,
            k_norm,
            q,
            k,
            v,
            o,
        });
    }
    let mut ffn_buffers = Vec::with_capacity(ffn_layers.len());
    for layer in ffn_layers {
        let post_norm = dev.new_buffer(layer.post_norm.len() * std::mem::size_of::<u16>())?;
        let gate = dev.new_buffer(layer.gate.len() * std::mem::size_of::<u16>())?;
        let up = dev.new_buffer(layer.up.len() * std::mem::size_of::<u16>())?;
        let down = dev.new_buffer(layer.down.len() * std::mem::size_of::<u16>())?;
        unsafe {
            post_norm.write(0, layer.post_norm);
            gate.write(0, layer.gate);
            up.write(0, layer.up);
            down.write(0, layer.down);
        }
        ffn_buffers.push(FfnLayerBuffers {
            post_norm,
            gate,
            up,
            down,
        });
    }

    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let qkv_f32 = dev.new_buffer(qkv_rows * std::mem::size_of::<f32>())?;
    let z_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let beta_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let state_len = gate_rows * QWEN35_08B.deltanet_head_dim * QWEN35_08B.deltanet_head_dim;
    let state = dev.new_buffer(delta_layers.len() * state_len * std::mem::size_of::<f32>())?;
    let conv_state_len = qkv_rows * 3;
    let conv_state =
        dev.new_buffer(delta_layers.len() * conv_state_len * std::mem::size_of::<u16>())?;
    let delta_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let delta_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let attn_q_f32 = dev.new_buffer(attention_q_rows * std::mem::size_of::<f32>())?;
    let attn_k_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let attn_v_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let attn_k_cache = dev.new_buffer(
        attention_layers.len() * max_context * attention_kv_width * std::mem::size_of::<u16>(),
    )?;
    let attn_v_cache = dev.new_buffer(
        attention_layers.len() * max_context * attention_kv_width * std::mem::size_of::<u16>(),
    )?;
    let attn_half = dev.new_buffer(attention_width * std::mem::size_of::<u16>())?;
    let attn_splitk_blocks_usize = decode_attention_splitk_blocks(max_context);
    let attn_splitk_scalars = QWEN35_08B.attention_kv_heads
        * attn_splitk_blocks_usize
        * (QWEN35_08B.attention_q_heads / QWEN35_08B.attention_kv_heads);
    let attn_partial_m = dev.new_buffer(attn_splitk_scalars * std::mem::size_of::<f32>())?;
    let attn_partial_l = dev.new_buffer(attn_splitk_scalars * std::mem::size_of::<f32>())?;
    let attn_partial_acc = dev.new_buffer(
        attn_splitk_scalars * QWEN35_08B.attention_head_dim * std::mem::size_of::<f32>(),
    )?;
    let attn_splitk_blocks = u32::try_from(attn_splitk_blocks_usize)
        .map_err(|_| "decode attention split-k block count exceeds u32")?;
    let ffn_gate_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let ffn_up_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let ffn_act = dev.new_buffer(intermediate * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;

    let token_host = [config.input_token];
    let out_token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let out_score = dev.new_buffer(std::mem::size_of::<f32>())?;

    let zero_state = vec![0.0f32; delta_layers.len() * state_len];
    let zero_conv_state = vec![0u16; delta_layers.len() * conv_state_len];
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, embedding_tiled);
        norm.write(0, norm_weight);
        lm_head.write(0, lm_head_tiled);
    }

    for _ in 0..config.warmup {
        unsafe {
            state.write(0, &zero_state);
            conv_state.write(0, &zero_conv_state);
        }
        let _ = dispatch_decode_layered_pattern_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &delta_buffers,
            &attention_buffers,
            &ffn_buffers,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            state_len * std::mem::size_of::<f32>(),
            &conv_state,
            conv_state_len * std::mem::size_of::<u16>(),
            &delta_f32,
            &delta_half,
            &attn_q_f32,
            &attn_k_f32,
            &attn_v_f32,
            &attn_k_cache,
            &attn_v_cache,
            max_context * attention_kv_width * std::mem::size_of::<u16>(),
            &attn_half,
            &attn_partial_m,
            &attn_partial_l,
            &attn_partial_acc,
            attn_splitk_blocks,
            &ffn_gate_f32,
            &ffn_up_f32,
            &ffn_act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            &out_token,
            &out_score,
            vocab_rows_u32,
            hidden_rows_u32,
            intermediate_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            delta_col_tiles_u32,
            intermediate_col_tiles_u32,
            config.decode_position,
            u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?,
            true,
            true,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut lm_normed_is_a = false;
    for _ in 0..config.iterations {
        unsafe {
            state.write(0, &zero_state);
            conv_state.write(0, &zero_conv_state);
        }
        let start = Instant::now();
        lm_normed_is_a = dispatch_decode_layered_pattern_tiled_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &delta_buffers,
            &attention_buffers,
            &ffn_buffers,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            state_len * std::mem::size_of::<f32>(),
            &conv_state,
            conv_state_len * std::mem::size_of::<u16>(),
            &delta_f32,
            &delta_half,
            &attn_q_f32,
            &attn_k_f32,
            &attn_v_f32,
            &attn_k_cache,
            &attn_v_cache,
            max_context * attention_kv_width * std::mem::size_of::<u16>(),
            &attn_half,
            &attn_partial_m,
            &attn_partial_l,
            &attn_partial_acc,
            attn_splitk_blocks,
            &ffn_gate_f32,
            &ffn_up_f32,
            &ffn_act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            &out_token,
            &out_score,
            vocab_rows_u32,
            hidden_rows_u32,
            intermediate_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            delta_col_tiles_u32,
            intermediate_col_tiles_u32,
            config.decode_position,
            u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?,
            true,
            true,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut score = [0.0f32; 1];
    let mut next_token = [0u32; 1];
    unsafe {
        out_token.read(0, &mut next_token);
        out_score.read(0, &mut score);
    }
    let top_logits = if config.debug_top_k == 0 {
        Vec::new()
    } else {
        let mut lm_normed = vec![0.0f32; hidden];
        unsafe {
            if lm_normed_is_a {
                hidden_a.read(0, &mut lm_normed);
            } else {
                hidden_b.read(0, &mut lm_normed);
            }
        }
        topk_lm_head_tiled_f32(
            &lm_normed,
            lm_head_tiled,
            rows,
            row_tile,
            col_tile,
            config.debug_top_k,
        )
    };

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let weight_elems: usize = delta_layers
        .iter()
        .map(|l| l.qkv.len() + l.z.len() + l.b.len() + l.a.len() + l.out.len())
        .sum::<usize>()
        + attention_layers
            .iter()
            .map(|l| l.q.len() + l.k.len() + l.v.len() + l.o.len())
            .sum::<usize>()
        + ffn_layers
            .iter()
            .map(|l| l.gate.len() + l.up.len() + l.down.len())
            .sum::<usize>()
        + lm_head_tiled.len();
    let state_bytes = delta_layers.len() * state_len * std::mem::size_of::<f32>();
    let scratch_bytes = qkv_rows * std::mem::size_of::<f32>()
        + delta_width * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>() * 5)
        + hidden * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>() * 3)
        + (attention_q_rows + attention_kv_width * 2 + attention_width)
            * std::mem::size_of::<f32>()
        + (attention_width + attention_layers.len() * max_context * attention_kv_width * 2)
            * std::mem::size_of::<u16>()
        + intermediate * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>())
        + pair_bytes * 2;
    let effective_gb_s =
        (weight_elems * std::mem::size_of::<u16>() + scratch_bytes + state_bytes * 2) as f64
            / median_s.max(1e-12)
            / 1e9;

    Ok(DecodeSkeletonBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        next_token: next_token[0],
        score: score[0],
        top_logits,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_decode_layered_pattern_tiled_sequence_with_weights(
    config: DecodeSkeletonBenchConfig,
    steps: usize,
    embedding_tiled: &[u16],
    norm_weight: &[u16],
    delta_layers: &[DeltaLayerTiled<'_>],
    attention_layers: &[AttentionLayerTiled<'_>],
    ffn_layers: &[FfnLayerTiled<'_>],
    lm_head_tiled: &[u16],
    row_tile: usize,
    col_tile: usize,
) -> Result<DecodeSequenceBenchResult, String> {
    if steps == 0 {
        return Err("sequence steps must be > 0".to_string());
    }
    if delta_layers.len() != QWEN35_08B.n_deltanet_layers() {
        return Err(format!(
            "expected {} DeltaNet layers, got {}",
            QWEN35_08B.n_deltanet_layers(),
            delta_layers.len()
        ));
    }
    if attention_layers.len() != QWEN35_08B.n_full_attention_layers() {
        return Err(format!(
            "expected {} attention layers, got {}",
            QWEN35_08B.n_full_attention_layers(),
            attention_layers.len()
        ));
    }
    if ffn_layers.len() != QWEN35_08B.n_layers {
        return Err(format!(
            "expected {} FFN layers, got {}",
            QWEN35_08B.n_layers,
            ffn_layers.len()
        ));
    }

    let rows = config.vocab_rows;
    let prefill_steps =
        usize::try_from(config.decode_position).map_err(|_| "decode position exceeds usize")?;
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    let delta_width = QWEN35_08B.deltanet_width();
    let qkv_rows = QWEN35_08B.deltanet_qkv_width();
    let gate_rows = QWEN35_08B.deltanet_v_heads;
    let attention_width = QWEN35_08B.attention_q_width();
    let attention_kv_width = QWEN35_08B.attention_kv_width();
    let attention_q_rows =
        expected_attention_q_rows(attention_layers[0].q.len(), row_tile, col_tile)?;
    let max_context = config.max_context.max(prefill_steps + steps);

    validate_tiled_matvec_shape(rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_q_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(attention_kv_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, attention_width, row_tile, col_tile)?;
    validate_tiled_matvec_shape(qkv_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(delta_width, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(gate_rows, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, delta_width, row_tile, col_tile)?;
    validate_tiled_matvec_shape(intermediate, hidden, row_tile, col_tile)?;
    validate_tiled_matvec_shape(hidden, intermediate, row_tile, col_tile)?;
    if norm_weight.len() != hidden {
        return Err(format!(
            "norm_weight length must be {hidden}, got {}",
            norm_weight.len()
        ));
    }

    let vocab_weights = tiled_matvec_len(rows, hidden, row_tile, col_tile);
    let attention_q_weights = tiled_matvec_len(attention_q_rows, hidden, row_tile, col_tile);
    let attention_kv_weights = tiled_matvec_len(attention_kv_width, hidden, row_tile, col_tile);
    let attention_o_weights = tiled_matvec_len(hidden, attention_width, row_tile, col_tile);
    let delta_qkv_weights = tiled_matvec_len(qkv_rows, hidden, row_tile, col_tile);
    let delta_z_weights = tiled_matvec_len(delta_width, hidden, row_tile, col_tile);
    let delta_gate_weights = tiled_matvec_len(gate_rows, hidden, row_tile, col_tile);
    let delta_out_weights = tiled_matvec_len(hidden, delta_width, row_tile, col_tile);
    let ffn_up_weights = tiled_matvec_len(intermediate, hidden, row_tile, col_tile);
    let ffn_down_weights = tiled_matvec_len(hidden, intermediate, row_tile, col_tile);
    if embedding_tiled.len() != vocab_weights || lm_head_tiled.len() != vocab_weights {
        return Err("embedding/lm_head tiled length does not match vocab shape".to_string());
    }
    for (idx, layer) in delta_layers.iter().enumerate() {
        if layer.input_norm.len() != hidden
            || layer.qkv.len() != delta_qkv_weights
            || layer.z.len() != delta_z_weights
            || layer.b.len() != delta_gate_weights
            || layer.a.len() != delta_gate_weights
            || layer.a_log.len() != gate_rows
            || layer.dt_bias.len() != gate_rows
            || layer.gated_norm.len() != QWEN35_08B.deltanet_head_dim
            || layer.conv_weight.len() != qkv_rows * 4
            || layer.conv_bias.len() != qkv_rows
            || layer.out.len() != delta_out_weights
        {
            return Err(format!("DeltaNet layer {idx} tiled weight length mismatch"));
        }
    }
    for (idx, layer) in attention_layers.iter().enumerate() {
        if layer.input_norm.len() != hidden
            || layer.q_norm.len() != QWEN35_08B.attention_head_dim
            || layer.k_norm.len() != QWEN35_08B.attention_head_dim
            || layer.q.len() != attention_q_weights
            || layer.k.len() != attention_kv_weights
            || layer.v.len() != attention_kv_weights
            || layer.o.len() != attention_o_weights
        {
            return Err(format!(
                "attention layer {idx} tiled weight length mismatch"
            ));
        }
    }
    for (idx, layer) in ffn_layers.iter().enumerate() {
        if layer.post_norm.len() != hidden
            || layer.gate.len() != ffn_up_weights
            || layer.up.len() != ffn_up_weights
            || layer.down.len() != ffn_down_weights
        {
            return Err(format!("FFN layer {idx} tiled weight length mismatch"));
        }
    }

    let vocab_rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let hidden_rows_u32 = u32::try_from(hidden).map_err(|_| "hidden exceeds u32")?;
    let intermediate_rows_u32 =
        u32::try_from(intermediate).map_err(|_| "intermediate exceeds u32")?;
    let attention_q_rows_u32 =
        u32::try_from(attention_q_rows).map_err(|_| "attention q rows exceed u32")?;
    let attention_kv_rows_u32 =
        u32::try_from(attention_kv_width).map_err(|_| "attention kv rows exceed u32")?;
    let delta_rows_u32 = u32::try_from(delta_width).map_err(|_| "delta width exceeds u32")?;
    let qkv_rows_u32 = u32::try_from(qkv_rows).map_err(|_| "qkv rows exceed u32")?;
    let gate_rows_u32 = u32::try_from(gate_rows).map_err(|_| "gate rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceeds u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceeds u32")?;
    let hidden_col_tiles_u32 =
        u32::try_from(hidden.div_ceil(col_tile)).map_err(|_| "hidden col tiles exceed u32")?;
    let attention_col_tiles_u32 = u32::try_from(attention_width.div_ceil(col_tile))
        .map_err(|_| "attention col tiles exceed u32")?;
    let delta_col_tiles_u32 =
        u32::try_from(delta_width.div_ceil(col_tile)).map_err(|_| "delta col tiles exceed u32")?;
    let intermediate_col_tiles_u32 = u32::try_from(intermediate.div_ceil(col_tile))
        .map_err(|_| "intermediate col tiles exceed u32")?;
    let max_context_u32 = u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = new_readonly_buffer(&dev, embedding_tiled)?;
    let norm = new_readonly_buffer(&dev, norm_weight)?;
    let lm_head = new_readonly_buffer(&dev, lm_head_tiled)?;
    let mut delta_buffers = Vec::with_capacity(delta_layers.len());
    for layer in delta_layers {
        let input_norm = new_readonly_buffer(&dev, layer.input_norm)?;
        let qkv = new_readonly_buffer(&dev, layer.qkv)?;
        let z = new_readonly_buffer(&dev, layer.z)?;
        let b = new_readonly_buffer(&dev, layer.b)?;
        let a = new_readonly_buffer(&dev, layer.a)?;
        let a_log = new_readonly_buffer(&dev, layer.a_log)?;
        let dt_bias = new_readonly_buffer(&dev, layer.dt_bias)?;
        let gated_norm = new_readonly_buffer(&dev, layer.gated_norm)?;
        let conv_weight = new_readonly_buffer(&dev, layer.conv_weight)?;
        let conv_bias = new_readonly_buffer(&dev, layer.conv_bias)?;
        let out = new_readonly_buffer(&dev, layer.out)?;
        delta_buffers.push(DeltaLayerBuffers {
            input_norm,
            qkv,
            z,
            b,
            a,
            a_log,
            dt_bias,
            gated_norm,
            conv_weight,
            conv_bias,
            out,
        });
    }
    let mut attention_buffers = Vec::with_capacity(attention_layers.len());
    for layer in attention_layers {
        let input_norm = new_readonly_buffer(&dev, layer.input_norm)?;
        let q_norm = new_readonly_buffer(&dev, layer.q_norm)?;
        let k_norm = new_readonly_buffer(&dev, layer.k_norm)?;
        let q = new_readonly_buffer(&dev, layer.q)?;
        let k = new_readonly_buffer(&dev, layer.k)?;
        let v = new_readonly_buffer(&dev, layer.v)?;
        let o = new_readonly_buffer(&dev, layer.o)?;
        attention_buffers.push(AttentionLayerBuffers {
            input_norm,
            q_norm,
            k_norm,
            q,
            k,
            v,
            o,
        });
    }
    let mut ffn_buffers = Vec::with_capacity(ffn_layers.len());
    for layer in ffn_layers {
        let post_norm = new_readonly_buffer(&dev, layer.post_norm)?;
        let gate = new_readonly_buffer(&dev, layer.gate)?;
        let up = new_readonly_buffer(&dev, layer.up)?;
        let down = new_readonly_buffer(&dev, layer.down)?;
        ffn_buffers.push(FfnLayerBuffers {
            post_norm,
            gate,
            up,
            down,
        });
    }

    let hidden_a = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let hidden_b = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let qkv_f32 = dev.new_buffer(qkv_rows * std::mem::size_of::<f32>())?;
    let z_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let beta_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate_raw = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let q_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let k_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let v_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let gate = dev.new_buffer(gate_rows * std::mem::size_of::<f32>())?;
    let state_len = gate_rows * QWEN35_08B.deltanet_head_dim * QWEN35_08B.deltanet_head_dim;
    let state = dev.new_buffer(delta_layers.len() * state_len * std::mem::size_of::<f32>())?;
    let conv_state_len = qkv_rows * 3;
    let conv_state =
        dev.new_buffer(delta_layers.len() * conv_state_len * std::mem::size_of::<u16>())?;
    let delta_f32 = dev.new_buffer(delta_width * std::mem::size_of::<f32>())?;
    let delta_half = dev.new_buffer(delta_width * std::mem::size_of::<u16>())?;
    let attn_q_f32 = dev.new_buffer(attention_q_rows * std::mem::size_of::<f32>())?;
    let attn_k_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let attn_v_f32 = dev.new_buffer(attention_kv_width * std::mem::size_of::<f32>())?;
    let attn_cache_stride_bytes = max_context * attention_kv_width * std::mem::size_of::<u16>();
    let attn_k_cache = dev.new_buffer(attention_layers.len() * attn_cache_stride_bytes)?;
    let attn_v_cache = dev.new_buffer(attention_layers.len() * attn_cache_stride_bytes)?;
    let attn_half = dev.new_buffer(attention_width * std::mem::size_of::<u16>())?;
    let attn_splitk_blocks_usize = decode_attention_splitk_blocks(max_context);
    let attn_splitk_scalars = QWEN35_08B.attention_kv_heads
        * attn_splitk_blocks_usize
        * (QWEN35_08B.attention_q_heads / QWEN35_08B.attention_kv_heads);
    let attn_partial_m = dev.new_buffer(attn_splitk_scalars * std::mem::size_of::<f32>())?;
    let attn_partial_l = dev.new_buffer(attn_splitk_scalars * std::mem::size_of::<f32>())?;
    let attn_partial_acc = dev.new_buffer(
        attn_splitk_scalars * QWEN35_08B.attention_head_dim * std::mem::size_of::<f32>(),
    )?;
    let attn_splitk_blocks = u32::try_from(attn_splitk_blocks_usize)
        .map_err(|_| "decode attention split-k block count exceeds u32")?;
    let ffn_gate_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let ffn_up_f32 = dev.new_buffer(intermediate * std::mem::size_of::<f32>())?;
    let ffn_act = dev.new_buffer(intermediate * std::mem::size_of::<u16>())?;
    let hidden_f32 = dev.new_buffer(hidden * std::mem::size_of::<f32>())?;
    let scores_a = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let scores_b = dev.new_buffer(rows * std::mem::size_of::<f32>())?;
    let ids_a = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let ids_b = dev.new_buffer(rows * std::mem::size_of::<u32>())?;
    let out_token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let out_score = dev.new_buffer(std::mem::size_of::<f32>())?;

    let zero_state = vec![0.0f32; delta_layers.len() * state_len];
    let zero_conv_state = vec![0u16; delta_layers.len() * conv_state_len];

    for _ in 0..config.warmup {
        unsafe {
            state.write(0, &zero_state);
            conv_state.write(0, &zero_conv_state);
        }
        run_layered_pattern_sequence_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &delta_buffers,
            &attention_buffers,
            &ffn_buffers,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            state_len * std::mem::size_of::<f32>(),
            &conv_state,
            conv_state_len * std::mem::size_of::<u16>(),
            &delta_f32,
            &delta_half,
            &attn_q_f32,
            &attn_k_f32,
            &attn_v_f32,
            &attn_k_cache,
            &attn_v_cache,
            attn_cache_stride_bytes,
            &attn_half,
            &attn_partial_m,
            &attn_partial_l,
            &attn_partial_acc,
            attn_splitk_blocks,
            &ffn_gate_f32,
            &ffn_up_f32,
            &ffn_act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            &out_token,
            &out_score,
            config.input_token,
            prefill_steps,
            steps,
            vocab_rows_u32,
            hidden_rows_u32,
            intermediate_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            delta_col_tiles_u32,
            intermediate_col_tiles_u32,
            max_context_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    let mut final_tokens = Vec::new();
    let mut last_score = 0.0f32;
    for _ in 0..config.iterations {
        unsafe {
            state.write(0, &zero_state);
            conv_state.write(0, &zero_conv_state);
        }
        let start = Instant::now();
        let (tokens, score) = run_layered_pattern_sequence_once(
            &dev,
            &token,
            &embedding,
            &norm,
            &delta_buffers,
            &attention_buffers,
            &ffn_buffers,
            &lm_head,
            &hidden_a,
            &hidden_b,
            &qkv_f32,
            &z_f32,
            &beta_raw,
            &gate_raw,
            &q_half,
            &k_half,
            &v_half,
            &beta,
            &gate,
            &state,
            state_len * std::mem::size_of::<f32>(),
            &conv_state,
            conv_state_len * std::mem::size_of::<u16>(),
            &delta_f32,
            &delta_half,
            &attn_q_f32,
            &attn_k_f32,
            &attn_v_f32,
            &attn_k_cache,
            &attn_v_cache,
            attn_cache_stride_bytes,
            &attn_half,
            &attn_partial_m,
            &attn_partial_l,
            &attn_partial_acc,
            attn_splitk_blocks,
            &ffn_gate_f32,
            &ffn_up_f32,
            &ffn_act,
            &hidden_f32,
            &scores_a,
            &ids_a,
            &scores_b,
            &ids_b,
            &out_token,
            &out_score,
            config.input_token,
            prefill_steps,
            steps,
            vocab_rows_u32,
            hidden_rows_u32,
            intermediate_rows_u32,
            attention_q_rows_u32,
            attention_kv_rows_u32,
            delta_rows_u32,
            qkv_rows_u32,
            gate_rows_u32,
            row_tile_u32,
            col_tile_u32,
            hidden_col_tiles_u32,
            attention_col_tiles_u32,
            delta_col_tiles_u32,
            intermediate_col_tiles_u32,
            max_context_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
        final_tokens = tokens;
        last_score = score;
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let pair_bytes = rows * (std::mem::size_of::<f32>() + std::mem::size_of::<u32>());
    let weight_elems: usize = delta_layers
        .iter()
        .map(|l| l.qkv.len() + l.z.len() + l.b.len() + l.a.len() + l.out.len())
        .sum::<usize>()
        + attention_layers
            .iter()
            .map(|l| l.q.len() + l.k.len() + l.v.len() + l.o.len())
            .sum::<usize>()
        + ffn_layers
            .iter()
            .map(|l| l.gate.len() + l.up.len() + l.down.len())
            .sum::<usize>()
        + lm_head_tiled.len();
    let state_bytes = delta_layers.len() * state_len * std::mem::size_of::<f32>();
    let scratch_bytes = qkv_rows * std::mem::size_of::<f32>()
        + delta_width * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>() * 5)
        + hidden * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>() * 3)
        + (attention_q_rows + attention_kv_width * 2 + attention_width)
            * std::mem::size_of::<f32>()
        + (attention_width + attention_layers.len() * max_context * attention_kv_width * 2)
            * std::mem::size_of::<u16>()
        + intermediate * (std::mem::size_of::<f32>() * 2 + std::mem::size_of::<u16>())
        + pair_bytes * 2;
    let prefill_weight_elems = weight_elems.saturating_sub(lm_head_tiled.len());
    let prefill_scratch_bytes = scratch_bytes.saturating_sub(pair_bytes * 2);
    let effective_gb_s = (prefill_steps
        * (prefill_weight_elems * std::mem::size_of::<u16>() + prefill_scratch_bytes)
        + steps * (weight_elems * std::mem::size_of::<u16>() + scratch_bytes)
        + state_bytes * 2) as f64
        / median_s.max(1e-12)
        / 1e9;

    Ok(DecodeSequenceBenchResult {
        vocab_rows: rows,
        cols: hidden,
        input_token: config.input_token,
        steps,
        iterations: config.iterations,
        median_s,
        p95_s,
        effective_gb_s,
        tokens: final_tokens,
        last_score,
    })
}

fn pack_row_major_fp16_to_tiles(
    row_major: &[u16],
    rows: usize,
    cols: usize,
    row_tile: usize,
    col_tile: usize,
) -> Vec<u16> {
    let padded_rows = round_up_usize(rows, row_tile);
    let padded_cols = round_up_usize(cols, col_tile);
    let mut tiled = vec![0u16; padded_rows * padded_cols];
    let mut out = 0;
    for row_base in (0..padded_rows).step_by(row_tile) {
        for col_base in (0..padded_cols).step_by(col_tile) {
            for row_lane in 0..row_tile {
                for col_lane in 0..col_tile {
                    let row = row_base + row_lane;
                    let col = col_base + col_lane;
                    if row < rows && col < cols {
                        tiled[out] = row_major[row * cols + col];
                    }
                    out += 1;
                }
            }
        }
    }
    tiled
}

fn topk_lm_head_tiled_f32(
    hidden: &[f32],
    lm_head_tiled: &[u16],
    rows: usize,
    row_tile: usize,
    col_tile: usize,
    k: usize,
) -> Vec<(u32, f32)> {
    if k == 0 || hidden.len() != QWEN35_08B.hidden_size {
        return Vec::new();
    }
    let cols = QWEN35_08B.hidden_size;
    let n_col_tiles = cols.div_ceil(col_tile);
    let mut top = Vec::<(u32, f32)>::with_capacity(k);
    for row in 0..rows {
        let row_tile_idx = row / row_tile;
        let row_lane = row - row_tile_idx * row_tile;
        let mut score = 0.0f32;
        for (col, x) in hidden.iter().copied().enumerate() {
            let col_tile_idx = col / col_tile;
            let col_lane = col - col_tile_idx * col_tile;
            let packed_idx = ((row_tile_idx * n_col_tiles + col_tile_idx) * row_tile + row_lane)
                * col_tile
                + col_lane;
            score += f16::from_bits(lm_head_tiled[packed_idx]).to_f32() * x;
        }
        insert_topk(&mut top, (row as u32, score), k);
    }
    top
}

fn insert_topk(top: &mut Vec<(u32, f32)>, candidate: (u32, f32), k: usize) {
    let idx = top
        .iter()
        .position(|(_, score)| candidate.1 > *score)
        .unwrap_or(top.len());
    if idx < k {
        top.insert(idx, candidate);
        top.truncate(k);
    }
}

fn synthetic_matvec_x(cols: usize) -> Vec<u16> {
    (0..cols)
        .map(|i| f16::from_f32(((i % 97) as f32 - 48.0) / 97.0).to_bits())
        .collect()
}

fn synthetic_matvec_w(rows: usize, cols: usize) -> Vec<u16> {
    (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(13) % 251) as f32 - 125.0) / 251.0;
            f16::from_f32(v).to_bits()
        })
        .collect()
}

fn validate_tiled_matvec_shape(
    rows: usize,
    cols: usize,
    row_tile: usize,
    col_tile: usize,
) -> Result<(), String> {
    if rows == 0 {
        return Err("matvec rows must be > 0".to_string());
    }
    if row_tile == 0 || col_tile == 0 {
        return Err("row_tile and col_tile must be > 0".to_string());
    }
    if row_tile > 8 {
        return Err("row-tiled matvec currently specializes row_tile<=8".to_string());
    }
    if !cols.is_multiple_of(col_tile) {
        return Err(format!(
            "hidden size {cols} must be a multiple of col_tile {col_tile}"
        ));
    }
    Ok(())
}

fn round_up_usize(value: usize, multiple: usize) -> usize {
    value.div_ceil(multiple) * multiple
}

fn tiled_matvec_len(rows: usize, cols: usize, row_tile: usize, col_tile: usize) -> usize {
    round_up_usize(rows, row_tile) * round_up_usize(cols, col_tile)
}

fn expected_attention_q_rows(
    q_tiled_len: usize,
    row_tile: usize,
    col_tile: usize,
) -> Result<usize, String> {
    let hidden = QWEN35_08B.hidden_size;
    let q_width = QWEN35_08B.attention_q_width();
    let q_with_gate = QWEN35_08B.attention_q_with_head_gate_width();
    let q_len = tiled_matvec_len(q_width, hidden, row_tile, col_tile);
    let q_with_gate_len = tiled_matvec_len(q_with_gate, hidden, row_tile, col_tile);
    if q_tiled_len == q_len {
        Ok(q_width)
    } else if q_tiled_len == q_with_gate_len {
        Ok(q_with_gate)
    } else {
        Err(format!(
            "attention q tiled length must match [{q_width}, {hidden}] or [{q_with_gate}, {hidden}], got {q_tiled_len}"
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_skeleton_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding_and_lm_head: &crate::metal::ffi::Buffer,
    hidden: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    rows: u32,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_pairs_fp16_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding_and_lm_head, 0);
    enc.set_buffer(2, hidden, 0);
    enc.set_bytes(3, &rows);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, hidden, 0);
    enc.set_buffer(1, embedding_and_lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &rows);
    enc.dispatch_threadgroups((rows as usize, 1, 1), (256, 1, 1));

    let mut n = rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_skeleton_tiled_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden, 0);
    enc.set_bytes(3, &rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &n_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, hidden, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups((rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_one_projection_tiled_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    projection: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    vocab_rows: u32,
    hidden_rows: u32,
    row_tile: u32,
    col_tile: u32,
    n_col_tiles: u32,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024")?;
    let rms_project_pso = dev.pipeline("qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let cast_pso = dev.pipeline("qwen35_08b_hidden_f32_to_fp16_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden_a, 0);
    enc.set_bytes(3, &vocab_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &n_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&rms_project_pso);
    enc.set_buffer(0, hidden_a, 0);
    enc.set_buffer(1, norm, 0);
    enc.set_buffer(2, projection, 0);
    enc.set_buffer(3, hidden_f32, 0);
    enc.set_bytes(4, &hidden_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    enc.set_pipeline(&cast_pso);
    enc.set_buffer(0, hidden_f32, 0);
    enc.set_buffer(1, hidden_b, 0);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, hidden_b, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &vocab_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &n_col_tiles);
    enc.dispatch_threadgroups((vocab_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = vocab_rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_ffn_tiled_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    up: &crate::metal::ffi::Buffer,
    down: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    gate_f32: &crate::metal::ffi::Buffer,
    up_f32: &crate::metal::ffi::Buffer,
    act: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    vocab_rows: u32,
    intermediate_rows: u32,
    hidden_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    intermediate_col_tiles: u32,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024")?;
    let rms_project_pso = dev.pipeline("qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let swiglu_pso = dev.pipeline("qwen35_08b_swiglu_f32_to_fp16_i3584")?;
    let down_pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k3584_f32")?;
    let cast_pso = dev.pipeline("qwen35_08b_hidden_f32_to_fp16_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden_a, 0);
    enc.set_bytes(3, &vocab_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &hidden_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&rms_project_pso);
    enc.set_buffer(0, hidden_a, 0);
    enc.set_buffer(1, norm, 0);
    enc.set_buffer(2, gate, 0);
    enc.set_buffer(3, gate_f32, 0);
    enc.set_bytes(4, &intermediate_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    enc.dispatch_threadgroups(
        (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
        (256, 1, 1),
    );

    enc.set_pipeline(&rms_project_pso);
    enc.set_buffer(0, hidden_a, 0);
    enc.set_buffer(1, norm, 0);
    enc.set_buffer(2, up, 0);
    enc.set_buffer(3, up_f32, 0);
    enc.set_bytes(4, &intermediate_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    enc.dispatch_threadgroups(
        (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
        (256, 1, 1),
    );

    enc.set_pipeline(&swiglu_pso);
    enc.set_buffer(0, gate_f32, 0);
    enc.set_buffer(1, up_f32, 0);
    enc.set_buffer(2, act, 0);
    enc.dispatch_threads(QWEN35_08B.ffn_intermediate, 256);

    enc.set_pipeline(&down_pso);
    enc.set_buffer(0, act, 0);
    enc.set_buffer(1, down, 0);
    enc.set_buffer(2, hidden_f32, 0);
    enc.set_bytes(3, &hidden_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &intermediate_col_tiles);
    enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    enc.set_pipeline(&cast_pso);
    enc.set_buffer(0, hidden_f32, 0);
    enc.set_buffer(1, hidden_b, 0);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, hidden_b, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &vocab_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    enc.dispatch_threadgroups((vocab_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = vocab_rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_repeated_ffn_tiled_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    up: &crate::metal::ffi::Buffer,
    down: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    gate_f32: &crate::metal::ffi::Buffer,
    up_f32: &crate::metal::ffi::Buffer,
    act: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    vocab_rows: u32,
    intermediate_rows: u32,
    hidden_rows: u32,
    ffn_layers: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    intermediate_col_tiles: u32,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024")?;
    let rms_project_pso = dev.pipeline("qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let swiglu_pso = dev.pipeline("qwen35_08b_swiglu_f32_to_fp16_i3584")?;
    let down_pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k3584_f32")?;
    let residual_pso = dev.pipeline("qwen35_08b_residual_add_f32_to_fp16_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden_a, 0);
    enc.set_bytes(3, &vocab_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &hidden_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    let mut current_is_a = true;
    for _ in 0..ffn_layers {
        let input = if current_is_a { hidden_a } else { hidden_b };
        let output = if current_is_a { hidden_b } else { hidden_a };

        enc.set_pipeline(&rms_project_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, gate, 0);
        enc.set_buffer(3, gate_f32, 0);
        enc.set_bytes(4, &intermediate_rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &col_tile);
        enc.set_bytes(7, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
            (256, 1, 1),
        );

        enc.set_pipeline(&rms_project_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, up, 0);
        enc.set_buffer(3, up_f32, 0);
        enc.set_bytes(4, &intermediate_rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &col_tile);
        enc.set_bytes(7, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
            (256, 1, 1),
        );

        enc.set_pipeline(&swiglu_pso);
        enc.set_buffer(0, gate_f32, 0);
        enc.set_buffer(1, up_f32, 0);
        enc.set_buffer(2, act, 0);
        enc.dispatch_threads(QWEN35_08B.ffn_intermediate, 256);

        enc.set_pipeline(&down_pso);
        enc.set_buffer(0, act, 0);
        enc.set_buffer(1, down, 0);
        enc.set_buffer(2, hidden_f32, 0);
        enc.set_bytes(3, &hidden_rows);
        enc.set_bytes(4, &row_tile);
        enc.set_bytes(5, &col_tile);
        enc.set_bytes(6, &intermediate_col_tiles);
        enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

        enc.set_pipeline(&residual_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, hidden_f32, 0);
        enc.set_buffer(2, output, 0);
        enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

        current_is_a = !current_is_a;
    }

    let lm_input = if current_is_a { hidden_a } else { hidden_b };
    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, lm_input, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &vocab_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    enc.dispatch_threadgroups((vocab_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = vocab_rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_attention_tiled_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    q: &crate::metal::ffi::Buffer,
    k: &crate::metal::ffi::Buffer,
    v: &crate::metal::ffi::Buffer,
    o: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    q_f32: &crate::metal::ffi::Buffer,
    k_f32: &crate::metal::ffi::Buffer,
    v_f32: &crate::metal::ffi::Buffer,
    k_cache: &crate::metal::ffi::Buffer,
    v_cache: &crate::metal::ffi::Buffer,
    attn: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    vocab_rows: u32,
    hidden_rows: u32,
    attention_q_rows: u32,
    attention_kv_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    attention_col_tiles: u32,
    position: u32,
    max_context: u32,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024")?;
    let rms_project_pso = dev.pipeline("qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let decode_attention_qh4 = decode_attention_qh4_enabled();
    let attention_pso = if decode_attention_qh4 {
        dev.pipeline("qwen35_08b_attention_single_token_qh4_gqa8_kv2_d256_rope_cache_to_fp16")?
    } else {
        dev.pipeline("qwen35_08b_attention_single_token_gqa8_kv2_d256_rope_cache_to_fp16")?
    };
    let project_pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k2048_f32")?;
    let cast_pso = dev.pipeline("qwen35_08b_hidden_f32_to_fp16_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden_a, 0);
    enc.set_bytes(3, &vocab_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &hidden_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    for (weights, out, projection_rows) in [
        (q, q_f32, attention_q_rows),
        (k, k_f32, attention_kv_rows),
        (v, v_f32, attention_kv_rows),
    ] {
        enc.set_pipeline(&rms_project_pso);
        enc.set_buffer(0, hidden_a, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, weights, 0);
        enc.set_buffer(3, out, 0);
        enc.set_bytes(4, &projection_rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &col_tile);
        enc.set_bytes(7, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (projection_rows.div_ceil(row_tile) as usize, 1, 1),
            (256, 1, 1),
        );
    }

    enc.set_pipeline(&attention_pso);
    enc.set_buffer(0, q_f32, 0);
    enc.set_buffer(1, k_f32, 0);
    enc.set_buffer(2, v_f32, 0);
    enc.set_buffer(3, k_cache, 0);
    enc.set_buffer(4, v_cache, 0);
    enc.set_buffer(5, attn, 0);
    enc.set_bytes(6, &attention_q_rows);
    enc.set_bytes(7, &position);
    enc.set_bytes(8, &max_context);
    enc.dispatch_threadgroups(
        (
            if decode_attention_qh4 {
                QWEN35_08B.attention_kv_heads
            } else {
                QWEN35_08B.attention_q_heads
            },
            1,
            1,
        ),
        (256, 1, 1),
    );

    enc.set_pipeline(&project_pso);
    enc.set_buffer(0, attn, 0);
    enc.set_buffer(1, o, 0);
    enc.set_buffer(2, hidden_f32, 0);
    enc.set_bytes(3, &hidden_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &attention_col_tiles);
    enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    enc.set_pipeline(&cast_pso);
    enc.set_buffer(0, hidden_f32, 0);
    enc.set_buffer(1, hidden_b, 0);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, hidden_b, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &vocab_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    enc.dispatch_threadgroups((vocab_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = vocab_rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_deltanet_tiled_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    qkv: &crate::metal::ffi::Buffer,
    z: &crate::metal::ffi::Buffer,
    b: &crate::metal::ffi::Buffer,
    a: &crate::metal::ffi::Buffer,
    out_proj: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    qkv_f32: &crate::metal::ffi::Buffer,
    z_f32: &crate::metal::ffi::Buffer,
    beta_raw: &crate::metal::ffi::Buffer,
    gate_raw: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    state: &crate::metal::ffi::Buffer,
    delta_f32: &crate::metal::ffi::Buffer,
    delta_half: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    vocab_rows: u32,
    hidden_rows: u32,
    delta_rows: u32,
    qkv_rows: u32,
    gate_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    delta_col_tiles: u32,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024")?;
    let rms_project_pso = dev.pipeline("qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let split_pso = dev.pipeline("qwen35_08b_deltanet_split_qkv_f32_to_fp16_h16d128")?;
    let activate_pso = dev.pipeline("qwen35_08b_deltanet_activate_beta_gate_h16")?;
    let step_pso = dev.pipeline(decode_deltanet_step_kernel())?;
    let z_gate_pso = dev.pipeline("qwen35_08b_deltanet_apply_z_gate_f32_to_fp16_k2048")?;
    let out_project_pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k2048_f32")?;
    let cast_pso = dev.pipeline("qwen35_08b_hidden_f32_to_fp16_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden_a, 0);
    enc.set_bytes(3, &vocab_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &hidden_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    for (weights, out, rows) in [
        (qkv, qkv_f32, qkv_rows),
        (z, z_f32, delta_rows),
        (b, beta_raw, gate_rows),
        (a, gate_raw, gate_rows),
    ] {
        enc.set_pipeline(&rms_project_pso);
        enc.set_buffer(0, hidden_a, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, weights, 0);
        enc.set_buffer(3, out, 0);
        enc.set_bytes(4, &rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &col_tile);
        enc.set_bytes(7, &hidden_col_tiles);
        enc.dispatch_threadgroups((rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));
    }

    enc.set_pipeline(&split_pso);
    enc.set_buffer(0, qkv_f32, 0);
    enc.set_buffer(1, q_half, 0);
    enc.set_buffer(2, k_half, 0);
    enc.set_buffer(3, v_half, 0);
    enc.dispatch_threads(
        QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim,
        256,
    );

    enc.set_pipeline(&activate_pso);
    enc.set_buffer(0, beta_raw, 0);
    enc.set_buffer(1, gate_raw, 0);
    enc.set_buffer(2, beta, 0);
    enc.set_buffer(3, gate, 0);
    enc.dispatch_threads(QWEN35_08B.deltanet_v_heads, 16);

    enc.set_pipeline(&step_pso);
    enc.set_buffer(0, q_half, 0);
    enc.set_buffer(1, k_half, 0);
    enc.set_buffer(2, v_half, 0);
    enc.set_buffer(3, beta, 0);
    enc.set_buffer(4, gate, 0);
    enc.set_buffer(5, state, 0);
    enc.set_buffer(6, delta_f32, 0);
    enc.dispatch_threadgroups((QWEN35_08B.deltanet_v_heads, 1, 1), (128, 1, 1));

    enc.set_pipeline(&z_gate_pso);
    enc.set_buffer(0, delta_f32, 0);
    enc.set_buffer(1, z_f32, 0);
    enc.set_buffer(2, delta_half, 0);
    enc.dispatch_threads(
        QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim,
        256,
    );

    enc.set_pipeline(&out_project_pso);
    enc.set_buffer(0, delta_half, 0);
    enc.set_buffer(1, out_proj, 0);
    enc.set_buffer(2, hidden_f32, 0);
    enc.set_bytes(3, &hidden_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &delta_col_tiles);
    enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    enc.set_pipeline(&cast_pso);
    enc.set_buffer(0, hidden_f32, 0);
    enc.set_buffer(1, hidden_b, 0);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, hidden_b, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &vocab_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    enc.dispatch_threadgroups((vocab_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = vocab_rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn run_attention_sequence_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    q: &crate::metal::ffi::Buffer,
    k: &crate::metal::ffi::Buffer,
    v: &crate::metal::ffi::Buffer,
    o: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    q_f32: &crate::metal::ffi::Buffer,
    k_f32: &crate::metal::ffi::Buffer,
    v_f32: &crate::metal::ffi::Buffer,
    k_cache: &crate::metal::ffi::Buffer,
    v_cache: &crate::metal::ffi::Buffer,
    attn: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    input_token: u32,
    steps: usize,
    vocab_rows: u32,
    hidden_rows: u32,
    attention_q_rows: u32,
    attention_kv_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    attention_col_tiles: u32,
    max_context: u32,
) -> Result<(Vec<u32>, f32), String> {
    let mut current_token = input_token;
    let mut tokens = Vec::with_capacity(steps);
    let mut last_score = 0.0f32;
    for position in 0..steps {
        unsafe {
            token.write(0, &[current_token]);
        }
        let final_in_a = dispatch_decode_attention_tiled_once(
            dev,
            token,
            embedding,
            norm,
            q,
            k,
            v,
            o,
            lm_head,
            hidden_a,
            q_f32,
            k_f32,
            v_f32,
            k_cache,
            v_cache,
            attn,
            hidden_f32,
            hidden_b,
            scores_a,
            ids_a,
            scores_b,
            ids_b,
            vocab_rows,
            hidden_rows,
            attention_q_rows,
            attention_kv_rows,
            row_tile,
            col_tile,
            hidden_col_tiles,
            attention_col_tiles,
            u32::try_from(position).map_err(|_| "decode position exceeds u32")?,
            max_context,
        )?;
        let mut score = [0.0f32; 1];
        let mut next_token = [0u32; 1];
        unsafe {
            if final_in_a {
                scores_a.read(0, &mut score);
                ids_a.read(0, &mut next_token);
            } else {
                scores_b.read(0, &mut score);
                ids_b.read(0, &mut next_token);
            }
        }
        current_token = next_token[0];
        tokens.push(current_token);
        last_score = score[0];
    }
    Ok((tokens, last_score))
}

#[allow(clippy::too_many_arguments)]
fn run_layered_pattern_sequence_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    delta_layers: &[DeltaLayerBuffers],
    attention_layers: &[AttentionLayerBuffers],
    ffn_layers: &[FfnLayerBuffers],
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    qkv_f32: &crate::metal::ffi::Buffer,
    z_f32: &crate::metal::ffi::Buffer,
    beta_raw: &crate::metal::ffi::Buffer,
    gate_raw: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    state: &crate::metal::ffi::Buffer,
    state_stride_bytes: usize,
    conv_state: &crate::metal::ffi::Buffer,
    conv_state_stride_bytes: usize,
    delta_f32: &crate::metal::ffi::Buffer,
    delta_half: &crate::metal::ffi::Buffer,
    attn_q_f32: &crate::metal::ffi::Buffer,
    attn_k_f32: &crate::metal::ffi::Buffer,
    attn_v_f32: &crate::metal::ffi::Buffer,
    attn_k_cache: &crate::metal::ffi::Buffer,
    attn_v_cache: &crate::metal::ffi::Buffer,
    attn_cache_stride_bytes: usize,
    attn_half: &crate::metal::ffi::Buffer,
    attn_partial_m: &crate::metal::ffi::Buffer,
    attn_partial_l: &crate::metal::ffi::Buffer,
    attn_partial_acc: &crate::metal::ffi::Buffer,
    attn_splitk_blocks: u32,
    ffn_gate_f32: &crate::metal::ffi::Buffer,
    ffn_up_f32: &crate::metal::ffi::Buffer,
    ffn_act: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    out_token: &crate::metal::ffi::Buffer,
    out_score: &crate::metal::ffi::Buffer,
    input_token: u32,
    prefill_steps: usize,
    steps: usize,
    vocab_rows: u32,
    hidden_rows: u32,
    intermediate_rows: u32,
    attention_q_rows: u32,
    attention_kv_rows: u32,
    delta_rows: u32,
    qkv_rows: u32,
    gate_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    attention_col_tiles: u32,
    delta_col_tiles: u32,
    intermediate_col_tiles: u32,
    max_context: u32,
) -> Result<(Vec<u32>, f32), String> {
    let collect_all_tokens = steps <= 32;
    let async_commands = decode_async_commands_enabled() && !collect_all_tokens;
    let mut tokens = Vec::with_capacity(steps);

    for position in 0..prefill_steps.saturating_sub(1) {
        unsafe {
            token.write(0, &[input_token]);
        }
        let _ = dispatch_decode_layered_pattern_tiled_once(
            dev,
            token,
            embedding,
            norm,
            delta_layers,
            attention_layers,
            ffn_layers,
            lm_head,
            hidden_a,
            hidden_b,
            qkv_f32,
            z_f32,
            beta_raw,
            gate_raw,
            q_half,
            k_half,
            v_half,
            beta,
            gate,
            state,
            state_stride_bytes,
            conv_state,
            conv_state_stride_bytes,
            delta_f32,
            delta_half,
            attn_q_f32,
            attn_k_f32,
            attn_v_f32,
            attn_k_cache,
            attn_v_cache,
            attn_cache_stride_bytes,
            attn_half,
            attn_partial_m,
            attn_partial_l,
            attn_partial_acc,
            attn_splitk_blocks,
            ffn_gate_f32,
            ffn_up_f32,
            ffn_act,
            hidden_f32,
            scores_a,
            ids_a,
            scores_b,
            ids_b,
            out_token,
            out_score,
            vocab_rows,
            hidden_rows,
            intermediate_rows,
            attention_q_rows,
            attention_kv_rows,
            delta_rows,
            qkv_rows,
            gate_rows,
            row_tile,
            col_tile,
            hidden_col_tiles,
            attention_col_tiles,
            delta_col_tiles,
            intermediate_col_tiles,
            u32::try_from(position).map_err(|_| "prefill position exceeds u32")?,
            max_context,
            false,
            false,
        )?;
    }

    let mut generated = 0usize;
    if prefill_steps > 0 && steps > 0 {
        unsafe {
            token.write(0, &[input_token]);
        }
        let last_prompt_position = prefill_steps - 1;
        let wait_until_completed = collect_all_tokens || steps == 1;
        let _ = dispatch_decode_layered_pattern_tiled_once(
            dev,
            token,
            embedding,
            norm,
            delta_layers,
            attention_layers,
            ffn_layers,
            lm_head,
            hidden_a,
            hidden_b,
            qkv_f32,
            z_f32,
            beta_raw,
            gate_raw,
            q_half,
            k_half,
            v_half,
            beta,
            gate,
            state,
            state_stride_bytes,
            conv_state,
            conv_state_stride_bytes,
            delta_f32,
            delta_half,
            attn_q_f32,
            attn_k_f32,
            attn_v_f32,
            attn_k_cache,
            attn_v_cache,
            attn_cache_stride_bytes,
            attn_half,
            attn_partial_m,
            attn_partial_l,
            attn_partial_acc,
            attn_splitk_blocks,
            ffn_gate_f32,
            ffn_up_f32,
            ffn_act,
            hidden_f32,
            scores_a,
            ids_a,
            scores_b,
            ids_b,
            token,
            out_score,
            vocab_rows,
            hidden_rows,
            intermediate_rows,
            attention_q_rows,
            attention_kv_rows,
            delta_rows,
            qkv_rows,
            gate_rows,
            row_tile,
            col_tile,
            hidden_col_tiles,
            attention_col_tiles,
            delta_col_tiles,
            intermediate_col_tiles,
            u32::try_from(last_prompt_position).map_err(|_| "prefill position exceeds u32")?,
            max_context,
            true,
            wait_until_completed,
        )?;
        if collect_all_tokens {
            let mut next_token = [0u32; 1];
            unsafe {
                token.read(0, &mut next_token);
            }
            tokens.push(next_token[0]);
        }
        generated = 1;
    } else {
        unsafe {
            token.write(0, &[input_token]);
        }
    }

    for step in generated..steps {
        let decode_position = if prefill_steps > 0 {
            prefill_steps + step - 1
        } else {
            step
        };
        let wait_until_completed = !async_commands || step + 1 == steps;
        let _ = dispatch_decode_layered_pattern_tiled_once(
            dev,
            token,
            embedding,
            norm,
            delta_layers,
            attention_layers,
            ffn_layers,
            lm_head,
            hidden_a,
            hidden_b,
            qkv_f32,
            z_f32,
            beta_raw,
            gate_raw,
            q_half,
            k_half,
            v_half,
            beta,
            gate,
            state,
            state_stride_bytes,
            conv_state,
            conv_state_stride_bytes,
            delta_f32,
            delta_half,
            attn_q_f32,
            attn_k_f32,
            attn_v_f32,
            attn_k_cache,
            attn_v_cache,
            attn_cache_stride_bytes,
            attn_half,
            attn_partial_m,
            attn_partial_l,
            attn_partial_acc,
            attn_splitk_blocks,
            ffn_gate_f32,
            ffn_up_f32,
            ffn_act,
            hidden_f32,
            scores_a,
            ids_a,
            scores_b,
            ids_b,
            token,
            out_score,
            vocab_rows,
            hidden_rows,
            intermediate_rows,
            attention_q_rows,
            attention_kv_rows,
            delta_rows,
            qkv_rows,
            gate_rows,
            row_tile,
            col_tile,
            hidden_col_tiles,
            attention_col_tiles,
            delta_col_tiles,
            intermediate_col_tiles,
            u32::try_from(decode_position).map_err(|_| "decode position exceeds u32")?,
            max_context,
            true,
            wait_until_completed,
        )?;
        if collect_all_tokens {
            let mut next_token = [0u32; 1];
            unsafe {
                token.read(0, &mut next_token);
            }
            tokens.push(next_token[0]);
        }
    }
    if !collect_all_tokens {
        let mut final_token = [0u32; 1];
        unsafe {
            token.read(0, &mut final_token);
        }
        tokens.push(final_token[0]);
    }
    let mut score = [0.0f32; 1];
    unsafe {
        out_score.read(0, &mut score);
    }
    Ok((tokens, score[0]))
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_superblock_tiled_once(
    dev: &Device,
    superblocks: usize,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    norm: &crate::metal::ffi::Buffer,
    delta_qkv: &crate::metal::ffi::Buffer,
    delta_z: &crate::metal::ffi::Buffer,
    delta_b: &crate::metal::ffi::Buffer,
    delta_a: &crate::metal::ffi::Buffer,
    delta_out: &crate::metal::ffi::Buffer,
    attn_q: &crate::metal::ffi::Buffer,
    attn_k: &crate::metal::ffi::Buffer,
    attn_v: &crate::metal::ffi::Buffer,
    attn_o: &crate::metal::ffi::Buffer,
    ffn_gate: &crate::metal::ffi::Buffer,
    ffn_up: &crate::metal::ffi::Buffer,
    ffn_down: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    qkv_f32: &crate::metal::ffi::Buffer,
    z_f32: &crate::metal::ffi::Buffer,
    beta_raw: &crate::metal::ffi::Buffer,
    gate_raw: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    state: &crate::metal::ffi::Buffer,
    state_stride_bytes: usize,
    delta_f32: &crate::metal::ffi::Buffer,
    delta_half: &crate::metal::ffi::Buffer,
    attn_q_f32: &crate::metal::ffi::Buffer,
    attn_k_f32: &crate::metal::ffi::Buffer,
    attn_v_f32: &crate::metal::ffi::Buffer,
    attn_k_cache: &crate::metal::ffi::Buffer,
    attn_v_cache: &crate::metal::ffi::Buffer,
    attn_half: &crate::metal::ffi::Buffer,
    ffn_gate_f32: &crate::metal::ffi::Buffer,
    ffn_up_f32: &crate::metal::ffi::Buffer,
    ffn_act: &crate::metal::ffi::Buffer,
    hidden_f32: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    vocab_rows: u32,
    hidden_rows: u32,
    intermediate_rows: u32,
    attention_q_rows: u32,
    attention_kv_rows: u32,
    delta_rows: u32,
    qkv_rows: u32,
    gate_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    attention_col_tiles: u32,
    delta_col_tiles: u32,
    intermediate_col_tiles: u32,
    position: u32,
    max_context: u32,
    include_lm_head: bool,
    wait_until_completed: bool,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024")?;
    let rms_project_pso = dev.pipeline("qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32")?;
    let split_pso = dev.pipeline("qwen35_08b_deltanet_split_qkv_f32_to_fp16_h16d128")?;
    let activate_pso = dev.pipeline("qwen35_08b_deltanet_activate_beta_gate_h16")?;
    let step_pso = dev.pipeline(decode_deltanet_step_kernel())?;
    let z_gate_pso = dev.pipeline("qwen35_08b_deltanet_apply_z_gate_f32_to_fp16_k2048")?;
    let delta_out_pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k2048_f32")?;
    let attn_pso =
        dev.pipeline("qwen35_08b_attention_single_token_gqa8_kv2_d256_rope_cache_to_fp16")?;
    let project2048_pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k2048_f32")?;
    let swiglu_pso = dev.pipeline("qwen35_08b_swiglu_f32_to_fp16_i3584")?;
    let ffn_down_pso = dev.pipeline("qwen35_08b_matvec_rowtiles_fp16_tiled_k3584_f32")?;
    let residual_pso = dev.pipeline("qwen35_08b_residual_add_f32_to_fp16_k1024")?;
    let score_pso = dev.pipeline("qwen35_08b_lm_head_score_rowtiles_fp16_tiled_k1024")?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden_a, 0);
    enc.set_bytes(3, &vocab_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &hidden_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    let mut current_is_a = true;
    for block_index in 0..superblocks {
        for d_index in 0..3 {
            let input = if current_is_a { hidden_a } else { hidden_b };
            let output = if current_is_a { hidden_b } else { hidden_a };

            for (weights, out, rows) in [
                (delta_qkv, qkv_f32, qkv_rows),
                (delta_z, z_f32, delta_rows),
                (delta_b, beta_raw, gate_rows),
                (delta_a, gate_raw, gate_rows),
            ] {
                enc.set_pipeline(&rms_project_pso);
                enc.set_buffer(0, input, 0);
                enc.set_buffer(1, norm, 0);
                enc.set_buffer(2, weights, 0);
                enc.set_buffer(3, out, 0);
                enc.set_bytes(4, &rows);
                enc.set_bytes(5, &row_tile);
                enc.set_bytes(6, &col_tile);
                enc.set_bytes(7, &hidden_col_tiles);
                enc.dispatch_threadgroups((rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));
            }

            enc.set_pipeline(&split_pso);
            enc.set_buffer(0, qkv_f32, 0);
            enc.set_buffer(1, q_half, 0);
            enc.set_buffer(2, k_half, 0);
            enc.set_buffer(3, v_half, 0);
            enc.dispatch_threads(
                QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim,
                256,
            );

            enc.set_pipeline(&activate_pso);
            enc.set_buffer(0, beta_raw, 0);
            enc.set_buffer(1, gate_raw, 0);
            enc.set_buffer(2, beta, 0);
            enc.set_buffer(3, gate, 0);
            enc.dispatch_threads(QWEN35_08B.deltanet_v_heads, 16);

            enc.set_pipeline(&step_pso);
            enc.set_buffer(0, q_half, 0);
            enc.set_buffer(1, k_half, 0);
            enc.set_buffer(2, v_half, 0);
            enc.set_buffer(3, beta, 0);
            enc.set_buffer(4, gate, 0);
            enc.set_buffer(5, state, (block_index * 3 + d_index) * state_stride_bytes);
            enc.set_buffer(6, delta_f32, 0);
            enc.dispatch_threadgroups((QWEN35_08B.deltanet_v_heads, 1, 1), (128, 1, 1));

            enc.set_pipeline(&z_gate_pso);
            enc.set_buffer(0, delta_f32, 0);
            enc.set_buffer(1, z_f32, 0);
            enc.set_buffer(2, delta_half, 0);
            enc.dispatch_threads(
                QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim,
                256,
            );

            enc.set_pipeline(&delta_out_pso);
            enc.set_buffer(0, delta_half, 0);
            enc.set_buffer(1, delta_out, 0);
            enc.set_buffer(2, hidden_f32, 0);
            enc.set_bytes(3, &hidden_rows);
            enc.set_bytes(4, &row_tile);
            enc.set_bytes(5, &col_tile);
            enc.set_bytes(6, &delta_col_tiles);
            enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

            enc.set_pipeline(&residual_pso);
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, hidden_f32, 0);
            enc.set_buffer(2, output, 0);
            enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

            current_is_a = !current_is_a;
            let input = if current_is_a { hidden_a } else { hidden_b };
            let output = if current_is_a { hidden_b } else { hidden_a };

            enc.set_pipeline(&rms_project_pso);
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, norm, 0);
            enc.set_buffer(2, ffn_gate, 0);
            enc.set_buffer(3, ffn_gate_f32, 0);
            enc.set_bytes(4, &intermediate_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &col_tile);
            enc.set_bytes(7, &hidden_col_tiles);
            enc.dispatch_threadgroups(
                (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
                (256, 1, 1),
            );

            enc.set_pipeline(&rms_project_pso);
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, norm, 0);
            enc.set_buffer(2, ffn_up, 0);
            enc.set_buffer(3, ffn_up_f32, 0);
            enc.set_bytes(4, &intermediate_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &col_tile);
            enc.set_bytes(7, &hidden_col_tiles);
            enc.dispatch_threadgroups(
                (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
                (256, 1, 1),
            );

            enc.set_pipeline(&swiglu_pso);
            enc.set_buffer(0, ffn_gate_f32, 0);
            enc.set_buffer(1, ffn_up_f32, 0);
            enc.set_buffer(2, ffn_act, 0);
            enc.dispatch_threads(QWEN35_08B.ffn_intermediate, 256);

            enc.set_pipeline(&ffn_down_pso);
            enc.set_buffer(0, ffn_act, 0);
            enc.set_buffer(1, ffn_down, 0);
            enc.set_buffer(2, hidden_f32, 0);
            enc.set_bytes(3, &hidden_rows);
            enc.set_bytes(4, &row_tile);
            enc.set_bytes(5, &col_tile);
            enc.set_bytes(6, &intermediate_col_tiles);
            enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

            enc.set_pipeline(&residual_pso);
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, hidden_f32, 0);
            enc.set_buffer(2, output, 0);
            enc.dispatch_threads(QWEN35_08B.hidden_size, 256);
            current_is_a = !current_is_a;
        }

        let input = if current_is_a { hidden_a } else { hidden_b };
        let output = if current_is_a { hidden_b } else { hidden_a };
        for (weights, out, projection_rows) in [
            (attn_q, attn_q_f32, attention_q_rows),
            (attn_k, attn_k_f32, attention_kv_rows),
            (attn_v, attn_v_f32, attention_kv_rows),
        ] {
            enc.set_pipeline(&rms_project_pso);
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, norm, 0);
            enc.set_buffer(2, weights, 0);
            enc.set_buffer(3, out, 0);
            enc.set_bytes(4, &projection_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &col_tile);
            enc.set_bytes(7, &hidden_col_tiles);
            enc.dispatch_threadgroups(
                (projection_rows.div_ceil(row_tile) as usize, 1, 1),
                (256, 1, 1),
            );
        }

        enc.set_pipeline(&attn_pso);
        enc.set_buffer(0, attn_q_f32, 0);
        enc.set_buffer(1, attn_k_f32, 0);
        enc.set_buffer(2, attn_v_f32, 0);
        enc.set_buffer(3, attn_k_cache, 0);
        enc.set_buffer(4, attn_v_cache, 0);
        enc.set_buffer(5, attn_half, 0);
        enc.set_bytes(6, &attention_q_rows);
        enc.set_bytes(7, &position);
        enc.set_bytes(8, &max_context);
        enc.dispatch_threadgroups((QWEN35_08B.attention_q_heads, 1, 1), (256, 1, 1));

        enc.set_pipeline(&project2048_pso);
        enc.set_buffer(0, attn_half, 0);
        enc.set_buffer(1, attn_o, 0);
        enc.set_buffer(2, hidden_f32, 0);
        enc.set_bytes(3, &hidden_rows);
        enc.set_bytes(4, &row_tile);
        enc.set_bytes(5, &col_tile);
        enc.set_bytes(6, &attention_col_tiles);
        enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

        enc.set_pipeline(&residual_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, hidden_f32, 0);
        enc.set_buffer(2, output, 0);
        enc.dispatch_threads(QWEN35_08B.hidden_size, 256);
        current_is_a = !current_is_a;

        let input = if current_is_a { hidden_a } else { hidden_b };
        let output = if current_is_a { hidden_b } else { hidden_a };
        enc.set_pipeline(&rms_project_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, ffn_gate, 0);
        enc.set_buffer(3, ffn_gate_f32, 0);
        enc.set_bytes(4, &intermediate_rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &col_tile);
        enc.set_bytes(7, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
            (256, 1, 1),
        );

        enc.set_pipeline(&rms_project_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, ffn_up, 0);
        enc.set_buffer(3, ffn_up_f32, 0);
        enc.set_bytes(4, &intermediate_rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &col_tile);
        enc.set_bytes(7, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
            (256, 1, 1),
        );

        enc.set_pipeline(&swiglu_pso);
        enc.set_buffer(0, ffn_gate_f32, 0);
        enc.set_buffer(1, ffn_up_f32, 0);
        enc.set_buffer(2, ffn_act, 0);
        enc.dispatch_threads(QWEN35_08B.ffn_intermediate, 256);

        enc.set_pipeline(&ffn_down_pso);
        enc.set_buffer(0, ffn_act, 0);
        enc.set_buffer(1, ffn_down, 0);
        enc.set_buffer(2, hidden_f32, 0);
        enc.set_bytes(3, &hidden_rows);
        enc.set_bytes(4, &row_tile);
        enc.set_bytes(5, &col_tile);
        enc.set_bytes(6, &intermediate_col_tiles);
        enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

        enc.set_pipeline(&residual_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, hidden_f32, 0);
        enc.set_buffer(2, output, 0);
        enc.dispatch_threads(QWEN35_08B.hidden_size, 256);
        current_is_a = !current_is_a;
    }

    if !include_lm_head {
        enc.end();
        if wait_until_completed {
            cmd.commit_and_wait()?;
        } else {
            cmd.commit();
        }
        return Ok(current_is_a);
    }

    let lm_input = if current_is_a { hidden_a } else { hidden_b };
    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, lm_input, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &vocab_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    enc.dispatch_threadgroups((vocab_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

    let mut n = vocab_rows;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.end();
    cmd.commit_and_wait()?;
    Ok(input_is_a)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_decode_layered_pattern_tiled_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    final_norm: &crate::metal::ffi::Buffer,
    delta_layers: &[DeltaLayerBuffers],
    attention_layers: &[AttentionLayerBuffers],
    ffn_layers: &[FfnLayerBuffers],
    lm_head: &crate::metal::ffi::Buffer,
    hidden_a: &crate::metal::ffi::Buffer,
    hidden_b: &crate::metal::ffi::Buffer,
    qkv_f32: &crate::metal::ffi::Buffer,
    z_f32: &crate::metal::ffi::Buffer,
    beta_raw: &crate::metal::ffi::Buffer,
    gate_raw: &crate::metal::ffi::Buffer,
    q_half: &crate::metal::ffi::Buffer,
    k_half: &crate::metal::ffi::Buffer,
    v_half: &crate::metal::ffi::Buffer,
    beta: &crate::metal::ffi::Buffer,
    gate: &crate::metal::ffi::Buffer,
    state: &crate::metal::ffi::Buffer,
    state_stride_bytes: usize,
    conv_state: &crate::metal::ffi::Buffer,
    conv_state_stride_bytes: usize,
    delta_f32: &crate::metal::ffi::Buffer,
    delta_half: &crate::metal::ffi::Buffer,
    attn_q_f32: &crate::metal::ffi::Buffer,
    attn_k_f32: &crate::metal::ffi::Buffer,
    attn_v_f32: &crate::metal::ffi::Buffer,
    attn_k_cache: &crate::metal::ffi::Buffer,
    attn_v_cache: &crate::metal::ffi::Buffer,
    attn_cache_stride_bytes: usize,
    attn_half: &crate::metal::ffi::Buffer,
    attn_partial_m: &crate::metal::ffi::Buffer,
    attn_partial_l: &crate::metal::ffi::Buffer,
    attn_partial_acc: &crate::metal::ffi::Buffer,
    attn_splitk_blocks: u32,
    _ffn_gate_f32: &crate::metal::ffi::Buffer,
    _ffn_up_f32: &crate::metal::ffi::Buffer,
    ffn_act: &crate::metal::ffi::Buffer,
    _hidden_f32: &crate::metal::ffi::Buffer,
    scores_a: &crate::metal::ffi::Buffer,
    ids_a: &crate::metal::ffi::Buffer,
    scores_b: &crate::metal::ffi::Buffer,
    ids_b: &crate::metal::ffi::Buffer,
    out_token: &crate::metal::ffi::Buffer,
    out_score: &crate::metal::ffi::Buffer,
    vocab_rows: u32,
    hidden_rows: u32,
    intermediate_rows: u32,
    attention_q_rows: u32,
    attention_kv_rows: u32,
    delta_rows: u32,
    qkv_rows: u32,
    gate_rows: u32,
    row_tile: u32,
    col_tile: u32,
    hidden_col_tiles: u32,
    attention_col_tiles: u32,
    delta_col_tiles: u32,
    intermediate_col_tiles: u32,
    position: u32,
    max_context: u32,
    include_lm_head: bool,
    wait_until_completed: bool,
) -> Result<bool, String> {
    let gather_pso = dev.pipeline("qwen35_08b_embedding_gather_fp16_tiled_k1024_f32")?;
    let deltanet_project_pso =
        dev.pipeline("qwen35_08b_deltanet_qkv_z_b_a_rms_project_f32_tiled_k1024")?;
    let attention_project_pso =
        dev.pipeline("qwen35_08b_attention_q_k_v_rms_project_f32_tiled_k1024")?;
    let conv_pso = dev.pipeline("qwen35_08b_deltanet_causal_conv1d_update_silu_c6144_k4")?;
    let split_norm_pso = dev.pipeline("qwen35_08b_deltanet_split_qkv_norm_f32_to_fp16_h16d128")?;
    let activate_pso = dev.pipeline("qwen35_08b_deltanet_activate_beta_decay_h16")?;
    let step_pso = dev.pipeline(decode_deltanet_step_kernel())?;
    let z_gate_pso = dev.pipeline("qwen35_08b_deltanet_gated_rmsnorm_f32_to_fp16_h16d128")?;
    let out2048_residual_pso =
        dev.pipeline("qwen35_08b_matvec_residual_rowtiles_fp16_tiled_k2048_f32")?;
    let decode_attention_qh4 = decode_attention_qh4_enabled();
    let decode_attention_splitk = decode_attention_splitk_enabled(position) && decode_attention_qh4;
    let decode_attention_splitk_block = decode_attention_splitk_block_size();
    let attn_pso = if decode_attention_qh4 {
        dev.pipeline("qwen35_08b_attention_norm_rope_cache_qh4_gqa8_kv2_d256_to_fp16")?
    } else {
        dev.pipeline("qwen35_08b_attention_norm_rope_cache_gqa8_kv2_d256_to_fp16")?
    };
    let ffn_gate_up_pso =
        dev.pipeline("qwen35_08b_ffn_gate_up_swiglu_rowtiles_f32_tiled_k1024_i3584")?;
    let ffn_down_residual_pso =
        dev.pipeline("qwen35_08b_matvec_residual_rowtiles_fp16_tiled_k3584_f32")?;
    let final_norm_pso = dev.pipeline("qwen35_08b_rmsnorm_hidden_f32_k1024")?;
    let score_pso = dev.pipeline(decode_lm_head_argmax_kernel())?;
    let reduce_pso = dev.pipeline("qwen35_08b_argmax_pairs_reduce_f32")?;
    let compact_pso = dev.pipeline("qwen35_08b_argmax_pair_to_token_score")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;

    enc.set_pipeline(&gather_pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, hidden_a, 0);
    enc.set_bytes(3, &vocab_rows);
    enc.set_bytes(4, &row_tile);
    enc.set_bytes(5, &col_tile);
    enc.set_bytes(6, &hidden_col_tiles);
    enc.dispatch_threads(QWEN35_08B.hidden_size, 256);

    let mut current_is_a = true;
    let mut d_i = 0usize;
    let mut a_i = 0usize;
    for layer_i in 0..QWEN35_08B.n_layers {
        if layer_i % 4 == 3 {
            let input = if current_is_a { hidden_a } else { hidden_b };
            let output = if current_is_a { hidden_b } else { hidden_a };
            let layer = &attention_layers[a_i];
            let attention_projection_groups = attention_q_rows.div_ceil(row_tile)
                + attention_kv_rows.div_ceil(row_tile)
                + attention_kv_rows.div_ceil(row_tile);
            enc.set_pipeline(&attention_project_pso);
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, &layer.input_norm, 0);
            enc.set_buffer(2, &layer.q, 0);
            enc.set_buffer(3, &layer.k, 0);
            enc.set_buffer(4, &layer.v, 0);
            enc.set_buffer(5, attn_q_f32, 0);
            enc.set_buffer(6, attn_k_f32, 0);
            enc.set_buffer(7, attn_v_f32, 0);
            enc.set_bytes(8, &attention_q_rows);
            enc.set_bytes(9, &attention_kv_rows);
            enc.set_bytes(10, &attention_kv_rows);
            enc.set_bytes(11, &row_tile);
            enc.set_bytes(12, &col_tile);
            enc.set_bytes(13, &hidden_col_tiles);
            enc.dispatch_threadgroups((attention_projection_groups as usize, 1, 1), (256, 1, 1));

            if decode_attention_splitk {
                let active_attn_splitk_blocks = position
                    .saturating_add(1)
                    .min(max_context)
                    .div_ceil(decode_attention_splitk_block as u32)
                    .max(1)
                    .min(attn_splitk_blocks);
                let attn_splitk_partial_pso = dev.pipeline(
                    decode_attention_splitk_partial_kernel(decode_attention_splitk_block),
                )?;
                let attn_splitk_combine_pso = dev.pipeline(
                    "qwen35_08b_attention_norm_rope_cache_qh4_splitk256_combine_gqa8_kv2_d256_to_fp16",
                )?;
                enc.set_pipeline(&attn_splitk_partial_pso);
                enc.set_buffer(0, attn_q_f32, 0);
                enc.set_buffer(1, attn_k_f32, 0);
                enc.set_buffer(2, attn_v_f32, 0);
                enc.set_buffer(3, &layer.q_norm, 0);
                enc.set_buffer(4, &layer.k_norm, 0);
                enc.set_buffer(5, attn_k_cache, a_i * attn_cache_stride_bytes);
                enc.set_buffer(6, attn_v_cache, a_i * attn_cache_stride_bytes);
                enc.set_buffer(7, attn_partial_m, 0);
                enc.set_buffer(8, attn_partial_l, 0);
                enc.set_buffer(9, attn_partial_acc, 0);
                enc.set_bytes(10, &attention_q_rows);
                enc.set_bytes(11, &position);
                enc.set_bytes(12, &max_context);
                enc.set_bytes(13, &active_attn_splitk_blocks);
                enc.dispatch_threadgroups(
                    (
                        QWEN35_08B.attention_kv_heads,
                        active_attn_splitk_blocks as usize,
                        1,
                    ),
                    (256, 1, 1),
                );

                enc.set_pipeline(&attn_splitk_combine_pso);
                enc.set_buffer(0, attn_q_f32, 0);
                enc.set_buffer(1, attn_partial_m, 0);
                enc.set_buffer(2, attn_partial_l, 0);
                enc.set_buffer(3, attn_partial_acc, 0);
                enc.set_buffer(4, attn_half, 0);
                enc.set_bytes(5, &attention_q_rows);
                enc.set_bytes(6, &active_attn_splitk_blocks);
                enc.dispatch_threadgroups((QWEN35_08B.attention_kv_heads, 1, 1), (256, 1, 1));
            } else {
                enc.set_pipeline(&attn_pso);
                enc.set_buffer(0, attn_q_f32, 0);
                enc.set_buffer(1, attn_k_f32, 0);
                enc.set_buffer(2, attn_v_f32, 0);
                enc.set_buffer(3, &layer.q_norm, 0);
                enc.set_buffer(4, &layer.k_norm, 0);
                enc.set_buffer(5, attn_k_cache, a_i * attn_cache_stride_bytes);
                enc.set_buffer(6, attn_v_cache, a_i * attn_cache_stride_bytes);
                enc.set_buffer(7, attn_half, 0);
                enc.set_bytes(8, &attention_q_rows);
                enc.set_bytes(9, &position);
                enc.set_bytes(10, &max_context);
                enc.dispatch_threadgroups(
                    (
                        if decode_attention_qh4 {
                            QWEN35_08B.attention_kv_heads
                        } else {
                            QWEN35_08B.attention_q_heads
                        },
                        1,
                        1,
                    ),
                    (256, 1, 1),
                );
            }

            enc.set_pipeline(&out2048_residual_pso);
            enc.set_buffer(0, attn_half, 0);
            enc.set_buffer(1, &layer.o, 0);
            enc.set_buffer(2, input, 0);
            enc.set_buffer(3, output, 0);
            enc.set_bytes(4, &hidden_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &col_tile);
            enc.set_bytes(7, &attention_col_tiles);
            enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

            current_is_a = !current_is_a;
            a_i += 1;
        } else {
            let input = if current_is_a { hidden_a } else { hidden_b };
            let output = if current_is_a { hidden_b } else { hidden_a };
            let layer = &delta_layers[d_i];
            let deltanet_projection_groups = qkv_rows.div_ceil(row_tile)
                + delta_rows.div_ceil(row_tile)
                + gate_rows.div_ceil(row_tile)
                + gate_rows.div_ceil(row_tile);
            enc.set_pipeline(&deltanet_project_pso);
            enc.set_buffer(0, input, 0);
            enc.set_buffer(1, &layer.input_norm, 0);
            enc.set_buffer(2, &layer.qkv, 0);
            enc.set_buffer(3, &layer.z, 0);
            enc.set_buffer(4, &layer.b, 0);
            enc.set_buffer(5, &layer.a, 0);
            enc.set_buffer(6, qkv_f32, 0);
            enc.set_buffer(7, z_f32, 0);
            enc.set_buffer(8, beta_raw, 0);
            enc.set_buffer(9, gate_raw, 0);
            enc.set_bytes(10, &qkv_rows);
            enc.set_bytes(11, &delta_rows);
            enc.set_bytes(12, &gate_rows);
            enc.set_bytes(13, &gate_rows);
            enc.set_bytes(14, &row_tile);
            enc.set_bytes(15, &col_tile);
            enc.set_bytes(16, &hidden_col_tiles);
            enc.dispatch_threadgroups((deltanet_projection_groups as usize, 1, 1), (256, 1, 1));

            enc.set_pipeline(&conv_pso);
            enc.set_buffer(0, qkv_f32, 0);
            enc.set_buffer(1, conv_state, d_i * conv_state_stride_bytes);
            enc.set_buffer(2, &layer.conv_weight, 0);
            enc.set_buffer(3, &layer.conv_bias, 0);
            enc.set_buffer(4, qkv_f32, 0);
            enc.dispatch_threads(qkv_rows as usize, 256);

            enc.set_pipeline(&split_norm_pso);
            enc.set_buffer(0, qkv_f32, 0);
            enc.set_buffer(1, q_half, 0);
            enc.set_buffer(2, k_half, 0);
            enc.set_buffer(3, v_half, 0);
            enc.dispatch_threadgroups((QWEN35_08B.deltanet_v_heads, 1, 1), (128, 1, 1));

            enc.set_pipeline(&activate_pso);
            enc.set_buffer(0, beta_raw, 0);
            enc.set_buffer(1, gate_raw, 0);
            enc.set_buffer(2, &layer.a_log, 0);
            enc.set_buffer(3, &layer.dt_bias, 0);
            enc.set_buffer(4, beta, 0);
            enc.set_buffer(5, gate, 0);
            enc.dispatch_threads(QWEN35_08B.deltanet_v_heads, 16);

            enc.set_pipeline(&step_pso);
            enc.set_buffer(0, q_half, 0);
            enc.set_buffer(1, k_half, 0);
            enc.set_buffer(2, v_half, 0);
            enc.set_buffer(3, beta, 0);
            enc.set_buffer(4, gate, 0);
            enc.set_buffer(5, state, d_i * state_stride_bytes);
            enc.set_buffer(6, delta_f32, 0);
            enc.dispatch_threadgroups((QWEN35_08B.deltanet_v_heads, 1, 1), (128, 1, 1));

            enc.set_pipeline(&z_gate_pso);
            enc.set_buffer(0, delta_f32, 0);
            enc.set_buffer(1, z_f32, 0);
            enc.set_buffer(2, &layer.gated_norm, 0);
            enc.set_buffer(3, delta_half, 0);
            enc.dispatch_threads(QWEN35_08B.deltanet_v_heads, 16);

            enc.set_pipeline(&out2048_residual_pso);
            enc.set_buffer(0, delta_half, 0);
            enc.set_buffer(1, &layer.out, 0);
            enc.set_buffer(2, input, 0);
            enc.set_buffer(3, output, 0);
            enc.set_bytes(4, &hidden_rows);
            enc.set_bytes(5, &row_tile);
            enc.set_bytes(6, &col_tile);
            enc.set_bytes(7, &delta_col_tiles);
            enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

            current_is_a = !current_is_a;
            d_i += 1;
        }

        let input = if current_is_a { hidden_a } else { hidden_b };
        let output = if current_is_a { hidden_b } else { hidden_a };
        let ffn = &ffn_layers[layer_i];
        enc.set_pipeline(&ffn_gate_up_pso);
        enc.set_buffer(0, input, 0);
        enc.set_buffer(1, &ffn.post_norm, 0);
        enc.set_buffer(2, &ffn.gate, 0);
        enc.set_buffer(3, &ffn.up, 0);
        enc.set_buffer(4, ffn_act, 0);
        enc.set_bytes(5, &intermediate_rows);
        enc.set_bytes(6, &row_tile);
        enc.set_bytes(7, &col_tile);
        enc.set_bytes(8, &hidden_col_tiles);
        enc.dispatch_threadgroups(
            (intermediate_rows.div_ceil(row_tile) as usize, 1, 1),
            (256, 1, 1),
        );

        enc.set_pipeline(&ffn_down_residual_pso);
        enc.set_buffer(0, ffn_act, 0);
        enc.set_buffer(1, &ffn.down, 0);
        enc.set_buffer(2, input, 0);
        enc.set_buffer(3, output, 0);
        enc.set_bytes(4, &hidden_rows);
        enc.set_bytes(5, &row_tile);
        enc.set_bytes(6, &col_tile);
        enc.set_bytes(7, &intermediate_col_tiles);
        enc.dispatch_threadgroups((hidden_rows.div_ceil(row_tile) as usize, 1, 1), (256, 1, 1));

        current_is_a = !current_is_a;
    }

    if !include_lm_head {
        enc.end();
        if wait_until_completed {
            cmd.commit_and_wait()?;
        } else {
            cmd.commit();
        }
        return Ok(current_is_a);
    }

    let lm_input = if current_is_a { hidden_a } else { hidden_b };
    let lm_normed = if current_is_a { hidden_b } else { hidden_a };
    let lm_normed_is_a = !current_is_a;
    enc.set_pipeline(&final_norm_pso);
    enc.set_buffer(0, lm_input, 0);
    enc.set_buffer(1, final_norm, 0);
    enc.set_buffer(2, lm_normed, 0);
    enc.dispatch_threadgroups((1, 1, 1), (256, 1, 1));

    enc.set_pipeline(&score_pso);
    enc.set_buffer(0, lm_normed, 0);
    enc.set_buffer(1, lm_head, 0);
    enc.set_buffer(2, scores_a, 0);
    enc.set_buffer(3, ids_a, 0);
    enc.set_bytes(4, &vocab_rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &hidden_col_tiles);
    let vocab_rowtile_groups = vocab_rows.div_ceil(row_tile);
    enc.dispatch_threadgroups((vocab_rowtile_groups as usize, 1, 1), (256, 1, 1));

    let mut n = vocab_rowtile_groups;
    let mut input_is_a = true;
    while n > 1 {
        let groups = n.div_ceil(256);
        enc.set_pipeline(&reduce_pso);
        if input_is_a {
            enc.set_buffer(0, scores_a, 0);
            enc.set_buffer(1, ids_a, 0);
            enc.set_buffer(2, scores_b, 0);
            enc.set_buffer(3, ids_b, 0);
        } else {
            enc.set_buffer(0, scores_b, 0);
            enc.set_buffer(1, ids_b, 0);
            enc.set_buffer(2, scores_a, 0);
            enc.set_buffer(3, ids_a, 0);
        }
        enc.set_bytes(4, &n);
        enc.dispatch_threadgroups((groups as usize, 1, 1), (256, 1, 1));
        n = groups;
        input_is_a = !input_is_a;
    }

    enc.set_pipeline(&compact_pso);
    if input_is_a {
        enc.set_buffer(0, scores_a, 0);
        enc.set_buffer(1, ids_a, 0);
    } else {
        enc.set_buffer(0, scores_b, 0);
        enc.set_buffer(1, ids_b, 0);
    }
    enc.set_buffer(2, out_token, 0);
    enc.set_buffer(3, out_score, 0);
    enc.dispatch_threads(1, 1);

    enc.end();
    if wait_until_completed {
        cmd.commit_and_wait()?;
    } else {
        cmd.commit();
    }
    Ok(lm_normed_is_a)
}

pub fn run_synthetic_mega_bench(
    config: SyntheticMegaBenchConfig,
) -> Result<SyntheticMegaBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    let layers = config.layers;
    if rows == 0 {
        return Err("synthetic mega vocab rows must be > 0".to_string());
    }
    if layers == 0 || layers > QWEN35_08B.n_layers {
        return Err(format!(
            "synthetic mega layers must be 1..={}",
            QWEN35_08B.n_layers
        ));
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let layers_u32 = u32::try_from(layers).map_err(|_| "layers exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let layer_weights = dev.new_buffer(layers * cols * cols * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let out_token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let out_score = dev.new_buffer(std::mem::size_of::<f32>())?;

    let token_host = [config.input_token];
    let emb_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 257.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let layer_host: Vec<u16> = (0..layers * cols * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(7) % 127) as f32 - 63.0) / 2048.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let lm_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(19) % 251) as f32 - 125.0) / 251.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, &emb_host);
        layer_weights.write(0, &layer_host);
        lm_head.write(0, &lm_host);
    }

    for _ in 0..config.warmup {
        dispatch_synthetic_mega_once(
            &dev,
            &token,
            &embedding,
            &layer_weights,
            &lm_head,
            &out_token,
            &out_score,
            rows_u32,
            layers_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_synthetic_mega_once(
            &dev,
            &token,
            &embedding,
            &layer_weights,
            &lm_head,
            &out_token,
            &out_score,
            rows_u32,
            layers_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut next_token = [0u32; 1];
    let mut score = [0.0f32; 1];
    unsafe {
        out_token.read(0, &mut next_token);
        out_score.read(0, &mut score);
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let bytes_moved = rows * cols * 2 + layers * cols * cols * 2 + rows * cols * 2;
    let estimated_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(SyntheticMegaBenchResult {
        vocab_rows: rows,
        cols,
        input_token: config.input_token,
        layers,
        iterations: config.iterations,
        median_s,
        p95_s,
        estimated_gb_s,
        next_token: next_token[0],
        score: score[0],
    })
}

#[allow(clippy::too_many_arguments)]
fn dispatch_synthetic_mega_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    layer_weights: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    out_token: &crate::metal::ffi::Buffer,
    out_score: &crate::metal::ffi::Buffer,
    rows: u32,
    layers: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_synthetic_mega_decode_fp16")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, layer_weights, 0);
    enc.set_buffer(3, lm_head, 0);
    enc.set_buffer(4, out_token, 0);
    enc.set_buffer(5, out_score, 0);
    enc.set_bytes(6, &rows);
    enc.set_bytes(7, &layers);
    enc.dispatch_threadgroups((1, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

pub fn run_pattern_mega_bench(
    config: PatternMegaBenchConfig,
) -> Result<PatternMegaBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    if rows == 0 {
        return Err("pattern mega vocab rows must be > 0".to_string());
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let attention_weights = dev.new_buffer(
        QWEN35_08B.n_full_attention_layers() * cols * cols * std::mem::size_of::<u16>(),
    )?;
    let recurrent_state =
        dev.new_buffer(QWEN35_08B.n_deltanet_layers() * cols * std::mem::size_of::<f32>())?;
    let lm_head = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let out_token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let out_score = dev.new_buffer(std::mem::size_of::<f32>())?;

    let token_host = [config.input_token];
    let emb_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 257.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let attn_host: Vec<u16> = (0..QWEN35_08B.n_full_attention_layers() * cols * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(7) % 127) as f32 - 63.0) / 4096.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let state_host: Vec<f32> = (0..QWEN35_08B.n_deltanet_layers() * cols)
        .map(|i| ((i.wrapping_mul(11) % 127) as f32 - 63.0) / 1024.0)
        .collect();
    let lm_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(19) % 251) as f32 - 125.0) / 251.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, &emb_host);
        attention_weights.write(0, &attn_host);
        recurrent_state.write(0, &state_host);
        lm_head.write(0, &lm_host);
    }

    for _ in 0..config.warmup {
        dispatch_pattern_mega_once(
            &dev,
            &token,
            &embedding,
            &attention_weights,
            &recurrent_state,
            &lm_head,
            &out_token,
            &out_score,
            rows_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_pattern_mega_once(
            &dev,
            &token,
            &embedding,
            &attention_weights,
            &recurrent_state,
            &lm_head,
            &out_token,
            &out_score,
            rows_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut next_token = [0u32; 1];
    let mut score = [0.0f32; 1];
    unsafe {
        out_token.read(0, &mut next_token);
        out_score.read(0, &mut score);
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let bytes_moved = rows * cols * 2
        + QWEN35_08B.n_full_attention_layers() * cols * cols * 2
        + QWEN35_08B.n_deltanet_layers() * cols * std::mem::size_of::<f32>() * 2
        + rows * cols * 2;
    let estimated_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PatternMegaBenchResult {
        vocab_rows: rows,
        cols,
        input_token: config.input_token,
        layers: QWEN35_08B.n_layers,
        iterations: config.iterations,
        median_s,
        p95_s,
        estimated_gb_s,
        next_token: next_token[0],
        score: score[0],
    })
}

#[allow(clippy::too_many_arguments)]
fn dispatch_pattern_mega_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    attention_weights: &crate::metal::ffi::Buffer,
    recurrent_state: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    out_token: &crate::metal::ffi::Buffer,
    out_score: &crate::metal::ffi::Buffer,
    rows: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_pattern_mega_decode_fp16")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, attention_weights, 0);
    enc.set_buffer(3, recurrent_state, 0);
    enc.set_buffer(4, lm_head, 0);
    enc.set_buffer(5, out_token, 0);
    enc.set_buffer(6, out_score, 0);
    enc.set_bytes(7, &rows);
    enc.dispatch_threadgroups((1, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

pub fn run_pattern_ffn_mega_bench(
    config: PatternFfnMegaBenchConfig,
) -> Result<PatternFfnMegaBenchResult, String> {
    let rows = config.vocab_rows;
    let cols = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    if rows == 0 {
        return Err("pattern FFN mega vocab rows must be > 0".to_string());
    }
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;

    let dev = Device::default_system()?;
    let token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let embedding = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let attention_weights = dev.new_buffer(
        QWEN35_08B.n_full_attention_layers() * cols * cols * std::mem::size_of::<u16>(),
    )?;
    let recurrent_state =
        dev.new_buffer(QWEN35_08B.n_deltanet_layers() * cols * std::mem::size_of::<f32>())?;
    let gate_w = dev.new_buffer(intermediate * cols * std::mem::size_of::<u16>())?;
    let up_w = dev.new_buffer(intermediate * cols * std::mem::size_of::<u16>())?;
    let down_w = dev.new_buffer(cols * intermediate * std::mem::size_of::<u16>())?;
    let lm_head = dev.new_buffer(rows * cols * std::mem::size_of::<u16>())?;
    let out_token = dev.new_buffer(std::mem::size_of::<u32>())?;
    let out_score = dev.new_buffer(std::mem::size_of::<f32>())?;

    let token_host = [config.input_token];
    let emb_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 257.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let attn_host: Vec<u16> = (0..QWEN35_08B.n_full_attention_layers() * cols * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(7) % 127) as f32 - 63.0) / 4096.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let state_host: Vec<f32> = (0..QWEN35_08B.n_deltanet_layers() * cols)
        .map(|i| ((i.wrapping_mul(11) % 127) as f32 - 63.0) / 1024.0)
        .collect();
    let gate_host: Vec<u16> = (0..intermediate * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(13) % 251) as f32 - 125.0) / 512.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let up_host: Vec<u16> = (0..intermediate * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(17) % 257) as f32 - 128.0) / 512.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let down_host: Vec<u16> = (0..cols * intermediate)
        .map(|i| {
            let v = ((i.wrapping_mul(19) % 263) as f32 - 131.0) / 1024.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    let lm_host: Vec<u16> = (0..rows * cols)
        .map(|i| {
            let v = ((i.wrapping_mul(23) % 269) as f32 - 134.0) / 269.0;
            f16::from_f32(v).to_bits()
        })
        .collect();
    unsafe {
        token.write(0, &token_host);
        embedding.write(0, &emb_host);
        attention_weights.write(0, &attn_host);
        recurrent_state.write(0, &state_host);
        gate_w.write(0, &gate_host);
        up_w.write(0, &up_host);
        down_w.write(0, &down_host);
        lm_head.write(0, &lm_host);
    }

    for _ in 0..config.warmup {
        dispatch_pattern_ffn_mega_once(
            &dev,
            &token,
            &embedding,
            &attention_weights,
            &recurrent_state,
            &gate_w,
            &up_w,
            &down_w,
            &lm_head,
            &out_token,
            &out_score,
            rows_u32,
        )?;
    }

    let mut samples = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let start = Instant::now();
        dispatch_pattern_ffn_mega_once(
            &dev,
            &token,
            &embedding,
            &attention_weights,
            &recurrent_state,
            &gate_w,
            &up_w,
            &down_w,
            &lm_head,
            &out_token,
            &out_score,
            rows_u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let mut next_token = [0u32; 1];
    let mut score = [0.0f32; 1];
    unsafe {
        out_token.read(0, &mut next_token);
        out_score.read(0, &mut score);
    }

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let ffn_bytes_per_layer = intermediate * cols * 2 * 2 + cols * intermediate * 2;
    let bytes_moved = rows * cols * 2
        + QWEN35_08B.n_full_attention_layers() * cols * cols * 2
        + QWEN35_08B.n_layers * ffn_bytes_per_layer
        + rows * cols * 2;
    let estimated_gb_s = bytes_moved as f64 / median_s.max(1e-12) / 1e9;

    Ok(PatternFfnMegaBenchResult {
        vocab_rows: rows,
        cols,
        intermediate,
        input_token: config.input_token,
        layers: QWEN35_08B.n_layers,
        iterations: config.iterations,
        median_s,
        p95_s,
        estimated_gb_s,
        next_token: next_token[0],
        score: score[0],
    })
}

#[allow(clippy::too_many_arguments)]
fn dispatch_pattern_ffn_mega_once(
    dev: &Device,
    token: &crate::metal::ffi::Buffer,
    embedding: &crate::metal::ffi::Buffer,
    attention_weights: &crate::metal::ffi::Buffer,
    recurrent_state: &crate::metal::ffi::Buffer,
    gate_w: &crate::metal::ffi::Buffer,
    up_w: &crate::metal::ffi::Buffer,
    down_w: &crate::metal::ffi::Buffer,
    lm_head: &crate::metal::ffi::Buffer,
    out_token: &crate::metal::ffi::Buffer,
    out_score: &crate::metal::ffi::Buffer,
    rows: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_pattern_ffn_mega_decode_fp16")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, token, 0);
    enc.set_buffer(1, embedding, 0);
    enc.set_buffer(2, attention_weights, 0);
    enc.set_buffer(3, recurrent_state, 0);
    enc.set_buffer(4, gate_w, 0);
    enc.set_buffer(5, up_w, 0);
    enc.set_buffer(6, down_w, 0);
    enc.set_buffer(7, lm_head, 0);
    enc.set_buffer(8, out_token, 0);
    enc.set_buffer(9, out_score, 0);
    enc.set_bytes(10, &rows);
    enc.dispatch_threadgroups((1, 1, 1), (256, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}
