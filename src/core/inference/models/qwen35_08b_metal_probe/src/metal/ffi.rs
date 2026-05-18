//! Minimal Metal FFI wrapper for research benchmarks.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSString, NSUInteger};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLComputeCommandEncoder, MTLComputePipelineState, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLLibrary, MTLResourceOptions, MTLSize,
};

#[cfg(ctox_qwen35_08b_has_metallib)]
const METALLIB_BLOB: &[u8] = include_bytes!(env!("CTOX_QWEN35_08B_METALLIB"));

#[cfg(not(ctox_qwen35_08b_has_metallib))]
const METALLIB_BLOB: &[u8] = &[];

pub struct Device {
    mtl: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    library: Retained<ProtocolObject<dyn MTLLibrary>>,
    pipelines: Mutex<HashMap<String, Retained<ProtocolObject<dyn MTLComputePipelineState>>>>,
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl Device {
    pub fn default_system() -> Result<Self, String> {
        if METALLIB_BLOB.is_empty() {
            return Err("metallib blob is empty; build.rs did not produce shaders".to_string());
        }

        let raw: *mut ProtocolObject<dyn MTLDevice> = unsafe { MTLCreateSystemDefaultDevice() };
        if raw.is_null() {
            return Err("MTLCreateSystemDefaultDevice returned null".to_string());
        }
        let mtl: Retained<ProtocolObject<dyn MTLDevice>> = unsafe {
            Retained::from_raw(raw).ok_or_else(|| "failed to retain MTLDevice".to_string())?
        };
        let queue = mtl
            .newCommandQueue()
            .ok_or_else(|| "failed to create Metal command queue".to_string())?;
        let library = load_library_from_blob(&mtl, METALLIB_BLOB)?;

        Ok(Self {
            mtl,
            queue,
            library,
            pipelines: Mutex::new(HashMap::new()),
        })
    }

    pub fn pipeline(
        &self,
        kernel_name: &str,
    ) -> Result<Retained<ProtocolObject<dyn MTLComputePipelineState>>, String> {
        {
            let guard = self
                .pipelines
                .lock()
                .map_err(|_| "pipeline cache poisoned".to_string())?;
            if let Some(pso) = guard.get(kernel_name) {
                return Ok(pso.clone());
            }
        }

        let ns_name = NSString::from_str(kernel_name);
        let func = self
            .library
            .newFunctionWithName(&ns_name)
            .ok_or_else(|| format!("Metal function `{kernel_name}` not found"))?;
        let pso = self
            .mtl
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| format!("pipeline `{kernel_name}` failed: {err:?}"))?;
        self.pipelines
            .lock()
            .map_err(|_| "pipeline cache poisoned".to_string())?
            .insert(kernel_name.to_string(), pso.clone());
        Ok(pso)
    }

    pub fn new_buffer(&self, byte_len: usize) -> Result<Buffer, String> {
        let opts = MTLResourceOptions::MTLResourceStorageModeShared;
        let inner = self
            .mtl
            .newBufferWithLength_options(byte_len as NSUInteger, opts)
            .ok_or_else(|| format!("failed to allocate {byte_len} byte Metal buffer"))?;
        Ok(Buffer { inner })
    }

    pub fn new_private_buffer(&self, byte_len: usize) -> Result<Buffer, String> {
        let opts = MTLResourceOptions::MTLResourceStorageModePrivate;
        let inner = self
            .mtl
            .newBufferWithLength_options(byte_len as NSUInteger, opts)
            .ok_or_else(|| format!("failed to allocate {byte_len} byte private Metal buffer"))?;
        Ok(Buffer { inner })
    }

    pub fn new_private_buffer_with_data<T: Copy>(&self, data: &[T]) -> Result<Buffer, String> {
        let byte_len = std::mem::size_of_val(data);
        let staging = self.new_buffer(byte_len)?;
        unsafe {
            staging.write(0, data);
        }
        let private = self.new_private_buffer(byte_len)?;
        let cmd = self.command_buffer()?;
        let blit = cmd.blit()?;
        unsafe {
            blit.copy_from_buffer(&staging, &private, byte_len);
        }
        blit.end();
        cmd.commit_and_wait()?;
        Ok(private)
    }

    pub fn command_buffer(&self) -> Result<CommandBuffer, String> {
        let inner = self
            .queue
            .commandBuffer()
            .ok_or_else(|| "failed to create Metal command buffer".to_string())?;
        Ok(CommandBuffer { inner })
    }

    pub fn raw_device_ptr(&self) -> *mut c_void {
        self.mtl.as_ref() as *const ProtocolObject<dyn MTLDevice> as *mut c_void
    }
}

fn load_library_from_blob(
    mtl: &ProtocolObject<dyn MTLDevice>,
    blob: &[u8],
) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>, String> {
    let tmp = std::env::temp_dir().join(format!(
        "ctox_qwen35_08b_metal_probe_{}.metallib",
        std::process::id()
    ));
    std::fs::write(&tmp, blob)
        .map_err(|err| format!("failed to write temp metallib {}: {err}", tmp.display()))?;
    let url = unsafe {
        objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(
            tmp.to_string_lossy().as_ref(),
        ))
    };
    let lib = unsafe { mtl.newLibraryWithURL_error(&url) }
        .map_err(|err| format!("newLibraryWithURL_error failed: {err:?}"))?;
    let _ = std::fs::remove_file(&tmp);
    Ok(lib)
}

