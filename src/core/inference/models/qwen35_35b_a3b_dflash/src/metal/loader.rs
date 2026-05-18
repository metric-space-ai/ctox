//! Weight loaders for the Metal backend.
//!
//! Two formats:
//!
//!   * **Target**: `mlx-community/Qwen3.5-35B-A3B-4bit` ships as
//!     `model-*-of-*.safetensors` shards plus a `config.json`. The
//!     groupwise 4-bit quantization convention is MLX':
//!       * for a `Linear(out_features=O, in_features=I)` layer,
//!         the checkpoint stores three tensors with the shared prefix:
//!         `{prefix}.weight`        dtype uint32, shape [O, I/8]
//!         `{prefix}.scales`        dtype bf16,   shape [O, I/GS]
//!         `{prefix}.biases`        dtype bf16,   shape [O, I/GS]
//!       * `GS` (group-size) defaults to 64, pinned in the config as
//!         `"quantization.group_size"`.
//!
//!   * **Draft**: the z-lab DFlash draft (`z-lab/Qwen3.5-35B-A3B-DFlash`)
//!     ships as a flat bf16 safetensors file — *not* MLX-4bit. We
//!     load those tensors directly into `bfloat16` buffers.
//!
//! # Status
//!
//! Concrete parser for both formats. Returns structured
//! [`TargetWeights`] / [`DraftWeights`] populated with `Buffer`
//! handles on the GPU. Config parsing covers the fields the Qwen3.5
//! module tree reads; unknown fields are ignored.
//!
//! ref:
//!   - `dflash_mlx/runtime.py:680-778`         (load_target_bundle / load_draft_bundle)
//!   - `mlx_lm/utils.py::load_model`           (HF-style safetensors sharding)
//!   - `mlx/backend/common/quantized.cpp`      (group-wise 4-bit decode)

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use memmap2::Mmap;
use safetensors::SafeTensors;

use crate::common::constants::{
    DFLASH35B_DRAFT_LAYERS, DFLASH35B_DRAFT_N_TARGET_LAYERS, DFLASH35B_TARGET_HIDDEN,
};
use crate::metal::ffi::{Buffer, Device};
use crate::metal::moe::{ExpertLinear4Bit, ExpertSet, MoeBlock};
use crate::metal::vision::{
    VisionBlock, VisionLayerNorm, VisionLinear, VisionMerger, VisionWeights,
};

/// Subset of the HF `config.json` fields we read. Unknown fields are
/// ignored by serde's default behaviour.
#[derive(Debug)]
pub struct HfConfig {
    pub hidden_size: i32,
    pub num_hidden_layers: i32,
    pub num_attention_heads: i32,
    pub num_key_value_heads: i32,
    pub head_dim: i32,
    pub moe_intermediate_size: i32,
    pub shared_expert_intermediate_size: i32,
    pub num_experts: i32,
    pub num_experts_per_tok: i32,
    pub vocab_size: i32,
    pub rms_norm_eps: f32,
    pub rope_theta: f32,
    pub partial_rotary_factor: f32,
    pub quantization: Option<QuantConfig>,
    pub full_attention_interval: i32,
    pub layer_types: Vec<String>,
    pub linear_conv_kernel_dim: i32,
    pub linear_key_head_dim: i32,
    pub linear_value_head_dim: i32,
    pub linear_num_key_heads: i32,
    pub linear_num_value_heads: i32,
}

#[derive(Debug, serde::Deserialize)]
pub struct QuantConfig {
    pub group_size: i32,
    pub bits: i32,
}

#[derive(Debug, Default, serde::Deserialize)]
struct HfConfigFields {
    hidden_size: Option<i32>,
    num_hidden_layers: Option<i32>,
    num_attention_heads: Option<i32>,
    num_key_value_heads: Option<i32>,
    head_dim: Option<i32>,
    intermediate_size: Option<i32>,
    moe_intermediate_size: Option<i32>,
    shared_expert_intermediate_size: Option<i32>,
    num_experts: Option<i32>,
    num_experts_per_tok: Option<i32>,
    vocab_size: Option<i32>,
    rms_norm_eps: Option<f32>,
    rope_theta: Option<f32>,
    full_attention_interval: Option<i32>,
    #[serde(default)]
    layer_types: Vec<String>,
    linear_conv_kernel_dim: Option<i32>,
    linear_key_head_dim: Option<i32>,
    linear_value_head_dim: Option<i32>,
    linear_num_key_heads: Option<i32>,
    linear_num_value_heads: Option<i32>,
    rope_parameters: Option<RopeParameters>,
}

