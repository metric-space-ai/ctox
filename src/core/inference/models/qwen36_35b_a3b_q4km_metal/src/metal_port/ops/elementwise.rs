// Origin: CTOX
// License: AGPL-3.0-only

//! Element-wise op dispatchers — silu / sigmoid / add / mul.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:1013-1014 (FC_unary_op, FC_unary_cnt)
//! ref: vendor/ggml-metal/ggml-metal.metal:1203-1206 (FC_bin_op, FC_bin_f, FC_bin_rb, FC_bin_cb)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:99-100   (FC_UNARY=1200, FC_BIN=1300)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:124,128  (OP_UNARY_NUM_SIGMOID=102, OP_UNARY_NUM_SILU=106)
//!
//! Used by Stage-4 layer-block driver:
//! - silu, mul → SwiGLU activation in MoE FFN
//! - sigmoid, mul → attn_output_gate in full-attention block
//! - add → residual additions throughout

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;

use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBuffer, MTLCommandEncoder, MTLComputeCommandEncoder, MTLComputePipelineState, MTLDataType,
    MTLDevice, MTLFunctionConstantValues, MTLLibrary, MTLSize,
};

use crate::metal_port::runtime::MetalRuntime;

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsUnary {
    ne00: i32,
    ne01: i32,
    ne02: i32,
    ne03: i32,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne0: i32,
    ne1: i32,
    ne2: i32,
    ne3: i32,
    nb0: u64,
    nb1: u64,
    nb2: u64,
    nb3: u64,
    slope: f32,
    scale: f32,
    bias: f32,
    val: f32,
    min: f32,
    max: f32,
}
// 4×i32 + 4×u64 + 4×i32 + 4×u64 + 6×f32 = 16+32+16+32+24 = 120
const _: () = assert!(size_of::<KargsUnary>() == 120);

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsBin {
    ne00: i32,
    ne01: i32,
    ne02: i32,
    ne03: i32,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne10: i32,
    ne11: i32,
    ne12: i32,
    ne13: i32,
    nb10: u64,
    nb11: u64,
    nb12: u64,
    nb13: u64,
    ne0: i32,
    ne1: i32,
    ne2: i32,
    ne3: i32,
    nb0: u64,
    nb1: u64,
    nb2: u64,
    nb3: u64,
    offs: u64,
    o1: [u64; 8],
}
// 4×i32 + 4×u64 + 4×i32 + 4×u64 + 4×i32 + 4×u64 + u64 + 8×u64
//   = 16 + 32 + 16 + 32 + 16 + 32 + 8 + 64 = 216
const _: () = assert!(size_of::<KargsBin>() == 216);

const FC_UNARY_OP_INDEX: u64 = 1200;
const FC_UNARY_CNT_INDEX: u64 = 1201;
const FC_BIN_OP_INDEX: u64 = 1300;
const FC_BIN_F_INDEX: u64 = 1301;
const FC_BIN_RB_INDEX: u64 = 1302;
const FC_BIN_CB_INDEX: u64 = 1303;

const OP_UNARY_NUM_SIGMOID: i16 = 102;
const OP_UNARY_NUM_SILU: i16 = 106;

const OP_BIN_ADD: i16 = 0;
const OP_BIN_MUL: i16 = 2;

pub struct UnaryKernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
}

pub struct BinKernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
}

fn build_unary(rt: &MetalRuntime, op: i16) -> Result<UnaryKernel> {
    let constants = MTLFunctionConstantValues::new();
    let cnt: bool = true;
    let op_nn = NonNull::new(&op as *const i16 as *mut c_void)
        .ok_or_else(|| anyhow!("op ptr null"))?;
    let cnt_nn = NonNull::new(&cnt as *const bool as *mut c_void)
        .ok_or_else(|| anyhow!("cnt ptr null"))?;
    unsafe {
        constants.setConstantValue_type_atIndex(
            op_nn,
            MTLDataType::Short,
            FC_UNARY_OP_INDEX as usize,
        );
        constants.setConstantValue_type_atIndex(
            cnt_nn,
            MTLDataType::Bool,
            FC_UNARY_CNT_INDEX as usize,
        );
    }
    let name = NSString::from_str("kernel_unary_f32_f32");
    let func = rt
        .library
        .newFunctionWithName_constantValues_error(&name, &constants)
        .map_err(|err| anyhow!("kernel_unary_f32_f32: {err:?}"))?;
    let pso = rt
        .device
        .newComputePipelineStateWithFunction_error(&func)
        .map_err(|err| anyhow!("pso: {err:?}"))?;
    Ok(UnaryKernel { pso })
}

pub fn build_silu_kernel(rt: &MetalRuntime) -> Result<UnaryKernel> {
    build_unary(rt, OP_UNARY_NUM_SILU)
}

pub fn build_sigmoid_kernel(rt: &MetalRuntime) -> Result<UnaryKernel> {
    build_unary(rt, OP_UNARY_NUM_SIGMOID)
}

