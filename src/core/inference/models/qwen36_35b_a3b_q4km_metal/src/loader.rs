// Origin: CTOX
// License: Apache-2.0

//! GGUF v3 reader.
//!
//! Implements the bare minimum needed to load Qwen3.6-35B-A3B Q4_K_M
//! from a single GGUF file: magic + version, scalar/string/array KV
//! metadata, the tensor table (name, shape, dtype, file offset), and
//! mmap-based access to tensor blobs. There is **no link against
//! libggml** — this is a pure-Rust port of the GGUF v3 binary spec
//! (ref: <https://github.com/ggml-org/ggml/blob/master/docs/gguf.md>),
//! cross-checked against `llama.cpp`'s `gguf.cpp` at the commit pinned
//! in `vendor/llama-cpp.version`.
//!
//! What is intentionally NOT implemented yet (deferred to stage 3):
//!
//! - Per-tensor *dequant* paths. The loader hands back tensor blobs
//!   in their on-disk layout. Q4_K_M dequant lives in the matmul
//!   kernel ports (`metal_port::ops::mul_mat_q4_k_m`, stage 3) and
//!   reads tensor bytes directly off mmap.
//! - Streaming/chunked load. The 21 GiB Q4_K_M fits in 32 GiB unified
//!   memory comfortably; mmap with `MAP_PRIVATE` is enough for now.
//! - Multi-file shards. Qwen3.6-35B-A3B Q4_K_M is expected as a
//!   single-file GGUF; if community shards arrive the loader gains a
//!   `gguf-split` index pass.

use std::fs::File;
use std::path::{Path, PathBuf};

use memmap2::Mmap;
use thiserror::Error;

use crate::model::Qwen36MoeTextConfig;

/// GGUF v3 magic bytes — `b"GGUF"`.
const GGUF_MAGIC: &[u8; 4] = b"GGUF";
/// We only support GGUF version 3 — the current llama.cpp/ggml release
/// format. v1/v2 are pre-2024 and we don't need them.
const GGUF_VERSION_SUPPORTED: u32 = 3;

/// Tensor element type tag, byte-compatible with `enum ggml_type` in
/// `vendor/ggml-metal/ggml-common.h`. Only the dtypes we actually need
/// for Qwen3.6-35B-A3B Q4_K_M are populated; everything else maps to
/// [`GgmlType::Unsupported`] so the loader rejects unexpected tensors
/// loudly rather than silently mis-decoding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum GgmlType {
    F32,
    F16,
    Q4_K,
    Q6_K,
    BF16,
    Unsupported(u32),
}

impl GgmlType {
    /// Numeric tag used in the GGUF tensor table. Matches `enum
    /// ggml_type` in upstream's `ggml.h`.
    pub const fn tag(self) -> u32 {
        match self {
            Self::F32 => 0,
            Self::F16 => 1,
            Self::Q4_K => 12,
            Self::Q6_K => 14,
            Self::BF16 => 30,
            Self::Unsupported(raw) => raw,
        }
    }

    fn from_u32(raw: u32) -> Self {
        match raw {
            0 => Self::F32,
            1 => Self::F16,
            12 => Self::Q4_K,
            14 => Self::Q6_K,
            30 => Self::BF16,
            other => Self::Unsupported(other),
        }
    }
}

/// GGUF metadata value type tag (matches the v3 spec table).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
#[allow(non_camel_case_types)]
enum GgufType {
    UInt8 = 0,
    Int8 = 1,
    UInt16 = 2,
    Int16 = 3,
    UInt32 = 4,
    Int32 = 5,
    Float32 = 6,
    Bool = 7,
    String = 8,
    Array = 9,
    UInt64 = 10,
    Int64 = 11,
    Float64 = 12,
}

impl GgufType {
    fn from_u32(raw: u32) -> Result<Self, LoadError> {
        Ok(match raw {
            0 => Self::UInt8,
            1 => Self::Int8,
            2 => Self::UInt16,
            3 => Self::Int16,
            4 => Self::UInt32,
            5 => Self::Int32,
            6 => Self::Float32,
            7 => Self::Bool,
            8 => Self::String,
            9 => Self::Array,
            10 => Self::UInt64,
            11 => Self::Int64,
            12 => Self::Float64,
            other => return Err(LoadError::UnknownGgufType(other)),
        })
    }
}

