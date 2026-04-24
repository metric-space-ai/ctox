//! Embedded PTX blobs compiled from `vendor/ggml-cuda/*.cu` by
//! `build.rs`.
//!
//! `build.rs` writes one `<stem>.ptx` per listed op into `$OUT_DIR`,
//! and this module pulls each into the final binary via
//! `include_str!`. Runtime code loads the blob with
//! `cuModuleLoadData` once per process (see [`load_module`]) and
//! looks up each mangled kernel name with `cuModuleGetFunction`.
//!
//! Keep the list in sync with `build.rs::CUDA_PORT_PTX_MODULES`.

use std::ffi::CString;

use super::driver::{
    cuGetErrorString, cuModuleGetFunction, cuModuleLoadData, CUDA_SUCCESS, CUfunction, CUmodule,
    CUresult,
};

/// `vendor/ggml-cuda/norm.cu` compiled to PTX at build time.
pub const NORM_PTX: &str = include_str!(concat!(env!("OUT_DIR"), "/norm.ptx"));

/// Populated lookup for diagnostics.
pub fn all_embedded_ptx() -> &'static [(&'static str, &'static str)] {
    &[("norm", NORM_PTX)]
}

/// Load a PTX blob into the current CUDA context.
///
/// The blob must be NUL-terminated when passed to
/// `cuModuleLoadData`. `include_str!` produces a `&str` without a
/// trailing NUL, so we wrap it with `CString::new` at load time.
pub fn load_module(ptx: &str) -> Result<CUmodule, String> {
    let cptx = CString::new(ptx.as_bytes())
        .map_err(|e| format!("PTX contains interior NUL: {e}"))?;
    let mut module = CUmodule(std::ptr::null_mut());
    let rc = unsafe { cuModuleLoadData(&mut module, cptx.as_ptr() as *const _) };
    check(rc, "cuModuleLoadData")?;
    if module.0.is_null() {
        return Err("cuModuleLoadData succeeded but returned a null module".into());
    }
    Ok(module)
}

/// Look up a kernel function by its mangled name.
/// Pass a NUL-terminated byte slice (constants like
/// [`super::ops::norm::MANGLED_RMS_NORM_F32_B256`] include the NUL).
pub fn get_function(module: CUmodule, mangled_name_with_nul: &[u8]) -> Result<CUfunction, String> {
    debug_assert!(
        mangled_name_with_nul.ends_with(b"\0"),
        "mangled name constants must be NUL-terminated"
    );
    let mut func = CUfunction(std::ptr::null_mut());
    let rc = unsafe {
        cuModuleGetFunction(
            &mut func,
            module,
            mangled_name_with_nul.as_ptr() as *const _,
        )
    };
    let name = std::str::from_utf8(&mangled_name_with_nul[..mangled_name_with_nul.len() - 1])
        .unwrap_or("<invalid utf8>");
    check(rc, &format!("cuModuleGetFunction({name})"))?;
    if func.0.is_null() {
        return Err(format!(
            "cuModuleGetFunction({name}) succeeded but returned null"
        ));
    }
    Ok(func)
}

fn check(rc: CUresult, what: &str) -> Result<(), String> {
    if rc == CUDA_SUCCESS {
        Ok(())
    } else {
        let mut raw: *const std::os::raw::c_char = std::ptr::null();
        let sub = unsafe { cuGetErrorString(rc, &mut raw) };
        let msg = if sub == CUDA_SUCCESS && !raw.is_null() {
            unsafe { std::ffi::CStr::from_ptr(raw) }
                .to_string_lossy()
                .into_owned()
        } else {
            format!("CUresult({rc})")
        };
        Err(format!("{what}: {msg}"))
    }
}
