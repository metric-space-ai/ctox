mod cpu;
#[cfg(feature = "cuda")]
mod cuda;
#[cfg(feature = "cuda")]
mod ffi;

#[cfg(feature = "cuda")]
use std::sync::Once;
use std::{
    borrow::Cow,
    io::{Cursor, Read},
    sync::{atomic::AtomicUsize, Arc, Mutex},
};

use byteorder::{LittleEndian, ReadBytesExt};
use candle_core::{
    quantized::{ggml_file::qtensor_from_ggml, GgmlDType, QMatMul, QStorage, QTensor},
    DType, Device, DeviceLocation, Result, Tensor,
};
use candle_nn::Module;

use crate::{
    generate_isq, generate_isq_imatrix,
    utils::{deserialize_tensor, serialize_tensor, version_is_compatible, UQFF_VERSION},
    IsqType, QuantMethod, QuantMethodConfig, QuantizeOntoGuard, QuantizedSerde, QuantizedSerdeType,
};

#[derive(Debug)]
pub struct GgufMatMul {
    pub(crate) w: QMatMul,
    pub(crate) b: Option<Tensor>,
    relocated_qweights: Mutex<Vec<(DeviceLocation, Arc<QTensor>)>>,
}

#[cfg(feature = "cuda")]
fn indexed_moe_cuda_supported_dtype(dtype: GgmlDType) -> bool {
    matches!(
        dtype,
        GgmlDType::Q5_1
            | GgmlDType::Q2K
            | GgmlDType::Q3K
            | GgmlDType::Q4K
            | GgmlDType::Q5K
            | GgmlDType::Q6K
            | GgmlDType::Q8_0
    )
}

#[cfg(feature = "cuda")]
static INDEXED_MOE_CUDA_LOG_ONCE: Once = Once::new();
#[cfg(feature = "cuda")]
static INDEXED_MOE_CPU_FALLBACK_LOG_ONCE: Once = Once::new();

impl QuantMethod for GgufMatMul {
    fn new(method: QuantMethodConfig) -> Result<Self>
    where
        Self: Sized,
    {
        match method {
            QuantMethodConfig::Gguf { q_weight, b } => Ok(Self {
                w: QMatMul::from_arc(q_weight)?,
                b,
                relocated_qweights: Mutex::new(Vec::new()),
            }),
            QuantMethodConfig::GptqAwq { .. }
            | QuantMethodConfig::Unquantized(_)
            | QuantMethodConfig::Hqq { .. }
            | QuantMethodConfig::Dummy
            | QuantMethodConfig::FP8 { .. }
            | QuantMethodConfig::Bnb { .. }
            | QuantMethodConfig::BlockwiseFP8 { .. }
            | QuantMethodConfig::PerTensorFP8 { .. }
            | QuantMethodConfig::Afq { .. }
            | QuantMethodConfig::MXFP4 { .. } => unreachable!(),
        }
    }

    fn dequantize_w(&self) -> Result<Tensor> {
        self.w.dequantize_f16()?.to_dtype(DType::F32)
    }

    fn forward(&self, a: &Tensor) -> Result<Tensor> {
        match &self.w {
            QMatMul::QTensor(q) => {
                let q = self.qtensor_for_device(q, a.device())?;
                self.forward_with_weight(QMatMul::from_arc(q)?, a)
            }
            QMatMul::Tensor(t) => {
                let t = if t.device().same_device(a.device()) {
                    t.clone()
                } else {
                    t.to_device(a.device())?
                };
                self.forward_with_weight(QMatMul::Tensor(t), a)
            }
            QMatMul::TensorF16(t) => {
                let t = if t.device().same_device(a.device()) {
                    t.clone()
                } else {
                    t.to_device(a.device())?
                };
                self.forward_with_weight(QMatMul::TensorF16(t), a)
            }
        }
    }

