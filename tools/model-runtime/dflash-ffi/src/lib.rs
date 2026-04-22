//! Safe Rust FFI wrapper around the DFlash reference library
//! (`libdflash_run_lib.so`) exposing its persistent-state C API:
//!
//!   * `DflashCtx * dflash_ctx_init(const DflashCtxOpts *)`
//!   * `int          dflash_ctx_generate(ctx, prompt, n_new, out_buf, cap, len_out, result)`
//!   * `void         dflash_ctx_free(ctx)`
//!
//! The library is loaded via `libloading::Library` (dlopen) at runtime so
//! callers can point at any reference build on the host. Symbol names
//! match the C++ extraction in `dflash-ref/dflash/test/test_dflash_lib.cpp`.
//!
//! ## Lifetime rules
//!
//! * `DflashRuntime::new` dlopen's the `.so`, calls `dflash_ctx_init`, and
//!   stashes the symbol bindings for reuse.
//! * `DflashRuntime::generate` is `&mut self`: the library mutates its
//!   internal target/draft/KV state and is not thread-safe. The backing
//!   ggml backend uses process-global CUDA state so only one
//!   `DflashRuntime` should exist per process.
//! * `DflashRuntime::Drop` calls `dflash_ctx_free` and lets the `Library`
//!   destructor unmap the `.so`.
//!
//! ## Stability
//!
//! This crate is the **intermediate** production path — the engine will
//! use it until the bare-metal `ctox-qwen35-27b` port catches up. The
//! C API surface is owned by us (we refactored the reference to expose
//! it); it will be deleted together with the FFI binding once the
//! native port reaches tok/s parity.

use std::ffi::CString;
use std::os::raw::{c_char, c_float, c_int};
use std::path::{Path, PathBuf};
use std::ptr;

use libloading::{Library, Symbol};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DflashError {
    #[error("dlopen failed: {path}: {source}")]
    Dlopen {
        path: PathBuf,
        #[source]
        source: libloading::Error,
    },
    #[error("dlsym failed: symbol {symbol}: {source}")]
    Dlsym {
        symbol: &'static str,
        #[source]
        source: libloading::Error,
    },
    #[error("dflash_ctx_init returned NULL (check stderr for reason)")]
    InitFailed,
    #[error("dflash_ctx_generate returned non-zero exit code {0}")]
    GenerateFailed(i32),
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    PathNotUtf8(std::str::Utf8Error),
}

/// Opaque handle — lives inside the reference library. Field-for-field
/// mirror of the C++ `struct DflashCtx`, but we never access the fields
/// from Rust so the struct is just a forward declaration.
#[repr(C)]
struct DflashCtxRaw {
    _private: [u8; 0],
}

/// Mirror of the C++ `struct DflashCtxOpts`. Must match field-for-field.
#[repr(C)]
struct DflashCtxOptsRaw {
    target_gguf_path: *const c_char,
    draft_safetensors_path: *const c_char,
    max_ctx: c_int,
    ddtree_mode: c_int,
    ddtree_budget: c_int,
    ddtree_temp: c_float,
    ddtree_chain_seed: c_int,
    fast_rollback: c_int,
    seq_verify: c_int,
    cuda_device: c_int,
    tbq_kv: c_int,
}

/// Mirror of the C++ `struct DflashGenResult`. Filled by the library.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct DflashGenResultRaw {
    pub n_generated: c_int,
    pub n_draft_steps: c_int,
    pub n_accepted: c_int,
    pub n_proposed: c_int,
    pub wall_s: f64,
    pub decode_tok_s: f64,
    pub last_tok: i32,
}

// ─── extern "C" function signatures ─────────────────────────────────

type FnInit = unsafe extern "C" fn(*const DflashCtxOptsRaw) -> *mut DflashCtxRaw;
type FnGenerate = unsafe extern "C" fn(
    ctx: *mut DflashCtxRaw,
    prompt_ids: *const i32,
    prompt_len: c_int,
    n_new: c_int,
    out_ids_buf: *mut i32,
    out_buf_cap: c_int,
    out_ids_len_out: *mut c_int,
    result: *mut DflashGenResultRaw,
) -> c_int;
type FnFree = unsafe extern "C" fn(*mut DflashCtxRaw);

