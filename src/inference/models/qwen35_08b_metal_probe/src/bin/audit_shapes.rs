use std::{env, path::PathBuf, process};

use ctox_qwen35_08b_metal_probe::{
    audit_shape_contract, open_metalpack, ShapeAuditStatus, QWEN35_08B,
};

fn main() {
    let args = env::args_os().collect::<Vec<_>>();
    let pack = if let Some(root) = args.get(1) {
        let root = PathBuf::from(root);
        match open_metalpack(&root) {
            Ok(pack) => Some(pack),
            Err(error) => {
                eprintln!("failed to open metalpack {}: {error}", root.display());
                process::exit(1);
            }
        }
    } else {
        None
    };

    let rows = audit_shape_contract(pack.as_ref());
    let supported = rows
        .iter()
        .filter(|row| row.status == ShapeAuditStatus::Supported)
        .count();
    let placeholders = rows
        .iter()
        .filter(|row| row.status == ShapeAuditStatus::KernelPlaceholder)
        .count();
    let missing = rows
        .iter()
        .filter(|row| row.status == ShapeAuditStatus::Missing)
        .count();
    let unsupported = rows
        .iter()
        .filter(|row| row.status == ShapeAuditStatus::Unsupported)
        .count();

    println!("qwen35-08b shape audit");
    println!("model: {}", QWEN35_08B.model);
    println!(
        "widths: attn_q={} attn_q_plus_head_gate={} attn_kv={} deltanet={}",
        QWEN35_08B.attention_q_width(),
        QWEN35_08B.attention_q_with_head_gate_width(),
        QWEN35_08B.attention_kv_width(),
        QWEN35_08B.deltanet_width()
    );
    if let Some(pack) = &pack {
        println!("metalpack: {}", pack.root.display());
        println!("entries: {}", pack.entries.len());
    } else {
        println!("metalpack: none");
    }
    println!(
        "summary: supported={} placeholder={} missing={} unsupported={}",
        supported, placeholders, missing, unsupported
    );

    for row in rows
        .iter()
        .filter(|row| row.status != ShapeAuditStatus::Supported)
    {
        let layer = row
            .layer
            .map(|layer| layer.to_string())
            .unwrap_or_else(|| "-".to_owned());
        println!(
            "status={:?} layer={} class={} expected={} kernel={} actual={} note={}",
            row.status,
            layer,
            row.class.as_str(),
            fmt_shapes(&row.expected_shapes),
            fmt_shapes(&row.kernel_shapes),
            row.actual_shape
                .as_ref()
                .map(|shape| fmt_shape(shape))
                .unwrap_or_else(|| "-".to_owned()),
            row.note
        );
    }
}

fn fmt_shapes(shapes: &[Vec<usize>]) -> String {
    shapes
        .iter()
        .map(|shape| fmt_shape(shape))
        .collect::<Vec<_>>()
        .join("|")
}

fn fmt_shape(shape: &[usize]) -> String {
    format!(
        "[{}]",
        shape
            .iter()
            .map(|dim| dim.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}
