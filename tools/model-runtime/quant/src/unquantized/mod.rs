use std::{
    borrow::Cow,
    io::Cursor,
    sync::{atomic::AtomicUsize, Arc},
};

use byteorder::{LittleEndian, ReadBytesExt};
use candle_core::{quantized::GgmlDType, DType, Device, DeviceLocation, Result, Shape, Tensor, D};
use candle_nn::Linear;

use crate::{
    cublaslt::{maybe_init_cublas_lt_wrapper, CUBLASLT_CONTROLLER},
    generate_isq, generate_isq_imatrix,
    hqq::{HqqAxis, HqqBits, HqqConfig, HqqLayer, ISQ_HQQ_DEFAULT_OPT_STEPS, ISQ_HQQ_GROUP_SIZE},
    utils::{deserialize_tensor, serialize_tensor, version_is_compatible, UQFF_VERSION},
    AfqBits, AfqGroupSize, AfqLayer, FP8Linear, GgufMatMul, ImatrixLayerStats, IsqType, MatMul,
    QuantMethod, QuantMethodConfig, QuantizeOntoGuard, QuantizedSerde, QuantizedSerdeType,
};

fn pick_cuda_indexed_moe_dtype(weight: &Tensor, requested: GgmlDType) -> GgmlDType {
    let Some(&last_dim) = weight.shape().dims().last() else {
        return requested;
    };
    if last_dim.is_multiple_of(requested.block_size()) {
        return requested;
    }

    for candidate in [GgmlDType::Q5_1, GgmlDType::Q8_0] {
        if last_dim.is_multiple_of(candidate.block_size()) {
            return candidate;
        }
    }

    requested
}

#[derive(Debug)]
pub struct UnquantLinear {
    w: Tensor,
    b: Option<Tensor>,
    stats: Option<ImatrixLayerStats>,
}

impl UnquantLinear {
    fn debug_context(&self, a: &Tensor, w: &Tensor, branch: &'static str) -> String {
        let bias_summary = self.b.as_ref().map(|bias| {
            format!(
                "shape={:?} dtype={:?} device={:?}",
                bias.dims(),
                bias.dtype(),
                bias.device().location()
            )
        });
        format!(
            "unquantized.forward branch={branch} input_shape={:?} input_rank={} input_dtype={:?} input_device={:?} weight_shape={:?} weight_dtype={:?} weight_device={:?} bias={bias_summary:?}",
            a.dims(),
            a.rank(),
            a.dtype(),
            a.device().location(),
            w.dims(),
            w.dtype(),
            w.device().location(),
        )
    }
}

impl QuantMethod for UnquantLinear {
    fn new(method: QuantMethodConfig) -> candle_core::Result<Self>
    where
        Self: Sized,
    {
        match method {
            QuantMethodConfig::Gguf { .. }
            | QuantMethodConfig::GptqAwq { .. }
            | QuantMethodConfig::Hqq { .. }
            | QuantMethodConfig::Dummy
            | QuantMethodConfig::FP8 { .. }
            | QuantMethodConfig::Bnb { .. }
            | QuantMethodConfig::BlockwiseFP8 { .. }
            | QuantMethodConfig::PerTensorFP8 { .. }
            | QuantMethodConfig::Afq { .. }
            | QuantMethodConfig::MXFP4 { .. } => unreachable!(),
            QuantMethodConfig::Unquantized(l) => Ok(Self {
                w: l.weight().clone(),
                b: l.bias().cloned(),
                stats: None,
            }),
        }
    }

    fn dequantize_w(&self) -> Result<Tensor> {
        Ok(self.w.clone())
    }