#[derive(Debug, serde::Deserialize)]
struct RopeParameters {
    rope_theta: Option<f32>,
    partial_rotary_factor: Option<f32>,
    #[serde(default)]
    mrope_section: Vec<i32>,
}

#[derive(Debug, serde::Deserialize)]
struct RawHfConfig {
    #[serde(flatten)]
    root: HfConfigFields,
    #[serde(default)]
    text_config: Option<HfConfigFields>,
    #[serde(default)]
    quantization: Option<QuantConfig>,
}

fn req_i32(v: Option<i32>, name: &str) -> Result<i32> {
    v.ok_or_else(|| anyhow!("config.json missing {name}"))
}

/// Read + parse `config.json` from `dir`.
pub fn read_hf_config(dir: &Path) -> Result<HfConfig> {
    let p = dir.join("config.json");
    let bytes = fs::read(&p).with_context(|| format!("read {}", p.display()))?;
    let raw: RawHfConfig =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", p.display()))?;
    let cfg_src = raw.text_config.as_ref().unwrap_or(&raw.root);
    let hidden_size = req_i32(cfg_src.hidden_size, "text_config.hidden_size")?;
    let head_dim = cfg_src
        .head_dim
        .unwrap_or(hidden_size / req_i32(cfg_src.num_attention_heads, "num_attention_heads")?);
    let moe_intermediate_size = cfg_src
        .moe_intermediate_size
        .or(cfg_src.intermediate_size)
        .ok_or_else(|| anyhow!("config.json missing moe_intermediate_size/intermediate_size"))?;
    let rope_theta = cfg_src
        .rope_theta
        .or_else(|| cfg_src.rope_parameters.as_ref().and_then(|p| p.rope_theta))
        .unwrap_or(10_000_000.0);
    let partial_rotary_factor = cfg_src
        .rope_parameters
        .as_ref()
        .and_then(|p| p.partial_rotary_factor)
        .unwrap_or(1.0);
    let linear_value_head_dim = cfg_src.linear_value_head_dim.unwrap_or(128);
    let linear_num_value_heads = cfg_src.linear_num_value_heads.unwrap_or(32);
    let cfg = HfConfig {
        hidden_size,
        num_hidden_layers: req_i32(cfg_src.num_hidden_layers, "num_hidden_layers")?,
        num_attention_heads: req_i32(cfg_src.num_attention_heads, "num_attention_heads")?,
        num_key_value_heads: req_i32(cfg_src.num_key_value_heads, "num_key_value_heads")?,
        head_dim,
        moe_intermediate_size,
        shared_expert_intermediate_size: cfg_src
            .shared_expert_intermediate_size
            .unwrap_or(moe_intermediate_size),
        num_experts: req_i32(cfg_src.num_experts, "num_experts")?,
        num_experts_per_tok: req_i32(cfg_src.num_experts_per_tok, "num_experts_per_tok")?,
        vocab_size: req_i32(cfg_src.vocab_size, "vocab_size")?,
        rms_norm_eps: cfg_src.rms_norm_eps.unwrap_or(1e-6),
        rope_theta,
        partial_rotary_factor,
        quantization: raw.quantization,
        full_attention_interval: cfg_src.full_attention_interval.unwrap_or(4),
        layer_types: cfg_src.layer_types.clone(),
        linear_conv_kernel_dim: cfg_src.linear_conv_kernel_dim.unwrap_or(4),
        linear_key_head_dim: cfg_src.linear_key_head_dim.unwrap_or(128),
        linear_value_head_dim,
        linear_num_key_heads: cfg_src.linear_num_key_heads.unwrap_or(16),
        linear_num_value_heads,
    };
    Ok(cfg)
}

/// Enumerate safetensors shards under `dir`. Uses the HF naming
/// convention `model-*.safetensors` and returns shards sorted by name
/// so tensor ordering is deterministic.
pub fn safetensors_shards(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut shards: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("read_dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("safetensors")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains("model"))
                    .unwrap_or(false)
        })
        .collect();
    shards.sort();
    if shards.is_empty() {
        return Err(anyhow!(
            "no model*.safetensors files found under {}",
            dir.display()
        ));
    }
    Ok(shards)
}

/// Tensor entry — dtype + byte-range into a memory-mapped safetensors
/// shard, waiting to be uploaded to a `Buffer`.
pub struct TensorEntry {
    pub dtype: safetensors::Dtype,
    pub shape: Vec<usize>,
    storage: Arc<Mmap>,
    offset: usize,
    len: usize,
}

