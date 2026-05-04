#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("memory_forensics is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{format_bytes, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use std::path::PathBuf;

    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() < 2 || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(());
    }

    let metalpack = PathBuf::from(&args[1]);
    let tokens = parse_arg(&args, 2, "tokens")?.unwrap_or(512);
    let iterations = parse_arg(&args, 3, "iterations")?.unwrap_or(3);
    let sustained_gb_s = parse_f64_arg(&args, 4, "sustained-gb-s")?.unwrap_or(90.0);
    let mps_ffn_sidecar = args.get(5).map(PathBuf::from);
    let mps_delta_project_sidecar = args.get(6).map(PathBuf::from);
    let mps_attention_out_sidecar = args.get(7).map(PathBuf::from);
    let mps_delta_out_sidecar = args.get(8).map(PathBuf::from);
    if mps_delta_project_sidecar.is_some() && mps_ffn_sidecar.is_none() {
        return Err(
            "mps-delta-project-sidecar-dir currently requires mps-ffn-sidecar-dir".to_owned(),
        );
    }
    if tokens == 0 {
        return Err("tokens must be > 0".to_owned());
    }
    if iterations == 0 {
        return Err("iterations must be > 0".to_owned());
    }

    let bandwidth_bytes_s = sustained_gb_s * 1.0e9;
    let mut rows = Vec::new();

    rows.push(run_delta18_ffn(
        &metalpack,
        tokens,
        iterations,
        mps_ffn_sidecar.as_deref(),
        mps_delta_project_sidecar.as_deref(),
        mps_delta_out_sidecar.as_deref(),
    )?);
    rows.push(run_attention_core(
        &metalpack,
        tokens,
        iterations,
        mps_attention_out_sidecar.as_deref(),
        attention_mps_tiled_enabled(),
    )?);
    rows.push(run_attention_ffn(
        &metalpack,
        tokens,
        iterations,
        mps_ffn_sidecar.as_deref(),
    )?);

    let delta = rows
        .iter()
        .find(|row| row.name == "delta18+ffn")
        .ok_or_else(|| "missing delta18+ffn row".to_owned())?;
    let attention = rows
        .iter()
        .find(|row| row.name == "attention.core")
        .ok_or_else(|| "missing attention.core row".to_owned())?;
    let attention_ffn = rows
        .iter()
        .find(|row| row.name == "attention.ffn")
        .ok_or_else(|| "missing attention.ffn row".to_owned())?;

    let full_prefill_est_s = delta.median_s + 6.0 * (attention.median_s + attention_ffn.median_s);
    let full_prefill_est_tok_s = tokens as f64 / full_prefill_est_s.max(1e-12);

    println!("qwen35-08b memory forensics");
    println!("metalpack: {}", metalpack.display());
    println!("tokens: {tokens}");
    println!("iterations: {iterations}");
    println!("sustained_bandwidth_assumption: {sustained_gb_s:.2} GB/s");
    if let Some(sidecar) = &mps_ffn_sidecar {
        println!("delta18_mps_ffn_sidecar: {}", sidecar.display());
    }
    if let Some(sidecar) = &mps_delta_project_sidecar {
        println!("delta18_mps_delta_project_sidecar: {}", sidecar.display());
    }
    if let Some(sidecar) = &mps_attention_out_sidecar {
        println!("attention_mps_out_sidecar: {}", sidecar.display());
    }
    if attention_mps_tiled_enabled() {
        println!("attention_backend: exact MPS tiled QK-softmax-PV");
    } else {
        println!("attention_backend: accepted QH4 SIMD32 vec8");
    }
    if delta_scan_lanes4_sharedqk_enabled() {
        println!("delta_scan_backend: approximate SIMD32 lanes4_sharedqk");
    } else {
        println!("delta_scan_backend: exact rowcache_block32");
    }
    if let Some(sidecar) = &mps_delta_out_sidecar {
        println!("delta18_mps_delta_out_sidecar: {}", sidecar.display());
    }
    println!(
        "counter_limit: this Mac exposes GPUTimestamp only; L2/cache-miss rows below are inferred from byte floors, not hardware cache counters."
    );
    println!();
    println!(
        "{:<20} {:>10} {:>10} {:>12} {:>12} {:>12} {:>9}  {}",
        "scope",
        "median_ms",
        "eff_GB/s",
        "model_bytes",
        "floor_ms",
        "excess@BW",
        "ratio",
        "diagnosis"
    );
    for row in &rows {
        let floor_s = row.model_bytes as f64 / bandwidth_bytes_s;
        let implied_bytes = row.median_s * bandwidth_bytes_s;
        let excess = implied_bytes - row.model_bytes as f64;
        let ratio = row.median_s / floor_s.max(1e-12);
        println!(
            "{:<20} {:>10.3} {:>10.2} {:>12} {:>12.3} {:>12} {:>8.2}x  {}",
            row.name,
            row.median_s * 1_000.0,
            row.model_bytes as f64 / row.median_s.max(1e-12) / 1.0e9,
            format_bytes(row.model_bytes),
            floor_s * 1_000.0,
            format_signed_bytes(excess),
            ratio,
            row.diagnosis
        );
        if let Some(bench_gb_s) = row.benchmark_reported_gb_s {
            println!(
                "{:<20} {:>10} {:>10.2} {:>12} {:>12} {:>12} {:>9}  benchmark internal byte estimate",
                "",
                "",
                bench_gb_s,
                format_bytes(row.benchmark_reported_bytes()),
                "",
                "",
                ""
            );
        }
        print_cache_inference(row, implied_bytes);
    }
    println!();
    println!(
        "full_prefill_estimate_current_kernels: {:.3}s, {:.2} tok/s",
        full_prefill_est_s, full_prefill_est_tok_s
    );
    println!(
        "attention_t2_stream_floor: {} per attention layer at {tokens} tokens with {}.",
        format_bytes(attention_core_qh4_qblk1_bytes(tokens)),
        if attention_mps_tiled_enabled() {
            "MPS tiled exact attention; byte buckets are logical operands, not hardware DRAM counters"
        } else {
            "qh4 SIMD32 vec8 GQA KV sharing; long prefill is still limited by quadratic KV traffic"
        }
    );
    println!(
        "next_forensics_action: {}",
        if attention_mps_tiled_enabled() {
            "MPS tiled attention closes the exact long-prefill gap; next macro work is DeltaNet scan/gated-norm/out forensics and full profile promotion."
        } else {
            "DeltaNet prefill remains above the modeled byte floor after QKV/Z128; next macro work is scan recurrence math plus projection/register-pressure forensics, while attention still needs long-context compressed/block attention."
        }
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct ForensicsRow {
    name: &'static str,
    median_s: f64,
    model_bytes: usize,
    unique_weight_bytes: Option<usize>,
    bytes: Option<ByteBreakdown>,
    benchmark_reported_gb_s: Option<f64>,
    diagnosis: &'static str,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct ByteBreakdown {
    unique_weight_bytes: usize,
    weight_group_stream_bytes: usize,
    logical_operand_weight_bytes: usize,
    non_weight_bytes: usize,
    weight_reuse_floor_bytes: usize,
    persistent_reuse_floor_bytes: Option<usize>,
    attention_cache: Option<AttentionCacheBreakdown>,
    token_tile_summary: &'static str,
    token_group_summary: String,
    max_tail_underfill: f64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct AttentionCacheBreakdown {
    per_query_logical_kv_bytes: usize,
    qblk_logical_kv_bytes: usize,
    qhead_shared_kv_bytes: usize,
    unique_kv_cache_bytes: usize,
}

#[cfg(target_os = "macos")]
impl ForensicsRow {
    fn benchmark_reported_bytes(&self) -> usize {
        self.benchmark_reported_gb_s
            .map(|gb_s| (gb_s * 1.0e9 * self.median_s) as usize)
            .unwrap_or(0)
    }
}

#[cfg(target_os = "macos")]
fn run_delta18_ffn(
    metalpack: &std::path::Path,
    tokens: usize,
    iterations: usize,
    mps_ffn_sidecar: Option<&std::path::Path>,
    mps_delta_project_sidecar: Option<&std::path::Path>,
    mps_delta_out_sidecar: Option<&std::path::Path>,
) -> Result<ForensicsRow, String> {
    let mut bench_args = vec![
        metalpack.display().to_string(),
        "0".to_owned(),
        tokens.to_string(),
        iterations.to_string(),
        "1".to_owned(),
        "18".to_owned(),
    ];
    if let Some(sidecar) = mps_ffn_sidecar {
        bench_args.push(sidecar.display().to_string());
    }
    if let Some(sidecar) = mps_delta_project_sidecar {
        bench_args.push(sidecar.display().to_string());
    }
    if let Some(sidecar) = mps_delta_out_sidecar {
        bench_args.push(sidecar.display().to_string());
    }
    let mut bench_envs = vec![
        ("CTOX_QWEN35_PROJECT_SPLIT_NORM", "1"),
        ("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128", "1"),
        ("CTOX_QWEN35_DOWN_MMA64", "1"),
        ("CTOX_QWEN35_DOWN_MMA64_RESIDUAL", "1"),
        ("CTOX_QWEN35_DELTA_OUT_MMA64", "1"),
        ("CTOX_QWEN35_FFN_GATE_UP_MMA64", "1"),
        ("CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED", "1"),
    ];
    if delta_scan_lanes4_sharedqk_enabled() {
        bench_envs.push(("CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK", "1"));
    } else {
        bench_envs.push(("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1"));
        bench_envs.push(("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32", "1"));
    }
    let output = run_sibling_bin(
        "bench_metalpack_prefill_delta3_ffn_superblock",
        &bench_args,
        &bench_envs,
    )?;
    let median_s = parse_required_metric(&output, "median_s")?;
    let gb_s = parse_metric_containing(&output, "effective_gb_s_delta_ffn_stack_estimate");
    let packed_bytes = parse_usize_metric_containing(&output, "packed_bytes");
    let model_bytes = gb_s
        .map(|value| (value * 1.0e9 * median_s) as usize)
        .ok_or_else(|| "delta18+ffn benchmark did not report effective GB/s".to_owned())?;
    let bytes = if mps_delta_project_sidecar.is_some() {
        delta18_mps_sidecar_byte_breakdown(tokens, packed_bytes.unwrap_or(0), model_bytes)
    } else {
        delta18_ffn_byte_breakdown(tokens, true, true)
    };
    Ok(ForensicsRow {
        name: "delta18+ffn",
        median_s,
        model_bytes,
        unique_weight_bytes: packed_bytes,
        bytes: Some(bytes),
        benchmark_reported_gb_s: gb_s,
        diagnosis: if mps_delta_project_sidecar.is_some() {
            if mps_delta_out_sidecar.is_some() {
                "MPS FFN + MPS QKV/Z + MPS DeltaOut sidecars; scan traffic is now the largest Delta gap"
            } else {
                "MPS FFN + MPS QKV/Z sidecars; remaining weight-stream/scan traffic still above byte floor"
            }
        } else if mps_ffn_sidecar.is_some() {
            "MPS FFN sidecar; QKV/Z128, DeltaOut64, fused conv/split, scan rowcache_block32 active"
        } else {
            "weight-stream dominated; QKV/Z128, DeltaOut64, GateUp64, Down64 residual-in-MMA, fused conv/split, scan rowcache_block32 active"
        },
    })
}

#[cfg(target_os = "macos")]
fn run_attention_core(
    metalpack: &std::path::Path,
    tokens: usize,
    iterations: usize,
    mps_attention_out_sidecar: Option<&std::path::Path>,
    use_mps_tiled_attention: bool,
) -> Result<ForensicsRow, String> {
    let mut bench_args = vec![
        metalpack.display().to_string(),
        "3".to_owned(),
        tokens.to_string(),
        iterations.to_string(),
        "1".to_owned(),
    ];
    if let Some(sidecar) = mps_attention_out_sidecar {
        bench_args.push(sidecar.display().to_string());
    }
    let envs = if use_mps_tiled_attention {
        vec![
            ("CTOX_QWEN35_DELTA_OUT_MMA16", "1"),
            ("CTOX_QWEN35_ATTENTION_MPS_TILED", "1"),
        ]
    } else {
        vec![
            ("CTOX_QWEN35_DELTA_OUT_MMA16", "1"),
            ("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8", "1"),
        ]
    };
    let remove_envs = if use_mps_tiled_attention {
        &["CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8"][..]
    } else {
        &["CTOX_QWEN35_ATTENTION_MPS_TILED"][..]
    };
    let output = run_sibling_bin_with_removed_env(
        "bench_metalpack_prefill_attention_core",
        &bench_args,
        &envs,
        remove_envs,
    )?;
    let median_s = parse_required_metric(&output, "median_s")?;
    let gb_s = parse_metric_containing(&output, "effective_gb_s_attention_core_estimate");
    let packed_bytes = parse_usize_metric_containing(&output, "packed_bytes");
    Ok(ForensicsRow {
        name: "attention.core",
        median_s,
        model_bytes: attention_core_forensic_bytes(tokens, mps_attention_out_sidecar.is_some()),
        unique_weight_bytes: packed_bytes,
        bytes: Some(attention_core_byte_breakdown(
            tokens,
            mps_attention_out_sidecar.is_some(),
        )),
        benchmark_reported_gb_s: gb_s,
        diagnosis: if use_mps_tiled_attention {
            "exact MPS tiled QK-softmax-PV attention; matrix backend replaces custom per-query KV scan"
        } else if mps_attention_out_sidecar.is_some() {
            "qh4 SIMD32 vec8 barrier-free attention + MPS O-proj sidecar; scan now dominates"
        } else {
            "qh4 SIMD32 vec8 barrier-free attention; full GQA KV sharing without threadgroup reductions"
        },
    })
}

#[cfg(target_os = "macos")]
fn attention_mps_tiled_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_ATTENTION_MPS_TILED").is_some()
        || std::env::var_os("CTOX_QWEN35_FORENSICS_MPS_TILED_ATTENTION").is_some()
}

#[cfg(target_os = "macos")]
fn delta_scan_lanes4_sharedqk_enabled() -> bool {
    std::env::var_os("CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK").is_some()
        || std::env::var_os("CTOX_QWEN35_FORENSICS_DELTA_SCAN_LANES4_SHAREDQK").is_some()
}

#[cfg(target_os = "macos")]
fn run_attention_ffn(
    metalpack: &std::path::Path,
    tokens: usize,
    iterations: usize,
    mps_ffn_sidecar: Option<&std::path::Path>,
) -> Result<ForensicsRow, String> {
    if let Some(sidecar) = mps_ffn_sidecar {
        let output = run_sibling_bin(
            "bench_mps_ffn_sidecar_runtime",
            &[
                sidecar.display().to_string(),
                "3".to_owned(),
                tokens.to_string(),
                iterations.to_string(),
                "1".to_owned(),
                "1".to_owned(),
            ],
            &[],
        )?;
        let median_s = parse_required_metric(&output, "median_s")?;
        let bytes = ffn_mps_sidecar_byte_breakdown(tokens);
        return Ok(ForensicsRow {
            name: "attention.ffn",
            median_s,
            model_bytes: bytes.weight_reuse_floor_bytes,
            unique_weight_bytes: Some(bytes.unique_weight_bytes),
            bytes: Some(bytes),
            benchmark_reported_gb_s: None,
            diagnosis: "MPS FFN sidecar runtime; same backend class as Delta-layer FFNs",
        });
    }

    let output = run_sibling_bin(
        "bench_metalpack_prefill_ffn_block",
        &[
            metalpack.display().to_string(),
            "3".to_owned(),
            tokens.to_string(),
            iterations.to_string(),
        ],
        &[
            ("CTOX_QWEN35_DOWN_MMA64", "1"),
            ("CTOX_QWEN35_DOWN_MMA64_RESIDUAL", "1"),
            ("CTOX_QWEN35_FFN_GATE_UP_MMA64", "1"),
        ],
    )?;
    let median_s = parse_required_metric(&output, "median_s")?;
    let gb_s = parse_metric_containing(&output, "effective_gb_s_ffn_block_estimate")
        .or_else(|| parse_any_effective_gb_s(&output));
    let packed_bytes = parse_usize_metric_containing(&output, "packed_bytes");
    let model_bytes = gb_s
        .map(|value| (value * 1.0e9 * median_s) as usize)
        .unwrap_or_else(|| ffn_forensic_bytes(tokens));
    Ok(ForensicsRow {
        name: "attention.ffn",
        median_s,
        model_bytes,
        unique_weight_bytes: packed_bytes,
        bytes: Some(ffn_byte_breakdown(tokens)),
        benchmark_reported_gb_s: gb_s,
        diagnosis:
            "MMA64 gate/up + MMA64 down residual path; remaining work is launch/scratch traffic",
    })
}

#[cfg(target_os = "macos")]
fn run_sibling_bin(bin: &str, args: &[String], envs: &[(&str, &str)]) -> Result<String, String> {
    run_sibling_bin_with_removed_env(bin, args, envs, &[])
}

#[cfg(target_os = "macos")]
fn run_sibling_bin_with_removed_env(
    bin: &str,
    args: &[String],
    envs: &[(&str, &str)],
    remove_envs: &[&str],
) -> Result<String, String> {
    use std::process::Command;

    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let dir = exe
        .parent()
        .ok_or_else(|| format!("cannot resolve parent dir for {}", exe.display()))?;
    let sibling = dir.join(bin);
    if !sibling.exists() {
        return Err(format!(
            "missing benchmark binary `{}`; run `cargo build --release --bins` first",
            sibling.display()
        ));
    }

    let mut command = Command::new(&sibling);
    command.args(args);
    for key in remove_envs {
        command.env_remove(key);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().map_err(|err| {
        format!(
            "failed to run `{}` with args {:?}: {err}",
            sibling.display(),
            args
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "`{}` failed with status {}\nstdout:\n{}\nstderr:\n{}",
            sibling.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn parse_required_metric(output: &str, key: &str) -> Result<f64, String> {
    parse_metric_containing(output, key).ok_or_else(|| format!("missing metric `{key}`"))
}

#[cfg(target_os = "macos")]
fn parse_metric_containing(output: &str, key: &str) -> Option<f64> {
    output.lines().find_map(|line| {
        if !line.contains(key) {
            return None;
        }
        line.split_once(':')
            .and_then(|(_, rhs)| rhs.trim().parse::<f64>().ok())
    })
}

#[cfg(target_os = "macos")]
fn parse_usize_metric_containing(output: &str, key: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        if !line.contains(key) {
            return None;
        }
        let (_, rhs) = line.split_once(key)?;
        let value = rhs
            .trim_start_matches(['=', ':', ' '])
            .split(|ch: char| !ch.is_ascii_digit())
            .next()?;
        value.parse::<usize>().ok()
    })
}

#[cfg(target_os = "macos")]
fn parse_any_effective_gb_s(output: &str) -> Option<f64> {
    output.lines().find_map(|line| {
        if !line.contains("effective_gb_s") {
            return None;
        }
        line.split_once(':')
            .and_then(|(_, rhs)| rhs.trim().parse::<f64>().ok())
    })
}

#[cfg(target_os = "macos")]
fn print_cache_inference(row: &ForensicsRow, dram_equiv_bytes: f64) {
    let modeled = row.model_bytes as f64;
    let cache_or_overcount_lb = if modeled > dram_equiv_bytes {
        (modeled - dram_equiv_bytes) / modeled
    } else {
        0.0
    };
    let unmodeled_or_stall = if dram_equiv_bytes > modeled {
        dram_equiv_bytes - modeled
    } else {
        0.0
    };
    let unique = row.bytes.as_ref().map_or_else(
        || {
            row.unique_weight_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "n/a".to_owned())
        },
        |bytes| format_bytes(bytes.unique_weight_bytes),
    );
    let weight_stream_reuse = row
        .bytes
        .as_ref()
        .filter(|bytes| bytes.unique_weight_bytes > 0)
        .map(|bytes| bytes.weight_group_stream_bytes as f64 / bytes.unique_weight_bytes as f64);
    let weight_stream_reuse = weight_stream_reuse
        .map(|value| format!("{value:.1}x"))
        .unwrap_or_else(|| "n/a".to_owned());
    println!(
        "{:<20} {:>10} {:>10} {:>12} {:>12} {:>12} {:>9}  cache inference: dram_equiv={}, cache_or_model_overcount_lb={:.1}%, unmodeled_or_stall={}, unique_weights={}, weight_stream/unique={}",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        format_bytes(dram_equiv_bytes.max(0.0) as usize),
        cache_or_overcount_lb * 100.0,
        format_bytes(unmodeled_or_stall.max(0.0) as usize),
        unique,
        weight_stream_reuse
    );
    if let Some(bytes) = &row.bytes {
        let reuse_opportunity = bytes
            .logical_operand_weight_bytes
            .saturating_sub(bytes.weight_group_stream_bytes);
        let reuse_rate = if bytes.logical_operand_weight_bytes == 0 {
            0.0
        } else {
            reuse_opportunity as f64 / bytes.logical_operand_weight_bytes as f64
        };
        println!(
            "{:<20} {:>10} {:>10} {:>12} {:>12} {:>12} {:>9}  byte buckets: weights_stream={}, logical_weight_operands={}, reuse_opportunity={} ({:.1}%), non_weight={}, tiles={}, groups={}, max_tail_underfill={:.1}%",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            format_bytes(bytes.weight_group_stream_bytes),
            format_bytes(bytes.logical_operand_weight_bytes),
            format_bytes(reuse_opportunity),
            reuse_rate * 100.0,
            format_bytes(bytes.non_weight_bytes),
            bytes.token_tile_summary,
            bytes.token_group_summary,
            bytes.max_tail_underfill * 100.0
        );
        let weight_floor_gap = row
            .model_bytes
            .saturating_sub(bytes.weight_reuse_floor_bytes);
        let weight_floor_gap_rate = if row.model_bytes == 0 {
            0.0
        } else {
            weight_floor_gap as f64 / row.model_bytes as f64
        };
        println!(
            "{:<20} {:>10} {:>10} {:>12} {:>12} {:>12} {:>9}  weight-reuse floor: {}, stream/cache-miss budget above weight floor: {} ({:.1}%)",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            format_bytes(bytes.weight_reuse_floor_bytes),
            format_bytes(weight_floor_gap),
            weight_floor_gap_rate * 100.0
        );
        if let Some(persistent_floor) = bytes.persistent_reuse_floor_bytes {
            let persistent_gap = row.model_bytes.saturating_sub(persistent_floor);
            let persistent_gap_rate = if row.model_bytes == 0 {
                0.0
            } else {
                persistent_gap as f64 / row.model_bytes as f64
            };
            println!(
                "{:<20} {:>10} {:>10} {:>12} {:>12} {:>12} {:>9}  persistent/cache-resident floor: {}, current stream budget above persistent floor: {} ({:.1}%)",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                format_bytes(persistent_floor),
                format_bytes(persistent_gap),
                persistent_gap_rate * 100.0
            );
        }
        if let Some(attention) = &bytes.attention_cache {
            let qblk_saved = attention
                .per_query_logical_kv_bytes
                .saturating_sub(attention.qblk_logical_kv_bytes);
            let qhead_saved = attention
                .qblk_logical_kv_bytes
                .saturating_sub(attention.qhead_shared_kv_bytes);
            let cache_resident_gap = attention
                .qhead_shared_kv_bytes
                .saturating_sub(attention.unique_kv_cache_bytes);
            println!(
                "{:<20} {:>10} {:>10} {:>12} {:>12} {:>12} {:>9}  attention KV forensics: per_query_logical={}, qblk_logical={}, qblk_saved={}, cross_qhead_reuse_possible={}, cache_resident_floor={}, cache_residency_gap={}",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                format_bytes(attention.per_query_logical_kv_bytes),
                format_bytes(attention.qblk_logical_kv_bytes),
                format_bytes(qblk_saved),
                format_bytes(qhead_saved),
                format_bytes(attention.unique_kv_cache_bytes),
                format_bytes(cache_resident_gap)
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn attention_core_forensic_bytes(tokens: usize, mps_attention_out_sidecar: bool) -> usize {
    let bytes = attention_core_byte_breakdown(tokens, mps_attention_out_sidecar);
    bytes.weight_group_stream_bytes + bytes.non_weight_bytes
}

#[cfg(target_os = "macos")]
fn delta18_ffn_byte_breakdown(
    tokens: usize,
    residual_mma: bool,
    conv_split_fused: bool,
) -> ByteBreakdown {
    let h = QWEN35_08B.hidden_size;
    let i = QWEN35_08B.ffn_intermediate;
    let qkv_rows = QWEN35_08B.deltanet_qkv_width();
    let delta_width = QWEN35_08B.deltanet_width();
    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let layers = QWEN35_08B.n_deltanet_layers();
    let qkv_weights = qkv_rows * h * 2;
    let z_weights = delta_width * h * 2;
    let ba_weights = heads * h * 2 * 2;
    let delta_out_weights = h * delta_width * 2;
    let ffn_gate_up_weights = i * h * 2 * 2;
    let ffn_down_weights = h * i * 2;
    let unique_per_layer = qkv_weights
        + z_weights
        + ba_weights
        + delta_out_weights
        + ffn_gate_up_weights
        + ffn_down_weights;
    let qkvz_tile = 128;
    let gate_up_tile = 64;
    let down_tile = 64;
    let qkvz_groups = tokens.div_ceil(qkvz_tile);
    let ba_groups = tokens.div_ceil(4);
    let out_tile = 64;
    let out_groups = tokens.div_ceil(out_tile);
    let gate_up_groups = tokens.div_ceil(gate_up_tile);
    let down_groups = tokens.div_ceil(down_tile);
    let weight_stream_per_layer = qkvz_groups * (qkv_weights + z_weights)
        + ba_groups * ba_weights
        + out_groups * delta_out_weights
        + gate_up_groups * ffn_gate_up_weights
        + down_groups * ffn_down_weights;
    let state_bytes = heads * head_dim * head_dim * std::mem::size_of::<f32>();
    let state_stream_bytes = state_bytes * 2;
    let residual_mma_saved_bytes = if residual_mma {
        tokens * h * std::mem::size_of::<f32>() * 4
    } else {
        0
    };
    let conv_split_fused_saved_bytes = if conv_split_fused {
        tokens * qkv_rows * std::mem::size_of::<f32>() * 2
    } else {
        0
    };
    let non_weight_per_layer_before_residual_fusion =
        tokens * qkv_rows * (std::mem::size_of::<f32>() * 2 + 8)
            + state_stream_bytes
            + tokens * (h * 2 * 3 + h * 4 * 2 + i * 2);
    let non_weight_per_layer = non_weight_per_layer_before_residual_fusion
        .saturating_sub(residual_mma_saved_bytes)
        .saturating_sub(conv_split_fused_saved_bytes);
    ByteBreakdown {
        unique_weight_bytes: unique_per_layer * layers,
        weight_group_stream_bytes: weight_stream_per_layer * layers,
        logical_operand_weight_bytes: unique_per_layer * tokens * layers,
        non_weight_bytes: non_weight_per_layer * layers,
        weight_reuse_floor_bytes: (unique_per_layer + non_weight_per_layer) * layers,
        persistent_reuse_floor_bytes: None,
        attention_cache: None,
        token_tile_summary: "mixed qkv/z=128,b/a=4,out=64,gate/up=64,down=64",
        token_group_summary: format!(
            "qkvz={qkvz_groups}@{qkvz_tile}, b/a={ba_groups}@4, out={out_groups}@{out_tile}, gate_up={gate_up_groups}@{gate_up_tile}, down={down_groups}@{down_tile}"
        ),
        max_tail_underfill: max_tail_underfill(tokens, &[qkvz_tile, 4, out_tile, gate_up_tile, down_tile]),
    }
}

#[cfg(target_os = "macos")]
fn delta18_mps_sidecar_byte_breakdown(
    tokens: usize,
    unique_weight_bytes: usize,
    model_bytes: usize,
) -> ByteBreakdown {
    let old = delta18_ffn_byte_breakdown(tokens, true, true);
    let non_weight_bytes = model_bytes.saturating_sub(unique_weight_bytes);
    ByteBreakdown {
        unique_weight_bytes,
        weight_group_stream_bytes: unique_weight_bytes,
        logical_operand_weight_bytes: old.logical_operand_weight_bytes,
        non_weight_bytes,
        weight_reuse_floor_bytes: unique_weight_bytes + non_weight_bytes,
        persistent_reuse_floor_bytes: None,
        attention_cache: None,
        token_tile_summary: "MPS qkv/z + MPS ffn sidecars; b/a=4,out=64",
        token_group_summary: "MPS matrices stream sidecar weights once per layer; custom MSL remains for b/a, scan, gated norm, and DeltaOut".to_owned(),
        max_tail_underfill: max_tail_underfill(tokens, &[4, 64]),
    }
}

#[cfg(target_os = "macos")]
fn attention_core_byte_breakdown(tokens: usize, mps_attention_out_sidecar: bool) -> ByteBreakdown {
    let h = QWEN35_08B.hidden_size;
    let q_rows = QWEN35_08B.attention_q_with_head_gate_width();
    let kv_rows = QWEN35_08B.attention_kv_width();
    let q_width = QWEN35_08B.attention_q_width();
    let project_tile = attention_project_token_tile(tokens);
    let out_tile = attention_out_token_tile(tokens);
    let project_groups = tokens.div_ceil(project_tile);
    let out_groups = tokens.div_ceil(out_tile);
    let project_unique = (q_rows * h + kv_rows * h * 2) * 2;
    let out_unique = h * q_width * 2;
    let hidden_io_bytes = tokens * (h * 2 + h * 2);
    let prepare_bytes = tokens * (q_rows * 4 + kv_rows * 4 * 2 + q_width * 2 + kv_rows * 2 * 2);
    let attention_query_block = 1;
    let attention_per_query_bytes = attention_core_query_block_bytes(tokens, 1);
    let attention_bytes = attention_core_qh4_qblk1_bytes(tokens);
    let attention_cache_resident_bytes = tokens * (q_width * 2 + kv_rows * 2 * 2);
    let qhead_shared_attention_bytes = attention_bytes;
    let unique_kv_cache_bytes =
        tokens * QWEN35_08B.attention_kv_heads * QWEN35_08B.attention_head_dim * 2 * 2;
    let out_io_bytes = tokens * (q_width * 2 + h * 4);
    let cache_reuse_non_weight_bytes =
        hidden_io_bytes + prepare_bytes + attention_cache_resident_bytes + out_io_bytes;
    let out_weight_stream_bytes = if mps_attention_out_sidecar {
        out_unique
    } else {
        out_groups * out_unique
    };
    let out_group_summary = if mps_attention_out_sidecar {
        "mps".to_owned()
    } else {
        format!("{out_groups}@{out_tile}")
    };
    ByteBreakdown {
        unique_weight_bytes: project_unique + out_unique,
        weight_group_stream_bytes: project_groups * project_unique + out_weight_stream_bytes,
        logical_operand_weight_bytes: tokens * (project_unique + out_unique),
        non_weight_bytes: hidden_io_bytes + prepare_bytes + attention_bytes + out_io_bytes,
        weight_reuse_floor_bytes: project_unique
            + out_unique
            + hidden_io_bytes
            + prepare_bytes
            + attention_bytes
            + out_io_bytes,
        persistent_reuse_floor_bytes: Some(project_unique + out_unique + cache_reuse_non_weight_bytes),
        attention_cache: Some(AttentionCacheBreakdown {
            per_query_logical_kv_bytes: attention_per_query_bytes,
            qblk_logical_kv_bytes: attention_bytes,
            qhead_shared_kv_bytes: qhead_shared_attention_bytes,
            unique_kv_cache_bytes,
        }),
        token_tile_summary: "mixed project/attention/out",
        token_group_summary: format!(
            "project={project_groups}@{project_tile}, attention={}@qh4/simd32_vec8/qblk{attention_query_block}, out={out_group_summary}",
            tokens * QWEN35_08B.attention_kv_heads
        ),
        max_tail_underfill: max_tail_underfill(tokens, &[project_tile, attention_query_block, out_tile]),
    }
}

#[cfg(target_os = "macos")]
fn attention_project_token_tile(tokens: usize) -> usize {
    if std::env::var_os("CTOX_QWEN35_ATTENTION_PROJECT_MMA8").is_some() {
        8
    } else if tokens.is_multiple_of(16) {
        16
    } else {
        8
    }
}

#[cfg(target_os = "macos")]
fn attention_out_token_tile(tokens: usize) -> usize {
    if tokens.is_multiple_of(16) {
        16
    } else {
        4
    }
}

#[cfg(target_os = "macos")]
fn attention_core_query_block_bytes(tokens: usize, query_block: usize) -> usize {
    let q_heads = QWEN35_08B.attention_q_heads;
    let head_dim = QWEN35_08B.attention_head_dim;
    let kv_bytes_per_key_per_head = head_dim * 2 * 2;
    let block_count = tokens.div_ceil(query_block);
    let mut bytes = 0usize;
    for block in 0..block_count {
        let query_start = block * query_block;
        let last_query = (query_start + query_block - 1).min(tokens - 1);
        bytes += (last_query + 1) * q_heads * kv_bytes_per_key_per_head;
    }
    bytes
}

#[cfg(target_os = "macos")]
fn attention_core_qh4_qblk1_bytes(tokens: usize) -> usize {
    let q_heads = QWEN35_08B.attention_q_heads;
    let kv_heads = QWEN35_08B.attention_kv_heads;
    let head_dim = QWEN35_08B.attention_head_dim;
    let kv_bytes_per_key_per_head = head_dim * 2 * 2;
    let head_groups = kv_heads.max(1);
    let _heads_per_group = q_heads / kv_heads.max(1);
    tokens * (tokens + 1) / 2 * head_groups * kv_bytes_per_key_per_head
}

#[cfg(target_os = "macos")]
fn ffn_forensic_bytes(tokens: usize) -> usize {
    let bytes = ffn_byte_breakdown(tokens);
    bytes.weight_group_stream_bytes + bytes.non_weight_bytes
}

#[cfg(target_os = "macos")]
fn ffn_byte_breakdown(tokens: usize) -> ByteBreakdown {
    let h = QWEN35_08B.hidden_size;
    let i = QWEN35_08B.ffn_intermediate;
    let gate_up_tile = 64;
    let down_tile = 64;
    let gate_up_groups = tokens.div_ceil(gate_up_tile);
    let down_groups = tokens.div_ceil(down_tile);
    let gate_up_unique = i * h * 2 * 2;
    let down_unique = h * i * 2;
    let activation_bytes = tokens * (h * 2 + h * 2 + i * 2 + h * 4);
    ByteBreakdown {
        unique_weight_bytes: gate_up_unique + down_unique,
        weight_group_stream_bytes: gate_up_groups * gate_up_unique + down_groups * down_unique,
        logical_operand_weight_bytes: tokens * (gate_up_unique + down_unique),
        non_weight_bytes: activation_bytes,
        weight_reuse_floor_bytes: gate_up_unique + down_unique + activation_bytes,
        persistent_reuse_floor_bytes: None,
        attention_cache: None,
        token_tile_summary: "gate/up + down",
        token_group_summary: format!(
            "gate_up={gate_up_groups}@{gate_up_tile}, down={down_groups}@{down_tile}"
        ),
        max_tail_underfill: max_tail_underfill(tokens, &[gate_up_tile, down_tile]),
    }
}

#[cfg(target_os = "macos")]
fn ffn_mps_sidecar_byte_breakdown(tokens: usize) -> ByteBreakdown {
    let h = QWEN35_08B.hidden_size;
    let i = QWEN35_08B.ffn_intermediate;
    let gate_up_unique = i * h * 2 * 2;
    let down_unique = h * i * 2;
    let activation_bytes = tokens * (h * 2 + i * 2 * 2 + i * 2 + h * 2 + h * 2);
    ByteBreakdown {
        unique_weight_bytes: gate_up_unique + down_unique,
        weight_group_stream_bytes: gate_up_unique + down_unique,
        logical_operand_weight_bytes: tokens * (gate_up_unique + down_unique),
        non_weight_bytes: activation_bytes,
        weight_reuse_floor_bytes: gate_up_unique + down_unique + activation_bytes,
        persistent_reuse_floor_bytes: None,
        attention_cache: None,
        token_tile_summary: "MPS gate/up + MPS down sidecar",
        token_group_summary: "MPS matrices stream sidecar weights once for this layer".to_owned(),
        max_tail_underfill: 0.0,
    }
}

#[cfg(target_os = "macos")]
fn max_tail_underfill(tokens: usize, tiles: &[usize]) -> f64 {
    tiles
        .iter()
        .filter(|tile| **tile > 0)
        .map(|tile| {
            let groups = tokens.div_ceil(*tile);
            if groups == 0 {
                0.0
            } else {
                1.0 - tokens as f64 / (groups * tile) as f64
            }
        })
        .fold(0.0, f64::max)
}

#[cfg(target_os = "macos")]
fn format_signed_bytes(bytes: f64) -> String {
    if bytes >= 0.0 {
        format!("+{}", format_bytes(bytes as usize))
    } else {
        format!("-{}", format_bytes((-bytes) as usize))
    }
}

#[cfg(target_os = "macos")]
fn parse_arg(args: &[std::ffi::OsString], idx: usize, name: &str) -> Result<Option<usize>, String> {
    args.get(idx)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid {name} argument `{arg}`: {err}"))
        })
        .transpose()
}

#[cfg(target_os = "macos")]
fn parse_f64_arg(
    args: &[std::ffi::OsString],
    idx: usize,
    name: &str,
) -> Result<Option<f64>, String> {
    args.get(idx)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<f64>()
                .map_err(|err| format!("invalid {name} argument `{arg}`: {err}"))
        })
        .transpose()
}

#[cfg(target_os = "macos")]
fn print_usage() {
    println!(
        "usage: memory_forensics <metalpack-dir> [tokens=512] [iterations=3] [sustained-gb-s=90] [mps-ffn-sidecar-dir] [mps-delta-project-sidecar-dir] [mps-attention-out-sidecar-dir] [mps-delta-out-sidecar-dir]"
    );
    println!("requires sibling release binaries; run `cargo build --release --bins` first.");
}
