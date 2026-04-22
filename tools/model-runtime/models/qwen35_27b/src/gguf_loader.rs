//! GGUF weight loader.
//!
//! Parses a GGUF v3 container (llama.cpp's weight format) from disk
//! and emits a `HashMap<String, GgufTensor>` of device-resident
//! weights.
//!
//! Scope:
//!   * Header: magic `GGUF`, version 3, tensor/metadata counts.
//!   * Metadata: parsed only enough to locate `general.alignment`;
//!     values are otherwise skipped (we don't surface metadata from
//!     this loader — that's the tokenizer / config layer's job).
//!   * Tensor descriptors: name, n_dims, dims, ggml dtype, offset.
//!   * Tensor data: mmap'd, sliced per-tensor, uploaded to the GPU
//!     via `CudaTensor::from_host`. Byte-packed types (Q4_K_M) go
//!     through a `CudaTensor<i8>` of the raw block bytes.
//!
//! Supported dtypes:
//!   * F32     (ggml_type 0)
//!   * F16     (ggml_type 1)
//!   * Q8_0    (ggml_type 8  — 34 B / 32 elems, CPU-dequant to bf16)
//!   * Q4_K    (ggml_type 12 — 144 B / 256 elems, native packed)
//!   * Q5_K    (ggml_type 13 — 176 B / 256 elems, CPU-dequant to bf16)
//!   * Q6_K    (ggml_type 14 — 210 B / 256 elems, CPU-dequant to bf16)
//!   * IQ4_XS  (ggml_type 23 — 136 B / 256 elems, CPU-dequant to bf16)
//!   * I8      (ggml_type 24)
//!   * I32     (ggml_type 26)
//!   * BF16    (ggml_type 30)
//!
//! Any other ggml dtype returns an `unsupported ggml dtype` error.
//!
//! The Q5_K / Q6_K / Q8_0 / IQ4_XS paths have two modes, selected via
//! [`LoaderConfig::keep_packed`]:
//!
//!   * `keep_packed = false` (default, Phase-5 behavior): each block is
//!     CPU-dequantized into a `Vec<bf16>` and uploaded as
//!     [`GgufBuf::Bf16`]. Roughly doubles on-device memory for these
//!     tensors vs. the on-disk packed size, but lets any bf16-consumer
//!     forward kernel read them without a packed-mmvq path.
//!
//!   * `keep_packed = true` (Phase-6+): each tensor is uploaded as a
//!     1-D `CudaTensor<i8>` of the raw packed block bytes (same protocol
//!     Q4_K already uses) and surfaces as the matching
//!     [`GgufBuf::Q5K`] / `Q6K` / `Q8_0` / `IQ4XS` variant. On-device
//!     size matches the GGUF-on-disk size; the consumer is expected to
//!     dispatch a packed-aware mmvq kernel.
//!
//! In either mode the original ggml format is preserved in the
//! `GgufTensor.dtype` tag so callers can still print a per-format tensor
//! breakdown.
//!
//! Performance note: the data region for a 27B Q4_K_M model is ~15 GB,
//! so we mmap the file rather than slurping it into a `Vec<u8>`. Each
//! per-tensor upload is a single `cudaMemcpy` H2D via cudarc's
//! `memcpy_stod`; PCIe — not the CPU-side parse — is the bottleneck.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use half::{bf16, f16};
use memmap2::Mmap;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::dtype::DType;
use ctox_cuda_primitives::tensor::CudaTensor;

/// Magic bytes: ASCII "GGUF" stored little-endian as u32 = 0x46554747.
const GGUF_MAGIC: u32 = 0x4655_4747;

/// Supported GGUF container version. We intentionally pin v3 — earlier
/// versions (v1/v2) used different tensor-info encodings and v1 used
/// u32 counts instead of u64.
const GGUF_VERSION: u32 = 3;

/// Default tensor-data alignment when `general.alignment` is absent.
/// Matches the llama.cpp default.
const DEFAULT_ALIGNMENT: u64 = 32;

/// One device-resident tensor loaded from a GGUF file.
pub struct GgufTensor {
    pub name: String,
    pub dtype: DType,
    pub shape: Vec<usize>,
    pub buf: GgufBuf,
}

/// Device storage for a GGUF tensor, tagged by element type.
///
/// Q4_K (and any other sub-byte packed format we add later) lives in
/// an `i8` tensor whose length equals the block-byte count — the
/// unpacking kernels know how to read it.
///
/// Q5K/Q6K/Q8_0/IQ4XS appear as `i8` byte buffers when the loader is
/// run with [`LoaderConfig::keep_packed`] set (cuts the 27B resident-
/// weight footprint roughly in half); otherwise they arrive as
/// CPU-dequantized [`GgufBuf::Bf16`] tensors with the dtype tag carrying
/// the original ggml format for logging.
pub enum GgufBuf {
    F32(CudaTensor<f32>),
    F16(CudaTensor<f16>),
    Bf16(CudaTensor<bf16>),
    /// Byte-packed Q4_K_M blocks: 144 B per 256-element block.
    Q4K(CudaTensor<i8>),
    /// Byte-packed Q5_K blocks: 176 B per 256-element block
    /// (`keep_packed = true` only).
    Q5K(CudaTensor<i8>),
    /// Byte-packed Q6_K blocks: 210 B per 256-element block
    /// (`keep_packed = true` only).
    Q6K(CudaTensor<i8>),
    /// Byte-packed Q8_0 blocks: 34 B per 32-element block
    /// (`keep_packed = true` only).
    Q8_0(CudaTensor<i8>),
    /// Byte-packed IQ4_XS blocks: 136 B per 256-element block
    /// (`keep_packed = true` only).
    IQ4XS(CudaTensor<i8>),
    I32(CudaTensor<i32>),
    I8(CudaTensor<i8>),
}

/// Knobs for [`load_gguf_with_config`] / [`load_gguf_lenient_with_config`].
///
/// Default (`keep_packed = false`) preserves the Phase-5 behavior: all
/// supported quant types are CPU-dequantized to bf16 at load time and
/// arrive as [`GgufBuf::Bf16`] tensors. Set `keep_packed = true` to
/// upload Q5K/Q6K/Q8_0/IQ4_XS tensors as raw block bytes
/// (matching what Q4K already does) — on-device memory then matches
/// the GGUF-on-disk size, removing the Phase-5 VRAM doubling. The
/// consumer side (model layer structs / forward kernels) must handle
/// the new packed variants or it will still see zero placeholders
/// downstream.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoaderConfig {
    /// If true, keep Q5K/Q6K/Q8_0/IQ4_XS tensors as packed
    /// `CudaTensor<i8>` byte buffers; if false, CPU-dequant to bf16
    /// at load time (Phase-5 behavior).
    pub keep_packed: bool,
}

/// GGML tensor type codes (subset — only the ones we parse or reject).
///
/// Values come straight from `ggml.h`. We spell them out here to avoid
/// re-linking ggml just for the enum.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GgmlType {
    F32 = 0,
    F16 = 1,
    Q4_0 = 2,
    Q4_1 = 3,
    Q5_0 = 6,
    Q5_1 = 7,
    Q8_0 = 8,
    Q8_1 = 9,
    Q2K = 10,
    Q3K = 11,
    Q4K = 12,
    Q5K = 13,
    Q6K = 14,
    Q8K = 15,
    IQ2XXS = 16,
    IQ2XS = 17,
    IQ3XXS = 18,
    IQ1S = 19,
    IQ4NL = 20,
    IQ3S = 21,
    IQ2S = 22,
    IQ4XS = 23,
    I8 = 24,
    I16 = 25,
    I32 = 26,
    I64 = 27,
    F64 = 28,
    IQ1M = 29,
    BF16 = 30,
    Unknown(u32),
}

impl GgmlType {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => GgmlType::F32,
            1 => GgmlType::F16,
            2 => GgmlType::Q4_0,
            3 => GgmlType::Q4_1,
            6 => GgmlType::Q5_0,
            7 => GgmlType::Q5_1,
            8 => GgmlType::Q8_0,
            9 => GgmlType::Q8_1,
            10 => GgmlType::Q2K,
            11 => GgmlType::Q3K,
            12 => GgmlType::Q4K,
            13 => GgmlType::Q5K,
            14 => GgmlType::Q6K,
            15 => GgmlType::Q8K,
            16 => GgmlType::IQ2XXS,
            17 => GgmlType::IQ2XS,
            18 => GgmlType::IQ3XXS,
            19 => GgmlType::IQ1S,
            20 => GgmlType::IQ4NL,
            21 => GgmlType::IQ3S,
            22 => GgmlType::IQ2S,
            23 => GgmlType::IQ4XS,
            24 => GgmlType::I8,
            25 => GgmlType::I16,
            26 => GgmlType::I32,
            27 => GgmlType::I64,
            28 => GgmlType::F64,
            29 => GgmlType::IQ1M,
            30 => GgmlType::BF16,
            other => GgmlType::Unknown(other),
        }
    }
}

/// GGUF metadata value-type codes. Used while skipping metadata KVs.
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
enum GgufValueType {
    U8 = 0,
    I8 = 1,
    U16 = 2,
    I16 = 3,
    U32 = 4,
    I32 = 5,
    F32 = 6,
    Bool = 7,
    String = 8,
    Array = 9,
    U64 = 10,
    I64 = 11,
    F64 = 12,
}