/// Decoded metadata value. Strings own their bytes; arrays carry the
/// element-type tag and a flat byte buffer the caller can re-walk.
#[derive(Clone, Debug)]
pub enum MetaValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    String(String),
    /// Array of homogeneous values. The bytes are kept in their on-disk
    /// representation; the element-type tag tells callers how to walk
    /// them. This avoids materialising e.g. a `Vec<String>` for the
    /// 248_320-entry vocab tokens array on every load.
    Array {
        element_type: u32,
        count: u64,
        raw: Vec<u8>,
    },
    U64(u64),
    I64(i64),
    F64(f64),
}

/// One tensor entry in the GGUF tensor table. `data_offset` is
/// relative to the start of the *tensor data segment*, not the start
/// of the file — see the GGUF v3 spec.
#[derive(Clone, Debug)]
pub struct TensorRecord {
    pub name: String,
    pub shape: Vec<u64>,
    pub dtype: GgmlType,
    pub data_offset: u64,
}

impl TensorRecord {
    /// Element count = product of shape dims.
    pub fn elements(&self) -> u64 {
        self.shape.iter().copied().fold(1u64, u64::saturating_mul)
    }

    /// On-disk byte length of this tensor's data blob, computed from
    /// shape + dtype using the same block sizes as
    /// `vendor/ggml-metal/ggml-common.h`.
    pub fn byte_length(&self) -> Result<u64, LoadError> {
        let n = self.elements();
        Ok(match self.dtype {
            GgmlType::F32 => n * 4,
            GgmlType::F16 => n * 2,
            GgmlType::BF16 => n * 2,
            GgmlType::Q4_K => {
                if n % 256 != 0 {
                    return Err(LoadError::Q4KShape {
                        name: self.name.clone(),
                        elements: n,
                    });
                }
                // sizeof(block_q4_K) = 2*sizeof(half) + K_SCALE_SIZE + QK_K/2
                //                    = 4 + 12 + 128 = 144
                (n / 256) * 144
            }
            GgmlType::Q6_K => {
                if n % 256 != 0 {
                    return Err(LoadError::Q4KShape {
                        name: self.name.clone(),
                        elements: n,
                    });
                }
                // sizeof(block_q6_K) = sizeof(half) + QK_K/16 + 3*QK_K/4
                //                    = 2 + 16 + 192 = 210
                (n / 256) * 210
            }
            GgmlType::Unsupported(t) => {
                return Err(LoadError::UnsupportedDtype {
                    name: self.name.clone(),
                    raw: t,
                })
            }
        })
    }
}

/// Loaded GGUF + memory map. Holds borrowed mmap bytes; callers
/// access tensors by name through [`LoadedWeights::tensor_bytes`].
pub struct LoadedWeights {
    pub config: Qwen36MoeTextConfig,
    pub path: PathBuf,
    pub metadata: Vec<(String, MetaValue)>,
    pub tensors: Vec<TensorRecord>,
    /// Absolute file offset where the tensor data segment starts.
    /// Per-tensor `data_offset` is relative to this.
    pub data_offset: u64,
    pub mmap: Mmap,
}

impl LoadedWeights {
    /// Look up a tensor by exact name. Returns `None` if absent.
    pub fn tensor(&self, name: &str) -> Option<&TensorRecord> {
        self.tensors.iter().find(|t| t.name == name)
    }

    /// Borrow the tensor's on-disk bytes. The returned slice lives as
    /// long as `self` and is laid out in GGUF order — for Q4_K_M that
    /// is a tightly packed array of 144-byte super-blocks.
    pub fn tensor_bytes(&self, name: &str) -> Result<&[u8], LoadError> {
        let t = self
            .tensor(name)
            .ok_or_else(|| LoadError::MissingTensor(name.to_string()))?;
        let len = t.byte_length()? as usize;
        let start = self.data_offset as usize + t.data_offset as usize;
        let end = start
            .checked_add(len)
            .ok_or(LoadError::OffsetOverflow)?;
        if end > self.mmap.len() {
            return Err(LoadError::TensorOutOfBounds {
                name: name.to_string(),
                end,
                file_len: self.mmap.len(),
            });
        }
        Ok(&self.mmap[start..end])
    }

