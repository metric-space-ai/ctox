//! Target and draft loaders.
//!
//! Merged from two byte-exact ports of lucebox/dflash:
//!
//!   * `dflash/src/gguf_target_loader.cpp` — loads Qwen3.5-27B qwen35
//!     hybrid from GGUF into a CUDA-resident ggml context, including
//!     the CpuEmbedder mmap lifecycle
//!   * `dflash/src/safetensors_draft.cpp` — loads the z-lab DFlash draft
//!     from safetensors into a CUDA-resident ggml context
//!
//! Both translate file → ggml context + backend buffer; both surface
//! the same failure pattern (return `false`, populate `last_error`).
//! They live in the same module here so callers pull both loaders out
//! of a single `use crate::loader::{...}` import.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::path::Path;

use crate::ffi as sys;
use sys::{ggml_backend_t, ggml_context, ggml_tensor, ggml_type};

use crate::{
    set_last_error, DFLASH27B_DRAFT_LAYERS, DFLASH27B_DRAFT_N_TARGET_LAYERS,
    DFLASH27B_TARGET_HEAD_DIM, DFLASH27B_TARGET_HIDDEN, DFLASH27B_TARGET_INTERMEDIATE,
    DFLASH27B_TARGET_N_HEADS, DFLASH27B_TARGET_N_KV_HEADS, DFLASH27B_TARGET_VOCAB,
};
use crate::model::{CpuEmbedder, DraftLayer, DraftWeights, TargetLayer, TargetWeights};

// ════════════════════════════════════════════════════════════════
//  GGUF TARGET LOADER (gguf_target_loader.cpp)
// ════════════════════════════════════════════════════════════════

// ─── CpuEmbedder impls ───────────────────────────────────────────
//
// ref: `gguf_target_loader.cpp:62-78`

impl Drop for CpuEmbedder {
    /// ref: `gguf_target_loader.cpp:62-65`
    fn drop(&mut self) {
        unsafe {
            if !self.mmap_addr.is_null() {
                libc::munmap(self.mmap_addr, self.mmap_len);
            }
            if self.mmap_fd >= 0 {
                libc::close(self.mmap_fd);
            }
        }
    }
}

impl CpuEmbedder {
    /// Dequantize N rows specified by `ids` into `out_f32` (shape
    /// `[n_embd, n]`). Values are written contiguously row-major
    /// (`n_embd` fast axis).
    ///
    /// ref: `gguf_target_loader.cpp:67-78`
    pub fn embed(&self, ids: &[i32], out_f32: &mut [f32]) -> bool {
        if self.tok_embd_bytes.is_null() || self.tok_embd_type == ggml_type::GGML_TYPE_COUNT {
            return false;
        }
        let tr = unsafe { sys::ggml_get_type_traits(self.tok_embd_type) };
        if tr.is_null() {
            return false;
        }
        let to_float = unsafe { (*tr).to_float };
        let Some(to_float) = to_float else {
            return false;
        };
        let n = ids.len();
        if out_f32.len() < n * (self.n_embd as usize) {
            return false;
        }
        for i in 0..n {
            let id = ids[i];
            if id < 0 || id as i64 >= self.n_vocab {
                return false;
            }
            unsafe {
                let row = self
                    .tok_embd_bytes
                    .add((id as usize) * self.row_bytes);
                let dst = out_f32.as_mut_ptr().add(i * (self.n_embd as usize));
                to_float(row as *const c_void, dst, self.n_embd);
            }
        }
        true
    }
}

// ─── Mmap helper (private) ───────────────────────────────────────
//
// ref: `gguf_target_loader.cpp:80-106`
//
// Local mmap used only during load (separate from the one kept alive
// inside `TargetWeights::embedder`). We don't munmap when we want to
// hand ownership to the `CpuEmbedder` — see `release()`.

struct Mmap {
    addr: *mut c_void,
    len: libc::size_t,
    fd: libc::c_int,
}

impl Mmap {
    fn open_ro(path: &Path) -> Result<Self, String> {
        let cpath = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|e| format!("path contains NUL: {e}"))?;
        let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY) };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("open: {}: {err}", path.display()));
        }
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd, &mut st) } < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(format!("fstat: {err}"));
        }
        let len = st.st_size as libc::size_t;
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                fd,
                0,
            )
        };
        if addr == libc::MAP_FAILED {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(format!("mmap: {err}"));
        }
        Ok(Self { addr, len, fd })
    }

    /// Ownership transfer: release the mmap handle without unmapping.
    /// The consumer (`CpuEmbedder`) takes over lifetime management.
    fn release(mut self) -> (*mut c_void, libc::size_t, libc::c_int) {
        let r = (self.addr, self.len, self.fd);
        self.addr = std::ptr::null_mut();
        self.fd = -1;
        self.len = 0;
        r
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        unsafe {
            if !self.addr.is_null() {
                libc::munmap(self.addr, self.len);
            }
            if self.fd >= 0 {
                libc::close(self.fd);
            }
        }
    }
}