impl GgufValueType {
    fn from_u32(v: u32) -> Result<Self> {
        Ok(match v {
            0 => GgufValueType::U8,
            1 => GgufValueType::I8,
            2 => GgufValueType::U16,
            3 => GgufValueType::I16,
            4 => GgufValueType::U32,
            5 => GgufValueType::I32,
            6 => GgufValueType::F32,
            7 => GgufValueType::Bool,
            8 => GgufValueType::String,
            9 => GgufValueType::Array,
            10 => GgufValueType::U64,
            11 => GgufValueType::I64,
            12 => GgufValueType::F64,
            other => return Err(anyhow!("unknown gguf value type {}", other)),
        })
    }

    fn scalar_bytes(self) -> Option<usize> {
        Some(match self {
            GgufValueType::U8 | GgufValueType::I8 | GgufValueType::Bool => 1,
            GgufValueType::U16 | GgufValueType::I16 => 2,
            GgufValueType::U32 | GgufValueType::I32 | GgufValueType::F32 => 4,
            GgufValueType::U64 | GgufValueType::I64 | GgufValueType::F64 => 8,
            GgufValueType::String | GgufValueType::Array => return None,
        })
    }
}

/// Minimal forward-only cursor over the mmap'd file bytes.
///
/// All reads are little-endian per the GGUF spec.
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| anyhow!("cursor skip overflow"))?;
        if end > self.buf.len() {
            return Err(anyhow!(
                "cursor skip out of range: {} + {} > {}",
                self.pos,
                n,
                self.buf.len()
            ));
        }
        self.pos = end;
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8> {
        if self.pos >= self.buf.len() {
            return Err(anyhow!("read_u8 past end"));
        }
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32> {
        if self.pos + 4 > self.buf.len() {
            return Err(anyhow!("read_u32 past end"));
        }
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
        self.pos += 4;
        Ok(u32::from_le_bytes(b))
    }

    fn read_u64(&mut self) -> Result<u64> {
        if self.pos + 8 > self.buf.len() {
            return Err(anyhow!("read_u64 past end"));
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.buf[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(u64::from_le_bytes(b))
    }

    fn read_f32(&mut self) -> Result<f32> {
        if self.pos + 4 > self.buf.len() {
            return Err(anyhow!("read_f32 past end"));
        }
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
        self.pos += 4;
        Ok(f32::from_le_bytes(b))
    }

    fn read_string(&mut self) -> Result<String> {
        let len = self.read_u64()? as usize;
        if self.pos + len > self.buf.len() {
            return Err(anyhow!(
                "string length {} would exceed buf (pos={}, total={})",
                len,
                self.pos,
                self.buf.len()
            ));
        }
        let bytes = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        // GGUF strings are UTF-8 without a NUL terminator.
        String::from_utf8(bytes.to_vec()).map_err(|e| anyhow!("non-utf8 gguf string: {}", e))
    }

    /// Skip a GGUF metadata value of the given type. Recursive for
    /// arrays.
    fn skip_value(&mut self, ty: GgufValueType) -> Result<()> {
        match ty {
            GgufValueType::String => {
                let len = self.read_u64()? as usize;
                self.skip(len)?;
            }
            GgufValueType::Array => {
                let elem_ty = GgufValueType::from_u32(self.read_u32()?)?;
                let n = self.read_u64()? as usize;
                if let Some(sz) = elem_ty.scalar_bytes() {
                    self.skip(
                        n.checked_mul(sz)
                            .ok_or_else(|| anyhow!("array byte count overflow: {} * {}", n, sz))?,
                    )?;
                } else {
                    for _ in 0..n {
                        self.skip_value(elem_ty)?;
                    }
                }
            }
            other => {
                let sz = other
                    .scalar_bytes()
                    .expect("scalar types handled; non-scalars matched above");
                self.skip(sz)?;
            }
        }
        Ok(())
    }

    /// Read and (optionally) capture a GGUF metadata value. We capture
    /// only the handful of values we care about — specifically
    /// `general.alignment` (a u32) — and otherwise skip.
    fn read_value_maybe_u32(&mut self, ty: GgufValueType) -> Result<Option<u32>> {
        match ty {
            GgufValueType::U32 => Ok(Some(self.read_u32()?)),
            other => {
                self.skip_value(other)?;
                Ok(None)
            }
        }
    }
}

/// Result of `load_gguf_lenient` — both the successfully-uploaded
/// tensors and the list of tensors whose ggml dtype isn't in the
/// loader's supported set.
pub struct GgufLoad {
    pub tensors: HashMap<String, GgufTensor>,
    /// `(tensor_name, unsupported_dtype_description)` pairs. Entries
    /// correspond to tensors that were parsed out of the file but
    /// skipped at upload time because their ggml dtype isn't in the
    /// loader's supported set (Q2_K, Q3_K, Q5_K, Q6_K, Q8_0, …). The
    /// total descriptor count equals `tensors.len() + unsupported.len()`.
    pub unsupported: Vec<(String, String)>,
    /// Total tensor-descriptor count from the GGUF header (==
    /// `tensors.len() + unsupported.len()`). Stored explicitly so
    /// callers can cross-check against the header even if some
    /// tensors were skipped.
    pub total_descriptors: usize,
}

/// Parse a GGUF v3 container and upload every tensor to the GPU.
///
/// Strict mode: returns `Err` the moment an unsupported ggml dtype
/// is encountered. For models containing non-supported quant types
/// (Q6_K output heads, Q2_K/Q3_K/Q5_K/Q8_0, etc.) use
/// [`load_gguf_lenient`] instead.
///
/// Returns a map keyed by tensor name. Order of upload is the file's
/// declaration order.
pub fn load_gguf<P: AsRef<Path>>(
    device: &Arc<DeviceContext>,
    path: P,
) -> Result<HashMap<String, GgufTensor>> {
    load_gguf_with_config(device, path, LoaderConfig::default())
}

/// [`load_gguf`] with explicit loader config. See [`LoaderConfig`] for
/// the `keep_packed` switch.
pub fn load_gguf_with_config<P: AsRef<Path>>(
    device: &Arc<DeviceContext>,
    path: P,
    cfg: LoaderConfig,
) -> Result<HashMap<String, GgufTensor>> {
    let load = load_gguf_impl(device, path.as_ref(), true, cfg)?;
    Ok(load.tensors)
}

/// Lenient variant of [`load_gguf`]: tensors with unsupported ggml
/// dtypes are logged (at `tracing::warn!`), left un-uploaded, and
/// recorded in the returned `unsupported` list. All supported
/// tensors are still uploaded; parse errors (bad header, truncated
/// file) still hard-fail.
pub fn load_gguf_lenient<P: AsRef<Path>>(device: &Arc<DeviceContext>, path: P) -> Result<GgufLoad> {
    load_gguf_lenient_with_config(device, path, LoaderConfig::default())
}

/// [`load_gguf_lenient`] with explicit loader config.
pub fn load_gguf_lenient_with_config<P: AsRef<Path>>(
    device: &Arc<DeviceContext>,
    path: P,
    cfg: LoaderConfig,
) -> Result<GgufLoad> {
    load_gguf_impl(device, path.as_ref(), false, cfg)
}

fn load_gguf_impl(
    device: &Arc<DeviceContext>,
    path: &Path,
    strict: bool,
    cfg: LoaderConfig,
) -> Result<GgufLoad> {
    let file = File::open(path).with_context(|| format!("open gguf file {}", path.display()))?;
    // SAFETY: we mmap read-only; caller must not truncate the file
    // underneath us. The cudarc H2D copy reads from this mapping
    // synchronously (memcpy_stod is a blocking cudaMemcpy), so no
    // after-drop risk.
    let mmap = unsafe { Mmap::map(&file) }
        .with_context(|| format!("mmap gguf file {}", path.display()))?;

    let mut cur = Cursor::new(&mmap);

    // --- Header -----------------------------------------------------
    let magic = cur.read_u32()?;
    if magic != GGUF_MAGIC {
        return Err(anyhow!(
            "not a gguf file: magic=0x{:08x} (expected 0x{:08x})",
            magic,
            GGUF_MAGIC
        ));
    }
    let version = cur.read_u32()?;
    if version != GGUF_VERSION {
        return Err(anyhow!(
            "unsupported gguf version {} (expected {})",
            version,
            GGUF_VERSION
        ));
    }
    let tensor_count = cur.read_u64()? as usize;
    let metadata_kv_count = cur.read_u64()? as usize;

    // --- Metadata ---------------------------------------------------
    // We only care about one key: `general.alignment`. Everything else
    // is skipped in-place.
    let mut alignment: u64 = DEFAULT_ALIGNMENT;
    for _ in 0..metadata_kv_count {
        let key = cur.read_string()?;
        let ty = GgufValueType::from_u32(cur.read_u32()?)?;
        if key == "general.alignment" {
            if let Some(v) = cur.read_value_maybe_u32(ty)? {
                alignment = v as u64;
            }
        } else {
            cur.skip_value(ty)?;
        }
    }
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(anyhow!(
            "invalid gguf alignment: {} (must be nonzero power of two)",
            alignment
        ));
    }

    // --- Tensor descriptors ----------------------------------------
    struct Descriptor {
        name: String,
        shape: Vec<usize>,
        ggml: GgmlType,
        offset: u64,
    }
    let mut descriptors: Vec<Descriptor> = Vec::with_capacity(tensor_count);
    for _ in 0..tensor_count {
        let name = cur.read_string()?;
        let n_dims = cur.read_u32()? as usize;
        let mut shape = Vec::with_capacity(n_dims);
        for _ in 0..n_dims {
            shape.push(cur.read_u64()? as usize);
        }
        let ggml_ty = GgmlType::from_u32(cur.read_u32()?);
        let offset = cur.read_u64()?;
        descriptors.push(Descriptor {
            name,
            shape,
            ggml: ggml_ty,
            offset,
        });
    }

    // --- Align to data region --------------------------------------
    // Tensor data begins at the next `alignment`-aligned offset after
    // the tensor-info section.
    let hdr_end = cur.pos() as u64;
    let data_start = align_up(hdr_end, alignment) as usize;
    if data_start > mmap.len() {
        return Err(anyhow!(
            "gguf data region start ({}) exceeds file size ({})",
            data_start,
            mmap.len()
        ));
    }
    let data_region = &mmap[data_start..];

    // --- Upload each tensor ----------------------------------------
    let mut out = HashMap::with_capacity(tensor_count);
    let mut unsupported: Vec<(String, String)> = Vec::new();
    for d in descriptors {
        let Descriptor {
            name,
            shape,
            ggml,
            offset,
        } = d;

        // GGUF shapes are stored in reverse of row-major dimension
        // order. llama.cpp keeps them in "ggml order" (fastest-moving
        // dim first). Reverse here so the resulting shape matches the
        // logical `[rows, cols, ...]` convention that the rest of the
        // engine expects (`token_embd.weight` => [vocab, hidden]).
        let mut shape_row_major = shape.clone();
        shape_row_major.reverse();

        let n_elems: usize = shape_row_major.iter().product();
        let offset = offset as usize;

        let tensor = match ggml {
            GgmlType::F32 => {
                let slice = typed_slice::<f32>(data_region, offset, n_elems)?;
                let t = CudaTensor::from_host(device.clone(), shape_row_major.clone(), slice)
                    .with_context(|| format!("upload F32 tensor {}", name))?;
                GgufTensor {
                    name: name.clone(),
                    dtype: DType::F32,
                    shape: shape_row_major,
                    buf: GgufBuf::F32(t),
                }
            }
            GgmlType::F16 => {
                let slice = typed_slice::<f16>(data_region, offset, n_elems)?;
                let t = CudaTensor::from_host(device.clone(), shape_row_major.clone(), slice)
                    .with_context(|| format!("upload F16 tensor {}", name))?;
                GgufTensor {
                    name: name.clone(),
                    dtype: DType::F16,
                    shape: shape_row_major,
                    buf: GgufBuf::F16(t),
                }
            }
            GgmlType::BF16 => {
                let slice = typed_slice::<bf16>(data_region, offset, n_elems)?;
                let t = CudaTensor::from_host(device.clone(), shape_row_major.clone(), slice)
                    .with_context(|| format!("upload BF16 tensor {}", name))?;
                GgufTensor {
                    name: name.clone(),
                    dtype: DType::Bf16,
                    shape: shape_row_major,
                    buf: GgufBuf::Bf16(t),
                }
            }
            GgmlType::I32 => {
                let slice = typed_slice::<i32>(data_region, offset, n_elems)?;
                let t = CudaTensor::from_host(device.clone(), shape_row_major.clone(), slice)
                    .with_context(|| format!("upload I32 tensor {}", name))?;
                GgufTensor {
                    name: name.clone(),
                    dtype: DType::I32,
                    shape: shape_row_major,
                    buf: GgufBuf::I32(t),
                }
            }
            GgmlType::I8 => {
                let slice = typed_slice::<i8>(data_region, offset, n_elems)?;
                let t = CudaTensor::from_host(device.clone(), shape_row_major.clone(), slice)
                    .with_context(|| format!("upload I8 tensor {}", name))?;
                GgufTensor {
                    name: name.clone(),
                    dtype: DType::I8,
                    shape: shape_row_major,
                    buf: GgufBuf::I8(t),
                }
            }
            GgmlType::Q4K => {
                // Q4_K_M: 256-wide blocks of 144 bytes each. The last
                // axis must be a multiple of 256 in llama.cpp's quant
                // layout (otherwise this is a config bug upstream).
                let last = *shape_row_major
                    .last()
                    .ok_or_else(|| anyhow!("Q4K tensor {} has zero dimensions", name))?;
                if last % 256 != 0 {
                    return Err(anyhow!(
                        "Q4K tensor {} last-dim {} not a multiple of 256",
                        name,
                        last
                    ));
                }
                let byte_len = DType::Q4K.block_bytes_for_elements(n_elems);
                let slice = byte_slice_as_i8(data_region, offset, byte_len)
                    .with_context(|| format!("slice Q4K bytes for {}", name))?;
                // Build a 1-D [byte_len] i8 tensor of the raw packed
                // bytes; shape at the `GgufTensor` level remains the
                // logical element shape.
                let t = CudaTensor::from_host(device.clone(), vec![byte_len], slice)
                    .with_context(|| format!("upload Q4K tensor {}", name))?;
                GgufTensor {
                    name: name.clone(),
                    dtype: DType::Q4K,
                    shape: shape_row_major,
                    buf: GgufBuf::Q4K(t),
                }
            }
            GgmlType::Q5K => {
                if cfg.keep_packed {
                    load_packed_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::Q5K,
                        256,
                        GgufBuf::Q5K,
                    )?
                } else {
                    load_dequant_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::Q5K,
                        256,
                        dequant_q5_k_to_bf16,
                    )?
                }
            }
            GgmlType::Q6K => {
                if cfg.keep_packed {
                    load_packed_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::Q6K,
                        256,
                        GgufBuf::Q6K,
                    )?
                } else {
                    load_dequant_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::Q6K,
                        256,
                        dequant_q6_k_to_bf16,
                    )?
                }
            }
            GgmlType::Q8_0 => {
                if cfg.keep_packed {
                    load_packed_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::Q8_0,
                        32,
                        GgufBuf::Q8_0,
                    )?
                } else {
                    load_dequant_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::Q8_0,
                        32,
                        dequant_q8_0_to_bf16,
                    )?
                }
            }
            GgmlType::IQ4XS => {
                if cfg.keep_packed {
                    load_packed_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::IQ4XS,
                        256,
                        GgufBuf::IQ4XS,
                    )?
                } else {
                    load_dequant_tensor(
                        device,
                        data_region,
                        offset,
                        &name,
                        &shape_row_major,
                        n_elems,
                        DType::IQ4XS,
                        256,
                        dequant_iq4_xs_to_bf16,
                    )?
                }
            }
            unsupported_ty => {
                if strict {
                    return Err(anyhow!(
                        "unsupported ggml dtype {:?} for tensor {}",
                        unsupported_ty,
                        name
                    ));
                }
                tracing::warn!(
                    tensor = %name,
                    dtype = ?unsupported_ty,
                    "skipping tensor with unsupported ggml dtype"
                );
                unsupported.push((name.clone(), format!("{:?}", unsupported_ty)));
                continue;
            }
        };

        if out.insert(name.clone(), tensor).is_some() {
            return Err(anyhow!("duplicate tensor name in gguf: {}", name));
        }
    }

    Ok(GgufLoad {
        tensors: out,
        unsupported,
        total_descriptors: tensor_count,
    })
}