/// Rust-side options. Mirrors `DflashCtxOptsRaw` but with owned paths and
/// `Option<T>` so callers can leave defaults to the library.
#[derive(Debug, Clone)]
pub struct DflashOpts {
    pub target_gguf: PathBuf,
    pub draft_safetensors: PathBuf,
    pub max_ctx: u32,
    pub ddtree_mode: bool,
    pub ddtree_budget: u32,
    pub ddtree_temp: f32,
    pub ddtree_chain_seed: bool,
    /// If `ddtree_mode` is true, this is forced to true by the library.
    pub fast_rollback: bool,
    pub seq_verify: bool,
    pub cuda_device: u32,
    /// TurboQuant KV-cache alignment. Sets `g_kq_stride_pad = 256`. Only
    /// meaningful when the reference was built against TBQ FA kernels.
    pub tbq_kv: bool,
}

impl Default for DflashOpts {
    fn default() -> Self {
        Self {
            target_gguf: PathBuf::new(),
            draft_safetensors: PathBuf::new(),
            max_ctx: 4096,
            ddtree_mode: false,
            ddtree_budget: 22,
            ddtree_temp: 1.0,
            ddtree_chain_seed: true,
            fast_rollback: false,
            seq_verify: false,
            cuda_device: 0,
            tbq_kv: false,
        }
    }
}

/// Per-call statistics. Pretty-printable mirror of `DflashGenResultRaw`.
#[derive(Debug, Clone, Copy)]
pub struct DflashGenStats {
    pub n_generated: i32,
    pub n_draft_steps: i32,
    pub n_accepted: i32,
    pub n_proposed: i32,
    pub wall_s: f64,
    pub decode_tok_s: f64,
    pub last_tok: i32,
}

impl From<DflashGenResultRaw> for DflashGenStats {
    fn from(r: DflashGenResultRaw) -> Self {
        Self {
            n_generated: r.n_generated,
            n_draft_steps: r.n_draft_steps,
            n_accepted: r.n_accepted,
            n_proposed: r.n_proposed,
            wall_s: r.wall_s,
            decode_tok_s: r.decode_tok_s,
            last_tok: r.last_tok,
        }
    }
}

/// Persistent DFlash runtime. Holds the `.so`, cached symbols, and the
/// opaque `DflashCtx*` that survives across `generate` calls.
///
/// **Not `Send` or `Sync`**: the underlying ggml backend uses
/// process-global CUDA state, and the library itself is not internally
/// synchronized. One `DflashRuntime` per process, driven from one
/// thread.
pub struct DflashRuntime {
    // Field ordering matters: ctx must drop BEFORE lib, otherwise
    // dflash_ctx_free runs after the .so is unmapped and segfaults.
    ctx: *mut DflashCtxRaw,
    // Function pointers extracted from the loaded library. These are
    // valid as long as `lib` is alive.
    generate_fn: FnGenerate,
    free_fn: FnFree,
    _lib: Library,
}