    /// Compute matmul of `self` and `a`. `self` should contain the weights.
    ///
    /// If `a` is (n_tokens, 1, cols), `self` weights are (n_experts, rows, cols),
    /// then the indices are (n_tokens, n_experts_per_tok).
    fn gather_forward(&self, x: &Tensor, indices: &Tensor) -> Result<Tensor> {
        // Use indexed_moe_forward for efficient indexed matmul
        // Expected shapes:
        // - x: (n_tokens, 1, hidden_dim) or (n_tokens, n_experts_per_tok, hidden_dim)
        // - indices: (n_tokens, n_experts_per_tok)
        // - weights (self): (n_experts, out_features, in_features)
        #[cfg(feature = "cuda")]
        let res = match &self.w {
            QMatMul::QTensor(q) => {
                if matches!(x.device(), Device::Cuda(_))
                    && indexed_moe_cuda_supported_dtype(q.dtype())
                {
                    INDEXED_MOE_CUDA_LOG_ONCE.call_once(|| {
                        tracing::info!(
                            "GGUF indexed MoE using CUDA kernel dtype={:?} x_device={:?} w_device={:?}",
                            q.dtype(),
                            x.device().location(),
                            q.device().location()
                        );
                    });
                    let q = self.qtensor_for_device(q, x.device())?;
                    let weight = QMatMul::from_arc(q)?;
                    cuda::qmatmul_indexed_moe_forward(&weight, x, indices)?
                } else {
                    INDEXED_MOE_CPU_FALLBACK_LOG_ONCE.call_once(|| {
                        tracing::warn!(
                            "GGUF indexed MoE falling back to CPU dtype={:?} x_device={:?} w_device={:?}",
                            q.dtype(),
                            x.device().location(),
                            q.device().location()
                        );
                    });
                    cpu::cpu_indexed_moe_forward(&self.w, x, indices)?
                }
            }
            QMatMul::Tensor(t) => {
                INDEXED_MOE_CPU_FALLBACK_LOG_ONCE.call_once(|| {
                    tracing::warn!(
                        "GGUF indexed MoE falling back to CPU for dense tensor x_device={:?} w_device={:?}",
                        x.device().location(),
                        t.device().location()
                    );
                });
                let t = if t.device().same_device(x.device()) {
                    t.clone()
                } else {
                    t.to_device(x.device())?
                };
                let weight = QMatMul::Tensor(t);
                cpu::cpu_indexed_moe_forward(&weight, x, indices)?
            }
            QMatMul::TensorF16(t) => {
                INDEXED_MOE_CPU_FALLBACK_LOG_ONCE.call_once(|| {
                    tracing::warn!(
                        "GGUF indexed MoE falling back to CPU for f16 tensor x_device={:?} w_device={:?}",
                        x.device().location(),
                        t.device().location()
                    );
                });
                let t = if t.device().same_device(x.device()) {
                    t.clone()
                } else {
                    t.to_device(x.device())?
                };
                let weight = QMatMul::TensorF16(t);
                cpu::cpu_indexed_moe_forward(&weight, x, indices)?
            }
        };

        // For CPU and Metal: use dequantize-then-matmul approach
        #[cfg(not(feature = "cuda"))]
        let res = cpu::cpu_indexed_moe_forward(&self.w, x, indices)?;

        if let Some(ref b) = self.b {
            res.broadcast_add(b)
        } else {
            Ok(res)
        }
    }

    fn quantized_act_type(&self) -> Option<DType> {
        Some(DType::F32)
    }

    fn add_delta_w(&self, delta: &Tensor) -> Result<Arc<dyn QuantMethod>> {
        match self {
            Self {
                w: QMatMul::Tensor(w),
                b,
                ..
            } => Ok(Arc::new(Self {
                w: QMatMul::Tensor((w + delta)?),
                b: b.clone(),
                relocated_qweights: Mutex::new(Vec::new()),
            })),
            Self {
                w: QMatMul::TensorF16(w),
                b,
                ..
            } => Ok(Arc::new(Self {
                w: QMatMul::TensorF16((w + delta)?),
                b: b.clone(),
                relocated_qweights: Mutex::new(Vec::new()),
            })),
            Self {
                w: QMatMul::QTensor(w),
                b,
                ..
            } => {
                let (w, dtype) = (w.dequantize(&w.device())?, w.dtype());
                let w = QMatMul::QTensor(std::sync::Arc::new(
                    candle_core::quantized::QTensor::quantize(&(w + delta)?, dtype)?,
                ));
                Ok(Arc::new(Self {
                    w,
                    b: b.clone(),
                    relocated_qweights: Mutex::new(Vec::new()),
                }))
            }
        }
    }