/// Round `v` up to the next multiple of `align`. `align` must be a
/// power of two (checked in `load_gguf`).
fn align_up(v: u64, align: u64) -> u64 {
    (v + (align - 1)) & !(align - 1)
}

/// Reinterpret a region of `data` as `&[T]`, checking length and (best-
/// effort) alignment. Unsafe-free since we go through `bytemuck`.
fn typed_slice<T>(data: &[u8], offset: usize, n_elems: usize) -> Result<&[T]>
where
    T: bytemuck::Pod,
{
    let elem_size = std::mem::size_of::<T>();
    let byte_len = n_elems
        .checked_mul(elem_size)
        .ok_or_else(|| anyhow!("byte length overflow: {} * {}", n_elems, elem_size))?;
    let end = offset
        .checked_add(byte_len)
        .ok_or_else(|| anyhow!("offset+len overflow: {} + {}", offset, byte_len))?;
    if end > data.len() {
        return Err(anyhow!(
            "tensor slice out of bounds: [{}, {}) vs data len {}",
            offset,
            end,
            data.len()
        ));
    }
    let bytes = &data[offset..end];
    bytemuck::try_cast_slice::<u8, T>(bytes).map_err(|e| {
        anyhow!(
            "cast {} bytes to [{}; {}]: {}",
            byte_len,
            std::any::type_name::<T>(),
            n_elems,
            e
        )
    })
}

/// Reinterpret a region of `data` as `&[i8]` via a sign-punning cast.
/// Used only for the Q4_K byte-packed payload upload path.
fn byte_slice_as_i8(data: &[u8], offset: usize, byte_len: usize) -> Result<&[i8]> {
    let end = offset
        .checked_add(byte_len)
        .ok_or_else(|| anyhow!("offset+len overflow: {} + {}", offset, byte_len))?;
    if end > data.len() {
        return Err(anyhow!(
            "Q4K slice out of bounds: [{}, {}) vs data len {}",
            offset,
            end,
            data.len()
        ));
    }
    // Transmuting &[u8] -> &[i8] is safe: same size, same alignment,
    // and every bit pattern is a valid i8. bytemuck covers this.
    Ok(bytemuck::cast_slice::<u8, i8>(&data[offset..end]))
}

