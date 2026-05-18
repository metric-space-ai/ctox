//! Qwen3.5 vision tower weight surface.
//!
//! The target checkpoint includes `vision_tower.*` tensors alongside the
//! nested `language_model.*` text tensors. This module keeps those weights
//! physically represented in the 35B Metal engine. The actual image/video
//! forward kernels are still separate work.

use crate::metal::ffi::Buffer;

pub struct VisionLinear {
    pub weight: Buffer,
    pub bias: Option<Buffer>,
}

pub struct VisionLayerNorm {
    pub weight: Buffer,
    pub bias: Buffer,
}

pub struct VisionBlock {
    pub norm1: VisionLayerNorm,
    pub qkv: VisionLinear,
    pub proj: VisionLinear,
    pub norm2: VisionLayerNorm,
    pub mlp_fc1: VisionLinear,
    pub mlp_fc2: VisionLinear,
}

pub struct VisionMerger {
    pub norm: VisionLayerNorm,
    pub linear_fc1: VisionLinear,
    pub linear_fc2: VisionLinear,
}

pub struct VisionWeights {
    pub patch_embed: VisionLinear,
    pub pos_embed: Buffer,
    pub blocks: Vec<VisionBlock>,
    pub merger: VisionMerger,
}