    fn dtype_and_device(&self) -> (DType, candle_core::Device) {
        match &self.w {
            QMatMul::QTensor(q) => (DType::F32, q.device()),
            QMatMul::Tensor(t) | QMatMul::TensorF16(t) => (t.dtype(), t.device().clone()),
        }
    }

    fn apply_isq(
        self: Arc<Self>,
        dtype: Option<IsqType>,
        device: Device,
        n_quantized: &AtomicUsize,
        imatrix_weight: Option<Vec<f32>>,
        guard: QuantizeOntoGuard,
    ) -> Result<Arc<dyn QuantMethod>> {
        if let Some(dtype) = dtype {
            // F8Q8 is not a GgmlDType, so intercept before try_into()
            if dtype == IsqType::F8Q8 {
                let t = match &self.w {
                    QMatMul::QTensor(q) => q.dequantize(&q.device())?,
                    QMatMul::TensorF16(t) | QMatMul::Tensor(t) => t.clone(),
                };
                let t = t.to_device(&device)?;
                n_quantized.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(Arc::new(crate::F8Q8Linear::from_weight(
                    &t,
                    self.b.clone(),
                )?));
            }
            let t = match &self.w {
                QMatMul::QTensor(q) => q.dequantize(&q.device())?,
                QMatMul::TensorF16(t) | QMatMul::Tensor(t) => t.clone(),
            };
            let dtype = dtype.try_into()?;
            let res = if let Some(imatrix_weight) = imatrix_weight {
                generate_isq_imatrix!(t, imatrix_weight, device, dtype, n_quantized, guard)
            } else {
                generate_isq!(t, device, dtype, n_quantized, guard)
            };
            Ok(Arc::new(GgufMatMul::new(QuantMethodConfig::Gguf {
                q_weight: res,
                b: self.b.clone(),
            })?))
        } else {
            let w = match &self.w {
                QMatMul::QTensor(q) => QMatMul::QTensor(Arc::new(QTensor::quantize(
                    &q.dequantize(&device)?,
                    q.dtype(),
                )?)),
                QMatMul::Tensor(t) => QMatMul::Tensor(t.to_device(&device)?),
                QMatMul::TensorF16(t) => QMatMul::TensorF16(t.to_device(&device)?),
            };
            let b = if let Some(b) = &self.b {
                Some(b.to_device(&device)?)
            } else {
                None
            };
            Ok(Arc::new(GgufMatMul {
                w,
                b,
                relocated_qweights: Mutex::new(Vec::new()),
            }))
        }
    }
}

impl GgufMatMul {
    fn forward_with_weight(&self, weight: QMatMul, a: &Tensor) -> Result<Tensor> {
        let x = weight.forward(a)?;
        if let Some(ref b) = self.b {
            let b = if b.device().same_device(a.device()) {
                b.clone()
            } else {
                b.to_device(a.device())?
            };
            x.broadcast_add(&b)
        } else {
            Ok(x)
        }
    }

    fn qtensor_for_device(&self, q: &Arc<QTensor>, device: &Device) -> Result<Arc<QTensor>> {
        if q.device().same_device(device) {
            return Ok(q.clone());
        }

        let target_location = device.location();
        let mut cache = self
            .relocated_qweights
            .lock()
            .expect("GgufMatMul relocation cache poisoned");
        if let Some((_, cached)) = cache
            .iter()
            .find(|(location, _)| *location == target_location)
        {
            return Ok(cached.clone());
        }

        let data = q.data()?;
        let storage = QStorage::from_data(data, device, q.dtype())?;
        let relocated = Arc::new(QTensor::new(storage, q.shape())?);
        cache.push((target_location, relocated.clone()));
        Ok(relocated)
    }
}

// Serialization structure:
//
// -----------------------
// UQFF version, u32, little endian
// -----------------------
// ISQ type (0 for GGUF), u8, little endian
// -----------------------
// Tensor data length in bytes, u32, little endian
// -----------------------
// Whether bias data is included, u8 boolean
// -----------------------
// Quantized dtype, u32, little endian
// -----------------------
// Num shape dims, u32, little endian
// -----------------------
// ...
// Array (in original order): quantized weight shape dims, u32, little endian
// ...
// -----------------------
// ...
// Array: quantized weight data, u8s
// ...
// -----------------------
// [OPTIONAL] Bias tensor data generated by `serialize_tensor`. Refer to its docs for layout.
// -----------------------

