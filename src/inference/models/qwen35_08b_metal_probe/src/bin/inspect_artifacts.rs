use std::{env, path::PathBuf, process};

use ctox_qwen35_08b_metal_probe::{inspect_model_artifacts, TensorClass, QWEN35_08B};

fn main() {
    let root = match env::args_os().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: inspect_artifacts <local-hf-model-dir>");
            process::exit(2);
        }
    };

    let report = match inspect_model_artifacts(&root) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("artifact inspection failed: {error}");
            process::exit(1);
        }
    };

    println!("qwen35-08b artifact inspection");
    println!("root: {}", report.root.display());
    println!("target_model: {}", QWEN35_08B.model);
    println!("config: {}", report.config.path.display());
    if let Some(model_type) = &report.config.model_type {
        println!("model_type: {model_type}");
    }
    if !report.config.architectures.is_empty() {
        println!("architectures: {}", report.config.architectures.join(", "));
    }
    println!(
        "shape_compatible: {}",
        if report.is_shape_compatible() {
            "yes"
        } else {
            "no"
        }
    );
    for mismatch in &report.config.mismatches {
        println!("config_mismatch: {mismatch}");
    }

    println!("safetensor_shards: {}", report.safetensors.shards.len());
    println!("tensors: {}", report.safetensors.tensors.len());
    println!(
        "tensor_bytes: {} ({:.2} GiB)",
        report.safetensors.total_tensor_bytes,
        report.safetensors.total_tensor_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    if let Some(index) = &report.safetensors.index_path {
        println!("index: {}", index.display());
    }

    println!("pack_plan_classes:");
    for summary in &report.pack_plan.class_summary {
        println!(
            "  {:<18} count={:<5} bytes={}",
            summary.class.as_str(),
            summary.count,
            summary.bytes
        );
    }

    let blocking = report.blocking_warnings();
    println!("pack_plan_warnings: {}", report.pack_plan.warnings.len());
    println!("pack_plan_blocking_warnings: {}", blocking.len());
    for warning in report.pack_plan.warnings.iter().take(40) {
        let level = if warning.blocking { "blocking" } else { "note" };
        println!("  {level}: {}", warning.message);
    }
    if report.pack_plan.warnings.len() > 40 {
        println!(
            "  ... {} more warnings omitted",
            report.pack_plan.warnings.len() - 40
        );
    }

    let sample = report
        .pack_plan
        .entries
        .iter()
        .filter(|entry| entry.class != TensorClass::Other)
        .take(12);
    println!("pack_plan_sample:");
    for entry in sample {
        println!(
            "  layer={:<4} class={:<18} layout={:<28} quant={:<26} group={:<4} shape={:?} tensor={}",
            entry
                .layer
                .map(|layer| layer.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            entry.class.as_str(),
            entry.layout.as_str(),
            entry.quant_scheme.as_str(),
            entry.quant_group_size,
            entry.shape,
            entry.tensor
        );
    }

    if !report.is_shape_compatible() || !blocking.is_empty() {
        process::exit(3);
    }
}
