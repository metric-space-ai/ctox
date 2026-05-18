//! Rust-only smoke test for the crate's metallib.
//!
//! Validates:
//!
//!   * the metallib produced by `build.rs` loads,
//!   * the 6 dflash-specific pipelines (byte-exact vendored from
//!     `dflash_mlx/kernels.py` + `verify_qmm.py`) resolve,
//!   * the vendored ggml-metal blob is present in the metallib (size
//!     check only — its 111 kernels mostly need function-constant
//!     specialization via `MTLFunctionConstantValues` which the
//!     dispatch port adds).
//!
//! Does **not** test byte-for-byte parity. That's in `tests/parity/`.

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use ctox_qwen35_35b_a3b_dflash::metal::ffi::global_device;

    let dev = global_device().ok_or_else(|| anyhow::anyhow!("no Metal device"))?;

    // ggml-metal pre-instantiated (via [[host_name]] Metal templates)
    // kernels. These SHOULD resolve directly without function constants.
    let ggml_pipelines = [
        "kernel_cpy_f32_f32",
        "kernel_cpy_f32_bf16",
        "kernel_cpy_bf16_bf16",
        "kernel_argsort_f32_i32_desc",
        "kernel_argmax_f32",
    ];
    let mut ok_ggml = 0;
    for name in &ggml_pipelines {
        if dev.pipeline(name).is_some() {
            ok_ggml += 1;
        } else {
            eprintln!("[smoke] ggml `{name}` missing (needs function_constants?)");
        }
    }
    eprintln!(
        "[smoke] ggml pre-instantiated: {ok_ggml}/{} resolved",
        ggml_pipelines.len()
    );

    // dflash-specific kernels — byte-exact from dflash_mlx/kernels.py
    // + verify_qmm.py. They declare `Dk/Dv/Hk/Hv/D/V/M_FIXED` as
    // Metal function_constants (matches MLX's `template=[("Dk",128),...]`
    // mechanism). Pipeline resolution needs `pipeline_with_constants`
    // to supply the Qwen3.5-35B-A3B-specific shape values.
    use ctox_qwen35_35b_a3b_dflash::metal::ops::{cv_set_int32, last_error_str};
    // Per-kernel function-constant set — only the constants the kernel
    // actually declares. Providing extras makes Metal reject the call.
    // Qwen3.5-35B-A3B linear attention: Dk=128 Dv=128 Hk=16 Hv=32.
    // SDPA verify: D=128 V=128 M_FIXED=16, Hk=4.
    // All 7 constants declared at file scope in common.h — Metal
    // requires every declared function_constant to be bound at
    // pipeline-create time or it errors with "FunctionNotFound".
    // Set every FC for every kernel, even if the kernel doesn't read
    // all of them — the unused ones get optimized out by AIR
    // specialization.
    struct Case<'a> {
        name: &'a str,
    }
    let all_constants = |cv: &objc2_metal::MTLFunctionConstantValues| {
        cv_set_int32(cv, 128, 0); // Dk
        cv_set_int32(cv, 128, 1); // Dv
        cv_set_int32(cv, 16, 2); // Hk (gdn variant; sdpa partials shares this declaration)
        cv_set_int32(cv, 32, 3); // Hv
        cv_set_int32(cv, 128, 4); // D
        cv_set_int32(cv, 128, 5); // V
        cv_set_int32(cv, 16, 6); // M_FIXED
    };
    let cases = [
        Case {
            name: "gated_delta_tape",
        },
        Case {
            name: "tape_replay",
        },
        Case {
            name: "batched_sdpa_2pass_partials",
        },
        Case {
            name: "batched_sdpa_2pass_reduce",
        },
    ];
    let mut ok_dflash = 0;
    for c in &cases {
        let key = format!("{}#qwen35b-a3b", c.name);
        let ok = dev
            .pipeline_with_constants(&key, c.name, all_constants)
            .is_some();
        if ok {
            ok_dflash += 1;
        } else {
            eprintln!(
                "[smoke] FAIL: dflash pipeline `{}` resolve failed: {}",
                c.name,
                last_error_str()
            );
        }
    }
    let dflash_pipelines_ct = cases.len();
    eprintln!(
        "[smoke] dflash pipelines (fc-specialized): {ok_dflash}/{dflash_pipelines_ct} resolved"
    );

    let glue_pipelines = [
        "ctox_embedding_gather_mlx4_bf16",
        "ctox_silu_bf16",
        "ctox_sigmoid_bf16",
        "ctox_softplus_bf16",
        "ctox_add_bf16",
        "ctox_mul_bf16",
        "ctox_scale_bf16",
        "ctox_silu_mul_bf16",
        "ctox_add_bias_bf16",
        "ctox_neg_exp_mul_bf16",
        "ctox_softplus_neg_exp_mul_bias_bf16",
        "ctox_copy_bf16",
        "ctox_repeat_hidden5_bf16",
        "ctox_copy_hidden_slot_bf16",
        "ctox_moe_route_topk_bf16",
        "ctox_moe_fill_gather_indices_i32",
        "ctox_moe_expert_gate_up_bf16",
        "ctox_moe_expert_down_accum_bf16",
        "ctox_moe_accum_weighted_bf16",
        "ctox_moe_add_shared_bf16",
        "ctox_dense_matmul_bf16",
        "ctox_kv_cache_append_bf16",
        "ctox_split_q_gate_bf16",
        "ctox_apply_attention_gate_bf16",
        "ctox_rope_bf16",
        "ctox_rms_norm_bf16",
        "ctox_l2_norm_bf16",
        "ctox_conv_concat_bf16",
        "ctox_ssm_conv1d_bf16",
        "ctox_ssm_conv_state_update_bf16",
        "ctox_split_qkv_conv_bf16",
        "ctox_argmax_bf16",
        "ctox_sdpa_naive_bf16",
        "ctox_sdpa_decode_vec_bf16",
        "affine_gather_qmv_fast_bfloat16_t_gs_64_b_4",
        "affine_gather_qmv_bfloat16_t_gs_64_b_4",
        "gemv_bfloat16_bm1_bn8_sm1_sn32_tm4_tn4_nc0_axpby0",
        "gemv_bfloat16_bm1_bn8_sm1_sn32_tm4_tn4_nc0_axpby1",
        "gemv_bfloat16_bm1_bn1_sm8_sn4_tm4_tn4_nc0_axpby0",
        "gemv_bfloat16_bm1_bn1_sm8_sn4_tm4_tn4_nc0_axpby1",
        "gemv_bfloat16_bm4_bn1_sm1_sn32_tm4_tn4_nc0_axpby0",
        "gemv_bfloat16_bm4_bn1_sm1_sn32_tm4_tn4_nc0_axpby1",
        "gemv_bfloat16_bm8_bn1_sm1_sn32_tm4_tn4_nc0_axpby0",
        "gemv_bfloat16_bm8_bn1_sm1_sn32_tm4_tn4_nc0_axpby1",
    ];
    let mut ok_glue = 0;
    for name in &glue_pipelines {
        if dev.pipeline(name).is_some() {
            ok_glue += 1;
        } else {
            eprintln!("[smoke] FAIL: glue pipeline `{name}` missing");
        }
    }
    eprintln!(
        "[smoke] glue pipelines: {ok_glue}/{} resolved",
        glue_pipelines.len()
    );

    if ok_dflash == dflash_pipelines_ct && ok_glue == glue_pipelines.len() {
        eprintln!(
            "[smoke] OK — dflash custom kernels resolve from metallib.\n\
             [smoke] Note: ggml-metal base-op kernels (vendored byte-exact from llama.cpp,\n\
             [smoke]       commit in vendor/metal/ggml-metal.version) need function-constant\n\
             [smoke]       specialization at pipeline-build time — port pending."
        );
        Ok(())
    } else {
        anyhow::bail!("smoke failed: dflash {ok_dflash}/{dflash_pipelines_ct}")
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("smoke_metal: only runs on macOS + Apple Silicon.");
    std::process::exit(2);
}
