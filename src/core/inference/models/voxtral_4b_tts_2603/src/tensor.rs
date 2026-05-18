//! Lightweight tensor metadata and byte views.

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F32,
    F16,
    BF16,
    I64,
    U8,
    Unknown,
}

impl DType {
    pub fn from_safetensors(s: &str) -> Self {
        match s {
            "F32" => DType::F32,
            "F16" => DType::F16,
            "BF16" => DType::BF16,
            "I64" => DType::I64,
            "U8" => DType::U8,
            _ => DType::Unknown,
        }
    }

    pub fn byte_size(self) -> Option<usize> {
        match self {
            DType::F32 => Some(4),
            DType::F16 | DType::BF16 => Some(2),
            DType::I64 => Some(8),
            DType::U8 => Some(1),
            DType::Unknown => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TensorInfo {
    pub name: String,
    pub dtype: DType,
    pub shape: Vec<usize>,
    pub data_start: usize,
    pub data_end: usize,
}

impl TensorInfo {
    pub fn numel(&self) -> usize {
        self.shape.iter().copied().product()
    }

    pub fn validate_byte_len(&self) -> Result<()> {
        let expected = self
            .dtype
            .byte_size()
            .ok_or(Error::InvalidFormat("unknown dtype"))?
            .checked_mul(self.numel())
            .ok_or(Error::InvalidFormat("tensor byte length overflow"))?;
        let actual = self
            .data_end
            .checked_sub(self.data_start)
            .ok_or(Error::InvalidFormat("negative tensor data offset"))?;
        if expected != actual {
            return Err(Error::InvalidFormat(
                "tensor byte length does not match dtype*shape",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TensorBytes<'a> {
    pub info: &'a TensorInfo,
    pub bytes: &'a [u8],
}

impl<'a> TensorBytes<'a> {
    pub fn bf16_at(&self, idx: usize) -> Result<u16> {
        if self.info.dtype != DType::BF16 {
            return Err(Error::InvalidFormat("tensor is not BF16"));
        }
        let i = idx
            .checked_mul(2)
            .ok_or(Error::OutOfBounds("bf16 index overflow"))?;
        if i + 1 >= self.bytes.len() {
            return Err(Error::OutOfBounds("bf16 index"));
        }
        Ok(u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]]))
    }

    pub fn as_f32_slice_unaligned_checked(&self) -> Result<Vec<f32>> {
        if self.info.dtype != DType::F32 {
            return Err(Error::InvalidFormat("tensor is not F32"));
        }
        if self.bytes.len() % 4 != 0 {
            return Err(Error::InvalidFormat("F32 byte length not multiple of 4"));
        }
        let mut out = Vec::with_capacity(self.bytes.len() / 4);
        for chunk in self.bytes.chunks_exact(4) {
            out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(out)
    }
}
