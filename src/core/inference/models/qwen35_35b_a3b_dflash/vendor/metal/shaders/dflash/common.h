// Copyright 2026 ctox. Portions derived from bstnxbt/dflash-mlx (MIT)
// and Apple MLX (MIT). Kept in this crate so the Metal backend has
// zero external dependency on the MLX framework.
//
// This header provides the tiny subset of MLX-side Metal helpers that
// the vendored dflash kernels rely on: `Limits<T>::finite_min` and a
// convenience alias so kernels can read / write bf16 cleanly.
//
// ref: mlx/backend/metal/kernels/utils.h (upstream MLX)

#pragma once

#include <metal_stdlib>
#include <metal_math>
#include <metal_simdgroup>

using namespace metal;

// ── Type aliases ─────────────────────────────────────────────────────
//
// The Rust driver decides the input dtype when it creates the
// `MTLComputePipelineState` (one pipeline per dtype) by passing `-D`
// to `xcrun metal` at build time. If no variant is requested, default
// to bf16 — that's the dtype used by Qwen3.5-27B-4bit weights and by
// every hot-path activation in MLX for this model.

#if !defined(INPUT_DTYPE_BF16) && !defined(INPUT_DTYPE_F16) && !defined(INPUT_DTYPE_F32)
  #define INPUT_DTYPE_BF16
#endif

#if defined(INPUT_DTYPE_BF16)
  using InT = bfloat;
#elif defined(INPUT_DTYPE_F16)
  using InT = half;
#elif defined(INPUT_DTYPE_F32)
  using InT = float;
#endif

// ── Numeric limits mirror of MLX's Limits<T> ─────────────────────────

template <typename T>
struct Limits {
    static METAL_FUNC T finite_min();
};

template <>
struct Limits<float> {
    static METAL_FUNC float finite_min() { return -FLT_MAX; }
};

template <>
struct Limits<half> {
    static METAL_FUNC half finite_min() { return -HALF_MAX; }
};

template <>
struct Limits<bfloat> {
    // bfloat has the same exponent range as f32; -FLT_MAX casts cleanly.
    static METAL_FUNC bfloat finite_min() {
        return bfloat(-FLT_MAX);
    }
};

// ── Per-dispatch dims — declared as Metal function_constants. Matches
//    MLX's `mx.fast.metal_kernel(template=[("Dk", 128), ...])` which
//    internally uses `MTLFunctionConstantValues` and resolves the
//    pipeline via `newFunctionWithName:constantValues:error:`. The
//    Rust dispatcher sets these via `Device::pipeline_with_constants`
//    + `cv_set_int16` at the FC offset matching each id below.
//
//    id   name          used by
//    ───  ────          ───────
//     0   Dk            gated_delta / tape_replay
//     1   Dv            gated_delta / tape_replay
//     2   Hk            gated_delta / tape_replay / sdpa_partials
//     3   Hv            gated_delta / tape_replay
//     4   D             sdpa_partials
//     5   V             sdpa_partials / sdpa_reduce
//     6   M_FIXED       sdpa_partials / sdpa_reduce
constant int Dk      [[function_constant(0)]];
constant int Dv      [[function_constant(1)]];
constant int Hk      [[function_constant(2)]];
constant int Hv      [[function_constant(3)]];
constant int D       [[function_constant(4)]];
constant int V       [[function_constant(5)]];
constant int M_FIXED [[function_constant(6)]];