impl QuantizedSerde for GgufMatMul {
    fn isq_serde_supported(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "gguf"
    }
    fn serialize(&self) -> Result<Cow<'_, [u8]>> {
        self.serialize_with_bias(self.b.clone())
    }
    fn serialize_with_bias(&self, bias: Option<Tensor>) -> Result<Cow<'_, [u8]>> {
        let mut buffer = match &self.w {
            QMatMul::QTensor(qw) => {
                let w = qw.data()?.to_vec();
                let w_shape = qw.shape().dims();
                let dtype: u32 = match qw.dtype() {
                    GgmlDType::F32 => 0,
                    GgmlDType::F16 => 1,
                    GgmlDType::Q4_0 => 2,
                    GgmlDType::Q4_1 => 3,
                    GgmlDType::Q5_0 => 6,
                    GgmlDType::Q5_1 => 7,
                    GgmlDType::Q8_0 => 8,
                    GgmlDType::Q8_1 => 9,
                    GgmlDType::Q2K => 10,
                    GgmlDType::Q3K => 11,
                    GgmlDType::Q4K => 12,
                    GgmlDType::Q5K => 13,
                    GgmlDType::Q6K => 14,
                    GgmlDType::Q8K => 15,
                    // https://github.com/ggerganov/ggml/blob/29d87fc6676e7ed0cdfdec0804b06001d9c2bb44/include/ggml.h#L389
                    GgmlDType::BF16 => 30,
                };

                let mut buffer = Vec::new();

                // Version is always first!
                buffer.extend(&UQFF_VERSION.to_le_bytes());

                // ISQ type for GGUF is 0
                buffer.push(QuantizedSerdeType::Gguf as u8);

                // Length
                buffer.extend(&(w.len() as u32).to_le_bytes());

                // Has bias
                buffer.push(bias.is_some() as u8);

                // Dtype (u32)
                buffer.extend(&dtype.to_le_bytes());

                // Shape
                buffer.extend((w_shape.len() as u32).to_le_bytes());
                for dim in w_shape {
                    buffer.extend((*dim as u32).to_le_bytes());
                }

                // Quantized W Vec<u8> (just append it)
                buffer.extend(&w);

                buffer
            }
            QMatMul::TensorF16(_) | QMatMul::Tensor(_) => {
                candle_core::bail!("Cannot serialize non-quantized")
            }
        };

        if let Some(b) = bias.as_ref() {
            serialize_tensor(&mut buffer, b)?;
        }

        Ok(Cow::from(buffer))
    }

    fn deserialize(
        data: Cow<[u8]>,
        device: &Device,
        _comm: &Arc<crate::Comm>,
        guard: QuantizeOntoGuard,
    ) -> Result<Arc<dyn QuantMethod>> {
        let mut buffer = Cursor::new(data);

        let version = buffer.read_u32::<LittleEndian>()?;
        if let Err(e) = version_is_compatible(version) {
            return Err(candle_core::Error::wrap(e));
        }

        let isq_type = buffer.read_u8()? as usize;
        if isq_type != QuantizedSerdeType::Gguf as usize {
            candle_core::bail!(
                "ISQ type ({isq_type}) doesn't match expected type {}",
                QuantizedSerdeType::Gguf as usize
            );
        }

        let data_len = buffer.read_u32::<LittleEndian>()? as usize;

        let has_bias = buffer.read_u8()? != 0;

        // TODO: keep this in sync with get_isq_type_from_uqff!
        let dtype = buffer.read_u32::<LittleEndian>()?;
        let dtype = match dtype {
            0 => GgmlDType::F32,
            1 => GgmlDType::F16,
            2 => GgmlDType::Q4_0,
            3 => GgmlDType::Q4_1,
            6 => GgmlDType::Q5_0,
            7 => GgmlDType::Q5_1,
            8 => GgmlDType::Q8_0,
            9 => GgmlDType::Q8_1,
            10 => GgmlDType::Q2K,
            11 => GgmlDType::Q3K,
            12 => GgmlDType::Q4K,
            13 => GgmlDType::Q5K,
            14 => GgmlDType::Q6K,
            15 => GgmlDType::Q8K,
            // https://github.com/ggerganov/ggml/blob/29d87fc6676e7ed0cdfdec0804b06001d9c2bb44/include/ggml.h#L389
            30 => GgmlDType::BF16,
            _ => candle_core::bail!("unknown dtype for quantized weight tensor {dtype}"),
        };

        let n_dims = buffer.read_u32::<LittleEndian>()? as usize;

        let mut dims = Vec::with_capacity(n_dims);
        for _ in 0..n_dims {
            dims.push(buffer.read_u32::<LittleEndian>()? as usize)
        }

        let mut tensor_data = vec![0; data_len];
        buffer.read_exact(&mut tensor_data)?;

        let _acquired_load_guard = guard.acquire(device);
        // If we have bias
        let b = if has_bias {
            Some(deserialize_tensor(&mut buffer, device)?)
        } else {
            None
        };

        let w = qtensor_from_ggml(dtype, &tensor_data, dims, device)?;
        Ok(Arc::new(Self {
            w: QMatMul::QTensor(w.into()),
            b,
            relocated_qweights: Mutex::new(Vec::new()),
        }))
    }
    fn deserialize_ext_bias(
        data: Cow<[u8]>,
        device: &Device,
        guard: QuantizeOntoGuard,
    ) -> Result<(Arc<dyn QuantMethod>, Option<Tensor>)> {
        let mut buffer = Cursor::new(data);

        let version = buffer.read_u32::<LittleEndian>()?;
        if let Err(e) = version_is_compatible(version) {
            return Err(candle_core::Error::wrap(e));
        }

        let isq_type = buffer.read_u8()? as usize;
        if isq_type != QuantizedSerdeType::Gguf as usize {
            candle_core::bail!(
                "ISQ type ({isq_type}) doesn't match expected type {}",
                QuantizedSerdeType::Gguf as usize
            );
        }

        let data_len = buffer.read_u32::<LittleEndian>()? as usize;

        let has_bias = buffer.read_u8()? != 0;

        // TODO: keep this in sync with get_isq_type_from_uqff!
        let dtype = buffer.read_u32::<LittleEndian>()?;
        let dtype = match dtype {
            0 => GgmlDType::F32,
            1 => GgmlDType::F16,
            2 => GgmlDType::Q4_0,
            3 => GgmlDType::Q4_1,
            6 => GgmlDType::Q5_0,
            7 => GgmlDType::Q5_1,
            8 => GgmlDType::Q8_0,
            9 => GgmlDType::Q8_1,
            10 => GgmlDType::Q2K,
            11 => GgmlDType::Q3K,
            12 => GgmlDType::Q4K,
            13 => GgmlDType::Q5K,
            14 => GgmlDType::Q6K,
            15 => GgmlDType::Q8K,
            // https://github.com/ggerganov/ggml/blob/29d87fc6676e7ed0cdfdec0804b06001d9c2bb44/include/ggml.h#L389
            30 => GgmlDType::BF16,
            _ => candle_core::bail!("unknown dtype for quantized weight tensor {dtype}"),
        };

        let n_dims = buffer.read_u32::<LittleEndian>()? as usize;

        let mut dims = Vec::with_capacity(n_dims);
        for _ in 0..n_dims {
            dims.push(buffer.read_u32::<LittleEndian>()? as usize)
        }

        let mut tensor_data = vec![0; data_len];
        buffer.read_exact(&mut tensor_data)?;

        let _acquired_load_guard = guard.acquire(device);
        // If we have bias
        let b = if has_bias {
            Some(deserialize_tensor(&mut buffer, device)?)
        } else {
            None
        };

        let w = qtensor_from_ggml(dtype, &tensor_data, dims, device)?;
        Ok((
            Arc::new(Self {
                w: QMatMul::QTensor(w.into()),
                b: None,
                relocated_qweights: Mutex::new(Vec::new()),
            }),
            b,
        ))
    }
}

