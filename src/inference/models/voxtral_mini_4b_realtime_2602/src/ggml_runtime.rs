use crate::audio;
use crate::consts::*;
use crate::ffi;
use crate::kernels::VoxtralSttBackend;
use crate::{Error, Result};
use libc::{c_void, size_t};
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::ptr;
use std::time::Instant;

const GGML_DEFAULT_GRAPH_SIZE: usize = 2048;
const ENC_CHUNK_MEL: i32 = 3000;
const ENC_CHUNK_OVERLAP: i32 = 750;
const MAX_ENC_CHUNK: i64 = 2000;
const N_FREQ: usize = VOX_WINDOW_SIZE / 2 + 1;

type Tensor = *mut ffi::ggml_tensor;

pub struct GgmlVoxtralRuntime {
    model: GgmlModel,
    ctx: GgmlSession,
    backend: VoxtralSttBackend,
    max_decode_tokens: usize,
}

struct GgmlModel {
    ctx: *mut ffi::ggml_context,
    gguf: *mut ffi::gguf_context,
    weights_backend: ffi::ggml_backend_t,
    weights_buffer: ffi::ggml_backend_buffer_t,
    weights_on_accel: bool,
    enc_conv0_weight: Tensor,
    enc_conv0_bias: Tensor,
    enc_conv1_weight: Tensor,
    enc_conv1_bias: Tensor,
    enc_norm_weight: Tensor,
    enc_layers: Vec<EncoderLayer>,
    adapter_0_weight: Tensor,
    adapter_2_weight: Tensor,
    tok_embeddings_weight: Tensor,
    dec_norm_weight: Tensor,
    dec_layers: Vec<DecoderLayer>,
    mel_filters: Option<Tensor>,
    tokenizer_num_special_tokens: i32,
    tokenizer_special_ranks: HashSet<i32>,
    tokenizer_vocab_b64: Vec<String>,
    tokenizer_bytes_cache: HashMap<i32, String>,
}

#[derive(Clone, Copy)]
struct EncoderLayer {
    attn_norm_weight: Tensor,
    attn_q_weight: Tensor,
    attn_q_bias: Tensor,
    attn_k_weight: Tensor,
    attn_v_weight: Tensor,
    attn_v_bias: Tensor,
    attn_o_weight: Tensor,
    attn_o_bias: Tensor,
    ffn_norm_weight: Tensor,
    ffn_w1_weight: Tensor,
    ffn_w2_weight: Tensor,
    ffn_w2_bias: Tensor,
    ffn_w3_weight: Tensor,
}

#[derive(Clone, Copy)]
struct DecoderLayer {
    attn_norm_weight: Tensor,
    attn_q_weight: Tensor,
    attn_k_weight: Tensor,
    attn_v_weight: Tensor,
    attn_o_weight: Tensor,
    ffn_norm_weight: Tensor,
    ffn_w1_weight: Tensor,
    ffn_w2_weight: Tensor,
    ffn_w3_weight: Tensor,
    ada0_weight: Tensor,
    ada2_weight: Tensor,
}

struct GgmlSession {
    backend: ffi::ggml_backend_t,
    backend_cpu: ffi::ggml_backend_t,
    blas_backend: ffi::ggml_backend_t,
    has_accel: bool,
    ctx_persistent: *mut ffi::ggml_context,
    buf_persistent: ffi::ggml_backend_buffer_t,
    encoder_chunk_output: Tensor,
    decoder_logits: Tensor,
    kv_self_k: Tensor,
    kv_self_v: Tensor,
    ctx_enc_full: *mut ffi::ggml_context,
    buf_enc_full: ffi::ggml_backend_buffer_t,
    encoder_output: Tensor,
    ctx_dec_mem: *mut ffi::ggml_context,
    buf_dec_mem: ffi::ggml_backend_buffer_t,
    decoder_memory: Tensor,
    total_enc_tokens: i32,
    enc_seq_used: i32,
    dec_seq_len: i32,
    kv_used: i32,
    sched_encoder: *mut ffi::ggml_backend_sched,
    sched_adapter: *mut ffi::ggml_backend_sched,
    sched_dec_pre: *mut ffi::ggml_backend_sched,
    sched_dec_step: *mut ffi::ggml_backend_sched,
    mel_filters_cpu: Vec<f32>,
    mel_plan: audio::MelSpectrogramPlan,
    time_emb_cpu: Vec<f32>,
}

struct MetaContext {
    ctx: *mut ffi::ggml_context,
    _buf: Vec<u8>,
}

impl Drop for MetaContext {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            unsafe { ffi::ggml_free(self.ctx) };
        }
    }
}

unsafe impl Send for GgmlVoxtralRuntime {}

impl GgmlVoxtralRuntime {
    pub fn load(path: &Path, backend: VoxtralSttBackend) -> Result<Self> {
        let start = Instant::now();
        let model = GgmlModel::load(path, backend)?;
        let ctx = GgmlSession::new(&model, backend)?;
        eprintln!(
            "ctox_voxtral_stt: loaded ggml Q4 GGUF from {} in {:.2}s",
            path.display(),
            start.elapsed().as_secs_f64()
        );
        Ok(Self {
            model,
            ctx,
            backend,
            max_decode_tokens: 256,
        })
    }

