use half::f16;

fn main() -> Result<(), String> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() < 5 {
        return Err(
            "usage: compare_half_dump <baseline.bin> <candidate.bin> <tokens> <width>".to_owned(),
        );
    }

    let baseline_path = &args[1];
    let candidate_path = &args[2];
    let tokens = parse_arg(&args, 3, "tokens")?;
    let width = parse_arg(&args, 4, "width")?;
    if tokens == 0 || width == 0 {
        return Err("tokens and width must be > 0".to_owned());
    }

    let baseline = std::fs::read(baseline_path)
        .map_err(|err| format!("failed to read baseline dump {:?}: {err}", baseline_path))?;
    let candidate = std::fs::read(candidate_path)
        .map_err(|err| format!("failed to read candidate dump {:?}: {err}", candidate_path))?;
    let expected_bytes = tokens * width * std::mem::size_of::<u16>();
    if baseline.len() != expected_bytes || candidate.len() != expected_bytes {
        return Err(format!(
            "expected {expected_bytes} bytes, got baseline={} candidate={}",
            baseline.len(),
            candidate.len()
        ));
    }

    let elems = tokens * width;
    let mut first_mismatch = None;
    let mut mismatch_count = 0usize;
    let mut max_abs_error = 0.0f32;
    let mut max_abs_index = 0usize;
    let mut sum_abs_error = 0.0f64;
    let mut sum_sq_error = 0.0f64;
    let mut baseline_checksum = 0.0f64;
    let mut candidate_checksum = 0.0f64;

    for idx in 0..elems {
        let offset = idx * 2;
        let a_bits = u16::from_le_bytes([baseline[offset], baseline[offset + 1]]);
        let b_bits = u16::from_le_bytes([candidate[offset], candidate[offset + 1]]);
        let a = f16::from_bits(a_bits).to_f32();
        let b = f16::from_bits(b_bits).to_f32();
        baseline_checksum += a as f64;
        candidate_checksum += b as f64;

        let abs = (a - b).abs();
        sum_abs_error += abs as f64;
        sum_sq_error += (abs as f64) * (abs as f64);
        if a_bits != b_bits {
            mismatch_count += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some((idx, a_bits, b_bits, a, b));
            }
        }
        if abs > max_abs_error {
            max_abs_error = abs;
            max_abs_index = idx;
        }
    }

    println!("qwen35-08b half dump compare");
    println!("tokens: {tokens}");
    println!("width: {width}");
    println!("elements: {elems}");
    println!("mismatch_count: {mismatch_count}");
    println!("mean_abs_error: {:.9}", sum_abs_error / elems as f64);
    println!("rms_error: {:.9}", (sum_sq_error / elems as f64).sqrt());
    println!("max_abs_error: {:.9}", max_abs_error);
    print_index("max_abs", max_abs_index, width);
    println!("baseline_checksum: {:.9}", baseline_checksum);
    println!("candidate_checksum: {:.9}", candidate_checksum);
    println!(
        "checksum_delta: {:.9}",
        candidate_checksum - baseline_checksum
    );
    if let Some((idx, a_bits, b_bits, a, b)) = first_mismatch {
        print_index("first_mismatch", idx, width);
        println!("first_mismatch_baseline_bits: 0x{a_bits:04x}");
        println!("first_mismatch_candidate_bits: 0x{b_bits:04x}");
        println!("first_mismatch_baseline_f32: {:.9}", a);
        println!("first_mismatch_candidate_f32: {:.9}", b);
    }
    Ok(())
}

fn parse_arg(args: &[std::ffi::OsString], idx: usize, name: &str) -> Result<usize, String> {
    args.get(idx)
        .and_then(|arg| arg.to_str())
        .ok_or_else(|| format!("missing {name} argument"))?
        .parse::<usize>()
        .map_err(|err| format!("invalid {name} argument: {err}"))
}

fn print_index(label: &str, idx: usize, width: usize) {
    println!("{label}_index: {idx}");
    println!("{label}_token: {}", idx / width);
    println!("{label}_col: {}", idx % width);
}
