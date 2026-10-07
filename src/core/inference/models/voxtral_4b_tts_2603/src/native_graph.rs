//! Serialized host boundary for the pinned C graph. Upstream tokenizer and CUDA
//! state are process-global, so every load/generate/free runs under one mutex.
use crate::{Error, Result};
use std::{
    ffi::CString,
    path::Path,
    sync::{Arc, Mutex, Weak},
};
static GRAPH: Mutex<Option<Weak<NativeSession>>> = Mutex::new(None);
#[derive(Debug)]
pub struct NativeSession {
    root: std::path::PathBuf,
    ctx: usize,
}
#[cfg(voxtral_native)]
unsafe extern "C" {
    fn ctox_voxtral_load(dir: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn ctox_voxtral_free(ctx: *mut std::ffi::c_void);
    fn ctox_voxtral_generate(
        ctx: *mut std::ffi::c_void,
        text: *const std::ffi::c_char,
        voice: *const std::ffi::c_char,
        samples: *mut *mut f32,
        count: *mut i32,
    ) -> i32;
    fn ctox_voxtral_samples_free(samples: *mut f32);
}
pub fn available(cuda: bool) -> bool {
    cfg!(voxtral_native) && (!cuda || cfg!(voxtral_cuda))
}
pub fn load(root: &Path) -> Result<Arc<NativeSession>> {
    let mut guard = GRAPH
        .lock()
        .map_err(|_| Error::Unsupported("native graph lock poisoned"))?;
    if let Some(active) = guard.as_ref().and_then(Weak::upgrade) {
        if active.root == root {
            return Ok(active);
        }
        drop(guard);
        drop(active);
        return Err(Error::Unsupported(
            "another native Voxtral model is active in this process",
        ));
    }
    #[cfg(not(voxtral_native))]
    {
        let _ = root;
        Err(Error::Unsupported(
            "native graph unavailable on this platform",
        ))
    }
    #[cfg(voxtral_native)]
    {
        let dir = CString::new(root.as_os_str().as_encoded_bytes())
            .map_err(|_| Error::InvalidFormat("model path contains NUL"))?;
        // SAFETY: path lives through the call, global upstream state is locked.
        let ctx = unsafe { ctox_voxtral_load(dir.as_ptr()) };
        if ctx.is_null() {
            return Err(Error::Unsupported("native Voxtral model load failed"));
        }
        let session = Arc::new(NativeSession {
            root: root.into(),
            ctx: ctx as usize,
        });
        *guard = Some(Arc::downgrade(&session));
        Ok(session)
    }
}
impl Drop for NativeSession {
    fn drop(&mut self) {
        #[cfg(voxtral_native)]
        if let Ok(_guard) = GRAPH.lock() {
            // SAFETY: session owns this context and the global lock serializes
            // generation/free. No strong reference remains when Drop runs.
            unsafe { ctox_voxtral_free(self.ctx as *mut std::ffi::c_void) };
        }
    }
}
impl NativeSession {
    pub fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>> {
        #[cfg(not(voxtral_native))]
        {
            let _ = (text, voice);
            Err(Error::Unsupported(
                "native graph unavailable on this platform",
            ))
        }
        #[cfg(voxtral_native)]
        {
            let _guard = GRAPH
                .lock()
                .map_err(|_| Error::Unsupported("native graph lock poisoned"))?;
            let text = CString::new(text)
                .map_err(|_| Error::InvalidFormat("speech input contains NUL"))?;
            let voice =
                CString::new(voice).map_err(|_| Error::InvalidFormat("voice contains NUL"))?;
            let mut samples = std::ptr::null_mut();
            let mut count = 0;
            // SAFETY: this live session owns ctx, strings and output locations
            // remain valid, and process-global native state is exclusively held.
            let status = unsafe {
                ctox_voxtral_generate(
                    self.ctx as *mut std::ffi::c_void,
                    text.as_ptr(),
                    voice.as_ptr(),
                    &mut samples,
                    &mut count,
                )
            };
            if status != 0 || samples.is_null() || !(1..=983040).contains(&count) {
                unsafe { ctox_voxtral_samples_free(samples) };
                return Err(Error::Unsupported("native Voxtral synthesis failed"));
            }
            let data = unsafe { std::slice::from_raw_parts(samples, count as usize) };
            let result = encode_wav(data);
            unsafe { ctox_voxtral_samples_free(samples) };
            result
        }
    }
}
fn encode_wav(samples: &[f32]) -> Result<Vec<u8>> {
    if samples.iter().any(|s| !s.is_finite()) {
        return Err(Error::InvalidFormat("non-finite generated audio"));
    }
    let size = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(size + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&24000u32.to_le_bytes());
    wav.extend_from_slice(&48000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&size.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    Ok(wav)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_wave_header_and_pcm_are_consistent() {
        let wav = encode_wav(&[0.0, 1.0, -1.0]).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(&wav[44..], &[0, 0, 255, 127, 1, 128]);
        assert!(encode_wav(&[f32::NAN]).is_err());
    }
}
