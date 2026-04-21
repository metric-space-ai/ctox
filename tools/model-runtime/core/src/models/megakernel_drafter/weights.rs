//! Qwen3.5-0.8B drafter weight loading and `LayerWeights[24]`
//! packing for the megakernel FFI.
//!
//! The megakernel reads each layer's weights through a fixed 14-
//! pointer table (`LayerWeights { int layer_type; int _pad[3];
//! void *ptrs[14]; }`). Ordering per layer_type:
//!
//! DeltaNet (`layer_type = 0`), 14 pointers used:
//!   0:  input_layernorm
//!   1:  qkv_proj            (out=DN_CONV_CHANNELS, in=HIDDEN)
//!   2:  z_proj              (out=DN_V_SIZE,        in=HIDDEN)
//!   3:  beta_proj           (out=DN_HEADS,         in=HIDDEN)
//!   4:  alpha_proj          (out=DN_HEADS,         in=HIDDEN)
//!   5:  conv1d              (DN_CONV_CHANNELS, 1, DN_CONV_KERNEL)
//!   6:  a_log               (DN_HEADS)
//!   7:  dt_bias             (DN_HEADS)
//!   8:  norm_weight         (DN_VALUE_DIM)
//!   9:  out_proj            (HIDDEN, DN_V_SIZE)
//!  10:  post_attn_layernorm (HIDDEN)
//!  11:  gate_proj           (INTERMEDIATE, HIDDEN)
//!  12:  up_proj             (INTERMEDIATE, HIDDEN)
//!  13:  down_proj           (HIDDEN, INTERMEDIATE)
//!
//! FullAttention (`layer_type = 1`), 11 pointers used (12..=13 are
//! ignored by the kernel but must still be kept as a stable layout):
//!   0:  input_layernorm
//!   1:  q_proj              (FA_QPROJ_SIZE = FA_Q + FA_GATE, HIDDEN)
//!   2:  k_proj              (FA_KV_SIZE, HIDDEN)
//!   3:  v_proj              (FA_KV_SIZE, HIDDEN)
//!   4:  q_norm              (FA_HEAD_DIM)
//!   5:  k_norm              (FA_HEAD_DIM)
//!   6:  o_proj              (HIDDEN, FA_Q_SIZE)
//!   7:  post_attn_layernorm (HIDDEN)
//!   8:  gate_proj           (INTERMEDIATE, HIDDEN)
//!   9:  up_proj             (INTERMEDIATE, HIDDEN)
//!  10:  down_proj           (HIDDEN, INTERMEDIATE)
//!  11..=13: unused (pointer must still be valid, set to any weight).
//!
//! All weights are BF16 and must be contiguous on the same CUDA
//! device as the target. This module does NOT load safetensors —
//! callers are expected to build a `VarBuilder` in BF16 + CUDA and
//! pass the resolved tensors through.

#![cfg(feature = "cuda")]

use candle_core::{DType, Device, Result, Tensor};

use super::constants::*;
use crate::cuda::dflash_megakernel::{LayerWeights, LAYER_TYPE_DELTANET, LAYER_TYPE_FULL_ATTENTION};

/// Same layer pattern as exported by the FFI module, re-exported
/// here for the public surface since downstream callers import it
/// from `megakernel_drafter::QWEN35_0_8B_LAYER_PATTERN`.
pub const QWEN35_0_8B_LAYER_PATTERN: [u8; NUM_LAYERS] = LAYER_PATTERN;

/// Per-layer BF16 weight tensors, plus the shared (non-layer)
/// embedding / final-norm / lm-head tensors. Produced by
/// [`MegakernelWeights::load_from_hub`] (when a safetensors loader
/// is wired up) or assembled by hand from a `VarBuilder` in tests.
///
/// All tensors must be BF16, contiguous, on the same CUDA device.
pub struct MegakernelWeights {
    pub device: Device,
    pub embed: Tensor,
    pub final_norm: Tensor,
    pub lm_head: Tensor,

    /// Per-layer weights, indexed 0..NUM_LAYERS. Each entry is
    /// either `LayerBundle::DeltaNet(...)` or `LayerBundle::FullAttention(...)`;
    /// the variant must match `LAYER_PATTERN[i]`.
    pub layers: Vec<LayerBundle>,

