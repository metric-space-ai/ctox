//! Load-only smoke for the Qwen3.5-35B-A3B DFlash draft.
//!
//! This intentionally does not load the 35B target model or run a forward
//! pass. It verifies that the BF16 draft safetensors can be parsed and
//! uploaded with the 35B-specific layer count and fc shape.

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use std::path::PathBuf;

    use anyhow::{anyhow, Context};
    use ctox_qwen35_35b_a3b_dflash::common::constants::{
        DFLASH35B_DRAFT_LAYERS, DFLASH35B_DRAFT_N_TARGET_LAYERS, DFLASH35B_TARGET_HIDDEN,
    };
    use ctox_qwen35_35b_a3b_dflash::metal::ffi::global_device;
    use ctox_qwen35_35b_a3b_dflash::metal::loader::load_draft_safetensors;

    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| {
            anyhow!("usage: cargo run --example load_draft_metal -- <draft.safetensors>")
        })?;

    let dev = global_device().ok_or_else(|| anyhow!("failed to acquire default Metal device"))?;
    let draft = load_draft_safetensors(dev, &path)
        .with_context(|| format!("load_draft_safetensors({})", path.display()))?;

    let expected_fc_in = DFLASH35B_TARGET_HIDDEN * DFLASH35B_DRAFT_N_TARGET_LAYERS;
    anyhow::ensure!(
        draft.layers.len() == DFLASH35B_DRAFT_LAYERS as usize,
        "loaded {} draft layers, expected {}",
        draft.layers.len(),
        DFLASH35B_DRAFT_LAYERS
    );
    anyhow::ensure!(
        draft.fc.in_features == expected_fc_in && draft.fc.out_features == DFLASH35B_TARGET_HIDDEN,
        "loaded fc [{}, {}], expected [{}, {}]",
        draft.fc.out_features,
        draft.fc.in_features,
        DFLASH35B_TARGET_HIDDEN,
        expected_fc_in
    );

    println!(
        "draft loaded: layers={} fc=[{}, {}]",
        draft.layers.len(),
        draft.fc.out_features,
        draft.fc.in_features
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("load_draft_metal: only runs on macOS + Apple Silicon.");
    std::process::exit(2);
}