    fn forward(&self, a: &Tensor) -> Result<Tensor> {
        // Batch matrix multiplication
        maybe_init_cublas_lt_wrapper(a.device().clone());

        // Try custom GEMV for single-token decode (batch_size=1)
        #[cfg(feature = "cuda")]
        if crate::gemv::should_use_gemv(a, &self.w) {
            return crate::gemv::gemv(a, &self.w, self.b.as_ref());
        }

        let w = match *a.dims() {
            [b1, b2, _, _] => self.w.broadcast_left((b1, b2))?,
            [bsize, _, _] => self.w.broadcast_left(bsize)?,
            _ => self.w.clone(),
        };

        if let Some(stats) = &self.stats {
            stats.process(a)?;
        }

        if let Some(b) = self.b.as_ref() {
            let mut tgt_shape = a.dims().to_vec();
            tgt_shape[a.dims().len() - 1] = w.dim(D::Minus2)?;
            let b = b.broadcast_as(Shape::from_dims(&tgt_shape))?;

            match a.device().location() {
                DeviceLocation::Cuda { .. } => {
                    // Try to use cublaslt, otherwise fallback to gemm
                    if let (Device::Cuda(_), Some(cublaslt)) =
                        (a.device(), CUBLASLT_CONTROLLER.get_for_device(a.device()))
                    {
                        cublaslt
                            .batch_matmul(
                                a,
                                &w,
                                Some(&b.t()?.contiguous()?),
                                None,
                                Some(1.0),
                                None,
                                None,
                            )?
                            .t()
                            .map_err(|e| {
                                e.context(self.debug_context(a, &w, "cuda-batch-matmul-t"))
                            })
                    } else {
                        let wt = w
                            .t()
                            .map_err(|e| e.context(self.debug_context(a, &w, "cuda-weight-t")))?;
                        let matmul_result = a
                            .matmul(&wt)
                            .map_err(|e| e.context(self.debug_context(a, &w, "cuda-matmul")))?;
                        matmul_result
                            .broadcast_add(&b)
                            .map_err(|e| e.context(self.debug_context(a, &w, "cuda-bias-add")))
                    }
                }
                DeviceLocation::Metal { .. } => {
                    let wt = w
                        .t()
                        .map_err(|e| e.context(self.debug_context(a, &w, "metal-weight-t")))?;
                    let matmul_result = a
                        .matmul(&wt)
                        .map_err(|e| e.context(self.debug_context(a, &w, "metal-matmul")))?;
                    matmul_result
                        .broadcast_add(&b)
                        .map_err(|e| e.context(self.debug_context(a, &w, "metal-bias-add")))
                }
                DeviceLocation::Cpu => {
                    #[cfg(feature = "accelerate")]
                    {
                        let original_dtype = a.dtype();
                        let a_f32 = a.to_dtype(DType::F32)?;
                        let w_f32 = w.t()?.to_dtype(DType::F32)?;
                        let b_f32 = b.to_dtype(DType::F32)?;
                        let matmul_result = a_f32.matmul(&w_f32)?;
                        matmul_result
                            .broadcast_add(&b_f32)?
                            .to_dtype(original_dtype)
                    }
                    #[cfg(not(feature = "accelerate"))]
                    {
                        let wt = w
                            .t()
                            .map_err(|e| e.context(self.debug_context(a, &w, "cpu-weight-t")))?;
                        let matmul_result = a
                            .matmul(&wt)
                            .map_err(|e| e.context(self.debug_context(a, &w, "cpu-matmul")))?;
                        matmul_result
                            .broadcast_add(&b)
                            .map_err(|e| e.context(self.debug_context(a, &w, "cpu-bias-add")))
                    }
                }
            }
        } else if let (Device::Cuda(_), Some(cublaslt)) =
            (a.device(), CUBLASLT_CONTROLLER.get_for_device(a.device()))
        {
            // cuBLAS batch_matmul requires 3D tensors, fall back to regular matmul for 2D
            if a.rank() >= 3 && w.rank() >= 3 {
                cublaslt
                    .batch_matmul(a, &w, None, None, None, None, None)?
                    .t()
                    .map_err(|e| {
                        e.context(self.debug_context(a, &w, "cuda-batch-matmul-no-bias-t"))
                    })
            } else {
                let wt = w
                    .t()
                    .map_err(|e| e.context(self.debug_context(a, &w, "cuda-no-bias-weight-t")))?;
                MatMul
                    .matmul(a, &wt)
                    .map_err(|e| e.context(self.debug_context(a, &w, "cuda-no-bias-matmul")))
            }
        } else {
            let wt = w
                .t()
                .map_err(|e| e.context(self.debug_context(a, &w, "default-no-bias-weight-t")))?;
            MatMul
                .matmul(a, &wt)
                .map_err(|e| e.context(self.debug_context(a, &w, "default-no-bias-matmul")))
        }
    }