// ─── Small gguf helpers (mirror the anonymous-namespace C++ helpers) ─
//
// ref: `gguf_target_loader.cpp:109-132`

/// `expect_u32` — required uint32 metadata key → bound check. Returns
/// `Err(msg)` if missing or mismatched.
///
/// ref: `gguf_target_loader.cpp:109-120`
fn expect_u32(g: *const sys::gguf_context, key: &str, expected: u32) -> Result<(), String> {
    let ckey = CString::new(key).unwrap();
    let id = unsafe { sys::gguf_find_key(g, ckey.as_ptr()) };
    if id < 0 {
        return Err(format!("missing gguf key: {key}"));
    }
    let v = unsafe { sys::gguf_get_val_u32(g, id) };
    if v != expected {
        return Err(format!("gguf key {key}={v} expected {expected}"));
    }
    Ok(())
}

/// ref: `gguf_target_loader.cpp:122-126`
///
/// The reference uses this for optional i32 keys, but the shipping
/// 27B GGUF we target has all hparam keys as u32, so this helper is
/// kept for parity with future loaders that need it.
#[allow(dead_code)]
fn get_i32_or(g: *const sys::gguf_context, key: &str, fallback: i32) -> i32 {
    let ckey = CString::new(key).unwrap();
    let id = unsafe { sys::gguf_find_key(g, ckey.as_ptr()) };
    if id < 0 {
        return fallback;
    }
    unsafe { sys::gguf_get_val_i32(g, id) }
}

/// ref: `gguf_target_loader.cpp:128-132`
fn get_u32_or(g: *const sys::gguf_context, key: &str, fallback: u32) -> u32 {
    let ckey = CString::new(key).unwrap();
    let id = unsafe { sys::gguf_find_key(g, ckey.as_ptr()) };
    if id < 0 {
        return fallback;
    }
    unsafe { sys::gguf_get_val_u32(g, id) }
}

// ─── load_target_gguf ────────────────────────────────────────────
//
// ref: `gguf_target_loader.cpp:136-374`

/// Load a Q4_K_M target model from a GGUF file on disk.
///
/// Returns `false` and sets `last_error` on failure — mirroring the
/// reference return-convention exactly. Callers check the result and
/// read `errors::last_error()` for the message.
///
/// ref: `gguf_target_loader.cpp:136-374`
pub fn load_target_gguf(
    path: &Path,
    backend: ggml_backend_t,
    out: &mut TargetWeights,
) -> bool {
    match load_target_gguf_inner(path, backend, out) {
        Ok(summary) => {
            // Stash the total for callers that want to print it.
            set_last_error(summary);
            true
        }
        Err(msg) => {
            set_last_error(msg);
            false
        }
    }
}

