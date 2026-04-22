//! Element dtypes supported by `CudaTensor`.

use half::{bf16, f16};

/// Runtime dtype tag, stored on every `CudaTensor` for shape/dispatch
/// sanity checks and for bench/trace output.
///
/// The K-quant and IQ variants (Q5K, Q6K, Q8_0, IQ4_XS) are *debug
/// tags* only: the loader dequantizes these to bf16 at load time, so
/// the backing `CudaTensor` is `bf16` even when `DType == Q5K`. The
/// tag records the original ggml format so callers can print the
/// per-tensor dtype breakdown without hiding "this came from a K-quant
/// block" behind "Bf16".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DType {
    Bf16,
    F16,
    F32,
    I8,
    I32,
    /// Q4_K_M GGUF quantized weight block (256-wide group, 144 bytes
    /// per block). Byte-addressed; unpacking lives in the mmq/mmvq
    /// kernels.
    Q4K,
    /// Q5_K GGUF quantized weight block (256-wide group, 176 bytes per
    /// block). Loaded as bf16 after CPU-side dequant — tag only.
    Q5K,
    /// Q6_K GGUF quantized weight block (256-wide group, 210 bytes per
    /// block). Loaded as bf16 after CPU-side dequant — tag only.
    Q6K,
    /// Q8_0 GGUF quantized weight block (32-wide group, 34 bytes per
    /// block). Loaded as bf16 after CPU-side dequant — tag only.
    Q8_0,
    /// IQ4_XS GGUF quantized weight block (256-wide group, 136 bytes
    /// per block). Loaded as bf16 after CPU-side dequant — tag only.
    IQ4XS,
}

impl DType {
    pub fn element_size_bytes(self) -> usize {
        match self {
            DType::Bf16 | DType::F16 => 2,
            DType::F32 | DType::I32 => 4,
            DType::I8 => 1,
            DType::Q4K | DType::Q5K | DType::Q6K | DType::Q8_0 | DType::IQ4XS => {
                panic!(
                    "{:?} has sub-byte element size; use block_bytes_for_elements",
                    self
                )
            }
        }
    }

    /// Byte count for storing `n_elements` of this dtype. Handles the
    /// packed block formats correctly.
    pub fn block_bytes_for_elements(self, n_elements: usize) -> usize {
        match self {
            DType::Q4K => {
                let blocks = n_elements.div_ceil(256);
                blocks * 144
            }
            DType::Q5K => {
                let blocks = n_elements.div_ceil(256);
                blocks * 176
            }
            DType::Q6K => {
                let blocks = n_elements.div_ceil(256);
                blocks * 210
            }
            DType::Q8_0 => {
                let blocks = n_elements.div_ceil(32);
                blocks * 34
            }
            DType::IQ4XS => {
                let blocks = n_elements.div_ceil(256);
                blocks * 136
            }
            other => n_elements * other.element_size_bytes(),
        }
    }
}

/// Compile-time binding from a Rust scalar type to its runtime
/// `DType` tag. Implemented only for the types we actually store on
/// device. We intentionally do NOT require `bytemuck::Pod` —
/// `DeviceRepr` from cudarc is the authoritative "safe to memcpy to
/// device" marker, and cudarc ships impls for the half types that
/// bytemuck doesn't cover upstream.
pub trait DTypeTrait: Sized + Copy + 'static {
    const DTYPE: DType;
}

impl DTypeTrait for bf16 {
    const DTYPE: DType = DType::Bf16;
}
impl DTypeTrait for f16 {
    const DTYPE: DType = DType::F16;
}
impl DTypeTrait for f32 {
    const DTYPE: DType = DType::F32;
}
impl DTypeTrait for i8 {
    const DTYPE: DType = DType::I8;
}
impl DTypeTrait for i32 {
    const DTYPE: DType = DType::I32;
}