    pub fn transcribe_samples(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<String> {
        if sample_rate as usize != VOX_SAMPLE_RATE {
            return Err(Error::Unsupported(
                "Voxtral ggml runtime expects audio already resampled to 16 kHz",
            ));
        }
        if samples.is_empty() {
            return Err(Error::InvalidFormat("transcription audio is empty"));
        }

        let start = Instant::now();
        let padded =
            audio::pad_audio_streaming(&samples, VOX_N_LEFT_PAD_TOKENS, VOX_N_RIGHT_PAD_TOKENS);
        let mut mel = self.ctx.mel_plan.compute(&padded);
        let mut n_frames = padded.len() / VOX_HOP_LENGTH;
        if n_frames % 2 != 0 {
            mel = drop_first_mel_frame(&mel, n_frames);
            n_frames -= 1;
        }

        unsafe {
            let t_encoder = Instant::now();
            self.ctx
                .run_encoder_chunked(&self.model, mel.as_ptr(), n_frames as i32)?;
            let encoder_s = t_encoder.elapsed().as_secs_f64();
            let t_adapter = Instant::now();
            self.ctx.run_adapter(&self.model)?;
            let adapter_s = t_adapter.elapsed().as_secs_f64();
            let t_decode = Instant::now();
            let tokens = self.decode_greedy()?;
            let decode_s = t_decode.elapsed().as_secs_f64();
            let text = self.model.decode_tokens(&tokens);
            let elapsed = start.elapsed().as_secs_f64();
            let audio_s = samples.len() as f64 / VOX_SAMPLE_RATE as f64;
            eprintln!(
                "ctox_voxtral_stt: transcript complete backend={} audio={:.2}s wall={:.2}s rtf={:.2} encoder={:.2}s adapter={:.2}s decode={:.2}s tokens={}",
                self.backend.label(),
                audio_s,
                elapsed,
                elapsed / audio_s.max(0.001),
                encoder_s,
                adapter_s,
                decode_s,
                tokens.len()
            );
            Ok(text)
        }
    }

    unsafe fn decode_greedy(&mut self) -> Result<Vec<i32>> {
        unsafe {
            let n_audio = self.ctx.dec_seq_len;
            let mut prompt = Vec::with_capacity(VOX_N_LEFT_PAD_TOKENS + VOX_N_DELAY_TOKENS + 1);
            prompt.push(VOX_TOKEN_BOS);
            for _ in 0..(VOX_N_LEFT_PAD_TOKENS + VOX_N_DELAY_TOKENS) {
                prompt.push(VOX_TOKEN_STREAMING_PAD);
            }
            let prefix_len = prompt.len() as i32;
            if prefix_len > n_audio {
                return Err(Error::Runtime(format!(
                    "Voxtral prompt length {prefix_len} exceeds audio token count {n_audio}"
                )));
            }

            self.ctx.clear_kv_cache();
            let mut logits = vec![0.0f32; VOX_VOCAB_SIZE];
            if prefix_len > 1 {
                self.ctx.run_decoder_prefill(
                    &self.model,
                    &prompt[..prompt.len() - 1],
                    &mut logits,
                )?;
            }
            self.ctx.run_decoder_step(
                &self.model,
                *prompt.last().unwrap(),
                prefix_len - 1,
                prefix_len - 1,
                &mut logits,
            )?;
            let mut token = argmax(&logits) as i32;
            let mut out = vec![token];
            let mut consecutive_pad = 0;
            let mut seen_text = false;

            for pos in prefix_len..n_audio {
                if token == VOX_TOKEN_EOS || out.len() >= self.max_decode_tokens {
                    break;
                }
                self.ctx
                    .run_decoder_step(&self.model, token, pos, pos, &mut logits)?;
                token = argmax(&logits) as i32;
                out.push(token);

                if token == VOX_TOKEN_STREAMING_PAD {
                    consecutive_pad += 1;
                } else {
                    consecutive_pad = 0;
                    if token >= self.model.tokenizer_num_special_tokens {
                        seen_text = true;
                    }
                }
                if seen_text && consecutive_pad >= VOX_N_RIGHT_PAD_TOKENS as i32 {
                    break;
                }
            }
            if out.last().copied() == Some(VOX_TOKEN_EOS) {
                out.pop();
            }
            Ok(out)
        }
    }
}

impl GgmlModel {
    fn load(path: &Path, backend: VoxtralSttBackend) -> Result<Self> {
        let c_path = cstring_path(path)?;
        let mut meta_ctx: *mut ffi::ggml_context = ptr::null_mut();
        let params = ffi::gguf_init_params {
            no_alloc: true,
            ctx: &mut meta_ctx,
        };
        let gguf = unsafe { ffi::gguf_init_from_file(c_path.as_ptr(), params) };
        if gguf.is_null() || meta_ctx.is_null() {
            return Err(Error::Runtime(format!(
                "gguf_init_from_file failed for {}",
                path.display()
            )));
        }

        let (weights_backend, weights_on_accel) = init_weight_backend(backend);
        if weights_backend.is_null() {
            unsafe {
                ffi::gguf_free(gguf);
                ffi::ggml_free(meta_ctx);
            }
            return Err(Error::Runtime("failed to initialize ggml backend".into()));
        }

        let weights_buffer =
            unsafe { ffi::ggml_backend_alloc_ctx_tensors(meta_ctx, weights_backend) };
        if weights_buffer.is_null() {
            unsafe {
                ffi::ggml_backend_free(weights_backend);
                ffi::gguf_free(gguf);
                ffi::ggml_free(meta_ctx);
            }
            return Err(Error::Runtime(
                "ggml_backend_alloc_ctx_tensors failed for Voxtral weights".into(),
            ));
        }

        let tensors = load_weight_tensors(path, gguf, meta_ctx)?;
        let mut missing = Vec::new();
        let mut get = |name: &str| -> Tensor {
            match tensors.get(name).copied() {
                Some(t) if !t.is_null() => t,
                _ => {
                    missing.push(name.to_string());
                    ptr::null_mut()
                }
            }
        };

        let enc_conv0_weight = get("enc.conv0.weight");
        let enc_conv0_bias = get("enc.conv0.bias");
        let enc_conv1_weight = get("enc.conv1.weight");
        let enc_conv1_bias = get("enc.conv1.bias");
        let enc_norm_weight = get("enc.norm.weight");

        let mut enc_layers = Vec::with_capacity(VOX_ENC_LAYERS);
        for i in 0..VOX_ENC_LAYERS {
            enc_layers.push(EncoderLayer {
                attn_norm_weight: get(&format!("enc.blk.{i}.attn_norm.weight")),
                attn_q_weight: get(&format!("enc.blk.{i}.attn_q.weight")),
                attn_q_bias: get(&format!("enc.blk.{i}.attn_q.bias")),
                attn_k_weight: get(&format!("enc.blk.{i}.attn_k.weight")),
                attn_v_weight: get(&format!("enc.blk.{i}.attn_v.weight")),
                attn_v_bias: get(&format!("enc.blk.{i}.attn_v.bias")),
                attn_o_weight: get(&format!("enc.blk.{i}.attn_o.weight")),
                attn_o_bias: get(&format!("enc.blk.{i}.attn_o.bias")),
                ffn_norm_weight: get(&format!("enc.blk.{i}.ffn_norm.weight")),
                ffn_w1_weight: get(&format!("enc.blk.{i}.ffn_w1.weight")),
                ffn_w2_weight: get(&format!("enc.blk.{i}.ffn_w2.weight")),
                ffn_w2_bias: get(&format!("enc.blk.{i}.ffn_w2.bias")),
                ffn_w3_weight: get(&format!("enc.blk.{i}.ffn_w3.weight")),
            });
        }

        let adapter_0_weight = get("adapter.0.weight");
        let adapter_2_weight = get("adapter.2.weight");
        let tok_embeddings_weight = get("tok_embeddings.weight");
        let dec_norm_weight = get("norm.weight");
        let mel_filters = tensors.get("audio.mel_filters").copied();

        let mut dec_layers = Vec::with_capacity(VOX_DEC_LAYERS);
        for i in 0..VOX_DEC_LAYERS {
            dec_layers.push(DecoderLayer {
                attn_norm_weight: get(&format!("dec.blk.{i}.attn_norm.weight")),
                attn_q_weight: get(&format!("dec.blk.{i}.attn_q.weight")),
                attn_k_weight: get(&format!("dec.blk.{i}.attn_k.weight")),
                attn_v_weight: get(&format!("dec.blk.{i}.attn_v.weight")),
                attn_o_weight: get(&format!("dec.blk.{i}.attn_o.weight")),
                ffn_norm_weight: get(&format!("dec.blk.{i}.ffn_norm.weight")),
                ffn_w1_weight: get(&format!("dec.blk.{i}.ffn_w1.weight")),
                ffn_w2_weight: get(&format!("dec.blk.{i}.ffn_w2.weight")),
                ffn_w3_weight: get(&format!("dec.blk.{i}.ffn_w3.weight")),
                ada0_weight: get(&format!("dec.blk.{i}.ada0.weight")),
                ada2_weight: get(&format!("dec.blk.{i}.ada2.weight")),
            });
        }

        if !missing.is_empty() {
            unsafe {
                ffi::ggml_backend_buffer_free(weights_buffer);
                ffi::ggml_backend_free(weights_backend);
                ffi::gguf_free(gguf);
                ffi::ggml_free(meta_ctx);
            }
            return Err(Error::Parse(format!(
                "missing Voxtral GGUF tensors: {}",
                missing.join(", ")
            )));
        }

        let (tokenizer_num_special_tokens, tokenizer_special_ranks, tokenizer_vocab_b64) =
            unsafe { load_tokenizer_metadata(gguf)? };

        Ok(Self {
            ctx: meta_ctx,
            gguf,
            weights_backend,
            weights_buffer,
            weights_on_accel,
            enc_conv0_weight,
            enc_conv0_bias,
            enc_conv1_weight,
            enc_conv1_bias,
            enc_norm_weight,
            enc_layers,
            adapter_0_weight,
            adapter_2_weight,
            tok_embeddings_weight,
            dec_norm_weight,
            dec_layers,
            mel_filters,
            tokenizer_num_special_tokens,
            tokenizer_special_ranks,
            tokenizer_vocab_b64,
            tokenizer_bytes_cache: HashMap::new(),
        })
    }

    fn decode_tokens(&mut self, tokens: &[i32]) -> String {
        let mut out = String::new();
        for &token in tokens {
            if token < self.tokenizer_num_special_tokens
                || self.tokenizer_special_ranks.contains(&token)
            {
                continue;
            }
            let bytes = self.token_bytes_for_id(token);
            out.push_str(&bytes);
        }
        out
    }

    fn token_bytes_for_id(&mut self, token_id: i32) -> String {
        if let Some(value) = self.tokenizer_bytes_cache.get(&token_id) {
            return value.clone();
        }
        let mut decoded = String::new();
        let vocab_id = token_id - self.tokenizer_num_special_tokens;
        if vocab_id >= 0 {
            if let Some(raw) = self.tokenizer_vocab_b64.get(vocab_id as usize) {
                let bytes = base64_decode(raw);
                decoded = String::from_utf8_lossy(&bytes).into_owned();
            }
        }
        self.tokenizer_bytes_cache.insert(token_id, decoded.clone());
        decoded
    }
}

impl Drop for GgmlModel {
    fn drop(&mut self) {
        unsafe {
            if !self.weights_buffer.is_null() {
                ffi::ggml_backend_buffer_free(self.weights_buffer);
            }
            if !self.weights_backend.is_null() {
                ffi::ggml_backend_free(self.weights_backend);
            }
            if !self.gguf.is_null() {
                ffi::gguf_free(self.gguf);
            }
            if !self.ctx.is_null() {
                ffi::ggml_free(self.ctx);
            }
        }
    }
}

impl GgmlSession {
    fn new(model: &GgmlModel, requested: VoxtralSttBackend) -> Result<Self> {
        let threads = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(4)
            .min(i32::MAX as usize) as i32;
        let (backend, backend_cpu, has_accel) =
            init_compute_backend(requested, model.weights_on_accel, threads);
        if backend.is_null() {
            return Err(Error::Runtime(
                "failed to initialize ggml compute backend".into(),
            ));
        }
        let blas_backend = init_blas_backend(threads);

        let mut ctx = Self {
            backend,
            backend_cpu,
            blas_backend,
            has_accel,
            ctx_persistent: ptr::null_mut(),
            buf_persistent: ptr::null_mut(),
            encoder_chunk_output: ptr::null_mut(),
            decoder_logits: ptr::null_mut(),
            kv_self_k: ptr::null_mut(),
            kv_self_v: ptr::null_mut(),
            ctx_enc_full: ptr::null_mut(),
            buf_enc_full: ptr::null_mut(),
            encoder_output: ptr::null_mut(),
            ctx_dec_mem: ptr::null_mut(),
            buf_dec_mem: ptr::null_mut(),
            decoder_memory: ptr::null_mut(),
            total_enc_tokens: 0,
            enc_seq_used: 0,
            dec_seq_len: 0,
            kv_used: 0,
            sched_encoder: ptr::null_mut(),
            sched_adapter: ptr::null_mut(),
            sched_dec_pre: ptr::null_mut(),
            sched_dec_step: ptr::null_mut(),
            mel_filters_cpu: Vec::new(),
            mel_plan: audio::MelSpectrogramPlan::default(),
            time_emb_cpu: compute_time_embedding(VOX_N_DELAY_TOKENS as f32, VOX_DEC_DIM),
        };
        unsafe {
            ctx.allocate_persistent()?;
            ctx.init_schedulers()?;
            ctx.mel_filters_cpu = if let Some(mel) = model.mel_filters {
                let mut filters = vec![0.0f32; N_FREQ * VOX_NUM_MEL_BINS];
                ffi::ggml_backend_tensor_get(
                    mel,
                    filters.as_mut_ptr() as *mut c_void,
                    0,
                    filters.len() * std::mem::size_of::<f32>(),
                );
                filters
            } else {
                audio::mel_filter_bank()
            };
            ctx.mel_plan = audio::MelSpectrogramPlan::new(ctx.mel_filters_cpu.clone());
        }
        Ok(ctx)
    }