impl TensorEntry {
    fn from_view(storage: &Arc<Mmap>, view: safetensors::tensor::TensorView<'_>) -> Result<Self> {
        let data = view.data();
        let base = storage.as_ptr() as usize;
        let ptr = data.as_ptr() as usize;
        let offset = ptr
            .checked_sub(base)
            .ok_or_else(|| anyhow!("safetensors view does not point into mapped shard"))?;
        let end = offset
            .checked_add(data.len())
            .ok_or_else(|| anyhow!("safetensors view byte range overflows"))?;
        if end > storage.len() {
            return Err(anyhow!(
                "safetensors view byte range [{offset}, {end}) exceeds mapped shard size {}",
                storage.len()
            ));
        }
        Ok(Self {
            dtype: view.dtype(),
            shape: view.shape().to_vec(),
            storage: Arc::clone(storage),
            offset,
            len: data.len(),
        })
    }

    fn bytes(&self) -> &[u8] {
        &self.storage[self.offset..self.offset + self.len]
    }
}

/// Read every tensor from every shard, keyed by fully-qualified name
/// (`model.layers.0.self_attn.q_proj.weight` and so on). Collects into
/// a BTreeMap so the iteration order is deterministic.
pub fn collect_tensors(shards: &[PathBuf]) -> Result<BTreeMap<String, TensorEntry>> {
    let mut out: BTreeMap<String, TensorEntry> = BTreeMap::new();
    for shard in shards {
        let file = File::open(shard).with_context(|| format!("open {}", shard.display()))?;
        let mapped =
            unsafe { Mmap::map(&file) }.with_context(|| format!("mmap {}", shard.display()))?;
        let mmap = Arc::new(mapped);
        let st = SafeTensors::deserialize(&mmap)
            .with_context(|| format!("deserialize {}", shard.display()))?;
        for (name, view) in st.tensors() {
            out.insert(name.to_string(), TensorEntry::from_view(&mmap, view)?);
        }
    }
    Ok(out)
}

/// Upload a tensor entry into a shared MTLBuffer.
pub fn upload_tensor(dev: &Device, entry: &TensorEntry) -> Option<Buffer> {
    dev.new_buffer_from_slice(entry.bytes())
}

/// Convenience: take three tensor entries that together describe an
/// MLX-4bit linear layer, upload them, and bundle into a
/// [`super::qwen::Linear4Bit`].
pub fn upload_linear_4bit(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
) -> Result<super::qwen::Linear4Bit> {
    let w = tensors
        .get(&format!("{prefix}.weight"))
        .ok_or_else(|| anyhow!("missing {prefix}.weight"))?;
    let s = tensors
        .get(&format!("{prefix}.scales"))
        .ok_or_else(|| anyhow!("missing {prefix}.scales"))?;
    let b = tensors
        .get(&format!("{prefix}.biases"))
        .ok_or_else(|| anyhow!("missing {prefix}.biases"))?;
    if w.shape.len() != 2 || s.shape.len() != 2 || b.shape.len() != 2 {
        return Err(anyhow!(
            "{prefix}: expected 2-D (weight,scales,biases) triple, got \
             weight={:?} scales={:?} biases={:?}",
            w.shape,
            s.shape,
            b.shape,
        ));
    }
    let out_features = w.shape[0] as i32;
    let in_packed = w.shape[1] as i32;
    let in_features = in_packed * 8; // 8 nibbles per uint32

    let w_buf = upload_tensor(dev, w)
        .ok_or_else(|| anyhow!("upload {prefix}.weight: buffer alloc failed"))?;
    let s_buf = upload_tensor(dev, s)
        .ok_or_else(|| anyhow!("upload {prefix}.scales: buffer alloc failed"))?;
    let b_buf = upload_tensor(dev, b)
        .ok_or_else(|| anyhow!("upload {prefix}.biases: buffer alloc failed"))?;

    Ok(super::qwen::Linear4Bit {
        w_q: w_buf,
        scales: s_buf,
        biases: b_buf,
        in_features,
        out_features,
    })
}