    /// Packed `LayerWeights[NUM_LAYERS]` array on device, reused as
    /// the `layer_weights` arg to every prefill/decode call.
    /// Built by [`MegakernelWeights::pack`] from `layers`.
    packed: Option<Tensor>, // U8 buffer of NUM_LAYERS * 128 bytes
}

/// One layer's BF16 weight tensor set — DeltaNet or FullAttention
/// variant. Field order matches the pointer table in the module-level
/// docs; this indirection keeps the call-site type-safe (a FA layer
/// can't accidentally omit `q_norm`).
pub enum LayerBundle {
    DeltaNet(DeltaNetLayer),
    FullAttention(FullAttentionLayer),
}

pub struct DeltaNetLayer {
    pub input_layernorm: Tensor,
    pub qkv_proj: Tensor,
    pub z_proj: Tensor,
    pub beta_proj: Tensor,
    pub alpha_proj: Tensor,
    pub conv1d: Tensor,
    pub a_log: Tensor,
    pub dt_bias: Tensor,
    pub norm: Tensor,
    pub out_proj: Tensor,
    pub post_attn_layernorm: Tensor,
    pub gate_proj: Tensor,
    pub up_proj: Tensor,
    pub down_proj: Tensor,
}

pub struct FullAttentionLayer {
    pub input_layernorm: Tensor,
    pub q_proj: Tensor,
    pub k_proj: Tensor,
    pub v_proj: Tensor,
    pub q_norm: Tensor,
    pub k_norm: Tensor,
    pub o_proj: Tensor,
    pub post_attn_layernorm: Tensor,
    pub gate_proj: Tensor,
    pub up_proj: Tensor,
    pub down_proj: Tensor,
}

impl MegakernelWeights {
    /// Construct from individual tensors. The caller has already
    /// loaded them via a BF16 VarBuilder on CUDA. No shape checks
    /// here — the kernel asserts shapes internally via the baked-in
    /// constants.
    pub fn new(
        device: Device,
        embed: Tensor,
        final_norm: Tensor,
        lm_head: Tensor,
        layers: Vec<LayerBundle>,
    ) -> Result<Self> {
        if layers.len() != NUM_LAYERS {
            candle_core::bail!(
                "MegakernelWeights::new: got {} layers, expected {NUM_LAYERS}",
                layers.len()
            );
        }
        for (i, layer) in layers.iter().enumerate() {
            let expected_fa = LAYER_PATTERN[i] == 1;
            let is_fa = matches!(layer, LayerBundle::FullAttention(_));
            if is_fa != expected_fa {
                candle_core::bail!(
                    "MegakernelWeights::new: layer {i} kind mismatches \
                     LAYER_PATTERN (expected FA={expected_fa}, got FA={is_fa})"
                );
            }
        }
        Ok(Self {
            device,
            embed,
            final_norm,
            lm_head,
            layers,
            packed: None,
        })
    }

    /// Pack `layers` into a device-resident `LayerWeights[NUM_LAYERS]`
    /// array. Called lazily on first FFI dispatch; cached in
    /// `self.packed`. Must be re-called if any tensor is replaced.
    pub fn pack(&mut self) -> Result<&Tensor> {
        if self.packed.is_some() {
            return Ok(self.packed.as_ref().unwrap());
        }

        // Each LayerWeights entry is 128 bytes (16 B header + 14 × 8 B pointers).
        const STRUCT_SIZE: usize = 16 + 14 * 8;
        let mut buf = vec![0_u8; NUM_LAYERS * STRUCT_SIZE];

        for (i, layer) in self.layers.iter().enumerate() {
            let offset = i * STRUCT_SIZE;
            let (layer_type, ptrs) = layer_ptrs(layer)?;

            // Header: int layer_type; int _pad[3] — 16 bytes total.
            buf[offset..offset + 4].copy_from_slice(&layer_type.to_le_bytes());
            // _pad[3] left zero.

            for (j, &ptr) in ptrs.iter().enumerate() {
                let pos = offset + 16 + j * 8;
                buf[pos..pos + 8].copy_from_slice(&ptr.to_le_bytes());
            }
            // Remaining slots in the 14-pointer array left zero for
            // FA layers (kernel reads ptrs[11..=13] only for DN).
        }

        // Upload to device as a u8 tensor.
        let packed = Tensor::from_vec(buf, (NUM_LAYERS * STRUCT_SIZE,), &self.device)?;
        self.packed = Some(packed);
        Ok(self.packed.as_ref().unwrap())
    }

