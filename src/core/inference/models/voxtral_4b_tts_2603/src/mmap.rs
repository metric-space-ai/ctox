//! Minimal read-only mmap without external crates.
//!
//! This uses platform FFI directly, keeping the Rust dependency tree empty.

use crate::{Error, Result};
use std::fs::File;

#[cfg(unix)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::os::fd::AsRawFd;
    use std::ptr::NonNull;

    const PROT_READ: i32 = 0x1;
    const MAP_PRIVATE: i32 = 0x02;

    extern "C" {
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: isize,
        ) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> i32;
    }

    pub struct Mmap {
        ptr: NonNull<u8>,
        len: usize,
    }

    unsafe impl Send for Mmap {}
    unsafe impl Sync for Mmap {}

    impl Mmap {
        pub fn map_readonly(file: &File) -> Result<Self> {
            let len = file.metadata()?.len() as usize;
            if len == 0 {
                return Err(Error::InvalidFormat("cannot mmap empty file"));
            }
            let ptr = unsafe {
                mmap(
                    std::ptr::null_mut(),
                    len,
                    PROT_READ,
                    MAP_PRIVATE,
                    file.as_raw_fd(),
                    0,
                )
            };
            if ptr as isize == -1 {
                return Err(Error::Io(std::io::Error::last_os_error()));
            }
            let ptr =
                NonNull::new(ptr.cast::<u8>()).ok_or(Error::InvalidFormat("mmap returned null"))?;
            Ok(Self { ptr, len })
        }

        #[inline]
        pub fn as_slice(&self) -> &[u8] {
            unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.len
        }
    }

    impl Drop for Mmap {
        fn drop(&mut self) {
            let _ = unsafe { munmap(self.ptr.as_ptr().cast::<c_void>(), self.len) };
        }
    }
}

#[cfg(not(unix))]
mod imp {
    use super::*;

    pub struct Mmap {
        buf: Vec<u8>,
    }

    impl Mmap {
        pub fn map_readonly(file: &File) -> Result<Self> {
            use std::io::Read;
            let mut f = file.try_clone()?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            if buf.is_empty() {
                return Err(Error::InvalidFormat("empty file"));
            }
            Ok(Self { buf })
        }

        #[inline]
        pub fn as_slice(&self) -> &[u8] {
            &self.buf
        }
        #[inline]
        pub fn len(&self) -> usize {
            self.buf.len()
        }
    }
}

pub use imp::Mmap;