pub fn upload_bf16_linear(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
) -> Result<super::qwen::Bf16Linear> {
    let w = tensors
        .get(&format!("{prefix}.weight"))
        .ok_or_else(|| anyhow!("missing {prefix}.weight"))?;
    if w.shape.len() != 2 {
        return Err(anyhow!("{prefix}.weight: expected 2-D, got {:?}", w.shape));
    }
    let out_features = w.shape[0] as i32;
    let in_features = w.shape[1] as i32;
    let bias = if tensors.contains_key(&format!("{prefix}.bias")) {
        Some(upload_raw(dev, tensors, &format!("{prefix}.bias"))?)
    } else {
        None
    };
    Ok(super::qwen::Bf16Linear {
        weight: upload_tensor(dev, w)
            .ok_or_else(|| anyhow!("upload {prefix}.weight: buffer alloc failed"))?,
        bias,
        in_features,
        out_features,
    })
}

pub fn upload_expert_linear_4bit(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
) -> Result<ExpertLinear4Bit> {
    let w = tensors
        .get(&format!("{prefix}.weight"))
        .ok_or_else(|| anyhow!("missing {prefix}.weight"))?;
    let s = tensors
        .get(&format!("{prefix}.scales"))
        .ok_or_else(|| anyhow!("missing {prefix}.scales"))?;
    let b = tensors
        .get(&format!("{prefix}.biases"))
        .ok_or_else(|| anyhow!("missing {prefix}.biases"))?;
    if w.shape.len() != 3 || s.shape.len() != 3 || b.shape.len() != 3 {
        return Err(anyhow!(
            "{prefix}: expected 3-D expert (weight,scales,biases), got \
             weight={:?} scales={:?} biases={:?}",
            w.shape,
            s.shape,
            b.shape,
        ));
    }
    let num_experts = w.shape[0] as i32;
    let out_features = w.shape[1] as i32;
    let in_features = (w.shape[2] as i32) * 8;
    Ok(ExpertLinear4Bit {
        w_q: upload_tensor(dev, w)
            .ok_or_else(|| anyhow!("upload {prefix}.weight: buffer alloc failed"))?,
        scales: upload_tensor(dev, s)
            .ok_or_else(|| anyhow!("upload {prefix}.scales: buffer alloc failed"))?,
        biases: upload_tensor(dev, b)
            .ok_or_else(|| anyhow!("upload {prefix}.biases: buffer alloc failed"))?,
        num_experts,
        in_features,
        out_features,
    })
}

fn build_moe_block(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    base: &str,
    cfg: &HfConfig,
) -> Result<MoeBlock> {
    use super::qwen::Mlp;

    let router = upload_linear_4bit(dev, tensors, &format!("{base}.mlp.gate"))?;
    let experts = ExpertSet {
        gate: upload_expert_linear_4bit(dev, tensors, &format!("{base}.mlp.switch_mlp.gate_proj"))?,
        up: upload_expert_linear_4bit(dev, tensors, &format!("{base}.mlp.switch_mlp.up_proj"))?,
        down: upload_expert_linear_4bit(dev, tensors, &format!("{base}.mlp.switch_mlp.down_proj"))?,
    };
    let shared_expert = Mlp {
        gate: upload_linear_4bit(dev, tensors, &format!("{base}.mlp.shared_expert.gate_proj"))?,
        up: upload_linear_4bit(dev, tensors, &format!("{base}.mlp.shared_expert.up_proj"))?,
        down: upload_linear_4bit(dev, tensors, &format!("{base}.mlp.shared_expert.down_proj"))?,
        intermediate: cfg.shared_expert_intermediate_size,
    };
    Ok(MoeBlock {
        router,
        experts,
        shared_expert,
        shared_expert_gate: upload_linear_4bit(
            dev,
            tensors,
            &format!("{base}.mlp.shared_expert_gate"),
        )?,
        hidden: cfg.hidden_size,
        num_experts: cfg.num_experts,
        experts_per_tok: cfg.num_experts_per_tok,
        moe_intermediate: cfg.moe_intermediate_size,
    })
}

/// Same for RMSNorm — single `{prefix}.weight` tensor in bf16.
pub fn upload_rms_norm(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
    eps: f32,
) -> Result<super::qwen::RmsNorm> {
    upload_rms_norm_with_bias(dev, tensors, prefix, eps, 0.0)
}

fn upload_rms_norm_with_bias(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
    eps: f32,
    weight_bias: f32,
) -> Result<super::qwen::RmsNorm> {
    let w = tensors
        .get(&format!("{prefix}.weight"))
        .ok_or_else(|| anyhow!("missing {prefix}.weight"))?;
    if w.shape.len() != 1 {
        return Err(anyhow!("{prefix}.weight: expected 1-D, got {:?}", w.shape));
    }
    let d = w.shape[0] as i32;
    let buf = upload_tensor(dev, w)
        .ok_or_else(|| anyhow!("upload {prefix}.weight: buffer alloc failed"))?;
    Ok(super::qwen::RmsNorm {
        weight: buf,
        d,
        eps,
        weight_bias,
    })
}

