use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use ctox_qwen35_08b_metal_probe::{
    open_metalpack, MetalPack, MetalPackEntry, PackLayout, TensorClass, QWEN35_08B,
};
use serde_json::json;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(
            "usage: pack_mps_ffn_sidecar <source.metalpack-dir> <output.mps-ffn-dir>".to_owned(),
        );
    }
    let source = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    fs::create_dir_all(&output).map_err(|err| format!("{}: {err}", output.display()))?;

    let pack = open_metalpack(&source).map_err(|err| err.to_string())?;
    let weights_path = output.join("weights.bin");
    let mut weights =
        File::create(&weights_path).map_err(|err| format!("{}: {err}", weights_path.display()))?;
    let mut manifest_entries = Vec::with_capacity(QWEN35_08B.n_layers);
    let mut offset = 0u64;

    for layer in 0..QWEN35_08B.n_layers {
        let gate = find_layer_class(&pack, layer, TensorClass::MlpGate)?;
        let up = find_layer_class(&pack, layer, TensorClass::MlpUp)?;
        let down = find_layer_class(&pack, layer, TensorClass::MlpDown)?;
        validate_projection(
            "gate",
            gate,
            &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
        )?;
        validate_projection(
            "up",
            up,
            &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
        )?;
        validate_projection(
            "down",
            down,
            &[QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate],
        )?;

        let gate_bytes = pack
            .read_entry_bytes(gate)
            .map_err(|err| format!("{}: {err}", gate.tensor))?;
        let up_bytes = pack
            .read_entry_bytes(up)
            .map_err(|err| format!("{}: {err}", up.tensor))?;
        let down_bytes = pack
            .read_entry_bytes(down)
            .map_err(|err| format!("{}: {err}", down.tensor))?;

        let gate_up_offset = offset;
        let gate_up_written = write_gate_up_mps(&mut weights, gate, &gate_bytes, up, &up_bytes)?;
        offset += gate_up_written;

        let down_offset = offset;
        let down_written = write_down_mps(&mut weights, down, &down_bytes)?;
        offset += down_written;

        manifest_entries.push(json!({
            "layer": layer,
            "gate_tensor": gate.tensor,
            "up_tensor": up.tensor,
            "down_tensor": down.tensor,
            "gate_up": {
                "layout": "mps_fp16_row_major",
                "shape": [QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate * 2],
                "row_bytes": QWEN35_08B.ffn_intermediate * 2 * 2,
                "offset": gate_up_offset,
                "bytes": gate_up_written,
            },
            "down": {
                "layout": "mps_fp16_row_major",
                "shape": [QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
                "row_bytes": QWEN35_08B.hidden_size * 2,
                "offset": down_offset,
                "bytes": down_written,
            }
        }));
    }
    weights
        .flush()
        .map_err(|err| format!("{}: {err}", weights_path.display()))?;

    let manifest_path = output.join("manifest.json");
    let manifest = json!({
        "format": "ctox.qwen35_08b.mps_ffn_sidecar",
        "version": 1,
        "source_metalpack": source,
        "weights_file": "weights.bin",
        "packed_bytes": offset,
        "model": QWEN35_08B.model,
        "shape": {
            "hidden_size": QWEN35_08B.hidden_size,
            "ffn_intermediate": QWEN35_08B.ffn_intermediate,
            "layers": QWEN35_08B.n_layers,
        },
        "entries": manifest_entries,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|err| err.to_string())?;
    fs::write(&manifest_path, manifest_bytes)
        .map_err(|err| format!("{}: {err}", manifest_path.display()))?;

    println!("qwen35-08b MPS FFN sidecar written");
    println!("source_metalpack: {}", source.display());
    println!("output_dir: {}", output.display());
    println!("manifest: {}", manifest_path.display());
    println!("weights: {}", weights_path.display());
    println!("layers: {}", QWEN35_08B.n_layers);
    println!("packed_bytes: {offset}");
    println!(
        "packed_gib: {:.3}",
        offset as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    Ok(())
}

fn find_layer_class(
    pack: &MetalPack,
    layer: usize,
    class: TensorClass,
) -> Result<&MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| entry.layer == Some(layer) && entry.class == class)
        .ok_or_else(|| format!("missing layer {layer} {}", class.as_str()))
}

fn validate_projection(label: &str, entry: &MetalPackEntry, shape: &[usize]) -> Result<(), String> {
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != shape {
        return Err(format!(
            "{label}: expected shape {:?}, got {:?}",
            shape, entry.source_shape
        ));
    }
    Ok(())
}

fn write_gate_up_mps(
    output: &mut File,
    gate: &MetalPackEntry,
    gate_bytes: &[u8],
    up: &MetalPackEntry,
    up_bytes: &[u8],
) -> Result<u64, String> {
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    let row_stride = intermediate * 2;
    let mut dst = vec![0u16; hidden * row_stride];
    copy_row_tiled_transposed(gate, gate_bytes, &mut dst, row_stride, 0)?;
    copy_row_tiled_transposed(up, up_bytes, &mut dst, row_stride, intermediate)?;
    write_u16_slice(output, &dst)
}

fn write_down_mps(
    output: &mut File,
    down: &MetalPackEntry,
    down_bytes: &[u8],
) -> Result<u64, String> {
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    let mut dst = vec![0u16; intermediate * hidden];
    copy_row_tiled_transposed(down, down_bytes, &mut dst, hidden, 0)?;
    write_u16_slice(output, &dst)
}

fn copy_row_tiled_transposed(
    entry: &MetalPackEntry,
    bytes: &[u8],
    dst: &mut [u16],
    dst_row_stride: usize,
    dst_col_offset: usize,
) -> Result<(), String> {
    let rows = entry.source_shape[0];
    let cols = entry.source_shape[1];
    let row_tile = entry.row_tile.max(1);
    let col_tile = entry.col_tile.max(1);
    let padded_rows = round_up(rows, row_tile);
    let padded_cols = round_up(cols, col_tile);
    let mut element_index = 0usize;

    for row_base in (0..padded_rows).step_by(row_tile) {
        for col_base in (0..padded_cols).step_by(col_tile) {
            for local_row in 0..row_tile {
                let src_row = row_base + local_row;
                for local_col in 0..col_tile {
                    let src_col = col_base + local_col;
                    if src_row < rows && src_col < cols {
                        let byte_index = element_index * 2;
                        let value = u16::from_le_bytes([bytes[byte_index], bytes[byte_index + 1]]);
                        let dst_index = src_col * dst_row_stride + dst_col_offset + src_row;
                        let Some(slot) = dst.get_mut(dst_index) else {
                            return Err(format!(
                                "{} transposed dst index {dst_index} out of bounds {}",
                                entry.tensor,
                                dst.len()
                            ));
                        };
                        *slot = value;
                    }
                    element_index += 1;
                }
            }
        }
    }
    Ok(())
}

fn write_u16_slice(output: &mut File, values: &[u16]) -> Result<u64, String> {
    let mut bytes = Vec::with_capacity(values.len() * 2);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    output.write_all(&bytes).map_err(|err| err.to_string())?;
    Ok(bytes.len() as u64)
}

const fn round_up(value: usize, multiple: usize) -> usize {
    if multiple == 0 {
        value
    } else {
        value.div_ceil(multiple) * multiple
    }
}