    unsafe fn allocate_persistent(&mut self) -> Result<()> {
        unsafe {
            let params = ffi::ggml_init_params {
                mem_size: ffi::ggml_tensor_overhead() * 4,
                mem_buffer: ptr::null_mut(),
                no_alloc: true,
            };
            self.ctx_persistent = ffi::ggml_init(params);
            if self.ctx_persistent.is_null() {
                return Err(Error::Runtime(
                    "ggml_init failed for Voxtral persistent context".into(),
                ));
            }
            self.encoder_chunk_output = ffi::ggml_new_tensor_2d(
                self.ctx_persistent,
                ffi::ggml_type::GGML_TYPE_F32,
                VOX_ENC_DIM as i64,
                MAX_ENC_CHUNK,
            );
            set_name(self.encoder_chunk_output, "encoder_chunk_output");
            self.decoder_logits = ffi::ggml_new_tensor_1d(
                self.ctx_persistent,
                ffi::ggml_type::GGML_TYPE_F32,
                VOX_VOCAB_SIZE as i64,
            );
            set_name(self.decoder_logits, "decoder_logits");

            let kv_dim = (VOX_DEC_KV_HEADS * VOX_DEC_HEAD_DIM) as i64;
            self.kv_self_k = ffi::ggml_new_tensor_3d(
                self.ctx_persistent,
                ffi::ggml_type::GGML_TYPE_F32,
                kv_dim,
                VOX_DEC_WINDOW as i64,
                VOX_DEC_LAYERS as i64,
            );
            set_name(self.kv_self_k, "kv_self_k");
            self.kv_self_v = ffi::ggml_new_tensor_3d(
                self.ctx_persistent,
                ffi::ggml_type::GGML_TYPE_F32,
                kv_dim,
                VOX_DEC_WINDOW as i64,
                VOX_DEC_LAYERS as i64,
            );
            set_name(self.kv_self_v, "kv_self_v");

            self.buf_persistent =
                ffi::ggml_backend_alloc_ctx_tensors(self.ctx_persistent, self.backend);
            if self.buf_persistent.is_null() {
                return Err(Error::Runtime(
                    "failed to allocate Voxtral persistent ggml buffer".into(),
                ));
            }
            ffi::ggml_backend_buffer_clear(self.buf_persistent, 0);
            Ok(())
        }
    }

    unsafe fn init_schedulers(&mut self) -> Result<()> {
        unsafe {
            let mut backends = Vec::with_capacity(3);
            if self.has_accel {
                backends.push(self.backend);
            }
            if !self.blas_backend.is_null() {
                backends.push(self.blas_backend);
            }
            let cpu = if self.has_accel {
                self.backend_cpu
            } else {
                self.backend
            };
            backends.push(cpu);
            let op_offload = self.has_accel;

            self.sched_encoder = ffi::ggml_backend_sched_new(
                backends.as_mut_ptr(),
                ptr::null_mut(),
                backends.len() as i32,
                GGML_DEFAULT_GRAPH_SIZE,
                false,
                op_offload,
            );
            self.sched_adapter = ffi::ggml_backend_sched_new(
                backends.as_mut_ptr(),
                ptr::null_mut(),
                backends.len() as i32,
                GGML_DEFAULT_GRAPH_SIZE,
                false,
                op_offload,
            );
            self.sched_dec_pre = ffi::ggml_backend_sched_new(
                backends.as_mut_ptr(),
                ptr::null_mut(),
                backends.len() as i32,
                GGML_DEFAULT_GRAPH_SIZE,
                false,
                op_offload,
            );
            self.sched_dec_step = ffi::ggml_backend_sched_new(
                backends.as_mut_ptr(),
                ptr::null_mut(),
                backends.len() as i32,
                GGML_DEFAULT_GRAPH_SIZE,
                false,
                op_offload,
            );
            if self.sched_encoder.is_null()
                || self.sched_adapter.is_null()
                || self.sched_dec_pre.is_null()
                || self.sched_dec_step.is_null()
            {
                return Err(Error::Runtime(
                    "failed to initialize ggml backend schedulers".into(),
                ));
            }
            Ok(())
        }
    }

    unsafe fn alloc_encoder_output(&mut self, n_tokens: i32) -> Result<()> {
        unsafe {
            if !self.buf_enc_full.is_null() {
                ffi::ggml_backend_buffer_free(self.buf_enc_full);
                self.buf_enc_full = ptr::null_mut();
            }
            if !self.ctx_enc_full.is_null() {
                ffi::ggml_free(self.ctx_enc_full);
                self.ctx_enc_full = ptr::null_mut();
            }
            self.encoder_output = ptr::null_mut();

            let params = ffi::ggml_init_params {
                mem_size: ffi::ggml_tensor_overhead(),
                mem_buffer: ptr::null_mut(),
                no_alloc: true,
            };
            self.ctx_enc_full = ffi::ggml_init(params);
            self.encoder_output = ffi::ggml_new_tensor_2d(
                self.ctx_enc_full,
                ffi::ggml_type::GGML_TYPE_F32,
                VOX_ENC_DIM as i64,
                n_tokens as i64,
            );
            set_name(self.encoder_output, "encoder_output");
            self.buf_enc_full =
                ffi::ggml_backend_alloc_ctx_tensors(self.ctx_enc_full, self.backend);
            if self.buf_enc_full.is_null() {
                return Err(Error::Runtime(
                    "failed to allocate encoder output buffer".into(),
                ));
            }
            self.total_enc_tokens = n_tokens;
            Ok(())
        }
    }

    unsafe fn alloc_decoder_memory(&mut self, dec_seq: i32) -> Result<()> {
        unsafe {
            if !self.buf_dec_mem.is_null() {
                ffi::ggml_backend_buffer_free(self.buf_dec_mem);
                self.buf_dec_mem = ptr::null_mut();
            }
            if !self.ctx_dec_mem.is_null() {
                ffi::ggml_free(self.ctx_dec_mem);
                self.ctx_dec_mem = ptr::null_mut();
            }
            self.decoder_memory = ptr::null_mut();

            let params = ffi::ggml_init_params {
                mem_size: ffi::ggml_tensor_overhead(),
                mem_buffer: ptr::null_mut(),
                no_alloc: true,
            };
            self.ctx_dec_mem = ffi::ggml_init(params);
            self.decoder_memory = ffi::ggml_new_tensor_2d(
                self.ctx_dec_mem,
                ffi::ggml_type::GGML_TYPE_F32,
                VOX_DEC_DIM as i64,
                dec_seq as i64,
            );
            set_name(self.decoder_memory, "decoder_memory");
            self.buf_dec_mem = ffi::ggml_backend_alloc_ctx_tensors(self.ctx_dec_mem, self.backend);
            if self.buf_dec_mem.is_null() {
                return Err(Error::Runtime(
                    "failed to allocate decoder memory buffer".into(),
                ));
            }
            self.dec_seq_len = dec_seq;
            Ok(())
        }
    }

    unsafe fn clear_kv_cache(&mut self) {
        unsafe {
            if !self.buf_persistent.is_null() {
                ffi::ggml_backend_buffer_clear(self.buf_persistent, 0);
            }
            self.kv_used = 0;
        }
    }