fn upload_target_rms_norm(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
    eps: f32,
) -> Result<super::qwen::RmsNorm> {
    // The MLX 4-bit A3B target stores RMSNorm weights around 1.0
    // directly. Do not apply the zero-initialized "(weight + 1)"
    // convention used by some other Qwen variants.
    upload_rms_norm_with_bias(dev, tensors, prefix, eps, 0.0)
}

// ─── Top-level loaders ──────────────────────────────────────────────
//
// These walk the tensor map and drive the per-submodule uploads. The
// actual HF tensor-path conventions for Qwen3.5 (
// `model.layers.<i>.self_attn.q_proj.weight` etc.) are encoded here.
// Once `verify_linear` + `runtime` are ported, these will be called
// from the driver at startup.

/// Upload a raw bf16 tensor to a buffer. Used for tensors that don't
/// fit the MLX-4bit triple (RoPE cos/sin tables, ssm_a/dt_bias,
/// ssm_conv1d kernel, etc.).
pub fn upload_raw(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    path: &str,
) -> Result<Buffer> {
    let t = tensors
        .get(path)
        .ok_or_else(|| anyhow!("missing tensor {path}"))?;
    upload_tensor(dev, t).ok_or_else(|| anyhow!("upload {path}: alloc failed"))
}

pub fn upload_raw_optional(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    path: &str,
) -> Result<Option<Buffer>> {
    if tensors.contains_key(path) {
        Ok(Some(upload_raw(dev, tensors, path)?))
    } else {
        Ok(None)
    }
}

fn upload_vision_linear(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
) -> Result<VisionLinear> {
    Ok(VisionLinear {
        weight: upload_raw(dev, tensors, &format!("{prefix}.weight"))?,
        bias: upload_raw_optional(dev, tensors, &format!("{prefix}.bias"))?,
    })
}

fn upload_vision_norm(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    prefix: &str,
) -> Result<VisionLayerNorm> {
    Ok(VisionLayerNorm {
        weight: upload_raw(dev, tensors, &format!("{prefix}.weight"))?,
        bias: upload_raw(dev, tensors, &format!("{prefix}.bias"))?,
    })
}

fn load_vision_weights(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
) -> Result<Option<VisionWeights>> {
    if !tensors.contains_key("vision_tower.patch_embed.proj.weight") {
        return Ok(None);
    }

    let mut blocks = Vec::new();
    for i in 0.. {
        let base = format!("vision_tower.blocks.{i}");
        if !tensors.contains_key(&format!("{base}.norm1.weight")) {
            break;
        }
        blocks.push(VisionBlock {
            norm1: upload_vision_norm(dev, tensors, &format!("{base}.norm1"))?,
            qkv: upload_vision_linear(dev, tensors, &format!("{base}.attn.qkv"))?,
            proj: upload_vision_linear(dev, tensors, &format!("{base}.attn.proj"))?,
            norm2: upload_vision_norm(dev, tensors, &format!("{base}.norm2"))?,
            mlp_fc1: upload_vision_linear(dev, tensors, &format!("{base}.mlp.linear_fc1"))?,
            mlp_fc2: upload_vision_linear(dev, tensors, &format!("{base}.mlp.linear_fc2"))?,
        });
    }

    Ok(Some(VisionWeights {
        patch_embed: upload_vision_linear(dev, tensors, "vision_tower.patch_embed.proj")?,
        pos_embed: upload_raw(dev, tensors, "vision_tower.pos_embed.weight")?,
        blocks,
        merger: VisionMerger {
            norm: upload_vision_norm(dev, tensors, "vision_tower.merger.norm")?,
            linear_fc1: upload_vision_linear(dev, tensors, "vision_tower.merger.linear_fc1")?,
            linear_fc2: upload_vision_linear(dev, tensors, "vision_tower.merger.linear_fc2")?,
        },
    }))
}