fn build_bin(rt: &MetalRuntime, op: i16) -> Result<BinKernel> {
    let constants = MTLFunctionConstantValues::new();
    let f: i16 = 1; // single src1
    let rb: bool = false;
    let cb: bool = false;
    let op_nn = NonNull::new(&op as *const i16 as *mut c_void).ok_or_else(|| anyhow!("op nn"))?;
    let f_nn = NonNull::new(&f as *const i16 as *mut c_void).ok_or_else(|| anyhow!("f nn"))?;
    let rb_nn = NonNull::new(&rb as *const bool as *mut c_void).ok_or_else(|| anyhow!("rb nn"))?;
    let cb_nn = NonNull::new(&cb as *const bool as *mut c_void).ok_or_else(|| anyhow!("cb nn"))?;
    unsafe {
        constants.setConstantValue_type_atIndex(op_nn, MTLDataType::Short, FC_BIN_OP_INDEX as usize);
        constants.setConstantValue_type_atIndex(f_nn, MTLDataType::Short, FC_BIN_F_INDEX as usize);
        constants.setConstantValue_type_atIndex(rb_nn, MTLDataType::Bool, FC_BIN_RB_INDEX as usize);
        constants.setConstantValue_type_atIndex(cb_nn, MTLDataType::Bool, FC_BIN_CB_INDEX as usize);
    }
    let name = NSString::from_str("kernel_bin_fuse_f32_f32_f32");
    let func = rt
        .library
        .newFunctionWithName_constantValues_error(&name, &constants)
        .map_err(|err| anyhow!("kernel_bin_fuse_f32_f32_f32: {err:?}"))?;
    let pso = rt
        .device
        .newComputePipelineStateWithFunction_error(&func)
        .map_err(|err| anyhow!("pso: {err:?}"))?;
    Ok(BinKernel { pso })
}

pub fn build_add_kernel(rt: &MetalRuntime) -> Result<BinKernel> {
    build_bin(rt, OP_BIN_ADD)
}
pub fn build_mul_kernel(rt: &MetalRuntime) -> Result<BinKernel> {
    build_bin(rt, OP_BIN_MUL)
}

/// Record a 1-D contiguous unary op (silu / sigmoid).
pub fn record_unary_contig_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &UnaryKernel,
    src_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    n: usize,
) -> Result<()> {
    let kargs = KargsUnary {
        ne00: n as i32,
        ne01: 1,
        ne02: 1,
        ne03: 1,
        nb00: size_of::<f32>() as u64,
        nb01: (n * size_of::<f32>()) as u64,
        nb02: (n * size_of::<f32>()) as u64,
        nb03: (n * size_of::<f32>()) as u64,
        ne0: n as i32,
        ne1: 1,
        ne2: 1,
        ne3: 1,
        nb0: size_of::<f32>() as u64,
        nb1: (n * size_of::<f32>()) as u64,
        nb2: (n * size_of::<f32>()) as u64,
        nb3: (n * size_of::<f32>()) as u64,
        slope: 0.0,
        scale: 1.0,
        bias: 0.0,
        val: 0.0,
        min: f32::NEG_INFINITY,
        max: f32::INFINITY,
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsUnary as *mut c_void)
        .ok_or_else(|| anyhow!("kargs nn"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsUnary>(), 0);
        enc.setBuffer_offset_atIndex(Some(src_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 2);
    }
    // FC_unary_cnt=true → tgpig.x is the linear element index.
    let grid = MTLSize {
        width: n,
        height: 1,
        depth: 1,
    };
    let tg = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}

/// Record a 1-D contiguous binary op (add / mul of two equal-shape tensors).
pub fn record_bin_contig_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &BinKernel,
    src0_buf: &ProtocolObject<dyn MTLBuffer>,
    src1_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    n: usize,
) -> Result<()> {
    let kargs = KargsBin {
        ne00: n as i32,
        ne01: 1,
        ne02: 1,
        ne03: 1,
        nb00: size_of::<f32>() as u64,
        nb01: (n * size_of::<f32>()) as u64,
        nb02: (n * size_of::<f32>()) as u64,
        nb03: (n * size_of::<f32>()) as u64,
        ne10: n as i32,
        ne11: 1,
        ne12: 1,
        ne13: 1,
        nb10: size_of::<f32>() as u64,
        nb11: (n * size_of::<f32>()) as u64,
        nb12: (n * size_of::<f32>()) as u64,
        nb13: (n * size_of::<f32>()) as u64,
        ne0: n as i32,
        ne1: 1,
        ne2: 1,
        ne3: 1,
        nb0: size_of::<f32>() as u64,
        nb1: (n * size_of::<f32>()) as u64,
        nb2: (n * size_of::<f32>()) as u64,
        nb3: (n * size_of::<f32>()) as u64,
        offs: 0,
        o1: [0; 8],
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsBin as *mut c_void)
        .ok_or_else(|| anyhow!("kargs nn"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsBin>(), 0);
        enc.setBuffer_offset_atIndex(Some(src0_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(src1_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 3);
    }
    // Non-broadcast contiguous: kernel splits work across threads.
    // Grid (1, ne01=1, ne02=1) × tg (n, 1, 1).
    let max_threads = 256;
    let tg_w = max_threads.min(n);
    let grid = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };
    let tg = MTLSize {
        width: tg_w,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}