impl GgufMatMul {
    /// Return the underlying `Arc<QTensor>` that this `GgufMatMul` wraps,
    /// if the weight is a quantized tensor (not a dense BF16/F16 fallback).
    /// Used by [`stack_gguf_experts`] to build an `(n_experts, out, in)`
    /// stacked tensor out of per-expert caches at MoE load time, so the
    /// forward path can dispatch through a single grouped-GEMM kernel
    /// instead of N per-expert matmul launches.
    pub fn qtensor_arc(&self) -> Option<Arc<QTensor>> {
        match &self.w {
            QMatMul::QTensor(q) => Some(q.clone()),
            _ => None,
        }
    }

    pub fn get_isq_type_from_uqff(data: Cow<[u8]>) -> Result<IsqType> {
        let mut buffer = Cursor::new(data);

        let version = buffer.read_u32::<LittleEndian>()?;
        if let Err(e) = version_is_compatible(version) {
            return Err(candle_core::Error::wrap(e));
        }

        let isq_type = buffer.read_u8()? as usize;
        if isq_type != QuantizedSerdeType::Gguf as usize {
            candle_core::bail!(
                "ISQ type ({isq_type}) doesn't match expected type {}",
                QuantizedSerdeType::Gguf as usize
            );
        }

        let _ = buffer.read_u32::<LittleEndian>()? as usize;

        let _ = buffer.read_u8()? != 0;

        let dtype = buffer.read_u32::<LittleEndian>()?;
        let dtype = match dtype {
            0 => GgmlDType::F32,
            1 => GgmlDType::F16,
            2 => GgmlDType::Q4_0,
            3 => GgmlDType::Q4_1,
            6 => GgmlDType::Q5_0,
            7 => GgmlDType::Q5_1,
            8 => GgmlDType::Q8_0,
            9 => GgmlDType::Q8_1,
            10 => GgmlDType::Q2K,
            11 => GgmlDType::Q3K,
            12 => GgmlDType::Q4K,
            13 => GgmlDType::Q5K,
            14 => GgmlDType::Q6K,
            15 => GgmlDType::Q8K,
            // https://github.com/ggerganov/ggml/blob/29d87fc6676e7ed0cdfdec0804b06001d9c2bb44/include/ggml.h#L389
            30 => GgmlDType::BF16,
            _ => candle_core::bail!("unknown dtype for quantized weight tensor {dtype}"),
        };

        IsqType::try_from(dtype)
    }
}

