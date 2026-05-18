#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("autotune_metalpack_prefill_delta_stack is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--print-baseline-env") {
        let families = families();
        let selection = baseline_selection(&families);
        for (key, value) in baseline_env(&families, &selection) {
            println!("{key}={value}");
        }
        return Ok(());
    }
    if args.len() < 2 {
        return Err("usage: autotune_metalpack_prefill_delta_stack <metalpack-dir> [tokens] [iterations] [warmup] [delta-layer-count] [start-layer] [passes] [mps-ffn-sidecar-dir] [mps-delta-project-sidecar-dir] [mps-delta-out-sidecar-dir]\n       autotune_metalpack_prefill_delta_stack --print-baseline-env".to_owned());
    }

    let metalpack = PathBuf::from(&args[1]);
    let tokens = parse_arg(&args, 2, 4096usize, "tokens")?;
    let iterations = parse_arg(&args, 3, 3usize, "iterations")?;
    let warmup = parse_arg(&args, 4, 1usize, "warmup")?;
    let layer_count = parse_arg(&args, 5, 18usize, "delta-layer-count")?;
    let start_layer = parse_arg(&args, 6, 0usize, "start-layer")?;
    let passes = parse_arg(&args, 7, 2usize, "passes")?;
    let sidecar_args = args
        .iter()
        .skip(8)
        .take(3)
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let exe = env::current_exe().map_err(|err| format!("failed to resolve current exe: {err}"))?;
    let bin_dir = exe
        .parent()
        .ok_or_else(|| format!("failed to resolve binary directory for {}", exe.display()))?;
    let bench = bin_dir.join("bench_metalpack_prefill_delta3_ffn_superblock");
    let compare = bin_dir.join("compare_half_dump");

    let families = families();
    let conservative_selection = baseline_selection(&families);
    let mut chosen = conservative_selection.clone();
    let mut history: Vec<EvalRecord> = Vec::new();
    let coordinate_max_checksum_delta =
        parse_env_f64("CTOX_QWEN35_AUTOTUNE_COORD_MAX_CHECKSUM_DELTA", 0.0001)?;

    println!("qwen35-08b DeltaNet+FFN autotuner");
    println!("metalpack: {}", metalpack.display());
    println!(
        "shape: tokens={tokens} delta_layers={layer_count} start_layer={start_layer} iterations={iterations} warmup={warmup} passes={passes}"
    );
    for (idx, sidecar) in sidecar_args.iter().enumerate() {
        println!("sidecar_arg_{}: {}", idx + 1, sidecar.display());
    }
    println!(
        "method: serial coordinate descent; no parallel benchmarks; score=median_s then p95_s; checksum-drift guard before selection; final candidate gets a hidden-dump correctness gate"
    );
    println!();

    let baseline = run_candidate(
        &bench,
        &metalpack,
        &families,
        &chosen,
        tokens,
        iterations,
        warmup,
        layer_count,
        start_layer,
        &sidecar_args,
    )?;
    println!(
        "baseline: {:<70} median={:.6}s p95={:.6}s eff={:.2}GB/s",
        selection_label(&families, &chosen),
        baseline.median_s,
        baseline.p95_s,
        baseline.effective_gb_s.unwrap_or(0.0)
    );
    let baseline_checksum = baseline.checksum;
    if let Some(checksum) = baseline_checksum {
        println!(
            "coordinate_checksum_guard: baseline={checksum:.6} max_delta={coordinate_max_checksum_delta:.6}"
        );
    } else {
        println!("coordinate_checksum_guard: disabled_missing_baseline_checksum");
    }
    let mut incumbent_selection = chosen.clone();
    let mut incumbent_result = baseline.clone();
    history.push(EvalRecord::new(
        "baseline".to_owned(),
        "baseline".to_owned(),
        "baseline".to_owned(),
        selection_label(&families, &chosen),
        &baseline,
        tokens,
    ));

    for pass in 0..passes {
        println!();
        println!("pass {}", pass + 1);
        for family_idx in 0..families.len() {
            let family = &families[family_idx];
            let mut best_choice = chosen[family_idx];
            let mut best_result: Option<BenchResult> = None;

            for candidate_idx in 0..family.candidates.len() {
                let mut trial = chosen.clone();
                trial[family_idx] = candidate_idx;
                let result = run_candidate(
                    &bench,
                    &metalpack,
                    &families,
                    &trial,
                    tokens,
                    iterations,
                    warmup,
                    layer_count,
                    start_layer,
                    &sidecar_args,
                )?;
                println!(
                    "  {:<10} {:<20} median={:.6}s p95={:.6}s eff={:.2}GB/s checksum={:.6}{}",
                    family.name,
                    family.candidates[candidate_idx].name,
                    result.median_s,
                    result.p95_s,
                    result.effective_gb_s.unwrap_or(0.0),
                    result.checksum.unwrap_or(0.0),
                    coordinate_checksum_suffix(
                        &result,
                        baseline_checksum,
                        coordinate_max_checksum_delta
                    )
                );
                history.push(EvalRecord::new(
                    format!("pass{}", pass + 1),
                    family.name.to_owned(),
                    family.candidates[candidate_idx].name.to_owned(),
                    selection_label(&families, &trial),
                    &result,
                    tokens,
                ));
                let checksum_ok = coordinate_checksum_ok(
                    &result,
                    baseline_checksum,
                    coordinate_max_checksum_delta,
                );
                let is_best = checksum_ok
                    && best_result
                        .as_ref()
                        .map(|best| result.better_than(best))
                        .unwrap_or(true);
                if is_best {
                    best_choice = candidate_idx;
                    best_result = Some(result.clone());
                }
                if checksum_ok && result.better_than(&incumbent_result) {
                    incumbent_selection = trial;
                    incumbent_result = result.clone();
                }
            }

            chosen[family_idx] = best_choice;
            let best = best_result.expect("family has at least one candidate");
            println!(
                "  -> choose {:<10} {:<14} median={:.6}s",
                family.name, family.candidates[best_choice].name, best.median_s
            );
            history.push(EvalRecord::new(
                format!("pass{}_choice", pass + 1),
                family.name.to_owned(),
                family.candidates[best_choice].name.to_owned(),
                selection_label(&families, &chosen),
                &best,
                tokens,
            ));
        }
    }

    let final_result = run_candidate(
        &bench,
        &metalpack,
        &families,
        &chosen,
        tokens,
        iterations,
        warmup,
        layer_count,
        start_layer,
        &sidecar_args,
    )?;
    if final_result.better_than(&incumbent_result) {
        incumbent_selection = chosen.clone();
        incumbent_result = final_result.clone();
    }
    history.push(EvalRecord::new(
        "final_coordinate".to_owned(),
        "all".to_owned(),
        "final".to_owned(),
        selection_label(&families, &chosen),
        &final_result,
        tokens,
    ));

    println!();
    println!(
        "final_coordinate_selection: {}",
        selection_label(&families, &chosen)
    );
    println!("final_coordinate_median_s: {:.9}", final_result.median_s);
    println!(
        "best_selection: {}",
        selection_label(&families, &incumbent_selection)
    );
    println!("best_env:");
    for (key, value) in selection_env(&families, &incumbent_selection) {
        println!("  {key}={value}");
    }
    println!("best_median_s: {:.9}", incumbent_result.median_s);
    println!("best_p95_s: {:.9}", incumbent_result.p95_s);
    println!(
        "best_tok_s_prefill_delta_stack_only: {:.2}",
        tokens as f64 / incumbent_result.median_s
    );
    println!("evaluations: {}", history.len());

    let validation = validate_candidate_against_baseline(
        &bench,
        &compare,
        &metalpack,
        &families,
        &conservative_selection,
        &incumbent_selection,
        tokens,
        layer_count,
        start_layer,
        &sidecar_args,
    )?;
    println!("correctness_gate: {}", validation.status_line());
    let accepted_selection = if validation.passed {
        &incumbent_selection
    } else {
        &conservative_selection
    };
    println!(
        "accepted_selection: {}",
        selection_label(&families, accepted_selection)
    );
    println!("accepted_env:");
    for (key, value) in selection_env(&families, accepted_selection) {
        println!("  {key}={value}");
    }

    let report_path = env::var_os("CTOX_QWEN35_AUTOTUNE_CSV")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::temp_dir().join(format!(
                "ctox_qwen35_autotune_delta_stack_{}.csv",
                std::process::id()
            ))
        });
    write_history_csv(&report_path, &history, &validation)?;
    println!("history_csv: {}", report_path.display());
    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct Family {
    name: &'static str,
    candidates: Vec<Candidate>,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct Candidate {
    name: &'static str,
    envs: Vec<(&'static str, &'static str)>,
    baseline: bool,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct BenchResult {
    median_s: f64,
    p95_s: f64,
    effective_gb_s: Option<f64>,
    checksum: Option<f64>,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct EvalRecord {
    phase: String,
    family: String,
    candidate: String,
    selection: String,
    median_s: f64,
    p95_s: f64,
    effective_gb_s: Option<f64>,
    model_bytes: Option<usize>,
    checksum: Option<f64>,
    tok_s: f64,
}

#[cfg(target_os = "macos")]
impl EvalRecord {
    fn new(
        phase: String,
        family: String,
        candidate: String,
        selection: String,
        result: &BenchResult,
        tokens: usize,
    ) -> Self {
        let model_bytes = result
            .effective_gb_s
            .map(|gb_s| (gb_s * 1.0e9 * result.median_s) as usize);
        Self {
            phase,
            family,
            candidate,
            selection,
            median_s: result.median_s,
            p95_s: result.p95_s,
            effective_gb_s: result.effective_gb_s,
            model_bytes,
            checksum: result.checksum,
            tok_s: tokens as f64 / result.median_s.max(1e-12),
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct ValidationResult {
    skipped: bool,
    passed: bool,
    baseline_dump: Option<std::path::PathBuf>,
    candidate_dump: Option<std::path::PathBuf>,
    mismatch_count: Option<usize>,
    mean_abs_error: Option<f64>,
    rms_error: Option<f64>,
    max_abs_error: Option<f64>,
    checksum_delta: Option<f64>,
    reason: String,
}

#[cfg(target_os = "macos")]
impl ValidationResult {
    fn status_line(&self) -> String {
        if self.skipped {
            return format!("SKIPPED ({})", self.reason);
        }
        let status = if self.passed { "PASS" } else { "FAIL" };
        format!(
            "{status} mean_abs={:.9} rms={:.9} max_abs={:.9} checksum_delta={:.9} mismatch_count={} reason={} baseline_dump={} candidate_dump={}",
            self.mean_abs_error.unwrap_or(0.0),
            self.rms_error.unwrap_or(0.0),
            self.max_abs_error.unwrap_or(0.0),
            self.checksum_delta.unwrap_or(0.0),
            self.mismatch_count.unwrap_or(0),
            self.reason,
            self.baseline_dump
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            self.candidate_dump
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
        )
    }
}

#[cfg(target_os = "macos")]
impl BenchResult {
    fn better_than(&self, other: &Self) -> bool {
        let median_eps = 0.0025;
        if self.median_s < other.median_s * (1.0 - median_eps) {
            return true;
        }
        (self.median_s - other.median_s).abs() <= other.median_s * median_eps
            && self.p95_s < other.p95_s
    }
}

#[cfg(target_os = "macos")]
fn coordinate_checksum_ok(
    result: &BenchResult,
    baseline_checksum: Option<f64>,
    max_delta: f64,
) -> bool {
    if max_delta < 0.0 {
        return true;
    }
    match (baseline_checksum, result.checksum) {
        (Some(baseline), Some(candidate)) => (candidate - baseline).abs() <= max_delta,
        _ => true,
    }
}

#[cfg(target_os = "macos")]
fn coordinate_checksum_suffix(
    result: &BenchResult,
    baseline_checksum: Option<f64>,
    max_delta: f64,
) -> String {
    if coordinate_checksum_ok(result, baseline_checksum, max_delta) {
        return String::new();
    }
    let delta = match (baseline_checksum, result.checksum) {
        (Some(baseline), Some(candidate)) => candidate - baseline,
        _ => 0.0,
    };
    format!(" reject=checksum_drift delta={delta:.6}")
}

#[cfg(target_os = "macos")]
fn families() -> Vec<Family> {
    vec![
        Family {
            name: "qkvz",
            candidates: vec![
                cand(
                    "mma8",
                    &[("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA8", "1")],
                    false,
                ),
                cand(
                    "mma16",
                    &[("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA16", "1")],
                    false,
                ),
                cand(
                    "mma32",
                    &[("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32", "1")],
                    false,
                ),
                cand(
                    "mma64",
                    &[("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64", "1")],
                    false,
                ),
                cand(
                    "mma128",
                    &[("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128", "1")],
                    true,
                ),
            ],
        },
        Family {
            name: "delta_out",
            candidates: vec![
                cand(
                    "mma32_res",
                    &[
                        ("CTOX_QWEN35_DELTA_OUT_MMA32", "1"),
                        ("CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL", "1"),
                    ],
                    false,
                ),
                cand("mma64_res", &[("CTOX_QWEN35_DELTA_OUT_MMA64", "1")], true),
            ],
        },
        Family {
            name: "gate_up",
            candidates: vec![
                cand("mma32", &[("CTOX_QWEN35_FFN_GATE_UP_MMA32", "1")], false),
                cand("mma64", &[("CTOX_QWEN35_FFN_GATE_UP_MMA64", "1")], true),
            ],
        },
        Family {
            name: "down",
            candidates: vec![
                cand(
                    "mma32_res",
                    &[
                        ("CTOX_QWEN35_DOWN_MMA32", "1"),
                        ("CTOX_QWEN35_DOWN_MMA32_RESIDUAL", "1"),
                    ],
                    false,
                ),
                cand(
                    "mma64_res",
                    &[
                        ("CTOX_QWEN35_DOWN_MMA64", "1"),
                        ("CTOX_QWEN35_DOWN_MMA64_RESIDUAL", "1"),
                    ],
                    true,
                ),
            ],
        },
        Family {
            name: "scan",
            candidates: vec![
                cand(
                    "rowcache",
                    &[("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1")],
                    false,
                ),
                cand(
                    "rowcache_gated_norm",
                    &[
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1"),
                        ("CTOX_QWEN35_DELTA_SCAN_GATED_NORM", "1"),
                    ],
                    false,
                ),
                cand(
                    "rowcache_direct",
                    &[
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1"),
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT", "1"),
                    ],
                    false,
                ),
                cand(
                    "rowcache_block64",
                    &[
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1"),
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64", "1"),
                    ],
                    false,
                ),
                cand(
                    "rowcache_block32",
                    &[
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1"),
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32", "1"),
                    ],
                    true,
                ),
                cand(
                    "rowcache_auto32_64",
                    &[
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1"),
                        ("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO", "1"),
                    ],
                    false,
                ),
                cand("lanes4", &[("CTOX_QWEN35_DELTA_SCAN_LANES4", "1")], false),
                cand(
                    "lanes4_sharedqk",
                    &[("CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK", "1")],
                    false,
                ),
                cand(
                    "lanes4_ordered",
                    &[("CTOX_QWEN35_DELTA_SCAN_LANES4_ORDERED", "1")],
                    false,
                ),
            ],
        },
        Family {
            name: "conv",
            candidates: vec![
                cand(
                    "fused",
                    &[("CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED", "1")],
                    true,
                ),
                cand(
                    "fused_tok4",
                    &[("CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED_TOK4", "1")],
                    false,
                ),
            ],
        },
    ]
}

#[cfg(target_os = "macos")]
fn cand(name: &'static str, envs: &[(&'static str, &'static str)], baseline: bool) -> Candidate {
    Candidate {
        name,
        envs: envs.to_vec(),
        baseline,
    }
}

#[cfg(target_os = "macos")]
fn baseline_selection(families: &[Family]) -> Vec<usize> {
    families
        .iter()
        .map(|family| {
            family
                .candidates
                .iter()
                .position(|candidate| candidate.baseline)
                .unwrap_or(0)
        })
        .collect()
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_candidate(
    bench: &std::path::Path,
    metalpack: &std::path::Path,
    families: &[Family],
    selection: &[usize],
    tokens: usize,
    iterations: usize,
    warmup: usize,
    layer_count: usize,
    start_layer: usize,
    sidecar_args: &[std::path::PathBuf],
) -> Result<BenchResult, String> {
    let mut cmd = std::process::Command::new(bench);
    cmd.arg(metalpack)
        .arg(start_layer.to_string())
        .arg(tokens.to_string())
        .arg(iterations.to_string())
        .arg(warmup.to_string())
        .arg(layer_count.to_string());
    for sidecar in sidecar_args {
        cmd.arg(sidecar);
    }

    for key in tuning_env_keys() {
        cmd.env_remove(key);
    }
    cmd.env("CTOX_QWEN35_PROJECT_SPLIT_NORM", "1");
    for (key, value) in selection_env(families, selection) {
        cmd.env(key, value);
    }

    let output = cmd
        .output()
        .map_err(|err| format!("failed to run {}: {err}", bench.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed with status {}\nselection: {}\nstdout:\n{}\nstderr:\n{}",
            bench.display(),
            output.status,
            selection_label(families, selection),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("benchmark output is not valid UTF-8: {err}"))?;
    Ok(BenchResult {
        median_s: parse_metric(&stdout, "median_s")?,
        p95_s: parse_metric(&stdout, "p95_s")?,
        effective_gb_s: parse_optional_metric_containing(&stdout, "effective_gb_s"),
        checksum: parse_optional_metric(&stdout, "checksum16"),
    })
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn validate_candidate_against_baseline(
    bench: &std::path::Path,
    compare: &std::path::Path,
    metalpack: &std::path::Path,
    families: &[Family],
    baseline_selection: &[usize],
    candidate_selection: &[usize],
    tokens: usize,
    layer_count: usize,
    start_layer: usize,
    sidecar_args: &[std::path::PathBuf],
) -> Result<ValidationResult, String> {
    if std::env::var_os("CTOX_QWEN35_AUTOTUNE_SKIP_VALIDATE").is_some() {
        return Ok(ValidationResult {
            skipped: true,
            passed: false,
            baseline_dump: None,
            candidate_dump: None,
            mismatch_count: None,
            mean_abs_error: None,
            rms_error: None,
            max_abs_error: None,
            checksum_delta: None,
            reason: "CTOX_QWEN35_AUTOTUNE_SKIP_VALIDATE is set".to_owned(),
        });
    }

    if baseline_selection == candidate_selection {
        return Ok(ValidationResult {
            skipped: false,
            passed: true,
            baseline_dump: None,
            candidate_dump: None,
            mismatch_count: Some(0),
            mean_abs_error: Some(0.0),
            rms_error: Some(0.0),
            max_abs_error: Some(0.0),
            checksum_delta: Some(0.0),
            reason: "best candidate matches conservative baseline".to_owned(),
        });
    }

    if !compare.exists() {
        return Err(format!(
            "missing compare binary `{}`; build `compare_half_dump` before running autotune validation",
            compare.display()
        ));
    }

    let tmp = std::env::temp_dir();
    let stamp = format!(
        "ctox_qwen35_autotune_{}_{}_{}",
        std::process::id(),
        tokens,
        layer_count
    );
    let baseline_dump = tmp.join(format!("{stamp}_baseline.bin"));
    let candidate_dump = tmp.join(format!("{stamp}_candidate.bin"));
    let validation_iterations = parse_env_usize("CTOX_QWEN35_AUTOTUNE_VALIDATE_ITERATIONS", 1)?;
    let validation_warmup = parse_env_usize("CTOX_QWEN35_AUTOTUNE_VALIDATE_WARMUP", 0)?;

    let _baseline = run_candidate_dump(
        bench,
        metalpack,
        families,
        baseline_selection,
        tokens,
        validation_iterations,
        validation_warmup,
        layer_count,
        start_layer,
        sidecar_args,
        &baseline_dump,
    )?;
    let _candidate = run_candidate_dump(
        bench,
        metalpack,
        families,
        candidate_selection,
        tokens,
        validation_iterations,
        validation_warmup,
        layer_count,
        start_layer,
        sidecar_args,
        &candidate_dump,
    )?;

    let output = std::process::Command::new(compare)
        .arg(&baseline_dump)
        .arg(&candidate_dump)
        .arg(tokens.to_string())
        .arg("1024")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", compare.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            compare.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("compare output is not valid UTF-8: {err}"))?;
    let mismatch_count = parse_usize_metric(&stdout, "mismatch_count")?;
    let mean_abs_error = parse_metric(&stdout, "mean_abs_error")?;
    let rms_error = parse_metric(&stdout, "rms_error")?;
    let max_abs_error = parse_metric(&stdout, "max_abs_error")?;
    let checksum_delta = parse_metric(&stdout, "checksum_delta")?;

    let max_mean_abs = parse_env_f64("CTOX_QWEN35_AUTOTUNE_MAX_MEAN_ABS", 0.0005)?;
    let max_rms = parse_env_f64("CTOX_QWEN35_AUTOTUNE_MAX_RMS", 0.0010)?;
    let max_abs = parse_env_f64("CTOX_QWEN35_AUTOTUNE_MAX_ABS", 0.0100)?;
    let max_checksum_delta = parse_env_f64("CTOX_QWEN35_AUTOTUNE_MAX_CHECKSUM_DELTA", 1.0)?;
    let passed = mean_abs_error <= max_mean_abs
        && rms_error <= max_rms
        && max_abs_error <= max_abs
        && checksum_delta.abs() <= max_checksum_delta;
    let reason = if passed {
        "candidate within hidden-dump thresholds".to_owned()
    } else {
        format!(
            "thresholds mean_abs<={max_mean_abs:.6}, rms<={max_rms:.6}, max_abs<={max_abs:.6}, abs(checksum_delta)<={max_checksum_delta:.6}"
        )
    };

    Ok(ValidationResult {
        skipped: false,
        passed,
        baseline_dump: Some(baseline_dump),
        candidate_dump: Some(candidate_dump),
        mismatch_count: Some(mismatch_count),
        mean_abs_error: Some(mean_abs_error),
        rms_error: Some(rms_error),
        max_abs_error: Some(max_abs_error),
        checksum_delta: Some(checksum_delta),
        reason,
    })
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_candidate_dump(
    bench: &std::path::Path,
    metalpack: &std::path::Path,
    families: &[Family],
    selection: &[usize],
    tokens: usize,
    iterations: usize,
    warmup: usize,
    layer_count: usize,
    start_layer: usize,
    sidecar_args: &[std::path::PathBuf],
    dump_path: &std::path::Path,
) -> Result<BenchResult, String> {
    let mut cmd = std::process::Command::new(bench);
    cmd.arg(metalpack)
        .arg(start_layer.to_string())
        .arg(tokens.to_string())
        .arg(iterations.to_string())
        .arg(warmup.to_string())
        .arg(layer_count.to_string());
    for sidecar in sidecar_args {
        cmd.arg(sidecar);
    }

    for key in tuning_env_keys() {
        cmd.env_remove(key);
    }
    cmd.env("CTOX_QWEN35_PROJECT_SPLIT_NORM", "1");
    cmd.env("CTOX_QWEN35_DELTA_STACK_FINAL_DUMP", dump_path);
    for (key, value) in selection_env(families, selection) {
        cmd.env(key, value);
    }

    let output = cmd
        .output()
        .map_err(|err| format!("failed to run {}: {err}", bench.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed with status {}\nselection: {}\ndump: {}\nstdout:\n{}\nstderr:\n{}",
            bench.display(),
            output.status,
            selection_label(families, selection),
            dump_path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("benchmark output is not valid UTF-8: {err}"))?;
    Ok(BenchResult {
        median_s: parse_metric(&stdout, "median_s")?,
        p95_s: parse_metric(&stdout, "p95_s")?,
        effective_gb_s: parse_optional_metric_containing(&stdout, "effective_gb_s"),
        checksum: parse_optional_metric(&stdout, "checksum16"),
    })
}

#[cfg(target_os = "macos")]
fn selection_env(families: &[Family], selection: &[usize]) -> Vec<(&'static str, &'static str)> {
    families
        .iter()
        .zip(selection)
        .flat_map(|(family, idx)| family.candidates[*idx].envs.iter().copied())
        .collect()
}

#[cfg(target_os = "macos")]
fn baseline_env(families: &[Family], selection: &[usize]) -> Vec<(&'static str, &'static str)> {
    let mut env = vec![("CTOX_QWEN35_PROJECT_SPLIT_NORM", "1")];
    env.extend(selection_env(families, selection));
    env
}

#[cfg(target_os = "macos")]
fn selection_label(families: &[Family], selection: &[usize]) -> String {
    families
        .iter()
        .zip(selection)
        .map(|(family, idx)| format!("{}={}", family.name, family.candidates[*idx].name))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(target_os = "macos")]
fn tuning_env_keys() -> &'static [&'static str] {
    &[
        "CTOX_QWEN35_PROJECT_SPLIT_NORM",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA8",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA16",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_NO_MMA",
        "CTOX_QWEN35_DELTA_OUT_MMA16",
        "CTOX_QWEN35_DELTA_OUT_MMA32",
        "CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL",
        "CTOX_QWEN35_DELTA_OUT_MMA64",
        "CTOX_QWEN35_DELTA_OUT_TOK2",
        "CTOX_QWEN35_DELTA_OUT_TOK8",
        "CTOX_QWEN35_FFN_GATE_UP_MMA",
        "CTOX_QWEN35_FFN_GATE_UP_MMA16",
        "CTOX_QWEN35_FFN_GATE_UP_MMA32",
        "CTOX_QWEN35_FFN_GATE_UP_MMA64",
        "CTOX_QWEN35_FFN_GATE_UP_TOK2",
        "CTOX_QWEN35_FFN_GATE_UP_TOK8",
        "CTOX_QWEN35_DOWN_MMA",
        "CTOX_QWEN35_DOWN_MMA16",
        "CTOX_QWEN35_DOWN_MMA32",
        "CTOX_QWEN35_DOWN_MMA32_RESIDUAL",
        "CTOX_QWEN35_DOWN_MMA64",
        "CTOX_QWEN35_DOWN_MMA64_RESIDUAL",
        "CTOX_QWEN35_DOWN_TOK2",
        "CTOX_QWEN35_DOWN_TOK8",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE",
        "CTOX_QWEN35_DELTA_SCAN_LANES4",
        "CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK",
        "CTOX_QWEN35_DELTA_SCAN_LANES4_ORDERED",
        "CTOX_QWEN35_DELTA_SCAN_GATED_NORM",
        "CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64_MIN_TOKENS",
        "CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED",
        "CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED_TOK4",
        "CTOX_QWEN35_DELTA_STACK_PROFILE_STOP",
    ]
}

#[cfg(target_os = "macos")]
fn write_history_csv(
    path: &std::path::Path,
    history: &[EvalRecord],
    validation: &ValidationResult,
) -> Result<(), String> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    writeln!(
        file,
        "record_type,phase,family,candidate,selection,median_s,p95_s,effective_gb_s,model_bytes,tok_s,checksum,correctness_status,mismatch_count,mean_abs_error,rms_error,max_abs_error,checksum_delta,reason"
    )
    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    for record in history {
        writeln!(
            file,
            "eval,{},{},{},{},{:.9},{:.9},{},{},{:.6},{},{},{},{},{},{},{},{}",
            csv_escape(&record.phase),
            csv_escape(&record.family),
            csv_escape(&record.candidate),
            csv_escape(&record.selection),
            record.median_s,
            record.p95_s,
            opt_f64(record.effective_gb_s),
            opt_usize(record.model_bytes),
            record.tok_s,
            opt_f64(record.checksum),
            "",
            "",
            "",
            "",
            "",
            "",
            ""
        )
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    let correctness = if validation.skipped {
        "skipped"
    } else if validation.passed {
        "pass"
    } else {
        "fail"
    };
    writeln!(
        file,
        "validation,,,,,,,,,,,{},{},{},{},{},{},{}",
        correctness,
        opt_usize(validation.mismatch_count),
        opt_f64(validation.mean_abs_error),
        opt_f64(validation.rms_error),
        opt_f64(validation.max_abs_error),
        opt_f64(validation.checksum_delta),
        csv_escape(&validation.reason),
    )
    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(target_os = "macos")]
fn opt_f64(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.9}")).unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn opt_usize(value: Option<usize>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn parse_arg<T: std::str::FromStr>(
    args: &[std::ffi::OsString],
    idx: usize,
    default: T,
    label: &str,
) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    args.get(idx)
        .and_then(|arg| arg.to_str())
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|err| format!("invalid {label} argument `{value}`: {err}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[cfg(target_os = "macos")]
fn parse_env_usize(key: &str, default: usize) -> Result<usize, String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid {key} `{value}`: {err}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[cfg(target_os = "macos")]
fn parse_env_f64(key: &str, default: f64) -> Result<f64, String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|err| format!("invalid {key} `{value}`: {err}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[cfg(target_os = "macos")]
fn parse_metric(output: &str, key: &str) -> Result<f64, String> {
    output
        .lines()
        .find_map(|line| {
            let (lhs, rhs) = line.split_once(':')?;
            (lhs.trim() == key).then(|| rhs.trim().parse::<f64>())
        })
        .ok_or_else(|| format!("missing metric `{key}` in output:\n{output}"))?
        .map_err(|err| format!("failed to parse metric `{key}`: {err}"))
}

#[cfg(target_os = "macos")]
fn parse_usize_metric(output: &str, key: &str) -> Result<usize, String> {
    output
        .lines()
        .find_map(|line| {
            let (lhs, rhs) = line.split_once(':')?;
            (lhs.trim() == key).then(|| rhs.trim().parse::<usize>())
        })
        .ok_or_else(|| format!("missing metric `{key}` in output:\n{output}"))?
        .map_err(|err| format!("failed to parse metric `{key}`: {err}"))
}

#[cfg(target_os = "macos")]
fn parse_optional_metric(output: &str, key: &str) -> Option<f64> {
    output.lines().find_map(|line| {
        let (lhs, rhs) = line.split_once(':')?;
        (lhs.trim() == key)
            .then(|| rhs.trim().parse::<f64>().ok())
            .flatten()
    })
}

#[cfg(target_os = "macos")]
fn parse_optional_metric_containing(output: &str, needle: &str) -> Option<f64> {
    output.lines().find_map(|line| {
        let (lhs, rhs) = line.split_once(':')?;
        lhs.contains(needle)
            .then(|| rhs.trim().parse::<f64>().ok())
            .flatten()
    })
}
