fn main() {
    let tokens = parse_arg(1, 17usize);
    let dim = parse_arg(2, 8usize);
    let chunk = parse_arg(3, 4usize);

    let inputs = Inputs::new(tokens, dim);
    let serial = run_serial(&inputs);
    let chunked = run_chunk_affine(&inputs, chunk);
    let composed = run_local_zero_composed(&inputs, chunk);

    let mut max_state = 0.0f32;
    let mut max_out = 0.0f32;
    for (a, b) in serial.state.iter().zip(&chunked.state) {
        max_state = max_state.max((a - b).abs());
    }
    for (a, b) in serial.out.iter().zip(&chunked.out) {
        max_out = max_out.max((a - b).abs());
    }
    let mut max_composed_state = 0.0f32;
    let mut max_composed_out = 0.0f32;
    for (a, b) in serial.state.iter().zip(&composed.state) {
        max_composed_state = max_composed_state.max((a - b).abs());
    }
    for (a, b) in serial.out.iter().zip(&composed.out) {
        max_composed_out = max_composed_out.max((a - b).abs());
    }

    println!("deltanet_chunk_scan_math");
    println!("tokens: {tokens}");
    println!("dim: {dim}");
    println!("chunk: {chunk}");
    println!("max_state_abs_error: {max_state:.9}");
    println!("max_out_abs_error: {max_out:.9}");
    println!("max_composed_state_abs_error: {max_composed_state:.9}");
    println!("max_composed_out_abs_error: {max_composed_out:.9}");
    println!("serial_checksum: {:.9}", checksum(&serial.out));
    println!("chunked_checksum: {:.9}", checksum(&chunked.out));
    println!("composed_checksum: {:.9}", checksum(&composed.out));

    let tolerance = 2.0e-5f32;
    if max_state > tolerance
        || max_out > tolerance
        || max_composed_state > tolerance
        || max_composed_out > tolerance
    {
        eprintln!(
            "chunk scan mismatch: affine_state={max_state:.9} affine_out={max_out:.9} composed_state={max_composed_state:.9} composed_out={max_composed_out:.9} tol={tolerance}"
        );
        std::process::exit(1);
    }
}

fn parse_arg<T: std::str::FromStr>(idx: usize, default: T) -> T {
    std::env::args()
        .nth(idx)
        .and_then(|value| value.parse::<T>().ok())
        .unwrap_or(default)
}

#[derive(Clone)]
struct Inputs {
    tokens: usize,
    dim: usize,
    q: Vec<f32>,
    k: Vec<f32>,
    v: Vec<f32>,
    beta: Vec<f32>,
    decay: Vec<f32>,
    state0: Vec<f32>,
}

impl Inputs {
    fn new(tokens: usize, dim: usize) -> Self {
        let mut rng = Lcg::new(0x5eed_f00d_dead_beef);
        let q = fill_signed(&mut rng, tokens * dim, 0.25);
        let k = fill_signed(&mut rng, tokens * dim, 0.20);
        let v = fill_signed(&mut rng, tokens * dim, 0.30);
        let beta = (0..tokens).map(|_| 0.15 + rng.next_unit() * 0.70).collect();
        let decay = (0..tokens).map(|_| 0.55 + rng.next_unit() * 0.35).collect();
        let state0 = fill_signed(&mut rng, dim * dim, 0.05);
        Self {
            tokens,
            dim,
            q,
            k,
            v,
            beta,
            decay,
            state0,
        }
    }
}

fn fill_signed(rng: &mut Lcg, len: usize, scale: f32) -> Vec<f32> {
    (0..len).map(|_| rng.next_signed() * scale).collect()
}

struct Run {
    state: Vec<f32>,
    out: Vec<f32>,
}

fn run_serial(input: &Inputs) -> Run {
    let mut state = input.state0.clone();
    let mut out = vec![0.0f32; input.tokens * input.dim];
    let dim = input.dim;

    for t in 0..input.tokens {
        let q_t = &input.q[t * dim..(t + 1) * dim];
        let k_t = &input.k[t * dim..(t + 1) * dim];
        let v_t = &input.v[t * dim..(t + 1) * dim];
        let beta = input.beta[t];
        let decay = input.decay[t];

        for row in 0..dim {
            let row_base = row * dim;
            let mut kv_mem = 0.0f32;
            for col in 0..dim {
                kv_mem += state[row_base + col] * decay * k_t[col];
            }
            let delta = (v_t[row] - kv_mem) * beta;

            let mut acc = 0.0f32;
            for col in 0..dim {
                let next_state = state[row_base + col] * decay + k_t[col] * delta;
                state[row_base + col] = next_state;
                acc += next_state * q_t[col];
            }
            out[t * dim + row] = acc;
        }
    }

    Run { state, out }
}

