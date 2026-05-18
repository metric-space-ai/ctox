#![cfg_attr(not(target_os = "linux"), allow(unused))]

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("smoke_cuda: Linux/CUDA only");
}

#[cfg(target_os = "linux")]
fn bf16(x: f32) -> u16 {
    (x.to_bits() >> 16) as u16
}

#[cfg(target_os = "linux")]
fn f32_from_bf16(x: u16) -> f32 {
    f32::from_bits((x as u32) << 16)
}

#[cfg(target_os = "linux")]
fn check_cuda(code: i32, what: &str) -> anyhow::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        anyhow::bail!("{what}: CUDA error {code}")
    }
}

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::ffi::{c_int, c_void};
    use std::mem::size_of;
    use std::ptr::null_mut;

    use ctox_qwen35_35b_a3b_dflash::cuda::{
        launch_add_bf16, launch_argmax_bf16, launch_causal_mask_f16, launch_copy_hidden_slot_bf16,
        launch_dense_matmul_bf16, launch_dequant_q4_k_bf16, launch_fill_positions4_i32,
        launch_kv_store_bf16, launch_moe_route_topk_bf16, launch_mul_bf16, launch_q4_k_matvec_bf16,
        launch_repeat_hidden_slots_bf16, launch_rms_norm_bf16, launch_sdpa_decode_bf16,
        launch_silu_bf16,
    };

    const CUDA_MEMCPY_HOST_TO_DEVICE: c_int = 1;
    const CUDA_MEMCPY_DEVICE_TO_HOST: c_int = 2;

    extern "C" {
        fn cudaMalloc(ptr: *mut *mut c_void, size: usize) -> c_int;
        fn cudaFree(ptr: *mut c_void) -> c_int;
        fn cudaMemcpy(dst: *mut c_void, src: *const c_void, count: usize, kind: c_int) -> c_int;
        fn cudaDeviceSynchronize() -> c_int;
    }

    let x_h = [bf16(1.0), bf16(2.0), bf16(3.0), bf16(4.0)];
    let w_h = [bf16(1.0); 4];
    let mut y_h = [0_u16; 4];
    let mat_w_h = [
        bf16(1.0),
        bf16(0.0),
        bf16(0.0),
        bf16(0.0),
        bf16(0.0),
        bf16(1.0),
        bf16(0.0),
        bf16(0.0),
    ];
    let bias_h = [bf16(0.5), bf16(-0.5)];
    let route_logits_h = [bf16(1.0), bf16(3.0), bf16(2.0), bf16(0.0)];
    let mut q4_h = [0_u8; 144];
    q4_h[0] = 0x00;
    q4_h[1] = 0x3c; // f16 1.0 little-endian super-block scale.
    q4_h[4..16].fill(1);
    q4_h[16..].fill(0x21);
    let x256_h = [bf16(1.0); 256];
    let mut mat_y_h = [0_u16; 2];
    let mut add_y_h = [0_u16; 4];
    let mut mul_y_h = [0_u16; 4];
    let mut silu_y_h = [0_u16; 4];
    let mut arg_y_h = [0_i32; 1];
    let mut slot_y_h = [0_u16; 8];
    let mut repeat_y_h = [0_u16; 8];
    let mut pos_y_h = [0_i32; 8];
    let mut mask_y_h = [0_u16; 8];
    let mut k_cache_h = [0_u16; 12];
    let mut sdpa_y_h = [0_u16; 4];
    let mut deq_y_h = [0_u16; 256];
    let mut q4_mv_y_h = [0_u16; 1];
    let mut topk_weights_h = [0_u16; 2];
    let mut topk_ids_h = [0_i32; 2];

    let mut x_d: *mut c_void = null_mut();
    let mut w_d: *mut c_void = null_mut();
    let mut y_d: *mut c_void = null_mut();
    let mut mat_w_d: *mut c_void = null_mut();
    let mut bias_d: *mut c_void = null_mut();
    let mut mat_y_d: *mut c_void = null_mut();
    let mut add_y_d: *mut c_void = null_mut();
    let mut mul_y_d: *mut c_void = null_mut();
    let mut silu_y_d: *mut c_void = null_mut();
    let mut arg_y_d: *mut c_void = null_mut();
    let mut slot_y_d: *mut c_void = null_mut();
    let mut repeat_y_d: *mut c_void = null_mut();
    let mut pos_y_d: *mut c_void = null_mut();
    let mut mask_y_d: *mut c_void = null_mut();
    let mut k_cache_d: *mut c_void = null_mut();
    let mut v_cache_d: *mut c_void = null_mut();
    let mut sdpa_y_d: *mut c_void = null_mut();
    let mut q4_d: *mut c_void = null_mut();
    let mut deq_y_d: *mut c_void = null_mut();
    let mut x256_d: *mut c_void = null_mut();
    let mut q4_mv_y_d: *mut c_void = null_mut();
    let mut route_logits_d: *mut c_void = null_mut();
    let mut topk_weights_d: *mut c_void = null_mut();
    let mut topk_ids_d: *mut c_void = null_mut();
    let nbytes = x_h.len() * size_of::<u16>();

    unsafe {
        check_cuda(cudaMalloc(&mut x_d, nbytes), "cudaMalloc x")?;
        check_cuda(cudaMalloc(&mut w_d, nbytes), "cudaMalloc w")?;
        check_cuda(cudaMalloc(&mut y_d, nbytes), "cudaMalloc y")?;
        check_cuda(
            cudaMalloc(&mut mat_w_d, mat_w_h.len() * size_of::<u16>()),
            "cudaMalloc mat_w",
        )?;
        check_cuda(
            cudaMalloc(&mut bias_d, bias_h.len() * size_of::<u16>()),
            "cudaMalloc bias",
        )?;
        check_cuda(
            cudaMalloc(&mut mat_y_d, mat_y_h.len() * size_of::<u16>()),
            "cudaMalloc mat_y",
        )?;
        check_cuda(
            cudaMalloc(&mut add_y_d, add_y_h.len() * size_of::<u16>()),
            "cudaMalloc add_y",
        )?;
        check_cuda(
            cudaMalloc(&mut mul_y_d, mul_y_h.len() * size_of::<u16>()),
            "cudaMalloc mul_y",
        )?;
        check_cuda(
            cudaMalloc(&mut silu_y_d, silu_y_h.len() * size_of::<u16>()),
            "cudaMalloc silu_y",
        )?;
        check_cuda(
            cudaMalloc(&mut arg_y_d, arg_y_h.len() * size_of::<i32>()),
            "cudaMalloc arg_y",
        )?;
        check_cuda(
            cudaMalloc(&mut slot_y_d, slot_y_h.len() * size_of::<u16>()),
            "cudaMalloc slot_y",
        )?;
        check_cuda(
            cudaMalloc(&mut repeat_y_d, repeat_y_h.len() * size_of::<u16>()),
            "cudaMalloc repeat_y",
        )?;
        check_cuda(
            cudaMalloc(&mut pos_y_d, pos_y_h.len() * size_of::<i32>()),
            "cudaMalloc pos_y",
        )?;
        check_cuda(
            cudaMalloc(&mut mask_y_d, mask_y_h.len() * size_of::<u16>()),
            "cudaMalloc mask_y",
        )?;
        check_cuda(
            cudaMalloc(&mut k_cache_d, k_cache_h.len() * size_of::<u16>()),
            "cudaMalloc k_cache",
        )?;
        check_cuda(
            cudaMalloc(&mut v_cache_d, k_cache_h.len() * size_of::<u16>()),
            "cudaMalloc v_cache",
        )?;
        check_cuda(
            cudaMalloc(&mut sdpa_y_d, sdpa_y_h.len() * size_of::<u16>()),
            "cudaMalloc sdpa_y",
        )?;
        check_cuda(cudaMalloc(&mut q4_d, q4_h.len()), "cudaMalloc q4")?;
        check_cuda(
            cudaMalloc(&mut deq_y_d, deq_y_h.len() * size_of::<u16>()),
            "cudaMalloc deq_y",
        )?;
        check_cuda(
            cudaMalloc(&mut x256_d, x256_h.len() * size_of::<u16>()),
            "cudaMalloc x256",
        )?;
        check_cuda(
            cudaMalloc(&mut q4_mv_y_d, q4_mv_y_h.len() * size_of::<u16>()),
            "cudaMalloc q4_mv_y",
        )?;
        check_cuda(
            cudaMalloc(&mut route_logits_d, route_logits_h.len() * size_of::<u16>()),
            "cudaMalloc route_logits",
        )?;
        check_cuda(
            cudaMalloc(&mut topk_weights_d, topk_weights_h.len() * size_of::<u16>()),
            "cudaMalloc topk_weights",
        )?;
        check_cuda(
            cudaMalloc(&mut topk_ids_d, topk_ids_h.len() * size_of::<i32>()),
            "cudaMalloc topk_ids",
        )?;

        check_cuda(
            cudaMemcpy(
                x_d,
                x_h.as_ptr().cast::<c_void>(),
                nbytes,
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ),
            "cudaMemcpy x H2D",
        )?;
        check_cuda(
            cudaMemcpy(
                w_d,
                w_h.as_ptr().cast::<c_void>(),
                nbytes,
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ),
            "cudaMemcpy w H2D",
        )?;
        check_cuda(
            cudaMemcpy(
                mat_w_d,
                mat_w_h.as_ptr().cast::<c_void>(),
                mat_w_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ),
            "cudaMemcpy mat_w H2D",
        )?;
        check_cuda(
            cudaMemcpy(
                bias_d,
                bias_h.as_ptr().cast::<c_void>(),
                bias_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ),
            "cudaMemcpy bias H2D",
        )?;
        check_cuda(
            cudaMemcpy(
                q4_d,
                q4_h.as_ptr().cast::<c_void>(),
                q4_h.len(),
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ),
            "cudaMemcpy q4 H2D",
        )?;
        check_cuda(
            cudaMemcpy(
                x256_d,
                x256_h.as_ptr().cast::<c_void>(),
                x256_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ),
            "cudaMemcpy x256 H2D",
        )?;
        check_cuda(
            cudaMemcpy(
                route_logits_d,
                route_logits_h.as_ptr().cast::<c_void>(),
                route_logits_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ),
            "cudaMemcpy route_logits H2D",
        )?;

        launch_rms_norm_bf16(x_d, w_d, y_d, 4, 0.0, 0.0, 1, null_mut())?;
        launch_dense_matmul_bf16(x_d, mat_w_d, bias_d, mat_y_d, 1, 4, 2, true, null_mut())?;
        launch_add_bf16(x_d, w_d, add_y_d, 4, null_mut())?;
        launch_mul_bf16(x_d, w_d, mul_y_d, 4, null_mut())?;
        launch_silu_bf16(x_d, silu_y_d, 4, null_mut())?;
        launch_argmax_bf16(x_d, arg_y_d.cast::<i32>(), 4, 1, null_mut())?;
        launch_moe_route_topk_bf16(
            route_logits_d,
            topk_weights_d,
            topk_ids_d.cast::<i32>(),
            2,
            4,
            1,
            null_mut(),
        )?;
        launch_copy_hidden_slot_bf16(x_d, slot_y_d, 0, 1, 4, 2, null_mut())?;
        launch_repeat_hidden_slots_bf16(x_d, repeat_y_d, 4, 2, null_mut())?;
        launch_fill_positions4_i32(pos_y_d.cast::<i32>(), 7, 2, null_mut())?;
        launch_causal_mask_f16(mask_y_d, 1, 2, 3, 4, null_mut())?;
        launch_kv_store_bf16(x_d, k_cache_d, std::ptr::null(), 1, 2, 2, 3, null_mut())?;
        launch_kv_store_bf16(x_d, v_cache_d, std::ptr::null(), 1, 2, 2, 3, null_mut())?;
        launch_sdpa_decode_bf16(
            x_d,
            k_cache_d,
            v_cache_d,
            sdpa_y_d,
            2,
            2,
            2,
            1,
            3,
            1.0,
            null_mut(),
        )?;
        launch_dequant_q4_k_bf16(q4_d, deq_y_d, 1, null_mut())?;
        launch_q4_k_matvec_bf16(q4_d, x256_d, q4_mv_y_d, 256, 1, null_mut())?;
        check_cuda(cudaDeviceSynchronize(), "cudaDeviceSynchronize")?;
        check_cuda(
            cudaMemcpy(
                y_h.as_mut_ptr().cast::<c_void>(),
                y_d,
                nbytes,
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                mat_y_h.as_mut_ptr().cast::<c_void>(),
                mat_y_d,
                mat_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy mat_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                add_y_h.as_mut_ptr().cast::<c_void>(),
                add_y_d,
                add_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy add_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                mul_y_h.as_mut_ptr().cast::<c_void>(),
                mul_y_d,
                mul_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy mul_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                silu_y_h.as_mut_ptr().cast::<c_void>(),
                silu_y_d,
                silu_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy silu_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                arg_y_h.as_mut_ptr().cast::<c_void>(),
                arg_y_d,
                arg_y_h.len() * size_of::<i32>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy arg_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                slot_y_h.as_mut_ptr().cast::<c_void>(),
                slot_y_d,
                slot_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy slot_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                repeat_y_h.as_mut_ptr().cast::<c_void>(),
                repeat_y_d,
                repeat_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy repeat_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                pos_y_h.as_mut_ptr().cast::<c_void>(),
                pos_y_d,
                pos_y_h.len() * size_of::<i32>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy pos_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                mask_y_h.as_mut_ptr().cast::<c_void>(),
                mask_y_d,
                mask_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy mask_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                k_cache_h.as_mut_ptr().cast::<c_void>(),
                k_cache_d,
                k_cache_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy k_cache D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                sdpa_y_h.as_mut_ptr().cast::<c_void>(),
                sdpa_y_d,
                sdpa_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy sdpa_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                deq_y_h.as_mut_ptr().cast::<c_void>(),
                deq_y_d,
                deq_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy deq_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                q4_mv_y_h.as_mut_ptr().cast::<c_void>(),
                q4_mv_y_d,
                q4_mv_y_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy q4_mv_y D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                topk_weights_h.as_mut_ptr().cast::<c_void>(),
                topk_weights_d,
                topk_weights_h.len() * size_of::<u16>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy topk_weights D2H",
        )?;
        check_cuda(
            cudaMemcpy(
                topk_ids_h.as_mut_ptr().cast::<c_void>(),
                topk_ids_d,
                topk_ids_h.len() * size_of::<i32>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            ),
            "cudaMemcpy topk_ids D2H",
        )?;

        let _ = cudaFree(x_d);
        let _ = cudaFree(w_d);
        let _ = cudaFree(y_d);
        let _ = cudaFree(mat_w_d);
        let _ = cudaFree(bias_d);
        let _ = cudaFree(mat_y_d);
        let _ = cudaFree(add_y_d);
        let _ = cudaFree(mul_y_d);
        let _ = cudaFree(silu_y_d);
        let _ = cudaFree(arg_y_d);
        let _ = cudaFree(slot_y_d);
        let _ = cudaFree(repeat_y_d);
        let _ = cudaFree(pos_y_d);
        let _ = cudaFree(mask_y_d);
        let _ = cudaFree(k_cache_d);
        let _ = cudaFree(v_cache_d);
        let _ = cudaFree(sdpa_y_d);
        let _ = cudaFree(q4_d);
        let _ = cudaFree(deq_y_d);
        let _ = cudaFree(x256_d);
        let _ = cudaFree(q4_mv_y_d);
        let _ = cudaFree(route_logits_d);
        let _ = cudaFree(topk_weights_d);
        let _ = cudaFree(topk_ids_d);
    }

    let got: Vec<f32> = y_h.into_iter().map(f32_from_bf16).collect();
    let mat_got: Vec<f32> = mat_y_h.into_iter().map(f32_from_bf16).collect();
    let add_got: Vec<f32> = add_y_h.into_iter().map(f32_from_bf16).collect();
    let mul_got: Vec<f32> = mul_y_h.into_iter().map(f32_from_bf16).collect();
    let silu_got: Vec<f32> = silu_y_h.into_iter().map(f32_from_bf16).collect();
    let inv = (7.5_f32).sqrt().recip();
    let expected = [1.0 * inv, 2.0 * inv, 3.0 * inv, 4.0 * inv];
    for (i, (g, e)) in got.iter().zip(expected).enumerate() {
        if (g - e).abs() > 0.02 {
            anyhow::bail!("slot {i}: got {g}, expected {e}");
        }
    }
    if (mat_got[0] - 1.5).abs() > 0.02 || (mat_got[1] - 1.5).abs() > 0.02 {
        anyhow::bail!("matmul got {mat_got:?}, expected [1.5, 1.5]");
    }
    for (i, (g, e)) in add_got.iter().zip([2.0, 3.0, 4.0, 5.0]).enumerate() {
        if (g - e).abs() > 0.02 {
            anyhow::bail!("add slot {i}: got {g}, expected {e}");
        }
    }
    for (i, (g, e)) in mul_got.iter().zip([1.0, 2.0, 3.0, 4.0]).enumerate() {
        if (g - e).abs() > 0.02 {
            anyhow::bail!("mul slot {i}: got {g}, expected {e}");
        }
    }
    for (i, (g, x)) in silu_got.iter().zip([1.0_f32, 2.0, 3.0, 4.0]).enumerate() {
        let e = x / (1.0 + (-x).exp());
        if (g - e).abs() > 0.02 {
            anyhow::bail!("silu slot {i}: got {g}, expected {e}");
        }
    }
    if arg_y_h[0] != 3 {
        anyhow::bail!("argmax got {}, expected 3", arg_y_h[0]);
    }
    let topk_weights_got: Vec<f32> = topk_weights_h.into_iter().map(f32_from_bf16).collect();
    if topk_ids_h != [1, 2] {
        anyhow::bail!("moe_route_topk ids got {topk_ids_h:?}, expected [1, 2]");
    }
    for (i, (g, e)) in topk_weights_got.iter().zip([0.731_f32, 0.269]).enumerate() {
        if (g - e).abs() > 0.03 {
            anyhow::bail!("moe_route_topk weight {i}: got {g}, expected {e}");
        }
    }
    if slot_y_h[4..] != [bf16(1.0), bf16(2.0), bf16(3.0), bf16(4.0)] {
        anyhow::bail!("copy_hidden_slot got {slot_y_h:?}");
    }
    if repeat_y_h
        != [
            bf16(1.0),
            bf16(2.0),
            bf16(3.0),
            bf16(4.0),
            bf16(1.0),
            bf16(2.0),
            bf16(3.0),
            bf16(4.0),
        ]
    {
        anyhow::bail!("repeat_hidden_slots got {repeat_y_h:?}");
    }
    if pos_y_h != [7, 7, 7, 7, 8, 8, 8, 8] {
        anyhow::bail!("positions4 got {pos_y_h:?}");
    }
    if mask_y_h
        != [
            0x0000, 0x0000, 0xfc00, 0xfc00, 0x0000, 0x0000, 0x0000, 0xfc00,
        ]
    {
        anyhow::bail!("causal_mask got {mask_y_h:x?}");
    }
    if k_cache_h[0..2] != [bf16(1.0), bf16(2.0)] || k_cache_h[6..8] != [bf16(3.0), bf16(4.0)] {
        anyhow::bail!("kv_store got {k_cache_h:?}");
    }
    if sdpa_y_h != [bf16(1.0), bf16(2.0), bf16(3.0), bf16(4.0)] {
        anyhow::bail!("sdpa_decode got {sdpa_y_h:?}");
    }
    if deq_y_h[0..4] != [bf16(1.0); 4] || deq_y_h[32..36] != [bf16(2.0); 4] {
        anyhow::bail!(
            "dequant_q4_k got first={:?} hi={:?}",
            &deq_y_h[0..4],
            &deq_y_h[32..36]
        );
    }
    let q4_mv = f32_from_bf16(q4_mv_y_h[0]);
    if (q4_mv - 384.0).abs() > 1.0 {
        anyhow::bail!("q4_k_matvec got {q4_mv}, expected about 384");
    }

    println!(
        "[smoke-cuda] qwen35-35b glue kernels OK: norm={got:?} matmul={mat_got:?} add={add_got:?} mul={mul_got:?} silu={silu_got:?} argmax={:?} topk_ids={topk_ids_h:?} topk_weights={topk_weights_got:?} positions={pos_y_h:?} sdpa={:?} q4k=[{},{}] q4mv={q4_mv}",
        arg_y_h,
        sdpa_y_h,
        f32_from_bf16(deq_y_h[0]),
        f32_from_bf16(deq_y_h[32])
    );
    Ok(())
}