    fn gather_forward(&self, a: &Tensor, indices: &Tensor) -> Result<Tensor> {
        let a = if a.device().same_device(&self.w.device()) {
            a.clone()
        } else {
            a.to_device(&self.w.device())?
        };
        let indices = if indices.device().same_device(&self.w.device()) {
            indices.clone()
        } else {
            indices.to_device(&self.w.device())?
        };

        // Weights are [num_experts, out_features, in_features]
        // For Metal path:
        //   - a: (b_size, seq_len, 1, 1, hidden_dim) - 5D
        //   - indices: (b_size, seq_len, num_experts_per_tok) - 3D
        // For CUDA path:
        //   - a: (num_tokens, 1, hidden_dim) - 3D
        //   - indices: (num_tokens, num_experts_per_tok) - 2D

        let w = &self.w;
        let (_num_experts, out_features, _in_features) = w.dims3()?;

        match a.dims() {
            // Metal path: 5D input (b_size, seq_len, 1, 1, hidden_dim)
            &[b_size, seq_len, 1, 1, hidden_dim] => {
                let (_b, _s, num_experts_per_tok) = indices.dims3()?;
                // Flatten indices to select experts
                let flat_indices = indices.reshape((b_size * seq_len * num_experts_per_tok,))?;

                // Select expert weights: [b*s*k, out_features, in_features]
                let selected_w = w.index_select(&flat_indices, 0)?;

                // Reshape input: [b*s, hidden_dim]
                let a_flat = a.reshape((b_size * seq_len, hidden_dim))?;

                // For each token, we need to compute with each selected expert
                // Broadcast a to match: [b*s, 1, hidden_dim] -> [b*s, k, hidden_dim]
                let a_expanded = a_flat
                    .unsqueeze(1)?
                    .broadcast_as((b_size * seq_len, num_experts_per_tok, hidden_dim))?
                    .reshape((b_size * seq_len * num_experts_per_tok, hidden_dim))?;

                // Matmul: [b*s*k, hidden_dim] @ [b*s*k, hidden_dim, out_features] -> [b*s*k, out_features]
                let selected_w_t = selected_w.transpose(1, 2).map_err(|e| {
                    e.context(self.debug_context(&a_expanded, w, "gather-metal-selected-weight-t"))
                })?;
                let result = a_expanded
                    .unsqueeze(1)?
                    .matmul(&selected_w_t)
                    .map_err(|e| {
                        e.context(self.debug_context(&a_expanded, w, "gather-metal-matmul"))
                    })?
                    .squeeze(1)?;

                // Reshape back to [b, s, k, out_features]
                result.reshape((b_size, seq_len, num_experts_per_tok, out_features))
            }
            // CUDA path: 3D input (num_tokens, 1, hidden_dim)
            &[num_tokens, 1, hidden_dim] => {
                let (_, num_experts_per_tok) = indices.dims2()?;

                // Flatten indices
                let flat_indices = indices.reshape((num_tokens * num_experts_per_tok,))?;

                // Select expert weights: [n*k, out_features, in_features]
                let selected_w = w.index_select(&flat_indices, 0)?;

                // Broadcast input: [n, 1, hidden] -> [n, k, hidden] -> [n*k, hidden]
                let a_expanded = a
                    .broadcast_as((num_tokens, num_experts_per_tok, hidden_dim))?
                    .reshape((num_tokens * num_experts_per_tok, hidden_dim))?;

                // Matmul: [n*k, hidden] @ [n*k, hidden, out] -> [n*k, out]
                let selected_w_t = selected_w.transpose(1, 2).map_err(|e| {
                    e.context(self.debug_context(&a_expanded, w, "gather-cuda-selected-weight-t"))
                })?;
                let result = a_expanded
                    .unsqueeze(1)?
                    .matmul(&selected_w_t)
                    .map_err(|e| {
                        e.context(self.debug_context(&a_expanded, w, "gather-cuda-matmul"))
                    })?
                    .squeeze(1)?;

                // Reshape to [n, k, out]
                result.reshape((num_tokens, num_experts_per_tok, out_features))
            }
            // CUDA path when the input already carries one slice per selected expert:
            // [num_tokens, num_experts_per_tok, hidden_dim]
            &[num_tokens, num_experts_per_tok, hidden_dim] => {
                let (_, expected_num_experts_per_tok) = indices.dims2()?;
                if num_experts_per_tok != expected_num_experts_per_tok {
                    candle_core::bail!(
                        "UnquantLinear::gather_forward: input expert dim {num_experts_per_tok} does not match indices dim {expected_num_experts_per_tok}"
                    );
                }

                let flat_indices = indices.reshape((num_tokens * num_experts_per_tok,))?;
                let selected_w = w.index_select(&flat_indices, 0)?;
                let a_expanded = a.reshape((num_tokens * num_experts_per_tok, hidden_dim))?;

                let selected_w_t = selected_w.transpose(1, 2).map_err(|e| {
                    e.context(self.debug_context(
                        &a_expanded,
                        w,
                        "gather-cuda-preexpanded-selected-weight-t",
                    ))
                })?;
                let result = a_expanded
                    .unsqueeze(1)?
                    .matmul(&selected_w_t)
                    .map_err(|e| {
                        e.context(self.debug_context(
                            &a_expanded,
                            w,
                            "gather-cuda-preexpanded-matmul",
                        ))
                    })?
                    .squeeze(1)?;

                result.reshape((num_tokens, num_experts_per_tok, out_features))
            }
            dims => {
                candle_core::bail!(
                    "UnquantLinear::gather_forward: unsupported input shape {:?}",
                    dims
                );
            }
        }
    }

