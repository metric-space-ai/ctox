//! Pure-Rust byte-exact port of the GGUF v3 file parser.
//!
//! GGUF is llama.cpp's on-disk weight format, documented at
//! `llama.cpp/docs/gguf.md`. This parser reads the file header +
//! metadata KV pairs + tensor-info directory + returns a `Gguf`
//! struct the loader uses to stream tensors into MTLBuffers.
//!
//! Matches the on-wire layout defined by the pinned llama.cpp
//! commit in `vendor/metal/ggml-metal.version`. The wire format is
//! backend-agnostic — same bytes are parsed by ggml's C code and
//! our Rust port.
//!
//! ref:
//!   - `llama.cpp/ggml/src/gguf.cpp`   (official C parser)
//!   - `llama.cpp/docs/gguf.md`        (spec)

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{anyhow, Context, Result};

/// Wire-format type codes. byte-exact to `enum gguf_type` in
/// `llama.cpp/ggml/src/gguf.cpp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum GgufValueType {
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
    fn from_raw(raw: u32) -> Result<Self> {
        use GgufValueType::*;
        Ok(match raw {
            0 => U8,
            1 => I8,
            2 => U16,
            3 => I16,
            4 => U32,
            5 => I32,
            6 => F32,
            7 => Bool,
            8 => String,
            9 => Array,
            10 => U64,
            11 => I64,
            12 => F64,
            _ => return Err(anyhow!("unknown gguf_type code: {raw}")),
        })
    }
}

/// Decoded metadata value. `Array` carries the element type and a
/// flat Vec of the decoded scalar values.
#[derive(Clone, Debug)]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    String(String),
    Array(GgufArray),
}

#[derive(Clone, Debug)]
pub enum GgufArray {
    U8(Vec<u8>),
    I8(Vec<i8>),
    U16(Vec<u16>),
    I16(Vec<i16>),
    U32(Vec<u32>),
    I32(Vec<i32>),
    U64(Vec<u64>),
    I64(Vec<i64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
    Bool(Vec<bool>),
    String(Vec<String>),
}

/// Tensor descriptor from the GGUF directory. The actual tensor
/// data lives in the file's trailing data blob at
/// `data_offset + offset`.
#[derive(Clone, Debug)]
pub struct TensorInfo {
    pub name: String,
    /// `ne[0..n_dims]`. Unused axes are NOT present in the file;
    /// callers pad with `1` to reach 4 axes.
    pub shape: Vec<u64>,
    pub type_raw: u32,
    /// Byte offset within the tensor-data blob (not within the file).
    pub offset: u64,
}

/// Parsed GGUF file handle. Holds metadata + tensor index; the raw
/// tensor data stays on disk and is read on demand via
/// `read_tensor_bytes`.
pub struct Gguf {
    pub magic: [u8; 4],
    pub version: u32,
    pub kv: BTreeMap<String, GgufValue>,
    pub tensors: Vec<TensorInfo>,
    pub data_offset: u64,
    reader: BufReader<File>,
    alignment: u64,
}

impl Gguf {
    /// Open + parse a GGUF file. Leaves the reader positioned at the
    /// start of the tensor data blob (aligned to `general.alignment`).
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut r = BufReader::new(file);

        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != b"GGUF" {
            return Err(anyhow!(
                "GGUF magic mismatch: expected `GGUF`, got {:?}",
                magic
            ));
        }

        let version: u32 = read_u32(&mut r)?;
        if version != 3 {
            return Err(anyhow!(
                "unsupported GGUF version {version}; only v3 supported \
                 (llama.cpp commit in vendor/metal/ggml-metal.version)"
            ));
        }

        let tensor_count: u64 = read_u64(&mut r)?;
        let kv_count: u64 = read_u64(&mut r)?;

        // Parse metadata KV pairs.
        let mut kv: BTreeMap<String, GgufValue> = BTreeMap::new();
        for _ in 0..kv_count {
            let name = read_string(&mut r)?;
            let vt_raw: u32 = read_u32(&mut r)?;
            let vt = GgufValueType::from_raw(vt_raw).with_context(|| format!("kv `{name}`"))?;
            let value = read_value(&mut r, vt)?;
            kv.insert(name, value);
        }

        // Parse tensor-info directory.
        let mut tensors: Vec<TensorInfo> = Vec::with_capacity(tensor_count as usize);
        for _ in 0..tensor_count {
            let name = read_string(&mut r)?;
            let n_dims: u32 = read_u32(&mut r)?;
            if n_dims > 4 {
                return Err(anyhow!("tensor `{name}` has {n_dims} dims; ggml caps at 4"));
            }
            let mut shape: Vec<u64> = Vec::with_capacity(n_dims as usize);
            for _ in 0..n_dims {
                shape.push(read_u64(&mut r)?);
            }
            let type_raw: u32 = read_u32(&mut r)?;
            let offset: u64 = read_u64(&mut r)?;
            tensors.push(TensorInfo {
                name,
                shape,
                type_raw,
                offset,
            });
        }

        // Compute alignment — defaults to 32, overridden by the
        // `general.alignment` metadata if present.
        // ref: gguf.cpp::GGUF_DEFAULT_ALIGNMENT
        let alignment: u64 = match kv.get("general.alignment") {
            Some(GgufValue::U32(v)) => *v as u64,
            Some(GgufValue::U64(v)) => *v,
            _ => 32,
        };