    unsafe fn run_encoder_chunked(
        &mut self,
        model: &GgmlModel,
        mel_data: *const f32,
        total_mel_frames: i32,
    ) -> Result<()> {
        unsafe {
            let mel_overlap = ENC_CHUNK_OVERLAP * 2;
            let mel_stride = ENC_CHUNK_MEL - mel_overlap;
            let alloc_total = compute_total_enc_tokens(total_mel_frames);
            if alloc_total <= 0 {
                return Err(Error::Runtime(
                    "encoder produced no tokens for the supplied audio".into(),
                ));
            }
            self.alloc_encoder_output(alloc_total)?;

            let mut mel_offset = 0;
            let mut enc_write_offset = 0;
            let mut chunk_idx = 0;
            while mel_offset < total_mel_frames {
                let chunk_mel_frames = ENC_CHUNK_MEL.min(total_mel_frames - mel_offset);
                let skip = if chunk_idx > 0 { ENC_CHUNK_OVERLAP } else { 0 };
                let expected = mel_frames_to_enc_tokens(chunk_mel_frames);
                if expected - skip <= 0 {
                    break;
                }

                let mut chunk_mel_buf = Vec::new();
                let chunk_ptr = if mel_offset == 0 && chunk_mel_frames == total_mel_frames {
                    mel_data
                } else {
                    chunk_mel_buf.resize(VOX_NUM_MEL_BINS * chunk_mel_frames as usize, 0.0);
                    for m in 0..VOX_NUM_MEL_BINS {
                        let src = mel_data.add(m * total_mel_frames as usize + mel_offset as usize);
                        let dst = chunk_mel_buf
                            .as_mut_ptr()
                            .add(m * chunk_mel_frames as usize);
                        ptr::copy_nonoverlapping(src, dst, chunk_mel_frames as usize);
                    }
                    chunk_mel_buf.as_ptr()
                };

                let rope_offset = enc_write_offset - skip;
                let chunk_seq_len =
                    self.run_encoder_chunk(model, chunk_ptr, chunk_mel_frames, rope_offset)?;
                let mut stride = chunk_seq_len - skip;
                if stride <= 0 {
                    break;
                }
                if enc_write_offset + stride > alloc_total {
                    stride = alloc_total - enc_write_offset;
                    if stride <= 0 {
                        break;
                    }
                }

                let elem_bytes = VOX_ENC_DIM * std::mem::size_of::<f32>();
                let src_offset = skip as usize * elem_bytes;
                let dst_offset = enc_write_offset as usize * elem_bytes;
                let copy_bytes = stride as usize * elem_bytes;
                let mut tmp = vec![0u8; copy_bytes];
                ffi::ggml_backend_tensor_get(
                    self.encoder_chunk_output,
                    tmp.as_mut_ptr() as *mut c_void,
                    src_offset,
                    copy_bytes,
                );
                ffi::ggml_backend_tensor_set(
                    self.encoder_output,
                    tmp.as_ptr() as *const c_void,
                    dst_offset,
                    copy_bytes,
                );

                enc_write_offset += stride;
                mel_offset += mel_stride;
                chunk_idx += 1;
            }

            self.enc_seq_used =
                (enc_write_offset / VOX_DOWNSAMPLE_FACTOR as i32) * VOX_DOWNSAMPLE_FACTOR as i32;
            self.total_enc_tokens = self.enc_seq_used;
            Ok(())
        }
    }

    unsafe fn run_encoder_chunk(
        &mut self,
        model: &GgmlModel,
        chunk_mel_data: *const f32,
        chunk_mel_frames: i32,
        rope_pos_offset: i32,
    ) -> Result<i32> {
        unsafe {
            let meta = MetaContext::new(GGML_DEFAULT_GRAPH_SIZE * 4)?;
            let mut chunk_seq_len = 0;
            let gf =
                self.build_encoder_graph(model, meta.ctx, chunk_mel_frames, &mut chunk_seq_len)?;

            ffi::ggml_backend_sched_reset(self.sched_encoder);
            if !ffi::ggml_backend_sched_alloc_graph(self.sched_encoder, gf) {
                return Err(Error::Runtime(
                    "encoder chunk graph allocation failed".into(),
                ));
            }

            let mel_t = graph_tensor(gf, "mel_input");
            if !mel_t.is_null() {
                ffi::ggml_backend_tensor_set(
                    mel_t,
                    chunk_mel_data as *const c_void,
                    0,
                    VOX_NUM_MEL_BINS * chunk_mel_frames as usize * std::mem::size_of::<f32>(),
                );
            }

            let pos_t = graph_tensor(gf, "enc_positions");
            if !pos_t.is_null() {
                let pos = (0..chunk_seq_len)
                    .map(|i| i + rope_pos_offset)
                    .collect::<Vec<i32>>();
                ffi::ggml_backend_tensor_set(
                    pos_t,
                    pos.as_ptr() as *const c_void,
                    0,
                    pos.len() * std::mem::size_of::<i32>(),
                );
            }

            let mask_t = graph_tensor(gf, "enc_attn_mask");
            if !mask_t.is_null() {
                let mut mask = vec![0.0f32; chunk_seq_len as usize * chunk_seq_len as usize];
                for q in 0..chunk_seq_len {
                    let min_kv = 0.max(q - (VOX_ENC_WINDOW as i32 - 1));
                    for kv in 0..chunk_seq_len {
                        let allow = kv <= q && kv >= min_kv;
                        mask[q as usize * chunk_seq_len as usize + kv as usize] =
                            if allow { 0.0 } else { f32::NEG_INFINITY };
                    }
                }
                ffi::ggml_backend_tensor_set(
                    mask_t,
                    mask.as_ptr() as *const c_void,
                    0,
                    mask.len() * std::mem::size_of::<f32>(),
                );
            }

            check_status(
                ffi::ggml_backend_sched_graph_compute(self.sched_encoder, gf),
                "encoder chunk graph compute",
            )?;
            ffi::ggml_backend_sched_reset(self.sched_encoder);
            Ok(chunk_seq_len)
        }
    }

    unsafe fn run_adapter(&mut self, model: &GgmlModel) -> Result<()> {
        unsafe {
            let enc_seq = self.enc_seq_used;
            let dec_seq = enc_seq / VOX_DOWNSAMPLE_FACTOR as i32;
            self.alloc_decoder_memory(dec_seq)?;

            let meta = MetaContext::new(GGML_DEFAULT_GRAPH_SIZE)?;
            let gf = self.build_adapter_graph(model, meta.ctx);
            ffi::ggml_backend_sched_reset(self.sched_adapter);
            if !ffi::ggml_backend_sched_alloc_graph(self.sched_adapter, gf) {
                return Err(Error::Runtime("adapter graph allocation failed".into()));
            }
            check_status(
                ffi::ggml_backend_sched_graph_compute(self.sched_adapter, gf),
                "adapter graph compute",
            )?;
            ffi::ggml_backend_sched_reset(self.sched_adapter);
            Ok(())
        }
    }

    unsafe fn run_decoder_prefill(
        &mut self,
        model: &GgmlModel,
        token_ids: &[i32],
        logits_out: &mut [f32],
    ) -> Result<()> {
        unsafe {
            let n_tokens = token_ids.len() as i32;
            if n_tokens > VOX_DEC_WINDOW as i32 {
                return Err(Error::Runtime(
                    "decoder prefill exceeds context window".into(),
                ));
            }
            let meta = MetaContext::new(GGML_DEFAULT_GRAPH_SIZE * 4)?;
            let gf = self.build_decoder_prefill_graph(model, meta.ctx, n_tokens);
            ffi::ggml_backend_sched_reset(self.sched_dec_pre);
            if !ffi::ggml_backend_sched_alloc_graph(self.sched_dec_pre, gf) {
                return Err(Error::Runtime(
                    "decoder prefill graph allocation failed".into(),
                ));
            }

            let tok_t = graph_tensor(gf, "token_ids");
            if !tok_t.is_null() {
                ffi::ggml_backend_tensor_set(
                    tok_t,
                    token_ids.as_ptr() as *const c_void,
                    0,
                    token_ids.len() * std::mem::size_of::<i32>(),
                );
            }
            let pos_t = graph_tensor(gf, "positions");
            if !pos_t.is_null() {
                let pos = (0..n_tokens).collect::<Vec<i32>>();
                ffi::ggml_backend_tensor_set(
                    pos_t,
                    pos.as_ptr() as *const c_void,
                    0,
                    pos.len() * std::mem::size_of::<i32>(),
                );
            }
            let time_t = graph_tensor(gf, "time_emb");
            if !time_t.is_null() {
                ffi::ggml_backend_tensor_set(
                    time_t,
                    self.time_emb_cpu.as_ptr() as *const c_void,
                    0,
                    self.time_emb_cpu.len() * std::mem::size_of::<f32>(),
                );
            }
            let mask_t = graph_tensor(gf, "causal_mask");
            if !mask_t.is_null() {
                let mut mask = vec![0.0f32; n_tokens as usize * n_tokens as usize];
                for i in 0..n_tokens {
                    for j in 0..n_tokens {
                        mask[i as usize * n_tokens as usize + j as usize] =
                            if j <= i { 0.0 } else { f32::NEG_INFINITY };
                    }
                }
                ffi::ggml_backend_tensor_set(
                    mask_t,
                    mask.as_ptr() as *const c_void,
                    0,
                    mask.len() * std::mem::size_of::<f32>(),
                );
            }

            check_status(
                ffi::ggml_backend_sched_graph_compute(self.sched_dec_pre, gf),
                "decoder prefill graph compute",
            )?;
            ffi::ggml_backend_tensor_get(
                self.decoder_logits,
                logits_out.as_mut_ptr() as *mut c_void,
                0,
                VOX_VOCAB_SIZE * std::mem::size_of::<f32>(),
            );
            self.kv_used = n_tokens.min(VOX_DEC_WINDOW as i32);
            ffi::ggml_backend_sched_reset(self.sched_dec_pre);
            Ok(())
        }
    }