    fn quantized_act_type(&self) -> Option<DType> {
        None
    }

    fn add_delta_w(&self, delta: &Tensor) -> Result<Arc<dyn QuantMethod>> {
        Ok(Arc::new(Self {
            w: (&self.w + delta)?,
            b: self.b.clone(),
            stats: self.stats.clone(),
        }))
    }

    fn dtype_and_device(&self) -> (DType, candle_core::Device) {
        (self.w.dtype(), self.w.device().clone())
    }

    fn apply_isq(
        self: Arc<Self>,
        dtype: Option<IsqType>,
        device: Device,
        n_quantized: &AtomicUsize,
        imatrix_weight: Option<Vec<f32>>,
        guard: QuantizeOntoGuard,
    ) -> Result<Arc<dyn QuantMethod>> {
        match dtype {
            /*Some(IsqType::HQQ1 | IsqType::HQQ2 | IsqType::HQQ3 | */
            Some(IsqType::HQQ4 | IsqType::HQQ8) => {
                let _acquired_quantize_guard = guard.acquire(&device);
                if imatrix_weight.is_some() {
                    // TODO just warn?
                    candle_core::bail!("HQQ does not support imatrix.");
                }

                n_quantized.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let bits = match dtype.unwrap() {
                    IsqType::HQQ8 => HqqBits::Eight,
                    IsqType::HQQ4 => HqqBits::Four,
                    // IsqType::HQQ3 => HqqBits::Three,
                    // IsqType::HQQ2 => HqqBits::Two,
                    // IsqType::HQQ1 => HqqBits::One,
                    _ => unreachable!(),
                };
                let cfg = HqqConfig {
                    bits,
                    group_size: ISQ_HQQ_GROUP_SIZE.try_into()?,
                    axis: HqqAxis::Zero,
                    optimization_steps: ISQ_HQQ_DEFAULT_OPT_STEPS,
                    round_zeros: false,
                    channel_wise: true,
                };
                let res = HqqLayer::quantize(&self.w.to_device(&device)?, &device, cfg)?;
                if let Some(bias) = &self.b {
                    let bias = bias
                        .to_device(&device)?
                        .to_dtype(res.dtype_and_device().0)?;
                    Ok(Arc::new(res.with_bias(bias)))
                } else {
                    Ok(Arc::new(res))
                }
            }
            Some(IsqType::AFQ2 | IsqType::AFQ3 | IsqType::AFQ4 | IsqType::AFQ6 | IsqType::AFQ8) => {
                let _acquired_quantize_guard = guard.acquire(&device);
                if imatrix_weight.is_some() {
                    // TODO just warn?
                    candle_core::bail!("AFQ does not support imatrix.");
                }

                n_quantized.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let bits = match dtype.unwrap() {
                    IsqType::AFQ8 => AfqBits::Eight,
                    IsqType::AFQ6 => AfqBits::Six,
                    IsqType::AFQ4 => AfqBits::Four,
                    IsqType::AFQ3 => AfqBits::Three,
                    IsqType::AFQ2 => AfqBits::Two,
                    _ => unreachable!(),
                };

                Ok(Arc::new(AfqLayer::new(QuantMethodConfig::Afq {
                    weight: self.w.to_device(&device)?,
                    bias: self.b.as_ref().map(|b| b.to_device(&device).unwrap()),
                    bits,
                    group_size: AfqGroupSize::default(),
                })?))
            }
            Some(
                IsqType::Q2K
                | IsqType::Q3K
                | IsqType::Q4K
                | IsqType::Q4_0
                | IsqType::Q4_1
                | IsqType::Q5K
                | IsqType::Q5_0
                | IsqType::Q5_1
                | IsqType::Q6K
                | IsqType::Q8K
                | IsqType::Q8_0
                | IsqType::Q8_1,
            ) => {
                let requested_dtype: GgmlDType = dtype.unwrap().try_into()?;
                // Immediate ISQ must quantize from CPU tensors first; otherwise large CUDA
                // intermediates can be materialized before the compact qstorage exists.
                let weight_for_isq = if self.w.device().is_cpu() {
                    self.w.clone()
                } else {
                    self.w.to_device(&Device::Cpu)?
                };
                let dtype = if matches!(device, Device::Cuda(_)) && weight_for_isq.rank() == 3 {
                    let picked = pick_cuda_indexed_moe_dtype(&weight_for_isq, requested_dtype);
                    if picked != requested_dtype {
                        tracing::info!(
                            "Using CUDA indexed-MoE fallback dtype {:?} instead of {:?} for weight shape {:?}",
                            picked,
                            requested_dtype,
                            weight_for_isq.dims()
                        );
                    }
                    picked
                } else {
                    requested_dtype
                };
                let res = if let Some(imatrix_weight) = imatrix_weight {
                    generate_isq_imatrix!(
                        weight_for_isq,
                        imatrix_weight,
                        device,
                        dtype,
                        n_quantized,
                        guard
                    )
                } else if weight_for_isq.rank() == 3 {
                    // MoE-expert fast path: the weight is a
                    // [num_experts, out, in] stack. The standard
                    // `generate_isq!` invocation calls
                    // `QTensor::quantize(&weight, dtype)` which
                    // internally runs `src.to_dtype(F32)` on the
                    // *entire* stack — a peak of BF16+F32 over the
                    // whole [num_experts, out, in] slab on CPU.
                    // For Qwen3.6-35B-A3B that's ~1.5 GB per
                    // projection per layer; with Rayon loading 40
                    // layers × 3 projections concurrently the
                    // transient anon-rss reaches the 60-GiB OOM
                    // threshold on a 62-GiB host.
                    //
                    // Stream per-expert: narrow out one [out, in]
                    // slice, quantize it (F32 transient only ~4 MB
                    // for Qwen3.6 geometry), extend a pre-sized
                    // Q4K byte buffer. Q4K super-blocks never cross
                    // expert boundaries for MoE geometries (out*in
                    // is always a multiple of 256) so the byte-
                    // concat is layout-identical to quantising the
                    // full stack. Peak CPU per call falls from
                    // ~1.5 GB to ~150 MB, comfortably fitting even
                    // dozens of parallel workers.
                    use candle_core::quantized::{QStorage, QTensor};
                    use std::borrow::Cow;

                    let num_experts = weight_for_isq.dim(0)?;
                    let out_dim = weight_for_isq.dim(1)?;
                    let in_dim = weight_for_isq.dim(2)?;
                    let block_size = dtype.block_size();
                    let elem_per_expert = out_dim * in_dim;
                    if !elem_per_expert.is_multiple_of(block_size) {
                        candle_core::bail!(
                            "3D ISQ streaming: per-expert elem_count {elem_per_expert} is not \
                             a multiple of block size {block_size}; byte-concat would split a \
                             super-block. Falling back to 2D would require a different layout."
                        );
                    }
                    let type_size = dtype.type_size();
                    let bytes_per_expert = elem_per_expert / block_size * type_size;
                    let mut q_buf: Vec<u8> =
                        Vec::with_capacity(bytes_per_expert * num_experts);

                    for i in 0..num_experts {
                        let expert = weight_for_isq.narrow(0, i, 1)?.squeeze(0)?;
                        let expert = if expert.is_contiguous() {
                            expert
                        } else {
                            expert.contiguous()?
                        };
                        let qt = QTensor::quantize(&expert, dtype)?;
                        drop(expert);
                        let bytes = qt.data()?;
                        if bytes.len() != bytes_per_expert {
                            candle_core::bail!(
                                "3D ISQ streaming: expert[{i}] produced {} bytes, expected {}",
                                bytes.len(),
                                bytes_per_expert,
                            );
                        }
                        q_buf.extend_from_slice(&bytes);
                        drop(bytes);
                        drop(qt);
                    }

                    let _acquired_quantize_guard = guard.acquire(&device);
                    let storage =
                        QStorage::from_data(Cow::Owned(q_buf), &device, dtype)?;
                    let shape = weight_for_isq.shape().clone();
                    n_quantized.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Arc::new(QTensor::new(storage, shape)?)
                } else {
                    generate_isq!(weight_for_isq, device, dtype, n_quantized, guard)
                };
                Ok(Arc::new(GgufMatMul::new(QuantMethodConfig::Gguf {
                    q_weight: res,
                    b: self
                        .b
                        .as_ref()
                        .map(|b| b.to_dtype(DType::F32).unwrap().to_device(&device).unwrap()),
                })?))
            }
            Some(IsqType::F8E4M3) => {
                let _acquired_quantize_guard = guard.acquire(&device);
                if imatrix_weight.is_some() {
                    // TODO just warn?
                    candle_core::bail!("F8E4M3 does not support imatrix.");
                }

                let w = self.w.to_device(&device)?;
                let b = if let Some(b) = &self.b {
                    Some(b.to_device(&device)?)
                } else {
                    None
                };
                Ok(Arc::new(FP8Linear::new(QuantMethodConfig::FP8 {
                    lin: Linear::new(w, b),
                    dtype: DType::F8E4M3,
                })?))
            }
            Some(IsqType::MXFP4) => {
                let _acquired_quantize_guard = guard.acquire(&device);
                if imatrix_weight.is_some() {
                    candle_core::bail!("MXFP4 does not support imatrix.");
                }

                n_quantized.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let w = self.w.to_device(&device)?;
                let b = self.b.as_ref().map(|b| b.to_device(&device)).transpose()?;
                crate::MXFP4Layer::quantize(&w, b, &device)
            }
            Some(IsqType::F8Q8) => {
                let _acquired_quantize_guard = guard.acquire(&device);
                if imatrix_weight.is_some() {
                    candle_core::bail!("F8Q8 does not support imatrix.");
                }

                let w = self.w.to_device(&device)?;
                let b = if let Some(b) = &self.b {
                    Some(b.to_device(&device)?)
                } else {
                    None
                };
                Ok(Arc::new(crate::F8Q8Linear::from_weight(&w, b)?))
            }
            None => {
                let _acquired_quantize_guard = guard.acquire(&device);
                // Ignore imatrix altogether

                let w = self.w.to_device(&device)?;
                let b = if let Some(b) = &self.b {
                    Some(b.to_device(&device)?)
                } else {
                    None
                };
                Ok(Arc::new(UnquantLinear::new(
                    QuantMethodConfig::Unquantized(Linear::new(w, b)),
                )?))
            }
        }
    }