fn load_target_gguf_inner(
    path: &Path,
    backend: ggml_backend_t,
    out: &mut TargetWeights,
) -> Result<String, String> {
    // ── 1. Parse metadata + create a ggml_context holding tensor
    //       descriptors. ref: lines 140-149
    let cpath = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| format!("path contains NUL: {e}"))?;
    let mut meta_ctx: *mut ggml_context = std::ptr::null_mut();
    let gip = sys::gguf_init_params {
        no_alloc: true,
        ctx: &mut meta_ctx,
    };
    let gctx = unsafe { sys::gguf_init_from_file(cpath.as_ptr(), gip) };
    if gctx.is_null() {
        return Err(format!(
            "gguf_init_from_file failed: {}",
            path.display()
        ));
    }

    // GGUF ownership guard — auto-free on any early return path.
    struct GgufGuard(*mut sys::gguf_context);
    impl Drop for GgufGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { sys::gguf_free(self.0) };
            }
        }
    }
    let gctx_guard = GgufGuard(gctx);

    // Validate arch + the dimensions we hardcode everywhere.
    // ref: lines 151-165
    {
        let arch_key = CString::new("general.architecture").unwrap();
        let arch_id = unsafe { sys::gguf_find_key(gctx, arch_key.as_ptr()) };
        if arch_id < 0 {
            return Err("missing general.architecture".into());
        }
        let arch_ptr = unsafe { sys::gguf_get_val_str(gctx, arch_id) };
        let arch = unsafe { CStr::from_ptr(arch_ptr) }
            .to_string_lossy()
            .into_owned();
        if arch != "qwen35" {
            return Err(format!(
                "unexpected arch: {arch} (expected qwen35)"
            ));
        }
    }

    // ref: lines 167-195
    let n_embd = get_u32_or(gctx, "qwen35.embedding_length", 0);
    let n_ff = get_u32_or(gctx, "qwen35.feed_forward_length", 0);
    let n_layer = get_u32_or(gctx, "qwen35.block_count", 0);
    let n_head = get_u32_or(gctx, "qwen35.attention.head_count", 0);
    let n_headkv = get_u32_or(gctx, "qwen35.attention.head_count_kv", 0);
    let kl = get_u32_or(gctx, "qwen35.attention.key_length", 0);
    let vl = get_u32_or(gctx, "qwen35.attention.value_length", 0);
    let fai = get_u32_or(gctx, "qwen35.full_attention_interval", 0);
    let ssm_conv = get_u32_or(gctx, "qwen35.ssm.conv_kernel", 0);
    let ssm_inner = get_u32_or(gctx, "qwen35.ssm.inner_size", 0);
    let ssm_state = get_u32_or(gctx, "qwen35.ssm.state_size", 0);
    let ssm_dt = get_u32_or(gctx, "qwen35.ssm.time_step_rank", 0);
    let ssm_grp = get_u32_or(gctx, "qwen35.ssm.group_count", 0);

    if n_embd != 5120
        || n_layer != 64
        || n_head != 24
        || n_headkv != 4
        || kl != 256
        || vl != 256
        || n_ff != 17_408
        || fai != 4
        || ssm_conv != 4
        || ssm_inner != 6144
        || ssm_state != 128
        || ssm_dt != 48
        || ssm_grp != 16
    {
        return Err(format!(
            "unexpected hparams: n_embd={n_embd} n_layer={n_layer} n_head={n_head} n_head_kv={n_headkv} \
             kl={kl} vl={vl} n_ff={n_ff} fai={fai} ssm{{conv={ssm_conv} inner={ssm_inner} state={ssm_state} dt={ssm_dt} grp={ssm_grp}}}"
        ));
    }
    // Suppress unused warning — we validate but don't store per-key.
    let _ = expect_u32; // keep helper linked for future keys

    // rope dimension_sections (array of 4 uint32). ref: lines 197-208
    let mut rope_sections: [i32; 4] = [0, 0, 0, 0];
    {
        let rkey = CString::new("qwen35.rope.dimension_sections").unwrap();
        let rid = unsafe { sys::gguf_find_key(gctx, rkey.as_ptr()) };
        if rid >= 0 {
            let n = unsafe { sys::gguf_get_arr_n(gctx, rid) };
            if n >= 4 {
                let arr = unsafe { sys::gguf_get_arr_data(gctx, rid) as *const i32 };
                for k in 0..4 {
                    rope_sections[k] = unsafe { *arr.add(k) };
                }
            }
        }
    }

    // ref: lines 210-226
    out.ctx = meta_ctx;
    out.backend = backend;
    out.n_layer = n_layer as i32;
    out.n_embd = n_embd as i32;
    out.n_ff = n_ff as i32;
    out.n_head = n_head as i32;
    out.n_head_kv = n_headkv as i32;
    out.n_embd_head_k = kl as i32;
    out.n_embd_head_v = vl as i32;
    out.full_attention_interval = fai as i32;
    out.rope_sections = rope_sections;
    out.ssm_d_conv = ssm_conv as i32;
    out.ssm_d_inner = ssm_inner as i32;
    out.ssm_d_state = ssm_state as i32;
    out.ssm_dt_rank = ssm_dt as i32;
    out.ssm_n_group = ssm_grp as i32;
    out.layers.clear();
    out.layers.resize_with(n_layer as usize, TargetLayer::default);

    // ── 2. Wire our layer pointers to tensors inside meta_ctx.
    //       ref: lines 228-301
    let g = |name: &str| -> *mut ggml_tensor {
        let cname = CString::new(name).unwrap();
        unsafe { sys::ggml_get_tensor(meta_ctx, cname.as_ptr()) }
    };
    out.tok_embd = g("token_embd.weight");
    out.out_norm = g("output_norm.weight");
    out.output = g("output.weight");
    if out.tok_embd.is_null() || out.out_norm.is_null() || out.output.is_null() {
        return Err("missing top-level tensors (token_embd/output_norm/output)".into());
    }

    for il in 0..(n_layer as i32) {
        // Per-layer lookup helper. Builds `blk.<il>.<suffix>`.
        let fnd = |suffix: &str| -> *mut ggml_tensor {
            let name = format!("blk.{il}.{suffix}");
            let cname = CString::new(name).unwrap();
            unsafe { sys::ggml_get_tensor(meta_ctx, cname.as_ptr()) }
        };
        let l = &mut out.layers[il as usize];

        // Always-present tensors. ref: lines 249-261
        l.attn_norm = fnd("attn_norm.weight");
        l.attn_post_norm = fnd("post_attention_norm.weight");
        l.w_gate = fnd("ffn_gate.weight");
        l.w_up = fnd("ffn_up.weight");
        l.w_down = fnd("ffn_down.weight");
        if l.attn_norm.is_null()
            || l.attn_post_norm.is_null()
            || l.w_gate.is_null()
            || l.w_up.is_null()
            || l.w_down.is_null()
        {
            return Err(format!("layer {il}: missing shared tensor"));
        }

        // Full-attention tensors (only on layers where (il+1)%fai == 0,
        // i.e. il%4 == 3 for fai=4). May be null on deltanet layers.
        // ref: lines 263-270
        l.wq = fnd("attn_q.weight");
        l.wk = fnd("attn_k.weight");
        l.wv = fnd("attn_v.weight");
        l.wo = fnd("attn_output.weight");
        l.q_norm = fnd("attn_q_norm.weight");
        l.k_norm = fnd("attn_k_norm.weight");

        // Gated DeltaNet tensors (null on full-attention layers).
        // ref: lines 272-281
        l.wqkv = fnd("attn_qkv.weight");
        l.wqkv_gate = fnd("attn_gate.weight");
        l.ssm_conv1d = fnd("ssm_conv1d.weight");
        l.ssm_beta = fnd("ssm_beta.weight");
        l.ssm_alpha = fnd("ssm_alpha.weight");
        l.ssm_a = fnd("ssm_a");
        l.ssm_dt_bias = fnd("ssm_dt.bias");
        l.ssm_norm = fnd("ssm_norm.weight");
        l.ssm_out = fnd("ssm_out.weight");

        // Sanity: each layer must be EITHER full-attn OR deltanet, not
        // both, not neither. ref: lines 283-300
        let has_attn = !l.wq.is_null()
            && !l.wk.is_null()
            && !l.wv.is_null()
            && !l.wo.is_null()
            && !l.q_norm.is_null()
            && !l.k_norm.is_null();
        let has_ssm = !l.wqkv.is_null()
            && !l.wqkv_gate.is_null()
            && !l.ssm_conv1d.is_null()
            && !l.ssm_out.is_null();
        let is_full_attn_layer = ((il + 1) % out.full_attention_interval) == 0;
        if is_full_attn_layer && !has_attn {
            return Err(format!("layer {il} expected full-attn, missing tensors"));
        }
        if !is_full_attn_layer && !has_ssm {
            return Err(format!("layer {il} expected deltanet, missing tensors"));
        }
    }

    // ── 3. Allocate CUDA buffer for all tensors in meta_ctx.
    //       ref: lines 303-309
    out.buf = unsafe { sys::ggml_backend_alloc_ctx_tensors(meta_ctx, backend) };
    if out.buf.is_null() {
        return Err("ggml_backend_alloc_ctx_tensors failed (target)".into());
    }

    // ── 4. mmap the file and copy tensor bytes to CUDA.
    //
    // SKIP uploading token_embd.weight — it stays on CPU for embedding
    // lookup (CUDA get_rows doesn't support k-quants). We hand the
    // mmap ownership to `TargetWeights::embedder` at the end.
    //
    // ref: lines 311-344
    let mm = Mmap::open_ro(path)?;
    let data_start = unsafe { sys::gguf_get_data_offset(gctx) };
    let n_tensors = unsafe { sys::gguf_get_n_tensors(gctx) };

    let mut total: libc::size_t = 0;
    let mut tok_embd_off: libc::size_t = 0;
    let mut tok_embd_sz: libc::size_t = 0;
    let mut tok_embd_type = ggml_type::GGML_TYPE_COUNT;
    for tid in 0..n_tensors {
        let tname_ptr = unsafe { sys::gguf_get_tensor_name(gctx, tid) };
        if tname_ptr.is_null() {
            continue;
        }
        let t = unsafe { sys::ggml_get_tensor(meta_ctx, tname_ptr) };
        if t.is_null() {
            continue;
        }
        let off = data_start + unsafe { sys::gguf_get_tensor_offset(gctx, tid) };
        let sz = unsafe { sys::gguf_get_tensor_size(gctx, tid) };
        if off + sz > mm.len {
            let tname = unsafe { CStr::from_ptr(tname_ptr) }
                .to_string_lossy()
                .into_owned();
            return Err(format!("tensor '{tname}' overflows file"));
        }
        // Compare C string in place without an alloc.
        let is_tok_embd = unsafe {
            libc::strcmp(tname_ptr, b"token_embd.weight\0".as_ptr().cast()) == 0
        };
        if is_tok_embd {
            // Remember offset + size for the CPU embedder; don't upload
            // to GPU.
            tok_embd_off = off;
            tok_embd_sz = sz;
            tok_embd_type = unsafe { sys::gguf_get_tensor_type(gctx, tid) };
            continue;
        }
        unsafe {
            let src = (mm.addr as *const u8).add(off);
            sys::ggml_backend_tensor_set(t, src as *const c_void, 0, sz);
        }
        total += sz;
    }

    // ref: lines 346-351
    drop(gctx_guard); // explicit release — mirrors `gguf_free(gctx)` in the C++.

    if tok_embd_off == 0 || tok_embd_type == ggml_type::GGML_TYPE_COUNT {
        return Err("token_embd.weight not found or invalid type".into());
    }

    // ── 5. Transfer mmap ownership to the CpuEmbedder so it can
    //       dequantize rows on demand without uploading the full
    //       embedding table to GPU. ref: lines 353-363
    let (mm_addr, mm_len, mm_fd) = mm.release();
    out.embedder.mmap_addr = mm_addr;
    out.embedder.mmap_len = mm_len;
    out.embedder.mmap_fd = mm_fd;
    out.embedder.tok_embd_bytes =
        unsafe { (mm_addr as *const u8).add(tok_embd_off) };
    out.embedder.tok_embd_type = tok_embd_type;
    out.embedder.n_embd = out.n_embd as i64;
    out.embedder.n_vocab = DFLASH27B_TARGET_VOCAB as i64;
    out.embedder.row_bytes = tok_embd_sz / (DFLASH27B_TARGET_VOCAB as usize);

    // ref: lines 365-371
    let type_name = unsafe {
        CStr::from_ptr(sys::ggml_type_name(tok_embd_type))
            .to_string_lossy()
            .into_owned()
    };
    Ok(format!(
        "target loaded: {n_tensors} tensors on GPU {:.2} GiB, tok_embd {:.0} MiB CPU-only ({type_name})",
        total as f64 / (1024.0 * 1024.0 * 1024.0),
        tok_embd_sz as f64 / (1024.0 * 1024.0)
    ))
}