/// Shared plumbing for the four packed-on-device load paths (Q5_K,
/// Q6_K, Q8_0, IQ4_XS — same protocol as Q4_K's native packed path).
/// Slices the raw block bytes out of the mmap'd data region and
/// uploads them as a 1-D [byte_len] i8 `CudaTensor`; no CPU dequant.
///
/// The shape at the `GgufTensor` level stays as the logical element
/// shape; callers that consume packed bytes read the byte-count via
/// the dtype tag's [`DType::block_bytes_for_elements`].
#[allow(clippy::too_many_arguments)]
fn load_packed_tensor(
    device: &Arc<DeviceContext>,
    data_region: &[u8],
    offset: usize,
    name: &str,
    shape_row_major: &[usize],
    n_elems: usize,
    dtype_tag: DType,
    ggml_block_size: usize,
    wrap: fn(CudaTensor<i8>) -> GgufBuf,
) -> Result<GgufTensor> {
    let last = *shape_row_major
        .last()
        .ok_or_else(|| anyhow!("{:?} tensor {} has zero dimensions", dtype_tag, name))?;
    if last % ggml_block_size != 0 {
        return Err(anyhow!(
            "{:?} tensor {} last-dim {} not a multiple of {}",
            dtype_tag,
            name,
            last,
            ggml_block_size
        ));
    }
    let byte_len = dtype_tag.block_bytes_for_elements(n_elems);
    let slice = byte_slice_as_i8(data_region, offset, byte_len)
        .with_context(|| format!("slice {:?} bytes for {}", dtype_tag, name))?;
    let t = CudaTensor::from_host(device.clone(), vec![byte_len], slice)
        .with_context(|| format!("upload packed {:?} tensor {}", dtype_tag, name))?;
    Ok(GgufTensor {
        name: name.to_string(),
        dtype: dtype_tag,
        shape: shape_row_major.to_vec(),
        buf: wrap(t),
    })
}

/// Shared plumbing for the four CPU-dequant load paths (Q5_K, Q6_K,
/// Q8_0, IQ4_XS). Slices `block_bytes` worth of raw block bytes out of
/// the mmap'd data region, runs the given `dequant` function to produce
/// a `Vec<bf16>` of length `n_elems`, and uploads it as a bf16 tensor.
///
/// `ggml_block_size` is the element count per ggml block (32 for Q8_0,
/// 256 for the K-quants and IQ4_XS). The tensor's last dim must be a
/// multiple of it.
#[allow(clippy::too_many_arguments)]
fn load_dequant_tensor(
    device: &Arc<DeviceContext>,
    data_region: &[u8],
    offset: usize,
    name: &str,
    shape_row_major: &[usize],
    n_elems: usize,
    dtype_tag: DType,
    ggml_block_size: usize,
    dequant: fn(&[u8], usize) -> Result<Vec<bf16>>,
) -> Result<GgufTensor> {
    let last = *shape_row_major
        .last()
        .ok_or_else(|| anyhow!("{:?} tensor {} has zero dimensions", dtype_tag, name))?;
    if last % ggml_block_size != 0 {
        return Err(anyhow!(
            "{:?} tensor {} last-dim {} not a multiple of {}",
            dtype_tag,
            name,
            last,
            ggml_block_size
        ));
    }
    let byte_len = dtype_tag.block_bytes_for_elements(n_elems);
    let end = offset
        .checked_add(byte_len)
        .ok_or_else(|| anyhow!("offset+len overflow: {} + {}", offset, byte_len))?;
    if end > data_region.len() {
        return Err(anyhow!(
            "{:?} slice out of bounds for {}: [{}, {}) vs data len {}",
            dtype_tag,
            name,
            offset,
            end,
            data_region.len()
        ));
    }
    let bytes = &data_region[offset..end];
    let host = dequant(bytes, n_elems)
        .with_context(|| format!("dequantize {:?} tensor {}", dtype_tag, name))?;
    if host.len() != n_elems {
        return Err(anyhow!(
            "dequant {:?} tensor {} produced {} elems, expected {}",
            dtype_tag,
            name,
            host.len(),
            n_elems
        ));
    }
    let t = CudaTensor::from_host(device.clone(), shape_row_major.to_vec(), &host)
        .with_context(|| format!("upload dequantized {:?} tensor {}", dtype_tag, name))?;
    Ok(GgufTensor {
        name: name.to_string(),
        dtype: dtype_tag,
        shape: shape_row_major.to_vec(),
        buf: GgufBuf::Bf16(t),
    })
}

// ---- CPU-side dequant to bf16 -----------------------------------------
//
// Each routine ports the reference `dequantize_row_*` from llama.cpp's
// `ggml/src/ggml-quants.c` literally, operating on flat block bytes.
// Input is `n_elems / ggml_block_size` blocks packed back-to-back. We
// read `ggml_half` (fp16) super-scales as little-endian `u16` and
// widen to `f32` via `half::f16::to_f32` — that avoids pulling in any
// extra alignment assumptions on the mmap'd byte slice.
//
// Block sizes (QK_K = 256 throughout except Q8_0):
//   Q5_K:   176 B / 256 elems   { d, dmin, scales[12], qh[32], qs[128] }
//   Q6_K:   210 B / 256 elems   { ql[128], qh[64], scales[16 i8], d }
//   Q8_0:    34 B /  32 elems   { d, qs[32 i8] }
//   IQ4_XS: 136 B / 256 elems   { d, scales_h u16, scales_l[4], qs[128] }
//
// The IQ4_XS codebook is `kvalues_iq4nl` — a fixed 16-entry i8 table
// copied verbatim from `ggml-common.h`.

const IQ4_NL_KVALUES: [i8; 16] = [
    -127, -104, -83, -65, -49, -35, -22, -10, 1, 13, 25, 38, 53, 69, 89, 113,
];

/// Read a ggml_half (fp16) from a 2-byte little-endian slice and widen
/// to f32. Centralized so all four dequant paths use the same widening.
#[inline]
fn read_ggml_half(bytes: &[u8]) -> f32 {
    debug_assert!(bytes.len() >= 2);
    let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
    f16::from_bits(bits).to_f32()
}

/// Reference helper from `ggml-quants.c`:
/// ```text
/// static inline void get_scale_min_k4(int j, const uint8_t * q,
///                                     uint8_t * d, uint8_t * m) {
///     if (j < 4) {
///         *d = q[j] & 63;       *m = q[j+4] & 63;
///     } else {
///         *d = (q[j+4] & 0xF) | ((q[j-4] >> 6) << 4);
///         *m = (q[j+4] >>  4) | ((q[j-0] >> 6) << 4);
///     }
/// }
/// ```
/// `scales` is the 12-byte K_SCALE_SIZE array shared by Q4_K and Q5_K.
#[inline]
fn get_scale_min_k4(j: usize, scales: &[u8]) -> (u8, u8) {
    if j < 4 {
        (scales[j] & 63, scales[j + 4] & 63)
    } else {
        let d = (scales[j + 4] & 0x0F) | ((scales[j - 4] >> 6) << 4);
        let m = (scales[j + 4] >> 4) | ((scales[j] >> 6) << 4);
        (d, m)
    }
}

/// Dequantize a Q5_K tensor (176 bytes / 256 elems per block) to bf16.
///
/// Ports `dequantize_row_q5_K` from llama.cpp's `ggml-quants.c`. The
/// `qh` (high-bit) nibbles and `qs` (low-4-bit) nibbles are combined
/// per element; per-subblock scales and mins come from the 12-byte
/// packed `scales` field via `get_scale_min_k4`.
fn dequant_q5_k_to_bf16(bytes: &[u8], n_elems: usize) -> Result<Vec<bf16>> {
    const BLOCK: usize = 256;
    const BLOCK_BYTES: usize = 176;
    if !n_elems.is_multiple_of(BLOCK) {
        return Err(anyhow!(
            "Q5_K dequant: n_elems {} not a multiple of {}",
            n_elems,
            BLOCK
        ));
    }
    let nb = n_elems / BLOCK;
    if bytes.len() != nb * BLOCK_BYTES {
        return Err(anyhow!(
            "Q5_K dequant: got {} bytes, expected {} ({} blocks)",
            bytes.len(),
            nb * BLOCK_BYTES,
            nb
        ));
    }
    let mut out: Vec<bf16> = Vec::with_capacity(n_elems);
    // Per block: d(2) dmin(2) scales[12] qh[32] qs[128]
    for i in 0..nb {
        let b = &bytes[i * BLOCK_BYTES..(i + 1) * BLOCK_BYTES];
        let d = read_ggml_half(&b[0..2]);
        let min = read_ggml_half(&b[2..4]);
        let scales = &b[4..16];
        let qh = &b[16..48];
        let mut ql_off = 48usize; // qs starts at byte 48

        let mut is = 0usize;
        let mut u1: u8 = 1;
        let mut u2: u8 = 2;
        // Emit 256 elements in groups of 64 (two sub-blocks of 32).
        for _ in (0..BLOCK).step_by(64) {
            let (sc0, m0) = get_scale_min_k4(is, scales);
            let d1 = d * sc0 as f32;
            let m1 = min * m0 as f32;
            let (sc1, m1s) = get_scale_min_k4(is + 1, scales);
            let d2 = d * sc1 as f32;
            let m2 = min * m1s as f32;

            // low nibble + u1 bit from qh
            for l in 0..32 {
                let q = (b[ql_off + l] & 0x0F) as i32
                    + if (qh[l] & u1) != 0 { 16 } else { 0 };
                let v = d1 * (q as f32) - m1;
                out.push(bf16::from_f32(v));
            }
            // high nibble + u2 bit from qh
            for l in 0..32 {
                let q = ((b[ql_off + l] >> 4) & 0x0F) as i32
                    + if (qh[l] & u2) != 0 { 16 } else { 0 };
                let v = d2 * (q as f32) - m2;
                out.push(bf16::from_f32(v));
            }

            ql_off += 32;
            is += 2;
            u1 = u1.wrapping_shl(2);
            u2 = u2.wrapping_shl(2);
        }
    }
    Ok(out)
}