    unsafe fn run_decoder_step(
        &mut self,
        model: &GgmlModel,
        token_id: i32,
        position: i32,
        audio_pos: i32,
        logits_out: &mut [f32],
    ) -> Result<()> {
        unsafe {
            if self.kv_used >= VOX_DEC_WINDOW as i32 {
                return Err(Error::Unsupported(
                    "Voxtral KV cache shift is not implemented for >8192 decode positions",
                ));
            }

            let meta = MetaContext::new(GGML_DEFAULT_GRAPH_SIZE * 4)?;
            let gf = self.build_decoder_step_graph(model, meta.ctx, position, audio_pos);
            ffi::ggml_backend_sched_reset(self.sched_dec_step);
            if !ffi::ggml_backend_sched_alloc_graph(self.sched_dec_step, gf) {
                return Err(Error::Runtime(
                    "decoder step graph allocation failed".into(),
                ));
            }

            let tok_t = graph_tensor(gf, "token_id");
            if !tok_t.is_null() {
                ffi::ggml_backend_tensor_set(
                    tok_t,
                    &token_id as *const i32 as *const c_void,
                    0,
                    std::mem::size_of::<i32>(),
                );
            }
            let pos_t = graph_tensor(gf, "position");
            if !pos_t.is_null() {
                ffi::ggml_backend_tensor_set(
                    pos_t,
                    &position as *const i32 as *const c_void,
                    0,
                    std::mem::size_of::<i32>(),
                );
            }
            let time_t = graph_tensor(gf, "time_emb");
            if !time_t.is_null() {
                ffi::ggml_backend_tensor_set(
                    time_t,
                    self.time_emb_cpu.as_ptr() as *const c_void,
                    0,
                    self.time_emb_cpu.len() * std::mem::size_of::<f32>(),
                );
            }

            check_status(
                ffi::ggml_backend_sched_graph_compute(self.sched_dec_step, gf),
                "decoder step graph compute",
            )?;
            ffi::ggml_backend_tensor_get(
                self.decoder_logits,
                logits_out.as_mut_ptr() as *mut c_void,
                0,
                VOX_VOCAB_SIZE * std::mem::size_of::<f32>(),
            );
            self.kv_used += 1;
            ffi::ggml_backend_sched_reset(self.sched_dec_step);
            Ok(())
        }
    }

    unsafe fn build_encoder_graph(
        &mut self,
        model: &GgmlModel,
        gctx: *mut ffi::ggml_context,
        n_frames: i32,
        out_seq_len: &mut i32,
    ) -> Result<*mut ffi::ggml_cgraph> {
        unsafe {
            let gf = ffi::ggml_new_graph_custom(gctx, GGML_DEFAULT_GRAPH_SIZE * 4, false);
            let mel_input = ffi::ggml_new_tensor_3d(
                gctx,
                ffi::ggml_type::GGML_TYPE_F32,
                n_frames as i64,
                VOX_NUM_MEL_BINS as i64,
                1,
            );
            set_name(mel_input, "mel_input");
            ffi::ggml_backend_sched_set_tensor_backend(self.sched_encoder, mel_input, self.backend);

            let (mut x, conv0_len) = causal_conv1d_graph(
                gctx,
                mel_input,
                n_frames,
                model.enc_conv0_weight,
                model.enc_conv0_bias,
                VOX_ENC_DIM as i32,
                3,
                1,
            )?;
            x = ffi::ggml_gelu_erf(gctx, x);
            let (mut conv1_out, conv_out_len) = causal_conv1d_graph(
                gctx,
                x,
                conv0_len,
                model.enc_conv1_weight,
                model.enc_conv1_bias,
                VOX_ENC_DIM as i32,
                3,
                2,
            )?;
            conv1_out = ffi::ggml_gelu_erf(gctx, conv1_out);

            let trunc = conv_out_len % VOX_DOWNSAMPLE_FACTOR as i32;
            let mut seq_len = conv_out_len;
            let x_len_first = if trunc > 0 {
                seq_len = conv_out_len - trunc;
                ffi::ggml_view_3d(
                    gctx,
                    conv1_out,
                    (conv_out_len - trunc) as i64,
                    VOX_ENC_DIM as i64,
                    1,
                    (*conv1_out).nb[1],
                    (*conv1_out).nb[2],
                    trunc as usize * (*conv1_out).nb[0],
                )
            } else {
                conv1_out
            };
            let mut x = ffi::ggml_permute(gctx, x_len_first, 1, 0, 2, 3);
            x = ffi::ggml_cont(gctx, x);
            x = ffi::ggml_reshape_2d(gctx, x, VOX_ENC_DIM as i64, seq_len as i64);

            let enc_positions =
                ffi::ggml_new_tensor_1d(gctx, ffi::ggml_type::GGML_TYPE_I32, seq_len as i64);
            set_name(enc_positions, "enc_positions");
            ffi::ggml_backend_sched_set_tensor_backend(
                self.sched_encoder,
                enc_positions,
                self.backend,
            );

            let enc_attn_mask = ffi::ggml_new_tensor_2d(
                gctx,
                ffi::ggml_type::GGML_TYPE_F32,
                seq_len as i64,
                seq_len as i64,
            );
            set_name(enc_attn_mask, "enc_attn_mask");
            ffi::ggml_backend_sched_set_tensor_backend(
                self.sched_encoder,
                enc_attn_mask,
                self.backend,
            );
            let enc_attn_mask_f16 =
                ffi::ggml_cast(gctx, enc_attn_mask, ffi::ggml_type::GGML_TYPE_F16);

            for layer in &model.enc_layers {
                let residual = x;
                let mut x_norm = ffi::ggml_rms_norm(gctx, x, VOX_ENC_NORM_EPS);
                x_norm = ffi::ggml_mul(gctx, x_norm, layer.attn_norm_weight);
                let mut q = ffi::ggml_mul_mat(gctx, layer.attn_q_weight, x_norm);
                q = ffi::ggml_add(gctx, q, layer.attn_q_bias);
                let mut k = ffi::ggml_mul_mat(gctx, layer.attn_k_weight, x_norm);
                let mut v = ffi::ggml_mul_mat(gctx, layer.attn_v_weight, x_norm);
                v = ffi::ggml_add(gctx, v, layer.attn_v_bias);
                q = ffi::ggml_reshape_3d(
                    gctx,
                    q,
                    VOX_ENC_HEAD_DIM as i64,
                    VOX_ENC_HEADS as i64,
                    seq_len as i64,
                );
                k = ffi::ggml_reshape_3d(
                    gctx,
                    k,
                    VOX_ENC_HEAD_DIM as i64,
                    VOX_ENC_KV_HEADS as i64,
                    seq_len as i64,
                );
                q = rope(
                    gctx,
                    q,
                    enc_positions,
                    VOX_ENC_HEAD_DIM as i32,
                    VOX_ENC_ROPE_THETA,
                );
                k = rope(
                    gctx,
                    k,
                    enc_positions,
                    VOX_ENC_HEAD_DIM as i32,
                    VOX_ENC_ROPE_THETA,
                );
                q = ffi::ggml_permute(gctx, q, 0, 2, 1, 3);
                k = ffi::ggml_permute(gctx, k, 0, 2, 1, 3);
                v = ffi::ggml_reshape_3d(
                    gctx,
                    v,
                    VOX_ENC_HEAD_DIM as i64,
                    VOX_ENC_KV_HEADS as i64,
                    seq_len as i64,
                );
                v = ffi::ggml_permute(gctx, v, 0, 2, 1, 3);
                let scale = 1.0f32 / (VOX_ENC_HEAD_DIM as f32).sqrt();
                let mut attn_out =
                    ffi::ggml_flash_attn_ext(gctx, q, k, v, enc_attn_mask_f16, scale, 0.0, 0.0);
                attn_out = ffi::ggml_cont(gctx, attn_out);
                attn_out = ffi::ggml_reshape_2d(
                    gctx,
                    attn_out,
                    (VOX_ENC_HEADS * VOX_ENC_HEAD_DIM) as i64,
                    seq_len as i64,
                );
                let mut attn_proj = ffi::ggml_mul_mat(gctx, layer.attn_o_weight, attn_out);
                attn_proj = ffi::ggml_add(gctx, attn_proj, layer.attn_o_bias);
                x = ffi::ggml_add(gctx, residual, attn_proj);

                let residual = x;
                let mut x_norm = ffi::ggml_rms_norm(gctx, x, VOX_ENC_NORM_EPS);
                x_norm = ffi::ggml_mul(gctx, x_norm, layer.ffn_norm_weight);
                let mut gate = ffi::ggml_mul_mat(gctx, layer.ffn_w1_weight, x_norm);
                gate = ffi::ggml_silu(gctx, gate);
                let up = ffi::ggml_mul_mat(gctx, layer.ffn_w3_weight, x_norm);
                let mut ffn_out = ffi::ggml_mul(gctx, gate, up);
                ffn_out = ffi::ggml_mul_mat(gctx, layer.ffn_w2_weight, ffn_out);
                ffn_out = ffi::ggml_add(gctx, ffn_out, layer.ffn_w2_bias);
                x = ffi::ggml_add(gctx, residual, ffn_out);
            }

            x = ffi::ggml_rms_norm(gctx, x, VOX_ENC_NORM_EPS);
            x = ffi::ggml_mul(gctx, x, model.enc_norm_weight);
            let enc_out_view = ffi::ggml_view_2d(
                gctx,
                self.encoder_chunk_output,
                VOX_ENC_DIM as i64,
                seq_len as i64,
                (*self.encoder_chunk_output).nb[1],
                0,
            );
            ffi::ggml_build_forward_expand(gf, ffi::ggml_cpy(gctx, x, enc_out_view));
            *out_seq_len = seq_len;
            Ok(gf)
        }
    }

