//! CUDA backend staging for Qwen3-Embedding-0.6B.
//!
//! The selected kernel seed is vendored under
//! `vendor/cuda/kernels/ctox_qwen3_embedding_glue.cu`. It is not linked yet;
//! the next implementation slice ports the CUDA FFI/launch wrappers from the
//! existing Qwen3.5 backend and drops generation-only kernels.

use std::ffi::c_void;

pub const KERNEL_MANIFEST: &[&str] = &[
    "token_embedding/get_rows",
    "rms_norm",
    "rope",
    "q4_k_or_bf16_matmul",
    "sdpa_or_attention",
    "silu",
    "last_token_pool",
    "l2_normalize",
];

pub const CUDA_ARCHIVE: Option<&str> = option_env!("CTOX_QWEN3_EMBEDDING_CUDA_ARCHIVE");

pub type CudaStream = *mut c_void;

extern "C" {
    pub fn ctox_qwen3_embedding_last_token_pool_bf16_launch(
        hidden: *const c_void,
        out: *mut f32,
        batch: i32,
        seq_len: i32,
        dim: i32,
        stream: CudaStream,
    ) -> i32;

    pub fn ctox_qwen3_embedding_l2_normalize_f32_launch(
        vectors: *mut f32,
        batch: i32,
        dim: i32,
        stream: CudaStream,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_cuda_archive_when_build_script_compiles_it() {
        #[cfg(target_os = "linux")]
        {
            let _ = super::CUDA_ARCHIVE;
        }
    }
}
