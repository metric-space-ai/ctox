use crate::{Error, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufTensorInfo {
    pub name: String,
    pub dims: Vec<u64>,
    pub ggml_type: u32,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufInspection {
    pub version: u32,
    pub tensor_count: u64,
    pub metadata_count: u64,
    pub tensors: Vec<GgufTensorInfo>,
    pub architecture: Option<String>,
}

pub fn inspect(path: impl AsRef<Path>) -> Result<GgufInspection> {
    let mut reader = Reader::open(path.as_ref())?;
    if reader.read_exact_array::<4>()? != *b"GGUF" {
        return Err(Error::InvalidFormat("not a GGUF file"));
    }
    let version = reader.read_u32()?;
    let tensor_count = reader.read_u64()?;
    let metadata_count = reader.read_u64()?;
    let mut architecture = None;
    for _ in 0..metadata_count {
        let key = reader.read_string()?;
        let ty = reader.read_u32()?;
        if key == "general.architecture" && ty == GGUF_TYPE_STRING {
            architecture = Some(reader.read_string()?);
        } else {
            reader.skip_metadata_value(ty)?;
        }
    }
    let mut tensors = Vec::with_capacity(tensor_count.min(1_000_000) as usize);
    for _ in 0..tensor_count {
        let name = reader.read_string()?;
        let n_dims = reader.read_u32()? as usize;
        if n_dims > 8 {
            return Err(Error::InvalidFormat("GGUF tensor has too many dimensions"));
        }
        let mut dims = Vec::with_capacity(n_dims);
        for _ in 0..n_dims {
            dims.push(reader.read_u64()?);
        }
        let ggml_type = reader.read_u32()?;
        let offset = reader.read_u64()?;
        tensors.push(GgufTensorInfo {
            name,
            dims,
            ggml_type,
            offset,
        });
    }
    tensors.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(GgufInspection {
        version,
        tensor_count,
        metadata_count,
        tensors,
        architecture,
    })
}

const GGUF_TYPE_UINT8: u32 = 0;
const GGUF_TYPE_INT8: u32 = 1;
const GGUF_TYPE_UINT16: u32 = 2;
const GGUF_TYPE_INT16: u32 = 3;
const GGUF_TYPE_UINT32: u32 = 4;
const GGUF_TYPE_INT32: u32 = 5;
const GGUF_TYPE_FLOAT32: u32 = 6;
const GGUF_TYPE_BOOL: u32 = 7;
const GGUF_TYPE_STRING: u32 = 8;
const GGUF_TYPE_ARRAY: u32 = 9;
const GGUF_TYPE_UINT64: u32 = 10;
const GGUF_TYPE_INT64: u32 = 11;
const GGUF_TYPE_FLOAT64: u32 = 12;

struct Reader {
    file: File,
}

impl Reader {
    fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            file: File::open(path)?,
        })
    }

    fn read_exact_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        self.file.read_exact(&mut out)?;
        Ok(out)
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read_exact_array::<4>()?))
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read_exact_array::<8>()?))
    }

    fn read_string(&mut self) -> Result<String> {
        let len = self.read_u64()?;
        if len > 64 * 1024 * 1024 {
            return Err(Error::InvalidFormat("GGUF string is too large"));
        }
        let mut bytes = vec![0u8; len as usize];
        self.file.read_exact(&mut bytes)?;
        Ok(std::str::from_utf8(&bytes)?.to_string())
    }

    fn skip(&mut self, bytes: u64) -> Result<()> {
        self.file.seek(SeekFrom::Current(bytes as i64))?;
        Ok(())
    }

    fn skip_metadata_value(&mut self, ty: u32) -> Result<()> {
        match ty {
            GGUF_TYPE_UINT8 | GGUF_TYPE_INT8 | GGUF_TYPE_BOOL => self.skip(1),
            GGUF_TYPE_UINT16 | GGUF_TYPE_INT16 => self.skip(2),
            GGUF_TYPE_UINT32 | GGUF_TYPE_INT32 | GGUF_TYPE_FLOAT32 => self.skip(4),
            GGUF_TYPE_UINT64 | GGUF_TYPE_INT64 | GGUF_TYPE_FLOAT64 => self.skip(8),
            GGUF_TYPE_STRING => {
                let len = self.read_u64()?;
                self.skip(len)
            }
            GGUF_TYPE_ARRAY => {
                let elem_ty = self.read_u32()?;
                let len = self.read_u64()?;
                for _ in 0..len {
                    self.skip_metadata_value(elem_ty)?;
                }
                Ok(())
            }
            _ => Err(Error::InvalidFormat("unsupported GGUF metadata type")),
        }
    }
}
