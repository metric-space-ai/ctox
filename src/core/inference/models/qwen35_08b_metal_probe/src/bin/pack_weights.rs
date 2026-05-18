use std::{env, path::PathBuf, process};

use ctox_qwen35_08b_metal_probe::{
    inspect_model_artifacts, write_metalpack_from_report, QWEN35_08B,
};

fn main() {
    let mut args = env::args_os().skip(1).collect::<Vec<_>>();
    let allow_incomplete =
        if args.first().and_then(|arg| arg.to_str()) == Some("--allow-incomplete") {
            args.remove(0);
            true
        } else {
            false
        };

    if args.len() != 2 {
        eprintln!(
            "usage: pack_weights [--allow-incomplete] <local-hf-model-dir> <output.metalpack-dir>"
        );
        process::exit(2);
    }

    let model_dir = PathBuf::from(&args[0]);
    let output_dir = PathBuf::from(&args[1]);
    let report = match inspect_model_artifacts(&model_dir) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("artifact inspection failed: {error}");
            process::exit(1);
        }
    };

    let blocking = report.blocking_warnings();
    if (!report.is_shape_compatible() || !blocking.is_empty()) && !allow_incomplete {
        eprintln!("refusing to pack incomplete or shape-incompatible artifacts");
        eprintln!("target_model: {}", QWEN35_08B.model);
        for mismatch in &report.config.mismatches {
            eprintln!("config_mismatch: {mismatch}");
        }
        for warning in blocking.iter().take(40) {
            eprintln!("blocking: {}", warning.message);
        }
        if blocking.len() > 40 {
            eprintln!("... {} more blocking warnings omitted", blocking.len() - 40);
        }
        process::exit(3);
    }

    match write_metalpack_from_report(&report, &output_dir) {
        Ok(written) => {
            println!("qwen35-08b metalpack written");
            println!("output_dir: {}", written.output_dir.display());
            println!("manifest: {}", written.manifest_path.display());
            println!("weights: {}", written.weights_path.display());
            println!("entries: {}", written.entries);
            println!(
                "packed_bytes: {} ({:.2} GiB)",
                written.packed_bytes,
                written.packed_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );
        }
        Err(error) => {
            eprintln!("metalpack write failed: {error}");
            process::exit(1);
        }
    }
}