/// Build one full-attention target layer from the tensor map.
fn build_target_full_attn_layer(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    base: &str,
    cfg: &HfConfig,
) -> Result<super::model::TargetLayer> {
    use super::qwen::{Attention, Rope};

    let head_dim = cfg.head_dim;
    let rope_dim = ((head_dim as f32) * cfg.partial_rotary_factor).round() as i32;
    let attention = Attention {
        wq: upload_linear_4bit(dev, tensors, &format!("{base}.self_attn.q_proj"))?,
        wk: upload_linear_4bit(dev, tensors, &format!("{base}.self_attn.k_proj"))?,
        wv: upload_linear_4bit(dev, tensors, &format!("{base}.self_attn.v_proj"))?,
        wo: upload_linear_4bit(dev, tensors, &format!("{base}.self_attn.o_proj"))?,
        q_norm: tensors
            .get(&format!("{base}.self_attn.q_norm.weight"))
            .map(|_| {
                upload_target_rms_norm(
                    dev,
                    tensors,
                    &format!("{base}.self_attn.q_norm"),
                    cfg.rms_norm_eps,
                )
            })
            .transpose()?,
        k_norm: tensors
            .get(&format!("{base}.self_attn.k_norm.weight"))
            .map(|_| {
                upload_target_rms_norm(
                    dev,
                    tensors,
                    &format!("{base}.self_attn.k_norm"),
                    cfg.rms_norm_eps,
                )
            })
            .transpose()?,
        rope: Rope {
            head_dim,
            rope_dim,
            base: cfg.rope_theta,
        },
        n_heads: cfg.num_attention_heads,
        n_kv_heads: cfg.num_key_value_heads,
        head_dim,
    };

    let mlp = build_moe_block(dev, tensors, base, cfg)?;

    Ok(super::model::TargetLayer::FullAttention {
        attn_norm: upload_target_rms_norm(
            dev,
            tensors,
            &format!("{base}.input_layernorm"),
            cfg.rms_norm_eps,
        )?,
        attn_post_norm: upload_target_rms_norm(
            dev,
            tensors,
            &format!("{base}.post_attention_layernorm"),
            cfg.rms_norm_eps,
        )?,
        ffn_norm: upload_target_rms_norm(
            dev,
            tensors,
            &format!("{base}.post_attention_layernorm"),
            cfg.rms_norm_eps,
        )?,
        attention,
        mlp,
    })
}

/// Build one GatedDeltaNet target layer. MLX exports the GDN block
/// under `linear_attn.*` with fused QKV / gate / conv / beta / alpha
/// / a / dt_bias / norm / out tensors.
fn build_target_gdn_layer(
    dev: &Device,
    tensors: &BTreeMap<String, TensorEntry>,
    base: &str,
    cfg: &HfConfig,
) -> Result<super::model::TargetLayer> {
    use super::qwen::GatedDeltaNet;

    let delta = GatedDeltaNet {
        wqkv: upload_linear_4bit(dev, tensors, &format!("{base}.linear_attn.in_proj_qkv"))?,
        wqkv_gate: upload_linear_4bit(dev, tensors, &format!("{base}.linear_attn.in_proj_z"))?,
        ssm_conv_weight: upload_raw(dev, tensors, &format!("{base}.linear_attn.conv1d.weight"))?,
        ssm_conv_bias: tensors
            .get(&format!("{base}.linear_attn.conv1d.bias"))
            .and_then(|_| {
                upload_raw(dev, tensors, &format!("{base}.linear_attn.conv1d.bias")).ok()
            }),
        ssm_beta: upload_linear_4bit(dev, tensors, &format!("{base}.linear_attn.in_proj_b"))?,
        ssm_alpha: upload_linear_4bit(dev, tensors, &format!("{base}.linear_attn.in_proj_a"))?,
        ssm_a: upload_raw(dev, tensors, &format!("{base}.linear_attn.A_log"))?,
        ssm_dt_bias: upload_raw(dev, tensors, &format!("{base}.linear_attn.dt_bias"))?,
        ssm_norm: upload_target_rms_norm(
            dev,
            tensors,
            &format!("{base}.linear_attn.norm"),
            cfg.rms_norm_eps,
        )?,
        ssm_out: upload_linear_4bit(dev, tensors, &format!("{base}.linear_attn.out_proj"))?,
        d_conv: cfg.linear_conv_kernel_dim,
        d_inner: cfg.linear_num_value_heads * cfg.linear_value_head_dim,
        d_state: cfg.linear_value_head_dim,
        n_group: cfg.linear_num_key_heads,
    };

    let mlp = build_moe_block(dev, tensors, base, cfg)?;

    Ok(super::model::TargetLayer::GatedDelta {
        attn_norm: upload_target_rms_norm(
            dev,
            tensors,
            &format!("{base}.input_layernorm"),
            cfg.rms_norm_eps,
        )?,
        attn_post_norm: upload_target_rms_norm(
            dev,
            tensors,
            &format!("{base}.post_attention_layernorm"),
            cfg.rms_norm_eps,
        )?,
        ffn_norm: upload_target_rms_norm(
            dev,
            tensors,
            &format!("{base}.post_attention_layernorm"),
            cfg.rms_norm_eps,
        )?,
        delta,
        mlp,
    })
}