    /// Look up a metadata value by exact key.
    pub fn meta(&self, key: &str) -> Option<&MetaValue> {
        self.metadata
            .iter()
            .find_map(|(k, v)| (k == key).then_some(v))
    }
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("io error reading {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },

    #[error("not a GGUF file (bad magic): {0:?}")]
    BadMagic([u8; 4]),

    #[error(
        "unsupported GGUF version {found} (this loader only handles v{supported})"
    )]
    UnsupportedVersion { found: u32, supported: u32 },

    #[error("unexpected EOF at byte {offset}: needed {needed} more bytes, have {have}")]
    UnexpectedEof {
        offset: usize,
        needed: usize,
        have: usize,
    },

    #[error("string length {len} exceeds remaining file size {remaining}")]
    StringTooLong { len: u64, remaining: usize },

    #[error("array length overflow: {count} elements of type {element_type}")]
    ArrayOverflow { count: u64, element_type: u32 },

    #[error("unknown GGUF metadata type tag: {0}")]
    UnknownGgufType(u32),

    #[error("nested arrays are not supported in GGUF v3")]
    NestedArray,

    #[error("tensor `{name}`: dtype tag {raw} not supported by this loader")]
    UnsupportedDtype { name: String, raw: u32 },

    #[error("tensor `{name}`: Q4_K element count {elements} is not a multiple of 256")]
    Q4KShape { name: String, elements: u64 },

    #[error("tensor `{name}` data ends at byte {end} but file is only {file_len} bytes")]
    TensorOutOfBounds {
        name: String,
        end: usize,
        file_len: usize,
    },

    #[error("tensor `{0}` not found in GGUF")]
    MissingTensor(String),

    #[error("tensor offset arithmetic overflowed")]
    OffsetOverflow,

    #[error(
        "GGUF metadata `general.architecture` is `{found}` but this crate \
         is hard-coded for `qwen3_5_moe` (Qwen3.6-35B-A3B). Refusing to load."
    )]
    WrongArchitecture { found: String },
}