    unsafe fn build_adapter_graph(
        &mut self,
        model: &GgmlModel,
        gctx: *mut ffi::ggml_context,
    ) -> *mut ffi::ggml_cgraph {
        unsafe {
            let enc_seq = self.enc_seq_used;
            let dec_seq = enc_seq / VOX_DOWNSAMPLE_FACTOR as i32;
            let gf = ffi::ggml_new_graph(gctx);
            let enc_out = ffi::ggml_view_2d(
                gctx,
                self.encoder_output,
                VOX_ENC_DIM as i64,
                enc_seq as i64,
                (*self.encoder_output).nb[1],
                0,
            );
            let mut x = ffi::ggml_reshape_2d(
                gctx,
                enc_out,
                (VOX_ENC_DIM * VOX_DOWNSAMPLE_FACTOR) as i64,
                dec_seq as i64,
            );
            x = ffi::ggml_mul_mat(gctx, model.adapter_0_weight, x);
            x = ffi::ggml_gelu_erf(gctx, x);
            x = ffi::ggml_mul_mat(gctx, model.adapter_2_weight, x);
            let dec_mem_view = ffi::ggml_view_2d(
                gctx,
                self.decoder_memory,
                VOX_DEC_DIM as i64,
                dec_seq as i64,
                (*self.decoder_memory).nb[1],
                0,
            );
            ffi::ggml_build_forward_expand(gf, ffi::ggml_cpy(gctx, x, dec_mem_view));
            self.dec_seq_len = dec_seq;
            gf
        }
    }

    unsafe fn build_decoder_prefill_graph(
        &mut self,
        model: &GgmlModel,
        gctx: *mut ffi::ggml_context,
        n_tokens: i32,
    ) -> *mut ffi::ggml_cgraph {
        unsafe {
            let gf = ffi::ggml_new_graph_custom(gctx, GGML_DEFAULT_GRAPH_SIZE * 4, false);
            let token_ids =
                ffi::ggml_new_tensor_1d(gctx, ffi::ggml_type::GGML_TYPE_I32, n_tokens as i64);
            set_name(token_ids, "token_ids");
            ffi::ggml_backend_sched_set_tensor_backend(self.sched_dec_pre, token_ids, self.backend);
            let positions =
                ffi::ggml_new_tensor_1d(gctx, ffi::ggml_type::GGML_TYPE_I32, n_tokens as i64);
            set_name(positions, "positions");
            ffi::ggml_backend_sched_set_tensor_backend(self.sched_dec_pre, positions, self.backend);
            let time_emb =
                ffi::ggml_new_tensor_1d(gctx, ffi::ggml_type::GGML_TYPE_F32, VOX_DEC_DIM as i64);
            set_name(time_emb, "time_emb");
            ffi::ggml_backend_sched_set_tensor_backend(self.sched_dec_pre, time_emb, self.backend);

            let tok_emb = ffi::ggml_get_rows(gctx, model.tok_embeddings_weight, token_ids);
            let audio_emb = ffi::ggml_view_2d(
                gctx,
                self.decoder_memory,
                VOX_DEC_DIM as i64,
                n_tokens as i64,
                (*self.decoder_memory).nb[1],
                0,
            );
            let mut x = ffi::ggml_add(gctx, tok_emb, audio_emb);
            let causal_mask = ffi::ggml_new_tensor_2d(
                gctx,
                ffi::ggml_type::GGML_TYPE_F32,
                n_tokens as i64,
                n_tokens as i64,
            );
            set_name(causal_mask, "causal_mask");
            ffi::ggml_backend_sched_set_tensor_backend(
                self.sched_dec_pre,
                causal_mask,
                self.backend,
            );

            for (idx, _) in model.dec_layers.iter().enumerate() {
                x = self.build_decoder_layer(
                    model,
                    gctx,
                    gf,
                    x,
                    positions,
                    time_emb,
                    idx,
                    n_tokens,
                    0,
                    causal_mask,
                );
            }
            x = ffi::ggml_rms_norm(gctx, x, VOX_DEC_NORM_EPS);
            x = ffi::ggml_mul(gctx, x, model.dec_norm_weight);
            let last_hidden = ffi::ggml_view_1d(
                gctx,
                x,
                VOX_DEC_DIM as i64,
                (n_tokens as usize - 1) * (*x).nb[1],
            );
            let logits = ffi::ggml_mul_mat(gctx, model.tok_embeddings_weight, last_hidden);
            ffi::ggml_build_forward_expand(gf, ffi::ggml_cpy(gctx, logits, self.decoder_logits));
            gf
        }
    }