#[derive(Clone)]
pub struct Buffer {
    inner: Retained<ProtocolObject<dyn MTLBuffer>>,
}

impl Buffer {
    pub fn as_ptr(&self) -> *mut c_void {
        self.inner.contents().as_ptr()
    }

    pub unsafe fn write<T: Copy>(&self, byte_offset: usize, src: &[T]) {
        let dst = (self.as_ptr() as *mut u8).add(byte_offset);
        std::ptr::copy_nonoverlapping(src.as_ptr() as *const u8, dst, std::mem::size_of_val(src));
    }

    pub unsafe fn read<T: Copy>(&self, byte_offset: usize, dst: &mut [T]) {
        let src = (self.as_ptr() as *const u8).add(byte_offset);
        std::ptr::copy_nonoverlapping(src, dst.as_mut_ptr() as *mut u8, std::mem::size_of_val(dst));
    }

    fn raw(&self) -> &ProtocolObject<dyn MTLBuffer> {
        &self.inner
    }

    pub fn raw_buffer_ptr(&self) -> *mut c_void {
        self.inner.as_ref() as *const ProtocolObject<dyn MTLBuffer> as *mut c_void
    }
}

pub struct CommandBuffer {
    inner: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
}

impl CommandBuffer {
    pub fn compute(&self) -> Result<ComputeEncoder, String> {
        let inner = self
            .inner
            .computeCommandEncoder()
            .ok_or_else(|| "failed to create compute encoder".to_string())?;
        Ok(ComputeEncoder { inner })
    }

    pub fn blit(&self) -> Result<BlitEncoder, String> {
        let inner = self
            .inner
            .blitCommandEncoder()
            .ok_or_else(|| "failed to create blit encoder".to_string())?;
        Ok(BlitEncoder { inner })
    }

    pub fn commit_and_wait(self) -> Result<(), String> {
        self.inner.commit();
        unsafe { self.inner.waitUntilCompleted() };
        if let Some(err) = unsafe { self.inner.error() } {
            let desc = err.localizedDescription();
            let s: &NSString = desc.as_ref();
            return Err(s.to_string());
        }
        Ok(())
    }

    pub fn commit(self) {
        self.inner.commit();
    }

    pub fn raw_command_buffer_ptr(&self) -> *mut c_void {
        self.inner.as_ref() as *const ProtocolObject<dyn MTLCommandBuffer> as *mut c_void
    }
}

pub struct BlitEncoder {
    inner: Retained<ProtocolObject<dyn MTLBlitCommandEncoder>>,
}

impl BlitEncoder {
    pub unsafe fn copy_from_buffer(&self, src: &Buffer, dst: &Buffer, byte_len: usize) {
        self.inner
            .copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
                src.raw(),
                0,
                dst.raw(),
                0,
                byte_len as NSUInteger,
            );
    }

    pub fn end(self) {
        self.inner.endEncoding();
    }
}

pub struct ComputeEncoder {
    inner: Retained<ProtocolObject<dyn MTLComputeCommandEncoder>>,
}

impl ComputeEncoder {
    pub fn set_pipeline(&self, pso: &ProtocolObject<dyn MTLComputePipelineState>) {
        self.inner.setComputePipelineState(pso);
    }

    pub fn set_buffer(&self, index: usize, buf: &Buffer, offset: usize) {
        unsafe {
            self.inner.setBuffer_offset_atIndex(
                Some(buf.raw()),
                offset as NSUInteger,
                index as NSUInteger,
            );
        }
    }

    pub fn set_bytes<T>(&self, index: usize, value: &T) {
        unsafe {
            self.inner.setBytes_length_atIndex(
                std::ptr::NonNull::new_unchecked(value as *const T as *mut c_void),
                std::mem::size_of::<T>() as NSUInteger,
                index as NSUInteger,
            );
        }
    }

    pub fn dispatch_threads(&self, threads: usize, threads_per_threadgroup: usize) {
        let grid = MTLSize {
            width: threads as NSUInteger,
            height: 1,
            depth: 1,
        };
        let tg = MTLSize {
            width: threads_per_threadgroup as NSUInteger,
            height: 1,
            depth: 1,
        };
        self.inner.dispatchThreads_threadsPerThreadgroup(grid, tg);
    }

    pub fn dispatch_threadgroups(
        &self,
        threadgroups: (usize, usize, usize),
        threads_per_threadgroup: (usize, usize, usize),
    ) {
        let grid = MTLSize {
            width: threadgroups.0 as NSUInteger,
            height: threadgroups.1 as NSUInteger,
            depth: threadgroups.2 as NSUInteger,
        };
        let tg = MTLSize {
            width: threads_per_threadgroup.0 as NSUInteger,
            height: threads_per_threadgroup.1 as NSUInteger,
            depth: threads_per_threadgroup.2 as NSUInteger,
        };
        self.inner
            .dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    }

    pub fn end(self) {
        self.inner.endEncoding();
    }
}
