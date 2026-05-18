#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("sweep_metalpack_prefill_delta_autotune is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use std::{env, io::Write, path::PathBuf, process::Command};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(());
    }

    let metalpack = PathBuf::from(&args[1]);
    let tokens = parse_tokens_arg(&args, 2, "512,4096,16384")?;
    let iterations = parse_arg(&args, 3, 3usize, "iterations")?;
    let warmup = parse_arg(&args, 4, 1usize, "warmup")?;
    let layer_count = parse_arg(&args, 5, 18usize, "delta-layer-count")?;
    let start_layer = parse_arg(&args, 6, 0usize, "start-layer")?;
    let passes = parse_arg(&args, 7, 2usize, "passes")?;

    let exe = env::current_exe().map_err(|err| format!("failed to resolve current exe: {err}"))?;
    let bin_dir = exe
        .parent()
        .ok_or_else(|| format!("failed to resolve binary directory for {}", exe.display()))?;
    let autotune = bin_dir.join("autotune_metalpack_prefill_delta_stack");
    if !autotune.exists() {
        return Err(format!(
            "missing autotuner `{}`; build `autotune_metalpack_prefill_delta_stack` first",
            autotune.display()
        ));
    }

    let sweep_dir = env::var_os("CTOX_QWEN35_AUTOTUNE_SWEEP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::temp_dir().join(format!(
                "ctox_qwen35_delta_autotune_sweep_{}",
                std::process::id()
            ))
        });
    std::fs::create_dir_all(&sweep_dir)
        .map_err(|err| format!("failed to create {}: {err}", sweep_dir.display()))?;
    let summary_csv = sweep_dir.join("summary.csv");
    let mut summary = std::fs::File::create(&summary_csv)
        .map_err(|err| format!("failed to create {}: {err}", summary_csv.display()))?;
    writeln!(
        summary,
        "tokens,best_selection,accepted_selection,best_median_s,best_p95_s,best_tok_s,correctness_status,history_csv"
    )
    .map_err(|err| format!("failed to write {}: {err}", summary_csv.display()))?;

    println!("qwen35-08b DeltaNet+FFN autotune sweep");
    println!("metalpack: {}", metalpack.display());
    println!("tokens: {}", join_tokens(&tokens));
    println!(
        "shape: delta_layers={layer_count} start_layer={start_layer} iterations={iterations} warmup={warmup} passes={passes}"
    );
    println!("method: serial token sweep; each row runs the full autotuner with its own correctness gate");
    println!("sweep_dir: {}", sweep_dir.display());
    println!();
    println!(
        "{:>8} {:>12} {:>12} {:>12} {:<8} {}",
        "tokens", "median_s", "p95_s", "tok_s", "gate", "accepted_selection"
    );

    for token_count in tokens {
        let history_csv = sweep_dir.join(format!("tokens_{token_count}.csv"));
        let output = Command::new(&autotune)
            .arg(&metalpack)
            .arg(token_count.to_string())
            .arg(iterations.to_string())
            .arg(warmup.to_string())
            .arg(layer_count.to_string())
            .arg(start_layer.to_string())
            .arg(passes.to_string())
            .env("CTOX_QWEN35_AUTOTUNE_CSV", &history_csv)
            .output()
            .map_err(|err| format!("failed to run {}: {err}", autotune.display()))?;
        if !output.status.success() {
            return Err(format!(
                "{} failed for tokens={token_count} with status {}\nstdout:\n{}\nstderr:\n{}",
                autotune.display(),
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|err| format!("autotuner output is not valid UTF-8: {err}"))?;
        let best_selection = parse_string_metric(&stdout, "best_selection")?;
        let accepted_selection = parse_string_metric(&stdout, "accepted_selection")?;
        let best_median_s = parse_f64_metric(&stdout, "best_median_s")?;
        let best_p95_s = parse_f64_metric(&stdout, "best_p95_s")?;
        let best_tok_s = parse_f64_metric(&stdout, "best_tok_s_prefill_delta_stack_only")?;
        let correctness = parse_correctness_status(&stdout)?;
        println!(
            "{token_count:>8} {best_median_s:>12.6} {best_p95_s:>12.6} {best_tok_s:>12.2} {:<8} {}",
            correctness, accepted_selection
        );
        writeln!(
            summary,
            "{},{},{},{:.9},{:.9},{:.6},{},{}",
            token_count,
            csv_escape(&best_selection),
            csv_escape(&accepted_selection),
            best_median_s,
            best_p95_s,
            best_tok_s,
            correctness,
            history_csv.display()
        )
        .map_err(|err| format!("failed to write {}: {err}", summary_csv.display()))?;
    }
    println!();
    println!("summary_csv: {}", summary_csv.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn parse_tokens_arg(
    args: &[std::ffi::OsString],
    idx: usize,
    default: &str,
) -> Result<Vec<usize>, String> {
    let raw = args
        .get(idx)
        .and_then(|arg| arg.to_str())
        .unwrap_or(default);
    let mut out = Vec::new();
    for item in raw.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let value = item
            .parse::<usize>()
            .map_err(|err| format!("invalid tokens item `{item}`: {err}"))?;
        if value == 0 {
            return Err("tokens entries must be > 0".to_owned());
        }
        out.push(value);
    }
    if out.is_empty() {
        return Err("tokens list must not be empty".to_owned());
    }
    Ok(out)
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
fn parse_string_metric(output: &str, key: &str) -> Result<String, String> {
    output
        .lines()
        .find_map(|line| {
            let (lhs, rhs) = line.split_once(':')?;
            (lhs.trim() == key).then(|| rhs.trim().to_owned())
        })
        .ok_or_else(|| format!("missing metric `{key}` in output"))
}

#[cfg(target_os = "macos")]
fn parse_f64_metric(output: &str, key: &str) -> Result<f64, String> {
    parse_string_metric(output, key)?
        .parse::<f64>()
        .map_err(|err| format!("invalid metric `{key}`: {err}"))
}

#[cfg(target_os = "macos")]
fn parse_correctness_status(output: &str) -> Result<&'static str, String> {
    let line = output
        .lines()
        .find(|line| line.starts_with("correctness_gate:"))
        .ok_or_else(|| "missing correctness_gate line".to_owned())?;
    if line.contains("PASS") {
        Ok("pass")
    } else if line.contains("FAIL") {
        Ok("fail")
    } else if line.contains("SKIPPED") {
        Ok("skipped")
    } else {
        Err(format!("unknown correctness_gate line: {line}"))
    }
}

#[cfg(target_os = "macos")]
fn join_tokens(tokens: &[usize]) -> String {
    tokens
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
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
fn print_usage() {
    println!(
        "usage: sweep_metalpack_prefill_delta_autotune <metalpack-dir> [tokens-csv] [iterations] [warmup] [delta-layer-count] [start-layer] [passes]"
    );
}