impl DflashRuntime {
    /// Load `libdflash_run_lib.so` from `lib_path` and initialize a
    /// context with `opts`. The target GGUF + draft safetensors paths
    /// in `opts` must point at files the library can read.
    pub fn new(lib_path: &Path, opts: &DflashOpts) -> Result<Self, DflashError> {
        // Validate required paths.
        if opts.target_gguf.as_os_str().is_empty() {
            return Err(DflashError::Invalid("DflashOpts.target_gguf is empty"));
        }
        if opts.draft_safetensors.as_os_str().is_empty() {
            return Err(DflashError::Invalid("DflashOpts.draft_safetensors is empty"));
        }

        // dlopen the library.
        let lib = unsafe { Library::new(lib_path) }.map_err(|e| DflashError::Dlopen {
            path: lib_path.to_path_buf(),
            source: e,
        })?;

        // Resolve all symbols up front so we fail fast if the .so is
        // from a pre-refactor build.
        let init_fn: Symbol<FnInit> =
            unsafe { lib.get(b"dflash_ctx_init\0") }.map_err(|e| DflashError::Dlsym {
                symbol: "dflash_ctx_init",
                source: e,
            })?;
        let generate_fn: Symbol<FnGenerate> =
            unsafe { lib.get(b"dflash_ctx_generate\0") }.map_err(|e| DflashError::Dlsym {
                symbol: "dflash_ctx_generate",
                source: e,
            })?;
        let free_fn: Symbol<FnFree> =
            unsafe { lib.get(b"dflash_ctx_free\0") }.map_err(|e| DflashError::Dlsym {
                symbol: "dflash_ctx_free",
                source: e,
            })?;

        // Copy the raw function pointers out of the Symbols so we can
        // store them in the runtime struct without a reference to `lib`.
        // This is safe because `lib` lives inside the struct and only
        // drops after `ctx` + the function pointers are gone.
        let init_raw: FnInit = unsafe { *init_fn.into_raw() };
        let generate_raw: FnGenerate = unsafe { *generate_fn.into_raw() };
        let free_raw: FnFree = unsafe { *free_fn.into_raw() };

        // Build CString args with lifetime extending through the init
        // call. The library copies the strings internally (std::string
        // in load_target_gguf/load_draft_safetensors), so they're safe
        // to drop after init returns.
        let target_c = CString::new(
            opts.target_gguf
                .to_str()
                .ok_or(DflashError::Invalid("target_gguf path is not valid UTF-8"))?,
        )
        .map_err(|_| DflashError::Invalid("target_gguf contains interior NUL"))?;
        let draft_c = CString::new(
            opts.draft_safetensors
                .to_str()
                .ok_or(DflashError::Invalid("draft_safetensors path is not valid UTF-8"))?,
        )
        .map_err(|_| DflashError::Invalid("draft_safetensors contains interior NUL"))?;

        let raw_opts = DflashCtxOptsRaw {
            target_gguf_path: target_c.as_ptr(),
            draft_safetensors_path: draft_c.as_ptr(),
            max_ctx: opts.max_ctx as c_int,
            ddtree_mode: if opts.ddtree_mode { 1 } else { 0 },
            ddtree_budget: opts.ddtree_budget as c_int,
            ddtree_temp: opts.ddtree_temp as c_float,
            ddtree_chain_seed: if opts.ddtree_chain_seed { 1 } else { 0 },
            fast_rollback: if opts.fast_rollback { 1 } else { 0 },
            seq_verify: if opts.seq_verify { 1 } else { 0 },
            cuda_device: opts.cuda_device as c_int,
            tbq_kv: if opts.tbq_kv { 1 } else { 0 },
        };

        let ctx = unsafe { (init_raw)(&raw_opts) };
        if ctx.is_null() {
            // CStrings are dropped here — fine, library is done with them.
            return Err(DflashError::InitFailed);
        }

        tracing::info!(
            target = %opts.target_gguf.display(),
            draft = %opts.draft_safetensors.display(),
            max_ctx = opts.max_ctx,
            ddtree = opts.ddtree_mode,
            "dflash runtime initialized"
        );

        Ok(DflashRuntime {
            ctx,
            generate_fn: generate_raw,
            free_fn: free_raw,
            _lib: lib,
        })
    }

    /// Generate up to `n_new` tokens continuing from `prompt_ids`.
    ///
    /// Returns the full token sequence the library wrote into its
    /// output buffer (prompt + decoded continuation) plus per-call
    /// statistics.
    ///
    /// Each call resets the underlying KV/SSM cache internally, so
    /// prompts do NOT share prefix state with previous calls. If you
    /// want prefix reuse, that needs to be implemented in the C layer
    /// first (next phase of the API work).
    pub fn generate(
        &mut self,
        prompt_ids: &[i32],
        n_new: usize,
    ) -> Result<(Vec<i32>, DflashGenStats), DflashError> {
        if prompt_ids.is_empty() {
            return Err(DflashError::Invalid("prompt_ids is empty"));
        }

        let prompt_len = prompt_ids.len();
        let out_cap = prompt_len + n_new;
        let mut out_buf: Vec<i32> = vec![0; out_cap];
        let mut out_len: c_int = 0;
        let mut result_raw = DflashGenResultRaw::default();

        let rc = unsafe {
            (self.generate_fn)(
                self.ctx,
                prompt_ids.as_ptr(),
                prompt_len as c_int,
                n_new as c_int,
                out_buf.as_mut_ptr(),
                out_cap as c_int,
                &mut out_len as *mut c_int,
                &mut result_raw as *mut DflashGenResultRaw,
            )
        };
        if rc != 0 {
            return Err(DflashError::GenerateFailed(rc));
        }

        out_buf.truncate(out_len.max(0) as usize);
        Ok((out_buf, result_raw.into()))
    }
}

impl Drop for DflashRuntime {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            unsafe { (self.free_fn)(self.ctx) };
            self.ctx = ptr::null_mut();
        }
    }
}

// DflashRuntime contains a `*mut DflashCtxRaw`, which auto-derives
// !Send + !Sync (raw pointers are neither). That matches our
// intent: the library holds process-global CUDA state and isn't
// internally synchronized — one runtime per process, one thread.
