#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("attention_variant_sweep is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() < 2 || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage();
        return Ok(());
    }
    let metalpack = args[1]
        .to_str()
        .ok_or_else(|| "metalpack path must be valid UTF-8".to_owned())?
        .to_owned();
    let layer = parse_arg(&args, 2, "layer")?.unwrap_or(3);
    let tokens_csv = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .unwrap_or("512,1024,2048");
    let tokens = parse_tokens(tokens_csv)?;
    let iterations = parse_arg(&args, 4, "iterations")?.unwrap_or(3);
    let project_mma = parse_arg(&args, 5, "project-mma")?.unwrap_or(1);
    let mps_attention_out_sidecar = args.get(6).and_then(|arg| arg.to_str());

    println!("qwen35-08b attention variant sweep");
    println!("metalpack: {metalpack}");
    println!("layer: {layer}");
    println!("tokens: {tokens_csv}");
    println!("iterations: {iterations}");
    println!("project_mma: {}", project_mma != 0);
    if let Some(sidecar) = mps_attention_out_sidecar {
        println!("mps_attention_out_sidecar: {sidecar}");
    }
    println!();
    println!(
        "{:<18} {:>8} {:>12} {:>12} {:>12} {:>12}",
        "variant", "tokens", "median_ms", "p95_ms", "GB/s", "checksum"
    );

    for token_count in tokens {
        for variant in variants() {
            let result = run_variant(
                &metalpack,
                layer,
                token_count,
                iterations,
                project_mma,
                &variant,
                mps_attention_out_sidecar,
            )?;
            println!(
                "{:<18} {:>8} {:>12.3} {:>12.3} {:>12.2} {:>12.6}",
                variant.name,
                token_count,
                result.median_s * 1_000.0,
                result.p95_s * 1_000.0,
                result.effective_gb_s,
                result.checksum
            );
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
struct Variant {
    name: &'static str,
    envs: &'static [(&'static str, &'static str)],
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
struct VariantResult {
    median_s: f64,
    p95_s: f64,
    effective_gb_s: f64,
    checksum: f64,
}

#[cfg(target_os = "macos")]
fn variants() -> Vec<Variant> {
    vec![
        Variant {
            name: "baseline",
            envs: &[("CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE", "1")],
        },
        Variant {
            name: "qblk2",
            envs: &[
                ("CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE", "1"),
                ("CTOX_QWEN35_ATTENTION_QBLK2", "1"),
            ],
        },
        Variant {
            name: "simdreduce",
            envs: &[],
        },
        Variant {
            name: "qblk2_simd",
            envs: &[("CTOX_QWEN35_ATTENTION_QBLK2_SIMDREDUCE", "1")],
        },
        Variant {
            name: "qblk4_simd",
            envs: &[("CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE", "1")],
        },
        Variant {
            name: "qblk4_batch",
            envs: &[("CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH", "1")],
        },
        Variant {
            name: "qh2_qblk4_batch",
            envs: &[("CTOX_QWEN35_ATTENTION_QH2_QBLK4_SIMDREDUCE_BATCH", "1")],
        },
        Variant {
            name: "qh4_qblk2_batch",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_QBLK2_SIMDREDUCE_BATCH", "1")],
        },
        Variant {
            name: "qh4_simd32_vec8",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8", "1")],
        },
        Variant {
            name: "qh4_vec8_kvpack",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INTERLEAVED_KV", "1")],
        },
        Variant {
            name: "qh4_vec8_i8kv",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_KV", "1")],
        },
        Variant {
            name: "qh4_vec8_i8v",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V", "1")],
        },
        Variant {
            name: "qh4_vec8_i8v4",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V_PACK4", "1")],
        },
        Variant {
            name: "qh4_vec8_hacc",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFACC", "1")],
        },
        Variant {
            name: "qh4_vec8_hdot",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT", "1")],
        },
        Variant {
            name: "qh4_splitk64",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SPLITK64", "1")],
        },
        Variant {
            name: "qh4_splitk128",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SPLITK128", "1")],
        },
        Variant {
            name: "qh4_splitk256",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SPLITK256", "1")],
        },
        Variant {
            name: "qh4_splitk512",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SPLITK512", "1")],
        },
        Variant {
            name: "qh4_qblk2_vec8",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_QBLK2_SIMD32_VEC8", "1")],
        },
        Variant {
            name: "qh4_vec8_win2k",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW", "2048")],
        },
        Variant {
            name: "qh4_vec8_win4k",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW", "4096")],
        },
        Variant {
            name: "qh4_hdot_win4k",
            envs: &[(
                "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT",
                "4096",
            )],
        },
        Variant {
            name: "qh4_vec8_win8k",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW", "8192")],
        },
        Variant {
            name: "qh4_hdot_win8k",
            envs: &[(
                "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT",
                "8192",
            )],
        },
        Variant {
            name: "qh4_vec8_win16k",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW", "16384")],
        },
        Variant {
            name: "qh4_hdot_win16k",
            envs: &[(
                "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT",
                "16384",
            )],
        },
        Variant {
            name: "qh4_qblk1_batch",
            envs: &[("CTOX_QWEN35_ATTENTION_QH4_QBLK1_SIMDREDUCE_BATCH", "1")],
        },
        Variant {
            name: "qblk8_batch",
            envs: &[("CTOX_QWEN35_ATTENTION_QBLK8_SIMDREDUCE_BATCH", "1")],
        },
        Variant {
            name: "qblk4",
            envs: &[
                ("CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE", "1"),
                ("CTOX_QWEN35_ATTENTION_QBLK4", "1"),
            ],
        },
        Variant {
            name: "qblk2x512",
            envs: &[
                ("CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE", "1"),
                ("CTOX_QWEN35_ATTENTION_QBLK2X512", "1"),
            ],
        },
        Variant {
            name: "partial_qblk2",
            envs: &[
                ("CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE", "1"),
                ("CTOX_QWEN35_ATTENTION_PARTIAL_QBLK2", "1"),
            ],
        },
    ]
}

#[cfg(target_os = "macos")]
const ATTENTION_VARIANT_ENVS: &[&str] = &[
    "CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE",
    "CTOX_QWEN35_ATTENTION_QBLK2",
    "CTOX_QWEN35_ATTENTION_QBLK4",
    "CTOX_QWEN35_ATTENTION_QBLK2_SIMDREDUCE",
    "CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE",
    "CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH",
    "CTOX_QWEN35_ATTENTION_QBLK8_SIMDREDUCE_BATCH",
    "CTOX_QWEN35_ATTENTION_QH2_QBLK4_SIMDREDUCE_BATCH",
    "CTOX_QWEN35_ATTENTION_QH4_QBLK2_SIMDREDUCE_BATCH",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INTERLEAVED_KV",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_KV",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V_PACK4",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFACC",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT",
    "CTOX_QWEN35_ATTENTION_QH4_SPLITK64",
    "CTOX_QWEN35_ATTENTION_QH4_SPLITK128",
    "CTOX_QWEN35_ATTENTION_QH4_SPLITK256",
    "CTOX_QWEN35_ATTENTION_QH4_SPLITK512",
    "CTOX_QWEN35_ATTENTION_QH4_QBLK2_SIMD32_VEC8",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT",
    "CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WIN4096",
    "CTOX_QWEN35_ATTENTION_QH4_QBLK1_SIMDREDUCE_BATCH",
    "CTOX_QWEN35_ATTENTION_QBLK2X512",
    "CTOX_QWEN35_ATTENTION_PARTIAL_QBLK2",
];

#[cfg(target_os = "macos")]
fn run_variant(
    metalpack: &str,
    layer: usize,
    tokens: usize,
    iterations: usize,
    project_mma: usize,
    variant: &Variant,
    mps_attention_out_sidecar: Option<&str>,
) -> Result<VariantResult, String> {
    use std::process::Command;

    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let dir = exe
        .parent()
        .ok_or_else(|| format!("cannot resolve parent dir for {}", exe.display()))?;
    let bench = dir.join("bench_metalpack_prefill_attention_core");
    if !bench.exists() {
        return Err(format!(
            "missing benchmark binary `{}`; run `cargo build --release --bins` first",
            bench.display()
        ));
    }

    let mut command = Command::new(&bench);
    command.args([
        metalpack,
        &layer.to_string(),
        &tokens.to_string(),
        &iterations.to_string(),
        &project_mma.to_string(),
    ]);
    if let Some(sidecar) = mps_attention_out_sidecar {
        command.arg(sidecar);
    }
    for key in ATTENTION_VARIANT_ENVS {
        command.env_remove(key);
    }
    for (key, value) in variant.envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to run {}: {err}", bench.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed for variant {}\nstdout:\n{}\nstderr:\n{}",
            bench.display(),
            variant.name,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(VariantResult {
        median_s: parse_metric(&stdout, "median_s")?,
        p95_s: parse_metric(&stdout, "p95_s")?,
        effective_gb_s: parse_metric(&stdout, "effective_gb_s_attention_core_estimate")?,
        checksum: parse_metric(&stdout, "checksum16")?,
    })
}

#[cfg(target_os = "macos")]
fn parse_metric(output: &str, key: &str) -> Result<f64, String> {
    output
        .lines()
        .find_map(|line| {
            if !line.contains(key) {
                return None;
            }
            line.split_once(':')
                .and_then(|(_, rhs)| rhs.trim().parse::<f64>().ok())
        })
        .ok_or_else(|| format!("missing metric `{key}`"))
}

#[cfg(target_os = "macos")]
fn parse_tokens(csv: &str) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for item in csv.split(',') {
        let token = item
            .trim()
            .parse::<usize>()
            .map_err(|err| format!("invalid token count `{item}`: {err}"))?;
        if token == 0 {
            return Err("token counts must be > 0".to_owned());
        }
        out.push(token);
    }
    if out.is_empty() {
        return Err("at least one token count is required".to_owned());
    }
    Ok(out)
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
fn print_usage() {
    println!(
        "usage: attention_variant_sweep <metalpack-dir> [layer=3] [tokens=512,1024,2048] [iterations=3] [project-mma 0|1] [mps-attention-out-sidecar-dir]"
    );
}
