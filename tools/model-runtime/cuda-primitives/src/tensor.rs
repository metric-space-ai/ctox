//! `CudaTensor<T>` — owned device buffer + shape/stride/dtype.
//!
//! Intentionally NOT an op-bearing type. Operations are kernel
//! launches that accept `&CudaTensor` inputs and an `&mut CudaTensor`
//! output.
//!
//! Only row-major (C-order) storage is supported.

use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaSlice, DeviceRepr, ValidAsZeroBits};

use crate::device::DeviceContext;
use crate::dtype::{DType, DTypeTrait};

pub type Shape = Vec<usize>;
pub type Stride = Vec<usize>;

/// Trait bundle for element types stored in a `CudaTensor`:
///   * `DTypeTrait`           — our runtime dtype tag
///   * `DeviceRepr`           — cudarc's "safe to memcpy to device"
///   * `ValidAsZeroBits`      — so `zeros()` is sound
pub trait TensorElem: DTypeTrait + DeviceRepr + ValidAsZeroBits + Unpin {}

impl<T: DTypeTrait + DeviceRepr + ValidAsZeroBits + Unpin> TensorElem for T {}

pub struct CudaTensor<T: TensorElem> {
    buf: CudaSlice<T>,
    shape: Shape,
    stride: Stride,
    device: Arc<DeviceContext>,
    _marker: PhantomData<T>,
}

impl<T: TensorElem> CudaTensor<T> {
    /// Allocate zeroed storage for `shape`.
    pub fn zeros(device: Arc<DeviceContext>, shape: Shape) -> Result<Self> {
        let n_elems = shape.iter().product::<usize>();
        let stream = device.raw().default_stream();
        let buf = stream.alloc_zeros::<T>(n_elems).map_err(|e| {
            anyhow!(
                "alloc_zeros({} elems) on device {}: {:?}",
                n_elems,
                device.ordinal(),
                e
            )
        })?;
        let stride = default_stride(&shape);
        Ok(Self {
            buf,
            shape,
            stride,
            device,
            _marker: PhantomData,
        })
    }

    /// Allocate **uninitialized** storage for `shape`.
    ///
    /// Identical to [`zeros`][Self::zeros] except the initial memset
    /// is skipped. Only use this when the caller immediately
    /// overwrites every element of the buffer before anything reads
    /// from it — e.g. a matmul output or a cast destination. Reading
    /// from an `uninit` tensor before it has been written produces
    /// undefined garbage on the device.
    ///
    /// # Safety
    ///
    /// The type system can't enforce "overwrite before read" on a
    /// tensor-level granularity, so this constructor is `unsafe`
    /// and leaves the guarantee to the caller. Most consumers that
    /// zero-fill-then-overwrite should stay on `zeros`; using this
    /// is worth it for per-forward scratch tensors where the memset
    /// launch adds up across 64 layers × N scratches per layer.
    pub unsafe fn uninit(device: Arc<DeviceContext>, shape: Shape) -> Result<Self> {
        let n_elems = shape.iter().product::<usize>();
        let stream = device.raw().default_stream();
        let buf = unsafe { stream.alloc::<T>(n_elems) }.map_err(|e| {
            anyhow!(
                "alloc({} elems) on device {}: {:?}",
                n_elems,
                device.ordinal(),
                e
            )
        })?;
        let stride = default_stride(&shape);
        Ok(Self {
            buf,
            shape,
            stride,
            device,
            _marker: PhantomData,
        })
    }

    /// Upload host slice into a fresh device tensor with the given
    /// shape. `host.len()` must match `shape.iter().product()`.
    pub fn from_host(
        device: Arc<DeviceContext>,
        shape: Shape,
        host: &[T],
    ) -> Result<Self> {
        let n_elems = shape.iter().product::<usize>();
        if host.len() != n_elems {
            return Err(anyhow!(
                "from_host: host.len()={} != shape.product()={}",
                host.len(),
                n_elems
            ));
        }
        let stream = device.raw().default_stream();
        let buf = stream
            .memcpy_stod(host)
            .map_err(|e| anyhow!("memcpy_stod {} elems: {:?}", n_elems, e))?;
        let stride = default_stride(&shape);
        Ok(Self {
            buf,
            shape,
            stride,
            device,
            _marker: PhantomData,
        })
    }

    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn dtype(&self) -> DType {
        T::DTYPE
    }

    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    pub fn stride(&self) -> &[usize] {
        &self.stride
    }

    pub fn device(&self) -> &Arc<DeviceContext> {
        &self.device
    }

    pub fn buf(&self) -> &CudaSlice<T> {
        &self.buf
    }

    pub fn buf_mut(&mut self) -> &mut CudaSlice<T> {
        &mut self.buf
    }

    /// Download to host. Used by bench tooling, diff checkers, and
    /// logits readout. NOT a hot-path call — implies a stream sync.
    pub fn to_host(&self) -> Result<Vec<T>> {
        let stream = self.device.raw().default_stream();
        let v = stream
            .memcpy_dtov(&self.buf)
            .map_err(|e| anyhow!("memcpy_dtov {} elems: {:?}", self.numel(), e))?;
        Ok(v)
    }

    /// Logical reshape — hand the same backing buffer back with a new
    /// shape. No device work, no memcpy. The only constraint is that
    /// the new shape's element count matches the old one (we're
    /// row-major contiguous, so this is just a label swap).
    ///
    /// Consumes `self` to make it type-system-obvious that the old
    /// handle is gone; the returned tensor owns the same underlying
    /// `CudaSlice` and is safe to use exactly like a fresh
    /// allocation.
    pub fn reshape(self, new_shape: Shape) -> Result<Self> {
        let new_numel: usize = new_shape.iter().product();
        if new_numel != self.numel() {
            return Err(anyhow!(
                "CudaTensor::reshape: numel mismatch old {:?} ({}) -> new {:?} ({})",
                self.shape,
                self.numel(),
                new_shape,
                new_numel
            ));
        }
        let new_stride = default_stride(&new_shape);
        Ok(Self {
            buf: self.buf,
            shape: new_shape,
            stride: new_stride,
            device: self.device,
            _marker: PhantomData,
        })
    }
}

impl<T: TensorElem> std::fmt::Debug for CudaTensor<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CudaTensor")
            .field("dtype", &T::DTYPE)
            .field("shape", &self.shape)
            .field("stride", &self.stride)
            .field("dev", &self.device.ordinal())
            .finish()
    }
}

/// Default row-major stride for a shape. For shape `[a, b, c]` returns
/// `[b*c, c, 1]`.
fn default_stride(shape: &[usize]) -> Stride {
    let mut s = vec![1usize; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        s[i] = s[i + 1] * shape[i + 1];
    }
    s
}

#[cfg(test)]
mod tests {
    use super::default_stride;

    #[test]
    fn stride_matches_row_major() {
        assert_eq!(default_stride(&[2, 3, 4]), vec![12, 4, 1]);
        assert_eq!(default_stride(&[5]), vec![1]);
        assert_eq!(default_stride(&[]), Vec::<usize>::new());
    }
}