/// Stack `num_experts` per-expert [`GgufMatMul`] arcs into a single
/// [`GgufMatMul`] whose backing [`QTensor`] has shape
/// `(num_experts, out_features, in_features)`. Enables the candle
/// `qmatmul_indexed_moe_forward` grouped-GEMM CUDA kernel to dispatch
/// all top-k experts in one launch — eliminating the per-expert matmul
/// loop that otherwise caps MoE decode throughput for per-expert
/// backends.
///
/// All inputs must be GGUF-backed quantized tensors (ISQ output), all
/// on the same device, all sharing the same Q-dtype and per-expert
/// shape. Bias is unsupported in the stacked form (the MoE forward
/// path never uses it).
///
/// Concatenation works at the raw block-byte level: the Q-format's
/// block layout is row-major within each expert, and experts are
/// concatenated along a new leading axis — no unpacking, no
/// re-quantization, so no accuracy loss.
pub fn stack_gguf_experts(
    experts: &[Arc<dyn QuantMethod>],
    target_device: &Device,
) -> Result<Arc<dyn QuantMethod>> {
    if experts.is_empty() {
        candle_core::bail!("stack_gguf_experts: experts vec is empty");
    }
    // `QuantMethod` isn't `Any`, so we can't downcast the trait object.
    // Instead we go through `QuantizedSerde::serialize` which produces a
    // UQFF-versioned byte stream that embeds the raw Q-block bytes along
    // with shape + dtype. Parsing is ~20 lines, stable across crate
    // versions (the header format is pinned by `UQFF_VERSION`), and
    // avoids any assumptions about the concrete `QuantMethod` type.
    let mut per_expert_w: Vec<Vec<u8>> = Vec::with_capacity(experts.len());
    let mut first_dtype: Option<GgmlDType> = None;
    let mut first_shape: Option<Vec<usize>> = None;
    for (i, e) in experts.iter().enumerate() {
        if e.name() != "gguf" {
            candle_core::bail!(
                "stack_gguf_experts: expert[{i}] backend is `{}`, expected `gguf`",
                e.name()
            );
        }
        let bytes = e.serialize().map_err(|err| {
            candle_core::Error::msg(format!("stack_gguf_experts: serialize[{i}] failed: {err}"))
        })?;
        let mut cur = std::io::Cursor::new(bytes.as_ref());
        // version (u32) + type (u8) + w_len (u32) + has_bias (u8)
        // + dtype (u32) + shape_len (u32) + dims (u32 each) + w bytes
        let _version = cur.read_u32::<LittleEndian>()?;
        let type_tag = {
            let mut b = [0u8; 1];
            cur.read_exact(&mut b)?;
            b[0]
        };
        if type_tag != QuantizedSerdeType::Gguf as u8 {
            candle_core::bail!(
                "stack_gguf_experts: expert[{i}] type tag {} != Gguf",
                type_tag
            );
        }
        let w_len = cur.read_u32::<LittleEndian>()? as usize;
        let _has_bias = {
            let mut b = [0u8; 1];
            cur.read_exact(&mut b)?;
            b[0]
        };
        let dtype_u = cur.read_u32::<LittleEndian>()?;
        let dtype = match dtype_u {
            0 => GgmlDType::F32,
            1 => GgmlDType::F16,
            2 => GgmlDType::Q4_0,
            3 => GgmlDType::Q4_1,
            6 => GgmlDType::Q5_0,
            7 => GgmlDType::Q5_1,
            8 => GgmlDType::Q8_0,
            9 => GgmlDType::Q8_1,
            10 => GgmlDType::Q2K,
            11 => GgmlDType::Q3K,
            12 => GgmlDType::Q4K,
            13 => GgmlDType::Q5K,
            14 => GgmlDType::Q6K,
            15 => GgmlDType::Q8K,
            30 => GgmlDType::BF16,
            other => candle_core::bail!("stack_gguf_experts: unknown dtype tag {other}"),
        };
        let shape_len = cur.read_u32::<LittleEndian>()? as usize;
        let mut shape = Vec::with_capacity(shape_len);
        for _ in 0..shape_len {
            shape.push(cur.read_u32::<LittleEndian>()? as usize);
        }
        // Now cursor is at the start of the raw W bytes.
        let offset = cur.position() as usize;
        let w_slice = &bytes[offset..offset + w_len];
        if let Some(fs) = &first_shape {
            if fs != &shape {
                candle_core::bail!(
                    "stack_gguf_experts: shape mismatch expert[{i}]={:?} != expert[0]={:?}",
                    shape,
                    fs
                );
            }
            if first_dtype != Some(dtype) {
                candle_core::bail!(
                    "stack_gguf_experts: dtype mismatch expert[{i}]={:?} != expert[0]={:?}",
                    dtype,
                    first_dtype
                );
            }
        } else {
            first_shape = Some(shape);
            first_dtype = Some(dtype);
        }
        per_expert_w.push(w_slice.to_vec());
    }

    let first_shape = first_shape.unwrap();
    let first_dtype = first_dtype.unwrap();
    if first_shape.len() != 2 {
        candle_core::bail!(
            "stack_gguf_experts: expected per-expert rank-2 QTensor, got shape {:?}",
            first_shape
        );
    }

    // Concatenate block-packed W bytes. Block layout is row-major per
    // expert, so prepending a leading expert axis just lays out experts
    // back-to-back — which is exactly what a stacked rank-3 QTensor
    // expects, and what `qmatmul_indexed_moe_forward` consumes.
    let total_bytes: usize = per_expert_w.iter().map(|v| v.len()).sum();
    let mut stacked_bytes = Vec::with_capacity(total_bytes);
    for w in &per_expert_w {
        stacked_bytes.extend_from_slice(w);
    }

    let stacked_shape: Vec<usize> = std::iter::once(experts.len())
        .chain(first_shape.iter().copied())
        .collect();

    let storage = QStorage::from_data(Cow::Owned(stacked_bytes), target_device, first_dtype)?;
    let q_weight = Arc::new(QTensor::new(storage, stacked_shape)?);

    Ok(Arc::new(GgufMatMul::new(QuantMethodConfig::Gguf {
        q_weight,
        b: None,
    })?))
}