// ─── free_target_weights ────────────────────────────────────────
//
// ref: `gguf_target_loader.cpp:376-384`

/// Frees everything `load_target_gguf` allocated. Idempotent — safe
/// to call on a freshly-default-constructed `TargetWeights`.
///
/// ref: `gguf_target_loader.cpp:376-384`
pub fn free_target_weights(w: &mut TargetWeights) {
    unsafe {
        if !w.buf.is_null() {
            sys::ggml_backend_buffer_free(w.buf);
            w.buf = std::ptr::null_mut();
        }
        if !w.ctx.is_null() {
            sys::ggml_free(w.ctx);
            w.ctx = std::ptr::null_mut();
        }
    }
    // CpuEmbedder's own Drop impl handles the mmap automatically.
    // Reset the embedder to default so double-free is safe.
    w.embedder = CpuEmbedder::default();
    w.layers.clear();
    w.tok_embd = std::ptr::null_mut();
    w.out_norm = std::ptr::null_mut();
    w.output = std::ptr::null_mut();
}


// ════════════════════════════════════════════════════════════════
//  SAFETENSORS DRAFT LOADER (safetensors_draft.cpp)
// ════════════════════════════════════════════════════════════════

// ─── StEntry / StMap — 1:1 with reference, backed by safetensors crate ─
//
// ref: `safetensors_draft.cpp:49-57`