/// Dequantize a Q6_K tensor (210 bytes / 256 elems per block) to bf16.
///
/// Ports `dequantize_row_q6_K`. Each 6-bit quant is reconstructed as
/// `(ql & 0xF) | ((qh >> s) & 3) << 4` and re-centered around zero
/// (`- 32`). Sixteen signed i8 sub-block scales × the super-scale `d`.
fn dequant_q6_k_to_bf16(bytes: &[u8], n_elems: usize) -> Result<Vec<bf16>> {
    const BLOCK: usize = 256;
    const BLOCK_BYTES: usize = 210;
    if !n_elems.is_multiple_of(BLOCK) {
        return Err(anyhow!(
            "Q6_K dequant: n_elems {} not a multiple of {}",
            n_elems,
            BLOCK
        ));
    }
    let nb = n_elems / BLOCK;
    if bytes.len() != nb * BLOCK_BYTES {
        return Err(anyhow!(
            "Q6_K dequant: got {} bytes, expected {} ({} blocks)",
            bytes.len(),
            nb * BLOCK_BYTES,
            nb
        ));
    }
    let mut out: Vec<bf16> = Vec::with_capacity(n_elems);
    for i in 0..nb {
        let b = &bytes[i * BLOCK_BYTES..(i + 1) * BLOCK_BYTES];
        // Layout: ql[128] qh[64] scales[16 i8] d(2)
        let ql = &b[0..128];
        let qh = &b[128..192];
        let sc = &b[192..208]; // i8 via reinterpret below
        let d = read_ggml_half(&b[208..210]);

        // Produce 256 outputs in two passes of 128, matching the
        // reference's outer `for (int n = 0; n < QK_K; n += 128)`.
        // The reference writes `y[l+0/32/64/96]` non-contiguously; we
        // stage one sub-block into a 128-f32 scratch buffer, then copy
        // it out in order as bf16.
        for n in (0..BLOCK).step_by(128) {
            let ql_n = &ql[(n / 2)..(n / 2 + 64)];
            let qh_n = &qh[(n / 4)..(n / 4 + 32)];
            let sc_n = &sc[(n / 16)..(n / 16 + 8)];
            let mut scratch = [0f32; 128];
            for l in 0..32 {
                let is = l / 16;
                // Reference uses `(qh_n[l] >> 0) & 3`; we drop the
                // no-op shift to keep clippy quiet while preserving
                // the shift offsets 0/2/4/6 across the four quants.
                let q1 = ((ql_n[l] & 0x0F) | ((qh_n[l] & 3) << 4)) as i32 - 32;
                let q2 = ((ql_n[l + 32] & 0x0F) | (((qh_n[l] >> 2) & 3) << 4)) as i32 - 32;
                let q3 = ((ql_n[l] >> 4) | (((qh_n[l] >> 4) & 3) << 4)) as i32 - 32;
                let q4 = ((ql_n[l + 32] >> 4) | (((qh_n[l] >> 6) & 3) << 4)) as i32 - 32;
                // `sc` is int8_t in the reference — reinterpret via `as i8`.
                let s0 = sc_n[is] as i8 as f32;
                let s2 = sc_n[is + 2] as i8 as f32;
                let s4 = sc_n[is + 4] as i8 as f32;
                let s6 = sc_n[is + 6] as i8 as f32;
                scratch[l] = d * s0 * (q1 as f32);
                scratch[l + 32] = d * s2 * (q2 as f32);
                scratch[l + 64] = d * s4 * (q3 as f32);
                scratch[l + 96] = d * s6 * (q4 as f32);
            }
            for v in scratch.iter() {
                out.push(bf16::from_f32(*v));
            }
        }
    }
    Ok(out)
}

/// Dequantize a Q8_0 tensor (34 bytes / 32 elems per block) to bf16.
///
/// Ports `dequantize_row_q8_0`. Simplest of the four: per-block scale
/// `d` (fp16) × 32 signed i8 quants.
fn dequant_q8_0_to_bf16(bytes: &[u8], n_elems: usize) -> Result<Vec<bf16>> {
    const BLOCK: usize = 32;
    const BLOCK_BYTES: usize = 34;
    if !n_elems.is_multiple_of(BLOCK) {
        return Err(anyhow!(
            "Q8_0 dequant: n_elems {} not a multiple of {}",
            n_elems,
            BLOCK
        ));
    }
    let nb = n_elems / BLOCK;
    if bytes.len() != nb * BLOCK_BYTES {
        return Err(anyhow!(
            "Q8_0 dequant: got {} bytes, expected {} ({} blocks)",
            bytes.len(),
            nb * BLOCK_BYTES,
            nb
        ));
    }
    let mut out: Vec<bf16> = Vec::with_capacity(n_elems);
    for i in 0..nb {
        let b = &bytes[i * BLOCK_BYTES..(i + 1) * BLOCK_BYTES];
        let d = read_ggml_half(&b[0..2]);
        for j in 0..BLOCK {
            // i8 quants: sign-extended reinterpret of the raw byte.
            let q = b[2 + j] as i8 as f32;
            out.push(bf16::from_f32(q * d));
        }
    }
    Ok(out)
}

/// Dequantize an IQ4_XS tensor (136 bytes / 256 elems per block) to bf16.
///
/// Ports `dequantize_row_iq4_xs`. Each 4-bit quant indexes into the
/// fixed 16-entry `kvalues_iq4nl` codebook; the 6-bit per-sub-block
/// scale is reconstructed from `scales_l` (low 4) and `scales_h`
/// (high 2), then re-centered with `- 32` and scaled by `d`.
fn dequant_iq4_xs_to_bf16(bytes: &[u8], n_elems: usize) -> Result<Vec<bf16>> {
    const BLOCK: usize = 256;
    const BLOCK_BYTES: usize = 136;
    if !n_elems.is_multiple_of(BLOCK) {
        return Err(anyhow!(
            "IQ4_XS dequant: n_elems {} not a multiple of {}",
            n_elems,
            BLOCK
        ));
    }
    let nb = n_elems / BLOCK;
    if bytes.len() != nb * BLOCK_BYTES {
        return Err(anyhow!(
            "IQ4_XS dequant: got {} bytes, expected {} ({} blocks)",
            bytes.len(),
            nb * BLOCK_BYTES,
            nb
        ));
    }
    let mut out: Vec<bf16> = Vec::with_capacity(n_elems);
    for i in 0..nb {
        let b = &bytes[i * BLOCK_BYTES..(i + 1) * BLOCK_BYTES];
        // Layout: d(2) scales_h u16(2) scales_l[4] qs[128]
        let d = read_ggml_half(&b[0..2]);
        let scales_h = u16::from_le_bytes([b[2], b[3]]);
        let scales_l = &b[4..8];
        let qs = &b[8..136];
        for ib in 0..(BLOCK / 32) {
            let ls_low = (scales_l[ib / 2] >> (4 * (ib % 2))) & 0x0F;
            let ls_high = ((scales_h >> (2 * ib)) & 0x03) as u8;
            let ls = (ls_low | (ls_high << 4)) as i32;
            let dl = d * ((ls - 32) as f32);
            // 16 low nibbles then 16 high nibbles, using the codebook.
            let qs_ib = &qs[ib * 16..(ib + 1) * 16];
            let mut scratch = [0f32; 32];
            for j in 0..16 {
                let lo = (qs_ib[j] & 0x0F) as usize;
                let hi = ((qs_ib[j] >> 4) & 0x0F) as usize;
                scratch[j] = dl * (IQ4_NL_KVALUES[lo] as f32);
                scratch[j + 16] = dl * (IQ4_NL_KVALUES[hi] as f32);
            }
            for v in scratch.iter() {
                out.push(bf16::from_f32(*v));
            }
        }
    }
    Ok(out)
}

#[allow(dead_code)]
const _QWEN35_METADATA_SEPARATOR: () = ();

// ────────────────────────────────────────────────────────────────────
// Qwen3.5 metadata parser.
//
// Walks only the header + metadata KV section (never touches tensor
// descriptors or data), extracting the handful of numeric keys we need
// to build a `Qwen35Config` without patching it by hand. The canonical
// Qwen3.5 GGUF keys match llama.cpp's naming convention: prefixed with
// `qwen35.` rather than `qwen3.` — the hybrid-SSM variant was merged
// under its own arch label upstream.
// ────────────────────────────────────────────────────────────────────

/// Parsed numeric metadata from a Qwen3.5 GGUF header.
///
/// Populated from the file's `qwen35.*` keys (falling back to sentinel
/// values the caller can assert against when a key is absent). These
/// are the seven values [`crate::models::qwen35::Qwen35Config::from_metadata`]
/// needs to drive the whole model — head counts, embedding/ffn widths,
/// the RoPE base, the RMSNorm epsilon, and the maximum context length.
#[derive(Debug, Clone, Copy)]
pub struct Qwen35Metadata {
    /// `qwen35.block_count` — number of decoder layers (64 on 27B).
    pub block_count: usize,
    /// `qwen35.embedding_length` — hidden_dim (5120 on 27B).
    pub embedding_length: usize,
    /// `qwen35.attention.head_count` — n_q_heads (24 on 27B).
    pub head_count: usize,
    /// `qwen35.attention.head_count_kv` — n_kv_heads (4 on 27B).
    pub head_count_kv: usize,
    /// `qwen35.rope.freq_base` — RoPE base theta (10_000_000 on 27B).
    pub rope_theta: f32,
    /// `qwen35.attention.layer_norm_rms_epsilon` — RMSNorm eps (1e-6).
    pub rms_eps: f32,
    /// `qwen35.context_length` — max positions.
    pub context_length: usize,
    /// `qwen35.feed_forward_length` — FFN intermediate width (17408 on 27B).
    pub feed_forward_length: usize,
    /// `qwen35.attention.key_length` — per-head K feature width (256 on 27B).
    /// Equal to `qwen35.attention.value_length` in all shipping Qwen3.5
    /// variants, and equals `head_dim` in the typed config.
    pub key_length: usize,
    /// `qwen35.attention.value_length` — per-head V feature width (256 on 27B).
    pub value_length: usize,
}