fn run_chunk_affine(input: &Inputs, chunk: usize) -> Run {
    let mut state = input.state0.clone();
    let mut out = vec![0.0f32; input.tokens * input.dim];
    let dim = input.dim;

    for chunk_start in (0..input.tokens).step_by(chunk.max(1)) {
        let chunk_end = (chunk_start + chunk.max(1)).min(input.tokens);
        let mut prefix = identity(dim);
        let mut offsets = vec![vec![0.0f32; dim * dim]; chunk_end - chunk_start];
        let mut prefixes = Vec::with_capacity(chunk_end - chunk_start);

        for (local_t, t) in (chunk_start..chunk_end).enumerate() {
            let k_t = &input.k[t * dim..(t + 1) * dim];
            let beta = input.beta[t];
            let decay = input.decay[t];
            let token_a = token_matrix(k_t, beta, decay, dim);
            let new_prefix = matmul(&prefix, &token_a, dim);
            let transform_v = &input.v[t * dim..(t + 1) * dim];
            for row in 0..dim {
                let row_base = row * dim;
                for col in 0..dim {
                    offsets[local_t][row_base + col] = transform_v[row] * beta * k_t[col];
                }
                for prev in 0..local_t {
                    let transformed =
                        rowvec_mul(&offsets[prev][row_base..row_base + dim], &token_a, dim);
                    offsets[prev][row_base..row_base + dim].copy_from_slice(&transformed);
                }
            }
            prefixes.push(new_prefix.clone());

            let q_t = &input.q[t * dim..(t + 1) * dim];
            for row in 0..dim {
                let row_base = row * dim;
                let mut s_row = rowvec_mul(&state[row_base..row_base + dim], &new_prefix, dim);
                for item in offsets.iter().take(local_t + 1) {
                    for col in 0..dim {
                        s_row[col] += item[row_base + col];
                    }
                }
                out[t * dim + row] = dot(&s_row, q_t);
            }

            prefix = new_prefix;
        }

        let mut next_state = vec![0.0f32; dim * dim];
        for row in 0..dim {
            let row_base = row * dim;
            let mut s_row = rowvec_mul(&state[row_base..row_base + dim], &prefix, dim);
            for item in offsets.iter() {
                for col in 0..dim {
                    s_row[col] += item[row_base + col];
                }
            }
            next_state[row_base..row_base + dim].copy_from_slice(&s_row);
        }
        state = next_state;
    }

    Run { state, out }
}

fn run_local_zero_composed(input: &Inputs, chunk: usize) -> Run {
    let mut state = input.state0.clone();
    let mut out = vec![0.0f32; input.tokens * input.dim];
    let dim = input.dim;

    for chunk_start in (0..input.tokens).step_by(chunk.max(1)) {
        let chunk_end = (chunk_start + chunk.max(1)).min(input.tokens);
        let local_len = chunk_end - chunk_start;
        let mut prefix = identity(dim);
        let mut prefixes = Vec::with_capacity(local_len);

        for t in chunk_start..chunk_end {
            let k_t = &input.k[t * dim..(t + 1) * dim];
            let token_a = token_matrix(k_t, input.beta[t], input.decay[t], dim);
            prefix = matmul(&prefix, &token_a, dim);
            prefixes.push(prefix.clone());
        }

        let mut local_state = vec![0.0f32; dim * dim];
        let mut local_out = vec![0.0f32; local_len * dim];
        for (local_t, t) in (chunk_start..chunk_end).enumerate() {
            let q_t = &input.q[t * dim..(t + 1) * dim];
            let k_t = &input.k[t * dim..(t + 1) * dim];
            let v_t = &input.v[t * dim..(t + 1) * dim];
            let beta = input.beta[t];
            let decay = input.decay[t];

            for row in 0..dim {
                let row_base = row * dim;
                let mut kv_mem = 0.0f32;
                for col in 0..dim {
                    kv_mem += local_state[row_base + col] * decay * k_t[col];
                }
                let delta = (v_t[row] - kv_mem) * beta;

                let mut acc = 0.0f32;
                for col in 0..dim {
                    let next_state = local_state[row_base + col] * decay + k_t[col] * delta;
                    local_state[row_base + col] = next_state;
                    acc += next_state * q_t[col];
                }
                local_out[local_t * dim + row] = acc;
            }
        }

        for (local_t, t) in (chunk_start..chunk_end).enumerate() {
            let q_t = &input.q[t * dim..(t + 1) * dim];
            let prefix_t = &prefixes[local_t];
            for row in 0..dim {
                let row_base = row * dim;
                let from_initial = rowvec_mul(&state[row_base..row_base + dim], prefix_t, dim);
                out[t * dim + row] = local_out[local_t * dim + row] + dot(&from_initial, q_t);
            }
        }

        let mut next_state = vec![0.0f32; dim * dim];
        for row in 0..dim {
            let row_base = row * dim;
            let from_initial = rowvec_mul(&state[row_base..row_base + dim], &prefix, dim);
            for col in 0..dim {
                next_state[row_base + col] = local_state[row_base + col] + from_initial[col];
            }
        }
        state = next_state;
    }

    Run { state, out }
}

fn token_matrix(k: &[f32], beta: f32, decay: f32, dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim * dim];
    for r in 0..dim {
        for c in 0..dim {
            let ident = if r == c { 1.0 } else { 0.0 };
            out[r * dim + c] = decay * (ident - beta * k[r] * k[c]);
        }
    }
    out
}

fn identity(dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim * dim];
    for i in 0..dim {
        out[i * dim + i] = 1.0;
    }
    out
}

fn matmul(a: &[f32], b: &[f32], dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim * dim];
    for r in 0..dim {
        for c in 0..dim {
            let mut sum = 0.0f32;
            for k in 0..dim {
                sum += a[r * dim + k] * b[k * dim + c];
            }
            out[r * dim + c] = sum;
        }
    }
    out
}

fn rowvec_mul(x: &[f32], a: &[f32], dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim];
    for c in 0..dim {
        let mut sum = 0.0f32;
        for r in 0..dim {
            sum += x[r] * a[r * dim + c];
        }
        out[c] = sum;
    }
    out
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn checksum(values: &[f32]) -> f32 {
    values
        .iter()
        .enumerate()
        .map(|(idx, value)| *value * ((idx % 17) as f32 + 1.0))
        .sum()
}

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        let bits = (self.0 >> 40) as u32;
        bits as f32 / ((1u32 << 24) as f32)
    }

    fn next_signed(&mut self) -> f32 {
        self.next_unit() * 2.0 - 1.0
    }
}