    unsafe fn build_decoder_step_graph(
        &mut self,
        model: &GgmlModel,
        gctx: *mut ffi::ggml_context,
        position: i32,
        audio_pos: i32,
    ) -> *mut ffi::ggml_cgraph {
        unsafe {
            let gf = ffi::ggml_new_graph_custom(gctx, GGML_DEFAULT_GRAPH_SIZE * 4, false);
            let kv_used = self.kv_used;
            let token_id = ffi::ggml_new_tensor_1d(gctx, ffi::ggml_type::GGML_TYPE_I32, 1);
            set_name(token_id, "token_id");
            ffi::ggml_backend_sched_set_tensor_backend(self.sched_dec_step, token_id, self.backend);
            let pos_tensor = ffi::ggml_new_tensor_1d(gctx, ffi::ggml_type::GGML_TYPE_I32, 1);
            set_name(pos_tensor, "position");
            ffi::ggml_backend_sched_set_tensor_backend(
                self.sched_dec_step,
                pos_tensor,
                self.backend,
            );
            let time_emb =
                ffi::ggml_new_tensor_1d(gctx, ffi::ggml_type::GGML_TYPE_F32, VOX_DEC_DIM as i64);
            set_name(time_emb, "time_emb");
            ffi::ggml_backend_sched_set_tensor_backend(self.sched_dec_step, time_emb, self.backend);

            let tok_emb = ffi::ggml_get_rows(gctx, model.tok_embeddings_weight, token_id);
            let audio_emb = ffi::ggml_view_2d(
                gctx,
                self.decoder_memory,
                VOX_DEC_DIM as i64,
                1,
                (*self.decoder_memory).nb[1],
                audio_pos as usize * (*self.decoder_memory).nb[1],
            );
            let mut x = ffi::ggml_add(gctx, tok_emb, audio_emb);
            for (idx, _) in model.dec_layers.iter().enumerate() {
                x = self.build_decoder_layer(
                    model,
                    gctx,
                    gf,
                    x,
                    pos_tensor,
                    time_emb,
                    idx,
                    1,
                    kv_used,
                    ptr::null_mut(),
                );
            }
            x = ffi::ggml_rms_norm(gctx, x, VOX_DEC_NORM_EPS);
            x = ffi::ggml_mul(gctx, x, model.dec_norm_weight);
            let x_flat = ffi::ggml_reshape_1d(gctx, x, VOX_DEC_DIM as i64);
            let logits = ffi::ggml_mul_mat(gctx, model.tok_embeddings_weight, x_flat);
            ffi::ggml_build_forward_expand(gf, ffi::ggml_cpy(gctx, logits, self.decoder_logits));
            let _ = position;
            gf
        }
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn build_decoder_layer(
        &mut self,
        model: &GgmlModel,
        gctx: *mut ffi::ggml_context,
        gf: *mut ffi::ggml_cgraph,
        mut x: Tensor,
        positions: Tensor,
        time_emb: Tensor,
        layer_idx: usize,
        n_tokens: i32,
        kv_offset: i32,
        attn_mask: Tensor,
    ) -> Tensor {
        unsafe {
            let layer = model.dec_layers[layer_idx];
            let kv_dim = (VOX_DEC_KV_HEADS * VOX_DEC_HEAD_DIM) as i64;

            let residual = x;
            let mut x_norm = ffi::ggml_rms_norm(gctx, x, VOX_DEC_NORM_EPS);
            x_norm = ffi::ggml_mul(gctx, x_norm, layer.attn_norm_weight);
            let mut q = ffi::ggml_mul_mat(gctx, layer.attn_q_weight, x_norm);
            let mut k = ffi::ggml_mul_mat(gctx, layer.attn_k_weight, x_norm);
            let v = ffi::ggml_mul_mat(gctx, layer.attn_v_weight, x_norm);

            q = ffi::ggml_reshape_3d(
                gctx,
                q,
                VOX_DEC_HEAD_DIM as i64,
                VOX_DEC_HEADS as i64,
                n_tokens as i64,
            );
            k = ffi::ggml_reshape_3d(
                gctx,
                k,
                VOX_DEC_HEAD_DIM as i64,
                VOX_DEC_KV_HEADS as i64,
                n_tokens as i64,
            );
            q = rope(
                gctx,
                q,
                positions,
                VOX_DEC_HEAD_DIM as i32,
                VOX_DEC_ROPE_THETA,
            );
            k = rope(
                gctx,
                k,
                positions,
                VOX_DEC_HEAD_DIM as i32,
                VOX_DEC_ROPE_THETA,
            );
            q = ffi::ggml_cont(
                gctx,
                ffi::ggml_reshape_2d(
                    gctx,
                    q,
                    (VOX_DEC_HEADS * VOX_DEC_HEAD_DIM) as i64,
                    n_tokens as i64,
                ),
            );
            k = ffi::ggml_cont(gctx, ffi::ggml_reshape_2d(gctx, k, kv_dim, n_tokens as i64));

            let k_cache_slice = ffi::ggml_view_2d(
                gctx,
                self.kv_self_k,
                kv_dim,
                n_tokens as i64,
                (*self.kv_self_k).nb[1],
                layer_idx * (*self.kv_self_k).nb[2] + kv_offset as usize * (*self.kv_self_k).nb[1],
            );
            ffi::ggml_build_forward_expand(gf, ffi::ggml_cpy(gctx, k, k_cache_slice));
            let v_cache_slice = ffi::ggml_view_2d(
                gctx,
                self.kv_self_v,
                kv_dim,
                n_tokens as i64,
                (*self.kv_self_v).nb[1],
                layer_idx * (*self.kv_self_v).nb[2] + kv_offset as usize * (*self.kv_self_v).nb[1],
            );
            ffi::ggml_build_forward_expand(gf, ffi::ggml_cpy(gctx, v, v_cache_slice));

            let n_kv = kv_offset + n_tokens;
            let k_full = ffi::ggml_view_2d(
                gctx,
                self.kv_self_k,
                kv_dim,
                n_kv as i64,
                (*self.kv_self_k).nb[1],
                layer_idx * (*self.kv_self_k).nb[2],
            );
            let v_full = ffi::ggml_view_2d(
                gctx,
                self.kv_self_v,
                kv_dim,
                n_kv as i64,
                (*self.kv_self_v).nb[1],
                layer_idx * (*self.kv_self_v).nb[2],
            );

            let mut q3 = ffi::ggml_reshape_3d(
                gctx,
                q,
                VOX_DEC_HEAD_DIM as i64,
                VOX_DEC_HEADS as i64,
                n_tokens as i64,
            );
            q3 = ffi::ggml_permute(gctx, q3, 0, 2, 1, 3);
            let mut k3 = ffi::ggml_reshape_3d(
                gctx,
                k_full,
                VOX_DEC_HEAD_DIM as i64,
                VOX_DEC_KV_HEADS as i64,
                n_kv as i64,
            );
            k3 = ffi::ggml_permute(gctx, k3, 0, 2, 1, 3);
            let mut v3 = ffi::ggml_reshape_3d(
                gctx,
                v_full,
                VOX_DEC_HEAD_DIM as i64,
                VOX_DEC_KV_HEADS as i64,
                n_kv as i64,
            );
            v3 = ffi::ggml_permute(gctx, v3, 0, 2, 1, 3);
            let mask_f16 = if attn_mask.is_null() {
                ptr::null_mut()
            } else {
                ffi::ggml_cast(gctx, attn_mask, ffi::ggml_type::GGML_TYPE_F16)
            };
            let scale = 1.0f32 / (VOX_DEC_HEAD_DIM as f32).sqrt();
            let mut attn_out =
                ffi::ggml_flash_attn_ext(gctx, q3, k3, v3, mask_f16, scale, 0.0, 0.0);
            attn_out = ffi::ggml_cont(gctx, attn_out);
            attn_out = ffi::ggml_reshape_2d(
                gctx,
                attn_out,
                (VOX_DEC_HEADS * VOX_DEC_HEAD_DIM) as i64,
                n_tokens as i64,
            );
            let attn_proj = ffi::ggml_mul_mat(gctx, layer.attn_o_weight, attn_out);
            x = ffi::ggml_add(gctx, residual, attn_proj);

            let residual = x;
            let mut h_norm = ffi::ggml_rms_norm(gctx, x, VOX_DEC_NORM_EPS);
            h_norm = ffi::ggml_mul(gctx, h_norm, layer.ffn_norm_weight);
            let mut ada_hidden = ffi::ggml_mul_mat(gctx, layer.ada0_weight, time_emb);
            ada_hidden = ffi::ggml_gelu_erf(gctx, ada_hidden);
            let ada_scale = ffi::ggml_mul_mat(gctx, layer.ada2_weight, ada_hidden);
            let scaled = ffi::ggml_mul(gctx, h_norm, ada_scale);
            h_norm = ffi::ggml_add(gctx, h_norm, scaled);

            let mut gate = ffi::ggml_mul_mat(gctx, layer.ffn_w1_weight, h_norm);
            gate = ffi::ggml_silu(gctx, gate);
            let up = ffi::ggml_mul_mat(gctx, layer.ffn_w3_weight, h_norm);
            let mut ffn_out = ffi::ggml_mul(gctx, gate, up);
            ffn_out = ffi::ggml_mul_mat(gctx, layer.ffn_w2_weight, ffn_out);
            ffi::ggml_add(gctx, residual, ffn_out)
        }
    }
}

impl Drop for GgmlSession {
    fn drop(&mut self) {
        unsafe {
            if !self.sched_encoder.is_null() {
                ffi::ggml_backend_sched_free(self.sched_encoder);
            }
            if !self.sched_adapter.is_null() {
                ffi::ggml_backend_sched_free(self.sched_adapter);
            }
            if !self.sched_dec_pre.is_null() {
                ffi::ggml_backend_sched_free(self.sched_dec_pre);
            }
            if !self.sched_dec_step.is_null() {
                ffi::ggml_backend_sched_free(self.sched_dec_step);
            }
            if !self.buf_enc_full.is_null() {
                ffi::ggml_backend_buffer_free(self.buf_enc_full);
            }
            if !self.ctx_enc_full.is_null() {
                ffi::ggml_free(self.ctx_enc_full);
            }
            if !self.buf_dec_mem.is_null() {
                ffi::ggml_backend_buffer_free(self.buf_dec_mem);
            }
            if !self.ctx_dec_mem.is_null() {
                ffi::ggml_free(self.ctx_dec_mem);
            }
            if !self.buf_persistent.is_null() {
                ffi::ggml_backend_buffer_free(self.buf_persistent);
            }
            if !self.ctx_persistent.is_null() {
                ffi::ggml_free(self.ctx_persistent);
            }
            if !self.blas_backend.is_null() {
                ffi::ggml_backend_free(self.blas_backend);
            }
            if !self.backend_cpu.is_null() {
                ffi::ggml_backend_free(self.backend_cpu);
            }
            if !self.backend.is_null() {
                ffi::ggml_backend_free(self.backend);
            }
        }
    }
}

impl MetaContext {
    fn new(graph_size: usize) -> Result<Self> {
        let meta_size = unsafe {
            ffi::ggml_tensor_overhead() * graph_size
                + ffi::ggml_graph_overhead_custom(graph_size, false)
        };
        let mut buf = vec![0u8; meta_size];
        let params = ffi::ggml_init_params {
            mem_size: meta_size,
            mem_buffer: buf.as_mut_ptr() as *mut c_void,
            no_alloc: true,
        };
        let ctx = unsafe { ffi::ggml_init(params) };
        if ctx.is_null() {
            return Err(Error::Runtime(
                "ggml_init failed for Voxtral graph context".into(),
            ));
        }
        Ok(Self { ctx, _buf: buf })
    }
}

fn load_weight_tensors(
    path: &Path,
    gguf: *mut ffi::gguf_context,
    ctx: *mut ffi::ggml_context,
) -> Result<HashMap<String, Tensor>> {
    let mut file = File::open(path)?;
    let data_offset = unsafe { ffi::gguf_get_data_offset(gguf) as u64 };
    let n_tensors = unsafe { ffi::gguf_get_n_tensors(gguf) };
    let mut out = HashMap::with_capacity(n_tensors.max(0) as usize);

    for i in 0..n_tensors {
        let name_ptr = unsafe { ffi::gguf_get_tensor_name(gguf, i) };
        if name_ptr.is_null() {
            continue;
        }
        let name = unsafe { CStr::from_ptr(name_ptr) }
            .to_string_lossy()
            .into_owned();
        let tensor = unsafe { ffi::ggml_get_tensor(ctx, name_ptr) };
        if tensor.is_null() {
            continue;
        }

        let offset = data_offset + unsafe { ffi::gguf_get_tensor_offset(gguf, i) as u64 };
        let nbytes = unsafe { ffi::ggml_nbytes(tensor) };
        let mut bytes = vec![0u8; nbytes];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut bytes)?;
        unsafe {
            ffi::ggml_backend_tensor_set(tensor, bytes.as_ptr() as *const c_void, 0, nbytes);
        }
        out.insert(name, tensor);
    }

