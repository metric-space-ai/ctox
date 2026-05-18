#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_static_int8_matmul is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use half::f16;

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::ffi::Device;
    use std::time::Instant;

    let args = std::env::args().collect::<Vec<_>>();
    let tokens = parse_arg(&args, 1, 512usize, "tokens")?;
    let rows = parse_arg(&args, 2, 3584usize, "rows")?;
    let iterations = parse_arg(&args, 3, 10usize, "iterations")?;
    let warmup = parse_arg(&args, 4, 3usize, "warmup")?;
    let quant_group_size = parse_arg(&args, 5, 256usize, "quant_group_size")?;
    let row_tile = parse_arg(&args, 6, 8usize, "row_tile")?;
    let col_tile = parse_arg(&args, 7, 256usize, "col_tile")?;
    let kernel = KernelVariant::parse(args.get(8).map(String::as_str).unwrap_or("scalar"))?;

    let cols = 1024usize;
    if row_tile == 0 || row_tile > 16 {
        return Err(format!("row_tile must be in 1..=16, got {row_tile}"));
    }
    if quant_group_size == 0 || quant_group_size > col_tile || col_tile % quant_group_size != 0 {
        return Err(format!(
            "quant_group_size must be a nonzero divisor of col_tile={col_tile}, got {quant_group_size}"
        ));
    }
    let n_col_tiles = cols.div_ceil(col_tile);
    let x_host = fill_half(tokens * cols, 31, 17, 257.0);
    let w_host = fill_f32(rows * cols, 13, 29, 127.0);
    let w_quant = pack_int8_row_tiled(&w_host, rows, cols, row_tile, col_tile, quant_group_size);

    let dev = Device::default_system()?;
    let x = dev.new_buffer(x_host.len() * std::mem::size_of::<u16>())?;
    let w = dev.new_buffer(w_quant.len())?;
    let y = dev.new_buffer(tokens * rows * std::mem::size_of::<f32>())?;
    unsafe {
        x.write(0, &x_host);
        w.write(0, &w_quant);
    }

    let tokens_u32 = u32::try_from(tokens).map_err(|_| "tokens exceed u32")?;
    let rows_u32 = u32::try_from(rows).map_err(|_| "rows exceed u32")?;
    let row_tile_u32 = u32::try_from(row_tile).map_err(|_| "row_tile exceed u32")?;
    let col_tile_u32 = u32::try_from(col_tile).map_err(|_| "col_tile exceed u32")?;
    let quant_group_size_u32 =
        u32::try_from(quant_group_size).map_err(|_| "quant_group_size exceed u32")?;
    let n_col_tiles_u32 = u32::try_from(n_col_tiles).map_err(|_| "n_col_tiles exceed u32")?;

    for _ in 0..warmup {
        dispatch(
            &dev,
            &x,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            quant_group_size_u32,
            n_col_tiles_u32,
            kernel,
        )?;
    }

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        dispatch(
            &dev,
            &x,
            &w,
            &y,
            tokens_u32,
            rows_u32,
            row_tile_u32,
            col_tile_u32,
            quant_group_size_u32,
            n_col_tiles_u32,
            kernel,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; rows.min(16)];
    unsafe {
        y.read(0, &mut first);
    }
    let checksum = first.iter().sum::<f32>();
    let dense_weight_bytes = rows * cols * std::mem::size_of::<u16>();
    let quant_weight_bytes = w_quant.len();
    let output_bytes = tokens * rows * std::mem::size_of::<f32>();
    let input_bytes = tokens * cols * std::mem::size_of::<u16>();
    let visible_bytes = input_bytes + quant_weight_bytes + output_bytes;

    println!("static_int8_matmul_probe");
    println!("shape: tokens={tokens} rows={rows} cols={cols}");
    println!("kernel: {}", kernel.as_str());
    println!(
        "layout: int8_row_tiled row_tile={row_tile} col_tile={col_tile} quant_group_size={quant_group_size}"
    );
    println!("iterations: {iterations}");
    println!("warmup: {warmup}");
    println!("median_s: {median_s:.9}");
    println!("p95_s: {p95_s:.9}");
    println!("dense_fp16_weight_bytes: {dense_weight_bytes}");
    println!("quant_weight_bytes: {quant_weight_bytes}");
    println!(
        "weight_compression_ratio: {:.4}",
        dense_weight_bytes as f64 / quant_weight_bytes.max(1) as f64
    );
    println!(
        "effective_visible_gb_s: {:.3}",
        visible_bytes as f64 / median_s.max(1e-12) / 1e9
    );
    println!("checksum16: {checksum:.6}");
    Ok(())
}