pub fn load_target_mlx4bit(dev: &Device, dir: &Path) -> Result<super::model::TargetWeights> {
    use super::qwen::Rope;

    let cfg = read_hf_config(dir)?;
    let shards = safetensors_shards(dir)?;
    let tensors = collect_tensors(&shards)?;

    let root = if tensors.contains_key("language_model.model.embed_tokens.weight") {
        "language_model."
    } else {
        ""
    };
    let full_attn_interval = cfg.full_attention_interval;
    let head_dim = cfg.head_dim;

    // Token embedding. mlx-community exports it as an MLX-4bit linear
    // with prefix `model.embed_tokens`.
    let tok_embed = upload_linear_4bit(dev, &tensors, &format!("{root}model.embed_tokens"))?;

    // Per-layer walk: attention layers land on `(il + 1) % interval == 0`,
    // GDN elsewhere. Matches the reference's
    // `full_attention_interval` semantic.
    let mut layers: Vec<super::model::TargetLayer> =
        Vec::with_capacity(cfg.num_hidden_layers as usize);
    for il in 0..cfg.num_hidden_layers {
        let base = format!("{root}model.layers.{il}");
        let is_full = cfg
            .layer_types
            .get(il as usize)
            .map(|t| t == "full_attention")
            .unwrap_or_else(|| full_attn_interval > 0 && ((il + 1) % full_attn_interval == 0));
        let layer = if is_full {
            build_target_full_attn_layer(dev, &tensors, &base, &cfg)?
        } else {
            build_target_gdn_layer(dev, &tensors, &base, &cfg)?
        };
        layers.push(layer);
    }

    let out_norm = upload_target_rms_norm(
        dev,
        &tensors,
        &format!("{root}model.norm"),
        cfg.rms_norm_eps,
    )?;
    let output = upload_linear_4bit(dev, &tensors, &format!("{root}lm_head"))
        .or_else(|_| upload_linear_4bit(dev, &tensors, &format!("{root}model.embed_tokens")))?;
    let vision = load_vision_weights(dev, &tensors)?;

    Ok(super::model::TargetWeights {
        tok_embed,
        vision,
        layers,
        out_norm,
        output,
        full_attention_interval: full_attn_interval,
        rope_sections: [11, 11, 10, 0],
        n_embd_head_k: head_dim,
        n_embd_head_v: head_dim,
        n_head: cfg.num_attention_heads,
        n_head_kv: cfg.num_key_value_heads,
        n_layer: cfg.num_hidden_layers,
        n_embd: cfg.hidden_size,
        n_ff: cfg
            .moe_intermediate_size
            .max(cfg.shared_expert_intermediate_size),
        ssm_d_conv: cfg.linear_conv_kernel_dim,
        ssm_d_inner: cfg.linear_num_value_heads * cfg.linear_value_head_dim,
        ssm_d_state: cfg.linear_value_head_dim,
        ssm_dt_rank: cfg.linear_num_value_heads,
        ssm_n_group: cfg.linear_num_key_heads,
        rope: Rope {
            head_dim,
            rope_dim: ((head_dim as f32) * cfg.partial_rotary_factor).round() as i32,
            base: cfg.rope_theta,
        },
    })
}

