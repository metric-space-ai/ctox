//! GGUF → `Tensor` loader for the Metal backend.
//!
//! Reads the tensor data blob out of a parsed GGUF file (see
//! `gguf.rs`) into a single crate-owned `MTLBuffer`, then constructs
//! a `Tensor` view for every named tensor. Callers look up weights
//! by the HF tensor name (e.g. `model.layers.5.attn_q_weight`).
//!
//! # Layout choice
//!
//! We allocate ONE large shared-storage `MTLBuffer` sized to the full
//! tensor-data region of the GGUF file, copy the bytes contiguously
//! into it (preserving the GGUF offsets), and hand out `Tensor`s with
//! `offset` equal to the GGUF's own tensor offset. This matches how
//! llama.cpp's `ggml_backend_metal_buffer_type` allocates a single
//! backing buffer per model and creates tensor views into it — zero
//! duplication, O(1) per-tensor allocation.
//!
//! ref (same model-load pattern on the CUDA side):
//!      `src/cuda/loader.rs::load_target_gguf`

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use crate::metal::ffi::{Buffer, Device};
use crate::metal::gguf::{Gguf, GgufValue};
use crate::metal::tensor::{GgmlType, Tensor};

/// Full weight bundle loaded from a single `.gguf` file.
pub struct Weights {
    /// Parsed file metadata (hparams, tokenizer, etc.) — queryable
    /// via `kv_*` accessors.
    pub kv: BTreeMap<String, GgufValue>,
    /// Name → tensor lookup. Byte-exact to the GGUF tensor-info
    /// directory's name entries.
    pub tensors: BTreeMap<String, Tensor>,
    /// Backing buffer holding all tensor data. Arc'd so the
    /// `Tensor`s above can share ownership without taking lifetime
    /// on the `Weights`.
    pub backing: Arc<Buffer>,
}

impl Weights {
    /// Parse the GGUF file + upload every tensor to a single
    /// shared-storage `MTLBuffer`.
    pub fn load_gguf(dev: &Device, path: &Path) -> Result<Self> {
        let mut f = Gguf::open(path)?;

        // Size the backing buffer to fit the last tensor's end offset.
        // Tensor `offset` fields are relative to the start of the
        // tensor-data blob; total size = max(offset + nbytes).
        let mut total_bytes: u64 = 0;
        for t in &f.tensors {
            let dtype = GgmlType::from_raw(t.type_raw).ok_or_else(|| {
                anyhow!("tensor `{}`: unknown ggml_type code {}", t.name, t.type_raw)
            })?;
            let mut ne: [i64; 4] = [1, 1, 1, 1];
            for (i, &d) in t.shape.iter().enumerate() {
                if i < 4 {
                    ne[i] = d as i64;
                }
            }
            let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;
            let bs = dtype.block_size();
            let ts = dtype.type_size();
            let nbytes = (n / bs) * ts;
            let end = t.offset + nbytes as u64;
            if end > total_bytes {
                total_bytes = end;
            }
        }

        let backing = dev.new_buffer(total_bytes as usize).ok_or_else(|| {
            anyhow!(
                "failed to allocate {} bytes for GGUF backing buffer",
                total_bytes
            )
        })?;
        let backing = Arc::new(backing);

        // Stream tensor data into the backing buffer via a single
        // on-disk pass. We precompute per-tensor (name, offset,
        // nbytes) into an owned vec so we don't borrow `f.tensors`
        // while calling `f.read_tensor_bytes(idx, dst)` (which needs
        // `&mut f`).
        let stream_plan: Vec<(String, u64, usize)> = f
            .tensors
            .iter()
            .map(|t| {
                let dtype = GgmlType::from_raw(t.type_raw).unwrap();
                let mut ne: [i64; 4] = [1, 1, 1, 1];
                for (i, &d) in t.shape.iter().enumerate() {
                    if i < 4 {
                        ne[i] = d as i64;
                    }
                }
                let n = (ne[0] * ne[1] * ne[2] * ne[3]) as usize;
                let bs = dtype.block_size();
                let ts = dtype.type_size();
                (t.name.clone(), t.offset, (n / bs) * ts)
            })
            .collect();

        let base_ptr = backing.as_ptr() as *mut u8;
        for (idx, (name, offset, nbytes)) in stream_plan.iter().enumerate() {
            let dst =
                unsafe { std::slice::from_raw_parts_mut(base_ptr.add(*offset as usize), *nbytes) };
            f.read_tensor_bytes(idx, dst)
                .with_context(|| format!("read tensor `{}`", name))?;
        }

        // Build the name→Tensor map.
        let mut tensors: BTreeMap<String, Tensor> = BTreeMap::new();
        for t in &f.tensors {
            let dtype = GgmlType::from_raw(t.type_raw).unwrap();
            let mut ne: [i64; 4] = [1, 1, 1, 1];
            for (i, &d) in t.shape.iter().enumerate() {
                if i < 4 {
                    ne[i] = d as i64;
                }
            }
            let nb = Tensor::make_contiguous_strides(dtype, ne);
            tensors.insert(
                t.name.clone(),
                Tensor {
                    name: t.name.clone(),
                    dtype,
                    ne,
                    nb,
                    buffer: backing.clone(),
                    offset: t.offset as usize,
                },
            );
        }

        Ok(Self {
            kv: f.kv,
            tensors,
            backing,
        })
    }

    /// Look up a tensor by name. Returns `None` if the model doesn't
    /// have that tensor (e.g. for a layer that's not supposed to
    /// exist in the given checkpoint).
    pub fn get(&self, name: &str) -> Option<&Tensor> {
        self.tensors.get(name)
    }

    /// Same as `get` but returns an error with a clear message if
    /// the tensor is missing — convenient for required weights.
    pub fn require(&self, name: &str) -> Result<&Tensor> {
        self.get(name)
            .ok_or_else(|| anyhow!("required tensor `{name}` not in GGUF file"))
    }

    /// Metadata `u32` reader.
    pub fn kv_u32(&self, name: &str) -> Option<u32> {
        match self.kv.get(name)? {
            GgufValue::U32(v) => Some(*v),
            GgufValue::I32(v) => Some(*v as u32),
            _ => None,
        }
    }

    pub fn kv_u64(&self, name: &str) -> Option<u64> {
        match self.kv.get(name)? {
            GgufValue::U64(v) => Some(*v),
            GgufValue::U32(v) => Some(*v as u64),
            GgufValue::I64(v) => Some(*v as u64),
            _ => None,
        }
    }

    pub fn kv_f32(&self, name: &str) -> Option<f32> {
        match self.kv.get(name)? {
            GgufValue::F32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn kv_string(&self, name: &str) -> Option<&str> {
        match self.kv.get(name)? {
            GgufValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // No-op sanity: the module compiles + the public types are named
    // as the graph builder (next session's port) expects.
    #[test]
    fn weights_struct_shape() {
        fn _assert_has_api<T: Sized>(_: T) {}
        // `Weights::load_gguf` needs a Device and a path; both compile.
        _assert_has_api(Weights::load_gguf as fn(&Device, &Path) -> Result<Weights>);
    }
}