/// Sequential cursor that bounds-checks every read against the input
/// slice. Keeps the parser auditable and free of `unsafe`.
struct Cursor<'a> {
    buf: &'a [u8],
    off: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, off: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], LoadError> {
        if self.off + n > self.buf.len() {
            return Err(LoadError::UnexpectedEof {
                offset: self.off,
                needed: n,
                have: self.buf.len() - self.off,
            });
        }
        let s = &self.buf[self.off..self.off + n];
        self.off += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, LoadError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, LoadError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, LoadError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, LoadError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i8(&mut self) -> Result<i8, LoadError> {
        Ok(self.u8()? as i8)
    }
    fn i16(&mut self) -> Result<i16, LoadError> {
        Ok(self.u16()? as i16)
    }
    fn i32(&mut self) -> Result<i32, LoadError> {
        Ok(self.u32()? as i32)
    }
    fn i64(&mut self) -> Result<i64, LoadError> {
        Ok(self.u64()? as i64)
    }
    fn f32(&mut self) -> Result<f32, LoadError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, LoadError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn bool(&mut self) -> Result<bool, LoadError> {
        Ok(self.u8()? != 0)
    }

    fn string(&mut self) -> Result<String, LoadError> {
        let len = self.u64()?;
        if (len as usize) > self.buf.len() - self.off {
            return Err(LoadError::StringTooLong {
                len,
                remaining: self.buf.len() - self.off,
            });
        }
        let bytes = self.take(len as usize)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

fn read_meta_value(c: &mut Cursor<'_>, ty: GgufType) -> Result<MetaValue, LoadError> {
    Ok(match ty {
        GgufType::UInt8 => MetaValue::U8(c.u8()?),
        GgufType::Int8 => MetaValue::I8(c.i8()?),
        GgufType::UInt16 => MetaValue::U16(c.u16()?),
        GgufType::Int16 => MetaValue::I16(c.i16()?),
        GgufType::UInt32 => MetaValue::U32(c.u32()?),
        GgufType::Int32 => MetaValue::I32(c.i32()?),
        GgufType::Float32 => MetaValue::F32(c.f32()?),
        GgufType::Bool => MetaValue::Bool(c.bool()?),
        GgufType::String => MetaValue::String(c.string()?),
        GgufType::UInt64 => MetaValue::U64(c.u64()?),
        GgufType::Int64 => MetaValue::I64(c.i64()?),
        GgufType::Float64 => MetaValue::F64(c.f64()?),
        GgufType::Array => {
            let element_type = c.u32()?;
            let count = c.u64()?;
            let elem_kind = GgufType::from_u32(element_type)?;
            if matches!(elem_kind, GgufType::Array) {
                return Err(LoadError::NestedArray);
            }
            // Pull the right number of bytes for the array. For
            // String we don't know the byte length up front, so we
            // walk count entries and concatenate their on-disk
            // (length-prefixed) representations into the raw buffer.
            let raw = match elem_kind {
                GgufType::String => {
                    let start = c.off;
                    for _ in 0..count {
                        let _ = c.string()?;
                    }
                    c.buf[start..c.off].to_vec()
                }
                _ => {
                    let stride = match elem_kind {
                        GgufType::UInt8 | GgufType::Int8 | GgufType::Bool => 1,
                        GgufType::UInt16 | GgufType::Int16 => 2,
                        GgufType::UInt32 | GgufType::Int32 | GgufType::Float32 => 4,
                        GgufType::UInt64 | GgufType::Int64 | GgufType::Float64 => 8,
                        _ => unreachable!(),
                    };
                    let total = count
                        .checked_mul(stride as u64)
                        .and_then(|t| usize::try_from(t).ok())
                        .ok_or(LoadError::ArrayOverflow {
                            count,
                            element_type,
                        })?;
                    c.take(total)?.to_vec()
                }
            };
            MetaValue::Array {
                element_type,
                count,
                raw,
            }
        }
    })
}

/// Parse the GGUF header + metadata + tensor table out of the head of
/// the mmapped file. Returns the metadata + tensor table + the
/// absolute byte offset where tensor data starts (alignment-corrected
/// per the v3 spec).
fn parse_header(
    mmap: &[u8],
) -> Result<(Vec<(String, MetaValue)>, Vec<TensorRecord>, u64), LoadError> {
    let mut c = Cursor::new(mmap);

    let magic = c.take(4)?;
    if magic != GGUF_MAGIC {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(magic);
        return Err(LoadError::BadMagic(buf));
    }

    let version = c.u32()?;
    if version != GGUF_VERSION_SUPPORTED {
        return Err(LoadError::UnsupportedVersion {
            found: version,
            supported: GGUF_VERSION_SUPPORTED,
        });
    }

    let tensor_count = c.u64()?;
    let kv_count = c.u64()?;

    let mut metadata = Vec::with_capacity(kv_count as usize);
    for _ in 0..kv_count {
        let key = c.string()?;
        let ty = GgufType::from_u32(c.u32()?)?;
        let value = read_meta_value(&mut c, ty)?;
        metadata.push((key, value));
    }

    let mut tensors = Vec::with_capacity(tensor_count as usize);
    for _ in 0..tensor_count {
        let name = c.string()?;
        let n_dim = c.u32()?;
        let mut shape = Vec::with_capacity(n_dim as usize);
        for _ in 0..n_dim {
            shape.push(c.u64()?);
        }
        let dtype = GgmlType::from_u32(c.u32()?);
        let data_offset = c.u64()?;
        tensors.push(TensorRecord {
            name,
            shape,
            dtype,
            data_offset,
        });
    }

    // Tensor data segment starts at the next ALIGNMENT-aligned boundary
    // after the header. The default alignment is 32; if the GGUF set
    // `general.alignment`, honour that.
    let alignment = metadata
        .iter()
        .find_map(|(k, v)| {
            if k == "general.alignment" {
                if let MetaValue::U32(a) = v {
                    return Some(*a as u64);
                }
            }
            None
        })
        .unwrap_or(32);
    let header_end = c.off as u64;
    let pad = (alignment - (header_end % alignment)) % alignment;
    let data_offset = header_end + pad;

    Ok((metadata, tensors, data_offset))
}

/// Open a Qwen3.6-35B-A3B Q4_K_M GGUF and materialise the
/// [`LoadedWeights`]. Validates the architecture metadata against
/// `qwen3_5_moe` (the upstream model_type for Qwen3.6-35B-A3B).
pub fn load_q4km_gguf(
    path: &Path,
    config: Qwen36MoeTextConfig,
) -> Result<LoadedWeights, LoadError> {
    let file = File::open(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    // Safety: the file is opened read-only and the mmap lifetime is
    // tied to LoadedWeights. We never write through the mmap.
    let mmap = unsafe { Mmap::map(&file) }.map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let (metadata, tensors, data_offset) = parse_header(&mmap)?;

    if let Some(MetaValue::String(arch)) = metadata
        .iter()
        .find_map(|(k, v)| (k == "general.architecture").then_some(v))
    {
        if arch != "qwen3_5_moe" {
            return Err(LoadError::WrongArchitecture {
                found: arch.clone(),
            });
        }
    }

    Ok(LoadedWeights {
        config,
        path: path.to_path_buf(),
        metadata,
        tensors,
        data_offset,
        mmap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal but valid GGUF v3 byte stream in memory.
    /// One U32 metadata key "general.alignment" = 32 plus
    /// "general.architecture" = "qwen3_5_moe", and one tiny f32 tensor.
    fn synth_gguf() -> (Vec<u8>, Vec<f32>) {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(GGUF_MAGIC);
        b.extend_from_slice(&3u32.to_le_bytes()); // version
        b.extend_from_slice(&1u64.to_le_bytes()); // tensor_count = 1
        b.extend_from_slice(&2u64.to_le_bytes()); // kv_count = 2

        // KV 1: "general.architecture" : String : "qwen3_5_moe"
        let key1 = b"general.architecture";
        b.extend_from_slice(&(key1.len() as u64).to_le_bytes());
        b.extend_from_slice(key1);
        b.extend_from_slice(&(GgufType::String as u32).to_le_bytes());
        let arch = b"qwen3_5_moe";
        b.extend_from_slice(&(arch.len() as u64).to_le_bytes());
        b.extend_from_slice(arch);

        // KV 2: "general.alignment" : UInt32 : 32
        let key2 = b"general.alignment";
        b.extend_from_slice(&(key2.len() as u64).to_le_bytes());
        b.extend_from_slice(key2);
        b.extend_from_slice(&(GgufType::UInt32 as u32).to_le_bytes());
        b.extend_from_slice(&32u32.to_le_bytes());

        // Tensor: name "blk.0.attn_norm.weight", 1-d, shape=[4], f32, offset=0
        let tname = b"blk.0.attn_norm.weight";
        b.extend_from_slice(&(tname.len() as u64).to_le_bytes());
        b.extend_from_slice(tname);
        b.extend_from_slice(&1u32.to_le_bytes()); // n_dim
        b.extend_from_slice(&4u64.to_le_bytes()); // shape[0] = 4
        b.extend_from_slice(&0u32.to_le_bytes()); // dtype = F32
        b.extend_from_slice(&0u64.to_le_bytes()); // data_offset = 0

        // Pad to 32-byte alignment.
        let pad = (32 - (b.len() % 32)) % 32;
        b.extend(std::iter::repeat(0u8).take(pad));

        // Tensor data: 4 × f32.
        let weights: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        for w in &weights {
            b.extend_from_slice(&w.to_le_bytes());
        }
        (b, weights)
    }

    fn write_temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("ctox_qwen36_loader_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        path
    }

    fn must_load(
        path: &Path,
        config: Qwen36MoeTextConfig,
    ) -> LoadedWeights {
        match load_q4km_gguf(path, config) {
            Ok(w) => w,
            Err(err) => panic!("load_q4km_gguf failed: {err}"),
        }
    }

    fn must_fail(path: &Path, config: Qwen36MoeTextConfig) -> LoadError {
        match load_q4km_gguf(path, config) {
            Ok(_) => panic!("expected load to fail, but it succeeded"),
            Err(err) => err,
        }
    }

    #[test]
    fn loads_synthetic_gguf_and_round_trips_tensor_bytes() {
        use crate::model::QWEN36_35B_A3B_TEXT_CONFIG;
        let (bytes, weights) = synth_gguf();
        let path = write_temp_file("synth.gguf", &bytes);
        let loaded = must_load(&path, QWEN36_35B_A3B_TEXT_CONFIG.clone());

        // Architecture metadata round-trip.
        let arch = loaded.meta("general.architecture").unwrap();
        match arch {
            MetaValue::String(s) => assert_eq!(s, "qwen3_5_moe"),
            other => panic!("expected String, got {other:?}"),
        }

        // Tensor present + correct shape + correct dtype.
        let t = loaded.tensor("blk.0.attn_norm.weight").unwrap();
        assert_eq!(t.shape, vec![4]);
        assert_eq!(t.dtype, GgmlType::F32);

        // Bytes round-trip into the original f32 values.
        let raw = loaded.tensor_bytes("blk.0.attn_norm.weight").unwrap();
        assert_eq!(raw.len(), 16);
        let got: Vec<f32> = raw
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(got, weights);
    }

    #[test]
    fn rejects_non_qwen3_5_moe_architecture() {
        use crate::model::QWEN36_35B_A3B_TEXT_CONFIG;

        // Same as synth but with arch = "llama".
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(GGUF_MAGIC);
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&1u64.to_le_bytes());
        let key = b"general.architecture";
        b.extend_from_slice(&(key.len() as u64).to_le_bytes());
        b.extend_from_slice(key);
        b.extend_from_slice(&(GgufType::String as u32).to_le_bytes());
        let arch = b"llama";
        b.extend_from_slice(&(arch.len() as u64).to_le_bytes());
        b.extend_from_slice(arch);
        let pad = (32 - (b.len() % 32)) % 32;
        b.extend(std::iter::repeat(0u8).take(pad));

        let path = write_temp_file("wrongarch.gguf", &b);
        let err = must_fail(&path, QWEN36_35B_A3B_TEXT_CONFIG.clone());
        match err {
            LoadError::WrongArchitecture { found } => assert_eq!(found, "llama"),
            other => panic!("expected WrongArchitecture, got {other:?}"),
        }
    }

    #[test]
    fn rejects_bad_magic() {
        use crate::model::QWEN36_35B_A3B_TEXT_CONFIG;
        let path = write_temp_file("badmagic.gguf", b"NOPEnotgguf");
        let err = must_fail(&path, QWEN36_35B_A3B_TEXT_CONFIG.clone());
        assert!(matches!(err, LoadError::BadMagic(_)));
    }

    #[test]
    fn rejects_wrong_version() {
        use crate::model::QWEN36_35B_A3B_TEXT_CONFIG;
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(GGUF_MAGIC);
        b.extend_from_slice(&2u32.to_le_bytes()); // GGUF v2 — unsupported
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        let path = write_temp_file("v2.gguf", &b);
        let err = must_fail(&path, QWEN36_35B_A3B_TEXT_CONFIG.clone());
        assert!(matches!(
            err,
            LoadError::UnsupportedVersion { found: 2, supported: 3 }
        ));
    }

    #[test]
    fn q4_k_byte_length_matches_block_size_constant() {
        // 256 elements = 1 super-block = 144 bytes;
        // 1024 elements = 4 super-blocks = 576 bytes.
        let t = TensorRecord {
            name: "test".into(),
            shape: vec![256],
            dtype: GgmlType::Q4_K,
            data_offset: 0,
        };
        assert_eq!(t.byte_length().unwrap(), 144);
        let t2 = TensorRecord {
            name: "test2".into(),
            shape: vec![1024],
            dtype: GgmlType::Q4_K,
            data_offset: 0,
        };
        assert_eq!(t2.byte_length().unwrap(), 4 * 144);
    }
}