    fn unquant_weight_bias(&self) -> Option<(Tensor, Option<Tensor>)> {
        Some((self.w.clone(), self.b.clone()))
    }

    fn begin_track_stats(&mut self) -> Result<()> {
        self.stats = Some(ImatrixLayerStats::new(&self.w, self.w.device())?);
        Ok(())
    }

    fn end_track_stats(&self) -> Result<Tensor> {
        if let Some(stats) = &self.stats {
            let imatrix = stats.compute_imatrix()?;
            stats.clear()?;
            Ok(imatrix)
        } else {
            candle_core::bail!("`{}` does not support tracking stats.", self.name())
        }
    }
}

// Serialization structure:
//
// -----------------------
// UQFF version, u32, little endian
// -----------------------
// ISQ type (1 for unquantized), u8, little endian
// -----------------------
// Whether bias data is included, u8 boolean
// -----------------------
// Weight tensor data generated by `serialize_tensor`. Refer to its docs for layout.
// -----------------------
// [OPTIONAL] Bias tensor data generated by `serialize_tensor`. Refer to its docs for layout.
// -----------------------

impl QuantizedSerde for UnquantLinear {
    fn isq_serde_supported(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "unquant-linear"
    }
    fn serialize(&self) -> Result<Cow<'_, [u8]>> {
        self.serialize_with_bias(self.b.clone())
    }
    fn serialize_with_bias(&self, bias: Option<Tensor>) -> Result<Cow<'_, [u8]>> {
        let mut buffer = Vec::new();

        // Version is always first!

        buffer.extend(&UQFF_VERSION.to_le_bytes());

        // ISQ type for unquant is 1
        buffer.push(QuantizedSerdeType::Unquant as u8);

        // Has bias
        buffer.push(bias.is_some() as u8);

        // Weight
        serialize_tensor(&mut buffer, &self.w)?;

        if let Some(bias) = &bias {
            // Bias
            serialize_tensor(&mut buffer, bias)?;
        }

        Ok(Cow::from(buffer))
    }

    fn deserialize(
        data: Cow<[u8]>,
        device: &Device,
        _comm: &Arc<crate::Comm>,
        guard: QuantizeOntoGuard,
    ) -> Result<Arc<dyn QuantMethod>>
    where
        Self: Sized,
    {
        let mut buffer = Cursor::new(data);

        let version = buffer.read_u32::<LittleEndian>()?;
        if let Err(e) = version_is_compatible(version) {
            return Err(candle_core::Error::wrap(e));
        }

        let isq_type = buffer.read_u8()? as usize;
        if isq_type != QuantizedSerdeType::Unquant as usize {
            candle_core::bail!(
                "ISQ type ({isq_type}) doesn't match expected type {}",
                QuantizedSerdeType::Unquant as usize
            );
        }

        let has_bias = buffer.read_u8()? != 0;

        let _acquired_load_guard = guard.acquire(device);
        let w = deserialize_tensor(&mut buffer, device)?;

        let b = if has_bias {
            Some(deserialize_tensor(&mut buffer, device)?)
        } else {
            None
        };

        Ok(Arc::new(Self { w, b, stats: None }))
    }
    fn deserialize_ext_bias(
        data: Cow<[u8]>,
        device: &Device,
        guard: QuantizeOntoGuard,
    ) -> Result<(Arc<dyn QuantMethod>, Option<Tensor>)>
    where
        Self: Sized,
    {
        let mut buffer = Cursor::new(data);

        let version = buffer.read_u32::<LittleEndian>()?;
        if let Err(e) = version_is_compatible(version) {
            return Err(candle_core::Error::wrap(e));
        }

        let isq_type = buffer.read_u8()? as usize;
        if isq_type != QuantizedSerdeType::Unquant as usize {
            candle_core::bail!(
                "ISQ type ({isq_type}) doesn't match expected type {}",
                QuantizedSerdeType::Unquant as usize
            );
        }

        let has_bias = buffer.read_u8()? != 0;

        let _acquired_load_guard = guard.acquire(device);
        let w = deserialize_tensor(&mut buffer, device)?;

        let b = if has_bias {
            Some(deserialize_tensor(&mut buffer, device)?)
        } else {
            None
        };

        Ok((
            Arc::new(Self {
                w,
                b: None,
                stats: None,
            }),
            b,
        ))
    }
}