        // Align current position to `alignment`; the tensor-data
        // blob starts there.
        let here = r.stream_position()?;
        let data_offset = (here + alignment - 1) & !(alignment - 1);
        r.seek(SeekFrom::Start(data_offset))?;

        Ok(Self {
            magic,
            version,
            kv,
            tensors,
            data_offset,
            reader: r,
            alignment,
        })
    }

    /// Read the raw bytes for `tensor_idx` into `dst`. `dst` must be
    /// sized exactly to the tensor's on-disk nbytes.
    pub fn read_tensor_bytes(&mut self, tensor_idx: usize, dst: &mut [u8]) -> Result<()> {
        let t = self
            .tensors
            .get(tensor_idx)
            .ok_or_else(|| anyhow!("tensor idx {tensor_idx} out of range"))?;
        self.reader
            .seek(SeekFrom::Start(self.data_offset + t.offset))?;
        self.reader.read_exact(dst)?;
        Ok(())
    }

    /// Convenience: look up a tensor by name.
    pub fn tensor_by_name(&self, name: &str) -> Option<&TensorInfo> {
        self.tensors.iter().find(|t| t.name == name)
    }

    /// Alignment constant sourced from the file's metadata (or
    /// GGUF's 32-byte default).
    pub fn alignment(&self) -> u64 {
        self.alignment
    }
}

// ─── Low-level readers ─────────────────────────────────────────────

fn read_u32(r: &mut BufReader<File>) -> Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut BufReader<File>) -> Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn read_i32(r: &mut BufReader<File>) -> Result<i32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(i32::from_le_bytes(b))
}

fn read_i64(r: &mut BufReader<File>) -> Result<i64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(i64::from_le_bytes(b))
}

fn read_f32(r: &mut BufReader<File>) -> Result<f32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(f32::from_le_bytes(b))
}

fn read_f64(r: &mut BufReader<File>) -> Result<f64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(f64::from_le_bytes(b))
}

fn read_u16(r: &mut BufReader<File>) -> Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_i16(r: &mut BufReader<File>) -> Result<i16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(i16::from_le_bytes(b))
}

fn read_u8(r: &mut BufReader<File>) -> Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_i8(r: &mut BufReader<File>) -> Result<i8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0] as i8)
}

fn read_bool(r: &mut BufReader<File>) -> Result<bool> {
    Ok(read_u8(r)? != 0)
}

fn read_string(r: &mut BufReader<File>) -> Result<String> {
    let len: u64 = read_u64(r)?;
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| anyhow!("invalid UTF-8 in GGUF string: {e}"))
}

fn read_value(r: &mut BufReader<File>, vt: GgufValueType) -> Result<GgufValue> {
    use GgufValueType as V;
    Ok(match vt {
        V::U8 => GgufValue::U8(read_u8(r)?),
        V::I8 => GgufValue::I8(read_i8(r)?),
        V::U16 => GgufValue::U16(read_u16(r)?),
        V::I16 => GgufValue::I16(read_i16(r)?),
        V::U32 => GgufValue::U32(read_u32(r)?),
        V::I32 => GgufValue::I32(read_i32(r)?),
        V::U64 => GgufValue::U64(read_u64(r)?),
        V::I64 => GgufValue::I64(read_i64(r)?),
        V::F32 => GgufValue::F32(read_f32(r)?),
        V::F64 => GgufValue::F64(read_f64(r)?),
        V::Bool => GgufValue::Bool(read_bool(r)?),
        V::String => GgufValue::String(read_string(r)?),
        V::Array => GgufValue::Array(read_array(r)?),
    })
}

fn read_array(r: &mut BufReader<File>) -> Result<GgufArray> {
    let elt_vt_raw: u32 = read_u32(r)?;
    let elt_vt = GgufValueType::from_raw(elt_vt_raw)?;
    let n: u64 = read_u64(r)?;
    use GgufValueType as V;
    Ok(match elt_vt {
        V::U8 => GgufArray::U8((0..n).map(|_| read_u8(r)).collect::<Result<_>>()?),
        V::I8 => GgufArray::I8((0..n).map(|_| read_i8(r)).collect::<Result<_>>()?),
        V::U16 => GgufArray::U16((0..n).map(|_| read_u16(r)).collect::<Result<_>>()?),
        V::I16 => GgufArray::I16((0..n).map(|_| read_i16(r)).collect::<Result<_>>()?),
        V::U32 => GgufArray::U32((0..n).map(|_| read_u32(r)).collect::<Result<_>>()?),
        V::I32 => GgufArray::I32((0..n).map(|_| read_i32(r)).collect::<Result<_>>()?),
        V::U64 => GgufArray::U64((0..n).map(|_| read_u64(r)).collect::<Result<_>>()?),
        V::I64 => GgufArray::I64((0..n).map(|_| read_i64(r)).collect::<Result<_>>()?),
        V::F32 => GgufArray::F32((0..n).map(|_| read_f32(r)).collect::<Result<_>>()?),
        V::F64 => GgufArray::F64((0..n).map(|_| read_f64(r)).collect::<Result<_>>()?),
        V::Bool => GgufArray::Bool((0..n).map(|_| read_bool(r)).collect::<Result<_>>()?),
        V::String => GgufArray::String((0..n).map(|_| read_string(r)).collect::<Result<_>>()?),
        V::Array => return Err(anyhow!("GGUF array-of-arrays not supported")),
    })
}
