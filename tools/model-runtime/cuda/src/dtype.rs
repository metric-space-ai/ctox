//! Element dtypes supported by `CudaTensor`.

use half::{bf16, f16};

/// Runtime dtype tag, stored on every `CudaTensor` for shape/dispatch
/// sanity checks and for bench/trace output.
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
}

impl DType {
    pub fn element_size_bytes(self) -> usize {
        match self {
            DType::Bf16 | DType::F16 => 2,
            DType::F32 | DType::I32 => 4,
            DType::I8 => 1,
            DType::Q4K => {
                panic!("Q4K has sub-byte element size; use block_bytes_for_elements")
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