    /// Return a raw `*const LayerWeights` pointer to the packed
    /// array. Only valid after [`Self::pack`] has been called.
    pub fn packed_ptr(&self) -> Option<*const LayerWeights> {
        self.packed
            .as_ref()
            .map(|t| raw_u8_addr(t) as *const LayerWeights)
    }

    /// Device pointer for the embed table (BF16).
    pub fn embed_ptr(&self) -> *const std::ffi::c_void {
        raw_bf16_addr(&self.embed) as *const _
    }
    pub fn final_norm_ptr(&self) -> *const std::ffi::c_void {
        raw_bf16_addr(&self.final_norm) as *const _
    }
    pub fn lm_head_ptr(&self) -> *const std::ffi::c_void {
        raw_bf16_addr(&self.lm_head) as *const _
    }
}

/// Extract the ordered ptr table for one layer. Pointers are raw
/// device addresses — the caller must keep the source tensors alive
/// for at least as long as any pending kernel launch that consumes
/// the packed buffer.
fn layer_ptrs(layer: &LayerBundle) -> Result<(i32, Vec<u64>)> {
    match layer {
        LayerBundle::DeltaNet(dn) => {
            let ptrs = vec![
                device_ptr(&dn.input_layernorm)?,
                device_ptr(&dn.qkv_proj)?,
                device_ptr(&dn.z_proj)?,
                device_ptr(&dn.beta_proj)?,
                device_ptr(&dn.alpha_proj)?,
                device_ptr(&dn.conv1d)?,
                device_ptr(&dn.a_log)?,
                device_ptr(&dn.dt_bias)?,
                device_ptr(&dn.norm)?,
                device_ptr(&dn.out_proj)?,
                device_ptr(&dn.post_attn_layernorm)?,
                device_ptr(&dn.gate_proj)?,
                device_ptr(&dn.up_proj)?,
                device_ptr(&dn.down_proj)?,
            ];
            Ok((LAYER_TYPE_DELTANET, ptrs))
        }
        LayerBundle::FullAttention(fa) => {
            let ptrs = vec![
                device_ptr(&fa.input_layernorm)?,
                device_ptr(&fa.q_proj)?,
                device_ptr(&fa.k_proj)?,
                device_ptr(&fa.v_proj)?,
                device_ptr(&fa.q_norm)?,
                device_ptr(&fa.k_norm)?,
                device_ptr(&fa.o_proj)?,
                device_ptr(&fa.post_attn_layernorm)?,
                device_ptr(&fa.gate_proj)?,
                device_ptr(&fa.up_proj)?,
                device_ptr(&fa.down_proj)?,
            ];
            Ok((LAYER_TYPE_FULL_ATTENTION, ptrs))
        }
    }
}

fn device_ptr(t: &Tensor) -> Result<u64> {
    use candle_core::cuda_backend::cudarc::driver::DevicePtr;
    if t.dtype() != DType::BF16 {
        candle_core::bail!(
            "MegakernelWeights: tensor dtype {:?}, expected BF16",
            t.dtype()
        );
    }
    if !t.is_contiguous() {
        candle_core::bail!(
            "MegakernelWeights: tensor must be contiguous (shape {:?})",
            t.dims()
        );
    }
    let (storage, layout) = t.storage_and_layout();
    match &*storage {
        candle_core::Storage::Cuda(c) => {
            let s = c.as_cuda_slice::<half::bf16>()?;
            let (addr, _g) = s.slice(layout.start_offset()..).device_ptr(s.stream());
            Ok(addr)
        }
        _ => candle_core::bail!("MegakernelWeights: tensor must live on CUDA"),
    }
}

fn raw_bf16_addr(t: &Tensor) -> u64 {
    device_ptr(t).expect("raw_bf16_addr: extract device ptr")
}

fn raw_u8_addr(t: &Tensor) -> u64 {
    use candle_core::cuda_backend::cudarc::driver::DevicePtr;
    let (storage, layout) = t.storage_and_layout();
    match &*storage {
        candle_core::Storage::Cuda(c) => {
            let s = c.as_cuda_slice::<u8>().expect("u8 CudaSlice");
            let (addr, _g) = s.slice(layout.start_offset()..).device_ptr(s.stream());
            addr
        }
        _ => panic!("raw_u8_addr: non-CUDA tensor"),
    }
}
