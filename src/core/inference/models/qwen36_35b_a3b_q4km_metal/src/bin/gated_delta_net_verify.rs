// Origin: CTOX
// License: AGPL-3.0-only
//
// Stage-4 unblocker: verifier for the Qwen3.6 linear-attention
// "dflash" block. Drives `kernel_gated_delta_net_f32_4` against an
// in-Rust recurrent-scan reference at a small synthetic shape, plus a
// shape-bench at Qwen3.6's actual decode parameters
// (S_v=128, num_v_heads=32, num_k_heads=16, n_tokens=1, batch=1).

#![cfg(feature = "metal")]

use std::time::Instant;

use anyhow::{bail, Result};

use ctox_qwen36_35b_a3b_q4km_metal::metal_port::{
    ops::gated_delta_net::{
        cpu_reference_gated_delta_net_g1, dispatch_gated_delta_net, GatedDeltaNetKernel,
        GatedDeltaNetNsg, GdnDecodeShape,
    },
    runtime::MetalRuntime,
};

fn xs(s: &mut u32) -> u32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    *s
}
fn synth(elems: usize, seed: u32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(0x9E37_79B1).wrapping_add(0xDEAD_BEEF);
    (0..elems)
        .map(|_| ((xs(&mut s) as f32) / (u32::MAX as f32)) * 2.0 - 1.0)
        .collect()
}

fn run_correctness(
    rt: &MetalRuntime,
    label: &str,
    s_v: u32,
    num_q_heads: u32,
    num_k_heads: u32,
    num_v_heads: u32,
    n_tokens: u32,
    batch: u32,
) -> Result<f64> {
    // The kernel requires S_v / NSG == simd_width (= 32 on Apple GPUs)
    // because `simd_sum` aggregates across the whole simdgroup and that
    // simdgroup must equal one row of the recurrent state. Pick the
    // matching NSG variant; fail loudly if no valid combination exists.
    let nsg = match s_v {
        32 => GatedDeltaNetNsg::N1,
        64 => GatedDeltaNetNsg::N2,
        128 => GatedDeltaNetNsg::N4,
        other => bail!(
            "S_v={other} not supported by the vendored gated_delta_net \
             kernel; must be one of {{32, 64, 128}} so S_v/NSG = simd_width"
        ),
    };
    let kernel = GatedDeltaNetKernel::new(rt, nsg, s_v, /*g=*/ 1)?;

    let q_size = (batch * num_q_heads * n_tokens * s_v) as usize;
    let k_size = (batch * num_k_heads * n_tokens * s_v) as usize;
    let v_size = (batch * num_v_heads * n_tokens * s_v) as usize;
    let g_size = (batch * num_v_heads * n_tokens * 1) as usize;
    let b_size = (batch * num_v_heads * n_tokens) as usize;
    let s_size = (batch * num_v_heads * s_v * s_v) as usize;

    let q = synth(q_size, 0xC0FE_BABE);
    let k = synth(k_size, 0xFEED_FACE);
    let v = synth(v_size, 0xCAFEBABE);
    // Use small `g` so exp(g) doesn't blow up:
    let g_raw = synth(g_size, 0xBE11C0DE);
    let g: Vec<f32> = g_raw.iter().map(|x| x * 0.05).collect();
    let b = synth(b_size, 0xFADE_BABE);
    let s_init = synth(s_size, 0xDEAD_C0DE);

    let shape = GdnDecodeShape {
        num_q_heads,
        num_k_heads,
        num_v_heads,
        n_tokens,
        batch,
    };

    let gpu = dispatch_gated_delta_net(rt, &kernel, &q, &k, &v, &g, &b, &s_init, &shape)?;
    let cpu = cpu_reference_gated_delta_net_g1(
        &q,
        &k,
        &v,
        &g,
        &b,
        &s_init,
        s_v as usize,
        num_q_heads as usize,
        num_k_heads as usize,
        num_v_heads as usize,
        n_tokens as usize,
        batch as usize,
    );

    let attn_elems = (batch * num_v_heads * n_tokens * s_v) as usize;
    let mut max_attn = 0.0_f64;
    let mut max_state = 0.0_f64;
    for (i, (g, c)) in gpu.iter().zip(cpu.iter()).enumerate() {
        let d = (*g as f64 - *c as f64).abs();
        if i < attn_elems {
            if d > max_attn {
                max_attn = d;
            }
        } else if d > max_state {
            max_state = d;
        }
    }
    println!(
        "  correctness {label:<24} S_v={s_v} q_h={num_q_heads} k_h={num_k_heads} v_h={num_v_heads} n_t={n_tokens} b={batch}  max_attn={max_attn:.3e}  max_state={max_state:.3e}"
    );
    if max_attn > 1e-3 || max_state > 1e-3 {
        bail!("correctness drift exceeded 1e-3 (attn={max_attn:.3e} state={max_state:.3e})");
    }
    Ok(max_attn.max(max_state))
}