/// Per-tensor entry with owned data-slice — conceptually the same as
/// the reference's [`dtype, shape, data_offsets`] tuple, but carrying
/// the raw byte slice directly so we don't have to recompute blob
/// pointers downstream.
#[derive(Clone, Debug)]
struct StEntry<'a> {
    dtype: String,
    shape: Vec<i64>,
    data: &'a [u8],
}

type StMap<'a> = HashMap<String, StEntry<'a>>;

/// Parse the safetensors file. The `safetensors` crate handles the
/// JSON header lifting AND gives back per-tensor byte slices — the
/// reference's hand-rolled parser + separate offset tracking collapse
/// into one call.
///
/// ref: replaces lines 63-151 (`parse_st_header`) + offset logic at
///      lines 264-287 with one crate call.
fn parse_file(bytes: &[u8]) -> Result<StMap<'_>, String> {
    let st = safetensors::SafeTensors::deserialize(bytes)
        .map_err(|e| format!("safetensors: JSON header parse failed: {e}"))?;
    let mut out: StMap<'_> = HashMap::new();
    for (name, view) in st.tensors() {
        let dtype = format!("{:?}", view.dtype()); // "BF16" / "F16" / "F32" etc.
        let shape: Vec<i64> = view.shape().iter().map(|&x| x as i64).collect();
        out.insert(
            name.to_string(),
            StEntry {
                dtype,
                shape,
                data: view.data(),
            },
        );
    }
    Ok(out)
}

