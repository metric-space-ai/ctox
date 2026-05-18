//! Driver glue — thin layer over [`runtime::DFlashRuntime`] that
//! matches the CUDA side's `run_dflash_gen_loop` signature so both
//! backends can be driven from a common bench / CLI.
//!
//! ref: `dflash_mlx/generate.py` (CLI + outer loop)
//!     + `cuda::driver::run_dflash_gen_loop` (CUDA analog)

use anyhow::Result;

use crate::metal::ffi::Device;
use crate::metal::model::{DraftWeights, TargetWeights};
use crate::metal::runtime::{DFlashRuntime, GenConfig, RunStats};

/// Same signature as `cuda::driver::run_dflash_gen_loop` modulo
/// ggml-specific args. Returns the final `RunStats` and writes every
/// generated token (plus the prompt prefix) into `out`.
pub fn run_dflash_gen_loop(
    dev: &Device,
    target: TargetWeights,
    draft: DraftWeights,
    prompt_ids: &[i32],
    n_gen: i32,
    out: &mut Vec<i32>,
    cfg: GenConfig,
) -> Result<RunStats> {
    let mut rt = DFlashRuntime::new(dev, target, draft, cfg)?;
    rt.generate(dev, prompt_ids, n_gen, out)
}
