//! Functional smoke for the Qwen3.5-35B-A3B Metal glue wrappers.
//!
//! `smoke_metal` checks pipeline resolution. This example dispatches a
//! representative subset of the 35B-local glue kernels with tiny buffers and
//! verifies the CPU-visible results. It catches argument-order and dispatch
//! shape mistakes without loading model weights.

#[cfg(target_os = "macos")]
fn bf16(x: f32) -> u16 {
    (x.to_bits() >> 16) as u16
}

#[cfg(target_os = "macos")]
fn f32_from_bf16(x: u16) -> f32 {
    f32::from_bits((x as u32) << 16)
}

#[cfg(target_os = "macos")]
fn assert_close(name: &str, got: &[u16], expected: &[f32], tol: f32) -> anyhow::Result<()> {
    anyhow::ensure!(
        got.len() == expected.len(),
        "{name}: got len {}, expected {}",
        got.len(),
        expected.len()
    );
    for (i, (&g, &e)) in got.iter().zip(expected.iter()).enumerate() {
        let gf = f32_from_bf16(g);
        anyhow::ensure!((gf - e).abs() <= tol, "{name}[{i}]: got {gf}, expected {e}");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn check(ok: bool, what: &str) -> anyhow::Result<()> {
    use ctox_qwen35_35b_a3b_dflash::metal::ops::last_error_str;

    if ok {
        Ok(())
    } else {
        anyhow::bail!("{what}: {}", last_error_str())
    }
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use ctox_qwen35_35b_a3b_dflash::metal::ffi::global_device;
    use ctox_qwen35_35b_a3b_dflash::metal::kernels;

    let dev = global_device().ok_or_else(|| anyhow::anyhow!("no Metal device"))?;
    let x = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc x"))?;
    let ones = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc ones"))?;
    let bias2 = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc bias2"))?;
    let mat_w = dev
        .new_buffer(8 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc mat_w"))?;
    let add_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc add_y"))?;
    let mul_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc mul_y"))?;
    let scale_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc scale_y"))?;
    let silu_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc silu_y"))?;
    let silu_mul_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc silu_mul_y"))?;
    let sigmoid_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc sigmoid_y"))?;
    let softplus_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc softplus_y"))?;
    let add_bias_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc add_bias_y"))?;
    let neg_exp_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc neg_exp_y"))?;
    let fused_g_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc fused_g_y"))?;
    let copy_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc copy_y"))?;
    let repeat_y = dev
        .new_buffer(10 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc repeat_y"))?;
    let slot_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc slot_y"))?;
    let rms_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc rms_y"))?;
    let l2_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc l2_y"))?;
    let mat_y = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc mat_y"))?;
    let arg_y = dev
        .new_buffer(std::mem::size_of::<i32>())
        .ok_or_else(|| anyhow::anyhow!("alloc arg_y"))?;
    let logits = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc logits"))?;
    let topk_ids = dev
        .new_buffer(2 * std::mem::size_of::<i32>())
        .ok_or_else(|| anyhow::anyhow!("alloc topk_ids"))?;
    let topk_weights = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc topk_weights"))?;
    let moe_lhs_token = dev
        .new_buffer(2 * std::mem::size_of::<i32>())
        .ok_or_else(|| anyhow::anyhow!("alloc moe_lhs_token"))?;
    let moe_lhs_slot = dev
        .new_buffer(2 * std::mem::size_of::<i32>())
        .ok_or_else(|| anyhow::anyhow!("alloc moe_lhs_slot"))?;
    let moe_down_slots = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc moe_down_slots"))?;
    let moe_accum_y = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc moe_accum_y"))?;
    let kv_cache = dev
        .new_buffer(8 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc kv_cache"))?;
    let raw_q_gate = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc raw_q_gate"))?;
    let q_split = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc q_split"))?;
    let gate_split = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc gate_split"))?;
    let attn_gate = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc attn_gate"))?;
    let gate_zero = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc gate_zero"))?;
    let rope_y = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc rope_y"))?;
    let positions = dev
        .new_buffer(std::mem::size_of::<i32>())
        .ok_or_else(|| anyhow::anyhow!("alloc positions"))?;
    let conv_state = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc conv_state"))?;
    let qkv_new = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc qkv_new"))?;
    let conv_concat_y = dev
        .new_buffer(8 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc conv_concat_y"))?;
    let conv_weight = dev
        .new_buffer(6 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc conv_weight"))?;
    let conv_bias = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc conv_bias"))?;
    let conv_y = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc conv_y"))?;
    let conv_state_out = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc conv_state_out"))?;
    let split_conv = dev
        .new_buffer(5 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc split_conv"))?;
    let split_q = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc split_q"))?;
    let split_k = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc split_k"))?;
    let split_v = dev
        .new_buffer(std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc split_v"))?;
    let sdpa_q = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc sdpa_q"))?;
    let sdpa_k = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc sdpa_k"))?;
    let sdpa_v = dev
        .new_buffer(4 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc sdpa_v"))?;
    let sdpa_y = dev
        .new_buffer(2 * std::mem::size_of::<u16>())
        .ok_or_else(|| anyhow::anyhow!("alloc sdpa_y"))?;

    unsafe {
        x.write(0, &[bf16(1.0), bf16(2.0), bf16(3.0), bf16(4.0)]);
        ones.write(0, &[bf16(1.0); 4]);
        bias2.write(0, &[bf16(0.5), bf16(-0.5)]);
        mat_w.write(
            0,
            &[
                bf16(1.0),
                bf16(0.0),
                bf16(0.0),
                bf16(0.0),
                bf16(0.0),
                bf16(1.0),
                bf16(0.0),
                bf16(0.0),
            ],
        );
        slot_y.write(0, &[bf16(0.0); 4]);
        logits.write(0, &[bf16(1.0), bf16(3.0), bf16(2.0), bf16(0.0)]);
        moe_down_slots.write(0, &[bf16(10.0), bf16(20.0), bf16(100.0), bf16(200.0)]);
        kv_cache.write(0, &[bf16(0.0); 8]);
        raw_q_gate.write(0, &[bf16(10.0), bf16(11.0), bf16(20.0), bf16(21.0)]);
        attn_gate.write(0, &[bf16(2.0), bf16(4.0)]);
        gate_zero.write(0, &[bf16(0.0), bf16(0.0)]);
        positions.write(0, &[0i32]);
        conv_state.write(0, &[bf16(1.0), bf16(2.0), bf16(3.0), bf16(4.0)]);
        qkv_new.write(0, &[bf16(5.0), bf16(6.0), bf16(7.0), bf16(8.0)]);
        conv_weight.write(0, &[bf16(1.0); 6]);
        conv_bias.write(0, &[bf16(0.0), bf16(0.0)]);
        split_conv.write(0, &[bf16(1.0), bf16(2.0), bf16(3.0), bf16(4.0), bf16(5.0)]);
        sdpa_q.write(0, &[bf16(1.0), bf16(0.0)]);
        sdpa_k.write(0, &[bf16(1.0), bf16(0.0), bf16(0.0), bf16(1.0)]);
        sdpa_v.write(0, &[bf16(10.0), bf16(20.0), bf16(30.0), bf16(40.0)]);
    }

    let cb = dev
        .new_command_buffer()
        .ok_or_else(|| anyhow::anyhow!("new command buffer"))?;
    let enc = cb
        .compute()
        .ok_or_else(|| anyhow::anyhow!("new compute encoder"))?;

    check(kernels::add_bf16(&enc, dev, &x, &ones, &add_y, 4), "add")?;
    check(kernels::mul_bf16(&enc, dev, &x, &ones, &mul_y, 4), "mul")?;
    check(
        kernels::scale_bf16(&enc, dev, &x, &scale_y, 0.5, 4),
        "scale",
    )?;
    check(kernels::silu_bf16(&enc, dev, &x, &silu_y, 4), "silu")?;
    check(
        kernels::silu_mul_bf16(&enc, dev, &x, &ones, &silu_mul_y, 4),
        "silu_mul",
    )?;
    check(
        kernels::sigmoid_bf16(&enc, dev, &x, &sigmoid_y, 4),
        "sigmoid",
    )?;
    check(
        kernels::softplus_bf16(&enc, dev, &x, &softplus_y, 4),
        "softplus",
    )?;
    check(
        kernels::add_bias_bf16(&enc, dev, &x, &bias2, &add_bias_y, 2, 2),
        "add_bias",
    )?;
    check(
        kernels::neg_exp_mul_bf16(&enc, dev, &x, &bias2, &neg_exp_y, 2, 2),
        "neg_exp_mul",
    )?;
    check(
        kernels::softplus_neg_exp_mul_bias_bf16(&enc, dev, &x, &bias2, &bias2, &fused_g_y, 2, 2),
        "softplus_neg_exp_mul_bias",
    )?;
    check(kernels::copy_raw_bf16(&enc, dev, &x, &copy_y, 4), "copy")?;
    check(
        kernels::repeat_hidden5_bf16(&enc, dev, &x, &repeat_y, 0, 1, 2),
        "repeat_hidden5",
    )?;
    check(
        kernels::copy_hidden_slot_bf16(&enc, dev, &x, &slot_y, 1, 2, 1),
        "copy_hidden_slot",
    )?;
    check(
        kernels::rms_norm_bf16(&enc, dev, &x, &ones, &rms_y, 4, 0.0, 0.0, 1),
        "rms_norm",
    )?;
    check(
        kernels::l2_norm_last_bf16(&enc, dev, &x, &l2_y, 4, 0.0, 1),
        "l2_norm",
    )?;
    check(
        kernels::dense_matmul_bf16(&enc, dev, &x, &mat_w, Some(&bias2), &mat_y, 1, 4, 2),
        "dense_matmul",
    )?;
    check(
        kernels::argmax_last_bf16(&enc, dev, &x, &arg_y, 4, 1),
        "argmax",
    )?;
    check(
        kernels::moe_route_topk_bf16(&enc, dev, &logits, &topk_ids, &topk_weights, 1, 4, 2),
        "moe_route_topk",
    )?;
    check(
        kernels::moe_fill_gather_indices_i32(&enc, dev, &moe_lhs_token, &moe_lhs_slot, 1, 2),
        "moe_fill_gather_indices",
    )?;
    check(
        kernels::moe_accum_weighted_bf16(
            &enc,
            dev,
            &moe_down_slots,
            &topk_weights,
            &moe_accum_y,
            1,
            2,
            2,
        ),
        "moe_accum_weighted",
    )?;
    check(
        kernels::kv_cache_append_bf16(&enc, dev, &x, &kv_cache, 2, 1, 2, 4, 1),
        "kv_cache_append",
    )?;
    check(
        kernels::split_q_gate_bf16(&enc, dev, &raw_q_gate, &q_split, &gate_split, 1, 2, 2),
        "split_q_gate",
    )?;
    check(
        kernels::apply_attention_gate_bf16(&enc, dev, &attn_gate, &gate_zero, 2),
        "apply_attention_gate",
    )?;
    check(
        kernels::rope_apply_bf16(&enc, dev, &x, &positions, &rope_y, 4, 4, 1, 10_000.0, 1),
        "rope_apply",
    )?;
    check(
        kernels::conv_concat_bf16(&enc, dev, &conv_state, &qkv_new, &conv_concat_y, 2, 2, 2),
        "conv_concat",
    )?;
    check(
        kernels::ssm_conv1d_bf16(
            &enc,
            dev,
            &conv_state,
            &qkv_new,
            &conv_weight,
            &conv_bias,
            &conv_y,
            &conv_state_out,
            1,
            2,
            3,
            true,
        ),
        "ssm_conv1d",
    )?;
    check(
        kernels::split_qkv_conv_bf16(
            &enc,
            dev,
            &split_conv,
            &split_q,
            &split_k,
            &split_v,
            1,
            2,
            1,
            5,
        ),
        "split_qkv_conv",
    )?;
    check(
        kernels::sdpa_naive_bf16(
            &enc, dev, &sdpa_q, &sdpa_k, &sdpa_v, None, &sdpa_y, 1, 1, 1, 2, 2, 1.0, true,
        ),
        "sdpa_decode_vec",
    )?;

    enc.end();
    cb.commit_and_wait()
        .map_err(|e| anyhow::anyhow!("command buffer failed: {e}"))?;

    let mut got4 = [0u16; 4];
    unsafe { add_y.read(0, &mut got4) };
    assert_close("add", &got4, &[2.0, 3.0, 4.0, 5.0], 0.02)?;
    unsafe { mul_y.read(0, &mut got4) };
    assert_close("mul", &got4, &[1.0, 2.0, 3.0, 4.0], 0.02)?;
    unsafe { scale_y.read(0, &mut got4) };
    assert_close("scale", &got4, &[0.5, 1.0, 1.5, 2.0], 0.02)?;
    unsafe { silu_y.read(0, &mut got4) };
    assert_close("silu", &got4, &[0.731, 1.762, 2.858, 3.928], 0.04)?;
    unsafe { silu_mul_y.read(0, &mut got4) };
    assert_close("silu_mul", &got4, &[0.731, 1.762, 2.858, 3.928], 0.04)?;
    unsafe { sigmoid_y.read(0, &mut got4) };
    assert_close("sigmoid", &got4, &[0.731, 0.881, 0.953, 0.982], 0.02)?;
    unsafe { softplus_y.read(0, &mut got4) };
    assert_close("softplus", &got4, &[1.313, 2.127, 3.049, 4.018], 0.04)?;
    unsafe { add_bias_y.read(0, &mut got4) };
    assert_close("add_bias", &got4, &[1.5, 1.5, 3.5, 3.5], 0.02)?;
    unsafe { neg_exp_y.read(0, &mut got4) };
    assert_close(
        "neg_exp_mul",
        &got4,
        &[-1.6487, -1.2131, -4.946, -2.426],
        0.08,
    )?;
    unsafe { fused_g_y.read(0, &mut got4) };
    assert_close(
        "softplus_neg_exp_mul_bias",
        &got4,
        &[0.0605, 0.356, 0.00296, 0.1177],
        0.08,
    )?;
    unsafe { copy_y.read(0, &mut got4) };
    assert_close("copy", &got4, &[1.0, 2.0, 3.0, 4.0], 0.02)?;
    unsafe { slot_y.read(0, &mut got4) };
    assert_close("copy_hidden_slot", &got4, &[0.0, 0.0, 3.0, 4.0], 0.02)?;
    unsafe { rms_y.read(0, &mut got4) };
    assert_close("rms_norm", &got4, &[0.365, 0.730, 1.095, 1.461], 0.03)?;
    unsafe { l2_y.read(0, &mut got4) };
    assert_close("l2_norm", &got4, &[0.183, 0.365, 0.548, 0.730], 0.03)?;

    let mut got2 = [0u16; 2];
    unsafe { mat_y.read(0, &mut got2) };
    assert_close("dense_matmul", &got2, &[1.5, 1.5], 0.03)?;
    unsafe { topk_weights.read(0, &mut got2) };
    assert_close("moe_topk_weights", &got2, &[0.269, 0.731], 0.03)?;
    unsafe { moe_accum_y.read(0, &mut got2) };
    assert_close("moe_accum_weighted", &got2, &[75.8, 151.6], 0.8)?;
    unsafe { q_split.read(0, &mut got2) };
    assert_close("split_q_gate.q", &got2, &[10.0, 11.0], 0.03)?;
    unsafe { gate_split.read(0, &mut got2) };
    assert_close("split_q_gate.gate", &got2, &[20.0, 21.0], 0.03)?;
    unsafe { attn_gate.read(0, &mut got2) };
    assert_close("apply_attention_gate", &got2, &[1.0, 2.0], 0.03)?;
    unsafe { conv_y.read(0, &mut got2) };
    assert_close("ssm_conv1d", &got2, &[9.0, 12.0], 0.05)?;
    unsafe { sdpa_y.read(0, &mut got2) };
    assert_close("sdpa_decode_vec", &got2, &[15.38, 25.38], 0.08)?;
    unsafe { split_q.read(0, &mut got2) };
    assert_close("split_qkv_conv.q", &got2, &[1.0, 2.0], 0.03)?;
    unsafe { split_k.read(0, &mut got2) };
    assert_close("split_qkv_conv.k", &got2, &[3.0, 4.0], 0.03)?;

    let mut got10 = [0u16; 10];
    unsafe { repeat_y.read(0, &mut got10) };
    assert_close(
        "repeat_hidden5",
        &got10,
        &[1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0],
        0.02,
    )?;
    let mut got8 = [0u16; 8];
    unsafe { kv_cache.read(0, &mut got8) };
    assert_close(
        "kv_cache_append",
        &got8,
        &[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0],
        0.03,
    )?;
    unsafe { conv_concat_y.read(0, &mut got8) };
    assert_close(
        "conv_concat",
        &got8,
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        0.03,
    )?;
    unsafe { rope_y.read(0, &mut got4) };
    assert_close("rope_apply_pos0", &got4, &[1.0, 2.0, 3.0, 4.0], 0.03)?;
    unsafe { conv_state_out.read(0, &mut got4) };
    assert_close("ssm_conv_state_update", &got4, &[3.0, 4.0, 5.0, 6.0], 0.03)?;

    let mut got1 = [0u16; 1];
    unsafe { split_v.read(0, &mut got1) };
    assert_close("split_qkv_conv.v", &got1, &[5.0], 0.03)?;

    let mut arg = [0i32; 1];
    unsafe { arg_y.read(0, &mut arg) };
    anyhow::ensure!(arg[0] == 3, "argmax: got {}, expected 3", arg[0]);

    let mut ids = [0i32; 2];
    unsafe { topk_ids.read(0, &mut ids) };
    anyhow::ensure!(ids == [2, 1], "topk ids: got {ids:?}, expected [2, 1]");
    unsafe { moe_lhs_token.read(0, &mut ids) };
    anyhow::ensure!(
        ids == [0, 0],
        "moe lhs token ids: got {ids:?}, expected [0, 0]"
    );
    unsafe { moe_lhs_slot.read(0, &mut ids) };
    anyhow::ensure!(
        ids == [0, 1],
        "moe lhs slot ids: got {ids:?}, expected [0, 1]"
    );

    println!(
        "[smoke-metal-glue-ops] OK: elementwise/norm/matmul/copy/topk/moe-gather-glue/kv/rope/conv wrappers execute"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("smoke_metal_glue_ops: only runs on macOS + Apple Silicon.");
    std::process::exit(2);
}
