use ctox_qwen35_08b_metal_probe::{
    format_bytes, qwen35_cache_analysis, CacheModelConfig, CacheResidency, CounterPriority,
};

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let mut cfg = CacheModelConfig::default();
    let mut csv = false;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--tokens" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--tokens requires a value".to_string())?;
                cfg.tokens = value
                    .parse::<usize>()
                    .map_err(|err| format!("invalid --tokens `{value}`: {err}"))?;
            }
            "--decode-position" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--decode-position requires a value".to_string())?;
                cfg.decode_position = value
                    .parse::<usize>()
                    .map_err(|err| format!("invalid --decode-position `{value}`: {err}"))?;
            }
            "--modeled-l2-mib" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--modeled-l2-mib requires a value".to_string())?;
                let mib = value
                    .parse::<usize>()
                    .map_err(|err| format!("invalid --modeled-l2-mib `{value}`: {err}"))?;
                cfg.modeled_l2_bytes = mib * 1024 * 1024;
            }
            "--sustained-gb-s" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--sustained-gb-s requires a value".to_string())?;
                let gb_s = value
                    .parse::<f64>()
                    .map_err(|err| format!("invalid --sustained-gb-s `{value}`: {err}"))?;
                cfg.sustained_bandwidth_bytes_s = gb_s * 1.0e9;
            }
            "--csv" => csv = true,
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
        i += 1;
    }

    let ops = qwen35_cache_analysis(cfg);
    if csv {
        println!("op,kernel_family,layers,token_tile,working_set_bytes,logical_bytes,modeled_unavoidable_dram_miss_bytes,avoidable_miss_budget_bytes,modeled_cache_hit_bytes,modeled_hit_rate,modeled_time_ms,residency,dominant,optimization");
        for op in ops {
            println!(
                "{},{},{},{},{},{},{},{},{},{:.6},{:.6},{:?},{},{}",
                op.op,
                op.kernel_family,
                op.layers_per_model,
                op.token_tile,
                op.working_set_bytes,
                op.logical_bytes,
                op.modeled_dram_miss_bytes,
                0,
                op.modeled_cache_hit_bytes,
                op.modeled_hit_rate,
                op.modeled_time_ms(cfg),
                op.residency,
                op.dominant,
                op.optimization.replace(',', ";"),
            );
        }
        return Ok(());
    }

    println!("qwen35-08b cache / miss analysis model");
    println!("tokens: {}", cfg.tokens);
    println!("decode_position: {}", cfg.decode_position);
    println!("modeled_l2: {}", format_bytes(cfg.modeled_l2_bytes));
    println!(
        "sustained_bandwidth: {:.2} GB/s",
        cfg.sustained_bandwidth_bytes_s / 1.0e9
    );
    println!();
    println!(
        "{:<28} {:>7} {:>12} {:>12} {:>9} {:>9} {:<11} {}",
        "op", "tile", "workset", "unavoid", "hit%", "ms", "residency", "next action"
    );
    for op in &ops {
        let residency = match op.residency {
            CacheResidency::FitsModeledL2 => "fit-model",
            CacheResidency::StreamsBeyondModeledL2 => "stream",
        };
        println!(
            "{:<28} {:>7} {:>12} {:>12} {:>8.1}% {:>9.3} {:<11} {}",
            op.op,
            op.token_tile,
            format_bytes(op.working_set_bytes),
            format_bytes(op.modeled_dram_miss_bytes),
            op.modeled_hit_rate * 100.0,
            op.modeled_time_ms(cfg),
            residency,
            op.optimization
        );
    }

    println!();
    println!("required counter checks:");
    for op in ops
        .iter()
        .filter(|op| op.residency == CacheResidency::StreamsBeyondModeledL2)
        .take(8)
    {
        let required = op
            .counters
            .iter()
            .filter(|counter| counter.priority == CounterPriority::Required)
            .map(|counter| counter.question)
            .collect::<Vec<_>>()
            .join("; ");
        println!("- {}: {}", op.op, required);
    }
    println!();
    println!(
        "Interpretation: modeled misses are the unavoidable compulsory/streaming floor. The avoidable miss budget is zero: a kernel is actionable when trace misses exceed this model or when a fit-model working set still streams from DRAM."
    );
    Ok(())
}

fn print_usage() {
    println!(
        "usage: cache_analysis [--tokens N] [--decode-position N] [--modeled-l2-mib N] [--sustained-gb-s N] [--csv]"
    );
}
