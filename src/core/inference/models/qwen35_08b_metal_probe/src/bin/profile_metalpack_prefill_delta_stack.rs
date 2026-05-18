#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("profile_metalpack_prefill_delta_stack is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: profile_metalpack_prefill_delta_stack <metalpack-dir> [tokens] [iterations] [warmup] [delta-layer-count] [start-layer] [mps-ffn-sidecar-dir] [mps-delta-project-sidecar-dir] [mps-delta-out-sidecar-dir]".to_owned());
    }

    let metalpack = PathBuf::from(&args[1]);
    let tokens = parse_arg(&args, 2, 4096usize, "tokens")?;
    let iterations = parse_arg(&args, 3, 5usize, "iterations")?;
    let warmup = parse_arg(&args, 4, 1usize, "warmup")?;
    let layer_count = parse_arg(&args, 5, 1usize, "delta-layer-count")?;
    let start_layer = parse_arg(&args, 6, 0usize, "start-layer")?;
    let mps_ffn_sidecar = args.get(7).map(PathBuf::from);
    let mps_delta_project_sidecar = args.get(8).map(PathBuf::from);
    let mps_delta_out_sidecar = args.get(9).map(PathBuf::from);

    let exe = env::current_exe().map_err(|err| format!("failed to resolve current exe: {err}"))?;
    let bin_dir = exe
        .parent()
        .ok_or_else(|| format!("failed to resolve binary directory for {}", exe.display()))?;
    let bench = bin_dir.join("bench_metalpack_prefill_delta3_ffn_superblock");

    let modes = [
        ("project", "project"),
        ("conv_split", "conv/split+ba"),
        ("scan_norm", "scan+norm"),
        ("delta_out", "delta out"),
        ("ffn_gate_up", "ffn norm+gate/up"),
        ("full", "ffn down"),
    ];

    let mut rows = Vec::with_capacity(modes.len());
    for (mode, label) in modes {
        let output = run_profile_mode(
            &bench,
            &metalpack,
            mode,
            tokens,
            iterations,
            warmup,
            layer_count,
            start_layer,
            mps_ffn_sidecar.as_deref(),
            mps_delta_project_sidecar.as_deref(),
            mps_delta_out_sidecar.as_deref(),
        )?;
        let median_s = parse_metric(&output, "median_s")?;
        let p95_s = parse_metric(&output, "p95_s")?;
        rows.push(ProfileRow {
            label,
            median_s,
            p95_s,
        });
    }

    println!("qwen35-08b DeltaNet+FFN superblock prefix profiler");
    println!("metalpack: {}", metalpack.display());
    println!(
        "shape: tokens={tokens} delta_layers={layer_count} start_layer={start_layer} iterations={iterations} warmup={warmup}"
    );
    if let Some(sidecar) = &mps_ffn_sidecar {
        println!("mps_ffn_sidecar: {}", sidecar.display());
    }
    if let Some(sidecar) = &mps_delta_project_sidecar {
        println!("mps_delta_project_sidecar: {}", sidecar.display());
    }
    if let Some(sidecar) = &mps_delta_out_sidecar {
        println!("mps_delta_out_sidecar: {}", sidecar.display());
    }
    println!(
        "profile defaults: accepted profile env when sourced; current baseline is QKV/Z128, DeltaOut64 residual, GateUp64, Down64 residual, rowcache_block32 scan, fused conv/split"
    );
    println!(
        "{:<18} {:>12} {:>12} {:>12} {:>8}",
        "phase", "cum_ms", "delta_ms", "p95_ms", "share"
    );

    let total_s = rows
        .last()
        .map(|row| row.median_s)
        .ok_or_else(|| "missing profile rows".to_owned())?;
    let mut prev_s = 0.0f64;
    for row in &rows {
        let delta_s = (row.median_s - prev_s).max(0.0);
        let share = if total_s > 0.0 {
            delta_s / total_s
        } else {
            0.0
        };
        println!(
            "{:<18} {:>12.3} {:>12.3} {:>12.3} {:>7.1}%",
            row.label,
            row.median_s * 1_000.0,
            delta_s * 1_000.0,
            row.p95_s * 1_000.0,
            share * 100.0
        );
        prev_s = row.median_s;
    }

    println!("full_median_s: {:.9}", total_s);
    println!(
        "full_tok_s_prefill_delta_stack_only: {:.2}",
        tokens as f64 / total_s
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct ProfileRow {
    label: &'static str,
    median_s: f64,
    p95_s: f64,
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
#[allow(clippy::too_many_arguments)]
fn run_profile_mode(
    bench: &std::path::Path,
    metalpack: &std::path::Path,
    mode: &'static str,
    tokens: usize,
    iterations: usize,
    warmup: usize,
    layer_count: usize,
    start_layer: usize,
    mps_ffn_sidecar: Option<&std::path::Path>,
    mps_delta_project_sidecar: Option<&std::path::Path>,
    mps_delta_out_sidecar: Option<&std::path::Path>,
) -> Result<String, String> {
    let mut cmd = std::process::Command::new(bench);
    cmd.arg(metalpack)
        .arg(start_layer.to_string())
        .arg(tokens.to_string())
        .arg(iterations.to_string())
        .arg(warmup.to_string())
        .arg(layer_count.to_string());
    if let Some(sidecar) = mps_ffn_sidecar {
        cmd.arg(sidecar);
    }
    if let Some(sidecar) = mps_delta_project_sidecar {
        cmd.arg(sidecar);
    }
    if let Some(sidecar) = mps_delta_out_sidecar {
        cmd.arg(sidecar);
    }

    for (key, value) in accepted_profile_env(mode) {
        if key == "CTOX_QWEN35_DELTA_STACK_PROFILE_STOP" || std::env::var_os(key).is_none() {
            cmd.env(key, value);
        }
    }

    let output = cmd
        .output()
        .map_err(|err| format!("failed to run {}: {err}", bench.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed for mode {mode} with status {}\nstdout:\n{}\nstderr:\n{}",
            bench.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|err| format!("benchmark output is not valid UTF-8: {err}"))
}

#[cfg(target_os = "macos")]
fn accepted_profile_env(mode: &'static str) -> Vec<(&'static str, &'static str)> {
    let mut envs = vec![("CTOX_QWEN35_PROJECT_SPLIT_NORM", "1")];
    if no_env(&[
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA8",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA16",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED",
        "CTOX_QWEN35_DELTA_PROJECT_QKVZ_NO_MMA",
    ]) {
        envs.push(("CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128", "1"));
    }
    if no_env(&[
        "CTOX_QWEN35_DELTA_OUT_MMA16",
        "CTOX_QWEN35_DELTA_OUT_MMA32",
        "CTOX_QWEN35_DELTA_OUT_MMA64",
        "CTOX_QWEN35_DELTA_OUT_TOK2",
        "CTOX_QWEN35_DELTA_OUT_TOK8",
    ]) {
        envs.push(("CTOX_QWEN35_DELTA_OUT_MMA64", "1"));
    }
    if no_env(&[
        "CTOX_QWEN35_FFN_GATE_UP_MMA",
        "CTOX_QWEN35_FFN_GATE_UP_MMA16",
        "CTOX_QWEN35_FFN_GATE_UP_MMA32",
        "CTOX_QWEN35_FFN_GATE_UP_MMA64",
        "CTOX_QWEN35_FFN_GATE_UP_TOK2",
        "CTOX_QWEN35_FFN_GATE_UP_TOK8",
    ]) {
        envs.push(("CTOX_QWEN35_FFN_GATE_UP_MMA64", "1"));
    }
    if no_env(&[
        "CTOX_QWEN35_DOWN_MMA",
        "CTOX_QWEN35_DOWN_MMA16",
        "CTOX_QWEN35_DOWN_MMA32",
        "CTOX_QWEN35_DOWN_MMA64",
        "CTOX_QWEN35_DOWN_TOK2",
        "CTOX_QWEN35_DOWN_TOK8",
    ]) {
        envs.push(("CTOX_QWEN35_DOWN_MMA64", "1"));
        envs.push(("CTOX_QWEN35_DOWN_MMA64_RESIDUAL", "1"));
    }
    if no_env(&[
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32",
        "CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64",
        "CTOX_QWEN35_DELTA_SCAN_LANES4",
        "CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK",
        "CTOX_QWEN35_DELTA_SCAN_GATED_NORM",
    ]) {
        envs.push(("CTOX_QWEN35_DELTA_SCAN_ROWCACHE", "1"));
        envs.push(("CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32", "1"));
    }
    if no_env(&[
        "CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED",
        "CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED_TOK4",
    ]) {
        envs.push(("CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED", "1"));
    }
    envs.push(("CTOX_QWEN35_DELTA_STACK_PROFILE_STOP", mode));
    envs
}

#[cfg(target_os = "macos")]
fn no_env(keys: &[&str]) -> bool {
    keys.iter().all(|key| std::env::var_os(key).is_none())
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