#[cfg(target_os = "macos")]
fn dispatch(
    dev: &ctox_qwen35_08b_metal_probe::metal::ffi::Device,
    x: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    w: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    y: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    tokens: u32,
    rows: u32,
    row_tile: u32,
    col_tile: u32,
    quant_group_size: u32,
    n_col_tiles: u32,
    kernel: KernelVariant,
) -> Result<(), String> {
    let pso = dev.pipeline(kernel.pipeline_name())?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, x, 0);
    enc.set_buffer(1, w, 0);
    enc.set_buffer(2, y, 0);
    enc.set_bytes(3, &tokens);
    enc.set_bytes(4, &rows);
    enc.set_bytes(5, &row_tile);
    enc.set_bytes(6, &col_tile);
    enc.set_bytes(7, &quant_group_size);
    enc.set_bytes(8, &n_col_tiles);
    enc.dispatch_threadgroups(
        (
            (rows as usize).div_ceil(row_tile as usize),
            tokens as usize,
            1,
        ),
        (kernel.threads_per_threadgroup(row_tile), 1, 1),
    );
    enc.end();
    cmd.commit_and_wait()
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum KernelVariant {
    Scalar,
    Simd32,
}

#[cfg(target_os = "macos")]
impl KernelVariant {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "scalar" => Ok(Self::Scalar),
            "simd32" => Ok(Self::Simd32),
            other => Err(format!(
                "unsupported kernel variant `{other}`; expected scalar or simd32"
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Simd32 => "simd32",
        }
    }

    const fn pipeline_name(self) -> &'static str {
        match self {
            Self::Scalar => "qwen35_08b_prefill_matmul_int8_row_tiled_k1024_f32",
            Self::Simd32 => "qwen35_08b_prefill_matmul_int8_row_tiled_simd32_k1024_f32",
        }
    }

    fn threads_per_threadgroup(self, row_tile: u32) -> usize {
        match self {
            Self::Scalar => 256,
            Self::Simd32 => row_tile as usize * 32,
        }
    }
}

#[cfg(target_os = "macos")]
fn pack_int8_row_tiled(
    weights: &[f32],
    rows: usize,
    cols: usize,
    row_tile: usize,
    col_tile: usize,
    quant_group_size: usize,
) -> Vec<u8> {
    let padded_rows = rows.div_ceil(row_tile) * row_tile;
    let padded_cols = cols.div_ceil(col_tile) * col_tile;
    let groups_per_col_tile = col_tile / quant_group_size;
    let row_payload = groups_per_col_tile * (2 + quant_group_size);
    let mut out = Vec::with_capacity(
        (padded_rows / row_tile) * (padded_cols / col_tile) * row_tile * row_payload,
    );
    for row_base in (0..padded_rows).step_by(row_tile) {
        for col_base in (0..padded_cols).step_by(col_tile) {
            for row in 0..row_tile {
                for group_id in 0..groups_per_col_tile {
                    let group_col_base = col_base + group_id * quant_group_size;
                    let mut max_abs = 0.0f32;
                    for local_col in 0..quant_group_size {
                        let src_row = row_base + row;
                        let src_col = group_col_base + local_col;
                        let value = if src_row < rows && src_col < cols {
                            weights[src_row * cols + src_col]
                        } else {
                            0.0
                        };
                        max_abs = max_abs.max(value.abs());
                    }
                    let scale = if max_abs > 0.0 { max_abs / 127.0 } else { 1.0 };
                    out.extend_from_slice(&f16::from_f32(scale).to_bits().to_le_bytes());
                    for local_col in 0..quant_group_size {
                        let src_row = row_base + row;
                        let src_col = group_col_base + local_col;
                        let value = if src_row < rows && src_col < cols {
                            weights[src_row * cols + src_col]
                        } else {
                            0.0
                        };
                        let q = (value / scale).round().clamp(-127.0, 127.0) as i8;
                        out.push(q.to_ne_bytes()[0]);
                    }
                }
            }
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn fill_half(len: usize, a: usize, b: usize, denom: f32) -> Vec<u16> {
    (0..len)
        .map(|idx| {
            let value = (((idx * a + b) % 251) as f32 - 125.0) / denom;
            f16::from_f32(value).to_bits()
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn fill_f32(len: usize, a: usize, b: usize, denom: f32) -> Vec<f32> {
    (0..len)
        .map(|idx| (((idx * a + b) % 253) as f32 - 126.0) / denom)
        .collect()
}

#[cfg(target_os = "macos")]
fn parse_arg(args: &[String], idx: usize, fallback: usize, name: &str) -> Result<usize, String> {
    args.get(idx)
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid {name} `{value}`: {err}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(fallback))
}

#[cfg(target_os = "macos")]
fn percentile_sorted(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    values[idx.min(values.len() - 1)]
}