/// Load the z-lab DFlash draft (bf16 safetensors). Format: flat
/// single-file bf16 safetensors with HF-style keys. Mirrors the
/// CUDA-side `cuda::loader::load_draft_safetensors` expectations.
pub fn load_draft_safetensors(dev: &Device, path: &Path) -> Result<super::model::DraftWeights> {
    use super::qwen::{Bf16Attention, Bf16Mlp, Rope};

    let file = File::open(path).with_context(|| format!("open draft {}", path.display()))?;
    let mapped =
        unsafe { Mmap::map(&file) }.with_context(|| format!("mmap draft {}", path.display()))?;
    let mmap = Arc::new(mapped);
    let st = SafeTensors::deserialize(&mmap)
        .with_context(|| format!("deserialize draft {}", path.display()))?;

    let mut tensors: BTreeMap<String, TensorEntry> = BTreeMap::new();
    for (name, view) in st.tensors() {
        tensors.insert(name.to_string(), TensorEntry::from_view(&mmap, view)?);
    }

    let n_layers = tensors
        .keys()
        .filter_map(|k| {
            k.strip_prefix("layers.")
                .and_then(|rest| rest.split('.').next())
                .and_then(|idx| idx.parse::<i32>().ok())
        })
        .max()
        .map(|x| x + 1)
        .ok_or_else(|| anyhow!("draft safetensors: no layers.* tensors found"))?;
    if n_layers != DFLASH35B_DRAFT_LAYERS {
        return Err(anyhow!(
            "draft safetensors layer count mismatch: found {n_layers}, expected {DFLASH35B_DRAFT_LAYERS}"
        ));
    }
    let head_dim: i32 = 128;
    let rope_theta: f32 = 10_000_000.0;
    let rms_eps: f32 = 1e-6;
    let q0 = tensors
        .get("layers.0.self_attn.q_proj.weight")
        .ok_or_else(|| anyhow!("missing layers.0.self_attn.q_proj.weight"))?;
    let k0 = tensors
        .get("layers.0.self_attn.k_proj.weight")
        .ok_or_else(|| anyhow!("missing layers.0.self_attn.k_proj.weight"))?;
    let n_heads = (q0.shape[0] as i32) / head_dim;
    let n_kv_heads = (k0.shape[0] as i32) / head_dim;
    if q0.shape.len() != 2 || q0.shape[1] as i32 != DFLASH35B_TARGET_HIDDEN {
        return Err(anyhow!(
            "draft q_proj shape mismatch: got {:?}, expected [*, {}]",
            q0.shape,
            DFLASH35B_TARGET_HIDDEN
        ));
    }
    let fc = upload_bf16_linear(dev, &tensors, "fc")?;
    let expected_fc_in = DFLASH35B_TARGET_HIDDEN * DFLASH35B_DRAFT_N_TARGET_LAYERS;
    if fc.in_features != expected_fc_in || fc.out_features != DFLASH35B_TARGET_HIDDEN {
        return Err(anyhow!(
            "draft fc shape mismatch: got [{}, {}], expected [{}, {}]",
            fc.out_features,
            fc.in_features,
            DFLASH35B_TARGET_HIDDEN,
            expected_fc_in
        ));
    }

    let mut layers: Vec<super::model::DraftLayer> = Vec::with_capacity(n_layers as usize);
    for il in 0..n_layers {
        let base = format!("layers.{il}");
        let attention = Bf16Attention {
            wq: upload_bf16_linear(dev, &tensors, &format!("{base}.self_attn.q_proj"))?,
            wk: upload_bf16_linear(dev, &tensors, &format!("{base}.self_attn.k_proj"))?,
            wv: upload_bf16_linear(dev, &tensors, &format!("{base}.self_attn.v_proj"))?,
            wo: upload_bf16_linear(dev, &tensors, &format!("{base}.self_attn.o_proj"))?,
            q_norm: Some(upload_rms_norm(
                dev,
                &tensors,
                &format!("{base}.self_attn.q_norm"),
                rms_eps,
            )?),
            k_norm: Some(upload_rms_norm(
                dev,
                &tensors,
                &format!("{base}.self_attn.k_norm"),
                rms_eps,
            )?),
            rope: Rope {
                head_dim,
                rope_dim: head_dim,
                base: rope_theta,
            },
            n_heads,
            n_kv_heads,
            head_dim,
        };
        let gate = upload_bf16_linear(dev, &tensors, &format!("{base}.mlp.gate_proj"))?;
        let mlp = Bf16Mlp {
            intermediate: gate.out_features,
            gate,
            up: upload_bf16_linear(dev, &tensors, &format!("{base}.mlp.up_proj"))?,
            down: upload_bf16_linear(dev, &tensors, &format!("{base}.mlp.down_proj"))?,
        };
        layers.push(super::model::DraftLayer {
            attn_norm: upload_rms_norm(dev, &tensors, &format!("{base}.input_layernorm"), rms_eps)?,
            ffn_norm: upload_rms_norm(
                dev,
                &tensors,
                &format!("{base}.post_attention_layernorm"),
                rms_eps,
            )?,
            attention,
            mlp,
        });
    }

    Ok(super::model::DraftWeights {
        fc,
        hidden_norm: upload_rms_norm(dev, &tensors, "hidden_norm", rms_eps)?,
        layers,
        out_norm: upload_rms_norm(dev, &tensors, "norm", rms_eps)?,
    })
}