/// Read-only GGUF header walker: opens the file, parses header +
/// metadata section, and captures the Qwen3.5 scalar keys. Does not
/// touch the tensor section or allocate device memory.
///
/// Returns `Err` if:
///   * the file isn't a GGUF v3 container,
///   * a required Qwen3.5 key is missing (`embedding_length`,
///     `block_count`, `attention.head_count`, `attention.head_count_kv`,
///     `attention.key_length`),
///   * a captured value has an unexpected GGUF scalar type (e.g.
///     `head_count` stored as f32).
///
/// Soft-fallback keys (`rope.freq_base`, `layer_norm_rms_epsilon`,
/// `context_length`, `feed_forward_length`, `attention.value_length`)
/// default to sensible 27B values when absent so the caller can still
/// construct a config for older builds.
pub fn parse_qwen35_metadata<P: AsRef<Path>>(path: P) -> Result<Qwen35Metadata> {
    let path = path.as_ref();
    let file = File::open(path).with_context(|| format!("open gguf file {}", path.display()))?;
    // SAFETY: read-only mmap; we don't hold the mapping past this fn.
    let mmap = unsafe { Mmap::map(&file) }
        .with_context(|| format!("mmap gguf file {}", path.display()))?;
    let mut cur = Cursor::new(&mmap);

    // Header: magic, version, tensor_count, metadata_kv_count. Same
    // layout as `load_gguf_impl`; we re-parse rather than share because
    // the tensor loader is pass-through over a `DeviceContext` we don't
    // want to require for a metadata-only read.
    let magic = cur.read_u32()?;
    if magic != GGUF_MAGIC {
        return Err(anyhow!(
            "not a gguf file: magic=0x{:08x} (expected 0x{:08x})",
            magic,
            GGUF_MAGIC
        ));
    }
    let version = cur.read_u32()?;
    if version != GGUF_VERSION {
        return Err(anyhow!(
            "unsupported gguf version {} (expected {})",
            version,
            GGUF_VERSION
        ));
    }
    let _tensor_count = cur.read_u64()?;
    let metadata_kv_count = cur.read_u64()? as usize;

    // Key buckets. Using `Option` so we can distinguish "absent → apply
    // fallback" from "present but wrong type → hard error".
    let mut block_count: Option<usize> = None;
    let mut embedding_length: Option<usize> = None;
    let mut head_count: Option<usize> = None;
    let mut head_count_kv: Option<usize> = None;
    let mut rope_theta: Option<f32> = None;
    let mut rms_eps: Option<f32> = None;
    let mut context_length: Option<usize> = None;
    let mut feed_forward_length: Option<usize> = None;
    let mut key_length: Option<usize> = None;
    let mut value_length: Option<usize> = None;

    for _ in 0..metadata_kv_count {
        let key = cur.read_string()?;
        let ty = GgufValueType::from_u32(cur.read_u32()?)?;
        match key.as_str() {
            "qwen35.block_count" => {
                block_count = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.embedding_length" => {
                embedding_length = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.attention.head_count" => {
                head_count = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.attention.head_count_kv" => {
                head_count_kv = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.context_length" => {
                context_length = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.feed_forward_length" => {
                feed_forward_length = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.attention.key_length" => {
                key_length = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.attention.value_length" => {
                value_length = Some(expect_u32_key(&mut cur, &key, ty)? as usize);
            }
            "qwen35.rope.freq_base" => {
                rope_theta = Some(expect_f32_key(&mut cur, &key, ty)?);
            }
            "qwen35.attention.layer_norm_rms_epsilon" => {
                rms_eps = Some(expect_f32_key(&mut cur, &key, ty)?);
            }
            _ => cur.skip_value(ty)?,
        }
    }

    // Required keys — a missing one means this isn't a Qwen3.5 GGUF (or
    // an upstream format skew we should flag rather than paper over).
    let embedding_length = embedding_length
        .ok_or_else(|| anyhow!("qwen35 metadata: missing qwen35.embedding_length"))?;
    let block_count =
        block_count.ok_or_else(|| anyhow!("qwen35 metadata: missing qwen35.block_count"))?;
    let head_count = head_count
        .ok_or_else(|| anyhow!("qwen35 metadata: missing qwen35.attention.head_count"))?;
    let head_count_kv = head_count_kv
        .ok_or_else(|| anyhow!("qwen35 metadata: missing qwen35.attention.head_count_kv"))?;
    let key_length = key_length
        .ok_or_else(|| anyhow!("qwen35 metadata: missing qwen35.attention.key_length"))?;

    // Soft-fallback keys. Defaults come from the reference 27B build
    // (dflash's qwen35_target_graph.cpp constants).
    let rope_theta = rope_theta.unwrap_or(10_000_000.0);
    let rms_eps = rms_eps.unwrap_or(1e-6);
    let context_length = context_length.unwrap_or(131_072);
    let feed_forward_length = feed_forward_length.unwrap_or(17_408);
    let value_length = value_length.unwrap_or(key_length);

    Ok(Qwen35Metadata {
        block_count,
        embedding_length,
        head_count,
        head_count_kv,
        rope_theta,
        rms_eps,
        context_length,
        feed_forward_length,
        key_length,
        value_length,
    })
}

/// Helper: read a metadata value we expect to be u32. Any other ggml
/// scalar kind is rejected with the key name attached so the error is
/// debuggable (some builds store counts as i32; we don't auto-coerce).
fn expect_u32_key(cur: &mut Cursor<'_>, key: &str, ty: GgufValueType) -> Result<u32> {
    match ty {
        GgufValueType::U32 => cur.read_u32(),
        other => {
            cur.skip_value(other)?;
            Err(anyhow!(
                "qwen35 metadata: key {} has type {:?}, expected U32",
                key,
                other
            ))
        }
    }
}

