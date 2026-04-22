//! Legacy cuda crate build script — now a no-op. Kernels migrated to
//! `models/qwen35_27b/kernels/sm_XX/`. The new per-model crate owns
//! nvcc compilation.

fn main() {
    // Intentionally empty. Keep the file so the crate layout matches
    // what was there before migration.
}