fn run_qwen36_decode_bench(rt: &MetalRuntime) -> Result<()> {
    // Qwen3.6 linear-attention shape (per the frozen ABI):
    //   linear_value_head_dim = 128
    //   linear_num_value_heads = 32
    //   linear_num_key_heads = 16
    //   linear_key_head_dim = 128
    //   n_tokens = 1 (decode), batch = 1
    let s_v: u32 = 128;
    let num_q_heads: u32 = 16;
    let num_k_heads: u32 = 16;
    let num_v_heads: u32 = 32;
    let n_tokens: u32 = 1;
    let batch: u32 = 1;

    let kernel = GatedDeltaNetKernel::new(rt, GatedDeltaNetNsg::N4, s_v, /*g=*/ 1)?;

    let q = synth((batch * num_q_heads * n_tokens * s_v) as usize, 0xCAFE);
    let k = synth((batch * num_k_heads * n_tokens * s_v) as usize, 0xBEEF);
    let v = synth((batch * num_v_heads * n_tokens * s_v) as usize, 0xFEED);
    let g_raw = synth((batch * num_v_heads * n_tokens) as usize, 0xBABE);
    let g: Vec<f32> = g_raw.iter().map(|x| x * 0.05).collect();
    let b = synth((batch * num_v_heads * n_tokens) as usize, 0xDEAD);
    let s_init = synth((batch * num_v_heads * s_v * s_v) as usize, 0xC0FE);

    let shape = GdnDecodeShape {
        num_q_heads,
        num_k_heads,
        num_v_heads,
        n_tokens,
        batch,
    };

    // warm-up
    for _ in 0..5 {
        let _ = dispatch_gated_delta_net(rt, &kernel, &q, &k, &v, &g, &b, &s_init, &shape)?;
    }
    let mut samples = Vec::with_capacity(20);
    for _ in 0..20 {
        let t = Instant::now();
        let _ = dispatch_gated_delta_net(rt, &kernel, &q, &k, &v, &g, &b, &s_init, &shape)?;
        samples.push(t.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = samples[0];
    let med = samples[10];
    println!(
        "  bench Qwen3.6 linear-attention layer (decode) min={:>7.1} µs  med={:>7.1} µs",
        min * 1e6,
        med * 1e6,
    );
    Ok(())
}

fn main() -> Result<()> {
    println!("qwen36-35b-a3b-q4km-metal-gated-delta-net-verify");
    let rt = MetalRuntime::new()?;

    println!("--- correctness (kernel requires S_v/NSG = 32) ---");
    // Smallest valid: S_v=32 with NSG=1 → 1 simdgroup per row.
    run_correctness(&rt, "S_v=32 1/1/1", 32, 1, 1, 1, 1, 1)?;
    // GQA mismatch: 4 q-heads, 2 k-heads, 4 v-heads
    run_correctness(&rt, "S_v=32 4/2/4", 32, 4, 2, 4, 1, 1)?;
    // Multi-token recurrent scan
    run_correctness(&rt, "S_v=32 2/2/4 t=3", 32, 2, 2, 4, 3, 1)?;
    // Mid: S_v=64 with NSG=2
    run_correctness(&rt, "S_v=64 4/4/4", 64, 4, 4, 4, 1, 1)?;
    // Qwen3.6 actual shape: S_v=128 with NSG=4 (the production case)
    run_correctness(&rt, "S_v=128 16/16/32 (Qwen3.6)", 128, 16, 16, 32, 1, 1)?;

    println!();
    println!("--- bench at Qwen3.6 decode shape ---");
    run_qwen36_decode_bench(&rt)?;

    println!();
    println!("OK");
    Ok(())
}