/// Map safetensors dtype string to ggml type.
///
/// ref: `safetensors_draft.cpp:154-159`
fn st_dtype_to_ggml(dt: &str) -> ggml_type {
    match dt {
        "BF16" => ggml_type::GGML_TYPE_BF16,
        "F16" => ggml_type::GGML_TYPE_F16,
        "F32" => ggml_type::GGML_TYPE_F32,
        _ => ggml_type::GGML_TYPE_COUNT,
    }
}

// ─── Mmap helper (shared with the target loader above) ──────────────
//
// The reference has an identical helper in `safetensors_draft.cpp:161-191`;
// we reuse the target loader's `Mmap` above since the draft loader never
// transfers mmap ownership onwards (no `release()` call).

/// Convert an array of bf16 values to f32 into a destination buffer.
///
/// ref: `safetensors_draft.cpp:255-260`
fn bf16_to_f32_array(src: &[u16], dst: &mut [f32]) {
    for (i, &bf) in src.iter().enumerate() {
        let bits: u32 = (bf as u32) << 16;
        dst[i] = f32::from_bits(bits);
    }
}

// ─── alloc_tensor — 1:1 with reference ───────────────────────────
//
// ref: `safetensors_draft.cpp:200-252`

/// `gt_override`: if not `GGML_TYPE_COUNT`, use this as the ggml
/// storage type instead of the safetensors dtype. Used to store
/// small "norm" weights as F32 while the safetensors file has them
/// as BF16 — required because ggml's CUDA elementwise ops (`ggml_mul`
/// in particular) reject BF16 src1. The actual bf16→f32 conversion
/// happens later in the data-copy loop.
fn alloc_tensor(
    ctx: *mut ggml_context,
    st: &StMap<'_>,
    name: &str,
    expected_shape: &[i64],
    dtype_expected: &str,
    gt_override: ggml_type,
) -> *mut ggml_tensor {
    let Some(e) = st.get(name) else {
        set_last_error(format!("safetensors: missing tensor '{name}'"));
        return std::ptr::null_mut();
    };
    if e.dtype != dtype_expected {
        set_last_error(format!(
            "safetensors: '{name}' dtype={} expected {dtype_expected}",
            e.dtype
        ));
        return std::ptr::null_mut();
    }
    if e.shape.len() != expected_shape.len() {
        set_last_error(format!("safetensors: '{name}' ndim mismatch"));
        return std::ptr::null_mut();
    }
    for (k, (&got, &want)) in e.shape.iter().zip(expected_shape.iter()).enumerate() {
        if got != want {
            set_last_error(format!(
                "safetensors: '{name}' shape[{k}]={got} expected {want}"
            ));
            return std::ptr::null_mut();
        }
    }
    let gt = if gt_override == ggml_type::GGML_TYPE_COUNT {
        st_dtype_to_ggml(dtype_expected)
    } else {
        gt_override
    };
    if gt == ggml_type::GGML_TYPE_COUNT {
        set_last_error(format!(
            "safetensors: unsupported dtype {dtype_expected}"
        ));
        return std::ptr::null_mut();
    }

    // Shape convention: HF row-major [out, in] → ggml col-major [in, out].
    let t: *mut ggml_tensor = match expected_shape.len() {
        1 => unsafe { sys::ggml_new_tensor_1d(ctx, gt, expected_shape[0]) },
        2 => {
            // expected_shape is written as [out, in]; ggml wants ne[0]=in, ne[1]=out
            unsafe { sys::ggml_new_tensor_2d(ctx, gt, expected_shape[1], expected_shape[0]) }
        }
        _ => {
            set_last_error(format!(
                "safetensors: unexpected ndim > 2 for '{name}'"
            ));
            return std::ptr::null_mut();
        }
    };
    let cname = CString::new(name).unwrap();
    unsafe {
        sys::ggml_set_name(t, cname.as_ptr());
    }
    t
}

// ─── load_draft_safetensors ──────────────────────────────────────
//
// ref: `safetensors_draft.cpp:264-396`

/// Load draft weights. Returns `false` on failure and sets
/// [`crate::last_error`], mirroring the reference.
pub fn load_draft_safetensors(
    path: &Path,
    backend: ggml_backend_t,
    out: &mut DraftWeights,
) -> bool {
    match load_draft_safetensors_inner(path, backend, out) {
        Ok(()) => true,
        Err(msg) => {
            set_last_error(msg);
            false
        }
    }
}