    Ok(out)
}

unsafe fn load_tokenizer_metadata(
    gguf: *mut ffi::gguf_context,
) -> Result<(i32, HashSet<i32>, Vec<String>)> {
    unsafe {
        let mut num_special = 1000;
        let key_num = cstring("voxtral.tokenizer.num_special_tokens");
        let key_num_id = ffi::gguf_find_key(gguf, key_num.as_ptr());
        if key_num_id >= 0 {
            num_special = ffi::gguf_get_val_i32(gguf, key_num_id);
        }

        let mut special_ranks = HashSet::new();
        let key_special = cstring("voxtral.tokenizer.special_token_ranks");
        let key_special_id = ffi::gguf_find_key(gguf, key_special.as_ptr());
        if key_special_id >= 0
            && ffi::gguf_get_kv_type(gguf, key_special_id) == ffi::GGUF_TYPE_ARRAY
            && ffi::gguf_get_arr_type(gguf, key_special_id) == ffi::GGUF_TYPE_INT32
        {
            let n = ffi::gguf_get_arr_n(gguf, key_special_id).max(0) as usize;
            let data = ffi::gguf_get_arr_data(gguf, key_special_id) as *const i32;
            if !data.is_null() {
                for i in 0..n {
                    special_ranks.insert(*data.add(i));
                }
            }
        }

        let mut vocab = Vec::new();
        let key_vocab = cstring("voxtral.tokenizer.vocab_token_bytes_b64");
        let key_vocab_id = ffi::gguf_find_key(gguf, key_vocab.as_ptr());
        if key_vocab_id >= 0
            && ffi::gguf_get_kv_type(gguf, key_vocab_id) == ffi::GGUF_TYPE_ARRAY
            && ffi::gguf_get_arr_type(gguf, key_vocab_id) == ffi::GGUF_TYPE_STRING
        {
            let n = ffi::gguf_get_arr_n(gguf, key_vocab_id).max(0) as usize;
            vocab.reserve(n);
            for i in 0..n {
                let s = ffi::gguf_get_arr_str(gguf, key_vocab_id, i as size_t);
                vocab.push(if s.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(s).to_string_lossy().into_owned()
                });
            }
        }
        if vocab.is_empty() {
            return Err(Error::Parse(
                "Voxtral GGUF is missing tokenizer vocab metadata".into(),
            ));
        }
        Ok((num_special, special_ranks, vocab))
    }
}

fn init_weight_backend(backend: VoxtralSttBackend) -> (ffi::ggml_backend_t, bool) {
    match backend {
        VoxtralSttBackend::Metal => {
            #[cfg(target_os = "macos")]
            unsafe {
                let metal = ffi::ggml_backend_metal_init();
                if !metal.is_null() {
                    return (metal, true);
                }
            }
            (init_cpu_backend(), false)
        }
        VoxtralSttBackend::Cpu | VoxtralSttBackend::Wgsl | VoxtralSttBackend::Cuda => {
            (init_cpu_backend(), false)
        }
    }
}

fn init_compute_backend(
    backend: VoxtralSttBackend,
    prefer_accel: bool,
    threads: i32,
) -> (ffi::ggml_backend_t, ffi::ggml_backend_t, bool) {
    match backend {
        VoxtralSttBackend::Metal if prefer_accel => {
            #[cfg(target_os = "macos")]
            unsafe {
                let metal = ffi::ggml_backend_metal_init();
                if !metal.is_null() {
                    let cpu = init_cpu_backend_with_threads(threads);
                    return (metal, cpu, true);
                }
            }
            (
                init_cpu_backend_with_threads(threads),
                ptr::null_mut(),
                false,
            )
        }
        _ => (
            init_cpu_backend_with_threads(threads),
            ptr::null_mut(),
            false,
        ),
    }
}

fn init_cpu_backend() -> ffi::ggml_backend_t {
    let threads = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
        .min(i32::MAX as usize) as i32;
    init_cpu_backend_with_threads(threads)
}

fn init_cpu_backend_with_threads(threads: i32) -> ffi::ggml_backend_t {
    let backend = unsafe { ffi::ggml_backend_cpu_init() };
    if !backend.is_null() {
        unsafe {
            ffi::ggml_backend_cpu_set_n_threads(backend, threads);
        }
    }
    backend
}

fn init_blas_backend(threads: i32) -> ffi::ggml_backend_t {
    let backend = unsafe { ffi::ggml_backend_blas_init() };
    if !backend.is_null() {
        unsafe {
            ffi::ggml_backend_blas_set_n_threads(backend, threads);
        }
    }
    backend
}

unsafe fn causal_conv1d_graph(
    ctx: *mut ffi::ggml_context,
    x: Tensor,
    in_len: i32,
    weight: Tensor,
    bias: Tensor,
    out_channels: i32,
    kernel_size: i32,
    stride: i32,
) -> Result<(Tensor, i32)> {
    unsafe {
        let dims = compute_causal_conv1d_dims(in_len, kernel_size, stride);
        if dims.out_len <= 0 {
            return Err(Error::Runtime("invalid causal conv1d output shape".into()));
        }
        let x_pad = ffi::ggml_pad_ext(ctx, x, dims.pad_left, dims.pad_right, 0, 0, 0, 0, 0, 0);
        if x_pad.is_null() {
            return Err(Error::Runtime("ggml_pad_ext returned null".into()));
        }
        let mut y = ffi::ggml_conv_1d(ctx, weight, x_pad, stride, 0, 1);
        if y.is_null() {
            return Err(Error::Runtime("ggml_conv_1d returned null".into()));
        }
        if !bias.is_null() {
            y = ffi::ggml_add(
                ctx,
                y,
                ffi::ggml_reshape_3d(ctx, bias, 1, out_channels as i64, 1),
            );
        }
        Ok((y, dims.out_len))
    }
}

struct ConvDims {
    pad_left: i32,
    pad_right: i32,
    out_len: i32,
}

fn compute_causal_conv1d_dims(in_len: i32, kernel_size: i32, stride: i32) -> ConvDims {
    if in_len <= 0 || kernel_size <= 0 || stride <= 0 {
        return ConvDims {
            pad_left: 0,
            pad_right: 0,
            out_len: 0,
        };
    }
    let padding_total = kernel_size - stride;
    let n_frames = (in_len - kernel_size + padding_total) as f32 / stride as f32 + 1.0;
    let target_length = (n_frames.ceil() as i32 - 1) * stride + (kernel_size - padding_total);
    let extra_padding = target_length - in_len;
    let pad_left = padding_total;
    let pad_right = extra_padding.max(0);
    let padded_len = in_len + pad_left + pad_right;
    let out_len = (padded_len - kernel_size) / stride + 1;
    ConvDims {
        pad_left,
        pad_right,
        out_len,
    }
}

fn mel_frames_to_enc_tokens(n_frames: i32) -> i32 {
    let d0 = compute_causal_conv1d_dims(n_frames, 3, 1);
    let d1 = compute_causal_conv1d_dims(d0.out_len, 3, 2);
    d1.out_len - d1.out_len % VOX_DOWNSAMPLE_FACTOR as i32
}

fn compute_total_enc_tokens(total_mel_frames: i32) -> i32 {
    let mel_stride = ENC_CHUNK_MEL - ENC_CHUNK_OVERLAP * 2;
    let mut total = 0;
    let mut mel_offset = 0;
    let mut first = true;
    while mel_offset < total_mel_frames {
        let chunk_mel = ENC_CHUNK_MEL.min(total_mel_frames - mel_offset);
        let chunk_tokens = mel_frames_to_enc_tokens(chunk_mel);
        let skip = if first { 0 } else { ENC_CHUNK_OVERLAP };
        let stride = chunk_tokens - skip;
        if stride <= 0 {
            break;
        }
        total += stride;
        mel_offset += mel_stride;
        first = false;
    }
    total
}

unsafe fn rope(
    ctx: *mut ffi::ggml_context,
    x: Tensor,
    positions: Tensor,
    head_dim: i32,
    theta: f32,
) -> Tensor {
    unsafe {
        ffi::ggml_rope_ext(
            ctx,
            x,
            positions,
            ptr::null_mut(),
            head_dim,
            0,
            0,
            theta,
            1.0,
            0.0,
            1.0,
            0.0,
            0.0,
        )
    }
}

fn compute_time_embedding(t: f32, dim: usize) -> Vec<f32> {
    let half = dim / 2;
    let mut out = vec![0.0; dim];
    for i in 0..half {
        let inv_freq = (-(10000.0f32).ln() * i as f32 / half as f32).exp();
        let angle = t * inv_freq;
        out[i] = angle.cos();
        out[i + half] = angle.sin();
    }
    out
}

fn drop_first_mel_frame(mel: &[f32], n_frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; VOX_NUM_MEL_BINS * (n_frames - 1)];
    for m in 0..VOX_NUM_MEL_BINS {
        let src = &mel[m * n_frames + 1..(m + 1) * n_frames];
        let dst = &mut out[m * (n_frames - 1)..(m + 1) * (n_frames - 1)];
        dst.copy_from_slice(src);
    }
    out
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

unsafe fn graph_tensor(gf: *mut ffi::ggml_cgraph, name: &str) -> Tensor {
    unsafe {
        let name = cstring(name);
        ffi::ggml_graph_get_tensor(gf, name.as_ptr())
    }
}

unsafe fn set_name(t: Tensor, name: &str) {
    unsafe {
        if !t.is_null() {
            let name = cstring(name);
            ffi::ggml_set_name(t, name.as_ptr());
        }
    }
}

fn check_status(status: ffi::ggml_status, what: &str) -> Result<()> {
    if status == ffi::ggml_status::GGML_STATUS_SUCCESS {
        Ok(())
    } else {
        Err(Error::Runtime(format!("{what} failed: {status:?}")))
    }
}

fn base64_decode(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4 + 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let val = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => 26 + byte - b'a',
            b'0'..=b'9' => 52 + byte - b'0',
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        } as u32;
        acc = (acc << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    out
}

fn cstring_path(path: &Path) -> Result<CString> {
    CString::new(path.as_os_str().to_string_lossy().as_bytes())
        .map_err(|_| Error::InvalidFormat("path contains interior NUL byte"))
}

fn cstring(value: &str) -> CString {
    CString::new(value).expect("static strings passed to ggml do not contain NUL bytes")
}
