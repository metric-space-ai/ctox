//! Private snapshot of the actual quiescent Core/provider state.
//! This object has no wire constructor and grants no target execution.
use ctox_protocol::ThreadId;
use serde::Serialize;
use std::io::{self, Write};

pub struct NativeSessionState {
    session_id: ThreadId,
    model: String,
    provider_id: String,
    bytes: Vec<u8>,
}

impl NativeSessionState {
    pub(crate) fn from_core(
        session_id: ThreadId,
        model: String,
        provider_id: String,
        payload: &impl Serialize,
    ) -> io::Result<Self> {
        struct Bounded(Vec<u8>);
        impl Write for Bounded {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if bytes.len() > (64 * 1024 * 1024usize).saturating_sub(self.0.len()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "native state exceeds capture budget",
                    ));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut output = Bounded(Vec::new());
        serde_json::to_writer(&mut output, payload).map_err(io::Error::other)?;
        Ok(Self {
            session_id,
            model,
            provider_id,
            bytes: output.0,
        })
    }
    pub fn session_id(&self) -> ThreadId {
        self.session_id
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }
    /// Protected source input only: never emit these bytes in receipts/logs.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