fn load_draft_safetensors_inner(
    path: &Path,
    backend: ggml_backend_t,
    out: &mut DraftWeights,
) -> Result<(), String> {
    // ── 1. Open + mmap. ref: lines 267-271
    let mm = Mmap::open_ro(path)?;
    if mm.len < 8 {
        return Err("safetensors: file too small".into());
    }

    // ── 2. Parse header + tensor map via the safetensors crate.
    //       ref: lines 273-287 (JSON header) collapsed into one
    //       deserialize() call.
    let file_slice = unsafe { std::slice::from_raw_parts(mm.addr as *const u8, mm.len) };
    let st = parse_file(file_slice)?;

    // ── 3. Allocate ggml context big enough for 5 layers × 11 + 3 top.
    //       ref: lines 289-299
    let n_layers = DFLASH27B_DRAFT_LAYERS;
    let n_tensors = 3 + 11 * n_layers; // with some headroom below
    let ip = sys::ggml_init_params {
        mem_size: ((n_tensors + 16) as libc::size_t) * unsafe { sys::ggml_tensor_overhead() },
        mem_buffer: std::ptr::null_mut(),
        no_alloc: true,
    };
    out.ctx = unsafe { sys::ggml_init(ip) };
    if out.ctx.is_null() {
        return Err("ggml_init failed for draft ctx".into());
    }
    out.backend = backend;
    out.layers.clear();
    out.layers.resize_with(n_layers as usize, DraftLayer::default);

    // ref: lines 301-306
    let hidden: i64 = DFLASH27B_TARGET_HIDDEN as i64; // 5120
    let q_dim: i64 = (DFLASH27B_TARGET_N_HEADS * DFLASH27B_TARGET_HEAD_DIM) as i64; // 4096
    let kv_dim: i64 = (DFLASH27B_TARGET_N_KV_HEADS * DFLASH27B_TARGET_HEAD_DIM) as i64; // 1024
    let inter: i64 = DFLASH27B_TARGET_INTERMEDIATE as i64; // 17408
    let hd: i64 = DFLASH27B_TARGET_HEAD_DIM as i64; // 128
    let fc_in: i64 = (DFLASH27B_DRAFT_N_TARGET_LAYERS as i64) * hidden; // 25600

    // ── 4. Create named tensors in the context. ref: lines 308-339
    //
    // Norms (rms_norm weights) are loaded as F32 because ggml's CUDA
    // elementwise ops require F32/F16 operands. Projection weights
    // stay bf16.
    let norm_gt = ggml_type::GGML_TYPE_F32;
    let any_gt = ggml_type::GGML_TYPE_COUNT;

    out.fc = alloc_tensor(out.ctx, &st, "fc.weight", &[hidden, fc_in], "BF16", any_gt);
    out.hidden_norm = alloc_tensor(
        out.ctx,
        &st,
        "hidden_norm.weight",
        &[hidden],
        "BF16",
        norm_gt,
    );
    out.out_norm = alloc_tensor(
        out.ctx,
        &st,
        "norm.weight",
        &[hidden],
        "BF16",
        norm_gt,
    );
    if out.fc.is_null() || out.hidden_norm.is_null() || out.out_norm.is_null() {
        return Err(crate::last_error());
    }

    for il in 0..(n_layers as usize) {
        let p = format!("layers.{il}.");
        let l = &mut out.layers[il];
        l.attn_norm = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}input_layernorm.weight"),
            &[hidden],
            "BF16",
            norm_gt,
        );
        l.ffn_norm = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}post_attention_layernorm.weight"),
            &[hidden],
            "BF16",
            norm_gt,
        );
        l.wq = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}self_attn.q_proj.weight"),
            &[q_dim, hidden],
            "BF16",
            any_gt,
        );
        l.wk = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}self_attn.k_proj.weight"),
            &[kv_dim, hidden],
            "BF16",
            any_gt,
        );
        l.wv = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}self_attn.v_proj.weight"),
            &[kv_dim, hidden],
            "BF16",
            any_gt,
        );
        l.wo = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}self_attn.o_proj.weight"),
            &[hidden, q_dim],
            "BF16",
            any_gt,
        );
        l.q_norm = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}self_attn.q_norm.weight"),
            &[hd],
            "BF16",
            norm_gt,
        );
        l.k_norm = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}self_attn.k_norm.weight"),
            &[hd],
            "BF16",
            norm_gt,
        );
        l.w_gate = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}mlp.gate_proj.weight"),
            &[inter, hidden],
            "BF16",
            any_gt,
        );
        l.w_up = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}mlp.up_proj.weight"),
            &[inter, hidden],
            "BF16",
            any_gt,
        );
        l.w_down = alloc_tensor(
            out.ctx,
            &st,
            &format!("{p}mlp.down_proj.weight"),
            &[hidden, inter],
            "BF16",
            any_gt,
        );
        if l.attn_norm.is_null()
            || l.ffn_norm.is_null()
            || l.wq.is_null()
            || l.wk.is_null()
            || l.wv.is_null()
            || l.wo.is_null()
            || l.q_norm.is_null()
            || l.k_norm.is_null()
            || l.w_gate.is_null()
            || l.w_up.is_null()
            || l.w_down.is_null()
        {
            return Err(crate::last_error());
        }
    }

    // ── 5. Allocate backend buffer, copy bytes. ref: lines 341-393
    out.buf = unsafe { sys::ggml_backend_alloc_ctx_tensors(out.ctx, backend) };
    if out.buf.is_null() {
        return Err("ggml_backend_alloc_ctx_tensors failed (draft)".into());
    }

    // Walk the tensors in the context and upload their bytes. For
    // tensors whose ggml type differs from the safetensors dtype
    // (BF16-on-disk, F32-in-ggml for norms), convert on the fly via
    // a scratch float buffer.
    //
    // ref: lines 345-393
    let mut scratch_f32: Vec<f32> = Vec::new();
    let mut t = unsafe { sys::ggml_get_first_tensor(out.ctx) };
    while !t.is_null() {
        let name_ptr = unsafe { sys::ggml_get_name(t) };
        if name_ptr.is_null() {
            return Err("post-alloc: tensor with null name".into());
        }
        let name = unsafe { CStr::from_ptr(name_ptr) }.to_string_lossy();
        let Some(e) = st.get(name.as_ref()) else {
            return Err(format!(
                "post-alloc: tensor '{name}' vanished from header"
            ));
        };
        let src_nbytes = e.data.len();
        let dst_nbytes = unsafe { sys::ggml_nbytes(t) };
        let ttype = unsafe { (*t).type_ };
        let same_dtype = ttype == st_dtype_to_ggml(&e.dtype);

        if same_dtype {
            if src_nbytes != dst_nbytes {
                return Err(format!(
                    "byte count mismatch for '{name}': blob={src_nbytes} ggml={dst_nbytes}"
                ));
            }
            unsafe {
                sys::ggml_backend_tensor_set(
                    t,
                    e.data.as_ptr() as *const c_void,
                    0,
                    dst_nbytes,
                );
            }
        } else if e.dtype == "BF16" && ttype == ggml_type::GGML_TYPE_F32 {
            let n = unsafe { sys::ggml_nelements(t) } as usize;
            if src_nbytes != n * std::mem::size_of::<u16>()
                || dst_nbytes != n * std::mem::size_of::<f32>()
            {
                return Err(format!("BF16->F32 size mismatch for '{name}'"));
            }
            scratch_f32.resize(n, 0.0);
            // Safety: `e.data` came from the safetensors crate which
            // already validated alignment. Interpret the bf16 half-
            // words as `u16`.
            let src_u16 =
                unsafe { std::slice::from_raw_parts(e.data.as_ptr() as *const u16, n) };
            bf16_to_f32_array(src_u16, scratch_f32.as_mut_slice());
            unsafe {
                sys::ggml_backend_tensor_set(
                    t,
                    scratch_f32.as_ptr() as *const c_void,
                    0,
                    dst_nbytes,
                );
            }
        } else {
            let tname = unsafe {
                CStr::from_ptr(sys::ggml_type_name(ttype))
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(format!(
                "unsupported dtype conversion for '{name}': {} -> ggml type {tname}",
                e.dtype
            ));
        }

        t = unsafe { sys::ggml_get_next_tensor(out.ctx, t) };
    }

    Ok(())
}

// ─── free_draft_weights ──────────────────────────────────────────
//
// ref: `safetensors_draft.cpp:398-405`

/// Frees everything `load_draft_safetensors` allocated. Idempotent.
pub fn free_draft_weights(w: &mut DraftWeights) {
    unsafe {
        if !w.buf.is_null() {
            sys::ggml_backend_buffer_free(w.buf);
            w.buf = std::ptr::null_mut();
        }
        if !w.ctx.is_null() {
            sys::ggml_free(w.ctx);
            w.ctx = std::ptr::null_mut();
        }
    }
    w.layers.clear();
    w.fc = std::ptr::null_mut();
    w.hidden_norm = std::ptr::null_mut();
    w.out_norm = std::ptr::null_mut();
}