/// Helper: read a metadata value we expect to be f32. Strict variant of
/// `read_value_maybe_u32`, scoped to this metadata parser.
fn expect_f32_key(cur: &mut Cursor<'_>, key: &str, ty: GgufValueType) -> Result<f32> {
    match ty {
        GgufValueType::F32 => cur.read_f32(),
        other => {
            cur.skip_value(other)?;
            Err(anyhow!(
                "qwen35 metadata: key {} has type {:?}, expected F32",
                key,
                other
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_correctly() {
        assert_eq!(align_up(0, 32), 0);
        assert_eq!(align_up(1, 32), 32);
        assert_eq!(align_up(32, 32), 32);
        assert_eq!(align_up(33, 32), 64);
        assert_eq!(align_up(63, 32), 64);
        assert_eq!(align_up(64, 32), 64);
    }

    #[test]
    fn ggml_type_roundtrip_known_codes() {
        assert_eq!(GgmlType::from_u32(0), GgmlType::F32);
        assert_eq!(GgmlType::from_u32(1), GgmlType::F16);
        assert_eq!(GgmlType::from_u32(8), GgmlType::Q8_0);
        assert_eq!(GgmlType::from_u32(12), GgmlType::Q4K);
        assert_eq!(GgmlType::from_u32(13), GgmlType::Q5K);
        assert_eq!(GgmlType::from_u32(14), GgmlType::Q6K);
        assert_eq!(GgmlType::from_u32(23), GgmlType::IQ4XS);
        assert_eq!(GgmlType::from_u32(24), GgmlType::I8);
        assert_eq!(GgmlType::from_u32(26), GgmlType::I32);
        assert_eq!(GgmlType::from_u32(30), GgmlType::BF16);
        matches!(GgmlType::from_u32(9999), GgmlType::Unknown(9999));
    }

    /// Q8_0 dequant is the simplest of the four: for each 34-byte
    /// block, y[i] = d * qs[i] where `d` is the fp16 super-scale. We
    /// synthesize a single block with known `d` and a linear ramp of
    /// int8 quants, then check the bf16 output matches `d * q`.
    #[test]
    fn dequant_q8_0_single_block_matches_reference() {
        // Pick a clean fp16-representable d = 0.125 (= 2^-3).
        let d = f16::from_f32(0.125f32);
        let mut block = Vec::with_capacity(34);
        block.extend_from_slice(&d.to_bits().to_le_bytes());
        // q = -16..16 ramp, as i8 bytes.
        for i in 0..32i32 {
            let q = (i - 16) as i8;
            block.push(q as u8);
        }
        let out = dequant_q8_0_to_bf16(&block, 32).expect("q8_0 dequant");
        assert_eq!(out.len(), 32);
        for i in 0..32 {
            let q = (i as i32) - 16;
            let expected = bf16::from_f32(0.125f32 * q as f32);
            assert_eq!(
                out[i].to_bits(),
                expected.to_bits(),
                "mismatch at i={}: got {}, expected {}",
                i,
                out[i].to_f32(),
                expected.to_f32()
            );
        }
    }

    /// Length and shape invariants for each dequant path on a trivial
    /// all-zero block. A zero block should produce all-zero outputs
    /// regardless of format-specific scale bits (since every `d` and
    /// every `sc` ends up multiplied into zero quants).
    #[test]
    fn dequant_all_zero_blocks_produce_all_zero_bf16() {
        // Q5_K: 176 B / 256 elems.
        let zeros = vec![0u8; 176];
        let out = dequant_q5_k_to_bf16(&zeros, 256).expect("q5_k dequant");
        assert_eq!(out.len(), 256);
        assert!(out.iter().all(|v| v.to_f32() == 0.0));

        // Q6_K: 210 B / 256 elems.
        let zeros = vec![0u8; 210];
        let out = dequant_q6_k_to_bf16(&zeros, 256).expect("q6_k dequant");
        assert_eq!(out.len(), 256);
        assert!(out.iter().all(|v| v.to_f32() == 0.0));

        // Q8_0: 34 B / 32 elems.
        let zeros = vec![0u8; 34];
        let out = dequant_q8_0_to_bf16(&zeros, 32).expect("q8_0 dequant");
        assert_eq!(out.len(), 32);
        assert!(out.iter().all(|v| v.to_f32() == 0.0));

        // IQ4_XS: 136 B / 256 elems.
        let zeros = vec![0u8; 136];
        let out = dequant_iq4_xs_to_bf16(&zeros, 256).expect("iq4_xs dequant");
        assert_eq!(out.len(), 256);
        // Note: iq4_xs uses `(ls - 32)` as the effective subblock
        // scale, so an all-zero block gives dl = d * -32 * codebook[0]
        // — but d=0 since `d` is the 2-byte fp16 super-scale that is
        // zero. So outputs are still all zero.
        assert!(out.iter().all(|v| v.to_f32() == 0.0));
    }

    /// Block-size / length validation: pass mismatched byte lengths
    /// and make sure each dequant path returns an error instead of
    /// silently truncating.
    #[test]
    fn dequant_rejects_mismatched_lengths() {
        assert!(dequant_q5_k_to_bf16(&vec![0u8; 100], 256).is_err());
        assert!(dequant_q5_k_to_bf16(&vec![0u8; 176], 257).is_err()); // not div 256
        assert!(dequant_q6_k_to_bf16(&vec![0u8; 100], 256).is_err());
        assert!(dequant_q6_k_to_bf16(&vec![0u8; 210], 200).is_err());
        assert!(dequant_q8_0_to_bf16(&vec![0u8; 30], 32).is_err());
        assert!(dequant_q8_0_to_bf16(&vec![0u8; 34], 31).is_err()); // not div 32
        assert!(dequant_iq4_xs_to_bf16(&vec![0u8; 100], 256).is_err());
        assert!(dequant_iq4_xs_to_bf16(&vec![0u8; 136], 128).is_err());
    }

    /// `get_scale_min_k4` is the 12-byte packed-scale decoder shared
    /// by Q4_K and Q5_K. Ported verbatim from `ggml-quants.c`; verify
    /// the branch structure on a crafted scales array with bits set
    /// in positions that exercise both the `j < 4` (low) branch and
    /// the `j >= 4` (high, cross-referenced) branch.
    #[test]
    fn get_scale_min_k4_low_and_high_branches() {
        // Build a `scales[12]` array where:
        //   q[0] = 0b00_100011  (lower 6 = 35, upper 2 = 0)
        //   q[4] = 0b00_000111  (lower 6 =  7, upper 2 = 0)
        // Expectation for j=0: d = 0x23 & 63 = 35, m = q[4] & 63 = 7.
        // For j=4 (high branch):
        //   d = (q[8] & 0xF) | ((q[0] >> 6) << 4) = (q[8] & 0xF) | 0
        //   m = (q[8] >> 4)   | ((q[4] >> 6) << 4) = (q[8] >> 4) | 0
        let mut q = [0u8; 12];
        q[0] = 0b0010_0011;
        q[4] = 0b0000_0111;
        q[8] = 0b1010_1100;
        let (d, m) = get_scale_min_k4(0, &q);
        assert_eq!(d, 35);
        assert_eq!(m, 7);
        let (d4, m4) = get_scale_min_k4(4, &q);
        assert_eq!(d4, q[8] & 0x0F);
        assert_eq!(m4, q[8] >> 4);

        // Add non-zero top-2 bits in q[0] and q[4] to cover the
        // cross-reference path:
        //   q[0] = 0b11_100011  (top-2 = 3)
        //   q[4] = 0b01_000111  (top-2 = 1)
        q[0] = 0b1110_0011;
        q[4] = 0b0100_0111;
        let (d4, m4) = get_scale_min_k4(4, &q);
        assert_eq!(d4, (q[8] & 0x0F) | ((q[0] >> 6) << 4));
        assert_eq!(m4, (q[8] >> 4) | ((q[4] >> 6) << 4));
    }

    /// IQ4_XS codebook sanity: verify the first and last kvalues
    /// entries match the reference table in `ggml-common.h`.
    #[test]
    fn iq4_nl_codebook_matches_reference() {
        assert_eq!(IQ4_NL_KVALUES[0], -127);
        assert_eq!(IQ4_NL_KVALUES[8], 1);
        assert_eq!(IQ4_NL_KVALUES[15], 113);
    }

    /// Integration test: parses the 27B Q4_K_M GGUF on the A6000 host
    /// and checks tensor count, a known tensor's shape, and the
    /// byte-length invariant for a Q4_K block-packed weight. Also
    /// spot-checks the CPU-dequant paths for Q5_K / Q6_K / Q8_0 /
    /// IQ4_XS: counts per dtype must be positive and the raw bf16
    /// values can't all be zero.
    ///
    /// Ignored by default — requires the file to exist and a working
    /// CUDA device.
    #[test]
    #[ignore]
    fn load_gguf_27b_q4km_smoke() {
        let path = "/home/metricspace/dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf";
        let device = Arc::new(DeviceContext::new(0).expect("init CUDA device 0"));
        let load = load_gguf_lenient(&device, path).expect("load gguf");

        eprintln!(
            "parsed {} total descriptors, uploaded {} tensors, skipped {} unsupported",
            load.total_descriptors,
            load.tensors.len(),
            load.unsupported.len()
        );
        for (name, ty) in &load.unsupported {
            eprintln!("  unsupported: {} ({})", name, ty);
        }
        assert_eq!(
            load.total_descriptors, 851,
            "expected 851 tensor descriptors in 27B Q4_K_M"
        );
        assert_eq!(
            load.tensors.len() + load.unsupported.len(),
            load.total_descriptors,
            "tensors + unsupported should sum to total"
        );
        assert_eq!(
            load.unsupported.len(),
            0,
            "expected 0 unsupported tensors once Q5K/Q6K/Q8_0/IQ4_XS \
             dequant is in; got {}",
            load.unsupported.len()
        );
        assert_eq!(
            load.tensors.len(),
            851,
            "expected all 851 tensors to be loaded"
        );

        // Spot check 1: token embedding shape. The Qwen3.5-27B model
        // in /home/metricspace has vocab 248320; hidden dim should be
        // 5120. (An earlier Qwen build had vocab 151936; we check only
        // the hidden-dim invariant to stay robust to vocab resizes.)
        let tok_embd = load
            .tensors
            .get("token_embd.weight")
            .expect("token_embd.weight not found");
        eprintln!(
            "token_embd.weight: dtype={:?} shape={:?}",
            tok_embd.dtype, tok_embd.shape
        );
        assert_eq!(
            tok_embd.shape.len(),
            2,
            "token_embd.weight should be 2-D, got {:?}",
            tok_embd.shape
        );
        assert_eq!(
            tok_embd.shape[1], 5120,
            "token_embd.weight hidden-dim {} != 5120",
            tok_embd.shape[1]
        );
        assert!(
            tok_embd.shape[0] > 0,
            "token_embd.weight vocab dim must be positive"
        );

        // Spot check 2: per-dtype tensor counts. The Qwen3.5-27B
        // Q4_K_M mixture uses Q4_K as the bulk format plus Q5_K /
        // Q6_K / Q8_0 / IQ4_XS for select weights; each count must be
        // positive.
        let mut counts: std::collections::HashMap<DType, usize> =
            std::collections::HashMap::new();
        for t in load.tensors.values() {
            *counts.entry(t.dtype).or_insert(0) += 1;
        }
        let mut kv: Vec<(DType, usize)> = counts.iter().map(|(k, v)| (*k, *v)).collect();
        kv.sort_by_key(|(d, _)| format!("{:?}", d));
        eprintln!("dtype breakdown:");
        for (d, c) in &kv {
            eprintln!("  {:?}: {}", d, c);
        }
        // Q4K / Q5K / Q6K / Q8_0 must be present in this model's
        // Q4_K_M mixture. IQ4_XS is NOT present in this particular
        // 27B build — llama.cpp only emits IQ4_XS for smaller/denser
        // mixtures. We still test the IQ4_XS dequant code in unit
        // tests; here we just assert count >= 0 and log it.
        for (dt, label) in [
            (DType::Q4K, "Q4K"),
            (DType::Q5K, "Q5K"),
            (DType::Q6K, "Q6K"),
            (DType::Q8_0, "Q8_0"),
        ] {
            let n = counts.get(&dt).copied().unwrap_or(0);
            assert!(
                n > 0,
                "expected at least one {} tensor in the 27B Q4_K_M file",
                label
            );
        }
        let iq4_xs_count = counts.get(&DType::IQ4XS).copied().unwrap_or(0);
        eprintln!(
            "IQ4_XS tensors: {} (informational — this model variant may not use them)",
            iq4_xs_count
        );

        // Spot check 3: Q4K byte-count invariant on a deterministic
        // sample (sort by name so the output is stable across hashmap
        // iteration).
        let mut q4k_samples: Vec<(&String, &GgufTensor)> = load
            .tensors
            .iter()
            .filter(|(_, t)| t.dtype == DType::Q4K)
            .collect();
        q4k_samples.sort_by_key(|(n, _)| (*n).clone());
        let (q_name, q) = q4k_samples[0];
        eprintln!("spot-check Q4K tensor {}: shape={:?}", q_name, q.shape);
        let n_elems: usize = q.shape.iter().product();
        assert!(
            n_elems % 256 == 0,
            "Q4K tensor {} n_elems {} not divisible by 256",
            q_name,
            n_elems
        );
        let expected_bytes = (n_elems / 256) * 144;
        match q.buf {
            GgufBuf::Q4K(ref t) => {
                assert_eq!(
                    t.numel(),
                    expected_bytes,
                    "Q4K byte count mismatch for {}",
                    q_name
                );
            }
            _ => panic!("{} marked Q4K but buf variant is wrong", q_name),
        }

        // Spot check 4: `output.weight` — Qwen3.5-27B's output head is
        // Q6_K in the Q4_K_M mixture. It should now be loaded as bf16
        // (dtype tag = Q6K) and should not be all zeros.
        let output = load
            .tensors
            .get("output.weight")
            .expect("output.weight not found");
        eprintln!(
            "output.weight: dtype={:?} shape={:?}",
            output.dtype, output.shape
        );
        assert_eq!(
            output.dtype,
            DType::Q6K,
            "output.weight expected Q6K, got {:?}",
            output.dtype
        );
        match output.buf {
            GgufBuf::Bf16(ref t) => {
                // Download the first 1024 elements to check non-zero.
                let host = t.to_host().expect("download output.weight");
                let head: Vec<f32> = host.iter().take(1024).map(|v| v.to_f32()).collect();
                let nonzero = head.iter().filter(|v| **v != 0.0).count();
                let max_abs = head.iter().fold(0f32, |a, v| a.max(v.abs()));
                eprintln!(
                    "output.weight head: nonzero={}/1024 max_abs={}",
                    nonzero, max_abs
                );
                assert!(
                    nonzero > 0,
                    "output.weight first 1024 elems are all zero \
                     — Q6K dequant likely broken"
                );
                assert!(
                    max_abs > 0.0,
                    "output.weight first 1024 elems have zero max_abs"
                );
            }
            _ => panic!("output.weight marked Q6K but buf variant is not Bf16"),
        }

        // Spot check 5: find a Q8_0 tensor and confirm it dequantized
        // to bf16 with non-zero values. ssm_alpha-style weights in
        // this Qwen variant are commonly stored as Q8_0; fall back to
        // any Q8_0 tensor if the specific name isn't present.
        let mut q8_0_samples: Vec<(&String, &GgufTensor)> = load
            .tensors
            .iter()
            .filter(|(_, t)| t.dtype == DType::Q8_0)
            .collect();
        q8_0_samples.sort_by_key(|(n, _)| (*n).clone());
        let (q8_name, q8) = q8_0_samples[0];
        eprintln!("spot-check Q8_0 tensor {}: shape={:?}", q8_name, q8.shape);
        match q8.buf {
            GgufBuf::Bf16(ref t) => {
                let host = t.to_host().expect("download q8_0 sample");
                let take = host.len().min(512);
                let head: Vec<f32> = host.iter().take(take).map(|v| v.to_f32()).collect();
                let nonzero = head.iter().filter(|v| **v != 0.0).count();
                let max_abs = head.iter().fold(0f32, |a, v| a.max(v.abs()));
                eprintln!(
                    "q8_0 {} head: nonzero={}/{} max_abs={}",
                    q8_name, nonzero, take, max_abs
                );
                assert!(
                    nonzero > 0,
                    "Q8_0 tensor {} first {} elems all zero — dequant broken",
                    q8_name,
                    take
                );
            }
            _ => panic!("{} marked Q8_0 but buf variant is not Bf16", q8_name),
        }
    }

    /// Phase-6 bring-up: load the 27B Q4_K_M with `keep_packed = true`
    /// and assert that (a) all tensors land without OOM on a 48 GB
    /// A6000, (b) the Q5K/Q6K/Q8_0 tensors surface as the new packed
    /// [`GgufBuf`] variants (not dequanted to bf16), and (c) their byte
    /// sizes match the on-disk block-byte calculation.
    ///
    /// This test proves the VRAM-doubling regression from Phase 5 is
    /// gone: with the bf16-dequant path, this load was ~30 GB of
    /// resident weights and OOMed on a 48 GB A6000 under the smoke
    /// test's extra KV/GDN-state allocations. With `keep_packed`, the
    /// resident weights match the GGUF-on-disk size (~15 GB for 27B
    /// Q4_K_M), leaving ~30 GB of headroom.
    ///
    /// Ignored by default — requires the file to exist and a working
    /// CUDA device.
    ///
    /// Run with:
    ///   cargo test -p ctox-engine-cuda --features cuda --release -- \
    ///     --ignored --nocapture load_gguf_27b_packed_no_oom
    #[test]
    #[ignore]
    fn load_gguf_27b_packed_no_oom() {
        let path = "/home/metricspace/dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf";
        let device = Arc::new(DeviceContext::new(0).expect("init CUDA device 0"));
        let cfg = LoaderConfig { keep_packed: true };
        let load = load_gguf_lenient_with_config(&device, path, cfg).expect("load gguf packed");

        eprintln!(
            "packed load: parsed {} total descriptors, uploaded {} tensors, skipped {} unsupported",
            load.total_descriptors,
            load.tensors.len(),
            load.unsupported.len()
        );
        for (name, ty) in &load.unsupported {
            eprintln!("  unsupported: {} ({})", name, ty);
        }
        assert_eq!(
            load.total_descriptors, 851,
            "expected 851 tensor descriptors in 27B Q4_K_M"
        );
        assert_eq!(
            load.unsupported.len(),
            0,
            "expected 0 unsupported tensors with packed loader"
        );
        assert_eq!(
            load.tensors.len(),
            851,
            "expected all 851 tensors to be loaded"
        );

        // Per-variant counts. Packed variants should replace the
        // bf16-dequant path for Q5K/Q6K/Q8_0/IQ4_XS.
        let mut n_q4k = 0usize;
        let mut n_q5k_packed = 0usize;
        let mut n_q6k_packed = 0usize;
        let mut n_q80_packed = 0usize;
        let mut n_iq4xs_packed = 0usize;
        let mut n_bf16 = 0usize;
        let mut n_f32 = 0usize;
        let mut n_f16 = 0usize;
        let mut n_i32 = 0usize;
        let mut n_i8 = 0usize;
        for t in load.tensors.values() {
            match &t.buf {
                GgufBuf::Q4K(_) => n_q4k += 1,
                GgufBuf::Q5K(_) => n_q5k_packed += 1,
                GgufBuf::Q6K(_) => n_q6k_packed += 1,
                GgufBuf::Q8_0(_) => n_q80_packed += 1,
                GgufBuf::IQ4XS(_) => n_iq4xs_packed += 1,
                GgufBuf::Bf16(_) => n_bf16 += 1,
                GgufBuf::F32(_) => n_f32 += 1,
                GgufBuf::F16(_) => n_f16 += 1,
                GgufBuf::I32(_) => n_i32 += 1,
                GgufBuf::I8(_) => n_i8 += 1,
            }
        }
        eprintln!(
            "packed variant counts: Q4K={} Q5K={} Q6K={} Q8_0={} IQ4XS={} \
             Bf16={} F32={} F16={} I32={} I8={}",
            n_q4k,
            n_q5k_packed,
            n_q6k_packed,
            n_q80_packed,
            n_iq4xs_packed,
            n_bf16,
            n_f32,
            n_f16,
            n_i32,
            n_i8,
        );

        // The 27B Q4_K_M mixture contains Q4K/Q5K/Q6K/Q8_0 tensors;
        // IQ4_XS isn't in this file (see the Phase-5 smoke for the same
        // assertion). With `keep_packed = true` the Q5K/Q6K/Q8_0
        // buffers must land as the packed variants, not as bf16.
        assert!(n_q4k > 0, "expected at least one Q4K tensor");
        assert!(
            n_q5k_packed > 0,
            "expected Q5K tensors to land as GgufBuf::Q5K with keep_packed=true, got 0"
        );
        assert!(
            n_q6k_packed > 0,
            "expected Q6K tensors to land as GgufBuf::Q6K with keep_packed=true, got 0"
        );
        assert!(
            n_q80_packed > 0,
            "expected Q8_0 tensors to land as GgufBuf::Q8_0 with keep_packed=true, got 0"
        );

        // Sample a packed Q5K tensor and confirm its on-device byte
        // length matches the expected block-bytes-for-elements value.
        let (sample_name, sample_t) = load
            .tensors
            .iter()
            .find(|(_, t)| matches!(t.buf, GgufBuf::Q5K(_)))
            .expect("at least one Q5K tensor");
        let n_elems: usize = sample_t.shape.iter().product();
        let expected_bytes = DType::Q5K.block_bytes_for_elements(n_elems);
        if let GgufBuf::Q5K(bytes) = &sample_t.buf {
            eprintln!(
                "spot-check Q5K {}: shape={:?} n_elems={} packed_bytes={} (expected {})",
                sample_name,
                sample_t.shape,
                n_elems,
                bytes.numel(),
                expected_bytes,
            );
            assert_eq!(
                bytes.numel(),
                expected_bytes,
                "Q5K packed byte count for {} mismatch",
                sample_name
            );
        }

        // Device sync — if any upload silently overcommitted, the
        // previous calls already would have returned an error. A
        // successful return here is the "no OOM" assertion.
        device.synchronize().expect("synchronize after packed load");
    }
}
